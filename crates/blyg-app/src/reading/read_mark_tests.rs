//! Opening an unread post marks it read: that one row changes, in place.
//! The list isn't rebuilt (no `reading_changed`, no re-read of the store),
//! and everything that says how much is left to read follows at once: the
//! row's dot, the status bar, the header's "N to read", the sources pane.
//!
//! On the real store (`blyg_core::synth`'s invented posts) through a
//! `LiveBackend` that never syncs, as the perf tests run it.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::store::Store;
use blyg_core::{Backend, ConfigStore, LiveBackend, ReadingItem, SyncOptions, synth};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::sources_vm::{self, Entry, Smart, Source};
use super::stream_vm::ReadMode;
use super::{View, vm};
use crate::app::MainView;
use crate::prefs::Prefs;
use crate::test_wait::settle;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";
/// Enough rows that the store's work for a refresh (about 4k VM steps
/// here) is over the bound below, and the open's own (about 900) under it.
const ROWS: usize = 2_000;

fn setup<'a>(
    dir: &std::path::Path,
    cx: &'a mut TestAppContext,
    mode: ReadMode,
) -> (
    Entity<MainView>,
    Arc<LiveBackend>,
    &'a mut VisualTestContext,
) {
    {
        let store = Store::open(&dir.join("blygger.db")).unwrap();
        store
            .take_served_subscriptions(&synth::subscriptions(), blyg_core::api::Prune::Allowed)
            .unwrap();
        store
            .merge_reading(&synth::reading(ROWS, 3), true, &synth::kinds())
            .unwrap();
    }
    let live = Arc::new(
        LiveBackend::open_with(
            dir,
            // Nothing listens here; nothing is ever sent anyway.
            "http://127.0.0.1:9",
            "read-mark-test",
            SyncOptions {
                start_worker: false,
                ..SyncOptions::default()
            },
        )
        .unwrap(),
    );
    let prefs = Prefs::from_config(ConfigStore::in_memory(CONNECTED).config());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(CONNECTED),
            Arc::new(MemoryTokenStore::default()),
            None,
            cx,
        );
    });
    let backend: Arc<dyn Backend> = live.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(backend, None, prefs, std::time::Instant::now(), window, cx)
    });
    settle(cx);
    view.update_in(cx, |v, window, cx| {
        if v.reading.view != View::Reading {
            v.show_view(View::Reading, window, cx);
        }
        v.set_read_mode(mode, window, cx);
    });
    settle(cx);
    (view, live, cx)
}

/// Everything that counts what's left to read, as the screen shows it.
#[derive(Debug, PartialEq)]
struct Counts {
    /// The status bar (`MainView::unread`).
    status: usize,
    /// The reading header's "N to read".
    header: usize,
    /// The sources pane's All unread.
    all_unread: usize,
    /// The sources pane's dot on the post's subscription.
    sub_dot: bool,
}

fn counts(v: &MainView, sub: &str) -> Counts {
    let entries = v.source_entries();
    Counts {
        status: v.unread,
        header: sources_vm::unread_total(&v.reading.rows, &v.reading.hopper_only),
        all_unread: entries
            .iter()
            .find_map(|e| match e {
                Entry::Smart {
                    smart: Smart::Unread,
                    unread,
                } => Some(*unread),
                _ => None,
            })
            .unwrap(),
        sub_dot: entries.iter().any(
            |e| matches!(e, Entry::Sub { sub: s, unread, .. } if s.id.as_str() == sub && *unread),
        ),
    }
}

fn row(v: &MainView, key: &vm::Key) -> ReadingItem {
    v.reading
        .rows
        .iter()
        .find(|r| vm::key(r) == *key)
        .cloned()
        .unwrap()
}

/// A shown post to read, from a subscription with another post to read
/// (so its dot stays on), never read before.
fn unread_post(v: &MainView) -> vm::Key {
    let to_read: Vec<&ReadingItem> = v
        .reading
        .shown_rows()
        .filter(|r| r.is_unread() && sources_vm::counts_as_unread(r, &v.reading.hopper_only))
        .collect();
    to_read
        .iter()
        .find(|r| {
            to_read
                .iter()
                .filter(|o| o.subscription_id == r.subscription_id)
                .count()
                > 1
        })
        .map(|r| vm::key(r))
        .unwrap()
}

#[gpui_kit::test]
fn opening_an_unread_post_marks_its_row_without_a_refresh(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, live, cx) = setup(dir.path(), cx, ReadMode::Reader);
    let key = view.read_with(cx, |v, _| unread_post(v));
    let sub = key.0.to_string();
    let before = view.read_with(cx, |v, _| counts(v, &sub));
    let refreshes = view.read_with(cx, |v, _| v.reading.refreshes);
    let held = view.read_with(cx, |v, _| v.reading.rows.len());

    let work = live.count_store_work();
    work.take();
    view.update_in(cx, |v, window, cx| v.open_reading(key.clone(), window, cx));
    settle(cx);
    let steps = work.take();
    eprintln!("open an unread post: {steps} VM steps, {held} rows held");

    view.read_with(cx, |v, _| {
        assert_eq!(
            v.reading.refreshes - refreshes,
            0,
            "opening a post rebuilt the whole list ({held} rows) to change one"
        );
        // The row's dot: read at its current version.
        let r = row(v, &key);
        assert_eq!(r.read_version, Some(r.version));
        assert!(!r.to_read());
        // The open post keeps the read state it had when it opened.
        let o = v.reading.opened.as_ref().unwrap();
        assert_eq!(o.key, key);
        assert_eq!(o.item.read_version, None);
        // Still listed where it was.
        assert!(v.reading.shown_pos(&key).is_some());
        assert_eq!(
            counts(v, &sub),
            Counts {
                status: before.status - 1,
                header: before.header - 1,
                all_unread: before.all_unread - 1,
                sub_dot: true,
            }
        );
    });
    // The store's work: the open (its versions, its responses), the mark
    // and the count, never what a refresh reads again (subscriptions,
    // folders, hoppers, trails, the shelf, …).
    assert!(
        (steps as usize) < held,
        "{steps} VM steps to mark one post read ({held} rows held)"
    );
}

