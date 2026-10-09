//! The runner's in-page half: one small library of DOM operations, called
//! with a JSON [`Op`] and answering a JSON [`Reply`]. It runs through
//! [`BrowserSurface::eval_json`](super::super::surface::BrowserSurface::eval_json)
//! (on macOS in WebKit's default client world: the page's own scripts can't
//! see or patch it). It only ever finds elements, focuses, clicks, reads
//! text, and puts the previewed text into one element.
//!
//! A selector is a `querySelectorAll` selector; a list (`a, b`) is tried
//! part by part, the first part with a match winning. Of the matches, the
//! first visible one (else the first). With `text`, only matches whose
//! `innerText` (or `value`) equals it (whitespace collapsed, curly quotes
//! folded), the innermost of a nested run (a card's label, not the whole
//! card); failing that, one whose placeholder or `aria-label` equals it.
//! A click is pointer and mouse down/up, then `click()`, all bubbling.

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
    /// A privacy-limited outline of the page's text boxes, dialogs and
    /// buttons ([`Outline`]), for `macro.log` when a selector misses.
    Outline,
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
    /// `document.readyState` (`loading`, `interactive`, `complete`).
    pub ready: String,
    /// The element has a box and isn't `visibility: hidden`.
    pub visible: bool,
    /// The element (or the button it's in) is `disabled` or
    /// `aria-disabled="true"`: a click wouldn't reach the site.
    pub disabled: bool,
    /// [`Op::Outline`]'s answer.
    pub outline: Option<Outline>,
}

/// What [`Op::Outline`] sees: the page's path (no query), the title's
/// length (never its text), and up to [`OUTLINE_MAX`] candidate elements.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Outline {
    pub path: String,
    pub title_len: usize,
    /// How many candidates the page has (only the first are listed).
    pub total: usize,
    pub els: Vec<OutlineEl>,
}

/// One candidate: its tag and attributes, never its text, except a
/// button's or link's label (at most 30 characters).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OutlineEl {
    pub tag: String,
    pub id: String,
    pub cls: Vec<String>,
    pub role: String,
    /// `aria-label`.
    pub aria: String,
    /// `placeholder`, else `data-placeholder`.
    pub ph: String,
    /// `data-testid`.
    pub testid: String,
    pub name: String,
    /// An `<input>`'s `type`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The `contenteditable` attribute.
    pub ce: String,
    pub vis: bool,
    pub disabled: bool,
    /// A button's or link's `innerText`, cut to 30 characters.
    pub label: String,
}

