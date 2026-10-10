//! reading-time: Burrow's bundled extension that shows an estimated
//! reading time ("4 min", the word count on hover) at the end of each
//! reading entry's byline. A port of blygger-studio's `reading-time`
//! extension (studio 0.39.0), so both clients show the same estimate.
//!
//! It contributes one reading slot, `entry-byline` (docs/EXTENSIONS.md §
//! Reading slots), and answers `extension/entry.byline` from the entry the
//! host sends: no fetch, no file, no network. Off until
//! `extension = reading-time` and consent (`reading.read`).
//!
//! - [`count`]: the counting, pure.
//! - [`run`]: the BXP extension around it (`blygger +ext reading-time`).

pub mod count;

use std::path::PathBuf;
use std::process::ExitCode;

use blyg_ext::ext::{Extension, HostClient, serve};
use blyg_ext::manifest::{Installed, Manifest, Origin};
use blyg_ext::protocol::{InitializeParams, InitializeResult, PROTOCOL_VERSION, RpcError};
use blyg_ext::reading::{ENTRY_BYLINE, EntryByline, EntryBylineParams, ReadingEntry};
use blyg_ext::rpc::{params, to_value};
use serde_json::Value;

/// The extension's name (`extension = reading-time`).
pub const NAME: &str = "reading-time";

const MANIFEST: &str = r#"
name = "reading-time"
version = "0.1.0"
protocol = 1
description = "Shows an estimated reading time and word count at the end of each reading entry's byline."
capabilities = ["reading.read"]
entry-byline = true
"#;

/// The manifest (checked by the same rules as any extension's).
pub fn manifest() -> Manifest {
    Manifest::parse(MANIFEST, true).expect("the bundled manifest is valid")
}

/// The bundled extension for the host's `HostConfig::bundled`: run as
/// `program args…` (the app passes its own executable and
/// `["+ext", "reading-time"]`). Off until `extension = reading-time`.
pub fn bundled(program: PathBuf, args: Vec<String>) -> Installed {
    Installed {
        manifest: manifest(),
        origin: Origin::Bundled { program, args },
    }
}

/// The marker for one entry: "< 1 min" with "3 words" on hover, or none
/// when there's nothing to read. An entry held only as Markdown is
/// counted from the HTML its blyg publishes for it.
pub fn byline(entry: &ReadingEntry) -> Option<EntryByline> {
    let html = if entry.content_html.trim().is_empty() && !entry.content_md.trim().is_empty() {
        blyg_render::render_markdown(&entry.content_md)
    } else {
        entry.content_html.clone()
    };
    let t = count::reading_time(&html);
    let label = count::format_minutes(t.minutes);
    if label.is_empty() {
        return None;
    }
    let tip = count::count_label(&t);
    Some(EntryByline {
        text: label,
        tip: (!tip.is_empty()).then_some(tip),
    })
}

/// The extension: stateless.
#[derive(Debug, Default)]
pub struct ReadingTimeExt;

impl Extension for ReadingTimeExt {
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
            ENTRY_BYLINE => {
                let p: EntryBylineParams = params(p)?;
                to_value(&byline(&p.entry))
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
    serve(read, write, &mut ReadingTimeExt);
    ExitCode::SUCCESS
}

/// [`run`] over this process's stdin/stdout (`blygger +ext reading-time`).
pub fn run_stdio() -> ExitCode {
    run(std::io::stdin(), std::io::stdout())
}

#[cfg(test)]
mod tests {
    use super::*;
    use blyg_ext::Capability;

    fn entry(html: &str, md: &str) -> ReadingEntry {
        ReadingEntry {
            subscription_id: "s1".into(),
            remote_id: "r1".into(),
            origin: "https://blyg.example.com".into(),
            kind: "fragment".into(),
            state: "current".into(),
            version: 1,
            title: String::new(),
            content_html: html.into(),
            content_md: md.into(),
            content_hash: None,
        }
    }

    #[test]
    fn the_manifest_asks_to_read_and_adds_a_byline_only() {
        let m = manifest();
        assert_eq!(m.name, NAME);
        assert_eq!(m.capabilities, vec![Capability::ReadingRead]);
        assert!(m.entry_byline);
        assert!(m.entry_actions.is_empty() && m.commands.is_empty() && m.macros.is_empty());
    }

    // As the Studio's byline test: "· < 1 min" with "3 words" on hover,
    // and nothing for an empty entry (the host draws the "·").
    #[test]
    fn the_byline_shows_minutes_with_the_count_on_hover() {
        let b = byline(&entry("<p>one two three</p>", "one two three")).unwrap();
        assert_eq!(b.text, "< 1 min");
        assert_eq!(b.tip.as_deref(), Some("3 words"));
        assert_eq!(byline(&entry("", "")), None);
        let long = "tide ".repeat(920);
        let b = byline(&entry(&format!("<p>{long}</p>"), "")).unwrap();
        assert_eq!(b.text, "4 min");
        assert_eq!(b.tip.as_deref(), Some("920 words"));
    }

    #[test]
    fn markdown_only_entries_count_their_rendered_html() {
        let b = byline(&entry(
            "",
            "# Tides\n\nThe *moon* pulls the [sea](https://example.com).",
        ))
        .unwrap();
        assert_eq!(b.tip.as_deref(), Some("6 words"));
    }
}
