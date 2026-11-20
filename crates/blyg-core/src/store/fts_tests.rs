//! Text-index upkeep (`store/fts.rs`).

use std::collections::HashMap;
use std::time::Instant;

use super::Store;
use crate::model::*;

/// A tiny deterministic PRNG (xorshift), so the synthetic text is the same
/// on every run.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A vocabulary of invented words, picked with a skew towards the first
/// ones (as real prose leans on its common words).
fn vocab(rng: &mut Rng) -> Vec<String> {
    let letters = b"etaoinshrdlcumwfgypbvkjxqz";
    (0..6000)
        .map(|_| {
            let n = 2 + rng.below(9) as usize;
            (0..n)
                .map(|_| {
                    // Skewed towards common letters.
                    let i = (rng.below(26) * rng.below(26) / 26) as usize;
                    letters[i] as char
                })
                .collect()
        })
        .collect()
}

fn prose(rng: &mut Rng, words: &[String], bytes: usize) -> String {
    let mut s = String::with_capacity(bytes + 16);
    while s.len() < bytes {
        let a = rng.below(words.len() as u64);
        let i = (a * rng.below(words.len() as u64) / words.len() as u64) as usize;
        s.push_str(&words[i]);
        s.push(if rng.below(14) == 0 { '\n' } else { ' ' });
    }
    s
}

fn row(i: usize, version: u32, body: String) -> ReadingItem {
    ReadingItem {
        subscription_id: format!("s{}", i % 50).into(),
        remote_id: format!("R{i}").into(),
        subscription_title: "A blyg".into(),
        origin: format!("https://b{}.blyg.example.com/", i % 50),
        kind: Kind::Fragment,
        state: "current".into(),
        version,
        created: None,
        updated: None,
        // One a second, in order: a merge of the whole list prunes nothing.
        observed_at: format!(
            "2026-01-01T{:02}:{:02}:{:02}Z",
            i / 3600,
            i / 60 % 60,
            i % 60
        ),
        content_md: body,
        content_html: String::new(),
        author: None,
        page: None,
        thumb: None,
        hoppers: vec![],
        pinned_version_retained: None,
        read_version: None,
        stub_of: None,
        forked_from: None,
        transclusions: vec![],
        lineage_known: false,
        source_url: None,
        generated: None,
        background: None,
    }
}

fn kinds() -> HashMap<SubId, SubscriptionKind> {
    (0..50)
        .map(|i| (format!("s{i}").into(), SubscriptionKind::Blyg))
        .collect()
}

fn sizes(s: &Store, t: &str) -> (i64, i64, i64) {
    let c = s.conn();
    let data: i64 = c
        .query_row(
            &format!("SELECT sum(length(block)) FROM {t}_data"),
            [],
            |r| r.get(0),
        )
        .unwrap();
    let content: i64 = c
        .query_row(
            &format!("SELECT sum(length(c0)) FROM {t}_content"),
            [],
            |r| r.get(0),
        )
        .unwrap();
    let pages: i64 = c
        .query_row(&format!("SELECT count(*) FROM {t}_data"), [], |r| r.get(0))
        .unwrap();
    (data, content, pages)
}

/// Write reading rows straight through SQL (the index triggers do the
/// work being measured), `chunk` rows a transaction like a pull's merge.
fn write_rows(s: &Store, rows: &[(usize, String)], chunk: usize) {
    for part in rows.chunks(chunk) {
        let mut c = s.conn();
        let tx = c.transaction().unwrap();
        for (i, body) in part {
            let json = serde_json::json!({ "content_md": body }).to_string();
            let n = tx
                .execute(
                    "UPDATE reading SET json = ?2 WHERE subscription_id = 's' AND remote_id = ?1",
                    rusqlite::params![format!("R{i}"), json],
                )
                .unwrap();
            if n == 0 {
                tx.execute(
                    "INSERT INTO reading (subscription_id, remote_id, origin, observed_at, state, version, json) \
                     VALUES ('s', ?1, 'https://b.blyg.example.com/', ?2, 'current', 1, ?3)",
                    rusqlite::params![format!("R{i}"), format!("{i:08}"), json],
                )
                .unwrap();
            }
        }
        tx.commit().unwrap();
    }
}