/// The most candidates an outline lists (the in-page library's `25`).
pub const OUTLINE_MAX: usize = 25;

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
// Quotes folded, whitespace collapsed: "What’s on your mind?" matches
// "What's on your mind?".
function norm(s) {
  return String(s == null ? "" : s).replace(/[‘’]/g, "'")
    .replace(/[“”]/g, '"').replace(/\s+/g, " ").trim();
}
function shown(el) {
  if (!el.getClientRects || el.getClientRects().length === 0) return false;
  const st = getComputedStyle(el);
  return st.visibility !== "hidden" && st.display !== "none";
}
function disabled(el) {
  const b = el.closest ? el.closest("button,[role=button]") : null;
  for (const e of [el, b]) {
    if (e && (e.disabled === true || e.getAttribute("aria-disabled") === "true")) return true;
  }
  return false;
}
// A selector list's top-level parts, in order (commas inside quotes,
// brackets or parentheses don't split).
function parts(sel) {
  const out = [];
  let depth = 0, quote = null, cur = "";
  for (const c of sel) {
    if (quote) { if (c === quote) quote = null; cur += c; continue; }
    if (c === "'" || c === '"') quote = c;
    else if (c === "(" || c === "[") depth++;
    else if (c === ")" || c === "]") depth--;
    else if (c === "," && depth === 0) { out.push(cur); cur = ""; continue; }
    cur += c;
  }
  out.push(cur);
  return out.map(p => p.trim()).filter(p => p);
}
// The first visible candidate (else the first); with text, the innermost
// of a nested run of matches (a card's text, not the whole card).
function best(cands, text) {
  const vis = cands.filter(shown);
  const list = vis.length ? vis : cands;
  let b = list[0];
  if (text) for (let i = 1; i < list.length && b.contains(list[i]); i++) b = list[i];
  return b;
}
const PH = ["data-placeholder", "placeholder", "aria-label"];
// A selector list's parts in order (the first part with a match wins);
// with text, elements whose innerText (or value) is it, else ones whose
// placeholder or aria-label is it.
function pick(sel, text) {
  try { document.querySelectorAll(sel); } catch (e) { return { bad: true }; }
  const want = text == null ? null : norm(text);
  const tests = want == null ? [() => true] : [
    el => (el.textContent || "").length <= want.length * 4 + 200 && norm(holds(el)) === want,
    el => PH.some(a => norm(el.getAttribute(a)) === want),
  ];
  for (const t of tests) {
    for (const p of parts(sel)) {
      let all;
      try { all = document.querySelectorAll(p); } catch (e) { continue; }
      const cands = Array.prototype.filter.call(all, t);
      if (cands.length) return { el: best(cands, want != null) };
    }
  }
  return {};
}
function holds(el) {
  if (!el) return "";
  if (el.tagName === "INPUT" || el.tagName === "TEXTAREA") return el.value || "";
  return el.innerText || el.textContent || "";
}
// What a page's handler sees from a real click: pointer and mouse down/up
// at the element's centre, then click. They bubble, so a handler on the
// closest clickable ancestor (or a framework's root listener) gets them.
function press(el) {
  const r = el.getBoundingClientRect();
  const base = { bubbles: true, cancelable: true, composed: true, view: window,
    clientX: r.left + r.width / 2, clientY: r.top + r.height / 2, button: 0 };
  const ptr = Object.assign({ pointerId: 1, pointerType: "mouse", isPrimary: true }, base);
  const P = typeof PointerEvent === "function" ? PointerEvent : null;
  if (P) el.dispatchEvent(new P("pointerdown", Object.assign({ buttons: 1 }, ptr)));
  el.dispatchEvent(new MouseEvent("mousedown", Object.assign({ buttons: 1 }, base)));
  if (P) el.dispatchEvent(new P("pointerup", Object.assign({ buttons: 0 }, ptr)));
  el.dispatchEvent(new MouseEvent("mouseup", Object.assign({ buttons: 0 }, base)));
  el.click();
}
// The candidates when a selector misses. Never text but a button's label
// (30 characters), never a value.
const OUTLINE = "[contenteditable],textarea,input:not([type=hidden]),[role=textbox],[role=dialog],button,[role=button]";
function cut(v, n) {
  const t = norm(v);
  return t.length > n ? t.slice(0, n) + "…" : t;
}
function outline() {
  const all = Array.from(document.querySelectorAll(OUTLINE));
  const seen = all.map(el => [el, shown(el)]);
  const order = seen.filter(x => x[1]).concat(seen.filter(x => !x[1])).slice(0, 25);
  const els = order.map(([el, vis]) => {
    const tag = el.tagName.toLowerCase();
    const role = el.getAttribute("role") || "";
    const at = a => cut(el.getAttribute(a), 40);
    const o = {
      tag, vis, role: cut(role, 20), id: cut(el.id, 40),
      cls: Array.from(el.classList || []).slice(0, 4).map(c => cut(c, 40)),
      aria: at("aria-label"), ph: at("placeholder") || at("data-placeholder"),
      testid: at("data-testid"), name: at("name"), ce: at("contenteditable"),
      disabled: disabled(el),
    };
    if (tag === "input") o.type = at("type");
    if (tag === "button" || tag === "a" || role === "button") o.label = cut(el.innerText, 30);
    return o;
  });
  return { path: location.pathname, titleLen: (document.title || "").length, total: all.length, els };
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
  const out = { found: false, url: location.href, ready: document.readyState };
  if (op.op === "outline") { out.outline = outline(); return out; }
  if (op.signedOut) {
    try { out.signedOut = document.querySelector(op.signedOut) !== null; } catch (e) {}
  }
  const p = pick(op.sel, op.text == null ? null : op.text);
  if (p.bad) { out.bad = true; return out; }
  const el = p.el;
  if (!el) return out;
  out.found = true;
  out.visible = shown(el);
  out.disabled = disabled(el);
  switch (op.op) {
    case "find": break;
    case "focus": {
      el.focus();
      const why = credential(document.activeElement);
      if (why) out.refused = why;
      break;
    }
    case "click": press(el); break;
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
