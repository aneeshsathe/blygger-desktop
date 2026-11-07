//! Hover cards: the directive grammar and the cards (pure), then the
//! editor's hover, "label it" and ⌘-click in a headless window against a
//! zero-latency FakeBackend.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, Item, Kind, LocalId, ReadingItem, ServerId, Status};
use gpui_kit::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

use super::{
    Card, DELAY, Directive, EXCERPT_CHARS, Place, directive_at, directives, editor_card,
    excerpt_of, labelled, row_card, unheld_card,
};
use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const ID: &str = "7c9wk2n4h6q1x8v0z3m5rjy2ke";

// ------------------------------------------------------------------ pure

#[test]
fn directives_are_links_and_quotes_with_their_ranges() {
    let t = format!("See [[{ID}]] and [[{ID} | the bench]].\n\n![[{ID} | a note]]\n");
    let ds = directives(&t);
    assert_eq!(ds.len(), 3);
    assert!(!ds[0].quote && ds[0].label.is_none());
    assert_eq!(&t[ds[0].range.clone()], format!("[[{ID}]]"));
    assert_eq!(ds[1].label.as_deref(), Some("the bench"));
    assert!(ds[2].quote);
    assert_eq!(&t[ds[2].range.clone()], format!("![[{ID} | a note]]"));
    let at = t.find("![[").unwrap();
    assert_eq!(directive_at(&t, at).map(|d| d.quote), Some(true));
    assert_eq!(directive_at(&t, 0), None);
    assert_eq!(
        labelled(&t, &ds[0], "Bench notes"),
        t.replacen(&format!("[[{ID}]]"), &format!("[[{ID} | Bench notes]]"), 1)
    );
}

