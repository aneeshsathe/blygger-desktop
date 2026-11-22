//! --- reader order --- Studio 0.68's reading tabs on Burrow's sources:
//! the Feed (pace-layered or chronological), the Agentic feed, Saved and
//! Raw; and the running app keeping the choice.

use blyg_core::background::BackgroundSet;
use blyg_core::order::ReaderOrder;
use blyg_core::{OWN_SUBSCRIPTION, ReadingItem};
use serde_json::json;

use super::feed::{Feed, arrange, follows_feed, raw_order, shows_kinds};
use super::sources_vm::{Smart, Source, in_source};
use super::stream_vm::ReadMode;

fn row(sub: &str, id: &str, version: u32, created: &str, updated: &str) -> ReadingItem {
    serde_json::from_value(json!({
        "subscription_id": sub, "remote_id": id, "subscription_title": "Ada",
        "origin": "https://ada.blyg.example.com/", "kind": "fragment", "state": "current",
        "version": version, "created": created, "updated": updated, "observed_at": updated,
        "content_md": "A sample post", "content_html": "", "author": null, "page": null,
        "thumb": null, "hoppers": [],
    }))
    .unwrap()
}

fn ids(rows: &[ReadingItem], shown: &[usize]) -> Vec<String> {
    shown
        .iter()
        .map(|&i| rows[i].remote_id.to_string())
        .collect()
}

/// a (10:00), b (11:00), c (12:00); a's v2 at 13:00.
fn sample() -> Vec<ReadingItem> {
    vec![
        row(
            "sub-ada",
            "a",
            2,
            "2026-11-01T10:00:00Z",
            "2026-11-01T13:00:00Z",
        ),
        row(
            "sub-ada",
            "b",
            1,
            "2026-11-01T11:00:00Z",
            "2026-11-01T11:00:00Z",
        ),
        row(
            "sub-ada",
            "c",
            1,
            "2026-11-01T12:00:00Z",
            "2026-11-01T12:00:00Z",
        ),
    ]
}

/// Studio 0.68: the Feed is pace-layered by default (an edit climbs one
/// place); Chronological is newest first, as Burrow listed before.
#[test]
fn the_feed_is_pace_layered_unless_chronological() {
    let rows = sample();
    let mut feed = Feed::new(ReaderOrder::default());
    assert_eq!(feed.order, ReaderOrder::PaceLayered);
    feed.rebuild(&rows, &Default::default());
    let mut shown = vec![0, 1, 2];
    arrange(
        &rows,
        &mut shown,
        ReadMode::Stream,
        &Source::default(),
        &feed,
    );
    assert_eq!(ids(&rows, &shown), ["c", "a", "b"]);
    // Chronological leaves the list as it came (newest first).
    let mut feed = Feed::new(ReaderOrder::Newest);
    feed.rebuild(&rows, &Default::default());
    let mut shown = vec![0, 2, 1];
    arrange(
        &rows,
        &mut shown,
        ReadMode::Stream,
        &Source::default(),
        &feed,
    );
    assert_eq!(ids(&rows, &shown), ["a", "c", "b"]);
}

/// A row with no place yet (it arrived after the last rebuild) goes on
/// top, as the studio's does.
#[test]
fn a_row_newer_than_the_order_goes_on_top() {
    let mut rows = sample();
    let mut feed = Feed::new(ReaderOrder::PaceLayered);
    feed.rebuild(&rows, &Default::default());
    rows.push(row(
        "sub-ada",
        "new",
        1,
        "2026-11-01T09:00:00Z",
        "2026-11-01T09:00:00Z",
    ));
    let mut shown = vec![0, 1, 2, 3];
    arrange(
        &rows,
        &mut shown,
        ReadMode::Stream,
        &Source::default(),
        &feed,
    );
    assert_eq!(ids(&rows, &shown)[0], "new");
}

/// A row held when the order was worked out but given no place (none of
/// its times parse, so it has no versions to replay) goes to the bottom,
/// not the top: only a row that arrived since is new.
#[test]
fn a_row_the_order_couldnt_place_goes_to_the_bottom() {
    let mut rows = sample();
    let mut odd = row("sub-ada", "odd", 1, "not a date", "not a date");
    odd.observed_at = "never".into();
    rows.insert(0, odd);
    let mut feed = Feed::new(ReaderOrder::PaceLayered);
    feed.rebuild(&rows, &Default::default());
    let mut shown = vec![0, 1, 2, 3];
    arrange(
        &rows,
        &mut shown,
        ReadMode::Stream,
        &Source::default(),
        &feed,
    );
    assert_eq!(ids(&rows, &shown), ["c", "a", "b", "odd"]);
}

