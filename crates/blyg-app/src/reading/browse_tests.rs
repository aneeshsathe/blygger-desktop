//! --- browse --- The walk's pure logic (trail, breadcrumb, log grouping,
//! link collection), and the reader's browsing on a zero-latency
//! FakeBackend through the same methods the clicks and keys call.

use std::sync::Arc;

use blyg_core::browse::{BrowseStep, BrowseVia};
use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::browse::{self, Step, Trail};
use crate::app::MainView;
use crate::fake::reading_seed::*;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const A: &str = "https://a.blyg.example.com/";
const B: &str = "https://b.blyg.example.com/";

fn step(origin: &str, id: &str, title: &str) -> Step {
    Step::new(origin, id, Some(1), title)
}

fn fresh() -> String {
    "trail-0001".into()
}

// ---------------------------------------------------------------- trail

#[test]
fn opening_from_the_feed_starts_a_walk_logged_only_once_it_goes_on() {
    let mut t = Trail::default();
    assert_eq!(t.visit(step(A, "01a", "First"), fresh), None, "a lone read");
    assert_eq!(t.id.as_deref(), Some("trail-0001"));
    assert!(t.crumbs().is_empty(), "no breadcrumb for one entry");
    // Opening another post from the feed: a new walk, still nothing logged.
    assert_eq!(
        t.visit(step(A, "01b", "Second"), || "trail-0002".into()),
        None
    );
    assert_eq!(t.id.as_deref(), Some("trail-0002"));
    assert_eq!(t.stack.len(), 1);
    // Following a quote: the walk's first entry is logged now, then the step.
    t.follow(BrowseVia::Quote);
    let plan = t.visit(step(B, "01c", "Quoted"), fresh).expect("logged");
    let start = plan.start.expect("the first entry, late");
    assert_eq!(
        (start.remote_id.as_str(), start.via, start.parent_id),
        ("01b", BrowseVia::Start, None)
    );
    assert_eq!(start.trail, "trail-0002");
    assert_eq!(plan.step.via, BrowseVia::Quote);
    assert_eq!(plan.step.remote_id, "01c");
    assert_eq!(plan.step.title.as_deref(), Some("Quoted"));
    assert_eq!(t.crumbs(), ["Second", "Quoted"]);
}

#[test]
fn each_follow_names_the_step_it_was_reached_from() {
    let mut t = Trail::default();
    t.visit(step(A, "01a", "One"), fresh);
    t.follow(BrowseVia::Link);
    let p1 = t.visit(step(B, "01b", "Two"), fresh).unwrap();
    // The answers arrive: ids label the entries still on the trail.
    t.answered("trail-0001", A, "01a", 7);
    t.answered("trail-0001", B, "01b", 8);
    t.answered("another-trail", B, "01b", 99); // ignored
    assert_eq!(p1.step.parent_id, None, "filled from the start's answer");
    t.follow(BrowseVia::Stub);
    let p2 = t.visit(step(A, "01c", "Three"), fresh).unwrap();
    assert_eq!(p2.start, None, "the start is logged once");
    assert_eq!(p2.step.parent_id, Some(8));
    assert_eq!(p2.step.via, BrowseVia::Stub);
    assert_eq!(
        t.stack.iter().map(|s| s.step_id).collect::<Vec<_>>(),
        [Some(7), Some(8), None]
    );
}

#[test]
fn back_retraces_one_entry_at_a_time_and_is_logged() {
    let mut t = Trail::default();
    t.visit(step(A, "01a", "One"), fresh);
    t.follow(BrowseVia::Quote);
    t.visit(step(B, "01b", "Two"), fresh);
    t.answered("trail-0001", B, "01b", 2);
    t.follow(BrowseVia::Link);
    t.visit(step(A, "01c", "Three"), fresh);
    t.answered("trail-0001", A, "01c", 3);
    let to = t.back().expect("one back");
    assert_eq!(to.id, "01b");
    // The reader opens it: a "back" row, reached from the entry left.
    let plan = t.visit(step(B, "01b", "Two"), fresh).expect("logged");
    assert_eq!(plan.step.via, BrowseVia::Back);
    assert_eq!(plan.step.parent_id, Some(3));
    assert_eq!(plan.start, None);
    assert_eq!(t.crumbs(), ["One", "Two"]);
    // Its own id stays (a follow from here hangs under the first visit).
    assert_eq!(t.stack[1].step_id, Some(2));
    assert_eq!(t.back().map(|s| s.id), Some("01a".into()));
    t.visit(step(A, "01a", "One"), fresh);
    assert!(!t.can_go_back());
    assert_eq!(t.back(), None, "nothing behind the first entry");
}

