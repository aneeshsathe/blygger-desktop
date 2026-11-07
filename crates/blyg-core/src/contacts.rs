//! --- contacts --- Studio 0.56 parity (contract 5.7.0, proposal 14).
//!
//! - `GET /api/contacts`: the owner's address book, `{handle, name,
//!   origin}`. A handle is the author's own word for an origin (rule 6: no
//!   identity layer); nothing resolves it anywhere else.
//! - Publish rewrites `@handle` to an h-card link and answers the handles
//!   it couldn't resolve as `unresolvedHandles`.
//! - `GET /api/reading?mentions=true`: the held posts that mention you.
//!
//! The 5.0.0 OpenAPI fixture in this repo predates these; the shapes here
//! follow the studio 0.56 summary: a cursor page (or a bare array) of
//! contacts, and `ReadingEntry` rows for the mentions lens.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One address-book entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    /// Without the `@`.
    pub handle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub origin: String,
}

/// Whether a studio's `contractVersion` serves contacts, the `@handle`
/// rewrite and the mentions lens (5.7.0, studio 0.56).
pub fn serves_contacts(version: &str) -> bool {
    let mut parts = version.trim().split('.').map(|p| p.parse::<u64>().ok());
    match (parts.next().flatten(), parts.next().flatten()) {
        (Some(5), Some(minor)) => minor >= 7,
        (Some(major), _) => major > 5,
        _ => false,
    }
}

/// The contacts in a `GET /api/contacts` answer: `{items: […]}`,
/// `{contacts: […]}` or a bare array. Entries without a handle or an
/// origin are left out; a leading `@` is dropped.
pub fn parse_contacts(v: &Value) -> Vec<Contact> {
    let list = v
        .get("items")
        .or_else(|| v.get("contacts"))
        .unwrap_or(v)
        .as_array()
        .cloned()
        .unwrap_or_default();
    list.into_iter()
        .filter_map(|c| {
            let s = |k: &str| {
                c.get(k)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            };
            Some(Contact {
                handle: s("handle")?.trim_start_matches('@').to_string(),
                name: s("name"),
                origin: s("origin")?,
            })
        })
        .collect()
}

/// `(subscription id, remote id)` of each imported row in a
/// `GET /api/reading` page.
pub fn parse_reading_keys(v: &Value) -> Vec<(String, String)> {
    v.get("items")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let i = e.get("imported")?;
                    Some((
                        i.get("subscriptionId")?.as_str()?.to_string(),
                        i.get("remoteId")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The publish warning for the handles the blyg left as written.
pub fn unresolved_warning(handles: &[String]) -> Option<String> {
    if handles.is_empty() {
        return None;
    }
    let list = handles
        .iter()
        .map(|h| format!("@{}", h.trim_start_matches('@')))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!("not in your contacts, left as written: {list}"))
}

/// Contacts whose handle, name or origin contains `q` (case-insensitive),
/// handles that start with it first.
pub fn matching<'a>(contacts: &'a [Contact], q: &str) -> Vec<&'a Contact> {
    let q = q.trim().trim_start_matches('@').to_lowercase();
    let mut out: Vec<(bool, &Contact)> = contacts
        .iter()
        .filter(|c| {
            q.is_empty()
                || c.handle.to_lowercase().contains(&q)
                || c.name
                    .as_deref()
                    .is_some_and(|n| n.to_lowercase().contains(&q))
                || c.origin.to_lowercase().contains(&q)
        })
        .map(|c| (!c.handle.to_lowercase().starts_with(&q), c))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.handle.cmp(&b.1.handle)));
    out.into_iter().map(|(_, c)| c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn contracts_from_5_7_serve_contacts() {
        assert!(serves_contacts("5.7.0"));
        assert!(serves_contacts("5.12.1"));
        assert!(serves_contacts("6.0.0"));
        assert!(!serves_contacts("5.6.3"));
        assert!(!serves_contacts("5.0.0"));
        assert!(!serves_contacts("x"));
    }

    #[test]
    fn contacts_parse_from_any_list_shape() {
        let one = json!({"handle": "@lin", "name": "Lin", "origin": "https://lin.example.org"});
        for v in [
            json!({"items": [one.clone()], "page": {"limit": 100, "nextCursor": null}}),
            json!({"contacts": [one.clone()]}),
            json!([one.clone(), {"handle": "", "origin": "https://x.example"}, {"handle": "y"}]),
        ] {
            assert_eq!(
                parse_contacts(&v),
                [Contact {
                    handle: "lin".into(),
                    name: Some("Lin".into()),
                    origin: "https://lin.example.org".into()
                }]
            );
        }
    }

    #[test]
    fn matching_puts_handle_prefixes_first() {
        let c = |h: &str, n: &str| Contact {
            handle: h.into(),
            name: Some(n.into()),
            origin: format!("https://{h}.example.org"),
        };
        let all = [c("rue", "Lin's neighbour"), c("lin", "Lin")];
        let m: Vec<_> = matching(&all, "@lin")
            .iter()
            .map(|c| c.handle.clone())
            .collect();
        assert_eq!(m, ["lin", "rue"]);
        assert_eq!(matching(&all, "").len(), 2);
    }

    #[test]
    fn unresolved_handles_are_named() {
        assert_eq!(unresolved_warning(&[]), None);
        assert_eq!(
            unresolved_warning(&["sam".into(), "@kit".into()]).unwrap(),
            "not in your contacts, left as written: @sam, @kit"
        );
    }
}
