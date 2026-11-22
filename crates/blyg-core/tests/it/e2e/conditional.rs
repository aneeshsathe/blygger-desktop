//! Studio 0.65 e2e: conditional reads (studio#152, contract 5.14.0).
//!
//! Every private `GET` but `/changes` answers with an `ETag`, and a bodyless
//! 304 when asked with it and nothing changed; `POST /read-batch` children
//! take `ifNoneMatch`. `GET /changes` is deprecated, and gone in studio 0.67:
//! [`Shim`] answers it 404 on an older studio too, to prove sync does without it.
//! On a studio before 0.65 (no `ETag`s) each test says so and skips.

use super::*;
use blyg_core::api::read_batch::BatchRead;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Whether the studio under test answers reads with an `ETag`.
fn conditional(api: &Api) -> bool {
    api.contract_version().expect("capabilities");
    if api.conditional_reads() {
        return true;
    }
    eprintln!("skipped: this studio sends no ETag (older than 0.65)");
    false
}

/// A pass-through proxy (like `Proxy`) that counts what comes back: the
/// response bytes (headers and bodies) and the 304s. With `no_changes`, it
/// asks the Worker for `/api/changez` in place of `/api/changes` (same
/// length, so nothing else moves): an unknown route, 404, as studio 0.67
/// answers once `/changes` is gone.
pub(super) struct Shim {
    pub(super) url: String,
    bytes: Arc<AtomicU64>,
    not_modified: Arc<AtomicU64>,
    requests: Arc<AtomicU64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Wire {
    pub(super) requests: u64,
    pub(super) not_modified: u64,
    pub(super) bytes: u64,
}

impl Shim {
    pub(super) fn start(target: &str, no_changes: bool) -> Shim {
        use std::io::{Read, Write};
        let upstream = target.trim_start_matches("http://").to_string();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let here = listener.local_addr().unwrap().to_string();
        let (bytes, not_modified, requests) = (
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(0)),
        );
        let (b2, n2, r2) = (bytes.clone(), not_modified.clone(), requests.clone());
        let here2 = here.clone();
        std::thread::spawn(move || {
            for client in listener.incoming() {
                let Ok(mut client) = client else { return };
                let Ok(mut server) = std::net::TcpStream::connect(&upstream) else {
                    continue;
                };
                let (mut c2, mut s2) = (client.try_clone().unwrap(), server.try_clone().unwrap());
                let (b3, n3) = (b2.clone(), n2.clone());
                // Worker → app: count bytes and 304 status lines.
                std::thread::spawn(move || {
                    let mut buf = [0u8; 16 * 1024];
                    let mut tail = Vec::new();
                    loop {
                        let n = match s2.read(&mut buf) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => n,
                        };
                        b3.fetch_add(n as u64, Ordering::Relaxed);
                        tail.extend_from_slice(&buf[..n]);
                        let text = String::from_utf8_lossy(&tail);
                        let hits = text.matches("HTTP/1.1 304").count() as u64;
                        n3.fetch_add(hits, Ordering::Relaxed);
                        // Keep a little, so a status line split across reads counts once.
                        let keep = if hits > 0 { 0 } else { 16.min(tail.len()) };
                        tail.drain(..tail.len() - keep);
                        if c2.write_all(&buf[..n]).is_err() {
                            break;
                        }
                    }
                    let _ = c2.shutdown(std::net::Shutdown::Both);
                });
                let (from_host, to_host) = (here2.clone(), upstream.clone());
                let r3 = r2.clone();
                // App → Worker: the Worker must see its own host (tokens are
                // bound to it); `/changes` optionally renamed.
                std::thread::spawn(move || {
                    let mut buf = [0u8; 16 * 1024];
                    loop {
                        let n = match client.read(&mut buf) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => n,
                        };
                        if let Ok(mut text) = String::from_utf8(buf[..n].to_vec()) {
                            r3.fetch_add(
                                ["GET /", "POST /", "PUT /", "PATCH /", "DELETE /"]
                                    .iter()
                                    .map(|m| text.matches(m).count() as u64)
                                    .sum(),
                                Ordering::Relaxed,
                            );
                            text = text.replace(&from_host, &to_host);
                            if no_changes {
                                text = text.replace("GET /api/changes ", "GET /api/changez ");
                            }
                            if server.write_all(text.as_bytes()).is_err() {
                                break;
                            }
                        } else if server.write_all(&buf[..n]).is_err() {
                            break;
                        }
                    }
                    let _ = server.shutdown(std::net::Shutdown::Both);
                });
            }
        });
        Shim {
            url: format!("http://{here}"),
            bytes,
            not_modified,
            requests,
        }
    }

    /// What came through since the last call.
    pub(super) fn take(&self) -> Wire {
        Wire {
            requests: self.requests.swap(0, Ordering::Relaxed),
            not_modified: self.not_modified.swap(0, Ordering::Relaxed),
            bytes: self.bytes.swap(0, Ordering::Relaxed),
        }
    }
}