#[test]
fn the_breadcrumb_jumps_back_without_logging() {
    let mut t = Trail::default();
    t.visit(step(A, "01a", ""), fresh);
    for (id, via) in [("01b", BrowseVia::Quote), ("01c", BrowseVia::Link)] {
        t.follow(via);
        t.visit(step(B, id, &format!("Post {id}")), fresh);
    }
    // An untitled entry is named by its id's first eight characters.
    assert_eq!(t.crumbs(), ["01a", "Post 01b", "Post 01c"]);
    assert_eq!(t.jump(2), None, "the current entry isn't a jump");
    assert_eq!(t.jump(0).map(|s| s.id), Some("01a".into()));
    assert_eq!(t.visit(step(A, "01a", "First"), fresh), None, "not logged");
    assert_eq!(t.stack.len(), 1);
    assert_eq!(t.stack[0].title, "First", "a late title is kept");
}

#[test]
fn reopening_the_same_entry_or_a_cancelled_open_changes_nothing() {
    let mut t = Trail::default();
    t.visit(step(A, "01a", "One"), fresh);
    t.follow(BrowseVia::Quote);
    t.visit(step(B, "01b", "Two"), fresh);
    let before = t.stack.clone();
    // A re-render, or a version step, of the same post.
    assert_eq!(t.visit(step(B, "01B", "Two"), fresh), None);
    // A follow whose fetch failed: the next feed open starts a new walk.
    t.follow(BrowseVia::Link);
    t.cancel();
    assert_eq!(t.stack, before);
    assert_eq!(
        t.visit(step(A, "01z", "Other"), || "trail-0002".into()),
        None
    );
    assert_eq!(t.id.as_deref(), Some("trail-0002"));
    t.end();
    assert!(t.stack.is_empty() && t.id.is_none());
}

#[test]
fn titles_sent_to_the_log_are_cut_to_the_servers_limit() {
    let mut t = Trail::default();
    t.visit(step(A, "01a", "One"), fresh);
    t.follow(BrowseVia::Link);
    let long = "x".repeat(500);
    let plan = t.visit(step(B, "01b", &long), fresh).unwrap();
    assert_eq!(plan.step.title.unwrap().chars().count(), browse::TITLE_MAX);
    // "…" (no title of its own) is no title.
    t.follow(BrowseVia::Link);
    let plan = t.visit(step(A, "01c", "…"), fresh).unwrap();
    assert_eq!(plan.step.title, None);
}

// ---------------------------------------------------------------- the log

fn row(id: i64, trail: &str, parent: Option<i64>, via: BrowseVia, title: &str) -> BrowseStep {
    BrowseStep {
        id,
        at: format!("2026-10-30T10:00:{id:02}Z"),
        trail: trail.into(),
        parent_id: parent,
        origin: A.into(),
        remote_id: format!("01k{id:023}"),
        version: Some(1),
        via,
        title: (!title.is_empty()).then(|| title.into()),
    }
}

#[test]
fn history_groups_walks_newest_first_as_trees() {
    // Newest first, as `browse_history` returns them.
    let log = vec![
        row(7, "t2", Some(6), BrowseVia::Link, "Later link"),
        row(6, "t2", None, BrowseVia::Start, "Later start"),
        row(5, "t1", Some(4), BrowseVia::Back, "Mid"),
        row(4, "t1", Some(3), BrowseVia::Stub, "Deep"),
        row(3, "t1", Some(2), BrowseVia::Quote, "Mid"),
        row(2, "t1", None, BrowseVia::Start, "Top"),
    ];
    let w = browse::walks(&log);
    assert_eq!(
        w.iter().map(|w| w.trail.as_str()).collect::<Vec<_>>(),
        ["t2", "t1"]
    );
    let t1 = &w[1];
    assert_eq!(t1.at, "2026-10-30T10:00:02Z", "when it started");
    assert_eq!(t1.opened, 3, "back steps don't count as opened");
    assert_eq!(
        t1.rows
            .iter()
            .map(|r| (r.step.id, r.depth))
            .collect::<Vec<_>>(),
        [(2, 0), (3, 1), (4, 2), (5, 2)]
    );
    assert_eq!(browse::via_word(BrowseVia::Quote), "quoted layer");
    assert_eq!(browse::via_word(BrowseVia::Stub), "stubbed source");
    assert_eq!(browse::label("", "01k2abcdefgh"), "01k2abcd");
    assert!(browse::walks(&[]).is_empty());
}

