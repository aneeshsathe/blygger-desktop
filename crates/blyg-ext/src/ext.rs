//! The extension side, for extensions written in Rust (the bundled
//! markdown-notes, the test extension). Implement [`Extension`] and call
//! [`serve`] with stdin/stdout; use [`HostClient`] for the `burrow/*` calls.
//! An extension in another language just speaks the same lines.

use std::io::{Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::protocol::*;
use crate::rpc::{self, Connection, Incoming};

/// How long the client waits for Burrow to answer a `burrow/*` call.
/// `requestCapability` waits for the user, so it gets longer.
pub const HOST_CALL_TIMEOUT: Duration = Duration::from_secs(30);
pub const CONSENT_TIMEOUT: Duration = Duration::from_secs(300);

/// An extension. Every method runs on the thread that called [`serve`],
/// one at a time, so `&mut self` needs no locking. A `burrow/*` call made
/// from inside a handler is fine: its answer arrives on the reader thread.
pub trait Extension {
    fn initialize(
        &mut self,
        host: &HostClient,
        params: InitializeParams,
    ) -> Result<InitializeResult, RpcError>;

    /// Any other request (`extension/command`, `extension/library.*`, …).
    fn request(
        &mut self,
        host: &HostClient,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        let _ = (host, params);
        Err(RpcError::method_not_found(method))
    }

    /// A notification (`burrow/itemPublished`, `burrow/settingsChanged`, …).
    fn notification(&mut self, host: &HostClient, method: &str, params: Value) {
        let _ = (host, method, params);
    }

    /// Called about every [`tick_interval`](Self::tick_interval) while idle.
    fn tick(&mut self, host: &HostClient) {
        let _ = host;
    }

    fn tick_interval(&self) -> Option<Duration> {
        None
    }
}

/// Run `ext` until the host sends `shutdown` (answered, then return) or
/// closes the stream. A panicking handler answers `-32603` and the
/// extension keeps running.
pub fn serve<R, W, E>(read: R, write: W, ext: &mut E)
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
    E: Extension,
{
    let (conn, rx) = Connection::spawn(read, write);
    let host = HostClient { conn: conn.clone() };
    let mut next_tick = ext.tick_interval().map(|t| Instant::now() + t);
    loop {
        // The interval may be set (or change) once `initialize` has run.
        match (ext.tick_interval(), next_tick) {
            (None, _) => next_tick = None,
            (Some(t), None) => next_tick = Some(Instant::now() + t),
            _ => {}
        }
        // Tick on a deadline, so a steady stream of requests can't starve it.
        if let Some(due) = next_tick
            && Instant::now() >= due
        {
            let _ = catch_unwind(AssertUnwindSafe(|| ext.tick(&host)));
            next_tick = ext.tick_interval().map(|t| Instant::now() + t);
        }
        let msg = match next_tick {
            Some(due) => match rx.recv_timeout(due.saturating_duration_since(Instant::now())) {
                Ok(m) => m,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(m) => m,
                Err(_) => break,
            },
        };
        match msg {
            Incoming::Request { id, method, params } => {
                if method == methods::SHUTDOWN {
                    conn.respond(id, Ok(json!({})));
                    break;
                }
                let r = catch_unwind(AssertUnwindSafe(|| {
                    if method == methods::INITIALIZE {
                        rpc::params(params)
                            .and_then(|p| ext.initialize(&host, p))
                            .and_then(|r| rpc::to_value(&r))
                    } else {
                        ext.request(&host, &method, params)
                    }
                }))
                .unwrap_or_else(|_| {
                    Err(RpcError::new(
                        codes::INTERNAL_ERROR,
                        "the extension panicked",
                    ))
                });
                conn.respond(id, r);
            }
            Incoming::Notification { method, params } => {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    ext.notification(&host, &method, params)
                }));
            }
        }
    }
    conn.close_and_flush();
}

/// Typed `burrow/*` calls. Each needs its capability; without it Burrow
/// answers `-32001 permission denied`.
#[derive(Clone)]
pub struct HostClient {
    conn: Connection,
}

impl HostClient {
    /// The raw connection, for a method this client doesn't wrap.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.conn.request(method, params, HOST_CALL_TIMEOUT)
    }

    pub fn list_items(&self, p: &ListItemsParams) -> Result<Vec<ItemSummary>, RpcError> {
        self.conn.call(methods::LIST_ITEMS, p, HOST_CALL_TIMEOUT)
    }

    pub fn get_item(&self, id: &str) -> Result<ItemSummary, RpcError> {
        self.conn.call(
            methods::GET_ITEM,
            &GetItemParams { id: id.into() },
            HOST_CALL_TIMEOUT,
        )
    }

    pub fn search_items(
        &self,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ItemSummary>, RpcError> {
        let p = SearchItemsParams {
            query: query.into(),
            limit,
        };
        self.conn.call(methods::SEARCH_ITEMS, &p, HOST_CALL_TIMEOUT)
    }

    pub fn create_draft(&self, p: &CreateDraftParams) -> Result<CreatedDraft, RpcError> {
        self.conn.call(methods::CREATE_DRAFT, p, HOST_CALL_TIMEOUT)
    }

    pub fn save_item(&self, p: &SaveItemParams) -> Result<SavedItem, RpcError> {
        self.conn.call(methods::SAVE_ITEM, p, HOST_CALL_TIMEOUT)
    }

    pub fn list_reading(&self, p: &ListReadingParams) -> Result<Vec<ReadingSummary>, RpcError> {
        self.conn.call(methods::LIST_READING, p, HOST_CALL_TIMEOUT)
    }

    pub fn open_item(&self, id: &str) -> Result<(), RpcError> {
        self.conn
            .request(methods::OPEN_ITEM, json!({ "id": id }), HOST_CALL_TIMEOUT)
            .map(|_| ())
    }

    pub fn toast(&self, text: &str, detail: Option<&str>) -> Result<(), RpcError> {
        let p = ToastParams {
            text: text.into(),
            detail: detail.map(str::to_string),
        };
        self.conn
            .request(methods::TOAST, rpc::to_value(&p)?, HOST_CALL_TIMEOUT)
            .map(|_| ())
    }

    /// Ask the user for a capability not granted yet (once per capability
    /// per session; later asks answer false without asking).
    pub fn request_capability(&self, capability: &str) -> Result<bool, RpcError> {
        let p = RequestCapabilityParams {
            capability: capability.into(),
        };
        let a: CapabilityAnswer =
            self.conn
                .call(methods::REQUEST_CAPABILITY, &p, CONSENT_TIMEOUT)?;
        Ok(a.granted)
    }
}
