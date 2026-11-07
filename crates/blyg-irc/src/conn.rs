//! The connection: one background thread per client, a blocking socket
//! (TLS through rustls, or plain TCP for the in-process test server), the
//! session over it, and reconnects with backoff. No async runtime.

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::message::Message;
use crate::session::{Command, Event, Output, Session, SessionConfig};

/// How to secure the socket.
#[derive(Clone)]
pub enum Tls {
    /// TLS, trusting the system's roots (the real servers).
    System,
    /// TLS with a caller's rustls config (tests: a self-signed root).
    Custom(Arc<rustls::ClientConfig>),
    /// No TLS. Only for the in-process test server on 127.0.0.1;
    /// `Client::start` refuses it for any other host.
    PlainForTests,
}

impl std::fmt::Debug for Tls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Tls::System => "System",
            Tls::Custom(_) => "Custom",
            Tls::PlainForTests => "PlainForTests",
        })
    }
}

/// A server to connect to.
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub tls: Tls,
}

impl Endpoint {
    /// `host[:port]` over TLS (port 6697 if none is given).
    pub fn parse(s: &str) -> Option<Endpoint> {
        let s = s.trim();
        let (host, port) = match s.rsplit_once(':') {
            Some((h, p)) => (h, p.parse().ok()?),
            None => (s, 6697),
        };
        let ok = !host.is_empty()
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        ok.then(|| Endpoint {
            host: host.to_ascii_lowercase(),
            port,
            tls: Tls::System,
        })
    }

    pub fn is_loopback(&self) -> bool {
        matches!(self.host.as_str(), "127.0.0.1" | "localhost" | "::1")
    }
}

/// Timings (shortened in tests).
#[derive(Debug, Clone)]
pub struct Timing {
    /// Waits before each reconnect; the last repeats.
    pub backoff: Vec<Duration>,
    pub connect_timeout: Duration,
    /// With nothing heard for this long, send a PING …
    pub idle_ping: Duration,
    /// … and give up on the connection after this long.
    pub idle_drop: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            backoff: [2, 5, 15, 30, 60]
                .into_iter()
                .map(Duration::from_secs)
                .collect(),
            connect_timeout: Duration::from_secs(15),
            idle_ping: Duration::from_secs(120),
            idle_drop: Duration::from_secs(240),
        }
    }
}

enum Ctl {
    Cmd(Command),
    Stop,
}

/// A running client. Dropping it stops the connection (QUIT, then close).
pub struct Client {
    tx: Sender<Ctl>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

type Sink = Box<dyn Fn(Event) + Send>;

impl Client {
    /// Connect in the background and keep connected. Every event goes to
    /// `sink`, on the client's thread.
    pub fn start(
        endpoint: Endpoint,
        session: SessionConfig,
        timing: Timing,
        sink: impl Fn(Event) + Send + 'static,
    ) -> Client {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let sink: Sink = Box::new(sink);
        let thread = std::thread::Builder::new()
            .name("burrow-irc".into())
            .spawn(move || run(endpoint, session, timing, rx, s2, sink))
            .ok();
        Client { tx, stop, thread }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(Ctl::Cmd(cmd));
    }

    /// Stop, and wait (briefly) for the thread to say goodbye.
    pub fn stop(mut self) {
        self.halt();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    fn halt(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.tx.send(Ctl::Stop);
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.halt();
    }
}

enum Stream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Stream {
    fn tcp(&self) -> &TcpStream {
        match self {
            Stream::Plain(s) => s,
            Stream::Tls(s) => &s.sock,
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Plain(s) => s.read(buf),
            Stream::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Plain(s) => s.write(buf),
            Stream::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Stream::Plain(s) => s.flush(),
            Stream::Tls(s) => s.flush(),
        }
    }
}

fn system_tls() -> Result<Arc<rustls::ClientConfig>, String> {
    static CONFIG: OnceLock<Result<Arc<rustls::ClientConfig>, String>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut roots = rustls::RootCertStore::empty();
            let found = rustls_native_certs::load_native_certs();
            for cert in found.certs {
                let _ = roots.add(cert);
            }
            if roots.is_empty() {
                return Err("no trusted certificates found on this computer".into());
            }
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .map(|b| Arc::new(b.with_root_certificates(roots).with_no_client_auth()))
                .map_err(|e| e.to_string())
        })
        .clone()
}

