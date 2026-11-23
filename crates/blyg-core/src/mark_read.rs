//! --- studio 0.71 --- When a reader entry counts as read: the blyg's
//! `readerMarkRead` setting (studio 0.70, contract 6.3.0; Studio's Settings ›
//! reader order, "Count an entry as read"). Its values are the studio's:
//! `seen` (about a second fully on screen while scrolling, the default),
//! `opened` (its full view: Burrow's behaviour before), `never` (marked by
//! hand only).
//!
//! It's a setting, not an operation, so its presence in `GET /api/settings`
//! is the gate: a studio that doesn't send it (before 0.70) doesn't offer
//! it, and Burrow neither invents a value nor sends one (an older studio
//! refuses an unknown key). Read and written alone, as `readerOrder` is
//! (`crate::order`).

use serde_json::{Value, json};

/// Store meta: the value as last read from or written to the blyg.
pub const MARK_READ_META: &str = "reader.mark_read";
/// Store meta: "1" when the blyg's settings have `readerMarkRead`, "0"
/// when they were read without it; absent until read.
pub const MARK_READ_SERVED_META: &str = "reader.mark_read.served";

/// `Settings.readerMarkRead`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarkRead {
    /// On screen about a second while scrolling (the contract's default).
    #[default]
    Seen,
    /// When its full view opens.
    Opened,
    /// Only when marked by hand.
    Never,
}

impl MarkRead {
    pub const ALL: [MarkRead; 3] = [MarkRead::Seen, MarkRead::Opened, MarkRead::Never];

    /// Its value in Settings.
    pub fn as_str(self) -> &'static str {
        match self {
            MarkRead::Seen => "seen",
            MarkRead::Opened => "opened",
            MarkRead::Never => "never",
        }
    }

    pub fn parse(s: &str) -> Option<MarkRead> {
        match s {
            "seen" => Some(MarkRead::Seen),
            "opened" => Some(MarkRead::Opened),
            "never" => Some(MarkRead::Never),
            _ => None,
        }
    }

    /// From `GET /api/settings`: `None` when the studio doesn't have it
    /// (before 0.70). A value it has but Burrow doesn't know reads as
    /// `opened`, the safer rule, not the contract's default: a newer rule
    /// may count less than `seen` does, and under `opened` nothing is
    /// marked read that the reader didn't open.
    pub fn from_settings(v: &Value) -> Option<MarkRead> {
        let m = v.get("readerMarkRead")?;
        Some(
            m.as_str()
                .and_then(MarkRead::parse)
                .unwrap_or(MarkRead::Opened),
        )
    }

    /// The partial `PATCH /api/settings` body: this key and no other.
    pub fn patch(self) -> Value {
        json!({ "readerMarkRead": self.as_str() })
    }

    /// The choice as Studio words it ("Count an entry as read …").
    pub fn label(self) -> &'static str {
        match self {
            MarkRead::Seen => "when I see it",
            MarkRead::Opened => "when I open it",
            MarkRead::Never => "only when I mark it",
        }
    }
}
