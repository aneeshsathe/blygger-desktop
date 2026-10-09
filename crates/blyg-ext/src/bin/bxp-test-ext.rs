//! A tiny BXP extension for the host's tests. Not shipped.
//!
//! Commands (`extension/command` ids):
//! - `echo`: toasts back the selection.
//! - `sleep`: answers after 10 s (for timeouts).
//! - `crash`: exits with code 7.
//! - `call-host`: the selection is `{"method": …, "params": …}`; calls it on
//!   the host and toasts `{"ok": result}` or `{"err": error}` as JSON.
//! - `env`: toasts the names of its environment variables, as JSON.
//! - `stderr`: writes two lines to stderr.
//!
//! `extension/macro.prepare` for macro `page` calls `burrow/browser.page`
//! (outside any command) and answers the host's reply as the text; any
//! other macro is method-not-found.
//!
//! Settings: `hang-init=1` (never answers initialize), `crash-init=1`
//! (exits during initialize), `protocol=<n>` (answers with that version).
//! Every initialize is appended to `<storageDir>/starts.log`; notifications
//! to `<storageDir>/events.jsonl`.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use blyg_ext::ext::{Extension, HostClient, serve};
use blyg_ext::protocol::*;
use blyg_ext::rpc::{params, to_value};
use serde_json::{Value, json};

#[derive(Default)]
struct TestExt {
    storage: Option<PathBuf>,
}

impl TestExt {
    fn append(&self, file: &str, line: &str) {
        if let Some(d) = &self.storage
            && let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(d.join(file))
        {
            let _ = writeln!(f, "{line}");
        }
    }
}

impl Extension for TestExt {
    fn initialize(
        &mut self,
        _host: &HostClient,
        p: InitializeParams,
    ) -> Result<InitializeResult, RpcError> {
        self.storage = Some(p.storage_dir.clone());
        self.append("starts.log", &serde_json::to_string(&p).unwrap_or_default());
        let set = |k: &str| p.settings.get(k).map(String::as_str);
        if set("crash-init") == Some("1") {
            std::process::exit(3);
        }
        if set("hang-init") == Some("1") {
            std::thread::sleep(Duration::from_secs(600));
        }
        let protocol_version = set("protocol")
            .and_then(|v| v.parse().ok())
            .unwrap_or(PROTOCOL_VERSION);
        Ok(InitializeResult {
            protocol_version,
            name: "bxp-test".into(),
            version: "0.0.1".into(),
            commands: vec![],
            sources: vec![],
            libraries: vec![],
        })
    }

    fn request(&mut self, host: &HostClient, method: &str, p: Value) -> Result<Value, RpcError> {
        match method {
            methods::COMMAND => {
                let p: CommandParams = params(p)?;
                let sel = p.context.selection.clone().unwrap_or_default();
                let toast = match p.id.as_str() {
                    "echo" => sel,
                    "sleep" => {
                        std::thread::sleep(Duration::from_secs(10));
                        "slept".into()
                    }
                    "crash" => std::process::exit(7),
                    "call-host" => {
                        let req: Value =
                            serde_json::from_str(&sel).map_err(RpcError::invalid_params)?;
                        let m = req["method"].as_str().unwrap_or_default();
                        let r = host.call(m, req.get("params").cloned().unwrap_or(json!({})));
                        match r {
                            Ok(v) => json!({ "ok": v }).to_string(),
                            Err(e) => json!({ "err": e }).to_string(),
                        }
                    }
                    "env" => {
                        let mut keys: Vec<String> = std::env::vars_os()
                            .map(|(k, _)| k.to_string_lossy().into_owned())
                            .collect();
                        keys.sort();
                        json!(keys).to_string()
                    }
                    "stderr" => {
                        eprintln!("first line to stderr");
                        eprintln!("second line to stderr");
                        "wrote".into()
                    }
                    other => return Err(RpcError::new(-1, format!("no command {other}"))),
                };
                to_value(&CommandResult {
                    toast: Some(toast),
                    open: None,
                })
            }
            methods::LIBRARY_READ => {
                let p: LibraryReadParams = params(p)?;
                to_value(&LibraryDocument {
                    id: p.id.clone(),
                    title: p.id,
                    markdown: "hello".into(),
                    body: "hello".into(),
                    hash: "sha256:00".into(),
                })
            }
            // Macro "page": calls `burrow/browser.page` outside any command
            // and answers what the host said as the text. Others: none.
            methods::MACRO_PREPARE if p["macro"] == "page" => {
                let r = match host.call(methods::BROWSER_PAGE, json!({})) {
                    Ok(v) => json!({ "ok": v }),
                    Err(e) => json!({ "err": e }),
                };
                to_value(&MacroPrepareResult {
                    text: r.to_string(),
                    note: None,
                })
            }
            methods::LIBRARY_WRITE => Err(RpcError::new(codes::STALE, "changed on disk")
                .with_data(json!({ "currentHash": "sha256:11" }))),
            m => Err(RpcError::method_not_found(m)),
        }
    }

    fn notification(&mut self, _host: &HostClient, method: &str, params: Value) {
        self.append(
            "events.jsonl",
            &json!({ "method": method, "params": params }).to_string(),
        );
    }
}

fn main() {
    serve(std::io::stdin(), std::io::stdout(), &mut TestExt::default());
}