/// As the studio (`reading-data.ts`: every list but a hopper's, Saved and
/// Raw): Background and Mentions follow the Feed's order too; hoppers,
/// Saved, Raw, Burrow's trails and the empty Agentic feed keep their own.
#[test]
fn which_lists_follow_the_feeds_order() {
    for s in [
        Source::Smart(Smart::All),
        Source::Smart(Smart::Background),
        Source::Mentions,
        Source::Smart(Smart::Threads),
        Source::Smart(Smart::Unread),
        Source::Folder("f".into()),
        Source::Sub("sub-ada".into()),
        Source::Own,
    ] {
        assert!(follows_feed(ReadMode::Reader, &s), "{s:?}");
    }
    for s in [
        Source::Smart(Smart::Shelf),
        Source::Smart(Smart::Raw),
        Source::Smart(Smart::Agentic),
        Source::Hopper("h".into()),
        Source::Trail("t".into()),
    ] {
        assert!(!follows_feed(ReadMode::Reader, &s), "{s:?}");
    }
    assert!(follows_feed(ReadMode::Stream, &Source::Smart(Smart::Shelf)));
    // The Feed's kinds show on the Feed and its kinds, in Reader.
    assert!(shows_kinds(
        ReadMode::Reader,
        &Source::Smart(Smart::Fragments)
    ));
    assert!(!shows_kinds(
        ReadMode::Reader,
        &Source::Smart(Smart::Unread)
    ));
    assert!(!shows_kinds(ReadMode::Stream, &Source::Smart(Smart::All)));
}

/// Raw (the studio's Raw tab): what came in, Background or not, withdrawn
/// too, never your own posts, newest first by its date clamped to when it
/// came in. The Agentic feed lists nothing yet.
#[test]
fn raw_lists_what_came_in_and_the_agentic_feed_nothing() {
    let mut rows = sample();
    let mut gone = row(
        "sub-ada",
        "gone",
        3,
        "2026-11-01T08:00:00Z",
        "2026-11-01T14:00:00Z",
    );
    gone.state = "tombstone".into();
    let mut mine = row(
        OWN_SUBSCRIPTION,
        "mine",
        1,
        "2026-11-01T15:00:00Z",
        "2026-11-01T15:00:00Z",
    );
    mine.origin = "https://blyg.example.com/".into();
    // A future-dated post counts from when it came in.
    let mut future = row(
        "sub-ada",
        "future",
        1,
        "2027-01-01T00:00:00Z",
        "2027-01-01T00:00:00Z",
    );
    future.observed_at = "2026-11-01T10:30:00Z".into();
    rows.extend([gone, mine, future]);
    let set = BackgroundSet::from_entries(
        &[
            json!({"source": "imported", "imported": {"subscriptionId": "sub-ada", "remoteId": "b"},
                 "background": {"reasons": ["near-same"], "placement": null, "blyg": false}}),
        ],
        None,
    );
    set.annotate(&mut rows);
    let now = chrono::Utc::now();
    let raw = Source::Smart(Smart::Raw);
    let mut shown: Vec<usize> = (0..rows.len())
        .filter(|&i| in_source(&rows[i], &raw, &Default::default(), now))
        .collect();
    raw_order(&rows, &mut shown);
    assert_eq!(ids(&rows, &shown), ["gone", "a", "c", "b", "future"]);
    // The Feed leaves b (a small edit) and the withdrawn post to Background.
    let feed = Source::Smart(Smart::All);
    assert!(!in_source(&rows[1], &feed, &Default::default(), now));
    let agentic = Source::Smart(Smart::Agentic);
    assert!(
        rows.iter()
            .all(|r| !in_source(r, &agentic, &Default::default(), now))
    );
}

/// The tabs' names: studio 0.68's, on Burrow's sources.
#[test]
fn the_tabs_are_named_as_the_studio_names_them() {
    assert_eq!(Smart::All.label(), "Feed");
    assert_eq!(Smart::Agentic.label(), "Agentic feed");
    assert_eq!(Smart::Shelf.label(), "Saved");
    assert_eq!(Smart::Background.label(), "Background");
    assert_eq!(Smart::Raw.label(), "Raw");
    assert_eq!(super::feed::kind_label(Smart::All), "All");
    assert!(
        super::sources_vm::empty_label(&Source::Smart(Smart::Agentic)).starts_with("Coming soon.")
    );
    assert!(super::sources_vm::empty_label(&Source::Smart(Smart::Shelf)).contains("“+ Save”"));
}

