//! Reading-pane slots: what an extension adds to each reading entry. They
//! mirror the web Studio's extension slots (blygger-studio
//! `src/ui/extension-api.ts`), named after them:
//!
//! - **`entryByline`**: a short text marker at the end of each entry's
//!   byline (the bundled `reading-time` shows "4 min"). Manifest
//!   `entry-byline = true`; the host asks `extension/entry.byline` per
//!   entry, off the main thread, and caches the answer by (entry, version).
//!   No answer, an error or an empty `text` shows nothing.
//! - **`entryActions`**: rows in an entry's ⋯ sheet (manifest
//!   `[[entry-actions]]`). Choosing one asks `extension/entry.action`, and
//!   the host shows the answer ([`EntrySheet`]) natively: plain text,
//!   label/value fields, and an optional monospaced code block with a Copy
//!   button. The bundled `inspect` is one.
//!
//! Both hand the extension the entry's data, so both need `reading.read`:
//! the manifest must declare it, and the host only asks extensions that
//! were granted it. Both are optional methods of protocol 1 (an extension
//! that doesn't declare them is never asked), so no version bump.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::capability::Capability;
use crate::host::{ExtError, Host};
use crate::protocol::{RpcError, codes, kind_str};
use crate::rpc::to_value;

/// `extension/entry.byline` (host -> extension, 2 s): the marker for one
/// entry ([`EntryBylineParams`] -> [`EntryByline`]).
pub const ENTRY_BYLINE: &str = "extension/entry.byline";
/// `extension/entry.action` (host -> extension, 5 s): the sheet for one of
/// the extension's ⋯ rows on one entry ([`EntryActionParams`] ->
/// [`EntrySheet`]).
pub const ENTRY_ACTION: &str = "extension/entry.action";

/// The longest marker the host draws, in characters; longer ones are cut
/// with "…".
pub const BYLINE_MAX: usize = 40;

/// A row an extension adds to every reading entry's ⋯ sheet (manifest
/// `[[entry-actions]]`, as the Studio's `EntryAction`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryActionSpec {
    /// Sent back in [`EntryActionParams::action`].
    pub id: String,
    /// The row's label ("inspect").
    pub title: String,
    /// A line under it ("ids, versions, references, JSON").
    #[serde(default)]
    pub detail: String,
    /// A short glyph before it ("{ }").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// One reading entry, as the client holds it: the post from a
/// subscription (or one fetched to show a quote's original).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingEntry {
    pub subscription_id: String,
    pub remote_id: String,
    /// The author's blyg (or the feed's site).
    pub origin: String,
    /// `fragment` or `thread`.
    pub kind: String,
    /// `current` or `tombstone`.
    pub state: String,
    pub version: u32,
    #[serde(default)]
    pub title: String,
    /// The published HTML as held; `""` when the client holds only the
    /// Markdown (render `contentMd` then, as Burrow's reader does).
    #[serde(default)]
    pub content_html: String,
    #[serde(default)]
    pub content_md: String,
    /// `"sha256:" + hex(SHA-256(contentMd))`, the protocol's
    /// `content_hash`; absent when the client holds no Markdown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

impl ReadingEntry {
    pub fn of(r: &blyg_core::ReadingItem) -> Self {
        ReadingEntry {
            subscription_id: r.subscription_id.clone(),
            remote_id: r.remote_id.clone(),
            origin: r.origin.clone(),
            kind: kind_str(r.kind).into(),
            state: r.state.clone(),
            version: r.version,
            title: blyg_core::plain_title(&r.content_md).unwrap_or_default(),
            content_html: r.content_html.clone(),
            content_md: r.content_md.clone(),
            content_hash: (!r.content_md.is_empty())
                .then(|| blyg_core::content_hash(&r.content_md)),
        }
    }
}

/// The record the client stores for `r`, as JSON (snake_case, Burrow's
/// own reading row, bodies included): what `inspect` shows.
pub fn record_of(r: &blyg_core::ReadingItem) -> Value {
    serde_json::to_value(r).unwrap_or(Value::Null)
}

/// `extension/entry.byline`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryBylineParams {
    pub entry: ReadingEntry,
}

/// The answer to `extension/entry.byline`. An empty `text` shows nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryByline {
    /// Plain text, one line ("4 min"); the host puts it after a "·".
    #[serde(default)]
    pub text: String,
    /// Shown on hover ("920 words").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip: Option<String>,
}