// ---------------------------------------------------------------- links

#[test]
fn a_posts_links_are_collected_absolute_once_each() {
    let html = r##"<p><a href="https://lin.blyg.example.com/f/01k211n0bench0000000000001/">bench</a>,
        <a class="x" href='t/01ABC/'>relative</a>, <a href="/f/01DEF/#top">rooted</a>,
        <a href="https://lin.blyg.example.com/f/01k211n0bench0000000000001/#again">again</a>,
        <a href="mailto:a@b.example">mail</a> <a href="#fn1">note</a>
        <a href="javascript:alert(1)">no</a> <abbr>not a link</abbr>
        <a href="https://b.example/?a=1&amp;b=2">amp</a></p>"##;
    let links = browse::body_links(html, "https://ada.blyg.example.com");
    assert_eq!(
        links,
        [
            "https://lin.blyg.example.com/f/01k211n0bench0000000000001/",
            "https://ada.blyg.example.com/t/01ABC/",
            "https://ada.blyg.example.com/f/01DEF/",
            "https://b.example/?a=1&b=2",
        ]
    );
    let many: String = (0..60)
        .map(|i| format!("<a href=\"https://x.example/{i}\">{i}</a>"))
        .collect();
    assert_eq!(browse::body_links(&many, A).len(), browse::LINKS_PER_POST);
    assert_eq!(
        browse::norm_url("https://Lin.blyg.example.com#x").as_deref(),
        Some("https://lin.blyg.example.com/")
    );
    assert_eq!(browse::norm_url("mailto:a@b.example"), None);
}

// ---------------------------------------------------------------- in the app

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup(cx: &mut TestAppContext) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
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
    view.update(cx, |v, _| {
        v.reading.mode = super::stream_vm::ReadMode::Reader
    });
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
    settle(cx);
    (view, fake, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..20 {
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn open_row(view: &Entity<MainView>, id: &str, cx: &mut VisualTestContext) {
    let key = view.read_with(cx, |v, _| {
        v.reading
            .rows
            .iter()
            .find(|r| r.remote_id == id)
            .map(super::vm::key)
            .expect("row held")
    });
    view.update_in(cx, |v, window, cx| v.open_reading(key, window, cx));
    settle(cx);
}

fn opened_id(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |v, _| {
        v.reading.opened.as_ref().map(|o| o.item.remote_id.clone())
    })
}

fn crumbs(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |v, _| v.reading.browse.trail.crumbs())
}

