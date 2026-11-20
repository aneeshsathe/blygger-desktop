//! --- background --- The reading screen's Background rules: where a row
//! is listed, and which chips it gets.

use blyg_core::background::{BackgroundSet, Placement};
use blyg_core::{Kind, OWN_SUBSCRIPTION, ReadingItem};
use serde_json::json;

use super::background::{BLYG, CLEAR, PLACE, chips};
use super::own::with_background;
use super::sources_vm::{Smart, Source, in_source, needs_reading};

fn row(sub: &str, id: &str, state: &str, at: &str) -> ReadingItem {
    serde_json::from_value(json!({
        "subscription_id": sub, "remote_id": id, "subscription_title": "Ada",
        "origin": "https://ada.blyg.example.com/", "kind": "fragment", "state": state,
        "version": 2, "created": at, "updated": at, "observed_at": at,
        "content_md": "A sample post", "content_html": "", "author": null, "page": null,
        "thumb": null, "hoppers": [],
    }))
    .unwrap()
}

fn entry(
    sub: &str,
    id: &str,
    reasons: &[&str],
    placement: Option<&str>,
    blyg: bool,
) -> serde_json::Value {
    if sub == OWN_SUBSCRIPTION {
        json!({"source": "own", "own": {"id": id}, "background": {"reasons": reasons, "placement": placement, "blyg": blyg}})
    } else {
        json!({"source": "imported", "imported": {"subscriptionId": sub, "remoteId": id},
               "background": {"reasons": reasons, "placement": placement, "blyg": blyg}})
    }
}

fn ids(rows: &[ReadingItem], source: &Source) -> Vec<String> {
    let now = chrono::Utc::now();
    rows.iter()
        .filter(|r| in_source(r, source, &Default::default(), now))
        .map(|r| r.remote_id.to_string())
        .collect()
}

/// Studio 0.64: a thread lists its burst's parts ("+N parts"), listed when
/// open and folded on a stream card until unfolded; each part says why
/// it's in Background, the thread says nothing.
#[test]
fn a_thread_knows_its_parts_and_each_part_says_why() {
    use super::background::{Background, why};
    let mut rows = vec![
        row("s", "t", "current", "2026-11-05T10:10:00Z"),
        row("s", "p1", "current", "2026-11-05T10:00:00Z"),
        row("s", "p2", "current", "2026-11-05T10:05:00Z"),
    ];
    rows[0].kind = Kind::Thread;
    let part = |id: &str| {
        json!({"source": "imported", "imported": {"subscriptionId": "s", "remoteId": id},
               "background": {"reasons": ["burst"], "placement": null, "blyg": false,
                              "detail": {"thread": "t"}}})
    };
    let set = BackgroundSet::from_entries(&[part("p2"), part("p1")], None).indexed();
    set.annotate(&mut rows);
    let mut b = Background::default();
    b.set_rows(&rows);
    let key = |id: &str| (blyg_core::SubId::from("s"), blyg_core::RemoteId::from(id));
    assert_eq!(b.parts_of(&key("t")), [key("p1"), key("p2")]);
    assert!(b.parts_of(&key("p1")).is_empty());
    assert!(b.parts_listed(&key("t"), true) && !b.parts_listed(&key("t"), false));
    assert_eq!(why(&rows[1]).as_deref(), Some("published with a thread"));
    assert_eq!(why(&rows[0]), None, "the thread is in the foreground");
    let now = chrono::Utc::now();
    assert!(
        !rows
            .iter()
            .filter(|r| in_source(r, &Source::Smart(Smart::All), &Default::default(), now))
            .any(|r| r.remote_id != "t"),
        "only the thread is listed outside Background"
    );
}

#[test]
fn background_posts_leave_every_other_list_for_background() {
    let mut main = vec![
        row("s", "moved", "current", "2026-11-05T10:00:00Z"),
        row("s", "plain", "current", "2026-11-04T10:00:00Z"),
        row(
            OWN_SUBSCRIPTION,
            "mine-typo",
            "current",
            "2026-11-03T10:00:00Z",
        ),
    ];
    main[2].kind = Kind::Thread;
    let mut withdrawn = vec![
        row("s", "gone", "tombstone", "2026-11-02T10:00:00Z"),
        row("s", "kept", "tombstone", "2026-11-01T10:00:00Z"),
    ];
    let mut set = BackgroundSet::from_entries(
        &[
            entry("s", "moved", &[], Some("background"), false),
            entry(OWN_SUBSCRIPTION, "mine-typo", &["minor"], None, false),
            entry("s", "gone", &["withdrawn"], None, false),
        ],
        None,
    );
    // You brought one withdrawn post back to the foreground.
    set.place(
        &("s".into(), "kept".into()),
        true,
        Some(Placement::Foreground),
    );
    set.annotate(&mut main);
    set.annotate(&mut withdrawn);
    let rows = with_background(super::vm::order(main), withdrawn);
    assert_eq!(ids(&rows, &Source::Smart(Smart::All)), ["plain", "kept"]);
    assert_eq!(
        ids(&rows, &Source::Smart(Smart::Background)),
        ["moved", "mine-typo", "gone"]
    );
    assert!(
        ids(&rows, &Source::Smart(Smart::Threads)).is_empty(),
        "your minor change too"
    );
    assert!(ids(&rows, &Source::Own).is_empty());
    assert!(
        rows.iter()
            .filter(|r| r.is_background())
            .all(|r| !needs_reading(r))
    );
}

