//! --- web-writing --- The writing and blyg side of Studio, in the web app:
//! images, your own versions (history, view, word diff, restore, pin),
//! withdraw, forking a pinned version, TK generation on the server,
//! AI-drafted change notes and the change classifier in the publish sheet,
//! the blyg's basic settings, and `@handle` contacts (the `@` list, and the
//! preview's h-card link, as published).
//!
//! As everywhere in this crate, the rules are here and the page draws: each
//! action asks for the request to make (`{req: {method, path, body}}`,
//! always a same-origin `/api/…` path), the page's `live.js` `call()` makes
//! it, and the answer comes back here to be read. Nothing here runs on the
//! per-keystroke path except the preview's `@handle` and upload rewrites,
//! which cost nothing for a post without them.
//!
//! The rules reused from the desktop app: `vm::word_diff`, `vm::discard`,
//! `vm::absolute_url`; the reading view-model's own-version rows and actions
//! (`own_rows`, `own_actions`, `pin_confirmed`, `parse_links`); the
//! composer's `@` list (`crate::composer::mention`); blyg-core's contact
//! warnings. Studio's (ported, as nothing compiles from TypeScript): its
//! change-note composition (`change-category.ts`) and its `@handle`
//! rewrite (`contacts.ts`).

use std::collections::{HashMap, HashSet};

use blyg_core::api::wire::{Media, WireItem};
use blyg_core::{Item, Kind, LocalId, Status, Version};
use serde::Serialize;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::api::{self, ApiError, Request, enc};
use crate::composer::mention;
use crate::reading::vm as rvm;
use crate::studio::resolver::render_kind;
use crate::{BUILTIN_CSS, Burrow, Outcome, at, json, page_shell, rfc, vm};

/// The blyg's upload cap (Studio's `MEDIA_MAX_BYTES`).
pub const MEDIA_MAX: f64 = 5.0 * 1024.0 * 1024.0;

/// What Studio takes (`MEDIA_TYPES`).
const MEDIA_TYPES: [&str; 5] = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/svg+xml",
];

/// The change categories, smallest first (Studio's `CHANGE_CATEGORIES`).
pub const CATEGORIES: [&str; 4] = ["refresh", "typo", "minor", "major"];

/// What this side keeps, beside the store. None of it is persisted: the
/// settings and contacts are read again when the page starts.
#[derive(Default)]
pub struct Writing {
    /// `GET /api/settings`, as last read or saved (the demo: its own).
    settings: Option<Value>,
    /// The address book (`GET /api/contacts`).
    contacts: Vec<Card>,
    contacts_read: bool,
    /// Uploads made in this page: media URL → a small `data:` thumbnail.
    /// The preview runs in a sandboxed frame, which the blyg's cookie
    /// doesn't reach, and an upload no published version shows yet is
    /// served only to the owner.
    uploads: HashMap<String, String>,
    next_upload: u32,
    mention: Option<MentionList>,
    note: Option<NoteSession>,
    /// Your versions as last read: local id → oldest first.
    versions: HashMap<String, Vec<Version>>,
    /// Versions' text as read (`GET …/versions/{v}`): (local id, v) → md.
    version_md: HashMap<(String, u32), String>,
    /// The demo's pins (it has no blyg to keep them).
    demo_pins: HashSet<(String, u32)>,
}

/// One contact, as the h-card needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub handle: String,
    pub name: String,
    pub url: String,
    /// Identifiers that go on the wire (not `entered` ones).
    pub ids: Vec<String>,
}

struct MentionList {
    /// Byte offset of the `@` in the editor's text.
    at: usize,
    cands: Vec<mention::Candidate>,
    shown: Vec<mention::Candidate>,
}

/// The publish sheet's note, as Studio's note confirmation keeps it.
#[derive(Debug, Default, Clone)]
pub struct NoteSession {
    /// The note as you first gave it.
    pub typed: String,
    /// What the model drafted, if it did.
    pub drafted: Option<String>,
    /// What the classifier said.
    pub model: Option<String>,
    /// The category chosen now.
    pub category: Option<String>,
    /// Whether the classifier answered (or failed): a category goes with
    /// the publish only then.
    pub classified: bool,
    /// The category leads the note's text (`changeCategoryInNote`).
    pub in_note: bool,
}

// ---------------------------------------------------------------- pure rules

/// UTF-16 offset (a textarea's) → byte offset in `s`.
pub fn byte_of(s: &str, u16_at: u32) -> usize {
    let mut n = 0u32;
    for (b, c) in s.char_indices() {
        if n >= u16_at {
            return b;
        }
        n += c.len_utf16() as u32;
    }
    s.len()
}

/// Byte offset in `s` → UTF-16 offset.
pub fn u16_of(s: &str, byte: usize) -> u32 {
    s[..byte.min(s.len())]
        .chars()
        .map(|c| c.len_utf16() as u32)
        .sum()
}

fn is_category(s: &str) -> bool {
    CATEGORIES.contains(&s)
}

/// The category word a note starts with ("Typo fix." → typo).
pub fn leading_category(note: &str) -> Option<&'static str> {
    let t = note.trim_start();
    CATEGORIES.into_iter().find(|c| {
        t.len() >= c.len()
            && t[..c.len()].eq_ignore_ascii_case(c)
            && !t[c.len()..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
    })
}

/// The note without a prefix `compose_note` would have written.
pub fn strip_category(note: &str) -> String {
    let t = note.trim_start();
    if let Some(c) = leading_category(t) {
        let rest = t[c.len()..].trim_start();
        if rest.is_empty() {
            return String::new();
        }
        if let Some(r) = rest.strip_prefix(':') {
            return r.trim().to_string();
        }
    }
    note.trim().to_string()
}

/// Studio's `composeNote`: an empty note becomes the bare word, a note
/// becomes `minor: note`, and a note that already starts with a category
/// word is left alone.
pub fn compose_note(note: &str, category: Option<&str>) -> String {
    let text = note.trim();
    let Some(c) = category.filter(|c| is_category(c)) else {
        return text.to_string();
    };
    if text.is_empty() {
        return c.to_string();
    }
    if leading_category(text).is_some() {
        return text.to_string();
    }
    format!("{c}: {text}")
}

/// Studio's `recategorize`.
pub fn recategorize(note: &str, category: Option<&str>) -> String {
    compose_note(&strip_category(note), category)
}

impl NoteSession {
    /// The text the machine produced, if the note began as a machine's.
    fn machine(&self) -> Option<String> {
        let cat = if self.in_note {
            self.model.as_deref()
        } else {
            None
        };
        if let Some(d) = &self.drafted {
            return Some(compose_note(d, cat));
        }
        (self.typed.trim().is_empty())
            .then_some(cat)
            .flatten()
            .map(str::to_string)
    }

    /// Studio's `noteIsGenerated`: exactly the machine's text, unedited.
    pub fn generated(&self, text: &str) -> bool {
        let t = text.trim();
        self.machine()
            .is_some_and(|m| !t.is_empty() && t == m.trim())
    }
}

