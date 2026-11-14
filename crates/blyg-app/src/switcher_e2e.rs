//! The blyg switcher against two REAL local blygs (`wrangler dev`), in a
//! headless window: the app's own `open_live` path, data folders, Keychain
//! (an in-memory token store), config and keys, with nothing faked.
//!
//! `#[ignore]`d: `scripts/e2e-local.sh` starts the studios and runs it
//! (`cargo test -p blyg-app --bin blygger switcher::e2e -- --ignored`), with
//!
//! - `BLYG_E2E_URL`, `BLYG_E2E_TOKEN`: blyg `a` (`http://127.0.0.1:<port>`)
//! - `BLYG_E2E_SWITCH_URL`, `BLYG_E2E_SWITCH_TOKEN`: the second blyg, `b` as
//!   `http://localhost:<port>`, so its host (a blyg's identity) differs.
//!
//! Never point these at a real blyg: they create drafts and hoppers.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use blyg_core::api::Api;
use blyg_core::api::auth::Credential;
use blyg_core::config::blygs::{self, DB_OWNER_FILE};
use blyg_core::config::{MemoryTokenStore, TokenStore};
use blyg_core::{Backend, ConfigStore, CoreEvent, Kind, LocalId, SyncStatus};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::super::{MainView, Sheet};
use crate::connection::{Connection, Mode, SwitchBackend};
use crate::prefs::Prefs;

fn env(key: &str) -> String {
    let v = std::env::var(key)
        .unwrap_or_else(|_| panic!("{key} not set: run scripts/e2e-local.sh"))
        .trim_end_matches('/')
        .to_string();
    if key.ends_with("URL") {
        assert!(
            v.starts_with("http://127.0.0.1:") || v.starts_with("http://localhost:"),
            "the switcher e2e only ever runs against local blygs, not {v}"
        );
    }
    v
}

struct Blyg {
    url: String,
    token: String,
}

impl Blyg {
    fn api(&self) -> Api {
        Api::new(&self.url, Credential::Token(self.token.clone()))
    }

    /// Every item's text on the server.
    fn server_texts(&self) -> Vec<String> {
        self.api()
            .list_items()
            .expect("list the blyg's items")
            .into_iter()
            .map(|i| i.content_md)
            .collect()
    }

    fn server_has(&self, needle: &str) -> bool {
        self.server_texts().iter().any(|t| t.contains(needle))
    }

    fn server_hoppers(&self) -> Vec<String> {
        self.api()
            .hoppers()
            .expect("list the blyg's hoppers")
            .unwrap_or_default()
            .into_iter()
            .map(|h| h.name)
            .collect()
    }
}

fn tag() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("sw{n:x}")
}

/// The backends' events. They're raised on the sync threads; the test
/// scheduler only allows the test's own thread to wake the window's tasks,
/// so they wait here and [`pump`] hands them to the window in order, as the
/// app's channel would.
static EVENTS: Mutex<Vec<CoreEvent>> = Mutex::new(Vec::new());

fn pump(view: &Entity<MainView>, cx: &mut VisualTestContext) {
    loop {
        cx.run_until_parked();
        let evs = std::mem::take(&mut *EVENTS.lock().unwrap_or_else(|p| p.into_inner()));
        if evs.is_empty() {
            return;
        }
        for e in evs {
            view.update_in(cx, |v, window, cx| v.on_core_event(e, window, cx));
        }
    }
}