#[gpui_kit::test]
fn following_quotes_and_links_walks_and_cmd_bracket_comes_back(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open_row(&view, ADA_REPLY, cx);
    // Ada's reply quotes Lin's bench note: the quote box in the reader.
    view.update_in(cx, |v, window, cx| {
        v.browse_follow(
            BrowseVia::Quote,
            LIN.into(),
            LIN_BENCH.into(),
            Some(1),
            window,
            cx,
        )
    });
    settle(cx);
    assert_eq!(opened_id(&view, cx).as_deref(), Some(LIN_BENCH));
    // A link in it that names Kit's post (a blyg nobody here follows).
    let kit_link = format!("{KIT}f/{KIT_TIDES}/");
    view.update_in(cx, |v, _, _| {
        v.reading.browse.classified.insert(
            kit_link.clone(),
            Some(blyg_core::browse::ItemRef {
                origin: KIT.into(),
                id: KIT_TIDES.into(),
            }),
        )
    });
    view.update_in(cx, |v, window, cx| {
        v.browse_link(kit_link.clone(), false, true, window, cx)
    });
    settle(cx);
    assert_eq!(opened_id(&view, cx).as_deref(), Some(KIT_TIDES), "a peek");
    assert_eq!(crumbs(&view, cx).len(), 3);
    assert_eq!(crumbs(&view, cx)[0], "On turning beds over");
    // The log has the walk: start, quote, link, each under the one before.
    let log = fake.browse_history(None, 0, 50).unwrap();
    let vias: Vec<BrowseVia> = log.iter().rev().map(|s| s.via).collect();
    assert_eq!(vias, [BrowseVia::Start, BrowseVia::Quote, BrowseVia::Link]);
    assert_eq!(log[0].parent_id, Some(log[1].id));
    assert_eq!(log[1].parent_id, Some(log[2].id));
    // ⌘[ retraces it one entry at a time.
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-["));
    settle(cx);
    assert_eq!(opened_id(&view, cx).as_deref(), Some(LIN_BENCH));
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-["));
    settle(cx);
    assert_eq!(opened_id(&view, cx).as_deref(), Some(ADA_REPLY));
    assert!(crumbs(&view, cx).is_empty(), "back at the first entry");
    let log = fake.browse_history(None, 0, 50).unwrap();
    assert_eq!(log[0].via, BrowseVia::Back);
    assert_eq!(log[1].via, BrowseVia::Back);
    // Opening from the list starts a new walk.
    open_row(&view, RUE_TRUST, cx);
    let trails: std::collections::HashSet<String> = fake
        .browse_history(None, 0, 50)
        .unwrap()
        .into_iter()
        .map(|s| s.trail)
        .collect();
    assert_eq!(trails.len(), 1, "a lone read isn't logged");
    assert!(crumbs(&view, cx).is_empty());
}

#[gpui_kit::test]
fn links_are_classified_when_a_post_opens_and_plain_ones_stay_links(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    // Lin's bench note links its own notes page: asked, and not an item.
    open_row(&view, LIN_BENCH, cx);
    let answer = view.read_with(cx, |v, _| {
        v.reading
            .browse
            .target("https://lin.blyg.example.com/notes")
            .map(|t| t.cloned())
    });
    assert_eq!(answer, Some(None), "asked, and not a blyg item");
    // A page of a subscribed blyg's item is one (asked on the click).
    let link = format!("{ADA}f/{ADA_TIDES}/");
    view.update_in(cx, |v, window, cx| {
        v.browse_link(link.clone(), false, true, window, cx)
    });
    settle(cx);
    assert_eq!(opened_id(&view, cx).as_deref(), Some(ADA_TIDES));
    assert_eq!(crumbs(&view, cx).len(), 2, "a step of the walk");
}

#[gpui_kit::test]
fn a_peek_asks_to_subscribe_and_the_sheet_is_prefilled(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    view.update_in(cx, |v, window, cx| {
        v.open_original(KIT.into(), KIT_TIDES.into(), None, window, cx)
    });
    settle(cx);
    assert!(
        cx.debug_bounds("peek-subscribe").is_some(),
        "You're not subscribed to this blyg — subscribe?"
    );
    let at = cx.debug_bounds("peek-subscribe").unwrap().center();
    cx.simulate_click(at, gpui_kit::Modifiers::none());
    settle(cx);
    let typed = view.read_with(cx, |v, cx| match v.reading.sheet.as_ref() {
        Some(super::RSheet::Subscribe { input, .. }) => Some(input.read(cx).value().to_string()),
        _ => None,
    });
    assert_eq!(typed.as_deref(), Some(KIT));
}

#[gpui_kit::test]
fn the_history_pane_lists_walks_and_clears_them(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open_row(&view, ADA_REPLY, cx);
    view.update_in(cx, |v, window, cx| {
        v.browse_follow(
            BrowseVia::Quote,
            LIN.into(),
            LIN_BENCH.into(),
            None,
            window,
            cx,
        )
    });
    settle(cx);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-h"));
    settle(cx);
    assert!(cx.debug_bounds("browse-history").is_some());
    let rows = view.read_with(cx, |v, _| match &v.reading.browse.history {
        super::Load::Ready(r) => r.len(),
        _ => 0,
    });
    assert_eq!(rows, 2);
    // Hovering or pointing at it changes nothing; clearing takes two clicks.
    view.update_in(cx, |v, _, cx| v.clear_browse_history_for_test(cx));
    settle(cx);
    assert_eq!(fake.browse_history(None, 0, 50).unwrap().len(), 2);
    view.update_in(cx, |v, _, cx| v.clear_browse_history_for_test(cx));
    settle(cx);
    assert!(fake.browse_history(None, 0, 50).unwrap().is_empty());
    // esc closes the pane first, and the post stays open.
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(!view.read_with(cx, |v, _| v.reading.browse.history_open));
    assert_eq!(opened_id(&view, cx).as_deref(), Some(LIN_BENCH));
}
