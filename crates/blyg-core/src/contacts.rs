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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    /// Without the `@`.
    pub handle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Where the person can be found (the contract's `url`).
    pub origin: String,
    /// --- contacts --- The studio's id for the entry (`""` in a cache
    /// written before Burrow managed the book).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// --- contacts --- Alternate identifiers. Only `self` and `bound`
    /// ones go on the wire, in the h-card; the API can only add `entered`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<ContactId>,
}

/// --- contacts --- One of a contact's alternate identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactId {
    pub id: String,
    /// `self` (the person's own page says so), `bound` or `entered`.
    pub provenance: String,
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
                // Studio 0.56 calls it `url` (the person's address).
                origin: s("url").or_else(|| s("origin"))?,
                id: s("id").unwrap_or_default(),
                ids: parse_ids(c.get("ids")),
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

// --- contacts --- managing the book (studio 0.55–0.56: `POST /contacts`,
// `PATCH`/`DELETE /contacts/{id}`, `POST /contacts/{id}/resolve`).

/// A contact's `ids`, as served. Entries without an id are left out.
fn parse_ids(v: Option<&Value>) -> Vec<ContactId> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|i| {
                    Some(ContactId {
                        id: i.get("id")?.as_str()?.to_string(),
                        provenance: i
                            .get("provenance")
                            .and_then(Value::as_str)
                            .unwrap_or("entered")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One contact, as `POST`, `PATCH` and `…/resolve` answer it.
pub fn parse_contact(v: &Value) -> Option<Contact> {
    parse_contacts(&Value::Array(vec![v.clone()])).pop()
}

impl Contact {
    /// The name the public page shows (the handle when there's none).
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.handle)
    }

    /// Identifiers the person's own page vouches for (`self`, `bound`):
    /// the ones the h-card carries.
    pub fn checked_ids(&self) -> Vec<&str> {
        self.ids
            .iter()
            .filter(|i| i.provenance != "entered")
            .map(|i| i.id.as_str())
            .collect()
    }
}

/// What the add and edit forms send.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContactInput {
    pub handle: String,
    pub name: String,
    pub url: String,
}

impl ContactInput {
    /// Trimmed, with a typed `@` dropped from the handle.
    pub fn tidy(&self) -> ContactInput {
        ContactInput {
            handle: self.handle.trim().trim_start_matches('@').to_string(),
            name: self.name.trim().to_string(),
            url: self.url.trim().to_string(),
        }
    }

    /// The studio's own checks (`contactInput`, `HANDLE`), so a mistake is
    /// named before the round trip. `Err` is the sentence to show.
    pub fn check(&self) -> std::result::Result<(), String> {
        let t = self.tidy();
        if t.handle.is_empty() || t.name.is_empty() || t.url.is_empty() {
            return Err("A contact needs a handle, a display name and a URL".into());
        }
        if !valid_handle(&t.handle) {
            return Err(
                "A handle is letters, digits, - and _, starting with a letter or digit, \
                 at most 39 characters"
                    .into(),
            );
        }
        if t.name.chars().count() > 200 {
            return Err("A display name is at most 200 characters".into());
        }
        match url::Url::parse(&t.url) {
            Ok(u) if u.scheme() == "https" => Ok(()),
            Ok(_) => Err("A contact's URL must be https".into()),
            Err(_) => Err("A contact's URL must be an absolute https:// address".into()),
        }
    }
}

/// Studio's `HANDLE`: `^[A-Za-z0-9][A-Za-z0-9_-]{0,38}$`.
pub fn valid_handle(h: &str) -> bool {
    let b = h.as_bytes();
    !b.is_empty()
        && b.len() <= 39
        && b[0].is_ascii_alphanumeric()
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-')
}