// The running app on the sample blyg: the order control, kept by the blyg.
mod app {
    use std::sync::Arc;

    use blyg_core::config::MemoryTokenStore;
    use blyg_core::order::ReaderOrder;
    use blyg_core::{Backend, ConfigStore};
    use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

    use crate::app::MainView;
    use crate::app::reading::View;
    use crate::fake::{FakeBackend, Timing};
    use crate::prefs::Prefs;
    use crate::test_wait::settle_drawn as settle;

    const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

    fn setup(
        cx: &mut TestAppContext,
    ) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
        setup_with(cx, |_| {})
    }

    /// The sample blyg, its reader order set up by `before` first.
    fn setup_with(
        cx: &mut TestAppContext,
        before: impl FnOnce(&FakeBackend),
    ) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
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
        let fake = Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache());
        before(&fake);
        let backend: Arc<dyn Backend> = fake.clone();
        let f2 = fake.clone();
        let (view, cx) = cx.add_window_view(move |window, cx| {
            MainView::new(
                backend,
                Some(f2),
                prefs,
                std::time::Instant::now(),
                window,
                cx,
            )
        });
        cx.run_until_parked();
        (view, fake, cx)
    }

    /// Clicking Chronological in the reading header orders the Feed newest
    /// first and saves `readerOrder` on the blyg; Pace-layered brings it back.
    #[gpui_kit::test]
    fn the_order_control_switches_the_feed_and_the_blyg_keeps_it(cx: &mut TestAppContext) {
        let (view, fake, cx) = setup(cx);
        view.update_in(cx, |v, window, cx| v.show_view(View::Reading, window, cx));
        settle(cx);
        assert_eq!(fake.reader_order(), ReaderOrder::PaceLayered);
        let chrono = cx
            .debug_bounds("feed-order-newest")
            .expect("the Feed's order control is in the header");
        cx.simulate_click(chrono.center(), Modifiers::none());
        settle(cx);
        assert_eq!(
            view.read_with(cx, |v, _| v.reading.feed.order),
            ReaderOrder::Newest
        );
        assert_eq!(
            fake.reader_order(),
            ReaderOrder::Newest,
            "saved on the blyg"
        );
        let pace = cx.debug_bounds("feed-order-pace-layered").unwrap();
        cx.simulate_click(pace.center(), Modifiers::none());
        settle(cx);
        assert_eq!(fake.reader_order(), ReaderOrder::PaceLayered);
    }

    fn order(view: &Entity<MainView>, cx: &mut VisualTestContext) -> ReaderOrder {
        view.read_with(cx, |v, _| v.reading.feed.order)
    }

    /// The blyg keeps a different order than the one asked for: the Feed
    /// shows what it kept.
    #[gpui_kit::test]
    fn the_feed_shows_the_order_the_blyg_kept(cx: &mut TestAppContext) {
        let (view, fake, cx) = setup(cx);
        view.update_in(cx, |v, window, cx| v.show_view(View::Reading, window, cx));
        settle(cx);
        fake.reader_order_answers(Some(ReaderOrder::PaceLayered), false);
        view.update(cx, |v, cx| v.set_reader_order(ReaderOrder::Newest, cx));
        settle(cx);
        assert_eq!(order(&view, cx), ReaderOrder::PaceLayered);
        assert_eq!(fake.blyg_reader_order(), ReaderOrder::PaceLayered);
    }

    /// The save fails: the Feed goes back to the order it had, and says so.
    #[gpui_kit::test]
    fn a_failed_save_puts_the_order_back(cx: &mut TestAppContext) {
        let (view, fake, cx) = setup(cx);
        view.update_in(cx, |v, window, cx| v.show_view(View::Reading, window, cx));
        settle(cx);
        fake.reader_order_answers(None, true);
        view.update(cx, |v, cx| v.set_reader_order(ReaderOrder::Newest, cx));
        settle(cx);
        assert_eq!(order(&view, cx), ReaderOrder::PaceLayered);
        assert_eq!(fake.blyg_reader_order(), ReaderOrder::PaceLayered);
        let toast = view.read_with(cx, |v, _| v.toast.as_ref().map(|t| t.text.to_string()));
        assert!(
            toast.is_some_and(|t| t.starts_with("Couldn't change the order")),
            "says so"
        );
    }

    /// The blyg's order (chosen in the studio, say) replaces the one kept
    /// on this computer when Reading opens.
    #[gpui_kit::test]
    fn the_blygs_order_replaces_the_local_one(cx: &mut TestAppContext) {
        let (view, _fake, cx) = setup_with(cx, |f| {
            f.set_reader_order_state(ReaderOrder::Newest, Some(ReaderOrder::PaceLayered), true)
        });
        assert_eq!(
            order(&view, cx),
            ReaderOrder::PaceLayered,
            "the local one first"
        );
        view.update_in(cx, |v, window, cx| v.show_view(View::Reading, window, cx));
        settle(cx);
        assert_eq!(order(&view, cx), ReaderOrder::Newest);
    }

    /// A choice made before the session's first read of the blyg's order
    /// answers wins: the read doesn't undo it, and the blyg gets it.
    #[gpui_kit::test]
    fn a_choice_made_while_the_blygs_order_loads_wins(cx: &mut TestAppContext) {
        let (view, fake, cx) = setup_with(cx, |f| {
            f.set_reader_order_state(ReaderOrder::PaceLayered, None, false)
        });
        view.update_in(cx, |v, window, cx| {
            v.show_view(View::Reading, window, cx);
            v.set_reader_order(ReaderOrder::Newest, cx);
        });
        settle(cx);
        assert_eq!(order(&view, cx), ReaderOrder::Newest);
        assert_eq!(
            fake.blyg_reader_order(),
            ReaderOrder::Newest,
            "sent once the blyg answered"
        );
    }

    /// In the pace-layered Feed, ↓ and j step in the Feed's order, Space
    /// goes on to the next unread post in it, and Mark All Read marks what
    /// it shows.
    #[gpui_kit::test]
    fn keys_and_mark_all_follow_the_pace_layered_feed(cx: &mut TestAppContext) {
        use crate::app::reading::sources_vm::{Smart, Source};
        let (view, fake, cx) = setup(cx);
        view.update_in(cx, |v, window, cx| {
            v.show_view(View::Reading, window, cx);
            v.set_read_mode(crate::app::reading::stream_vm::ReadMode::Reader, window, cx);
            v.select_source(Source::Smart(Smart::All), cx);
        });
        settle(cx);
        assert_eq!(order(&view, cx), ReaderOrder::PaceLayered);
        let shown = |view: &Entity<MainView>, cx: &mut VisualTestContext| -> Vec<String> {
            view.read_with(cx, |v, _| {
                v.reading
                    .shown_rows()
                    .map(|r| r.remote_id.to_string())
                    .collect()
            })
        };
        let opened = |view: &Entity<MainView>, cx: &mut VisualTestContext| {
            view.read_with(cx, |v, _| {
                v.reading
                    .opened
                    .as_ref()
                    .map(|o| o.item.remote_id.to_string())
            })
        };
        let list = shown(&view, cx);
        assert!(list.len() > 3);
        cx.simulate_keystrokes("down");
        settle(cx);
        assert_eq!(opened(&view, cx).as_deref(), Some(list[0].as_str()));
        cx.simulate_keystrokes("j");
        settle(cx);
        let second = opened(&view, cx).unwrap();
        assert_eq!(
            second,
            shown(&view, cx)[1],
            "j: the next in the Feed's order"
        );
        cx.simulate_keystrokes("space");
        settle(cx);
        let next = opened(&view, cx).unwrap();
        view.read_with(cx, |v, _| {
            let rows: Vec<_> = v.reading.shown_rows().collect();
            let at = |id: &str| rows.iter().position(|r| r.remote_id == id).unwrap();
            assert!(at(&next) > at(&second), "Space goes forward in the Feed");
            for r in &rows[at(&second) + 1..at(&next)] {
                assert!(
                    !crate::app::reading::sources_vm::needs_reading(r),
                    "Space skips only what's read: {}",
                    r.remote_id
                );
            }
        });
        // Mark All Read on the Feed: everything it shows to read is sent
        // as read.
        let wanted = view.read_with(cx, |v, _| v.mark_all_keys(None));
        assert!(!wanted.is_empty());
        view.update_in(cx, |v, window, cx| {
            v.ask_mark_all_read(None, window, cx);
            v.confirm_mark_all(window, cx);
        });
        settle(cx);
        let sent = fake.reading_sends();
        for (sub, id) in wanted {
            assert!(
                sent.contains(&format!("set_read:{sub}:{id}")),
                "{id} marked read: {sent:?}"
            );
        }
    }
}