/// Where `@handle`s are in `md` (Studio's `handleSpots`): not after a word
/// or URL character, not inside code, links, autolinks or quoted lines.
/// (byte start, byte end, handle).
pub fn handle_spots(md: &str) -> Vec<(usize, usize, String)> {
    if !md.contains('@') {
        return vec![];
    }
    let mut skip: Vec<(usize, usize)> = blyg_render::code_ranges(md);
    skip.extend(opaque_ranges(md));
    let mut line_start = 0;
    for line in md.split_inclusive('\n') {
        let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
        if indent <= 3 && line[indent..].starts_with('>') {
            skip.push((line_start, line_start + line.trim_end_matches('\n').len()));
        }
        line_start += line.len();
    }
    let inside = |p: usize| skip.iter().any(|&(s, e)| p >= s && p < e);
    let b = md.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let head = |c: u8| c.is_ascii_alphanumeric();
    let tail = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'-';
    let mut out = Vec::new();
    for (i, _) in md.match_indices('@') {
        if inside(i) {
            continue;
        }
        // Not after a word character or one of `@./:+-` (emails, paths).
        if let Some(prev) = md[..i].chars().next_back()
            && (prev.is_alphanumeric() || prev == '_' || "@./:+-".contains(prev))
        {
            continue;
        }
        let start = i + 1;
        if !b.get(start).copied().is_some_and(head) {
            continue;
        }
        let mut end = start + 1;
        while end < b.len() && end - start < 39 && tail(b[end]) {
            end += 1;
        }
        // The regex backtracks: the longest run whose next character isn't
        // a word character or `@`, nor `.`/`-` before a word character.
        let ok = |e: usize| match b.get(e) {
            None => true,
            Some(&c) if word(c) || c == b'@' => false,
            Some(&c) if c == b'.' || c == b'-' => !b.get(e + 1).copied().is_some_and(word),
            Some(&c) => c < 0x80 || !md[e..].chars().next().is_some_and(char::is_alphanumeric),
        };
        let mut e = end;
        while e > start && !ok(e) {
            e -= 1;
        }
        if e > start {
            out.push((i, e, md[start..e].to_string()));
        }
    }
    out
}

/// Bracket directives and links, Markdown links and images, autolinks.
fn opaque_ranges(md: &str) -> Vec<(usize, usize)> {
    let b = md.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let j = if b[i] == b'!' { i + 1 } else { i };
        if b.get(j) == Some(&b'[') && b.get(j + 1) == Some(&b'[') {
            if let Some(n) = md[j + 2..].find(['\n', ']'])
                && md[j + 2 + n..].starts_with("]]")
            {
                out.push((start, j + 2 + n + 2));
                i = j + 2 + n + 2;
                continue;
            }
        } else if b.get(j) == Some(&b'[') {
            if let Some(n) = md[j + 1..].find(['\n', ']'])
                && md[j + 1 + n..].starts_with("](")
            {
                let k = j + 1 + n + 2;
                if let Some(m) = md[k..].find(['\n', ')'])
                    && md[k + m..].starts_with(')')
                {
                    out.push((start, k + m + 1));
                    i = k + m + 1;
                    continue;
                }
            }
        } else if b[i] == b'<'
            && let Some(n) = md[i + 1..].find(['\n', '>'])
            && md[i + 1 + n..].starts_with('>')
        {
            out.push((i, i + 1 + n + 1));
            i = i + 1 + n + 1;
            continue;
        }
        i += md[i..].chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// The link `content_md` carries for a contact (Studio's `mentionLink`).
pub fn mention_link(name: &str, url: &str) -> String {
    let mut n = String::new();
    for c in name.chars() {
        if "\\[]*_`<".contains(c) {
            n.push('\\');
        }
        n.push(c);
    }
    let u = url
        .replace('(', "%28")
        .replace(')', "%29")
        .replace(char::is_whitespace, "%20");
    format!("[@{n}]({u})")
}

/// The markup `content_html` carries for a contact (Studio's
/// `mentionCard`, tn-6 §5.3).
pub fn mention_card(c: &Card) -> String {
    let href = blyg_render::escape_html(&mention_href(&c.url));
    let data: String = c
        .ids
        .iter()
        .map(|id| {
            format!(
                "<data class=\"u-url\" value=\"{}\"></data>",
                blyg_render::escape_html(id)
            )
        })
        .collect();
    format!(
        "<span class=\"h-card\"><a class=\"u-url\" href=\"{href}\">@<span class=\"p-name\">{}</span></a>{data}</span>",
        blyg_render::escape_html(&c.name)
    )
}

fn mention_href(url: &str) -> String {
    url.replace('(', "%28")
        .replace(')', "%29")
        .replace(char::is_whitespace, "%20")
}

/// Studio's `consumeHandles`: each known `@handle` becomes its contact's
/// link. Returns the new text and the contacts used, or `None` when
/// nothing changed.
pub fn consume_handles<'a>(cards: &'a [Card], md: &str) -> Option<(String, Vec<&'a Card>)> {
    if cards.is_empty() {
        return None;
    }
    let spots = handle_spots(md);
    if spots.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(md.len() + 64);
    let mut used: Vec<&Card> = Vec::new();
    let mut last = 0;
    for (s, e, h) in spots {
        let Some(c) = cards.iter().find(|c| c.handle.eq_ignore_ascii_case(&h)) else {
            continue;
        };
        out.push_str(&md[last..s]);
        out.push_str(&mention_link(&c.name, &c.url));
        last = e;
        if !used.contains(&c) {
            used.push(c);
        }
    }
    if used.is_empty() {
        return None;
    }
    out.push_str(&md[last..]);
    Some((out, used))
}

/// After rendering: each contact's plain link becomes its h-card.
pub fn apply_cards(html: &str, used: &[&Card]) -> String {
    let mut html = html.to_string();
    for c in used {
        let href = blyg_render::escape_html(&mention_href(&c.url));
        let open = format!("<a href=\"{href}\"");
        let text = format!(">@{}</a>", blyg_render::escape_html(&c.name));
        let mut from = 0;
        while let Some(i) = html[from..].find(&open).map(|i| i + from) {
            let Some(gt) = html[i..].find('>').map(|g| g + i) else {
                break;
            };
            if html[gt..].starts_with(&text) {
                let card = mention_card(c);
                html.replace_range(i..gt + text.len(), &card);
                from = i + card.len();
            } else {
                from = gt;
            }
        }
    }
    html
}