/// Run the window until `f` holds (the backends sync on their own threads).
fn wait(
    view: &Entity<MainView>,
    cx: &mut VisualTestContext,
    what: &str,
    mut f: impl FnMut(&mut VisualTestContext) -> bool,
) {
    let t0 = Instant::now();
    loop {
        pump(view, cx);
        if f(cx) {
            return;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(45),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn active(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| crate::settings::blyg_url(cx))
}

/// The window's backend (the app's `SwitchBackend`).
fn current(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Arc<dyn Backend> {
    view.read_with(cx, |v, _| v.backend.clone())
}

fn toast(view: &Entity<MainView>, cx: &mut VisualTestContext) -> (String, Option<String>) {
    view.read_with(cx, |v, _| {
        v.toast
            .as_ref()
            .map(|t| (t.text.to_string(), t.sub.as_ref().map(|s| s.to_string())))
            .unwrap_or_default()
    })
}

/// Press the switcher key for the `n`th blyg (1-based), as this platform
/// binds it.
fn press_switch(view: &Entity<MainView>, n: usize, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(&crate::keymap::keys(&format!("ctrl-cmd-{n}")));
    pump(view, cx);
}

fn synced(b: &Arc<dyn Backend>, id: &LocalId) -> bool {
    b.item(id).is_some_and(|i| i.server_id.is_some()) && b.sync_status() == SyncStatus::Synced
}

fn owner_of(dir: &Path) -> String {
    std::fs::read_to_string(dir.join(DB_OWNER_FILE)).unwrap_or_default()
}

#[gpui_kit::test]
#[ignore = "needs two local blygs: scripts/e2e-local.sh"]
fn two_live_blygs_side_by_side(cx: &mut TestAppContext) {
    let a = Blyg {
        url: env("BLYG_E2E_URL"),
        token: env("BLYG_E2E_TOKEN"),
    };
    let s = Blyg {
        url: env("BLYG_E2E_SWITCH_URL"),
        token: env("BLYG_E2E_SWITCH_TOKEN"),
    };
    assert!(
        !blygs::same(&a.url, &s.url),
        "the two blygs need different hosts ({} / {})",
        a.url,
        s.url
    );
    let t = tag();

    // --- the app's world: a data dir, a Keychain, a config with both ---
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().to_path_buf();
    let tokens = Arc::new(MemoryTokenStore::default());
    tokens.set(&a.url, &a.token).unwrap();
    tokens.set(&s.url, &s.token).unwrap();
    let tokens: Arc<dyn TokenStore> = tokens;
    let config = format!("blyg-url = {}\nblyg = {}\nblyg = {}\n", a.url, a.url, s.url);
    let first = crate::connection::open_live(&root_path, &a.url, &tokens, Default::default())
        .expect("open blyg a")
        .expect("a has a sign-in");
    // The window starts on nothing, so no sync thread can reach it before
    // its events are routed through `EVENTS`; then blyg a comes in.
    let switch = SwitchBackend::new(
        Arc::new(crate::connection::Disconnected),
        Mode::Disconnected,
    );
    let backend: Arc<dyn Backend> = switch.clone();
    let prefs = Prefs::from_config(ConfigStore::in_memory(&config).config());
    {
        let (tokens, config, data_dir, switch) = (
            tokens.clone(),
            config.clone(),
            root_path.clone(),
            switch.clone(),
        );
        cx.update(move |cx| {
            gpui_kit::init(cx);
            super::super::bind_keys(cx);
            crate::settings::init(ConfigStore::in_memory(&config), tokens, None, cx);
            cx.set_global(Connection {
                switch,
                data_dir,
                verify: crate::connection::live_verifier,
            });
        });
    }
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(backend, None, prefs, std::time::Instant::now(), window, cx)
    });
    cx.run_until_parked();
    switch.set_event_sink(Box::new(|e| {
        EVENTS.lock().unwrap_or_else(|p| p.into_inner()).push(e)
    }));
    switch.switch(first, Mode::Live, || {});
    switch.set_live_dir(Some(blygs::data_dir_for(&root_path, &a.url)));
    view.update_in(cx, |v, window, cx| v.blyg_switched(window, cx));
    pump(&view, cx);

    // --- one blyg per host: b at a's host (127.0.0.1, another port) is
    // refused before its sign-in could replace a's in the Keychain ---
    if let Ok(b_same_host) = std::env::var("BLYG_E2E_URL_B") {
        let err = cx
            .update(|_, cx| {
                crate::settings::connect(
                    b_same_host.trim_end_matches('/'),
                    &Credential::Token("not-a-real-token".into()),
                    cx,
                )
            })
            .expect_err("a second blyg on a's host is refused");
        assert!(err.contains("one blyg per host"), "{err}");
        assert_eq!(
            tokens.get(&a.url).unwrap().as_deref(),
            Some(a.token.as_str())
        );
        assert_eq!(active(cx).as_deref(), Some(a.url.as_str()));
    }

    // --- each blyg its own folder ---
    let dir_a = blygs::data_dir_for(&root_path, &a.url);
    let dir_s = blygs::data_dir_for(&root_path, &s.url);
    assert_eq!(dir_a, root_path, "the first blyg keeps the data dir");
    assert_eq!(dir_s, root_path.join("blygs").join("localhost"));
    assert_eq!(owner_of(&dir_a), a.url);

    // --- sync a, write a draft there ---
    let b = current(&view, cx);
    b.sync_now().expect("sync a");
    let text_a = format!("Switcher a {t}: the heron at the weir.");
    let id_a = b.create_draft(Kind::Fragment, &text_a).unwrap();
    wait(&view, cx, "a's draft to reach a", |_| synced(&b, &id_a));
    assert!(a.server_has(&text_a));

    // --- switch to the second blyg by its key ---
    press_switch(&view, 2, cx);
    assert_eq!(active(cx).as_deref(), Some(s.url.as_str()));
    assert_eq!(switch.base_url().as_deref(), Some(s.url.as_str()));
    assert_eq!(
        toast(&view, cx).0,
        "Switched to localhost",
        "the toast names the blyg"
    );
    assert!(dir_s.join(blyg_core::live::DB_FILE).exists());
    assert_eq!(owner_of(&dir_s), s.url);
    assert_eq!(owner_of(&dir_a), a.url, "a's folder still a's");
    let b = current(&view, cx);
    wait(&view, cx, "the second blyg's first sync", |_| {
        b.sync_status() == SyncStatus::Synced
    });
    assert!(
        b.item(&id_a).is_none(),
        "a's draft isn't in the second blyg"
    );
    assert!(b.search(&t).is_empty(), "search sees only this blyg");
    assert!(
        view.read_with(cx, |v, _| v
            .list
            .results()
            .iter()
            .all(|i| !i.content_md.contains(&t))),
        "the posts list holds only this blyg's posts"
    );
    let text_s = format!("Switcher s {t}: moss grows where nobody walks.");
    let id_s = b.create_draft(Kind::Fragment, &text_s).unwrap();
    wait(&view, cx, "s's draft to reach s", |_| synced(&b, &id_s));
    assert!(s.server_has(&text_s), "the draft landed on its own blyg");
    assert!(!a.server_has(&text_s), "and not on the other");
    assert!(!s.server_has(&text_a));

    // --- hoppers per blyg ---
    let hop_s = b.create_hopper(&format!("Hop s {t}")).expect("hopper on s");
    assert!(b.hoppers().iter().any(|h| h.id == hop_s.id));

    // --- subscriptions per blyg: s follows a ---
    let had_sub = b
        .subscriptions()
        .into_iter()
        .find(|x| blygs::same(&x.origin, &a.url) || x.feed_url.contains(&a.url));
    let sub_s = match &had_sub {
        Some(x) => x.clone(),
        None => b.subscribe(&a.url, None).expect("s subscribes to a"),
    };
    assert!(b.subscriptions().iter().any(|x| x.id == sub_s.id));

    // --- outbox: an edit still waiting on s when we switch away ---
    let edit_s = format!("{text_s} Edited before the switch.");
    b.save(&id_s, &edit_s).unwrap();
    press_switch(&view, 1, cx);
    assert_eq!(active(cx).as_deref(), Some(a.url.as_str()));
    let (text, sub) = toast(&view, cx);
    assert_eq!(
        text,
        format!("Switched to {}", crate::vm::url_host(&a.url).unwrap())
    );
    assert_eq!(
        sub.as_deref(),
        Some("Changes waiting on localhost sync when you switch back to it")
    );
    let b = current(&view, cx);
    wait(&view, cx, "a to settle", |_| {
        b.sync_status() == SyncStatus::Synced
    });
    assert!(b.item(&id_a).is_some(), "a's draft is back");
    assert!(b.item(&id_s).is_none(), "s's draft never shows on a");
    assert_eq!(b.search(&t).len(), 1, "search on a finds a's draft only");
    assert!(
        b.hoppers().iter().all(|h| h.id != hop_s.id),
        "s's hopper isn't a's"
    );
    assert!(
        b.subscriptions().iter().all(|x| x.id != sub_s.id),
        "s's subscription isn't a's"
    );
    let subs_a: Vec<String> = b.subscriptions().into_iter().map(|x| x.id).collect();
    assert!(
        b.reading()
            .iter()
            .all(|r| subs_a.contains(&r.subscription_id) || r.subscription_id.starts_with('~')),
        "a's reading comes from a's subscriptions"
    );
    let hop_a = b.create_hopper(&format!("Hop a {t}")).expect("hopper on a");
    assert!(a.server_hoppers().contains(&hop_a.name));
    assert!(!a.server_hoppers().contains(&hop_s.name));
    assert!(!s.server_hoppers().contains(&hop_a.name));
    // s's queued edit waited in s's own database: it never went to a.
    std::thread::sleep(Duration::from_secs(2));
    assert!(!a.server_has("Edited before the switch."));
    let edit_landed_early = s.server_has(&edit_s);

    // --- back to s: its outbox flushes there ---
    press_switch(&view, 2, cx);
    let b = current(&view, cx);
    wait(&view, cx, "s's queued edit to land on s", |_| {
        s.server_has(&edit_s)
    });
    assert!(!a.server_has(&edit_s));
    assert!(b.hoppers().iter().any(|h| h.id == hop_s.id));
    assert!(b.hoppers().iter().all(|h| h.id != hop_a.id));
    eprintln!(
        "switcher e2e: the queued edit {} before switching back",
        if edit_landed_early {
            "had landed (the closing pass flushed it)"
        } else {
            "waited"
        }
    );

    // --- switching while a sync is in flight, back and forth ---
    for round in 0..4 {
        let cur = switch.current();
        let syncing = std::thread::spawn(move || {
            let _ = cur.sync_now();
        });
        press_switch(&view, if round % 2 == 0 { 1 } else { 2 }, cx);
        // The window never shows a status or error from the blyg it left.
        let (text, _) = toast(&view, cx);
        assert!(text.starts_with("Switched to"), "round {round}: {text}");
        syncing.join().expect("the in-flight sync finished cleanly");
    }
    // Ends on s (rounds 0..4 end on 2). Rapid switching kept every blyg whole.
    assert_eq!(active(cx).as_deref(), Some(s.url.as_str()));
    let b = current(&view, cx);
    wait(&view, cx, "s to settle after the rapid switching", |_| {
        b.sync_status() == SyncStatus::Synced
    });
    assert!(b.item(&id_s).is_some() && b.item(&id_a).is_none());
    view.read_with(cx, |v, _| {
        assert!(
            !matches!(v.sheet, Some(Sheet::Conflict { .. })),
            "no conflict sheet from the other blyg"
        );
        assert_eq!(v.base_url.as_deref(), Some(s.url.as_str()));
    });
    assert_eq!(owner_of(&dir_a), a.url);
    assert_eq!(owner_of(&dir_s), s.url);

    // --- clean up what we made on both blygs ---
    b.delete_hopper(&hop_s.id).ok();
    if had_sub.is_none() {
        b.unsubscribe(&sub_s.id).ok();
    }
    b.delete_draft(&id_s).ok();
    press_switch(&view, 1, cx);
    let b = current(&view, cx);
    b.delete_hopper(&hop_a.id).ok();
    b.delete_draft(&id_a).ok();
    pump(&view, cx);
}