/// In All unread, the open post stays listed once read (it's sticky); a
/// post marked read elsewhere (a stream card seen, another window) goes.
#[gpui_kit::test]
fn all_unread_drops_a_post_read_elsewhere_and_keeps_the_open_one(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, live, cx) = setup(dir.path(), cx, ReadMode::Reader);
    view.update(cx, |v, cx| {
        v.reading.source = Source::Smart(Smart::Unread);
        v.reading.refilter();
        cx.notify();
    });
    let key = view.read_with(cx, |v, _| unread_post(v));
    view.update_in(cx, |v, window, cx| v.open_reading(key.clone(), window, cx));
    settle(cx);
    let other = view.read_with(cx, |v, _| {
        assert!(v.reading.shown_pos(&key).is_some(), "the open post stays");
        v.reading
            .shown_rows()
            .map(vm::key)
            .find(|k| *k != key)
            .unwrap()
    });
    let shown = view.read_with(cx, |v, _| v.reading.shown.len());
    live.mark_read(&other.0, &other.1).unwrap();
    settle(cx);
    view.read_with(cx, |v, _| {
        assert!(
            v.reading.shown_pos(&other).is_none(),
            "read elsewhere: gone"
        );
        assert!(v.reading.shown_pos(&key).is_some(), "the open post stays");
        assert_eq!(v.reading.shown.len(), shown - 1);
        assert!(!row(v, &other).to_read());
    });
}

/// A read state that arrives by sync (another device) is a
/// `ReadingChanged`: the list is read again, as before.
#[gpui_kit::test]
fn a_read_mark_from_sync_still_refreshes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, live, cx) = setup(dir.path(), cx, ReadMode::Stream);
    let key = view.read_with(cx, |v, _| unread_post(v));
    let refreshes = view.read_with(cx, |v, _| v.reading.refreshes);
    // As a pull lands it: the store first, then the event.
    live.set_read(std::slice::from_ref(&key), true).unwrap();
    settle(cx);
    view.update_in(cx, |v, window, cx| {
        v.on_core_events(vec![blyg_core::CoreEvent::ReadingChanged], window, cx)
    });
    settle(cx);
    view.read_with(cx, |v, _| {
        assert!(v.reading.refreshes > refreshes);
        assert!(!row(v, &key).to_read());
    });
}

/// The Stream lists every post that isn't Background, whatever source the
/// Reader last had (All unread included): a card seen and marked read
/// stays where it is, before and after the next refresh. (The source only
/// filters the Reader's list, as it did before marks were in place.)
#[gpui_kit::test]
fn a_seen_stream_card_stays_listed_in_the_stream(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, live, cx) = setup(dir.path(), cx, ReadMode::Stream);
    view.update(cx, |v, cx| {
        v.reading.source = Source::Smart(Smart::Unread);
        v.reading.refilter();
        cx.notify();
    });
    let key = view.read_with(cx, |v, _| unread_post(v));
    let shown = view.read_with(cx, |v, _| v.reading.shown.len());
    // As the stream's seen-tracker marks a card.
    live.mark_read(&key.0, &key.1).unwrap();
    settle(cx);
    let marked = |v: &MainView| {
        assert!(!row(v, &key).to_read(), "marked read");
        assert!(v.reading.shown_pos(&key).is_some(), "still listed");
        assert_eq!(v.reading.shown.len(), shown);
    };
    view.read_with(cx, |v, _| marked(v));
    view.update(cx, |v, cx| v.reading_changed(cx));
    view.read_with(cx, |v, _| marked(v));
}

/// A `ReadMarked` the current backend's store doesn't bear out changes
/// nothing: one queued by a blyg just switched away from can name a
/// subscription and post id this blyg also holds (subscription ids are
/// each studio's own strings; two blygs can follow the same post).
#[gpui_kit::test]
fn a_read_mark_the_store_does_not_hold_changes_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, _live, cx) = setup(dir.path(), cx, ReadMode::Reader);
    let key = view.read_with(cx, |v, _| unread_post(v));
    let sub = key.0.to_string();
    let before = view.read_with(cx, |v, _| counts(v, &sub));
    let version = view.read_with(cx, |v, _| row(v, &key).version);
    // Never written to this store: as if from the previous blyg's.
    let stale = blyg_core::ReadMarked {
        sub: key.0.clone(),
        remote_id: key.1.clone(),
        version,
    };
    view.update_in(cx, |v, window, cx| {
        v.on_core_events(
            vec![blyg_core::CoreEvent::ReadMarked(vec![stale])],
            window,
            cx,
        )
    });
    settle(cx);
    view.read_with(cx, |v, _| {
        assert!(row(v, &key).is_unread(), "a mark this store never made");
        assert_eq!(counts(v, &sub), before);
    });
}
