//! Discover's rules, ported from the studio's `test-ui/discover.test.ts`
//! (v0.61.0) where they agree, plus Burrow's stricter URL check and the
//! fetch against a localhost server (never blygger.com).

use std::io::{Read, Write};
use std::net::TcpListener;

use serde_json::{Value, json};

use super::*;
use crate::model::{Subscription, SubscriptionKind};

const FIXTURE: &str = include_str!("../../tests/fixtures/discover/blygs.json");

fn fixture() -> Vec<DirectoryBlyg> {
    parse_directory(&serde_json::from_str(FIXTURE).unwrap()).unwrap()
}

fn origins<'a>(v: impl IntoIterator<Item = &'a DirectoryBlyg>) -> Vec<&'a str> {
    v.into_iter().map(|b| b.origin.as_str()).collect()
}

fn sub(origin: &str, feed: &str) -> Subscription {
    Subscription {
        id: format!("sub-{origin}"),
        kind: SubscriptionKind::Blyg,
        origin: origin.into(),
        feed_url: feed.into(),
        title: String::new(),
        status: "active".into(),
        in_blogroll: false,
        title_follows_source: true,
        poll: Default::default(),
    }
}

fn row(origin: &str, listed_at: &str) -> Value {
    json!({
        "origin": origin, "title": "A Blyg", "home": origin, "feed": format!("{origin}feed.xml"),
        "listedAt": listed_at,
        "activity": { "level": "high", "score": 4.2, "events": 6, "lastPost": "2026-11-12T10:00:00.000Z" },
    })
}

#[test]
fn parse_keeps_usable_rows_in_directory_order_and_drops_the_rest() {
    let blygs = fixture();
    assert_eq!(
        origins(&blygs),
        [
            "https://busy.blyg.example.com/",
            "https://www.Followed.blyg.example.com/",
            "https://quiet.blyg.example.com/",
            "https://older.blyg.example.com/blyg/",
            "https://self.blyg.example.com/",
        ]
    );
    assert_eq!(blygs[2].title, "Quiet Blyg", "trimmed");
    // A row with no title, a script home, no feed and an unknown level:
    // filled in from what it has, never guessed.
    assert_eq!(
        blygs[3],
        DirectoryBlyg {
            origin: "https://older.blyg.example.com/blyg/".into(),
            title: "older.blyg.example.com".into(),
            home: "https://older.blyg.example.com/blyg/".into(),
            feed: None,
            listed_at: "2026-08-01T00:00:00.000Z".into(),
            listed_ms: parse_ts_ms("2026-08-01T00:00:00.000Z").unwrap(),
            activity: Activity {
                level: None,
                events: 2,
                last_post: None
            },
        }
    );
    assert_eq!(blygs[0].activity.level, Some(ActivityLevel::High));
    assert_eq!(blygs[0].home_label(), "busy.blyg.example.com");
    assert_eq!(blygs[3].home_label(), "older.blyg.example.com/blyg");
}

#[test]
fn parse_refuses_what_is_not_the_directory() {
    for bad in [
        json!({ "nope": true }),
        json!({ "blygs": "x" }),
        json!([]),
        json!(null),
        json!("blygs"),
    ] {
        assert_eq!(parse_directory(&bad), Err(DiscoverError::NotDirectory));
    }
    assert_eq!(
        DiscoverError::NotDirectory.to_string(),
        "blygger.com answered with something that is not its directory."
    );
    assert_eq!(parse_directory(&json!({ "blygs": [] })), Ok(vec![]));
}

#[test]
fn only_https_urls_with_a_host_are_taken() {
    for ok in [
        "https://a.blyg.example.com/",
        "https://a.blyg.example.com:8443/x/",
    ] {
        assert_eq!(safe_url(&json!(ok)).as_deref(), Some(ok));
    }
    for bad in [
        json!("http://a.blyg.example.com/"),
        json!("javascript:alert(1)"),
        json!("data:text/html,hi"),
        json!("file:///etc/passwd"),
        json!("blob:https://a.blyg.example.com/x"),
        json!("ftp://a.blyg.example.com/"),
        json!("https://"),
        json!("a.blyg.example.com"),
        json!(""),
        json!(42),
        json!(null),
    ] {
        assert_eq!(safe_url(&bad), None, "{bad}");
    }
    // A bad home or feed falls back (home to the origin) or drops (feed).
    let b = &parse_directory(&json!({ "blygs": [
        { "origin": "https://a.blyg.example.com/", "home": "http://a.blyg.example.com/", "feed": "javascript:x", "listedAt": "2026-10-01" },
    ] }))
    .unwrap()[0];
    assert_eq!(b.home, "https://a.blyg.example.com/");
    assert_eq!(b.feed, None);
    assert_eq!(b.title, "a.blyg.example.com");
}