/// Studio's `suggestHandle`: the name's first word, reduced to what a
/// handle may hold (lowercase; accents dropped).
pub fn suggest_handle(name: &str) -> String {
    let word = name.split_whitespace().next().unwrap_or("");
    let folded: String = word
        .chars()
        .filter_map(|c| {
            let c = fold_accent(c).to_ascii_lowercase();
            (c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-').then_some(c)
        })
        .collect();
    folded
        .trim_start_matches(['_', '-'])
        .chars()
        .take(39)
        .collect()
}

/// The base letter of a common accented Latin letter (what NFKD and
/// dropping the marks gives), else the letter itself.
fn fold_accent(c: char) -> char {
    const FROM: &str = "àáâãäåāăąçćčďèéêëēėęěìíîïīįıñńňòóôõöøōőùúûüūůűųýÿžźżśšşřŕľĺłţťğ\
                        ÀÁÂÃÄÅĀĂĄÇĆČĎÈÉÊËĒĖĘĚÌÍÎÏĪĮİÑŃŇÒÓÔÕÖØŌŐÙÚÛÜŪŮŰŲÝŸŽŹŻŚŠŞŘŔĽĹŁŢŤĞ";
    const TO: &str = "aaaaaaaaacccdeeeeeeeeiiiiiiinnnooooooooouuuuuuuuyyzzzsssrrlllttg\
                      AAAAAAAAACCCDEEEEEEEEIIIIIIINNNOOOOOOOOUUUUUUUUYYZZZSSSRRLLLTTG";
    FROM.chars()
        .position(|f| f == c)
        .and_then(|i| TO.chars().nth(i))
        .unwrap_or(c)
}

/// Studio's `sameContactUrl`: the same place, ignoring the host's case and
/// a trailing slash.
pub fn same_contact_url(a: &str, b: &str) -> bool {
    let key = |raw: &str| match url::Url::parse(raw.trim()) {
        Ok(u) => {
            format!(
                "{}://{}{}{}",
                u.scheme(),
                u.host_str().unwrap_or("").to_ascii_lowercase(),
                u.port().map(|p| format!(":{p}")).unwrap_or_default(),
                u.path().trim_end_matches('/'),
            ) + &u.query().map(|q| format!("?{q}")).unwrap_or_default()
        }
        Err(_) => raw.trim().to_string(),
    };
    key(a) == key(b)
}

/// The contact already holding `url` (or `also`, e.g. the blyg's origin
/// when `url` is its author's page).
pub fn held<'a>(book: &'a [Contact], url: &str, also: Option<&str>) -> Option<&'a Contact> {
    book.iter().find(|c| {
        same_contact_url(&c.origin, url) || also.is_some_and(|o| same_contact_url(&c.origin, o))
    })
}

/// What an offer to add someone starts from: their name and address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactSeed {
    pub name: String,
    pub url: String,
}

impl ContactSeed {
    /// Studio's `authorSeed` / `blygContactSeed`: the author's own name and
    /// `https:` address when the post (or manifest) gives them, else the
    /// blyg's title and its site.
    pub fn from_author(
        author: Option<&crate::model::Author>,
        fallback_name: &str,
        site: &str,
    ) -> ContactSeed {
        let name = author
            .and_then(|a| a.name.as_deref())
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or(fallback_name)
            .to_string();
        let url = author
            .and_then(|a| a.url.as_deref())
            .map(str::trim)
            .filter(|u| u.starts_with("https:"))
            .unwrap_or(site)
            .to_string();
        ContactSeed { name, url }
    }

    /// The form's first values: a suggested handle, the name, the URL.
    pub fn input(&self) -> ContactInput {
        ContactInput {
            handle: suggest_handle(&self.name),
            name: self.name.clone(),
            url: self.url.clone(),
        }
    }
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
                    origin: "https://lin.example.org".into(),
                    ..Default::default()
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
            ..Default::default()
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

    // --- contacts --- managing the book.