/// `GET /api/contacts` (`{contacts: […]}`, `{items: […]}` or a bare array).
pub fn parse_cards(v: &Value) -> Vec<Card> {
    let list = v
        .get("contacts")
        .or_else(|| v.get("items"))
        .unwrap_or(v)
        .as_array()
        .cloned()
        .unwrap_or_default();
    list.iter()
        .filter_map(|c| {
            let s = |k: &str| {
                c.get(k)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            };
            let handle = s("handle")?.trim_start_matches('@').to_string();
            let url = s("url").or_else(|| s("origin"))?;
            let ids = c
                .get("ids")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter(|i| i.get("provenance").and_then(Value::as_str) != Some("entered"))
                        .filter_map(|i| i.get("id").and_then(Value::as_str).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            Some(Card {
                name: s("name").unwrap_or_else(|| handle.clone()),
                handle,
                url,
                ids,
            })
        })
        .collect()
}

/// What to do with a file before uploading it: as it is, re-encoded (a
/// photo too big, or a type the blyg doesn't take), or not at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "plan", rename_all = "snake_case")]
pub enum ImagePlan {
    AsIs,
    Reencode,
    Refuse { why: String },
}

pub fn image_plan(mime: &str, size: f64) -> ImagePlan {
    let mime = mime.trim().to_ascii_lowercase();
    if !mime.starts_with("image/") {
        return ImagePlan::Refuse {
            why: "That isn't an image".into(),
        };
    }
    let ok_type = MEDIA_TYPES.contains(&mime.as_str());
    let vector_or_anim = mime == "image/gif" || mime == "image/svg+xml";
    if ok_type && size <= MEDIA_MAX {
        return ImagePlan::AsIs;
    }
    if vector_or_anim {
        return ImagePlan::Refuse {
            why: "Too big: the blyg takes images up to 5 MB".into(),
        };
    }
    ImagePlan::Reencode
}

/// `![alt](url)`, the alt escaped as link text.
pub fn image_markdown(url: &str, alt: &str) -> String {
    let alt = alt.split_whitespace().collect::<Vec<_>>().join(" ");
    format!(
        "![{}]({})",
        vm::escape_link_text(&alt),
        vm::link_destination(url)
    )
}

/// The placeholder for upload `n` while it's on its way.
pub fn placeholder(n: u32) -> String {
    format!("![uploading {n}…]()")
}

/// A word diff of `old` against `new`, as HTML: removed words in `<del>`,
/// added ones in `<ins>`, the rest as it is (`vm::word_diff`).
pub fn diff_html(old: &str, new: &str) -> String {
    let (_, runs) = vm::word_diff(old, new);
    let mut out = String::new();
    for (op, t) in runs {
        let t = blyg_render::escape_html(&t);
        match op {
            vm::DiffOp::Same => out.push_str(&t),
            vm::DiffOp::Insert => {
                out.push_str("<ins>");
                out.push_str(&t);
                out.push_str("</ins>");
            }
            vm::DiffOp::Delete => {
                out.push_str("<del>");
                out.push_str(&t);
                out.push_str("</del>");
            }
        }
    }
    out
}

/// One version as `GET /api/items/{id}/versions[/{v}]` sends it.
fn version_of(v: &Value) -> Option<Version> {
    Some(Version {
        version: v.get("version").and_then(Value::as_u64)? as u32,
        published_at: v
            .get("publishedAt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        note: v.get("note").and_then(Value::as_str).map(str::to_string),
        pinned: v.get("pinned").and_then(Value::as_bool).unwrap_or(false),
        endcap: v.get("kind").and_then(Value::as_str) == Some("withdrawn"),
    })
}

fn err(text: impl Into<String>) -> String {
    json(&json!({ "error": { "text": text.into() } }))
}

fn api_err(e: &ApiError) -> String {
    json(&json!({ "error": e.to_json() }))
}

fn req(r: Request) -> String {
    json(&json!({ "req": r }))
}

fn post(path: String, body: Value) -> Request {
    Request {
        method: "POST",
        path,
        body: Some(body),
    }
}

/// The settings fields the Settings sheet edits, as text.
pub const TEXT_FIELDS: [&str; 6] = [
    "siteTitle",
    "authorName",
    "authorBio",
    "authorUrl",
    "siteUrl",
    "timezone",
];
const BOOL_FIELDS: [&str; 2] = ["showResponsesDefault", "highlightGeneratedDefault"];
pub const PICKER_TYPING: [&str; 3] = ["auto", "editor", "panel"];

/// The demo's settings (invented).
fn demo_settings() -> Value {
    json!({
        "siteTitle": "A garden notebook",
        "authorName": "Sam Example",
        "authorBio": "Notes on small gardens, bread and slow reading.",
        "authorUrl": "",
        "siteUrl": crate::seed::ORIGIN,
        "timezone": "",
        "authorLinks": [{ "label": "Elsewhere", "url": "https://example.org/sam" }],
        "pickerTyping": "auto",
        "showResponsesDefault": false,
        "highlightGeneratedDefault": false,
    })
}

/// What the Settings sheet sends: only what changed from `cur`, checked.
pub fn settings_patch(cur: &Value, form: &Value) -> Result<serde_json::Map<String, Value>, String> {
    let mut out = serde_json::Map::new();
    let s = |v: &Value, k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    for k in TEXT_FIELDS {
        let new = s(form, k);
        if matches!(k, "authorUrl" | "siteUrl") && !new.is_empty() {
            let ok = url::Url::parse(&new)
                .ok()
                .is_some_and(|u| matches!(u.scheme(), "http" | "https") && u.host().is_some());
            if !ok {
                let what = if k == "siteUrl" {
                    "Site URL"
                } else {
                    "Your address"
                };
                return Err(format!("{what}: write a full address, https://…"));
            }
        }
        if new != s(cur, k) {
            out.insert(k.into(), json!(new));
        }
    }
    for k in BOOL_FIELDS {
        let new = form.get(k).and_then(Value::as_bool).unwrap_or(false);
        if Some(new) != cur.get(k).and_then(Value::as_bool) {
            out.insert(k.into(), json!(new));
        }
    }
    let pt = s(form, "pickerTyping");
    if PICKER_TYPING.contains(&pt.as_str()) && pt != s(cur, "pickerTyping") {
        out.insert("pickerTyping".into(), json!(pt));
    }
    let links = rvm::parse_links(&s(form, "links")).map_err(|e| format!("Links, {e}"))?;
    let cur_links: Vec<(String, String)> = cur
        .get("authorLinks")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|l| (s(l, "label"), s(l, "url"))).collect())
        .unwrap_or_default();
    let new_links: Vec<(String, String)> = links
        .iter()
        .map(|l| (l.label.clone(), l.url.clone()))
        .collect();
    if new_links != cur_links {
        out.insert(
            "authorLinks".into(),
            json!(
                links
                    .iter()
                    .map(|l| json!({ "label": l.label, "url": l.url }))
                    .collect::<Vec<_>>()
            ),
        );
    }
    Ok(out)
}

// ---------------------------------------------------------------- the page's calls

#[derive(Serialize)]
struct VersionRow {
    version: u32,
    note: String,
    when: String,
    badge: &'static str,
    actions: Vec<ActionView>,
}

#[derive(Serialize)]
struct ActionView {
    act: &'static str,
    label: String,
    tip: String,
    on: bool,
}

impl Burrow {
    fn w_item(&self, id: &str) -> Option<Item> {
        self.store.item(&LocalId(id.into()))
    }

    fn w_rev(&self, id: &str) -> u64 {
        self.store.live.rev.get(id).copied().unwrap_or(0)
    }

    /// The item's id on the blyg, once its edits are there.
    fn w_synced(&self, id: &str) -> Result<(Item, String), String> {
        let Some(it) = self.w_item(id) else {
            return Err("That post isn't here any more".into());
        };
        if it.conflict {
            return Err("Choose which copy to keep first".into());
        }
        match (&it.server_id, it.pending_sync) {
            (Some(s), false) => {
                let s = s.0.clone();
                Ok((it, s))
            }
            _ => Err("Your last edits haven't reached the blyg yet. Try again in a moment.".into()),
        }
    }

    fn w_settings(&self) -> Value {
        match &self.writing.settings {
            Some(s) => s.clone(),
            None if !self.is_live() => demo_settings(),
            None => json!({}),
        }
    }

