//! Putting the previewed text into the composer, without OS-level synthetic
//! keystrokes. Each way is checked by reading the box back:
//!
//! 1. the text on the clipboard, the web view given the keyboard, and the
//!    Edit menu's `paste:` sent to it: a trusted paste, which editors like
//!    ProseMirror take (only where the surface has edit commands);
//! 2. `document.execCommand("insertText")`;
//! 3. a synthetic `paste` ClipboardEvent;
//! 4. by hand: the text is left on the clipboard, and the user pastes it.
//!
//! A password, email or sign-in field is refused before anything is typed.

use std::time::Duration;

use super::js::{Op, Reply};
use super::runner::{Page, Reason};

/// How long a paste gets to land before the box is read back.
pub const SETTLE: Duration = Duration::from_millis(250);

/// How the text went in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Paste,
    InsertText,
    SyntheticPaste,
    /// Nothing worked: the text is on the clipboard for the user to paste.
    Manual,
}

impl Via {
    pub fn name(self) -> &'static str {
        match self {
            Via::Paste => "paste",
            Via::InsertText => "insertText",
            Via::SyntheticPaste => "synthetic-paste",
            Via::Manual => "manual",
        }
    }
}

/// Text compared the way a composer shows it: whitespace runs (newlines,
/// non-breaking spaces) are one space, ends trimmed.
pub fn normalize(s: &str) -> String {
    s.split(|c: char| c.is_whitespace() || c == '\u{a0}')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The box holds the text (as far as a reader can tell).
pub fn same_text(read_back: &str, payload: &str) -> bool {
    normalize(read_back) == normalize(payload)
}

/// Put `payload` into `sel`. `Ok` says how; `Err` is a refusal or the box
/// vanishing. The caller saved the user's clipboard before this and
/// restores it afterwards (except after [`Via::Manual`]).
pub async fn insert<P: Page>(page: &mut P, sel: &str, payload: &str) -> Result<Via, Reason> {
    // The box gone before anything was typed is Missing; after, the
    // insert failed.
    let check = |r: Option<Reply>, gone: Reason| -> Result<Reply, Reason> {
        let r = r.ok_or(gone.clone())?;
        if let Some(why) = r.refused.clone() {
            return Err(Reason::Refused(why));
        }
        if !r.found {
            return Err(gone);
        }
        Ok(r)
    };
    // The refusal check (and focus, select-all) comes first, whatever way.
    check(
        page.eval(&Op::Prep { sel: sel.into() }).await,
        Reason::Missing,
    )?;
    if page.has_edit_commands() {
        page.set_clipboard(payload);
        page.focus_page();
        page.paste();
        page.sleep(SETTLE).await;
        if let Some(r) = page.eval(&Op::Read { sel: sel.into() }).await
            && r.found
            && same_text(&r.text, payload)
        {
            return Ok(Via::Paste);
        }
    }
    let r = check(
        page.eval(&Op::Exec {
            sel: sel.into(),
            payload: payload.into(),
        })
        .await,
        Reason::InsertFailed,
    )?;
    // Read again once the page's own handlers (an editor re-rendering from
    // its model) have run.
    page.sleep(SETTLE).await;
    let r = match page.eval(&Op::Read { sel: sel.into() }).await {
        Some(back) if back.found => back,
        _ => r,
    };
    if same_text(&r.text, payload) {
        return Ok(Via::InsertText);
    }
    let r = check(
        page.eval(&Op::Synth {
            sel: sel.into(),
            payload: payload.into(),
        })
        .await,
        Reason::InsertFailed,
    )?;
    page.sleep(SETTLE).await;
    let r = match page.eval(&Op::Read { sel: sel.into() }).await {
        Some(back) if back.found => back,
        _ => r,
    };
    if same_text(&r.text, payload) {
        return Ok(Via::SyntheticPaste);
    }
    page.set_clipboard(payload);
    Ok(Via::Manual)
}