#[test]
fn newest_listing_first_and_the_directory_order_breaks_ties() {
    let blygs = fixture();
    assert_eq!(
        origins(by_listing(&blygs)),
        [
            "https://quiet.blyg.example.com/",
            "https://www.Followed.blyg.example.com/",
            "https://busy.blyg.example.com/",
            "https://older.blyg.example.com/blyg/",
            "https://self.blyg.example.com/",
        ]
    );
    let tie = parse_directory(&json!({ "blygs": [
        row("https://a.example.com/", "2026-10-01T00:00:00.000Z"),
        row("https://b.example.com/", "2026-11-01T00:00:00.000Z"),
        row("https://c.example.com/", "2026-10-01T00:00:00Z"),
    ] }))
    .unwrap();
    assert_eq!(
        origins(by_listing(&tie)),
        [
            "https://b.example.com/",
            "https://a.example.com/",
            "https://c.example.com/"
        ]
    );
}

#[test]
fn new_means_listed_after_the_newest_listing_already_shown() {
    let v = parse_directory(&json!({ "blygs": [
        row("https://a.example.com/", "2026-10-01T00:00:00.000Z"),
        row("https://b.example.com/", "2026-11-01T00:00:00.000Z"),
    ] }))
    .unwrap();
    let (older, newer) = (&v[0], &v[1]);
    assert!(is_new(older, ""));
    assert!(!is_new(older, "2026-10-15T00:00:00.000Z"));
    assert!(is_new(newer, "2026-10-15T00:00:00.000Z"));
    assert!(!is_new(newer, "not a date"));
    assert_eq!(next_seen_at(&v, ""), "2026-11-01T00:00:00.000Z");
    // Never backwards, even when the directory drops its newest listing.
    assert_eq!(
        next_seen_at(&v[..1], "2026-11-01T00:00:00.000Z"),
        "2026-11-01T00:00:00.000Z"
    );
    // Written the studio's way (`toISOString`), so a mark without
    // milliseconds is rewritten once.
    assert_eq!(
        next_seen_at(&v[..1], "2026-11-01T00:00:00Z"),
        "2026-11-01T00:00:00.000Z"
    );
    assert_eq!(next_seen_at(&[], ""), "");
    assert_eq!(next_seen_at(&[], "garbage"), "garbage");
    assert_eq!(next_seen_at(&v, "garbage"), "2026-11-01T00:00:00.000Z");
    // The badge: never with no mark yet (everything would be new), never
    // on a followed blyg.
    assert!(!badge_new(newer, None, false));
    assert!(!badge_new(newer, Some(""), false));
    assert!(badge_new(newer, Some("2026-10-15T00:00:00.000Z"), false));
    assert!(!badge_new(newer, Some("2026-10-15T00:00:00.000Z"), true));
    assert!(!badge_new(older, Some("2026-10-15T00:00:00.000Z"), false));
}

#[test]
fn followed_matches_by_origin_or_feed_under_the_opml_spelling_rules() {
    let b = parse_directory(
        &json!({ "blygs": [row("https://www.a.example.com/", "2026-10-01T00:00:00.000Z")] }),
    )
    .unwrap()
    .remove(0);
    let keys = |subs: &[Subscription]| subscribed_keys(subs);
    assert!(is_subscribed(
        &b,
        &keys(&[sub(
            "https://A.example.com",
            "https://a.example.com/feed.xml"
        )])
    ));
    // By feed alone, whatever the scheme.
    assert!(is_subscribed(
        &b,
        &keys(&[sub(
            "https://other.example.com/",
            "http://a.example.com/feed.xml"
        )])
    ));
    // Following one hopper is not following the blyg.
    assert!(!is_subscribed(
        &b,
        &keys(&[sub(
            "https://a.example.com/",
            "https://a.example.com/h/garden/feed.xml"
        )])
    ));
    assert!(!is_subscribed(&b, &keys(&[])));
    assert!(is_self(&b, Some("https://a.example.com")));
    assert!(!is_self(&b, Some("https://a.example.com/blyg/")));
    assert!(!is_self(&b, None));
}

