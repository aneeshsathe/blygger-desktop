//! Lineage while writing: the line's model (pure), then the line under the
//! editor in a headless window against a zero-latency FakeBackend.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::lineage::Local;
use blyg_core::{
    Backend, ConfigStore, Item, Kind, LocalId, ReadingItem, ServerId, Status, TransclusionRef,
};
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::{Strip, compute, link_ids, refs_print};
use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

// ------------------------------------------------------------------ pure

const OURS: &str = "https://blyg.example.com";
const LIN: &str = "https://lin.example.org/";
const MINE: &str = "0123456789abcdefghjkmnpqrs";
const THEIRS: &str = "zyxwvtsrqpnmkjhgfedcba9876";
const LINKED: &str = "7c9wk2n4h6q1x8v0z3m5rjy2ke";

fn item(id: &str, status: Status, version: u32, text: &str) -> Item {
    Item {
        local_id: LocalId(format!("local-{id}")),
        server_id: Some(ServerId(id.into())),
        kind: Kind::Thread,
        status,
        version,
        dirty: false,
        content_md: text.into(),
        created: String::new(),
        updated: String::new(),
        permalink: None,
        stub_of: None,
        forked_from: None,
        show_responses: true,
        responses_mode: None,
        pending_sync: false,
        conflict: false,
        highlight: false,
        highlight_mode: None,
    }
}

fn row(id: &str, text: &str) -> ReadingItem {
    ReadingItem {
        subscription_id: "sub-lin".into(),
        remote_id: id.into(),
        subscription_title: "Lin".into(),
        origin: LIN.into(),
        kind: Kind::Thread,
        state: "current".into(),
        version: 2,
        created: None,
        updated: None,
        observed_at: "2026-11-01T00:00:00Z".into(),
        content_md: text.into(),
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
    }
}

fn strip(it: &Item, text: &str, rows: &[ReadingItem], own: &[Item]) -> Strip {
    let local = Local::build(rows, own, Some(OURS), &[]);
    compute(it, text, rows, own, Some(OURS), &[], &local)
}

#[test]
fn a_published_post_shows_both_sides_with_the_glyphs_numbers() {
    let mine = item(
        MINE,
        Status::Public,
        3,
        &format!("Gardens\n\n![[{THEIRS}]]\n"),
    );
    let mut quoting = row("quoter0000000000000000000a", "Re: gardens\n\nYes.");
    quoting.transclusions.push(TransclusionRef {
        id: MINE.into(),
        origin: Some(OURS.into()),
        version: Some(3),
        selector: None,
        cited: None,
    });
    let rows = vec![
        row(THEIRS, "Rock pool economics\n\nScarcity, scheduled."),
        quoting,
    ];
    let s = strip(&mine, &mine.content_md, &rows, std::slice::from_ref(&mine));
    assert!(s.published);
    assert_eq!(s.line(), "draws on 1 · drawn on by 1");
    assert_eq!(s.draws_on[0].title, "Rock pool economics");
    assert_eq!(s.draws_on[0].who, "Lin");
    assert_eq!(s.drawn_on_by[0].title, "Re: gardens");
    assert_eq!(s.glyph.label(), "1 · 1");
}

#[test]
fn a_draft_draws_on_what_its_text_names_and_links_are_listed_apart() {
    let linked = item(LINKED, Status::Public, 1, "The notebook habit");
    let draft = item("draft00000000000000000000a", Status::Draft, 0, "");
    let text = format!("Tide pools\n\n![[{THEIRS}]]\n\nSee [[{LINKED}]] and [[{THEIRS}]].");
    let rows = vec![row(THEIRS, "Rock pool economics")];
    let s = strip(&draft, &text, &rows, &[linked, draft.clone()]);
    assert!(!s.published);
    assert_eq!(s.draws_on.len(), 1, "the quote");
    assert_eq!(s.draws_on[0].title, "Rock pool economics");
    assert!(s.drawn_on_by.is_empty());
    // The link to what it already quotes isn't listed twice.
    assert_eq!(s.links.len(), 1);
    assert_eq!(s.links[0].title, "The notebook habit");
    assert!(s.links[0].own);
    assert_eq!(s.line(), "draws on 1 · links to 1 post");
    assert_eq!(s.glyph.label(), "1 · 0");

    // Nothing named: nothing to show.
    assert!(strip(&draft, "Just words.", &rows, &[]).is_empty());
}

#[test]
fn the_fingerprint_follows_references_not_words() {
    let a = format!("One\n\n![[{THEIRS}]] [[{LINKED}]]");
    let b = format!("One more word\n\n![[{THEIRS}]] [[{LINKED}]]");
    assert_eq!(refs_print(&a), refs_print(&b));
    assert_ne!(refs_print(&a), refs_print("One"));
    assert_eq!(link_ids(&a), [LINKED]);
}