/// Timings and sizes on a 6,000-post reading list of ~11 KB posts:
/// `cargo test -p blyg-core --lib fts_bench -- --ignored --nocapture`
/// (`FTS_SECURE=1` measures FTS5 secure-delete, which Burrow leaves off:
/// see `store/fts.rs`; `FTS_ROUNDS` sets the churn).
#[test]
#[ignore = "bench"]
fn fts_bench() {
    let secure = std::env::var("FTS_SECURE").map_or(0, |v| v.parse().unwrap_or(0));
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("blygger.db")).unwrap();
    s.conn()
        .execute(
            "INSERT INTO reading_fts(reading_fts, rank) VALUES('secure-delete', ?1)",
            [secure],
        )
        .unwrap();
    eprintln!("fts secure-delete = {secure}");
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let words = vocab(&mut rng);
    let n = 6000;
    let rows: Vec<_> = (0..n)
        .map(|i| (i, prose(&mut rng, &words, 11_000)))
        .collect();
    let t = Instant::now();
    write_rows(&s, &rows, 100);
    eprintln!("first load {:?}", t.elapsed());
    let (d, c, p) = sizes(&s, "reading_fts");
    eprintln!(
        "loaded: index {d} text {c} ratio {:.2} pages {p}",
        d as f64 / c as f64
    );
    // Churn: each round edits 200 posts and replaces the 100 oldest.
    let mut next = n;
    let mut writes = std::time::Duration::ZERO;
    let rounds = std::env::var("FTS_ROUNDS").map_or(30, |v| v.parse().unwrap_or(30));
    for round in 0..rounds {
        let mut page: Vec<_> = (0..200)
            .map(|_| {
                let i = next - n + rng.below(n as u64) as usize;
                (i, prose(&mut rng, &words, 11_000))
            })
            .collect();
        page.extend((next..next + 100).map(|i| (i, prose(&mut rng, &words, 11_000))));
        let t = Instant::now();
        write_rows(&s, &page, 100);
        s.conn()
            .execute(
                "DELETE FROM reading WHERE rowid IN \
                 (SELECT rowid FROM reading ORDER BY observed_at LIMIT 100)",
                [],
            )
            .unwrap();
        writes += t.elapsed();
        next += 100;
        if round % 5 == 4 {
            let (d, c, p) = sizes(&s, "reading_fts");
            eprintln!(
                "round {round}: index {d} text {c} ratio {:.2} pages {p} (writes so far {writes:?})",
                d as f64 / c as f64,
            );
        }
    }
    let t = Instant::now();
    let w = super::fts::weigh(&s.conn(), "reading_fts").unwrap();
    eprintln!("weighing {:?}: {w:?}", t.elapsed());
    let q = format!("\"{}\"", words[3]);
    let matches = |label: &str| {
        let t = Instant::now();
        for _ in 0..20 {
            s.conn()
                .query_row(
                    "SELECT count(*) FROM reading_fts WHERE reading_fts MATCH ?1",
                    [&q],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap();
        }
        eprintln!("MATCH {label}: {:?} each", t.elapsed() / 20);
    };
    matches("before");
    // Upkeep as the writer runs it: force a compaction, then step it.
    s.set_meta("fts.reading_fts.compacting", "1").unwrap();
    let t = Instant::now();
    let (mut calls, mut steps, mut worst) = (0, 0, std::time::Duration::ZERO);
    let mut now = 1_900_000_000_000;
    while s.fts_compacting() {
        let c = Instant::now();
        steps += s.fts_upkeep(now).unwrap().steps;
        worst = worst.max(c.elapsed());
        calls += 1;
        now += 1;
    }
    eprintln!(
        "compaction: {:?} in {calls} calls, {steps} steps of {} pages, longest call {worst:?}",
        t.elapsed(),
        super::fts::STEP
    );
    let (d, c, p) = sizes(&s, "reading_fts");
    eprintln!(
        "compacted: index {d} text {c} ratio {:.2} pages {p}",
        d as f64 / c as f64
    );
    let t = Instant::now();
    s.conn()
        .execute(
            "INSERT INTO reading_fts(reading_fts) VALUES('optimize')",
            [],
        )
        .unwrap();
    eprintln!("a second optimize, all at once: {:?}", t.elapsed());
    matches("after");
}

fn ratio(s: &Store, t: &str) -> f64 {
    let (index, text) = super::fts::weigh(&s.conn(), t).unwrap();
    index as f64 / text as f64
}

