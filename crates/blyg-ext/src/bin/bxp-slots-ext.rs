//! The reading-slots test extension (`tests/reading.rs`): answers
//! `extension/entry.byline` and `extension/entry.action` from what the host
//! sends, so the test can see exactly what crossed the wire.
//!
//! - byline: `v<version> <remoteId>` with the content hash as the tip;
//!   nothing for an entry whose Markdown says "silent"; an error for
//!   "broken"; a 3 s nap for "slow".
//! - action `echo`: a sheet whose fields are the entry's ids and the
//!   record's keys, and whose code is the record; action `fail`: an error.

use blyg_ext::ext::{Extension, HostClient, serve};
use blyg_ext::protocol::*;
use blyg_ext::reading::*;
use blyg_ext::rpc::{params, to_value};
use serde_json::Value;

struct Slots;

impl Extension for Slots {
    fn initialize(
        &mut self,
        _host: &HostClient,
        _p: InitializeParams,
    ) -> Result<InitializeResult, RpcError> {
        Ok(InitializeResult {
            protocol_version: PROTOCOL_VERSION,
            name: "bxp-slots".into(),
            version: "0.0.1".into(),
            commands: vec![],
            sources: vec![],
            libraries: vec![],
        })
    }

    fn request(&mut self, _host: &HostClient, method: &str, p: Value) -> Result<Value, RpcError> {
        match method {
            ENTRY_BYLINE => {
                let p: EntryBylineParams = params(p)?;
                let md = p.entry.content_md.as_str();
                if md.contains("slow") {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                }
                if md.contains("broken") {
                    return Err(RpcError::new(-1, "broken entry"));
                }
                if md.contains("silent") {
                    return Ok(Value::Null);
                }
                to_value(&EntryByline {
                    text: format!("v{} {}", p.entry.version, p.entry.remote_id),
                    tip: p.entry.content_hash.clone(),
                })
            }
            ENTRY_ACTION => {
                let p: EntryActionParams = params(p)?;
                match p.action.as_str() {
                    "echo" => {
                        let keys = p
                            .record
                            .as_object()
                            .map(|o| o.keys().cloned().collect::<Vec<_>>().join(","))
                            .unwrap_or_default();
                        to_value(&EntrySheet {
                            title: "echo".into(),
                            description: None,
                            text: format!("{} {}", p.entry.subscription_id, p.entry.remote_id),
                            fields: vec![SheetField {
                                label: "keys".into(),
                                value: keys,
                            }],
                            code: Some(p.record.to_string()),
                            language: Some("json".into()),
                        })
                    }
                    "fail" => Err(RpcError::new(-1, "it didn't work")),
                    other => Err(RpcError::invalid_params(other)),
                }
            }
            m => Err(RpcError::method_not_found(m)),
        }
    }
}

fn main() {
    serve(std::io::stdin(), std::io::stdout(), &mut Slots);
}
