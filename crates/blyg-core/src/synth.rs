//! A synthetic reading list at scale, for the performance tests and
//! benchmarks (`tests/perf.rs`, the app's list tests). Invented content
//! only: `blyg-NN.example` origins, filler text. Deterministic for a seed.
//!
//! The shape follows a real-world list after a blyg imported its archive
//! at once: thousands of posts over a few dozen subscriptions, a few
//! percent RSS, a handful withdrawn, most a few KB of Markdown plus its
//! HTML, and a few very large posts (~150 KB).

use std::collections::HashMap;

use crate::model::{Author, Kind, ReadingItem, Subscription, SubscriptionKind};

/// How many subscriptions the list spreads over.
pub const SUBSCRIPTIONS: usize = 34;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

const WORDS: &[&str] = &[
    "small", "tools", "used", "daily", "beat", "big", "rarely", "garden", "notes", "river",
    "lantern", "quiet", "morning", "reading", "letters", "paper", "window", "kettle", "orchard",
    "harbour", "thread", "pattern", "measure", "slowly", "compass", "meadow", "copper", "signal",
];

fn sentence(rng: &mut Lcg, words: usize) -> String {
    let mut s = String::new();
    for i in 0..words {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(WORDS[rng.below(WORDS.len() as u64) as usize]);
    }
    s
}

fn body(rng: &mut Lcg, target: usize) -> String {
    let mut md = format!("# {}\n\n", sentence(rng, 4));
    while md.len() < target {
        md.push_str(&sentence(rng, 40));
        md.push_str(".\n\n");
    }
    md
}

fn sub_id(i: usize) -> String {
    format!("S{i:02}")
}

fn origin(i: usize) -> String {
    format!("https://blyg-{i:02}.example")
}

fn kind_of(i: usize) -> SubscriptionKind {
    // Two of the subscriptions are plain feeds.
    if i % 17 == 16 {
        SubscriptionKind::Rss
    } else {
        SubscriptionKind::Blyg
    }
}

/// The subscriptions the generated rows belong to.
pub fn subscriptions() -> Vec<Subscription> {
    (0..SUBSCRIPTIONS)
        .map(|i| Subscription {
            id: sub_id(i),
            kind: kind_of(i),
            origin: origin(i),
            feed_url: format!("{}/feed.xml", origin(i)),
            title: format!("Blyg {i:02}"),
            status: "active".into(),
            in_blogroll: true,
            title_follows_source: true,
            poll: Default::default(),
        })
        .collect()
}

/// `subscription id → kind`, as `Store::merge_reading` takes it.
pub fn kinds() -> HashMap<String, SubscriptionKind> {
    (0..SUBSCRIPTIONS)
        .map(|i| (sub_id(i), kind_of(i)))
        .collect()
}

/// `n` reading rows, newest first by `observed_at`. Every 1000th row is
/// a ~150 KB post; 1 in 130 is withdrawn; 1 in 3 is read.
pub fn reading(n: usize, seed: u64) -> Vec<ReadingItem> {
    let mut rng = Lcg(seed ^ 0x9e37_79b9_7f4a_7c15);
    (0..n)
        .map(|i| {
            // One blyg holds about half the list (the big import).
            let s = if rng.below(2) == 0 {
                3
            } else {
                rng.below(SUBSCRIPTIONS as u64) as usize
            };
            let size = if i % 1000 == 999 {
                150_000
            } else {
                1_000 + rng.below(6_000) as usize
            };
            let md = body(&mut rng, size);
            let html = format!("<p>{}</p>", md.replace("\n\n", "</p><p>"));
            let tomb = i % 130 == 129;
            let secs = 2_000_000_000 - (i as i64) * 3_600;
            let at = chrono_free_iso(secs);
            let version = 1 + rng.below(3) as u32;
            ReadingItem {
                subscription_id: sub_id(s),
                remote_id: format!("r{i:06}"),
                subscription_title: format!("Blyg {s:02}"),
                origin: origin(s),
                kind: if i % 5 == 0 {
                    Kind::Thread
                } else {
                    Kind::Fragment
                },
                state: if tomb { "tombstone" } else { "current" }.into(),
                version,
                created: Some(at.clone()),
                updated: Some(at.clone()),
                observed_at: at,
                content_md: if tomb { String::new() } else { md },
                content_html: if tomb { String::new() } else { html },
                author: Some(Author {
                    name: Some(format!("Author {s:02}")),
                    url: Some(origin(s)),
                    extra: Default::default(),
                }),
                page: Some(format!("{}/p/r{i:06}", origin(s))),
                thumb: None,
                hoppers: vec![],
                pinned_version_retained: None,
                read_version: (i % 3 == 0).then_some(version),
                stub_of: None,
                forked_from: None,
                transclusions: vec![],
                lineage_known: true,
                source_url: None,
                generated: None,
            }
        })
        .collect()
}

/// `secs` since the epoch as `YYYY-MM-DDTHH:MM:SSZ` (civil-from-days).
fn chrono_free_iso(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn iso_dates() {
        assert_eq!(super::chrono_free_iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(
            super::chrono_free_iso(2_000_000_000),
            "2033-05-18T03:33:20Z"
        );
    }

    #[test]
    fn deterministic() {
        assert_eq!(super::reading(20, 7), super::reading(20, 7));
    }
}
