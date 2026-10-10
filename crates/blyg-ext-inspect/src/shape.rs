//! What the inspect sheet shows: the record the client holds for an entry,
//! reshaped for reading. A port of blygger-studio's
//! `extensions/inspect/ui/shape.ts` (studio 0.39.0), so both clients elide
//! and summarise alike. Nothing is fetched; this is the client's copy.
//!
//! Two changes, both display only:
//!   - content bodies (`content_md`, `content_html`) become a character
//!     count (UTF-16 units, as JavaScript's `length`), so the protocol
//!     fields are not buried under the text;
//!   - references kept as JSON strings in `*_json` fields are parsed, so a
//!     `stub_of` or `transclusions` list reads as a structure.

use serde_json::{Map, Value};

const BODY: [&str; 2] = ["content_md", "content_html"];

/// How a body reads once elided: "‹12 characters, not shown›".
pub fn elided(body: &str) -> String {
    format!("‹{} characters, not shown›", body.encode_utf16().count())
}

/// The record with bodies elided and `*_json` strings parsed, at any depth.
pub fn shape_record(value: &Value) -> Value {
    match value {
        Value::Array(a) => Value::Array(a.iter().map(shape_record).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(key, field)| {
                    let shaped = match field {
                        Value::String(s) if BODY.contains(&key.as_str()) => {
                            Value::String(elided(s))
                        }
                        Value::String(s) if key.ends_with("_json") => {
                            match serde_json::from_str::<Value>(s) {
                                Ok(v) => shape_record(&v),
                                Err(_) => field.clone(),
                            }
                        }
                        _ => shape_record(field),
                    };
                    (key.clone(), shaped)
                })
                .collect::<Map<_, _>>(),
        ),
        _ => value.clone(),
    }
}

/// The few fields worth reading first, as label/value pairs; absent ones
/// are left out. As the Studio's `summary`, plus Burrow's own reading row:
/// its `transclusions` list is a top-level field (left out when empty),
/// and it keeps no `content_hash`, so the hash of the Markdown it holds
/// (`held_hash`, the protocol's `content_hash`) stands in.
pub fn summary(record: &Value, held_hash: Option<&str>) -> Vec<(String, String)> {
    let get = |k: &str| record.get(k).filter(|v| !v.is_null());
    let published = get("published");
    let transclusions = match published.and_then(|p| p.get("transclusions")) {
        Some(Value::Array(a)) => Some(Value::from(a.len())),
        _ => match get("transclusions_json") {
            Some(Value::String(s)) => match serde_json::from_str::<Value>(s) {
                Ok(Value::Array(a)) => Some(Value::from(a.len())),
                Ok(Value::String(s)) => Some(Value::from(s.encode_utf16().count())),
                Ok(_) => Some(Value::from(0)),
                Err(_) => None,
            },
            _ => match get("transclusions") {
                Some(Value::Array(a)) => Some(Value::from(a.len())),
                // A Burrow reading row says nothing when it quotes nothing.
                _ if get("remote_id").is_some() && get("subscription_id").is_some() => {
                    Some(Value::from(0))
                }
                _ => None,
            },
        },
    };
    let hash = published
        .and_then(|p| p.get("content_hash"))
        .filter(|v| !v.is_null())
        .or_else(|| get("content_hash"))
        .cloned()
        .or_else(|| held_hash.map(Value::from));
    let rows: Vec<(&str, Option<Value>)> = vec![
        ("id", get("id").or_else(|| get("remote_id")).cloned()),
        ("kind", get("kind").cloned()),
        ("version", get("version").cloned()),
        ("state", get("status").or_else(|| get("state")).cloned()),
        ("content hash", hash),
        (
            "stub of",
            describe(
                get("stub_of")
                    .cloned()
                    .or_else(|| parse(get("stub_of_json"))),
            ),
        ),
        (
            "forked from",
            describe(
                get("forked_from")
                    .cloned()
                    .or_else(|| parse(get("forked_from_json"))),
            ),
        ),
        ("transclusions", transclusions),
    ];
    rows.into_iter()
        .filter_map(|(k, v)| {
            let v = v.filter(|v| !v.is_null())?;
            let s = js_string(&v);
            (!s.is_empty()).then(|| (k.to_string(), s))
        })
        .collect()
}

fn parse(field: Option<&Value>) -> Option<Value> {
    match field {
        Some(Value::String(s)) => serde_json::from_str(s).ok(),
        _ => None,
    }
}

/// A reference in one line: origin + id + version, or a `{url}` target.
fn describe(r: Option<Value>) -> Option<Value> {
    let r = r?;
    let o = r.as_object()?;
    if let Some(Value::String(url)) = o.get("url") {
        return Some(Value::String(url.clone()));
    }
    let version = o.get("version").map(|v| format!("v{}", js_string(v)));
    let parts: Vec<String> = [o.get("origin").cloned(), o.get("id").cloned()]
        .into_iter()
        .flatten()
        .filter(truthy)
        .map(|v| js_string(&v))
        .chain(version)
        .collect();
    Some(Value::String(parts.join(" ")))
}