// ------------------------------------------------------------------ the window

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";
const DRAFT: &str = "01J9QK3";
const PUBLIC: &str = "01J9PX1";
/// The sample's "On friction" (public v3): Rue stubbed it and Ada quoted
/// it (verified mentions).
const FRICTION: &str = "01J9M2A";
const PUBLIC_SERVER: &str = "01j9px1abcdefghjkmnpqrstv0";

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
    (view, fake, cx)
}

fn open(view: &Entity<MainView>, cx: &mut VisualTestContext, which: &str) {
    view.update_in(cx, |v, window, cx| {
        v.open(&LocalId(which.into()), window, cx)
    });
    settle(view, cx);
}

/// Draw, and let the line's builds (and the mentions it asks for) land.
fn settle(view: &Entity<MainView>, cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.run_until_parked();
        view.update(cx, |v, cx| {
            v.lineage_strip_now(cx);
        });
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn shown(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<Strip> {
    view.read_with(cx, |v, _| v.lineage_strip_shown().map(|s| (*s).clone()))
}

fn current(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |v, _| v.current.as_ref().map(|c| c.local_id.0.clone()))
}

#[gpui_kit::test]
fn your_published_post_shows_who_draws_on_it_and_cmd_j_opens_the_sheet(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    open(&view, cx, FRICTION);
    let s = shown(&view, cx).expect("the line");
    assert!(s.published);
    assert_eq!(s.line(), "draws on 0 · drawn on by 2");
    let whos: Vec<&str> = s.drawn_on_by.iter().map(|e| e.who.as_str()).collect();
    assert!(whos.contains(&"Rue") && whos.contains(&"Ada"), "{whos:?}");
    assert!(
        cx.debug_bounds("lineage-strip").is_some(),
        "under the editor"
    );
    assert!(cx.debug_bounds("lineage-strip-lists").is_none(), "folded");

    // A click opens the lists.
    let b = cx.debug_bounds("lineage-strip").unwrap();
    cx.simulate_click(b.center(), Modifiers::none());
    settle(&view, cx);
    assert!(cx.debug_bounds("lineage-strip-lists").is_some());

    // ⌘J opens the lineage sheet for it, as on Reading.
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-j"));
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.lineage_open()));
}

#[gpui_kit::test]
fn a_draft_lists_what_its_text_draws_on_and_a_row_opens_it(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    fake.set_server_id(&LocalId(PUBLIC.into()), PUBLIC_SERVER);
    open(&view, cx, DRAFT);
    assert!(
        shown(&view, cx).is_none_or(|s| s.is_empty()),
        "a plain draft"
    );

    let text = format!(
        "Pools at dusk\n\n![[{}]]\n\n![[{PUBLIC_SERVER}]]\n",
        crate::fake::SAMPLE_QUOTE_ID
    );
    view.update_in(cx, |v, window, cx| {
        v.set_editor_text(&text, window, cx);
        v.after_edit(cx);
    });
    settle(&view, cx);
    let s = shown(&view, cx).expect("the line");
    assert!(!s.published);
    assert_eq!(s.line(), "draws on 2");
    assert!(s.draws_on[0].title.starts_with("A tide pool"));
    assert!(s.draws_on[1].own);

    // ⌘J on a draft opens the lists (there's no published post to centre).
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-j"));
    settle(&view, cx);
    assert!(view.read_with(cx, |v, _| !v.lineage_open() && v.lineage_strip.open));
    assert!(cx.debug_bounds("lineage-strip-lists").is_some());

    // Your post opens in the editor…
    let mine = s.draws_on[1].clone();
    view.update_in(cx, |v, window, cx| v.lineage_strip_open(mine, window, cx));
    cx.run_until_parked();
    assert_eq!(current(&view, cx).as_deref(), Some(PUBLIC));
    // …and someone else's in Reading.
    let theirs = s.draws_on[0].clone();
    view.update_in(cx, |v, window, cx| v.lineage_strip_open(theirs, window, cx));
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        assert_eq!(v.reading.view, crate::app::reading::View::Reading);
        let o = v.reading.opened.as_ref().expect("opened");
        assert_eq!(o.item.remote_id, crate::fake::SAMPLE_QUOTE_ID);
    });
}

#[gpui_kit::test]
fn without_reading_rows_in_memory_the_line_builds_off_the_main_thread(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    view.update(cx, |v, _| v.reading.rows.clear());
    view.update_in(cx, |v, window, cx| {
        v.open(&LocalId(FRICTION.into()), window, cx)
    });
    // The first ask starts the build and has nothing of this post's yet.
    let first = view.update(cx, |v, cx| v.lineage_strip_now(cx));
    assert!(first.is_none());
    settle(&view, cx);
    let s = shown(&view, cx).expect("built");
    assert_eq!(s.line(), "draws on 0 · drawn on by 2");
}
