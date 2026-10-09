//! JSON-RPC 2.0 over any `Read`/`Write` pair, one message per line (the
//! child's stdout/stdin for the host; stdin/stdout for an extension).
//!
//! Requests go both ways on the one channel, as in LSP: a message with a
//! `method` is the other side's request (with an `id`) or notification
//! (without); a message with an `id` and no `method` answers one of ours.
//! Each side numbers its own requests, so the ids never collide.
//!
//! [`Connection::spawn`] starts a reader thread and a writer thread and
//! returns the [`Connection`] (send requests, notifications, responses)
//! plus a receiver of what the other side sends. The receiver disconnects
//! when the stream ends; every request still waiting then fails with
//! [`RpcError::closed`]. Sending never blocks on the other side: lines are
//! queued for the writer thread, so a stuck child can't stall the UI.
//! Malformed lines are skipped.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::protocol::{RpcError, codes};

/// The longest line read; longer ones are skipped (16 MiB).
pub const MAX_LINE: usize = 16 * 1024 * 1024;

/// Something the other side sent.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// Answer it with [`Connection::respond`].
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
}

type Waiter = Sender<Result<Value, RpcError>>;

struct Inner {
    out: Mutex<Option<Sender<String>>>,
    pending: Mutex<HashMap<u64, Waiter>>,
    next: AtomicU64,
    closed: AtomicBool,
    writer: Mutex<Option<std::thread::JoinHandle<()>>>,
}

/// One end of a JSON-RPC channel. Cheap to clone; every clone is the same
/// connection.
#[derive(Clone)]
pub struct Connection {
    inner: Arc<Inner>,
}

