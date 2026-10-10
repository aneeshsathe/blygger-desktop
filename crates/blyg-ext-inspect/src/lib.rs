//! inspect: Burrow's bundled extension that adds "inspect" to each reading
//! entry's ⋯ sheet: the record Burrow holds for the entry (ids, versions,
//! references, hashes) and its JSON, bodies elided. For people building
//! clients and debugging mentions and imports. A port of blygger-studio's
//! `inspect` extension (studio 0.39.0).
//!
//! It contributes one reading slot, an `[[entry-actions]]` row (docs/
//! EXTENSIONS.md § Reading slots), and answers `extension/entry.action`
//! from the record the host sends: nothing is fetched from the item's
//! origin. Off until `extension = inspect` and consent (`reading.read`).
//!
//! - [`shape`]: elision and the summary, pure.

pub mod shape;

use std::path::PathBuf;
use std::process::ExitCode;

use blyg_ext::ext::{Extension, HostClient, serve};
use blyg_ext::manifest::{Installed, Manifest, Origin};
use blyg_ext::protocol::{InitializeParams, InitializeResult, PROTOCOL_VERSION, RpcError};
use blyg_ext::reading::{ENTRY_ACTION, EntryActionParams, EntrySheet, SheetField};
use blyg_ext::rpc::{params, to_value};
use serde_json::Value;

/// The extension's name (`extension = inspect`).
pub const NAME: &str = "inspect";
/// Its one ⋯ row.
pub const ACTION: &str = "inspect";

const MANIFEST: &str = r#"
name = "inspect"
version = "0.1.0"
protocol = 1
description = "Adds “inspect” to each reading entry’s ⋯ sheet: the record Burrow holds for it (ids, versions, references, hashes) and its JSON."
capabilities = ["reading.read"]

[[entry-actions]]
id = "inspect"
title = "inspect"
detail = "ids, versions, references, JSON"
icon = "{ }"
"#;

/// The key prefix of a post Burrow fetched to show a quote's original
/// (`reading/original.rs`): never a real subscription id.
const EXTERNAL: &str = "ext:";

/// The manifest (checked by the same rules as any extension's).
pub fn manifest() -> Manifest {
    Manifest::parse(MANIFEST, true).expect("the bundled manifest is valid")
}

/// The bundled extension for the host's `HostConfig::bundled`: run as
/// `program args…` (the app passes its own executable and
/// `["+ext", "inspect"]`). Off until `extension = inspect`.
pub fn bundled(program: PathBuf, args: Vec<String>) -> Installed {
    Installed {
        manifest: manifest(),
        origin: Origin::Bundled { program, args },
    }
}

/// The sheet for one entry.
pub fn inspect(p: &EntryActionParams) -> Result<EntrySheet, RpcError> {
    if p.action != ACTION {
        return Err(RpcError::invalid_params(format!(
            "{NAME} has no action {:?}",
            p.action
        )));
    }
    let record = if p.record.is_null() {
        serde_json::to_value(&p.entry).unwrap_or(Value::Null)
    } else {
        p.record.clone()
    };
    let fields = shape::summary(&record, p.entry.content_hash.as_deref())
        .into_iter()
        .map(|(label, value)| SheetField { label, value })
        .collect();
    let description = if p.entry.subscription_id.starts_with(EXTERNAL) {
        "Fetched from the author’s public files to show it; Burrow doesn’t keep it."
    } else {
        "The copy Burrow imported."
    };
    Ok(EntrySheet {
        title: "inspect".into(),
        description: Some(description.into()),
        text: String::new(),
        fields,
        code: Some(shape::shaped_json(&record)),
        language: Some("json".into()),
    })
}

/// The extension: stateless.
#[derive(Debug, Default)]
pub struct InspectExt;

impl Extension for InspectExt {
    fn initialize(
        &mut self,
        _host: &HostClient,
        _p: InitializeParams,
    ) -> Result<InitializeResult, RpcError> {
        Ok(InitializeResult {
            protocol_version: PROTOCOL_VERSION,
            name: NAME.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            commands: vec![],
            sources: vec![],
            libraries: vec![],
        })
    }

    fn request(&mut self, _host: &HostClient, method: &str, p: Value) -> Result<Value, RpcError> {
        match method {
            ENTRY_ACTION => {
                let p: EntryActionParams = params(p)?;
                to_value(&inspect(&p)?)
            }
            m => Err(RpcError::method_not_found(m)),
        }
    }
}