/// Sync options for `Shim` tests: no worker, and the safety net every
/// `net` (zero: every pull reads everything whole).
fn shim_opts(conditional_reads: bool, net: Duration) -> SyncOptions {
    SyncOptions {
        conditional_reads,
        full_pull_every: net,
        full_pull_max: net,
        ..opts(false)
    }
}

/// Deletes a server item when dropped.
struct ServerItem<'a>(&'a Api, String);

impl Drop for ServerItem<'_> {
    fn drop(&mut self) {
        if let Ok(w) = self.0.get_item(&self.1) {
            let _ = self.0.delete_item(&self.1, w.revision);
        }
    }
}

/// A list read again unchanged comes back 304: the same list, and no body
/// bytes. A change on the blyg is a 200 with it in, never the stale list.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn a_304_hands_back_the_kept_list_without_its_body() {
    let e = e2e();
    let api = Api::new(&e.url, e.cred.clone());
    if !conditional(&api) {
        return;
    }
    let t = tag("etag");
    let w = api
        .create_item(&format!("{t} first"), Kind::Fragment, None, None)
        .unwrap();
    let _gone = ServerItem(&api, w.id.clone());

    let first = api.list_items().unwrap();
    let before = api.traffic();
    let again = api.list_items().unwrap();
    let quiet = api.traffic().since(&before);
    let ids = |l: &[blyg_core::api::wire::WireItem]| {
        l.iter()
            .map(|w| (w.id.clone(), w.revision))
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&first), ids(&again), "a 304 is the list as it was");
    assert!(ids(&again).iter().any(|(id, _)| *id == w.id));
    assert!(quiet.requests >= 2, "{quiet:?}");
    assert_eq!(quiet.not_modified, quiet.requests, "all 304: {quiet:?}");
    assert_eq!(quiet.body_bytes, 0, "no body came: {quiet:?}");

    // Another device adds an item: the next read is a 200 that has it.
    let other = Api::new(&e.url, e.cred.clone());
    let w2 = other
        .create_item(&format!("{t} second"), Kind::Fragment, None, None)
        .unwrap();
    let _gone2 = ServerItem(&api, w2.id.clone());
    let before = api.traffic();
    let after = api.list_items().unwrap();
    let moved = api.traffic().since(&before);
    assert!(after.iter().any(|x| x.id == w2.id), "the new item is read");
    assert!(moved.body_bytes > 0, "{moved:?}");

    // Turned off, the same read costs its whole body again.
    api.set_conditional(false);
    let before = api.traffic();
    api.list_items().unwrap();
    let whole = api.traffic().since(&before);
    assert_eq!(whole.not_modified, 0, "{whole:?}");
    assert!(whole.body_bytes > 0, "{whole:?}");
    eprintln!("list_items: unchanged {quiet:?}; asked whole {whole:?}");
}

