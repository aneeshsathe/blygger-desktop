//! The runner's in-page half: one small library of DOM operations, called
//! with a JSON [`Op`] and answering a JSON [`Reply`]. It runs through
//! [`BrowserSurface::eval_json`](super::super::surface::BrowserSurface::eval_json)
//! (on macOS in WebKit's default client world: the page's own scripts can't
//! see or patch it). It only ever finds elements, focuses, clicks, reads
//! text, and puts the previewed text into one element.
//!
//! A selector is a `querySelectorAll` selector; with `text`, the first
//! match whose `innerText.trim()` (or `value`) equals it exactly.

use serde::{Deserialize, Serialize};

/// One operation on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    /// Is the element there, and what does it hold? Also: does
    /// `signed_out` match (the site's signed-out selector)?
    Find {
        sel: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signed_out: Option<String>,
    },
    /// Focus the element (and check what has the keyboard then).
    Focus {
        sel: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// `element.click()`.
    Click {
        sel: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Before an insert: refuse a credential field, focus the element and
    /// select what it holds (so the text replaces it).
    Prep { sel: String },
    /// What the element holds now (`value`, else `innerText`).
    Read { sel: String },
    /// Prep, then `document.execCommand("insertText", false, payload)`.
    Exec { sel: String, payload: String },
    /// Prep, then a synthetic `paste` ClipboardEvent carrying `payload`.
    Synth { sel: String, payload: String },
}

/// What an [`Op`] answers. Missing fields are their defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Reply {
    /// The element was found (and, for an action, the action ran).
    pub found: bool,
    /// The selector itself is invalid (`querySelectorAll` threw).
    pub bad: bool,
    /// What the element holds (trimmed), when found.
    pub text: String,
    /// The signed-out selector matched.
    pub signed_out: bool,
    /// Why the element (or what has the keyboard) won't take text: a
    /// password, email or sign-in field, or something that isn't a text box.
    pub refused: Option<String>,
    /// `location.href`.
    pub url: String,
}

impl Reply {
    /// The JSON a page answered (`None` for `"null"` or garbage: no answer).
    pub fn parse(json: &str) -> Option<Reply> {
        serde_json::from_str::<Option<Reply>>(json).ok().flatten()
    }
}

const OPEN: &str = "/*burrow-op*/";
const CLOSE: &str = "/*end-op*/";

/// The script that runs `op`: an expression evaluating to the reply's JSON.
pub fn call(op: &Op) -> String {
    // JSON is a JS expression; U+2028/2029 are escaped for older engines.
    let arg = serde_json::to_string(op)
        .unwrap_or_else(|_| "null".into())
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    format!("(function(){{{LIB}\nreturn JSON.stringify(run({OPEN}{arg}{CLOSE}));}})()")
}

/// The [`Op`] inside a script made by [`call`] (the test DOM answers it).
#[cfg(test)]
pub fn decode(js: &str) -> Option<Op> {
    let start = js.find(OPEN)? + OPEN.len();
    let end = js.rfind(CLOSE)?;
    serde_json::from_str(&js[start..end]).ok()
}