/// The reading rows each query finds, through the index.
fn hits(s: &Store, queries: &[String]) -> Vec<Vec<i64>> {
    let c = s.conn();
    queries
        .iter()
        .map(|q| {
            let mut st = c
                .prepare("SELECT rowid FROM reading_fts WHERE reading_fts MATCH ?1 ORDER BY rowid")
                .unwrap();
            st.query_map([format!("\"{q}\"")], |r| r.get(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<i64>>>()
                .unwrap()
        })
        .collect()
}

fn integrity(s: &Store) {
    for t in ["reading_fts", "items_fts"] {
        s.conn()
            .execute(
                &format!("INSERT INTO {t}({t}, rank) VALUES ('integrity-check', 1)"),
                [],
            )
            .unwrap();
    }
}

/// An index bloated by churn is weighed, compacted in short steps, and
/// finds exactly what it found before; a lean one is left alone.
#[test]
fn bloated_index_is_compacted_in_steps_and_finds_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let (s, words, bloated) = bloated_store(dir.path());
    let queries: Vec<String> = words[..60]
        .iter()
        .filter(|w| w.len() >= 3)
        .cloned()
        .collect();
    let before = hits(&s, &queries);
    let mine = s.search("kettle");
    assert_eq!(mine.len(), 1);

    let now = 1_900_000_000_000;
    let first = s.fts_upkeep(now).unwrap();
    assert_eq!(first.weighed.len(), 2);
    assert_eq!(first.started, ["reading_fts"], "items_fts is lean");
    let mut calls = 1;
    let mut steps = first.steps;
    while s.fts_compacting() {
        let u = s.fts_upkeep(now + calls).unwrap();
        assert!(u.weighed.is_empty(), "weighed once per interval");
        steps += u.steps;
        calls += 1;
        assert!(calls < 10_000);
    }
    assert!(steps > 1, "in steps: {steps}");
    let after = ratio(&s, "reading_fts");
    assert!(after < bloated / 2.0, "{bloated:.2} → {after:.2}");
    assert!(after < super::fts::FIRST_BLOAT, "{after:.2}");
    integrity(&s);
    assert_eq!(hits(&s, &queries), before);
    assert_eq!(s.search("kettle"), mine);
    // Lean now: the next weighing starts nothing.
    let later = s.fts_upkeep(now + super::fts::WEIGH_EVERY_MS + 1).unwrap();
    assert_eq!(later.weighed.len(), 2);
    assert!(later.started.is_empty());
    assert_eq!(later.steps, 0);
}

/// Each upkeep call hands the store back soon (a step may overrun the
/// budget by one step's worth). Checked on the median: one disk stall on
/// a shared runner is the runner, not the code. A report, not a gate.
#[test]
#[ignore = "timing: run by the CI timing job"]
fn timing_compaction_calls_hand_the_store_back() {
    let dir = tempfile::tempdir().unwrap();
    let (s, _, _) = bloated_store(dir.path());
    let now = 1_900_000_000_000;
    s.fts_upkeep(now).unwrap();
    let mut calls = 1;
    let mut took = Vec::new();
    while s.fts_compacting() {
        let t = Instant::now();
        s.fts_upkeep(now + calls).unwrap();
        took.push(t.elapsed());
        calls += 1;
        assert!(calls < 10_000);
    }
    took.sort();
    let median = took[took.len() / 2];
    eprintln!(
        "compaction: median call {median:?}, worst {:?} over {} calls",
        took.last().unwrap(),
        took.len()
    );
    let mut budget = crate::wait::Budget::default();
    let ms = (super::fts::BUDGET * 4).as_secs_f64() * 1e3;
    budget.check("median upkeep call", median, ms);
    budget.done();
}

/// A store whose reading index churn has bloated (400 rows, each edited
/// three times over), with one post of your own; its vocabulary, and the
/// reading index's bloat ratio.
fn bloated_store(dir: &std::path::Path) -> (Store, Vec<String>, f64) {
    let s = Store::open(&dir.join("blygger.db")).unwrap();
    let k = kinds();
    let mut rng = Rng(42);
    let words = vocab(&mut rng);
    let n = 400;
    let rows: Vec<_> = (0..n)
        .map(|i| row(i, 1, prose(&mut rng, &words, 4_000)))
        .collect();
    s.merge_reading(&rows, false, &k).unwrap();
    s.create_draft(Kind::Fragment, "a harbour at dawn, kettle on", 0)
        .unwrap();
    let lean = ratio(&s, "reading_fts");
    // Churn: every row edited three times over.
    for v in 2..5 {
        let page: Vec<_> = (0..n)
            .map(|i| row(i, v, prose(&mut rng, &words, 4_000)))
            .collect();
        s.merge_reading(&page, false, &k).unwrap();
    }
    let bloated = ratio(&s, "reading_fts");
    assert!(
        bloated > super::fts::FIRST_BLOAT,
        "{lean:.2} → {bloated:.2}"
    );
    (s, words, bloated)
}

/// The upkeep runs on the typing writer's thread when it's idle, not on
/// the caller's (the UI's) and not on the sync worker.
#[test]
fn upkeep_runs_on_the_typing_writer() {
    let dir = tempfile::tempdir().unwrap();
    let opts = crate::SyncOptions {
        start_worker: false,
        debounce: std::time::Duration::from_millis(20),
        ..Default::default()
    };
    let b = crate::LiveBackend::open_with(
        dir.path(),
        "http://127.0.0.1:9",
        crate::api::auth::Credential::Token("t".into()),
        opts,
    )
    .unwrap();
    let store = Store::open(&dir.path().join(crate::live::DB_FILE)).unwrap();
    crate::wait::until("an upkeep run", || store.meta("fts.test.thread").is_some());
    assert_eq!(
        store.meta("fts.test.thread").as_deref(),
        Some("blyg-typing")
    );
    assert!(store.meta("fts.weighed_at").is_some());
    drop(b);
}