    #[test]
    fn studio_contacts_parse_with_ids() {
        let v = json!({"contacts": [{
            "id": "c1", "handle": "lin", "name": "Lin", "url": "https://lin.example.org/",
            "ids": [
                {"id": "https://social.example.net/@lin", "provenance": "self"},
                {"id": "acct:lin@example.org", "provenance": "entered"}
            ],
            "created": "2030-01-01T00:00:00Z", "updated": "2030-01-01T00:00:00Z"
        }]});
        let c = &parse_contacts(&v)[0];
        assert_eq!(c.id, "c1");
        assert_eq!(c.origin, "https://lin.example.org/");
        assert_eq!(c.checked_ids(), ["https://social.example.net/@lin"]);
        assert_eq!(parse_contact(&v["contacts"][0]).as_ref(), Some(c));
        // A cache written before ids were kept still reads.
        let old: Contact =
            serde_json::from_str(r#"{"handle":"lin","origin":"https://lin.example.org"}"#).unwrap();
        assert_eq!(old.id, "");
        assert!(old.ids.is_empty());
    }

    #[test]
    fn input_is_checked_as_the_studio_does() {
        let ok = ContactInput {
            handle: " @lin ".into(),
            name: "Lin".into(),
            url: "https://lin.example.org".into(),
        };
        assert_eq!(ok.check(), Ok(()));
        assert_eq!(ok.tidy().handle, "lin");
        let bad = |h: &str, n: &str, u: &str| {
            ContactInput {
                handle: h.into(),
                name: n.into(),
                url: u.into(),
            }
            .check()
            .unwrap_err()
        };
        assert!(bad("", "Lin", "https://x.example").contains("needs"));
        assert!(bad("-lin", "Lin", "https://x.example").contains("letters, digits"));
        assert!(bad("lin.x", "Lin", "https://x.example").contains("letters, digits"));
        assert!(bad("lin", "Lin", "http://x.example").contains("https"));
        assert!(bad("lin", "Lin", "x.example").contains("absolute"));
        assert!(valid_handle("a"));
        assert!(valid_handle(&"a".repeat(39)));
        assert!(!valid_handle(&"a".repeat(40)));
    }

    #[test]
    fn handles_are_suggested_from_a_name() {
        assert_eq!(suggest_handle("Lin Okafor"), "lin");
        assert_eq!(suggest_handle("  Zoë  Ng"), "zoe");
        assert_eq!(suggest_handle("_Ada's notes"), "adas");
        assert_eq!(suggest_handle(""), "");
    }

    #[test]
    fn the_same_place_ignores_host_case_and_a_trailing_slash() {
        assert!(same_contact_url(
            "https://Lin.Example.org/",
            "https://lin.example.org"
        ));
        assert!(same_contact_url(
            "https://x.example/a/",
            "https://x.example/a"
        ));
        assert!(!same_contact_url(
            "https://x.example/a",
            "https://x.example/b"
        ));
        let book = [Contact {
            handle: "lin".into(),
            origin: "https://lin.example.org/".into(),
            ..Default::default()
        }];
        assert!(held(&book, "https://LIN.example.org", None).is_some());
        assert!(
            held(
                &book,
                "https://lin.example.org/about",
                Some("https://lin.example.org")
            )
            .is_some()
        );
        assert!(held(&book, "https://rue.example.org", None).is_none());
    }

    #[test]
    fn a_seed_prefers_the_authors_own_https_address() {
        let author = crate::model::Author {
            name: Some("Lin Okafor".into()),
            url: Some("https://lin.example.org/about".into()),
            ..Default::default()
        };
        let s = ContactSeed::from_author(Some(&author), "Lin's notes", "https://lin.example.org/");
        assert_eq!(s.name, "Lin Okafor");
        assert_eq!(s.url, "https://lin.example.org/about");
        assert_eq!(s.input().handle, "lin");
        let plain = crate::model::Author {
            url: Some("http://lin.example.org/".into()),
            ..Default::default()
        };
        let s = ContactSeed::from_author(Some(&plain), "Lin's notes", "https://lin.example.org/");
        assert_eq!(
            (s.name.as_str(), s.url.as_str()),
            ("Lin's notes", "https://lin.example.org/")
        );
    }
}
