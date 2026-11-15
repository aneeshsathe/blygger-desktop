//! A contract test of Burrow in a browser against a REAL local studio
//! (`wrangler dev`): its Rust read path (`Pager` → `live_items` /
//! `live_reading` → `reading_view`), run natively over HTTP exactly as
//! `www/live.js` drives it, must list Reading in native Burrow's order
//! (`LiveBackend` → `blyg_core::own::merge` → `vm::order`), the one order of
//! `blyg_core::order`.
//!
//! `#[ignore]`d: run it with `scripts/e2e-local.sh`, which starts two
//! Workers and sets `BLYG_E2E_URL`, `BLYG_E2E_TOKEN`, `BLYG_E2E_URL_B` and
//! `BLYG_E2E_TOKEN_B`. It publishes on both: never point it at a real blyg.

use std::time::{Duration, Instant};

use blyg_core::api::Api;
use blyg_core::api::auth::Credential;
use blyg_core::{Backend, Kind, LiveBackend, LocalId, SyncOptions};
use blyg_web::Burrow;
use blyg_web::live::Pager;
use serde_json::Value;

fn local(var: &str) -> String {
    let url = std::env::var(var)
        .unwrap_or_else(|_| panic!("{var} not set: run scripts/e2e-local.sh"))
        .trim_end_matches('/')
        .to_string();
    assert!(
        url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost"),
        "e2e tests only ever run against a local Worker, not {url}"
    );
    url
}

fn token(var: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| panic!("{var} not set: run scripts/e2e-local.sh"))
}

fn backend(dir: &std::path::Path, url: &str, tok: &str) -> LiveBackend {
    let opts = SyncOptions {
        start_worker: false,
        reading_pages: 50,
        reading_gap: Duration::ZERO,
        ..SyncOptions::default()
    };
    LiveBackend::open_with(dir, url, Credential::Token(tok.to_string()), opts)
        .expect("open LiveBackend")
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .build()
}

/// `GET path` as the page's `call` does it: (status, body, Retry-After).
fn get(base: &str, tok: &str, path: &str) -> (u16, String, Option<String>) {
    let res = agent()
        .get(&format!("{base}{path}"))
        .set("authorization", &format!("Bearer {tok}"))
        .call();
    match res {
        Ok(r) => {
            let ra = r.header("retry-after").map(str::to_string);
            (r.status(), r.into_string().unwrap_or_default(), ra)
        }
        Err(ureq::Error::Status(c, r)) => {
            let ra = r.header("retry-after").map(str::to_string);
            (c, r.into_string().unwrap_or_default(), ra)
        }
        Err(e) => panic!("GET {path}: {e}"),
    }
}

/// `readAll` in `www/live.js`: every page of a cursor-paged list.
fn read_all(base: &str, tok: &str, path: &str) -> String {
    let mut p = Pager::new(path, "", 100);
    while let Some(next) = p.next_path() {
        let (st, body, ra) = get(base, tok, &next);
        let err = p.feed(st, &body, ra);
        assert_eq!(err, "null", "GET {next}: {err}");
    }
    p.items()
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as f64
}

/// Burrow in a browser, started and refreshed as `www/live.js` does.
struct Web {
    b: Burrow,
    base: String,
    tok: String,
}

impl Web {
    fn start(base: &str, tok: &str) -> Web {
        let (st, settings, _) = get(base, tok, "/api/settings");
        assert_eq!(st, 200, "{settings}");
        let public = Burrow::public_base(&settings, base);
        let mut w = Web {
            b: Burrow::new_live(now_ms(), None, None, &public),
            base: base.into(),
            tok: tok.into(),
        };
        w.refresh();
        w
    }

    /// A whole refresh: the items, then the reading list and hoppers.
    fn refresh(&mut self) {
        let items = read_all(&self.base, &self.tok, "/api/items");
        let out: Value = serde_json::from_str(&self.b.live_items(&items)).unwrap();
        assert!(out.get("error").is_none(), "{out}");
        let rows = read_all(&self.base, &self.tok, "/api/reading/imported");
        let hops = read_all(&self.base, &self.tok, "/api/hoppers");
        self.b.live_reading(&rows, &hops);
    }