    fn w_setting_str(&self, k: &str) -> String {
        self.writing
            .settings
            .as_ref()
            .and_then(|s| s.get(k))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    fn w_setting_bool(&self, k: &str) -> Option<bool> {
        self.writing
            .settings
            .as_ref()
            .and_then(|s| s.get(k))
            .and_then(Value::as_bool)
    }

    /// The preview's rewrites: `@handle` → the contact's link before
    /// rendering. Called by `Burrow::render`.
    pub(crate) fn w_before_render<'a>(&'a self, md: &str) -> Option<(String, Vec<&'a Card>)> {
        if md.contains('@') {
            consume_handles(&self.writing.contacts, md)
        } else {
            None
        }
    }

    /// After rendering: h-cards, and this page's uploads shown from their
    /// thumbnails.
    pub(crate) fn w_after_render(&self, html: &mut String, used: &[&Card]) {
        if !used.is_empty() {
            *html = apply_cards(html, used);
        }
        for (url, thumb) in &self.writing.uploads {
            if html.contains(url.as_str()) {
                let esc = blyg_render::escape_html(url);
                *html = html
                    .replace(&format!("src=\"{url}\""), &format!("src=\"{thumb}\""))
                    .replace(&format!("src=\"{esc}\""), &format!("src=\"{thumb}\""));
            }
        }
    }

    fn w_scheme(dark: bool) -> &'static str {
        if dark {
            ":root { --paper: #161513; --ink: #e4dfd3; --muted: #9a9489; --accent: #e0775a; --rule: #2f2d29; --wash: rgba(228,223,211,0.05); color-scheme: dark; }"
        } else {
            ":root { --paper: #fffff8; --ink: #111; --muted: #6b665d; --accent: #a4271b; --rule: #dcd6c9; --wash: rgba(17,17,17,0.04); color-scheme: light; }"
        }
    }

    fn w_version_list(&self, id: &str) -> Vec<Version> {
        if self.is_live() {
            return self.writing.versions.get(id).cloned().unwrap_or_default();
        }
        let lid = LocalId(id.into());
        let mut out: Vec<Version> = self
            .store
            .versions(&lid)
            .into_iter()
            .map(|p| Version {
                version: p.version,
                published_at: p.at,
                pinned: self
                    .writing
                    .demo_pins
                    .contains(&(id.to_string(), p.version)),
                note: p.note,
                endcap: false,
            })
            .collect();
        if let Some(it) = self.store.item(&lid)
            && it.status == Status::Withdrawn
            && !out.iter().any(|v| v.version == it.version)
        {
            out.push(Version {
                version: it.version,
                published_at: it.updated.clone(),
                note: None,
                pinned: false,
                endcap: true,
            });
        }
        out
    }

    fn w_version_md(&self, id: &str, v: u32) -> Option<String> {
        if self.is_live() {
            return self.writing.version_md.get(&(id.to_string(), v)).cloned();
        }
        self.store
            .versions(&LocalId(id.into()))
            .into_iter()
            .find(|p| p.version == v)
            .map(|p| p.content_md)
    }
}

#[wasm_bindgen]
impl Burrow {
    // ------------------------------------------------------------ settings

    /// `GET /api/settings` as read (on start, and after a save).
    pub fn w_set_settings(&mut self, body: &str) {
        if let Ok(v) = serde_json::from_str::<Value>(body)
            && v.is_object()
        {
            self.writing.settings = Some(v);
        }
    }

