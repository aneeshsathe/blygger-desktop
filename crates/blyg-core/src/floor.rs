//! blygger-studio 0.47.0: the oldest studio that understands labelled links.
//!
//! Burrow's `[[` picker writes labelled links, `[[id | label]]`, and a studio
//! older than 0.47 doesn't understand the label: publishing one there gives
//! literal text or a broken link. So on an older blyg a post whose text holds
//! a labelled link or directive ([`has_labels`]) waits: its outbox ops and
//! its publish are held (`CoreError::ServerOutdated`) until the blyg is
//! updated or the labels are gone ([`strip_labels`] writes them bare).
//! Everything else syncs as usual, and the picker writes the bare form there
//! ([`picker_label`]).
//!
//! What counts is the text Burrow would send, all of it: a `[TK]` scope's
//! generated output is part of the post and the studio renders it like the
//! rest, so a labelled link there holds the post too. Withdraw and pin don't
//! send the text, so they always go.
//!
//! How Burrow tells, in order:
//!
//! 1. The manifest, `GET {blyg}/blyg.json`. A studio's carries
//!    `generator: "blygger-studio/<version>"` (upstream `buildManifest`, from
//!    `CLIENT.version`). It is the one signal that changed between 0.46.1 and
//!    0.47.0: both send `blyg: "0.4"` and `level: 2`, so the protocol
//!    version can't tell them apart.
//! 2. Any other generator (other implementations exist), an unparseable
//!    version, or no manifest: a capability probe, `GET /api/search?limit=1`.
//!    Since 0.47 every hit carries `label` (required in the contract), the
//!    text the picker writes. A hit without it is an older studio.
//! 3. No hit to look at (an empty blyg), or no search route: Burrow can't
//!    tell, and doesn't lock the blyg out on a guess. It is treated as
//!    meeting the floor.

use serde_json::Value;

/// The floor, for messages: "blygger-studio 0.47 or later".
pub const MIN_STUDIO: &str = "0.47";
const MIN: (u64, u64, u64) = (0, 47, 0);
/// The manifest `generator` name the version check applies to.
const STUDIO: &str = "blygger-studio";

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Floor {
    /// blygger-studio 0.47 or later, or a server that has what it needs.
    Meets,
    /// Older: posts with labels wait. `version` is the studio's when known.
    Below { version: Option<String> },
    /// Nothing to tell by; treated as meeting the floor.
    Unknown,
}

impl Floor {
    /// Whether the blyg is new enough for every post (`Unknown` counts).
    pub fn meets(&self) -> bool {
        !matches!(self, Floor::Below { .. })
    }
}