/// Without `/changes` (studio 0.67), sync asks each list conditionally.
/// Pulls where nothing changed are 304s and prune nothing: every reading
/// row and item stays. A new post still arrives, and a removed
/// subscription's rows still go.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn without_changes_a_304_prunes_nothing_and_new_posts_still_arrive() {
    let e = e2e();
    if !conditional(&Api::new(&e.url, e.cred.clone())) {
        return;
    }
    let src_url = url_b();
    let (d1, d2, d3) = (
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    let source = backend_at(d2.path(), &src_url, cred_b(), false);
    let t = tag("no-changes");
    let (_, rid1) = published(&source, Kind::Fragment, &format!("{t} one"), None);
    let (_, rid2) = published(&source, Kind::Fragment, &format!("{t} two"), None);
    let me = manual(d1.path());
    let sub = me.subscribe(&src_url, Some("Conditional")).unwrap();
    let unsub = Unsub(&me, sub.id.clone());
    sync_until_held(&me, &[&rid1, &rid2]);
    let mine = me
        .create_draft(Kind::Fragment, &format!("{t} mine"))
        .unwrap();
    me.sync_now().unwrap();

    let shim = Shim::start(&e.url, true);
    let other = LiveBackend::open_with(
        d3.path(),
        &shim.url,
        e.cred.clone(),
        shim_opts(true, Duration::from_secs(3600)),
    )
    .unwrap();
    other.pull_now().unwrap();
    let held = |b: &LiveBackend| (b.reading().len(), b.items().len());
    let first = held(&other);
    assert!(reading_item(&other, &rid1).is_some() && reading_item(&other, &rid2).is_some());
    let sid = sid_of(&me, &mine);
    let has_mine = |b: &LiveBackend| {
        b.items()
            .iter()
            .any(|i| i.server_id.as_ref().is_some_and(|s| s.0 == sid))
    };
    assert!(has_mine(&other));
    shim.take();

    for _ in 0..5 {
        other.pull_now().unwrap();
        assert_eq!(held(&other), first, "a 304 pruned nothing");
    }
    let idle = shim.take();
    assert!(
        idle.not_modified * 10 >= idle.requests * 8,
        "idle pulls are mostly 304s: {idle:?}"
    );
    assert!(reading_item(&other, &rid1).is_some() && reading_item(&other, &rid2).is_some());
    assert!(has_mine(&other));
    eprintln!("no /changes, 5 idle pulls: {idle:?}");

    // A new post on `b`, polled in: it reaches `other` with no counters.
    let (_, rid3) = published(&source, Kind::Fragment, &format!("{t} three"), None);
    Api::new(&e.url, e.cred.clone())
        .resync_subscription(&sub.id)
        .unwrap();
    wait_until("the new post", Duration::from_secs(20), || {
        other.pull_now().unwrap();
        reading_item(&other, &rid3).is_some()
    });
    assert!(reading_item(&other, &rid1).is_some(), "nothing else went");

    // Unsubscribed: its rows go (a real change still prunes).
    drop(unsub);
    wait_until("the rows to go", Duration::from_secs(20), || {
        other.pull_now().unwrap();
        reading_item(&other, &rid1).is_none()
    });
}

/// `POST /read-batch` children carry the ETag kept for their URL; an
/// unchanged child comes back 304 with the kept body, a changed one 200.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn a_read_batch_child_304_hands_back_its_body() {
    let e = e2e();
    let api = Api::new(&e.url, e.cred.clone());
    if !conditional(&api) {
        return;
    }
    assert!(api.serves_read_batch().unwrap(), "0.65 lists batchRead");
    let t = tag("batch");
    let a = api
        .create_item(&format!("{t} a"), Kind::Fragment, None, None)
        .unwrap();
    let _ga = ServerItem(&api, a.id.clone());
    let b = api
        .create_item(&format!("{t} b"), Kind::Fragment, None, None)
        .unwrap();
    let _gb = ServerItem(&api, b.id.clone());
    let reads: Vec<BatchRead> = [&a.id, &b.id]
        .iter()
        .map(|id| BatchRead {
            path: vec![("id".into(), (*id).clone())],
            query: vec![],
        })
        .collect();

    let first = api.read_batch("getItem", "/items/{id}", &reads).unwrap();
    assert!(
        first.iter().all(|c| c.status == 200 && !c.not_modified),
        "{first:?}"
    );
    let again = api.read_batch("getItem", "/items/{id}", &reads).unwrap();
    assert!(
        again.iter().all(|c| c.status == 200 && c.not_modified),
        "{again:?}"
    );
    assert_eq!(first[0].body, again[0].body, "the kept body, whole");
    assert_eq!(again[1].body["id"], b.id.as_str());

    // `a` edited elsewhere: its child is a 200 with the edit, `b` still 304.
    Api::new(&e.url, e.cred.clone())
        .save_item(&a.id, a.revision, &format!("{t} a, edited"))
        .unwrap();
    let third = api.read_batch("getItem", "/items/{id}", &reads).unwrap();
    assert!(!third[0].not_modified, "{third:?}");
    assert!(
        third[0].body["contentMd"]
            .as_str()
            .unwrap_or_default()
            .contains("edited"),
        "{third:?}"
    );
    assert!(third[1].not_modified, "{third:?}");

    // The plain GET of the same URL shares the kept ETag: a 304.
    let before = api.traffic();
    let got = api.get_item(&b.id).unwrap();
    assert_eq!(got.id, b.id);
    assert_eq!(api.traffic().since(&before).not_modified, 1);
}