    /// What the Settings sheet shows.
    pub fn w_settings_form(&self) -> String {
        let s = self.w_settings();
        let mut f = serde_json::Map::new();
        for k in TEXT_FIELDS {
            f.insert(
                k.into(),
                json!(s.get(k).and_then(Value::as_str).unwrap_or_default()),
            );
        }
        for k in BOOL_FIELDS {
            f.insert(
                k.into(),
                json!(s.get(k).and_then(Value::as_bool).unwrap_or(false)),
            );
        }
        f.insert(
            "pickerTyping".into(),
            json!(
                s.get("pickerTyping")
                    .and_then(Value::as_str)
                    .unwrap_or("auto")
            ),
        );
        let links: Vec<blyg_core::AuthorLink> = s
            .get("authorLinks")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|l| {
                        Some(blyg_core::AuthorLink {
                            label: l.get("label")?.as_str()?.into(),
                            url: l.get("url")?.as_str()?.into(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        f.insert("links".into(), json!(rvm::format_links(&links)));
        json(&json!({
            "live": self.is_live(),
            "known": self.writing.settings.is_some() || !self.is_live(),
            "form": f,
        }))
    }

    /// The Settings sheet's Save: `{req}` (live), `{saved}` (the demo),
    /// `{nothing}` or `{error}`.
    pub fn w_settings_save(&mut self, form_json: &str) -> String {
        let Ok(form) = serde_json::from_str::<Value>(form_json) else {
            return err("That form doesn't read");
        };
        if self.is_live() && self.writing.settings.is_none() {
            return err("The blyg's settings haven't been read yet. Try again when it's back.");
        }
        let cur = self.w_settings();
        let patch = match settings_patch(&cur, &form) {
            Ok(p) => p,
            Err(e) => return err(e),
        };
        if patch.is_empty() {
            return json(&json!({ "nothing": "Nothing changed" }));
        }
        if !self.is_live() {
            let mut s = cur;
            for (k, v) in patch {
                s[k] = v;
            }
            self.writing.settings = Some(s);
            return json(&json!({ "saved": "Saved (the demo keeps it in this page only)" }));
        }
        if self.offline {
            return err("Saving settings needs a connection");
        }
        req(Request {
            method: "PATCH",
            path: "/api/settings".into(),
            body: Some(Value::Object(patch)),
        })
    }

    /// The blyg's answer to the save: the stored settings.
    pub fn w_settings_answer(&mut self, status: u16, body: &str, origin: &str) -> String {
        match api::read(status, body, None) {
            Ok(v) if v.is_object() => {
                let before = self.w_setting_str("siteUrl");
                self.writing.settings = Some(v);
                if self.w_setting_str("siteUrl") != before {
                    self.store.live.base = Burrow::public_base(body, origin);
                }
                json(&json!({ "saved": "Settings saved" }))
            }
            Ok(_) => err("The blyg answered without its settings"),
            Err(e) => api_err(&e),
        }
    }

    // ------------------------------------------------------------ contacts and @

    pub fn w_contacts_read(&self) -> bool {
        self.writing.contacts_read
    }

    /// `GET /api/contacts`. A blyg without an address book (404) has none.
    pub fn w_set_contacts(&mut self, status: u16, body: &str) -> usize {
        match api::read(status, body, None) {
            Ok(v) => {
                self.writing.contacts = parse_cards(&v);
                self.writing.contacts_read = true;
            }
            Err(ApiError::NotFound) => {
                self.writing.contacts.clear();
                self.writing.contacts_read = true;
            }
            Err(_) => {}
        }
        self.writing.contacts.len()
    }

    /// Typing in the editor: whether the `@` list is open, and its rows.
    /// `old` is the text before this input, `caret` the caret after it
    /// (UTF-16, as the textarea counts). `null`: closed.
    pub fn w_mention(&mut self, old: &str, new: &str, caret: u32, now_ms: f64) -> String {
        let cur = byte_of(new, caret);
        if let Some(at_byte) = mention::trigger(old, new, cur) {
            let contacts: Vec<blyg_core::contacts::Contact> = self
                .writing
                .contacts
                .iter()
                .map(|c| blyg_core::contacts::Contact {
                    handle: c.handle.clone(),
                    name: Some(c.name.clone()),
                    origin: c.url.clone(),
                })
                .collect();
            let items = self.store.items();
            let own = self.origin_str();
            let src = mention::Sources {
                subscriptions: &self.rd.subs,
                reading: &self.rd.rows,
                profiles: &[],
                items: &items,
                own_origin: Some(&own),
                contacts: &contacts,
                rewrites_handles: !contacts.is_empty(),
            };
            let cands = mention::gather(&src, at(now_ms));
            self.writing.mention = Some(MentionList {
                at: at_byte,
                cands,
                shown: vec![],
            });
        }
        let Some(m) = self.writing.mention.as_mut() else {
            return "null".into();
        };
        let Some(q) = mention::query(new, m.at, cur) else {
            self.writing.mention = None;
            return "null".into();
        };
        m.shown = mention::filter(&m.cands, q);
        if m.shown.is_empty() {
            return json(&json!({ "rows": [], "query": q }));
        }
        let rows: Vec<Value> = m
            .shown
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let sub = match &c.handle {
                    Some(h) => format!("@{h} · {}", c.host),
                    None => format!("{} · {}", c.host, c.why),
                };
                json!({ "i": i, "label": c.label(), "sub": sub, "contact": c.contact })
            })
            .collect();
        json(&json!({ "rows": rows, "query": q }))
    }

    /// A row of the `@` list picked: `{from, to, insert}` (UTF-16) for the
    /// page to put in the textarea, as typing would.
    pub fn w_mention_pick(&mut self, text: &str, caret: u32, i: usize) -> String {
        let Some(m) = self.writing.mention.take() else {
            return "null".into();
        };
        let Some(c) = m.shown.get(i) else {
            return "null".into();
        };
        // `mention::insert` replaces `at..caret` with the link; the page
        // does the same in the textarea, so undo works.
        let cur = byte_of(text, caret).max(m.at);
        json(&json!({
            "from": u16_of(text, m.at),
            "to": u16_of(text, cur),
            "insert": c.link(),
        }))
    }

    pub fn w_mention_close(&mut self) {
        self.writing.mention = None;
    }

    // ------------------------------------------------------------ images

    /// Whether (and how) a picked or pasted file can go up.
    pub fn w_image_plan(&self, mime: &str, size: f64) -> String {
        if !self.is_live() {
            return json(&ImagePlan::Refuse {
                why: "Adding an image needs your blyg: this is the demo, with nowhere to put it."
                    .into(),
            });
        }
        if self.offline {
            return json(&ImagePlan::Refuse {
                why: "Adding an image needs a connection. Your text is kept; add it when you're back."
                    .into(),
            });
        }
        json(&image_plan(mime, size))
    }

    /// A new upload: its number and the placeholder to put at the caret.
    pub fn w_upload_begin(&mut self) -> String {
        self.writing.next_upload += 1;
        let n = self.writing.next_upload;
        json(&json!({ "n": n, "placeholder": placeholder(n) }))
    }

    /// `POST /api/media`'s answer: the Markdown that replaces placeholder
    /// `n` (`{md, id}`), or `{error}`. `thumb` is a small `data:` image the
    /// preview shows in its place.
    pub fn w_upload_answer(&mut self, status: u16, body: &str, alt: &str, thumb: &str) -> String {
        let m: Media = match api::read(status, body, None).and_then(|v| {
            serde_json::from_value(v).map_err(|e| ApiError::Rejected {
                status,
                message: format!("The blyg's answer to the upload doesn't read: {e}"),
                details: vec![],
            })
        }) {
            Ok(m) => m,
            Err(e) => return api_err(&e),
        };
        // The text gets the absolute URL: the blyg renders a relative
        // `media/…` page-relative, which breaks under /f/<id>/.
        let base = self.origin_str();
        let url = vm::absolute_url(&m.url, Some(&base)).unwrap_or(m.url.clone());
        if thumb.starts_with("data:image/") && thumb.len() < 400_000 {
            self.writing.uploads.insert(url.clone(), thumb.to_string());
        }
        json(&json!({ "md": image_markdown(&url, alt), "id": m.id, "url": url }))
    }

    /// Where placeholder `n` is in `text`, as `{from, to}` (UTF-16), or
    /// `null` when it was deleted meanwhile.
    pub fn w_placeholder_at(&self, text: &str, n: u32) -> String {
        let p = placeholder(n);
        match text.find(&p) {
            Some(i) => json(&json!({ "from": u16_of(text, i), "to": u16_of(text, i + p.len()) })),
            None => "null".into(),
        }
    }

    /// Placeholder `n` in a post that isn't in the editor any more: replace
    /// it there (`md` empty: remove it). True if it was there.
    pub fn w_replace_placeholder(&mut self, id: &str, n: u32, md: &str, now_ms: f64) -> bool {
        let lid = LocalId(id.into());
        let Some(it) = self.store.item(&lid) else {
            return false;
        };
        let p = placeholder(n);
        if !it.content_md.contains(&p) {
            return false;
        }
        let text = if md.is_empty() {
            it.content_md
                .replacen(&format!("{p}\n\n"), "", 1)
                .replacen(&p, "", 1)
        } else {
            it.content_md.replacen(&p, md, 1)
        };
        if self.store.save(&lid, &text, &rfc(now_ms))
            && let Some(it) = self.store.item(&lid)
        {
            self.list.update_item(it);
        }
        true
    }

    /// `DELETE /api/media/{id}`: an upload whose placeholder was deleted
    /// before it landed, so nothing refers to it.
    pub fn w_media_delete(&self, media_id: &str) -> String {
        req(Request {
            method: "DELETE",
            path: format!("/api/media/{}", enc(media_id)),
            body: Some(json!({})),
        })
    }

    // ------------------------------------------------------------ versions

    /// The open post's history: `{id, path}` to read (live), `{id}` (the
    /// demo has it), or `{error}`.
    pub fn w_versions_begin(&self) -> String {
        let Some(it) = self.current_item() else {
            return err("Open a post first");
        };
        if it.version == 0 {
            return err("Not published yet, so there are no versions");
        }
        let id = it.local_id.0.clone();
        if !self.is_live() {
            return json(&json!({ "id": id }));
        }
        if self.offline {
            return err("Your versions are on the blyg: this needs a connection");
        }
        match &it.server_id {
            Some(s) => {
                json(&json!({ "id": id, "path": format!("{}/versions", api::item_path(&s.0)) }))
            }
            None => err("That post isn't on the blyg yet"),
        }
    }

    /// The versions read (`GET …/versions`, every page, as a JSON array),
    /// or `""` for the demo's. Returns the history: rows newest first, each
    /// with what you can do to it.
    pub fn w_versions_view(&mut self, id: &str, items_json: &str, now_ms: f64) -> String {
        if self.is_live() && !items_json.is_empty() {
            let vals: Vec<Value> = serde_json::from_str(items_json).unwrap_or_default();
            let mut vs: Vec<Version> = vals.iter().filter_map(version_of).collect();
            vs.sort_by_key(|v| v.version);
            self.writing.versions.insert(id.to_string(), vs);
        }
        let Some(it) = self.w_item(id) else {
            return err("That post isn't here any more");
        };
        let vs = self.w_version_list(id);
        let dirty = vm::has_unpublished_edits(&it);
        let rows: Vec<VersionRow> = rvm::own_rows(&vs, it.version, at(now_ms))
            .into_iter()
            .map(|r| {
                let v = vs.iter().find(|v| v.version == r.version);
                let actions = v
                    .map(|v| {
                        let mut acts: Vec<ActionView> = vec![];
                        if !v.endcap {
                            acts.push(ActionView {
                                act: "view",
                                label: "View".into(),
                                tip: format!("v{} as published", v.version),
                                on: true,
                            });
                            acts.push(ActionView {
                                act: "diff",
                                label: "Diff vs now".into(),
                                tip: format!(
                                    "What changed from v{} to the text you have now",
                                    v.version
                                ),
                                on: true,
                            });
                        }
                        for a in rvm::own_actions(v, it.version, dirty).list() {
                            let (act, on) = match a {
                                rvm::OwnAction::Pinned => ("pinned", false),
                                rvm::OwnAction::Pin => ("pin", true),
                                rvm::OwnAction::Fork => ("fork", true),
                                rvm::OwnAction::ForkNeedsPin => ("fork", false),
                                rvm::OwnAction::Restore => ("restore", true),
                                rvm::OwnAction::Discard => ("discard", true),
                            };
                            acts.push(ActionView {
                                act,
                                label: a.label(v.version),
                                tip: a.tip(v.version),
                                on,
                            });
                        }
                        acts
                    })
                    .unwrap_or_default();
                VersionRow {
                    version: r.version,
                    note: r.note,
                    when: r.when,
                    badge: r.badge,
                    actions,
                }
            })
            .collect();
        json(&json!({
            "id": id,
            "title": vm::truncate_chars(&vm::item_title(&it), 60),
            "current": it.version,
            "status": crate::status_name(it.status),
            "dirty": dirty,
            "rows": rows,
        }))
    }

    /// One version: `{path}` to read it (live), or `{ready}` (it's here).
    pub fn w_version_begin(&self, id: &str, v: u32) -> String {
        if self.w_version_md(id, v).is_some() {
            return json(&json!({ "ready": true }));
        }
        if !self.is_live() {
            return err(format!("v{v} isn't here"));
        }
        match self.w_item(id).and_then(|i| i.server_id) {
            Some(s) => json(&json!({ "path": format!("{}/versions/{v}", api::item_path(&s.0)) })),
            None => err("That post isn't on the blyg"),
        }
    }

    /// `GET …/versions/{v}`'s answer, kept for the diff.
    pub fn w_version_answer(&mut self, id: &str, v: u32, status: u16, body: &str) -> String {
        match api::read(status, body, None) {
            Ok(val) => {
                let md = val
                    .get("contentMd")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.writing.version_md.insert((id.to_string(), v), md);
                "null".into()
            }
            Err(e) => api_err(&e),
        }
    }

    /// Version `v` as published, as a sandboxable page (rendered from its
    /// text here, as the preview is).
    pub fn w_version_page(&self, id: &str, v: u32, nonce: &str, dark: bool) -> String {
        let Some(md) = self.w_version_md(id, v) else {
            return String::new();
        };
        let it = self.w_item(id);
        let kind = it.as_ref().map_or(Kind::Fragment, |i| i.kind);
        let out = self.render(&md, kind, it.as_ref());
        let body = blyg_render::article_html(render_kind(kind), &out.html, None, None);
        page_shell(
            &format!("{BUILTIN_CSS}\n{}\n", Self::w_scheme(dark)),
            &body,
            nonce,
        )
    }

    /// What changed from version `v` to the text you have now, as HTML.
    pub fn w_version_diff(&self, id: &str, v: u32) -> String {
        let (Some(old), Some(it)) = (self.w_version_md(id, v), self.w_item(id)) else {
            return String::new();
        };
        if old == it.content_md {
            return "<p class=\"muted\">No difference: the text now is v".to_string()
                + &v.to_string()
                + "'s.</p>";
        }
        diff_html(&old, &it.content_md)
    }

    /// Restore `v` (confirmed): the working copy becomes its text, to
    /// publish as the next version. Live, `remote_restore` goes to the blyg.
    pub fn w_restore(&mut self, id: &str, v: u32, now_ms: f64) -> String {
        let lid = LocalId(id.into());
        let Some(it) = self.store.item(&lid) else {
            return json(&Outcome::Nothing);
        };
        if self.is_live() {
            if self.offline {
                return json(&Outcome::Toast {
                    message: "Restoring needs a connection".into(),
                });
            }
            return json(&Outcome::RemoteRestore {
                id: id.into(),
                version: v,
            });
        }
        match self.store.restore(&lid, v, &rfc(now_ms)) {
            Some(_) => {
                if let Some(x) = self.store.get_mut_pub(&lid) {
                    x.dirty = v != it.version;
                }
                if let Some(x) = self.store.item(&lid) {
                    self.list.update_item(x);
                }
                json(&Outcome::Toast {
                    message: if v == it.version {
                        format!("Back to v{v} as published")
                    } else {
                        format!(
                            "v{v}'s text is back. Publish to make it v{}.",
                            it.version + 1
                        )
                    },
                })
            }
            None => json(&Outcome::Nothing),
        }
    }

    /// Pin `v`, once you typed the word: `{req}` (live), `{done}` (demo),
    /// or `{error}`.
    pub fn w_pin(&mut self, id: &str, v: u32, typed: &str) -> String {
        if !rvm::pin_confirmed(typed) {
            return err(format!("Type “{}” to pin v{v} forever", rvm::PIN_WORD));
        }
        if !self.is_live() {
            self.writing.demo_pins.insert((id.to_string(), v));
            return json(&json!({ "done": format!("Pinned v{v}") }));
        }
        if self.offline {
            return err("Pinning needs a connection");
        }
        match self.w_synced(id) {
            Ok((_, sid)) => req(Request {
                method: "PUT",
                path: format!("{}/versions/{v}/pin", api::item_path(&sid)),
                body: Some(json!({ "expectedRevision": self.w_rev(id) })),
            }),
            Err(e) => err(e),
        }
    }

    /// The open post's Withdraw: a confirmation, or why not.
    pub fn w_withdraw_check(&self) -> String {
        let Some(it) = self.current_item() else {
            return json(&Outcome::Nothing);
        };
        json(&match vm::discard(&it) {
            vm::Discard::Withdraw => Outcome::Confirm {
                title: format!(
                    "Withdraw “{}”?",
                    vm::truncate_chars(&vm::item_title(&it), 48)
                ),
                body: format!(
                    "It leaves your blyg as v{}, a withdrawal: the post stays listed as withdrawn, \
                     its text goes from the page and the feeds, and readers who hold it see it taken back. \
                     Pinned versions stay public. You can't undo this.",
                    it.version + 1
                ),
            },
            vm::Discard::Delete => Outcome::Toast {
                message: format!(
                    "Not published, so there's nothing to withdraw. Delete the {} instead.",
                    vm::discard_noun(&it)
                ),
            },
            vm::Discard::AlreadyWithdrawn => Outcome::Toast {
                message: "Already withdrawn".into(),
            },
        })
    }

    /// Withdraw `id` (confirmed), with an optional note.
    pub fn w_withdraw(&mut self, id: &str, note: &str, now_ms: f64) -> String {
        let note = note.trim();
        if !self.is_live() {
            let lid = LocalId(id.into());
            let Some(x) = self.store.get_mut_pub(&lid) else {
                return err("That post isn't here any more");
            };
            x.status = Status::Withdrawn;
            x.version += 1;
            x.dirty = false;
            x.updated = rfc(now_ms);
            let v = x.version;
            if let Some(it) = self.store.item(&lid) {
                self.list.update_item(it);
            }
            return json(&json!({ "done": withdrawn_toast(v, note) }));
        }
        if self.offline {
            return err("Withdrawing needs a connection");
        }
        match self.w_synced(id) {
            Ok((_, sid)) => {
                let mut body = json!({ "expectedRevision": self.w_rev(id) });
                if !note.is_empty() {
                    body["note"] = json!(note);
                }
                req(post(format!("{}/withdraw", api::item_path(&sid)), body))
            }
            Err(e) => err(e),
        }
    }

    /// The toast after a withdrawal the blyg confirmed.
    pub fn w_withdrawn_toast(&self, id: &str, note: &str) -> String {
        let v = self.w_item(id).map_or(0, |i| i.version);
        withdrawn_toast(v, note.trim())
    }

    // ------------------------------------------------------------ fork

    /// `GET /api/fork-options` for a reading row (`key`: `sub|remote id`),
    /// or for your own post (`key`: its local id, `own`).
    pub fn w_fork_options_path(&self, key: &str, own: bool) -> String {
        if !self.is_live() {
            return err(
                "Forking takes a pinned version from a blyg. The demo's sample posts have none to give.",
            );
        }
        if self.offline {
            return err("Forking needs a connection");
        }
        if own {
            return match self.w_item(key).and_then(|i| i.server_id) {
                Some(s) => json(&json!({ "path": format!("/api/fork-options?id={}", enc(&s.0)) })),
                None => err("That post isn't on the blyg"),
            };
        }
        let Some((sub, rid)) = key.split_once('|') else {
            return err("Open a post to fork it");
        };
        json(&json!({ "path": format!("/api/fork-options?id={}&sub={}", enc(rid), enc(sub)) }))
    }

    /// The pins you can fork, newest first: `{origin, id, versions}`, or
    /// `{error}` (none, or the source couldn't be read).
    pub fn w_fork_options(&self, status: u16, body: &str, id: &str, now_ms: f64) -> String {
        let v = match api::read(status, body, None) {
            Ok(v) => v,
            Err(e) => return api_err(&e),
        };
        let origin = v
            .get("origin")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut versions: Vec<Value> = v
            .get("versions")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        let n = x.get("version").and_then(Value::as_u64)?;
                        let when = x
                            .get("at")
                            .and_then(Value::as_str)
                            .map(|t| vm::relative_time(t, at(now_ms)))
                            .unwrap_or_default();
                        let note = x
                            .get("note")
                            .and_then(Value::as_str)
                            .filter(|s| !s.trim().is_empty())
                            .map(|s| format!("“{}”", s.trim()))
                            .unwrap_or_else(|| "no note".into());
                        Some(json!({ "version": n, "when": when, "note": note }))
                    })
                    .collect()
            })
            .unwrap_or_default();
        versions.sort_by_key(|x| std::cmp::Reverse(x["version"].as_u64().unwrap_or(0)));
        if versions.is_empty() {
            let why = v
                .get("error")
                .and_then(Value::as_str)
                .map(|e| format!("Nothing to fork: {e}"))
                .unwrap_or_else(|| {
                    "This post has no pinned versions, so there's nothing to fork: forks descend from pins only."
                        .into()
                });
            return err(why);
        }
        json(&json!({ "origin": origin, "id": id, "versions": versions }))
    }

    /// `POST /api/items {mode: "fork", source}`.
    pub fn w_fork_request(&self, origin: &str, id: &str, v: u32) -> String {
        req(post(
            "/api/items".into(),
            json!({ "mode": "fork", "source": { "origin": origin, "id": id, "version": v } }),
        ))
    }

    /// The new draft: into the store and open. `{id}` or `{error}`.
    pub fn w_fork_answer(&mut self, status: u16, body: &str) -> String {
        let w: WireItem = match api::read(status, body, None).and_then(|v| api::parse_item(&v)) {
            Ok(w) => w,
            Err(e) => return api_err(&e),
        };
        let base = self.origin_str();
        let it = crate::live::item_of(&w, &base);
        let id = it.local_id.clone();
        self.store.live.rev.insert(id.0.clone(), w.revision);
        self.store.upsert(it);
        self.list.set_query("", self.results(""));
        self.list.select(&id);
        self.new_post = None;
        json(&json!({ "id": id.0 }))
    }

    // ------------------------------------------------------------ TK

    /// The open post's TK scopes: `[{index, instruction, done, imported}]`.
    pub fn w_tk_view(&self) -> String {
        let Some(it) = self.current_item() else {
            return json(&json!({ "scopes": [], "errors": [], "live": self.is_live() }));
        };
        let (scopes, errors) = blyg_render::parse_scopes(&it.content_md);
        let scopes: Vec<Value> = scopes
            .iter()
            .enumerate()
            .map(|(i, s)| {
                json!({
                    "index": i,
                    "instruction": if s.instruction.is_empty() { "(an empty TK: a placeholder)".to_string() } else { vm::truncate_chars(&s.instruction, 140) },
                    "done": s.output.is_some(),
                    "imported": s.imported.is_some(),
                    "sources": s.source_ids.len(),
                })
            })
            .collect();
        let errors: Vec<String> = errors.iter().map(|e| e.reason.clone()).collect();
        json(&json!({ "scopes": scopes, "errors": errors, "live": self.is_live() }))
    }

    /// `POST /api/items/{id}/generate {scope, expectedRevision}` for scope
    /// `index` of the open post: `{id, req}` or `{error}`.
    pub fn w_generate_request(&self, index: u32) -> String {
        let Some(it) = self.current_item() else {
            return err("Open a post first");
        };
        if !self.is_live() {
            return err(
                "Generating runs on your blyg, with its model. The demo has none: write the text yourself, or try it live.",
            );
        }
        if self.offline {
            return err("Generating needs a connection");
        }
        let id = it.local_id.0.clone();
        match self.w_synced(&id) {
            Ok((_, sid)) => json(&json!({
                "id": id,
                "req": post(
                    format!("{}/generate", api::item_path(&sid)),
                    json!({ "scope": index, "expectedRevision": self.w_rev(&id) }),
                ),
            })),
            Err(e) => err(e),
        }
    }

    /// Which model the blyg used, from the generate answer.
    pub fn w_generated_with(body: &str) -> String {
        serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|v| v.get("model").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default()
    }

    // ------------------------------------------------------------ the publish sheet

    /// What the publish sheet offers for post `id`: AI-drafted notes and
    /// the change classifier, when the blyg is set up for them (Studio's
    /// `aiModelChangelog`, `autoChangeNotes`, `aiModelClassify`,
    /// `changeCategoryInNote`), on a republish only.
    pub fn w_note_offer(&self, id: &str) -> String {
        let republish = self
            .w_item(id)
            .is_some_and(|i| i.version > 0 && i.status == Status::Public);
        let live = self.is_live() && !self.offline && republish;
        let draft = live && !self.w_setting_str("aiModelChangelog").is_empty();
        let classify = live && !self.w_setting_str("aiModelClassify").is_empty();
        json(&json!({
            "draft": draft,
            "auto": draft && self.w_setting_bool("autoChangeNotes") == Some(true),
            "classify": classify,
            "in_note": classify && self.w_setting_bool("changeCategoryInNote") != Some(false),
            "categories": CATEGORIES,
        }))
    }

    /// The sheet opened with the note you'd typed so far.
    pub fn w_note_begin(&mut self, typed: &str, in_note: bool) {
        self.writing.note = Some(NoteSession {
            typed: typed.trim().to_string(),
            in_note,
            ..NoteSession::default()
        });
    }

    /// `POST /api/items/{id}/note-draft` or `…/classify`.
    pub fn w_note_request(&self, id: &str, which: &str) -> String {
        if !matches!(which, "note-draft" | "classify") {
            return err("Unknown request");
        }
        match self.w_synced(id) {
            Ok((_, sid)) => req(post(
                format!("{}/{which}", api::item_path(&sid)),
                json!({ "expectedRevision": self.w_rev(id) }),
            )),
            Err(e) => err(e),
        }
    }

    /// The drafted note arrived: the note's new text (`{text}`), or why
    /// there's none (`{error}`).
    pub fn w_note_drafted(&mut self, status: u16, body: &str) -> String {
        let note = match api::read(status, body, None) {
            Ok(v) => v
                .get("note")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string(),
            Err(e) => return api_err(&e),
        };
        let s = self.writing.note.get_or_insert_with(NoteSession::default);
        s.drafted = Some(note.clone());
        let text = if s.in_note {
            compose_note(&note, s.category.as_deref())
        } else {
            note
        };
        json(&json!({ "text": text }))
    }

    /// The classifier answered: `{category, probability, reason, text}`.
    pub fn w_note_classified(&mut self, status: u16, body: &str, text: &str) -> String {
        let s = self.writing.note.get_or_insert_with(NoteSession::default);
        s.classified = true;
        let (cat, prob, reason) = match api::read(status, body, None) {
            Ok(v) => (
                v.get("category")
                    .and_then(Value::as_str)
                    .filter(|c| is_category(c))
                    .map(str::to_string),
                v.get("probability").and_then(Value::as_f64),
                v.get("reason").and_then(Value::as_str).map(str::to_string),
            ),
            Err(e) => (None, None, Some(e.message())),
        };
        s.model = cat.clone();
        s.category = cat.clone();
        let text = if s.in_note {
            recategorize(text, cat.as_deref())
        } else {
            text.to_string()
        };
        json(&json!({ "category": cat, "probability": prob, "reason": reason, "text": text }))
    }

    /// A category chip pressed (again: none). Returns the note's new text.
    pub fn w_note_choose(&mut self, category: &str, text: &str) -> String {
        let s = self.writing.note.get_or_insert_with(NoteSession::default);
        s.category = if s.category.as_deref() == Some(category) || !is_category(category) {
            None
        } else {
            Some(category.to_string())
        };
        let text = if s.in_note {
            recategorize(text, s.category.as_deref())
        } else {
            text.to_string()
        };
        json(&json!({ "text": text, "category": s.category }))
    }

    /// Publishing: `{generated, classified, category}` for the note as it
    /// stands, and whether it's the machine's text, unedited.
    /// Whether `text` is still the model's draft (the sheet's hint), without
    /// ending the session.
    pub fn w_note_generated(&self, text: &str) -> bool {
        self.writing
            .note
            .as_ref()
            .is_some_and(|s| s.generated(text))
    }

    pub fn w_note_finish(&mut self, text: &str) -> String {
        let s = self.writing.note.take().unwrap_or_default();
        json(&json!({
            "generated": s.generated(text),
            "classified": s.classified,
            "category": s.category,
        }))
    }

    /// `POST …/publish` with the note's provenance (`noteGenerated`) and the
    /// change category Studio keeps privately (`changeCategory`).
    pub fn w_publish_request(
        &self,
        id: &str,
        note: &str,
        generated: bool,
        classified: bool,
        category: Option<String>,
    ) -> String {
        let plain = self.live_publish_request(id, note);
        let mut v: Value = serde_json::from_str(&plain).unwrap_or(Value::Null);
        if let Some(body) = v.pointer_mut("/req/body") {
            let has_note = !note.trim().is_empty();
            if generated && has_note {
                body["noteGenerated"] = json!(true);
            }
            if classified {
                body["changeCategory"] = match category.filter(|c| is_category(c)) {
                    Some(c) => json!(c),
                    None => Value::Null,
                };
            }
        }
        json(&v)
    }

    /// After a publish: the blyg's warning, if any (handles it left as
    /// written, and its own `warning`).
    pub fn w_publish_warning(status: u16, body: &str) -> Option<String> {
        let v = api::read(status, body, None).ok()?;
        let handles: Vec<String> = v
            .get("unresolvedHandles")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|h| h.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let mut parts = vec![];
        if let Some(w) = blyg_core::contacts::unresolved_warning(&handles) {
            parts.push(capitalised(&w));
        }
        if let Some(w) = v
            .get("warning")
            .and_then(Value::as_str)
            .filter(|w| !w.trim().is_empty())
        {
            parts.push(w.trim().to_string());
        }
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    // ------------------------------------------------------------ the ⋯ menu

    /// What the open post's ⋯ menu offers, in order.
    pub fn w_post_menu(&self) -> String {
        let Some(it) = self.current_item() else {
            return json(&json!({ "items": [] }));
        };
        let mut items: Vec<Value> = vec![];
        let mut add = |act: &str, label: &str, sub: String, on: bool| {
            items.push(json!({ "act": act, "label": label, "sub": sub, "on": on }));
        };
        let tk = blyg_render::parse_scopes(&it.content_md).0.len();
        add(
            "image",
            "🖼  Add an image…",
            if self.is_live() {
                "From your photos or files, with alt text".into()
            } else {
                "Needs your blyg".into()
            },
            true,
        );
        add(
            "tk",
            "✦  Generate TK…",
            match tk {
                0 => "Add [TK]what to write[/TK] first, then generate it on the blyg".into(),
                1 => "1 scope in this post".into(),
                n => format!("{n} scopes in this post"),
            },
            true,
        );
        add(
            "history",
            "⟲  Versions…",
            if it.version > 1 {
                format!("v1 to v{}: view, diff, restore, pin", it.version)
            } else if it.version == 1 {
                "v1: view, diff, restore, pin".into()
            } else {
                "Not published yet".into()
            },
            it.version > 0,
        );
        if it.status == Status::Public {
            add(
                "withdraw",
                "⊘  Withdraw…",
                "Take it back from your blyg (confirmed)".into(),
                true,
            );
        }
        if vm::has_unpublished_edits(&it) {
            add(
                "discard",
                "↺  Discard changes…",
                format!("Back to v{} as published", it.version),
                true,
            );
        }
        if it.version == 0 && it.status != Status::Scratch {
            add(
                "delete",
                "✕  Delete draft…",
                "Gone for good (confirmed)".into(),
                true,
            );
        }
        if it.version == 0 && it.status == Status::Scratch {
            add(
                "delete",
                "✕  Delete note…",
                "Gone for good (confirmed)".into(),
                true,
            );
        }
        json(&json!({ "items": items }))
    }
}

fn withdrawn_toast(v: u32, note: &str) -> String {
    if note.is_empty() {
        format!("Withdrawn as v{v}. It stays listed as withdrawn.")
    } else {
        format!("Withdrawn as v{v} · “{note}”. It stays listed as withdrawn.")
    }
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
#[path = "writing_tests.rs"]
mod tests;
