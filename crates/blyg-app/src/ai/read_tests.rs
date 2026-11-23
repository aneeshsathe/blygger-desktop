//! What a generation read, in words (user testing of studio 0.71's screens):
//! a post never by its id, the "tells no one" said once, a page's site only
//! when it isn't its host, and each gap by its own number.

use blyg_core::{LocalId, PageSource, ProvenanceSource, ScopeProvenance};
use serde_json::json;

use super::*;

fn prov() -> ScopeProvenance {
    ScopeProvenance {
        model: "m".into(),
        sources: vec![
            ProvenanceSource {
                id: "01own".into(),
                version: Some(1),
                origin: None,
                cited: None,
                ..Default::default()
            },
            ProvenanceSource {
                id: "01far".into(),
                version: Some(2),
                origin: Some("https://ada.example.com".into()),
                cited: None,
                ..Default::default()
            },
        ],
        pages: vec![PageSource {
            url: "https://news.example.com/a".into(),
            cited: json!({"source": "Example News", "url": "https://news.example.com/a"}),
            ..Default::default()
        }],
        at: None,
        ..Default::default()
    }
}

#[test]
fn posts_then_pages_and_only_another_blygs_post_tells() {
    let name = |s: &ProvenanceSource| {
        (s.id == "01own").then(|| Named {
            title: "Fog notes".into(),
            open: Some(Open::Own(LocalId("L1".into()))),
        })
    };
    let e = entries(&prov(), &name, true);
    assert_eq!(e.len(), 3);
    assert_eq!(e[0].what, "drew on your post “Fog notes”");
    assert_eq!(
        e[0].link,
        Some(("“Fog notes”".into(), Open::Own(LocalId("L1".into())))),
        "linked to the post"
    );
    assert!(!e[0].tells);
    assert_eq!(mark(&e[0]), None, "no mark on what tells no one");
    assert_eq!(e[1].what, "drew on a post on ada.example.com");
    assert!(e[1].tells);
    assert_eq!(mark(&e[1]), Some(TELLS));
    assert_eq!(e[2].what, "read Example News");
    assert_eq!(e[2].url.as_deref(), Some("https://news.example.com/a"));
    assert!(!e[2].tells, "a web page never tells");
    assert_eq!(mark(&e[2]), None);
}

/// Finding 2: never "[[01k…]]".
#[test]
fn a_post_not_held_here_is_named_by_its_blyg_never_its_id() {
    let e = entries(&prov(), &|_| None, true);
    assert_eq!(e[0].what, "drew on a post of this blyg");
    assert_eq!(e[1].what, "drew on a post on ada.example.com");
    for x in &e {
        assert!(!x.what.contains("[["), "{}", x.what);
        assert!(!x.what.contains("01"), "{}", x.what);
    }
    // Held here: its title, linked to its page, and its blyg.
    let named = |s: &ProvenanceSource| {
        (s.id == "01far").then(|| Named {
            title: "Tides".into(),
            open: Some(Open::Web("https://ada.example.com/f/01far/".into())),
        })
    };
    let e = entries(&prov(), &named, true);
    assert_eq!(e[1].what, "drew on “Tides” (ada.example.com)");
    assert_eq!(
        e[1].link,
        Some((
            "“Tides”".into(),
            Open::Web("https://ada.example.com/f/01far/".into())
        ))
    );
}

/// Finding 7: "read site0.example.org https://site0.example.org/…" says
/// its host twice.
#[test]
fn a_page_names_its_site_only_when_that_isnt_its_host() {
    let p = |url: &str, cited: serde_json::Value| PageSource {
        url: url.into(),
        cited,
        ..Default::default()
    };
    assert_eq!(
        page(&p("https://site0.example.org/a", json!({}))).what,
        "read"
    );
    assert_eq!(
        page(&p(
            "https://www.site0.example.org/a",
            json!({"source": "site0.example.org"})
        ))
        .what,
        "read"
    );
    assert_eq!(
        page(&p(
            "https://news.example.com/a",
            json!({"source": "Example News"})
        ))
        .what,
        "read Example News"
    );
}

#[test]
fn only_scopes_that_read_something_are_listed_by_their_own_number() {
    let bare = ScopeProvenance {
        sources: vec![],
        pages: vec![],
        ..prov()
    };
    let all = vec![None, Some(bare), Some(prov())];
    let listed = scopes(&all, &|_| None, true);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].0, 2);
    assert_eq!(scope_label(listed[0].0), "TK 3");
}

/// Finding 6: said once, under the heading.
#[test]
fn tells_no_one_is_one_line() {
    assert_eq!(TELLS_NO_ONE, "Web pages and your own posts tell no one.");
}