fn own(id: &str, status: Status, version: u32, text: &str) -> Item {
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

fn row(id: &str, state: &str, version: u32, text: &str) -> ReadingItem {
    ReadingItem {
        subscription_id: "sub-lin".into(),
        remote_id: id.into(),
        subscription_title: "Lin".into(),
        origin: "https://lin.example.org/".into(),
        kind: Kind::Thread,
        state: state.into(),
        version,
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

fn link(id: &str) -> Directive {
    let t = format!("[[{id}]]");
    directives(&t).remove(0)
}

#[test]
fn cards_say_what_this_mac_holds() {
    let mine = own(
        ID,
        Status::Public,
        3,
        "Bench notes\n\nWhat the bench taught me about patience and glue.",
    );
    let c = editor_card(&link(ID), &[mine], &[] as &[ReadingItem], None);
    assert_eq!(c.title, "Bench notes");
    assert_eq!(c.meta, "You · ≡ thread · v3");
    assert_eq!(
        c.excerpt.as_deref(),
        Some("What the bench taught me about patience and glue.")
    );
    assert_eq!(
        c.label_it.as_deref(),
        Some("Bench notes"),
        "an unlabelled link"
    );
    assert!(matches!(c.place, Place::Own(_)));

    // A labelled link, or a quote, isn't offered "label it".
    let t = format!("[[{ID} | glue]]");
    let labelled_link = directives(&t).remove(0);
    let mine = own(ID, Status::Public, 3, "Bench notes");
    assert_eq!(
        editor_card(&labelled_link, &[mine], &[] as &[ReadingItem], None).label_it,
        None
    );

    // A draft says it isn't published.
    let draft = own(ID, Status::Draft, 0, "Half a thought");
    let c = editor_card(&link(ID), &[draft], &[] as &[ReadingItem], None);
    assert!(c.meta.ends_with("draft"));
    assert!(c.note.unwrap().starts_with("Not published yet"));
}

#[test]
fn someone_elses_post_shows_now_and_withdrawn_shows_nothing() {
    let r = row(
        ID,
        "current",
        4,
        "Tides\n\nThe sea never signed the tables.",
    );
    let c = row_card(&r, Some(2), true, None);
    assert!(c.meta.ends_with("v4"), "{}", c.meta);
    assert_eq!(c.note.as_deref(), Some("Quoted at v2; this is v4."));
    assert_eq!(
        c.excerpt.as_deref(),
        Some("The sea never signed the tables.")
    );
    assert!(matches!(c.place, Place::Held { .. }));

    let gone = row(
        ID,
        "tombstone",
        4,
        "Tides\n\nThe sea never signed the tables.",
    );
    let c = editor_card(&link(ID), &[], std::slice::from_ref(&gone), None);
    assert_eq!(c.excerpt, None, "a withdrawn post shows no text");
    assert_eq!(c.note.as_deref(), Some("Withdrawn by its author."));
    assert!(c.meta.contains("withdrawn"));
    assert_eq!(c.label_it, None);
    // A current row wins over its withdrawn copy.
    let live = row(ID, "current", 5, "Tides\n\nNow with more moon.");
    let c = editor_card(&link(ID), &[], &[gone, live], None);
    assert!(c.meta.ends_with("v5"));

    // Not held: no text, and nowhere to open a bare link from.
    let c = editor_card(&link(ID), &[], &[] as &[ReadingItem], None);
    assert_eq!(c.title, "A post not held on this Mac");
    assert!(!c.opens());
    let cited = blyg_core::Cited {
        source: Some("Rue's Notebook".into()),
        author: Some("Rue".into()),
        excerpt: Some("An edit you can't read.".into()),
        url: None,
        retrieved: None,
    };
    let c = unheld_card(ID, Some("https://rue.example.org/"), Some(&cited), true);
    assert_eq!(c.meta, "Rue · rue.example.org");
    assert_eq!(c.excerpt.as_deref(), Some("An edit you can't read."));
    assert!(c.opens());
}

#[test]
fn excerpts_skip_the_title_and_stop_at_160() {
    let long = format!("Title\n\n{}", "word ".repeat(100));
    let e = excerpt_of(&long, "").unwrap();
    assert!(!e.starts_with("Title"));
    assert_eq!(e.chars().count(), EXCERPT_CHARS);
    assert!(e.ends_with('…'));
    assert_eq!(
        excerpt_of("", "<p>Only <b>HTML</b> &amp; text</p>").as_deref(),
        Some("Only HTML & text")
    );
    assert_eq!(excerpt_of("Just a title", ""), None);
}

// ------------------------------------------------------------------ the window

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";
const DRAFT: &str = "01J9QK3";
const PUBLIC: &str = "01J9PX1";
/// The public post's server id, in the blyg alphabet (`set_server_id`).
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
    fake.set_server_id(&LocalId(PUBLIC.into()), PUBLIC_SERVER);
    (view, fake, cx)
}

/// The draft open with `text` in the editor, drawn.
fn with_text(view: &Entity<MainView>, cx: &mut VisualTestContext, text: &str) {
    view.update_in(cx, |v, window, cx| {
        v.open(&LocalId(DRAFT.into()), window, cx);
        v.set_editor_text(text, window, cx);
        v.after_edit(cx);
    });
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

/// Where `needle` is drawn in the editor (window coordinates).
fn spot(
    view: &Entity<MainView>,
    cx: &mut VisualTestContext,
    needle: &str,
) -> gpui_kit::Point<gpui_kit::Pixels> {
    view.read_with(cx, |v, cx| {
        let s = v.editor.read(cx);
        let at = s.value().find(needle).expect("in the text");
        let b = s
            .range_to_bounds(&(at..at + needle.len()))
            .expect("laid out");
        b.center()
    })
}

fn shown(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<Card> {
    view.read_with(cx, |v, _| v.hover_card().cloned())
}

#[gpui_kit::test]
fn hovering_a_link_shows_its_card_and_label_it_labels_it(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    let link = format!("[[{PUBLIC_SERVER}]]");
    with_text(
        &view,
        cx,
        &format!("A note about the band: {link} and more."),
    );

    // Resting on it: nothing before the delay, the card after.
    let at = spot(&view, cx, &link);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.run_until_parked();
    assert!(shown(&view, cx).is_none(), "not before the delay");
    cx.executor().advance_clock(DELAY);
    cx.run_until_parked();
    let card = shown(&view, cx).expect("a card");
    assert!(
        card.title.starts_with("Band on Harbour Road"),
        "{}",
        card.title
    );
    assert!(
        card.meta.starts_with("You · ◦ fragment · v1"),
        "{}",
        card.meta
    );
    assert!(card.label_it.is_some());
    assert!(cx.debug_bounds("hover-card").is_some(), "drawn");

    // Away from it: the card goes.
    let elsewhere = spot(&view, cx, "A note");
    cx.simulate_mouse_move(elsewhere, None, Modifiers::none());
    cx.run_until_parked();
    assert!(shown(&view, cx).is_none());

    // Back, then "label it".
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.executor().advance_clock(DELAY);
    cx.run_until_parked();
    view.update_in(cx, |v, window, cx| v.hover_label_it(window, cx));
    cx.run_until_parked();
    let text = view.read_with(cx, |v, cx| v.editor.read(cx).value().to_string());
    let label = card.label_it.unwrap();
    assert!(
        text.contains(&format!("[[{PUBLIC_SERVER} | {label}]]")),
        "{text}"
    );
    assert!(shown(&view, cx).is_none());
}

#[gpui_kit::test]
fn cmd_click_opens_your_post_in_the_editor_and_anothers_in_reading(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    let quote = format!("![[{}]]", crate::fake::SAMPLE_QUOTE_ID);
    let link = format!("[[{PUBLIC_SERVER}]]");
    with_text(&view, cx, &format!("{link}\n\n{quote}\n"));

    // A plain click only moves the caret.
    let at = spot(&view, cx, &link);
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
    let current = |view: &Entity<MainView>, cx: &mut VisualTestContext| {
        view.read_with(cx, |v, _| v.current.as_ref().map(|c| c.local_id.0.clone()))
    };
    assert_eq!(current(&view, cx).as_deref(), Some(DRAFT));

    // ⌘-click: your post opens in the editor.
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::secondary_key());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::secondary_key());
    cx.run_until_parked();
    assert_eq!(current(&view, cx).as_deref(), Some(PUBLIC));

    // Someone else's post opens in Reading, as following a quote does.
    with_text(&view, cx, &format!("{link}\n\n{quote}\n"));
    let at = spot(&view, cx, &quote);
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::secondary_key());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::secondary_key());
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        assert_eq!(v.reading.view, crate::app::reading::View::Reading);
        let opened = v.reading.opened.as_ref().expect("opened in Reading");
        assert_eq!(opened.item.remote_id, crate::fake::SAMPLE_QUOTE_ID);
    });
}

#[gpui_kit::test]
fn the_card_shows_from_the_store_when_reading_was_never_opened(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    let quote = format!("![[{}]]", crate::fake::SAMPLE_QUOTE_ID);
    with_text(&view, cx, &format!("Pools\n\n{quote}\n"));
    view.update(cx, |v, cx| {
        v.reading.rows.clear(); // as before Reading ever opened
        let d = directive_at(&v.editor.read(cx).value(), 8).expect("the quote");
        v.hover_show(d, cx);
    });
    let card = shown(&view, cx).expect("a card");
    assert_eq!(
        card.meta,
        "Shoreline Notes · notes.example.org · ◦ fragment · v2"
    );
    assert!(card.quote);
    // A one-line fragment is all title.
    assert!(card.title.starts_with("A tide pool"), "{}", card.title);
}