/// `"0.47.0"` → `(0, 47, 0)`. A pre-release or build suffix is ignored
/// (`0.47.0-rc.1` reads as 0.47.0); a missing patch is 0.
fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.trim().split(['-', '+', ' ']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = match parts.next() {
        Some(p) => p.parse().ok()?,
        None => 0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// The verdict from a manifest, when it names a blygger-studio with a
/// version Burrow can read. `None` for anything else (another
/// implementation, no generator, a version that doesn't parse): the caller
/// probes instead.
pub fn from_manifest(manifest: &Value) -> Option<Floor> {
    let generator = manifest.get("generator")?.as_str()?;
    let (name, version) = generator.trim().split_once('/')?;
    if name != STUDIO {
        return None;
    }
    let v = parse_version(version)?;
    Some(if v >= MIN {
        Floor::Meets
    } else {
        Floor::Below {
            version: Some(version.trim().to_string()),
        }
    })
}

/// The verdict from a `GET /api/search?limit=1` answer: the first hit's
/// `label` (studio 0.47+). No hit: `Unknown`.
pub fn from_search(answer: &Value) -> Floor {
    match answer
        .get("items")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
    {
        None => Floor::Unknown,
        Some(hit) if hit.get("label").is_some_and(Value::is_string) => Floor::Meets,
        Some(_) => Floor::Below { version: None },
    }
}

/// One labelled reference outside code: the bytes it spans (the `!`
/// included for a directive), and the bare form it rewrites to.
fn labelled(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    use crate::grammar::link_refs;
    let code = blyg_render::code_ranges(text);
    let in_code = |at: usize| code.iter().any(|&(s, e)| s <= at && at < e);
    let mut out: Vec<(std::ops::Range<usize>, String)> = link_refs(text)
        .into_iter()
        .filter(|r| r.label.is_some() && !in_code(r.range.start))
        .map(|r| (r.range, format!("[[{}]]", r.id)))
        .collect();
    // `![[id | label]]`, own-line or inline (a `[TK]` instruction's source):
    // the `[[…]]` right after the `!`, read as a link at its start.
    let mut at = 0;
    while let Some(i) = text[at..].find("![[") {
        let p = at + i;
        let line_end = text[p..].find('\n').map_or(text.len(), |n| p + n);
        match link_refs(&text[p + 1..line_end]).into_iter().next() {
            Some(r) if r.range.start == 0 => {
                if r.label.is_some() && !in_code(p) {
                    out.push((p..p + 1 + r.range.end, format!("![[{}]]", r.id)));
                }
                at = p + 1 + r.range.end;
            }
            _ => at = p + 1,
        }
    }
    out.sort_by_key(|(r, _)| r.start);
    out
}

/// Whether `text` holds a labelled link or directive, `[[id | label]]` or
/// `![[id | label]]`, outside code and with a label that isn't empty: what
/// a studio older than 0.47 can't show.
pub fn has_labels(text: &str) -> bool {
    !labelled(text).is_empty()
}

/// `text` with every labelled link and directive outside code written bare,
/// `[[id]]` / `![[id]]`, which every studio understands. Code is untouched.
pub fn strip_labels(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (r, bare) in labelled(text) {
        out.push_str(&text[last..r.start]);
        out.push_str(&bare);
        last = r.end;
    }
    out.push_str(&text[last..]);
    out
}

/// The label a picker writes: none on a blyg below the floor (`below`), so a
/// new link goes in bare and nothing waits on it.
pub fn picker_label(below: bool, label: &str) -> Option<&str> {
    (!below).then_some(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "7c9wk2n4h6q1x8v0z3m5rjy2ke";

    #[test]
    fn labelled_links_and_directives_count() {
        assert!(has_labels(&format!("see [[{ID} | that post]] now")));
        assert!(has_labels(&format!("see [[{ID}|x]]")));
        assert!(has_labels(&format!("intro\n\n![[{ID} | their post]]\n")));
        assert!(has_labels(&format!(
            "[TK] summarise ![[{ID} | it]] [=] out [/TK]"
        )));
        // Generated output is text like any other.
        assert!(has_labels(&format!(
            "[TK] link it [=] as [[{ID} | they said]] [/TK]"
        )));
    }

    #[test]
    fn bare_empty_and_code_dont_count() {
        assert!(!has_labels(&format!("see [[{ID}]] and\n![[{ID}]]\n")));
        // An empty label is no label.
        assert!(!has_labels(&format!("see [[{ID} | ]] and [[{ID} |   ]]")));
        assert!(!has_labels(&format!("![[{ID} |]]")));
        // Not an id: not a link at all.
        assert!(!has_labels("see [[short | label]]"));
        // Inline code, fenced code, indented code.
        assert!(!has_labels(&format!("`[[{ID} | x]]`")));
        assert!(!has_labels(&format!(
            "```\n![[{ID} | x]]\n[[{ID} | y]]\n```\n"
        )));
        assert!(!has_labels(&format!("text\n\n    [[{ID} | x]]\n")));
        assert!(has_labels(&format!("`code` then [[{ID} | x]]")));
    }

    #[test]
    fn strip_writes_them_bare_and_leaves_code() {
        let t = format!("a [[{ID} | one]] b [[{ID}]] `[[{ID} | code]]`\n![[{ID} | quote]]\n");
        let bare = strip_labels(&t);
        assert_eq!(
            bare,
            format!("a [[{ID}]] b [[{ID}]] `[[{ID} | code]]`\n![[{ID}]]\n")
        );
        assert!(!has_labels(&bare));
        assert_eq!(strip_labels("nothing here"), "nothing here");
    }

    #[test]
    fn the_picker_writes_bare_below_the_floor() {
        assert_eq!(picker_label(true, "Hello"), None);
        assert_eq!(picker_label(false, "Hello"), Some("Hello"));
    }

    /// What upstream's `buildManifest` sends (the keys that matter here).
    fn studio(version: &str) -> Value {
        json!({
            "blyg": "0.4",
            "level": 2,
            "generator": format!("blygger-studio/{version}"),
            "generator_url": "https://github.com/blygger/blygger-studio",
            "site": "https://blyg.example.com",
            "title": "Example",
        })
    }

    #[test]
    fn studio_0_46_1_is_below_the_floor() {
        assert_eq!(
            from_manifest(&studio("0.46.1")),
            Some(Floor::Below {
                version: Some("0.46.1".into())
            })
        );
        assert!(!from_manifest(&studio("0.46.3")).unwrap().meets());
        assert!(!from_manifest(&studio("0.9.0")).unwrap().meets());
    }

    #[test]
    fn studio_0_47_0_and_later_meet_it() {
        for v in [
            "0.47.0",
            "0.47.1",
            "0.48.0",
            "0.100.0",
            "1.0.0",
            "0.47.0-rc.1",
        ] {
            assert_eq!(from_manifest(&studio(v)), Some(Floor::Meets), "{v}");
        }
    }

    #[test]
    fn the_protocol_version_alone_cant_tell_them_apart() {
        // Both send blyg "0.4": only the generator moved.
        assert_eq!(studio("0.46.1")["blyg"], studio("0.47.0")["blyg"]);
        assert_eq!(studio("0.46.1")["level"], studio("0.47.0")["level"]);
    }

    #[test]
    fn another_generator_or_an_unreadable_version_falls_back_to_the_probe() {
        for g in [
            json!({ "blyg": "0.4", "generator": "someone-elses-blyg/2.1.0" }),
            json!({ "blyg": "0.4", "generator": "blygger-studio" }),
            json!({ "blyg": "0.4", "generator": "blygger-studio/main" }),
            json!({ "blyg": "0.4", "generator": "blygger-studio/0.47.0.1" }),
            json!({ "blyg": "0.4", "generator": 47 }),
            json!({ "blyg": "0.4" }),
            json!([1, 2]),
        ] {
            assert_eq!(from_manifest(&g), None, "{g}");
        }
    }

    #[test]
    fn the_probe_reads_the_label_on_a_search_hit() {
        let hit = |label: Option<&str>| {
            let mut h = json!({ "id": "a1", "excerpt": "Hello", "version": 1 });
            if let Some(l) = label {
                h["label"] = json!(l);
            }
            json!({ "items": [h], "total": 1, "offset": 0, "limit": 1 })
        };
        assert_eq!(from_search(&hit(Some("Hello"))), Floor::Meets);
        assert_eq!(from_search(&hit(None)), Floor::Below { version: None });
        // An empty blyg has nothing to tell by: not locked out.
        let empty = json!({ "items": [], "total": 0, "offset": 0, "limit": 1 });
        assert_eq!(from_search(&empty), Floor::Unknown);
        assert!(Floor::Unknown.meets());
        assert_eq!(from_search(&json!({})), Floor::Unknown);
    }
}