/// The in-page library. Plain ES2017, no globals left behind.
const LIB: &str = r#"
"use strict";
const CRED = ["current-password", "new-password", "one-time-code", "username"];
function pick(sel, text) {
  let all;
  try { all = document.querySelectorAll(sel); } catch (e) { return { bad: true }; }
  for (const el of all) {
    if (text == null || holds(el).trim() === text) return { el };
  }
  return {};
}
function holds(el) {
  if (!el) return "";
  if (el.tagName === "INPUT" || el.tagName === "TEXTAREA") return el.value || "";
  return el.innerText || el.textContent || "";
}
function credential(el) {
  if (!el || !el.getAttribute) return null;
  const type = (el.getAttribute("type") || "").toLowerCase();
  if (el.tagName === "INPUT" && (type === "password" || type === "email")) return "a " + type + " field";
  const ac = (el.getAttribute("autocomplete") || "").toLowerCase().split(/\s+/);
  if (ac.some(a => CRED.indexOf(a) >= 0)) return "a sign-in field";
  return null;
}
function editable(el) {
  if (el.tagName === "TEXTAREA") return true;
  if (el.tagName === "INPUT") {
    const t = (el.getAttribute("type") || "text").toLowerCase();
    return ["text", "search", "url", ""].indexOf(t) >= 0;
  }
  return el.isContentEditable === true;
}
function selectAll(el) {
  if (el.tagName === "INPUT" || el.tagName === "TEXTAREA") { el.select(); return; }
  const r = document.createRange();
  r.selectNodeContents(el);
  const s = window.getSelection();
  s.removeAllRanges();
  s.addRange(r);
}
function prep(el) {
  const why = credential(el) || (editable(el) ? null : "something that isn't a text box");
  if (why) return why;
  el.focus();
  const active = document.activeElement;
  const whyActive = active && active !== el && !el.contains(active) ? credential(active) : null;
  if (whyActive) return whyActive;
  selectAll(el);
  return null;
}
function run(op) {
  const out = { found: false, url: location.href };
  if (op.signedOut) {
    try { out.signedOut = document.querySelector(op.signedOut) !== null; } catch (e) {}
  }
  const p = pick(op.sel, op.text == null ? null : op.text);
  if (p.bad) { out.bad = true; return out; }
  const el = p.el;
  if (!el) return out;
  out.found = true;
  switch (op.op) {
    case "find": break;
    case "focus": {
      el.focus();
      const why = credential(document.activeElement);
      if (why) out.refused = why;
      break;
    }
    case "click": el.click(); break;
    case "prep": {
      const why = prep(el);
      if (why) out.refused = why;
      break;
    }
    case "read": break;
    case "exec": {
      const why = prep(el);
      if (why) { out.refused = why; break; }
      document.execCommand("insertText", false, op.payload);
      break;
    }
    case "synth": {
      const why = prep(el);
      if (why) { out.refused = why; break; }
      const dt = new DataTransfer();
      dt.setData("text/plain", op.payload);
      el.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }));
      break;
    }
  }
  out.text = holds(el).trim();
  return out;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_carries_its_op_and_decodes() {
        let op = Op::Exec {
            sel: "div[contenteditable='true']".into(),
            payload: "Tides */ \"quoted\"\u{2028}line".into(),
        };
        let js = call(&op);
        assert!(js.starts_with("(function(){"));
        assert!(js.contains("execCommand"));
        assert!(!js.contains('\u{2028}'));
        assert_eq!(decode(&js), Some(op));
    }

    /// The names the in-page library reads (`op.op`, `op.sel`,
    /// `op.signedOut`, `op.payload`).
    #[test]
    fn ops_use_the_names_the_page_reads() {
        let js = call(&Op::Find {
            sel: "a".into(),
            text: Some("Post".into()),
            signed_out: Some("a.sign-in".into()),
        });
        assert!(
            js.contains(r#"{"op":"find","sel":"a","text":"Post","signedOut":"a.sign-in"}"#),
            "{js}"
        );
        assert!(LIB.contains("op.signedOut") && LIB.contains("op.payload"));
        let js = call(&Op::Synth {
            sel: "b".into(),
            payload: "x".into(),
        });
        assert!(js.contains(r#"{"op":"synth","sel":"b","payload":"x"}"#));
    }

    #[test]
    fn replies_parse_leniently() {
        assert_eq!(Reply::parse("null"), None);
        assert_eq!(Reply::parse("garbage"), None);
        let r = Reply::parse(r#"{"found":true,"text":"hi","url":"https://x.example/","extra":1}"#)
            .unwrap();
        assert!(r.found);
        assert_eq!(r.text, "hi");
        assert!(!r.signed_out);
        assert_eq!(r.refused, None);
    }
}