impl EntryByline {
    /// What the host draws: trimmed, on one line, at most [`BYLINE_MAX`]
    /// characters; `None` when there's nothing.
    pub fn shown(&self) -> Option<EntryByline> {
        let line: String = self.text.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            return None;
        }
        let text = if line.chars().count() > BYLINE_MAX {
            let cut: String = line.chars().take(BYLINE_MAX - 1).collect();
            format!("{}…", cut.trim_end())
        } else {
            line
        };
        Some(EntryByline {
            text,
            tip: self.tip.clone().filter(|t| !t.trim().is_empty()),
        })
    }
}

/// `extension/entry.action`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryActionParams {
    /// The [`EntryActionSpec::id`] chosen.
    pub action: String,
    pub entry: ReadingEntry,
    /// The record the client stores for the entry ([`record_of`]).
    #[serde(default)]
    pub record: Value,
}

/// One label/value line of an [`EntrySheet`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetField {
    pub label: String,
    pub value: String,
}

/// The answer to `extension/entry.action`: a sheet the host draws
/// natively, in this order: title, description, text, fields, code (with
/// a Copy button that copies it). Nothing in it is markup.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySheet {
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Plain text (paragraphs separated by blank lines).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<SheetField>,
    /// A block shown monospaced, as given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// What the code is (`json`): the Copy button says "Copy JSON".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl EntrySheet {
    /// The Copy button's label.
    pub fn copy_label(&self) -> String {
        match self.language.as_deref().map(str::trim) {
            Some(l) if l.eq_ignore_ascii_case("json") => "Copy JSON".into(),
            Some(l) if !l.is_empty() => format!("Copy {l}"),
            _ => "Copy".into(),
        }
    }
}

/// The reading slots of one running extension granted `reading.read`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySlots {
    pub ext: String,
    /// It answers `extension/entry.byline`.
    pub byline: bool,
    pub actions: Vec<EntryActionSpec>,
}

impl Host {
    /// The reading slots of running extensions that declare any and hold
    /// `reading.read`, by extension name. Cheap (no call into an
    /// extension): the UI asks it while drawing.
    pub fn entry_slots(&self) -> Vec<EntrySlots> {
        self.running_manifests()
            .into_iter()
            .filter(|(_, m, granted)| {
                (m.entry_byline || !m.entry_actions.is_empty())
                    && granted
                        .iter()
                        .any(|g| Capability::ReadingRead.covered_by(g))
            })
            .map(|(ext, m, _)| EntrySlots {
                ext,
                byline: m.entry_byline,
                actions: m.entry_actions,
            })
            .collect()
    }