/// Serve the extension over `read`/`write` until shutdown.
pub fn run<R, W>(read: R, write: W) -> ExitCode
where
    R: std::io::Read + Send + 'static,
    W: std::io::Write + Send + 'static,
{
    serve(read, write, &mut InspectExt);
    ExitCode::SUCCESS
}

/// [`run`] over this process's stdin/stdout (`blygger +ext inspect`).
pub fn run_stdio() -> ExitCode {
    run(std::io::stdin(), std::io::stdout())
}

#[cfg(test)]
mod tests {
    use super::*;
    use blyg_ext::Capability;
    use blyg_ext::protocol::codes;
    use blyg_ext::reading::{ReadingEntry, record_of};

    /// An invented reading row in Burrow's own shape.
    fn row() -> Value {
        serde_json::json!({
            "subscription_id": "sub-1", "remote_id": "r1", "subscription_title": "Rue",
            "origin": "https://blyg.example.com", "kind": "fragment", "state": "current",
            "version": 2, "created": null, "updated": null,
            "observed_at": "2026-10-01T00:00:00Z",
            "content_md": "The harbour bench floods.",
            "content_html": "<p>The harbour bench floods.</p>",
            "author": null, "page": null, "thumb": null, "hoppers": [],
            "stub_of": {"url": "https://elsewhere.example/post"},
        })
    }

    fn params(record: Value) -> EntryActionParams {
        let entry = ReadingEntry {
            subscription_id: "sub-1".into(),
            remote_id: "r1".into(),
            origin: "https://blyg.example.com".into(),
            kind: "fragment".into(),
            state: "current".into(),
            version: 2,
            title: String::new(),
            content_html: "<p>The harbour bench floods.</p>".into(),
            content_md: "The harbour bench floods.".into(),
            content_hash: Some("sha256:00".into()),
        };
        EntryActionParams {
            action: ACTION.into(),
            entry,
            record,
        }
    }

    #[test]
    fn the_manifest_adds_one_row_and_asks_to_read() {
        let m = manifest();
        assert_eq!(m.name, NAME);
        assert_eq!(m.capabilities, vec![Capability::ReadingRead]);
        assert!(!m.entry_byline);
        assert_eq!(m.entry_actions.len(), 1);
        let a = &m.entry_actions[0];
        assert_eq!((a.id.as_str(), a.title.as_str()), ("inspect", "inspect"));
        assert_eq!(a.detail, "ids, versions, references, JSON");
        assert_eq!(a.icon.as_deref(), Some("{ }"));
    }

    #[test]
    fn the_sheet_summarises_and_elides() {
        let s = inspect(&params(row())).unwrap();
        assert_eq!(s.title, "inspect");
        assert_eq!(s.description.as_deref(), Some("The copy Burrow imported."));
        let f: Vec<(&str, &str)> = s
            .fields
            .iter()
            .map(|f| (f.label.as_str(), f.value.as_str()))
            .collect();
        assert_eq!(
            f,
            [
                ("id", "r1"),
                ("kind", "fragment"),
                ("version", "2"),
                ("state", "current"),
                ("content hash", "sha256:00"),
                ("stub of", "https://elsewhere.example/post"),
                ("transclusions", "0"),
            ]
        );
        let code = s.code.unwrap();
        assert!(
            code.contains("\"content_md\": \"‹25 characters, not shown›\""),
            "{code}"
        );
        assert!(!code.contains("harbour"), "bodies never shown");
        assert_eq!(s.language.as_deref(), Some("json"));
    }

    #[test]
    fn a_real_reading_row_round_trips() {
        let r: blyg_core::ReadingItem = serde_json::from_value(row()).unwrap();
        let entry = ReadingEntry::of(&r);
        assert_eq!(
            entry.content_hash.as_deref(),
            Some(blyg_core::content_hash("The harbour bench floods.").as_str())
        );
        let mut p = params(record_of(&r));
        p.entry = entry;
        let s = inspect(&p).unwrap();
        assert!(s.fields.iter().any(|f| f.label == "id" && f.value == "r1"));
        assert!(
            s.fields
                .iter()
                .any(|f| f.label == "content hash" && f.value.starts_with("sha256:"))
        );
        assert_eq!(s.fields.last().map(|f| f.value.as_str()), Some("0"));
    }

    #[test]
    fn another_action_is_refused() {
        let mut p = params(row());
        p.action = "other".into();
        assert_eq!(inspect(&p).unwrap_err().code, codes::INVALID_PARAMS);
    }
}