/// JavaScript's `Boolean(v)`.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// JavaScript's `String(v)`, for the values a record holds.
fn js_string(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match (n.as_i64(), n.as_f64()) {
            (Some(i), _) => i.to_string(),
            (None, Some(f)) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{f:.0}"),
            _ => n.to_string(),
        },
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .map(|x| {
                if x.is_null() {
                    String::new()
                } else {
                    js_string(x)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

/// The JSON the sheet shows (two-space indent, as `JSON.stringify(…, null, 2)`).
pub fn shaped_json(record: &Value) -> String {
    serde_json::to_string_pretty(&shape_record(record)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    // The Studio's own example (test-ui/default-extensions.test.ts).
    #[test]
    fn elides_bodies_parses_json_columns_and_summarises_references() {
        let imported = json!({
            "remote_id": "abc", "kind": "thread", "version": 3, "state": "current",
            "content_hash": "sha256-x",
            "content_md": "hello", "content_html": "<p>hello</p>",
            "stub_of_json": json!({"origin": "https://a.example/", "id": "p1", "version": 2}).to_string(),
            "transclusions_json": json!([{"id": "t1"}, {"id": "t2"}]).to_string(),
            "forked_from_json": null, "media_json": "not json",
        });
        let mut want = imported.clone();
        want["content_md"] = json!("‹5 characters, not shown›");
        want["content_html"] = json!("‹12 characters, not shown›");
        want["stub_of_json"] = json!({"origin": "https://a.example/", "id": "p1", "version": 2});
        want["transclusions_json"] = json!([{"id": "t1"}, {"id": "t2"}]);
        assert_eq!(shape_record(&imported), want);
        assert_eq!(
            summary(&imported, None),
            pairs(&[
                ("id", "abc"),
                ("kind", "thread"),
                ("version", "3"),
                ("state", "current"),
                ("content hash", "sha256-x"),
                ("stub of", "https://a.example/ p1 v2"),
                ("transclusions", "2"),
            ])
        );
        let own = json!({
            "id": "mine", "kind": "fragment", "status": "public", "version": 1, "content_md": "x",
            "stub_of": {"url": "https://elsewhere.example/post"}, "forked_from": null,
            "published": {"content_hash": "sha256-y", "content_html": "<p>x</p>", "transclusions": []},
            "versions": [{"version": 1, "content_md": "x", "content_html": "<p>x</p>"}],
        });
        assert_eq!(
            summary(&own, None),
            pairs(&[
                ("id", "mine"),
                ("kind", "fragment"),
                ("version", "1"),
                ("state", "public"),
                ("content hash", "sha256-y"),
                ("stub of", "https://elsewhere.example/post"),
                ("transclusions", "0"),
            ])
        );
        assert_eq!(
            shape_record(&own)["versions"][0]["content_html"],
            "‹8 characters, not shown›"
        );
    }

    #[test]
    fn burrows_reading_row_summarises_too() {
        let row = json!({
            "subscription_id": "sub-1", "remote_id": "r9", "origin": "https://blyg.example.com",
            "kind": "thread", "state": "current", "version": 4,
            "content_md": "Tides 🌊", "content_html": "<p>Tides 🌊</p>",
            "forked_from": {"origin": "https://other.example.com", "id": "p2", "version": 1},
        });
        assert_eq!(
            summary(&row, Some("sha256:ab")),
            pairs(&[
                ("id", "r9"),
                ("kind", "thread"),
                ("version", "4"),
                ("state", "current"),
                ("content hash", "sha256:ab"),
                ("forked from", "https://other.example.com p2 v1"),
                ("transclusions", "0"),
            ])
        );
        // UTF-16 units, as the Studio counts: the wave is two.
        assert_eq!(
            shape_record(&row)["content_md"],
            "‹8 characters, not shown›"
        );
        let mut quoting = row.clone();
        quoting["transclusions"] = json!([{"id": "t1", "version": 2}]);
        assert!(summary(&quoting, None).contains(&("transclusions".into(), "1".into())));
        assert!(
            !summary(&quoting, None)
                .iter()
                .any(|(k, _)| k == "content hash"),
            "no hash held, none shown"
        );
    }

    #[test]
    fn references_read_as_one_line() {
        assert_eq!(describe(Some(json!({"id": "p1"}))), Some(json!("p1")));
        assert_eq!(
            describe(Some(json!({"origin": "", "id": "p1", "version": null}))),
            Some(json!("p1 vnull")),
            "as JavaScript's template string"
        );
        assert_eq!(describe(Some(json!("x"))), None);
        assert_eq!(describe(None), None);
        // An empty description is left out of the summary.
        let r = json!({"kind": "fragment", "stub_of": {}});
        assert_eq!(summary(&r, None), pairs(&[("kind", "fragment")]));
    }

    #[test]
    fn the_json_is_indented_by_two() {
        let s = shaped_json(&json!({"a": {"content_md": "hi"}}));
        assert_eq!(
            s,
            "{\n  \"a\": {\n    \"content_md\": \"‹2 characters, not shown›\"\n  }\n}"
        );
    }
}