    /// Ask `ext` for `entry`'s byline marker (blocking, up to
    /// `Timing::search`; run it off the main thread). `Ok(None)` when it
    /// has nothing to show (or doesn't implement the method).
    pub fn entry_byline(
        &self,
        ext: &str,
        entry: &ReadingEntry,
    ) -> Result<Option<EntryByline>, ExtError> {
        self.slot_allowed(ext, |s| s.byline)?;
        let p = to_value(&EntryBylineParams {
            entry: entry.clone(),
        })
        .map_err(ExtError::Rpc)?;
        match self.request::<Option<EntryByline>>(ext, ENTRY_BYLINE, p, self.timing().search) {
            Ok(b) => Ok(b.and_then(|b| b.shown())),
            Err(ExtError::Rpc(e)) if e.code == codes::METHOD_NOT_FOUND => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Run one of `ext`'s ⋯ rows on `r` (blocking, up to `Timing::read`).
    pub fn entry_action(
        &self,
        ext: &str,
        action: &str,
        r: &blyg_core::ReadingItem,
    ) -> Result<EntrySheet, ExtError> {
        self.slot_allowed(ext, |s| s.actions.iter().any(|a| a.id == action))?;
        let p = to_value(&EntryActionParams {
            action: action.into(),
            entry: ReadingEntry::of(r),
            record: record_of(r),
        })
        .map_err(ExtError::Rpc)?;
        self.request(ext, ENTRY_ACTION, p, self.timing().read)
    }

    /// `ext` is running, holds `reading.read` and declares the slot.
    fn slot_allowed(&self, ext: &str, has: impl Fn(&EntrySlots) -> bool) -> Result<(), ExtError> {
        match self.entry_slots().iter().find(|s| s.ext == ext) {
            Some(s) if has(s) => Ok(()),
            Some(_) => Err(ExtError::Rpc(RpcError::method_not_found(ENTRY_ACTION))),
            None if self.running_manifests().iter().any(|(n, _, _)| n == ext) => {
                Err(ExtError::Rpc(RpcError::permission_denied(
                    &Capability::ReadingRead.as_string(),
                )))
            }
            None => Err(ExtError::NotRunning),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_wire_is_camel_case_and_tolerant() {
        let e = ReadingEntry {
            subscription_id: "s1".into(),
            remote_id: "r1".into(),
            origin: "https://blyg.example.com".into(),
            kind: "fragment".into(),
            state: "current".into(),
            version: 2,
            title: String::new(),
            content_html: "<p>Hi</p>".into(),
            content_md: "Hi".into(),
            content_hash: Some(blyg_core::content_hash("Hi")),
        };
        let v = serde_json::to_value(EntryBylineParams { entry: e.clone() }).unwrap();
        assert_eq!(v["entry"]["remoteId"], "r1");
        assert_eq!(v["entry"]["contentHtml"], "<p>Hi</p>");
        let b: EntryByline = serde_json::from_value(json!({"text": "1 min", "x": 1})).unwrap();
        assert_eq!(b.text, "1 min");
        let s: EntrySheet = serde_json::from_value(json!({"title": "inspect"})).unwrap();
        assert!(s.code.is_none() && s.fields.is_empty());
        let a: EntryActionParams =
            serde_json::from_value(json!({"action": "inspect", "entry": v["entry"]})).unwrap();
        assert_eq!(a.record, Value::Null);
    }

    #[test]
    fn a_marker_is_one_short_line_or_nothing() {
        let b = |t: &str| EntryByline {
            text: t.into(),
            tip: Some(" ".into()),
        };
        assert_eq!(b("  ").shown(), None);
        let s = b(" 4\nmin ").shown().unwrap();
        assert_eq!(s.text, "4 min");
        assert_eq!(s.tip, None, "a blank tip is none");
        let long = b(&"word ".repeat(20)).shown().unwrap();
        assert_eq!(long.text.chars().count(), BYLINE_MAX);
        assert!(long.text.ends_with('…'));
    }

    #[test]
    fn manifests_declare_slots_behind_reading_read() {
        use crate::manifest::Manifest;
        let head = "name = \"x\"\nversion = \"1\"\nprotocol = 1\ncommand = [\"x\"]\n";
        let m = Manifest::parse(
            &format!(
                "{head}capabilities = [\"reading.read\"]\nentry-byline = true\n\
                 [[entry-actions]]\nid = \"look\"\ntitle = \"look\"\nicon = \"◇\"\n"
            ),
            false,
        )
        .unwrap();
        assert!(m.entry_byline);
        assert_eq!(m.entry_actions[0].id, "look");
        assert_eq!(m.entry_actions[0].icon.as_deref(), Some("◇"));
        // camelCase spellings load too.
        let m = Manifest::parse(
            &format!("{head}capabilities = [\"reading.read\"]\nentryByline = true\n"),
            false,
        )
        .unwrap();
        assert!(m.entry_byline);
        let none = Manifest::parse(head, false).unwrap();
        assert!(!none.entry_byline && none.entry_actions.is_empty());
        let bad = |extra: &str| {
            Manifest::parse(&format!("{head}{extra}"), false)
                .unwrap_err()
                .0
        };
        assert!(bad("entry-byline = true\n").contains("reading.read"));
        assert!(
            bad(
                "capabilities = [\"reading.read\"]\n[[entry-actions]]\nid = \"a\"\ntitle = \" \"\n"
            )
            .contains("no title")
        );
        assert!(
            bad(
                "capabilities = [\"reading.read\"]\n[[entry-actions]]\nid = \"a\"\ntitle = \"A\"\n\
                 [[entry-actions]]\nid = \"a\"\ntitle = \"B\"\n"
            )
            .contains("twice")
        );
    }

    #[test]
    fn the_copy_button_names_the_language() {
        let mut s = EntrySheet::default();
        assert_eq!(s.copy_label(), "Copy");
        s.language = Some("json".into());
        assert_eq!(s.copy_label(), "Copy JSON");
        s.language = Some("toml".into());
        assert_eq!(s.copy_label(), "Copy toml");
    }
}