fn connect(ep: &Endpoint, timing: &Timing) -> Result<Stream, String> {
    if matches!(ep.tls, Tls::PlainForTests) && !ep.is_loopback() {
        return Err("plain connections are for the local test server only".into());
    }
    let addrs: Vec<_> = (ep.host.as_str(), ep.port)
        .to_socket_addrs()
        .map_err(|e| format!("couldn't find {}: {e}", ep.host))?
        .collect();
    let mut last = format!("couldn't reach {}", ep.host);
    let mut tcp = None;
    for a in addrs {
        match TcpStream::connect_timeout(&a, timing.connect_timeout) {
            Ok(s) => {
                tcp = Some(s);
                break;
            }
            Err(e) => last = format!("couldn't reach {}: {e}", ep.host),
        }
    }
    let tcp = tcp.ok_or(last)?;
    let _ = tcp.set_nodelay(true);
    let config = match &ep.tls {
        Tls::PlainForTests => return Ok(Stream::Plain(tcp)),
        Tls::System => system_tls()?,
        Tls::Custom(c) => c.clone(),
    };
    let name = rustls::pki_types::ServerName::try_from(ep.host.clone())
        .map_err(|e| format!("bad server name: {e}"))?;
    let conn = rustls::ClientConnection::new(config, name).map_err(|e| e.to_string())?;
    let mut tls = rustls::StreamOwned::new(conn, tcp);
    tls.sock
        .set_read_timeout(Some(timing.connect_timeout))
        .map_err(|e| e.to_string())?;
    while tls.conn.is_handshaking() {
        tls.conn
            .complete_io(&mut tls.sock)
            .map_err(|e| format!("secure connection failed: {e}"))?;
    }
    Ok(Stream::Tls(Box::new(tls)))
}

/// How a connection ended.
enum End {
    /// Stop asked (or the session is done for good).
    Stop,
    /// Lost; try again. `welcomed`: it got as far as 001.
    Lost { reason: String, welcomed: bool },
}

fn run(
    ep: Endpoint,
    cfg: SessionConfig,
    timing: Timing,
    rx: Receiver<Ctl>,
    stop: Arc<AtomicBool>,
    sink: Sink,
) {
    let mut session = Session::new(cfg);
    let mut attempt: u32 = 0;
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        sink(Event::Connecting { attempt });
        let end = match connect(&ep, &timing) {
            Ok(stream) => {
                sink(Event::Connected);
                serve(stream, &mut session, &timing, &rx, &stop, &sink)
            }
            Err(reason) => End::Lost {
                reason,
                welcomed: false,
            },
        };
        let reason = match end {
            End::Stop => {
                let reason = session.fatal().unwrap_or("stopped").to_string();
                sink(Event::Disconnected {
                    reason,
                    retry_in: None,
                });
                break;
            }
            End::Lost { reason, welcomed } => {
                if welcomed {
                    attempt = 0;
                }
                reason
            }
        };
        if let Some(f) = session.fatal() {
            sink(Event::Disconnected {
                reason: f.to_string(),
                retry_in: None,
            });
            break;
        }
        let i = (attempt as usize).min(timing.backoff.len().saturating_sub(1));
        let wait = timing
            .backoff
            .get(i)
            .copied()
            .unwrap_or(Duration::from_secs(30));
        sink(Event::Disconnected {
            reason,
            retry_in: Some(wait),
        });
        attempt = attempt.saturating_add(1);
        // Wait out the backoff; a Stop ends it, commands are dropped
        // (nothing can be sent while offline).
        let until = Instant::now() + wait;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match rx.recv_timeout(left) {
                Ok(Ctl::Stop) | Err(RecvTimeoutError::Disconnected) => {
                    stop.store(true, Ordering::SeqCst);
                    break;
                }
                Ok(Ctl::Cmd(_)) => {}
                Err(RecvTimeoutError::Timeout) => break,
            }
        }
    }
}