#[test]
fn the_list_is_the_unfollowed_newest_first_keeping_rows_touched_this_visit() {
    let blygs = fixture();
    let keys = subscribed_keys(&[sub(
        "https://followed.blyg.example.com/",
        "https://followed.blyg.example.com/feed.xml",
    )]);
    let none = |_: &DirectoryBlyg| false;
    let list = unfollowed(&blygs, &keys, &none);
    assert_eq!(
        origins(list),
        [
            "https://quiet.blyg.example.com/",
            "https://busy.blyg.example.com/",
            "https://older.blyg.example.com/blyg/",
            "https://self.blyg.example.com/",
        ]
    );
    // Subscribed during this visit: it stays, saying so.
    let keep = |b: &DirectoryBlyg| b.origin.contains("Followed");
    assert_eq!(unfollowed(&blygs, &keys, &keep).len(), 5);
}

#[test]
fn the_activity_line_says_how_active_and_how_recent() {
    let now = parse_ts_ms("2026-11-14T12:00:00.000Z").unwrap();
    let mut b = parse_directory(
        &json!({ "blygs": [row("https://a.example.com/", "2026-10-01T00:00:00.000Z")] }),
    )
    .unwrap()
    .remove(0);
    assert_eq!(activity_line(&b, now), "last post 2 days ago");
    b.activity.last_post = None;
    assert_eq!(activity_line(&b, now), "high activity");
    b.activity.level = None;
    assert_eq!(activity_line(&b, now), "activity not measured yet");
    let at = |d: &str| ago(d, now);
    assert_eq!(at("2026-11-14T01:00:00.000Z"), "today");
    assert_eq!(at("2026-11-13T01:00:00.000Z"), "yesterday");
    assert_eq!(at("2026-10-01T00:00:00.000Z"), "a month ago");
    assert_eq!(at("2026-06-01T00:00:00.000Z"), "5 months ago");
    assert_eq!(at("2025-10-01T00:00:00.000Z"), "a year ago");
    assert_eq!(at("2023-10-01T00:00:00.000Z"), "3 years ago");
}

#[test]
fn release_builds_read_blygger_com() {
    assert_eq!(DIRECTORY_URL, "https://blygger.com/blygs.json");
    if std::env::var_os(URL_ENV).is_none() {
        assert_eq!(directory_url(), DIRECTORY_URL);
    }
}

// ------------------------------------------------- fetch, from localhost

/// Serve one HTTP response on 127.0.0.1 and return the URL.
fn serve_once(status: &str, body: Vec<u8>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let status = status.to_string();
    std::thread::spawn(move || {
        if let Ok((mut s, _)) = l.accept() {
            let mut buf = [0u8; 4096];
            let mut got = Vec::new();
            while !got.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => got.extend_from_slice(&buf[..n]),
                }
            }
            let head = String::from_utf8_lossy(&got).to_ascii_lowercase();
            // Token-less: nothing of the owner's ever goes to the directory.
            assert!(!head.contains("authorization"), "{head}");
            assert!(!head.contains("cookie"), "{head}");
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(&body);
        }
    });
    format!("http://127.0.0.1:{port}/blygs.json")
}

#[test]
fn fetch_reads_and_parses_the_directory() {
    let url = serve_once("200 OK", FIXTURE.as_bytes().to_vec());
    assert_eq!(fetch(&url).unwrap(), fixture());
}

#[test]
fn fetch_errors_are_the_studios_words() {
    let url = serve_once("503 Service Unavailable", b"{}".to_vec());
    let e = fetch(&url).unwrap_err();
    assert_eq!(e, DiscoverError::Status(503));
    assert_eq!(
        e.to_string(),
        "blygger.com answered 503. Try again in a moment."
    );
    let url = serve_once("200 OK", b"<html>not json</html>".to_vec());
    assert_eq!(fetch(&url), Err(DiscoverError::NotDirectory));
    let url = serve_once("200 OK", b"{\"version\":1}".to_vec());
    assert_eq!(fetch(&url), Err(DiscoverError::NotDirectory));
    // Larger than the cap: refused, not read whole.
    let mut big = b"{\"blygs\":[".to_vec();
    big.resize(MAX_BYTES as usize + 10, b' ');
    let url = serve_once("200 OK", big);
    assert_eq!(fetch(&url), Err(DiscoverError::NotDirectory));
    // Nothing listening.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let e = fetch(&format!("http://127.0.0.1:{port}/blygs.json")).unwrap_err();
    assert_eq!(e, DiscoverError::Unreachable);
    assert_eq!(e.to_string(), "Could not reach blygger.com.");
    assert_eq!(
        CoreError::from(DiscoverError::Unreachable).to_string(),
        "Could not reach blygger.com."
    );
}