impl Connection {
    /// Start the reader and writer threads over `read`/`write`.
    pub fn spawn<R, W>(read: R, write: W) -> (Connection, Receiver<Incoming>)
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        let (out_tx, out_rx) = mpsc::channel::<String>();
        let (in_tx, in_rx) = mpsc::channel::<Incoming>();
        let conn = Connection {
            inner: Arc::new(Inner {
                out: Mutex::new(Some(out_tx)),
                pending: Mutex::new(HashMap::new()),
                next: AtomicU64::new(1),
                closed: AtomicBool::new(false),
                writer: Mutex::new(None),
            }),
        };
        let w = std::thread::Builder::new()
            .name("bxp-write".into())
            .spawn(move || write_loop(write, out_rx))
            .expect("spawn writer");
        *conn.inner.writer.lock().unwrap_or_else(|p| p.into_inner()) = Some(w);
        let reader = conn.clone();
        std::thread::Builder::new()
            .name("bxp-read".into())
            .spawn(move || reader.read_loop(read, in_tx))
            .expect("spawn reader");
        (conn, in_rx)
    }

    /// Send a request and wait up to `timeout` for its answer. A timeout
    /// (or the stream ending) is an error with code [`codes::TIMEOUT`].
    pub fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        if self.is_closed() {
            return Err(RpcError::closed());
        }
        let id = self.inner.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.pending().insert(id, tx);
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if !self.send(msg) {
            self.pending().remove(&id);
            return Err(RpcError::closed());
        }
        match rx.recv_timeout(timeout) {
            Ok(r) => r,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.pending().remove(&id);
                Err(RpcError::timeout())
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(RpcError::closed()),
        }
    }

    /// [`request`](Self::request) with typed params and result.
    pub fn call<P: Serialize, T: DeserializeOwned>(
        &self,
        method: &str,
        params: &P,
        timeout: Duration,
    ) -> Result<T, RpcError> {
        let p = serde_json::to_value(params)
            .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()))?;
        let v = self.request(method, p, timeout)?;
        serde_json::from_value(v).map_err(|e| {
            RpcError::new(
                codes::INTERNAL_ERROR,
                format!("unexpected answer to {method}: {e}"),
            )
        })
    }

    /// Send a notification (no answer). False when the channel is closed.
    pub fn notify<P: Serialize>(&self, method: &str, params: &P) -> bool {
        let Ok(p) = serde_json::to_value(params) else {
            return false;
        };
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": p}))
    }

    /// Answer the other side's request `id`.
    pub fn respond(&self, id: Value, result: Result<Value, RpcError>) -> bool {
        let msg = match result {
            Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
            Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
        };
        self.send(msg)
    }

    /// The stream ended (or [`close`](Self::close) was called).
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::SeqCst)
    }

    /// Stop sending: the writer finishes what's queued and drops the
    /// stream (the child sees EOF on stdin). Waiting requests fail.
    pub fn close(&self) {
        self.inner
            .out
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        self.inner.closed.store(true, Ordering::SeqCst);
        self.fail_pending();
    }

    /// [`close`](Self::close), then wait for the writer to write out what's
    /// queued (an extension's last answer before it exits).
    pub fn close_and_flush(&self) {
        self.close();
        let w = self
            .inner
            .writer
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        if let Some(w) = w {
            let _ = w.join();
        }
    }

    fn pending(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Waiter>> {
        self.inner.pending.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn fail_pending(&self) {
        for (_, w) in self.pending().drain() {
            let _ = w.send(Err(RpcError::closed()));
        }
    }

    fn send(&self, msg: Value) -> bool {
        let line = msg.to_string();
        match &*self.inner.out.lock().unwrap_or_else(|p| p.into_inner()) {
            Some(tx) => tx.send(line).is_ok(),
            None => false,
        }
    }

    fn read_loop<R: Read>(self, read: R, incoming: Sender<Incoming>) {
        let mut r = BufReader::new(read);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match (&mut r)
                .take(MAX_LINE as u64 + 1)
                .read_until(b'\n', &mut buf)
            {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if buf.len() > MAX_LINE {
                // Too long: skip to the end of this line.
                let mut sink = Vec::new();
                if r.read_until(b'\n', &mut sink).unwrap_or(0) == 0 {
                    break;
                }
                continue;
            }
            let Ok(v) = serde_json::from_slice::<Value>(&buf) else {
                continue;
            };
            self.dispatch(v, &incoming);
        }
        self.inner.closed.store(true, Ordering::SeqCst);
        self.fail_pending();
    }

    fn dispatch(&self, v: Value, incoming: &Sender<Incoming>) {
        let Value::Object(mut m) = v else {
            return;
        };
        let id = m.remove("id").filter(|i| !i.is_null());
        match m.remove("method") {
            Some(Value::String(method)) => {
                let params = m.remove("params").unwrap_or(Value::Null);
                let msg = match id {
                    Some(id) => Incoming::Request { id, method, params },
                    None => Incoming::Notification { method, params },
                };
                let _ = incoming.send(msg);
            }
            Some(_) => {
                if let Some(id) = id {
                    self.respond(
                        id,
                        Err(RpcError::new(
                            codes::INVALID_REQUEST,
                            "method must be a string",
                        )),
                    );
                }
            }
            None => {
                let Some(n) = id.as_ref().and_then(Value::as_u64) else {
                    return;
                };
                let Some(w) = self.pending().remove(&n) else {
                    return; // late answer to a timed-out request
                };
                let r = match m.remove("error") {
                    Some(e) => Err(serde_json::from_value(e).unwrap_or_else(|_| {
                        RpcError::new(codes::INTERNAL_ERROR, "malformed error")
                    })),
                    None => Ok(m.remove("result").unwrap_or(Value::Null)),
                };
                let _ = w.send(r);
            }
        }
    }
}

fn write_loop<W: Write>(mut write: W, rx: Receiver<String>) {
    while let Ok(line) = rx.recv() {
        if write.write_all(line.as_bytes()).is_err()
            || write.write_all(b"\n").is_err()
            || write.flush().is_err()
        {
            break;
        }
    }
}

/// Decode a request's params, mapping failure to `-32602`.
pub fn params<T: DeserializeOwned>(v: Value) -> Result<T, RpcError> {
    let v = if v.is_null() { json!({}) } else { v };
    serde_json::from_value(v).map_err(RpcError::invalid_params)
}

/// Encode a result.
pub fn to_value<T: Serialize>(t: &T) -> Result<Value, RpcError> {
    serde_json::to_value(t).map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::pipe;

    /// Two connected ends.
    fn pair() -> (
        (Connection, Receiver<Incoming>),
        (Connection, Receiver<Incoming>),
    ) {
        let (a_read, b_write) = pipe().unwrap();
        let (b_read, a_write) = pipe().unwrap();
        (
            Connection::spawn(a_read, a_write),
            Connection::spawn(b_read, b_write),
        )
    }

    /// Answer every request on `rx` with its params, after `delay`.
    fn echo(conn: Connection, rx: Receiver<Incoming>, delay: Duration) {
        std::thread::spawn(move || {
            while let Ok(m) = rx.recv() {
                if let Incoming::Request { id, method, params } = m {
                    std::thread::sleep(delay);
                    let r = if method == "fail" {
                        Err(RpcError::new(-1, "no"))
                    } else {
                        Ok(params)
                    };
                    conn.respond(id, r);
                }
            }
        });
    }

    const T: Duration = Duration::from_secs(5);

    #[test]
    fn requests_get_their_own_answers() {
        let ((a, _arx), (b, brx)) = pair();
        echo(b, brx, Duration::ZERO);
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let a = a.clone();
                std::thread::spawn(move || a.request("echo", json!({ "n": i }), T).unwrap())
            })
            .collect();
        for (i, t) in threads.into_iter().enumerate() {
            assert_eq!(t.join().unwrap(), json!({ "n": i }));
        }
        assert_eq!(a.request("fail", json!({}), T).unwrap_err().code, -1);
    }

    #[test]
    fn both_directions_share_the_channel() {
        let ((a, arx), (b, brx)) = pair();
        echo(a.clone(), arx, Duration::ZERO);
        echo(b.clone(), brx, Duration::ZERO);
        assert_eq!(a.request("x", json!(1), T).unwrap(), json!(1));
        assert_eq!(b.request("y", json!(2), T).unwrap(), json!(2));
    }

    #[test]
    fn a_slow_answer_times_out_and_a_late_one_is_dropped() {
        let ((a, _arx), (b, brx)) = pair();
        echo(b, brx, Duration::from_millis(300));
        let e = a
            .request("slow", json!({}), Duration::from_millis(50))
            .unwrap_err();
        assert_eq!(e.code, codes::TIMEOUT);
        // The late answer to the first request doesn't satisfy this one.
        assert_eq!(a.request("next", json!(7), T).unwrap(), json!(7));
    }

    #[test]
    fn notifications_and_malformed_lines() {
        let (a_read, mut feed) = pipe().unwrap();
        let (_unused, a_write) = pipe().unwrap();
        let (_a, arx) = Connection::spawn(a_read, a_write);
        feed.write_all(b"not json\n[1,2]\n{\"jsonrpc\":\"2.0\",\"method\":\"hi\",\"params\":{\"x\":1},\"extra\":true}\n").unwrap();
        feed.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"q\"}\r\n")
            .unwrap();
        assert_eq!(
            arx.recv_timeout(T).unwrap(),
            Incoming::Notification {
                method: "hi".into(),
                params: json!({"x": 1})
            }
        );
        assert_eq!(
            arx.recv_timeout(T).unwrap(),
            Incoming::Request {
                id: json!(9),
                method: "q".into(),
                params: Value::Null
            },
            "a CRLF line ending is fine"
        );
    }

    #[test]
    fn eof_fails_waiting_requests_and_disconnects() {
        let (a_read, feed) = pipe().unwrap();
        let (_keep, a_write) = pipe().unwrap();
        let (a, arx) = Connection::spawn(a_read, a_write);
        let waiter = {
            let a = a.clone();
            std::thread::spawn(move || a.request("never", json!({}), T))
        };
        std::thread::sleep(Duration::from_millis(50));
        drop(feed);
        let e = waiter.join().unwrap().unwrap_err();
        assert_eq!(e.code, codes::TIMEOUT);
        assert!(arx.recv_timeout(T).is_err(), "receiver disconnects at EOF");
        assert!(a.is_closed());
        assert!(a.request("after", json!({}), T).is_err());
    }
}