fn write_line(stream: &mut Stream, line: &str) -> io::Result<()> {
    let mut b = line.as_bytes().to_vec();
    b.extend_from_slice(b"\r\n");
    stream.write_all(&b)?;
    stream.flush()
}

fn serve(
    mut stream: Stream,
    session: &mut Session,
    timing: &Timing,
    rx: &Receiver<Ctl>,
    stop: &AtomicBool,
    sink: &Sink,
) -> End {
    let lost = |e: io::Error, s: &Session| End::Lost {
        reason: e.to_string(),
        welcomed: s.registered(),
    };
    if let Err(e) = stream
        .tcp()
        .set_read_timeout(Some(Duration::from_millis(100)))
    {
        return lost(e, session);
    }
    for line in session.start() {
        if let Err(e) = write_line(&mut stream, &line) {
            return lost(e, session);
        }
    }
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut heard = Instant::now();
    let mut pinged = false;
    loop {
        // What the user asked for.
        loop {
            match rx.try_recv() {
                Ok(Ctl::Cmd(c)) => {
                    if !session.registered() {
                        // Before the welcome only a quit makes sense.
                        if !matches!(c, Command::Quit { .. }) {
                            continue;
                        }
                    }
                    for o in session.command(&c) {
                        match o {
                            Output::Send(l) => {
                                if let Err(e) = write_line(&mut stream, &l) {
                                    return lost(e, session);
                                }
                            }
                            Output::Event(e) => sink(e),
                        }
                    }
                }
                Ok(Ctl::Stop) | Err(TryRecvError::Disconnected) => {
                    let _ = write_line(&mut stream, "QUIT :Burrow");
                    return End::Stop;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        if stop.load(Ordering::SeqCst) {
            let _ = write_line(&mut stream, "QUIT :Burrow");
            return End::Stop;
        }
        match stream.read(&mut chunk) {
            Ok(0) => {
                return End::Lost {
                    reason: "the server closed the connection".into(),
                    welcomed: session.registered(),
                };
            }
            Ok(n) => {
                heard = Instant::now();
                pinged = false;
                buf.extend_from_slice(&chunk[..n]);
                while let Some(i) = buf.iter().position(|&b| b == b'\n') {
                    let raw: Vec<u8> = buf.drain(..=i).collect();
                    let line = String::from_utf8_lossy(&raw);
                    let Ok(m) = Message::parse(&line) else {
                        continue;
                    };
                    for o in session.handle(&m) {
                        match o {
                            Output::Send(l) => {
                                if let Err(e) = write_line(&mut stream, &l) {
                                    return lost(e, session);
                                }
                            }
                            Output::Event(e) => sink(e),
                        }
                    }
                    if session.fatal().is_some() {
                        return End::Stop;
                    }
                }
                if buf.len() > 64 * 1024 {
                    buf.clear(); // a runaway line
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                let idle = heard.elapsed();
                if idle > timing.idle_drop {
                    return End::Lost {
                        reason: "the server stopped answering".into(),
                        welcomed: session.registered(),
                    };
                }
                if idle > timing.idle_ping && !pinged {
                    pinged = true;
                    if let Err(e) = write_line(&mut stream, "PING :burrow") {
                        return lost(e, session);
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return lost(e, session),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints() {
        let e = Endpoint::parse("irc.libera.chat").unwrap();
        assert_eq!((e.host.as_str(), e.port), ("irc.libera.chat", 6697));
        let e = Endpoint::parse("IRC.Example.org:7000").unwrap();
        assert_eq!((e.host.as_str(), e.port), ("irc.example.org", 7000));
        assert!(Endpoint::parse("").is_none());
        assert!(Endpoint::parse("bad host:1").is_none());
        assert!(Endpoint::parse("h:notaport").is_none());
        assert!(!Endpoint::parse("irc.libera.chat").unwrap().is_loopback());
    }

    #[test]
    fn plain_tcp_is_refused_off_loopback() {
        let ep = Endpoint {
            host: "irc.example.org".into(),
            port: 6667,
            tls: Tls::PlainForTests,
        };
        let err = connect(&ep, &Timing::default()).err().unwrap();
        assert!(err.contains("local test server"));
    }
}