/// What sync costs on studio 0.65: requests and bytes on the wire for
/// pulls that find nothing new. Printed for the record; checked loosely.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn idle_sync_costs_less_with_conditional_reads() {
    let e = e2e();
    if !conditional(&Api::new(&e.url, e.cred.clone())) {
        return;
    }
    let src_url = url_b();
    let (d1, d2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let source = backend_at(d2.path(), &src_url, cred_b(), false);
    let t = tag("idle-cost");
    let (_, rid) = published(&source, Kind::Fragment, &format!("{t} one"), None);
    let me = manual(d1.path());
    let sub = me.subscribe(&src_url, Some("Idle cost")).unwrap();
    let _unsub = Unsub(&me, sub.id.clone());
    sync_until_held(&me, &[&rid]);
    // A modest blyg: 40 drafts of about a kilobyte each (deleted after).
    let api = Api::new(&e.url, e.cred.clone());
    let words = "An invented paragraph about burrows and their keepers. ".repeat(18);
    let _drafts: Vec<ServerItem> = (0..40)
        .map(|i| {
            let w = api
                .create_item(&format!("{t} {i} {words}"), Kind::Fragment, None, None)
                .unwrap();
            ServerItem(&api, w.id)
        })
        .collect();

    const PULLS: u64 = 5;
    // (label, conditional, /changes gone, safety net)
    let runs = [
        (
            "counters, idle, before",
            false,
            false,
            Duration::from_secs(3600),
        ),
        (
            "counters, idle, after",
            true,
            false,
            Duration::from_secs(3600),
        ),
        (
            "counters, net every pull, before",
            false,
            false,
            Duration::ZERO,
        ),
        (
            "counters, net every pull, after",
            true,
            false,
            Duration::ZERO,
        ),
        (
            "no /changes (0.67), idle, before",
            false,
            true,
            Duration::from_secs(3600),
        ),
        (
            "no /changes (0.67), idle, after",
            true,
            true,
            Duration::from_secs(3600),
        ),
    ];
    let mut out = Vec::new();
    for (label, cond, gone, net) in runs {
        let dir = tempfile::tempdir().unwrap();
        let shim = Shim::start(&e.url, gone);
        let b = LiveBackend::open_with(dir.path(), &shim.url, e.cred.clone(), shim_opts(cond, net))
            .unwrap();
        // The first pull reads everything whole; then idle.
        b.pull_now().unwrap();
        b.pull_now().unwrap();
        shim.take();
        for _ in 0..PULLS {
            b.pull_now().unwrap();
        }
        let w = shim.take();
        eprintln!(
            "idle sync, {label}: {:.1} requests, {:.0} bytes, {:.1} 304s per pull",
            w.requests as f64 / PULLS as f64,
            w.bytes as f64 / PULLS as f64,
            w.not_modified as f64 / PULLS as f64
        );
        out.push(w);
    }
    let per = |w: &Wire| w.bytes / PULLS;
    if super::has_changes() {
        // With counters an idle pull is one `/changes` either way.
        assert_eq!(out[0].requests, out[1].requests, "{out:?}");
    } else {
        // Without them (studio 0.67+) the "counters" runs have none either:
        // each idle pull asks its lists, and conditional reads answer them
        // with 304s (it may ask a list more, since a 304 is cheap).
        assert_eq!(out[0].not_modified, 0, "{out:?}");
        assert!(out[1].not_modified > 0, "{out:?}");
        assert!(per(&out[1]) * 5 < per(&out[0]) * 3, "{out:?}");
    }
    // The safety net and the counter-less pulls cost at least 40% fewer
    // bytes on the wire (gzip already shrinks the bodies; what's left is
    // mostly response headers).
    assert!(per(&out[3]) * 5 < per(&out[2]) * 3, "{out:?}");
    assert!(per(&out[5]) * 5 < per(&out[4]) * 3, "{out:?}");
    assert!(out[5].not_modified > 0, "{out:?}");
}