#[test]
fn without_the_studios_answer_withdrawn_is_background() {
    let main = vec![row("s", "live", "current", "2026-11-05T10:00:00Z")];
    let withdrawn = vec![row("s", "gone", "tombstone", "2026-11-02T10:00:00Z")];
    let rows = with_background(super::vm::order(main), withdrawn);
    assert_eq!(ids(&rows, &Source::Smart(Smart::All)), ["live"]);
    assert_eq!(ids(&rows, &Source::Smart(Smart::Background)), ["gone"]);
}

#[test]
fn chips_offer_the_studios_choices_only_when_served() {
    let mut r = row("s", "p", "current", "2026-11-05T10:00:00Z");
    assert!(
        chips(&r, false).is_empty(),
        "an older studio: no Background UI"
    );
    let set = BackgroundSet::default();
    set.annotate(std::slice::from_mut(&mut r));
    let c = chips(&r, true);
    assert_eq!(c.iter().map(|c| c.id).collect::<Vec<_>>(), [PLACE, BLYG]);
    assert_eq!(c[0].label, "↓ Background");
    assert_eq!(c[1].label, "⇊ Blyg to Background");

    let set = BackgroundSet::from_entries(&[entry("s", "p", &[], Some("background"), true)], None);
    set.annotate(std::slice::from_mut(&mut r));
    let c = chips(&r, true);
    assert_eq!(
        c.iter().map(|c| c.id).collect::<Vec<_>>(),
        [PLACE, CLEAR, BLYG]
    );
    assert_eq!(c[0].label, "↑ Foreground");
    assert_eq!(c[2].label, "⇈ Blyg to Foreground");

    let mut mine = row(OWN_SUBSCRIPTION, "o", "current", "2026-11-05T10:00:00Z");
    BackgroundSet::default().annotate(std::slice::from_mut(&mut mine));
    assert_eq!(
        chips(&mine, true).iter().map(|c| c.id).collect::<Vec<_>>(),
        [PLACE]
    );
}

// --- studio 0.64 --- the running app on the sample blyg: the burst
// thread's "+2 parts" in the stream and in its open pane, a part opening,
// and Settings offering every rule the blyg lists.
mod app {
    use std::sync::Arc;

    use blyg_core::config::MemoryTokenStore;
    use blyg_core::{Backend, ConfigStore};
    use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

    use crate::app::MainView;
    use crate::fake::background_seed::{LIN_PART_1, thread_key};
    use crate::fake::{FakeBackend, Timing};
    use crate::prefs::Prefs;
    use crate::test_wait::settle_drawn as settle;

    const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

    fn setup(
        cx: &mut TestAppContext,
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

    fn opened(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<String> {
        view.read_with(cx, |v, _| {
            v.reading
                .opened
                .as_ref()
                .map(|o| o.item.remote_id.to_string())
        })
    }

    #[gpui_kit::test]
    fn the_burst_thread_lists_its_parts_and_a_part_opens(cx: &mut TestAppContext) {
        let (view, _fake, cx) = setup(cx);
        for step in 0..2 {
            view.update_in(cx, |v, window, cx| v.burst_demo(step, window, cx));
            settle(cx);
        }
        assert_eq!(opened(&view, cx).as_deref(), Some(thread_key().1.as_ref()));
        assert_eq!(
            view.read_with(cx, |v, _| v
                .reading
                .background
                .parts_of(&thread_key())
                .len()),
            2
        );
        assert!(cx.debug_bounds("burst-toggle").is_some(), "on the card");
        assert!(
            cx.debug_bounds("burst-toggle-open").is_some(),
            "in the pane"
        );
        let part = cx.debug_bounds("burst-part-0").expect("a part listed");
        cx.simulate_click(part.center(), Modifiers::none());
        settle(cx);
        assert_eq!(
            opened(&view, cx).as_deref(),
            Some(LIN_PART_1),
            "the part opens"
        );
    }
}