    /// The Reading stream's keys, in the order shown.
    fn reading(&self) -> Vec<String> {
        let v: Value = serde_json::from_str(&self.b.reading_view(now_ms(), "", false)).unwrap();
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["key"].as_str().unwrap().to_string())
            .collect()
    }

    /// The Posts list's ids (server ids, live), in the order shown.
    fn posts(&self) -> Vec<String> {
        let v: Value = serde_json::from_str(&self.b.view(now_ms())).unwrap();
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect()
    }
}

/// Native Burrow's Reading list (`reading/own.rs` `reading_with_own`, less
/// the hopper-only and withdrawn rows this test makes none of), as keys.
fn native_reading(me: &LiveBackend) -> Vec<String> {
    me.sync_now().unwrap();
    let origin = me.base_url().expect("a live backend has a base");
    let own = blyg_core::own::rows(&me.items(), &origin);
    let rows = blyg_web::reading::vm::order(blyg_core::own::merge(me.reading(), own));
    rows.iter()
        .filter(|r| !blyg_web::reading::sources_vm::is_background(r))
        .map(|r| format!("{}|{}", r.subscription_id, r.remote_id))
        .collect()
}

fn wait_until(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !f() {
        assert!(t0.elapsed() < timeout, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn sid(b: &LiveBackend, id: &LocalId) -> String {
    b.item(id).and_then(|i| i.server_id).expect("a server id").0
}

fn publish(b: &LiveBackend, text: &str) -> (LocalId, String) {
    let id = b.create_draft(Kind::Fragment, text).unwrap();
    b.sync_now().unwrap();
    b.publish(&id, None).expect("publish");
    let s = sid(b, &id);
    (id, s)
}

fn tag() -> String {
    format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    )
}

#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn web_reading_and_posts_follow_native_order() {
    let (url_a, url_b) = (local("BLYG_E2E_URL"), local("BLYG_E2E_URL_B"));
    let tok_a = token("BLYG_E2E_TOKEN");
    let tok_b = std::env::var("BLYG_E2E_TOKEN_B").unwrap_or_else(|_| tok_a.clone());
    let (d1, d2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let me = backend(d1.path(), &url_a, &tok_a);
    let source = backend(d2.path(), &url_b, &tok_b);
    let t = tag();

    // `b` publishes 30 posts, a few to each second (ties included).
    let mut posts = Vec::new();
    for i in 0..30 {
        posts.push(publish(&source, &format!("Sample note {i:02} ({t})")));
        if i % 3 == 2 {
            std::thread::sleep(Duration::from_millis(1100));
        }
    }

    // `a` subscribes: the backfill imports them all in about one second.
    let sub = me.subscribe(&url_b, Some("Sample notes")).unwrap();
    let held = |me: &LiveBackend| {
        me.sync_now().unwrap();
        let reading = me.reading();
        posts
            .iter()
            .all(|(_, rid)| reading.iter().any(|r| &r.remote_id == rid))
    };
    wait_until("the backfill", Duration::from_secs(60), || held(&me));

    let mut web = Web::start(&url_a, &tok_a);
    let native = native_reading(&me);
    let shown = web.reading();
    assert_eq!(shown, native, "Burrow web lists Reading as native Burrow");
    // Newest first by the posts' own dates, whatever the import order.
    let ours: Vec<&String> = shown
        .iter()
        .filter(|k| k.starts_with(&format!("{}|", sub.id)))
        .collect();
    assert_eq!(ours.len(), 30);
    let want: Vec<String> = posts
        .iter()
        .rev()
        .map(|(_, rid)| format!("{}|{rid}", sub.id))
        .collect();
    let mut by_date = me.reading();
    by_date.retain(|r| r.subscription_id == sub.id);
    blyg_core::order::sort_reading(&mut by_date);
    let by_date: Vec<String> = by_date
        .iter()
        .map(|r| format!("{}|{}", r.subscription_id, r.remote_id))
        .collect();
    assert_eq!(ours, by_date.iter().collect::<Vec<_>>());
    // Published one after another: at worst ties within a second, never
    // an older post above a newer one's second.
    let pos = |k: &String| ours.iter().position(|x| *x == k).unwrap();
    assert!(pos(&want[0]) < 3, "the newest post is in the top second");
    assert!(
        pos(&want[29]) >= 27,
        "the oldest post is in the last second"
    );

    // `b` republishes an old post; once `a` polls, it tops both lists.
    let (old_id, old_rid) = &posts[4];
    std::thread::sleep(Duration::from_millis(1100));
    source
        .save(old_id, &format!("Sample note 04, revised ({t})"))
        .unwrap();
    source.sync_now().unwrap();
    source.publish(old_id, Some("revised")).unwrap();
    let api = Api::new(&url_a, Credential::Token(tok_a.clone()));
    let key = format!("{}|{old_rid}", sub.id);
    wait_until("the republish to arrive", Duration::from_secs(60), || {
        api.resync_subscription(&sub.id).unwrap();
        me.sync_now().unwrap();
        me.reading()
            .iter()
            .any(|r| &r.remote_id == old_rid && r.version >= 2)
    });
    web.refresh();
    let shown = web.reading();
    assert_eq!(shown[0], key, "the republished post moves to the top");
    assert_eq!(shown, native_reading(&me));

    // `a` publishes three posts inside one second: `updated DESC, id DESC`
    // in Posts and among the Reading rows, on the web as natively.
    let mut mine = Vec::new();
    for attempt in 0..5 {
        // Start just after a second turns over.
        let ms = now_ms() as u64 % 1000;
        std::thread::sleep(Duration::from_millis(1000 - ms + 20));
        let ids: Vec<LocalId> = (0..3)
            .map(|i| {
                me.create_draft(Kind::Fragment, &format!("Own post {i} ({t}, {attempt})"))
                    .unwrap()
            })
            .collect();
        me.sync_now().unwrap();
        for id in &ids {
            me.publish(id, None).expect("publish");
        }
        me.sync_now().unwrap();
        let updated: Vec<String> = ids.iter().map(|i| me.item(i).unwrap().updated).collect();
        if updated.iter().all(|u| *u == updated[0]) {
            mine = ids.iter().map(|i| sid(&me, i)).collect();
            break;
        }
    }
    assert_eq!(mine.len(), 3, "three publishes landed in one second");
    let mut want = mine.clone();
    want.sort_by(|a, b| b.cmp(a));
    web.refresh();
    let posts_shown: Vec<String> = web
        .posts()
        .into_iter()
        .filter(|id| mine.contains(id))
        .collect();
    assert_eq!(posts_shown, want, "Posts: updated DESC, id DESC");
    let native_posts: Vec<String> = me
        .items()
        .iter()
        .filter_map(|i| i.server_id.as_ref().map(|s| s.0.clone()))
        .filter(|id| mine.contains(id))
        .collect();
    assert_eq!(native_posts, want, "native Posts: the same");
    // `/api/items` (the studio's own order) agrees.
    let server: Vec<String> =
        serde_json::from_str::<Vec<Value>>(&read_all(&url_a, &tok_a, "/api/items"))
            .unwrap()
            .iter()
            .filter_map(|v| v["id"].as_str().map(str::to_string))
            .filter(|id| mine.contains(id))
            .collect();
    assert_eq!(server, want, "the studio's order");
    let shown = web.reading();
    let own_rows: Vec<String> = shown
        .iter()
        .filter_map(|k| k.strip_prefix("~own|"))
        .filter(|id| mine.iter().any(|m| m == id))
        .map(str::to_string)
        .collect();
    assert_eq!(own_rows, want, "Reading: yours in the same order");
    assert_eq!(shown, native_reading(&me), "and the whole list as native");
    eprintln!(
        "web order: {} reading rows, {} posts, matched native",
        shown.len(),
        web.posts().len()
    );
    me.unsubscribe(&sub.id).unwrap();
}
