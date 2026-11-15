//! --- discover --- The fake's blygger.com directory: invented blygs, never
//! the network. Built through `blyg_core::discover::parse_directory`, so
//! the sample goes through the same validation as the real file.
//!
//! Rue is followed already (the sample subscriptions), and the fake's own
//! blyg is listed too: neither is offered. Seven others are, newest listing
//! first, five at a time; the mark ([`SEEN_AT`]) makes the three listed in
//! the last week new.

use blyg_core::discover::{DirectoryBlyg, parse_directory};
use chrono::{Duration, Utc};
use serde_json::{Value, json};

/// Where the fake's Discover mark starts: ten days ago.
pub fn seen_at() -> String {
    (Utc::now() - Duration::days(10)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn ago(days: i64) -> String {
    (Utc::now() - Duration::days(days)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn row(host: &str, title: &str, listed: i64, level: Option<&str>, last: Option<i64>) -> Value {
    json!({
        "origin": format!("https://{host}/"),
        "title": title,
        "home": format!("https://{host}/"),
        "feed": format!("https://{host}/feed.xml"),
        "listedAt": ago(listed),
        "protocol": "0.4",
        "protocolLevel": 2,
        "generator": "blygger-studio/0.61.0",
        "activity": {
            "level": level,
            "score": 1.0,
            "events": if level.is_some() { 6 } else { 0 },
            "lastPost": last.map(ago),
        },
    })
}

/// The sample directory, in its own order (most active first).
pub fn directory() -> Vec<DirectoryBlyg> {
    let doc = json!({
        "version": 1,
        "directory": "https://blygger.com/",
        "blygs": [
            row("rue.blyg.example.com", "Rue", 40, Some("high"), Some(0)),
            row("tidepool.blyg.example.com", "Tide Pool Diary", 2, Some("high"), Some(1)),
            row("blyg.example.com", "This blyg", 90, Some("high"), Some(0)),
            row("lantern.blyg.example.com", "Lantern Hours", 5, Some("medium"), Some(3)),
            row("fieldnotes.blyg.example.com", "Field Notes from a Small Garden", 30, Some("medium"), Some(12)),
            row("quiet.blyg.example.com", "Quiet Harbour", 8, None, None),
            row("slowbread.blyg.example.com", "Slow Bread", 45, Some("low"), Some(40)),
            row("orbit.blyg.example.com", "Orbit & Ink", 60, Some("low"), Some(70)),
            row("moss.blyg.example.com", "", 120, Some("low"), Some(200)),
            // Dropped: not https, and no listing date.
            row("plain.blyg.example.com", "Plain", 1, None, None)
                .as_object()
                .map(|o| {
                    let mut o = o.clone();
                    o.insert("origin".into(), json!("http://plain.blyg.example.com/"));
                    Value::Object(o)
                })
                .unwrap_or_default(),
            json!({ "origin": "https://undated.blyg.example.com/", "title": "Undated" }),
        ],
    });
    parse_directory(&doc).unwrap_or_default()
}
