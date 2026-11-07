//! --- tray --- drag-to-quote and the quote tray: the pure rules, then a
//! headless window on a zero-latency FakeBackend.

use blyg_core::state::TrayEntry;
use blyg_core::{Backend, Kind, LocalId, PickSource, Pickable};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::trails_tests::{reading_on, settle, setup};
use super::tray::{DraggedQuote, Tray, drop_into, tray_first};
use crate::app::MainView;
use crate::app::reading::View;
use crate::app::reading::stream_vm::ReadMode;
use crate::fake::reading_seed::*;

const ID: &str = "01k2ada0t1des0000000000001";

#[test]
fn a_drop_in_a_thread_is_a_quote_on_its_own_line_after_that_line() {
    let text = "# Tide pools\nThree things fit.\nThe end.";
    let at = text.find("things").unwrap();
    let (out, caret) = drop_into(text, at, true, ID, Some("Tides"));
    assert_eq!(
        out,
        format!("# Tide pools\nThree things fit.\n![[{ID} | Tides]]\nThe end.")
    );
    assert_eq!(
        &out[..caret - 1],
        format!("# Tide pools\nThree things fit.\n![[{ID} | Tides]]")
    );
    // Bare below studio 0.47 (no label given).
    let (out, _) = drop_into("A", 1, true, ID, None);
    assert_eq!(out, format!("A\n![[{ID}]]\n"));
}

#[test]
fn a_drop_in_a_fragment_is_an_inline_link_where_it_landed() {
    let text = "Tides and gardens.";
    let at = text.find("and").unwrap();
    let (out, _) = drop_into(text, at, false, ID, Some("Tides"));
    assert_eq!(out, format!("Tides [[{ID} | Tides]]and gardens."));
}

fn pick(id: &str, title: &str) -> Pickable {
    Pickable {
        id: id.into(),
        title: title.into(),
        excerpt: "excerpt".into(),
        label: title.into(),
        source: PickSource::Imported,
        kind: Kind::Fragment,
        version: 2,
        at: String::new(),
        subscription_id: Some("s".into()),
        source_title: Some("Ada".into()),
    }
}

fn entry(id: &str, title: &str) -> TrayEntry {
    TrayEntry {
        id: id.into(),
        label: title.into(),
        title: title.into(),
        subscription_id: "s".into(),
    }
}

#[test]
fn the_picker_lists_the_tray_first_newest_first_and_only_once() {
    let tray = [entry("a", "Rock pools"), entry("b", "Weeding")];
    let found = vec![pick("x", "Other"), pick("a", "Rock pools")];
    let out = tray_first(&tray, found.clone(), "");
    let ids: Vec<_> = out.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["b", "a", "x"]);
    assert!(out[0].title.starts_with('❝'));
    assert_eq!(out[1].excerpt, "excerpt", "a held post keeps its excerpt");
    // A query filters the tray too.
    let out = tray_first(&tray, found, "weed");
    assert_eq!(out[0].id, "b");
    assert!(!out.iter().any(|p| p.id == "a" && p.title.starts_with('❝')));
}

#[test]
fn the_tray_keeps_one_of_each_and_drops_the_oldest() {
    let mut t = Tray::load(None, "blyg.example.com".into());
    assert!(t.add(entry("a", "A")));
    assert!(t.add(entry("b", "B")));
    assert!(!t.add(entry("a", "A")), "again: moved to the end");
    let ids: Vec<_> = t.items.iter().map(|e| e.id.clone()).collect();
    assert_eq!(ids, ["b", "a"]);
    for i in 0..30 {
        t.add(entry(&format!("n{i}"), "N"));
    }
    assert_eq!(t.items.len(), super::tray::MAX);
    t.remove("n29");
    assert!(!t.items.iter().any(|e| e.id == "n29"));
}

#[test]
fn the_tray_is_kept_per_blyg_in_the_state_file() {
    let dir = std::env::temp_dir().join(format!("burrow-tray-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut t = Tray::load(Some(dir.clone()), "a.example.com".into());
    t.add(entry("a", "A"));
    let again = Tray::load(Some(dir.clone()), "a.example.com".into());
    assert_eq!(again.items.len(), 1);
    let other = Tray::load(Some(dir.clone()), "b.example.com".into());
    assert!(other.items.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

fn thread(view: &Entity<MainView>, cx: &mut VisualTestContext, kind: Kind, text: &str) -> LocalId {
    let id = view.update_in(cx, |v, window, cx| {
        let id = v.backend.create_draft(kind, text).unwrap();
        v.open_new_draft(&id, window, cx);
        id
    });
    settle(cx);
    id
}

fn text(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, cx| v.editor.read(cx).value().to_string())
}

#[gpui_kit::test]
fn q_puts_the_post_on_the_tray_and_the_picker_places_it_first(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    reading_on(&view, ReadMode::Stream, ("sub-ada", ADA_TIDES), cx);
    cx.simulate_keystrokes("q");
    settle(cx);
    let label = view.read_with(cx, |v, _| {
        assert_eq!(v.tray.items.len(), 1);
        assert_eq!(v.tray.items[0].id, ADA_TIDES);
        v.tray.items[0].label.clone()
    });
    thread(&view, cx, Kind::Thread, "# Tide pools\n\nThree things.");
    view.update_in(cx, |v, window, cx| {
        v.reading.view = View::Posts;
        v.open_quote_picker(window, cx)
    });
    settle(cx);
    let first = view.read_with(cx, |v, cx| v.quote_candidates(cx).first().cloned());
    let first = first.expect("candidates");
    assert_eq!(first.id, ADA_TIDES);
    assert!(first.title.starts_with('❝'));
    view.update_in(cx, |v, window, cx| v.pick_quote(window, cx));
    settle(cx);
    assert!(text(&view, cx).contains(&format!("![[{ADA_TIDES} | {label}]]")));
    assert!(
        view.read_with(cx, |v, _| v.tray.items.is_empty()),
        "placed, so off the tray"
    );
}

#[gpui_kit::test]
fn dropping_a_post_quotes_it_in_a_thread_and_links_it_in_a_fragment(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    // The reading rows are held once Reading has been shown.
    reading_on(&view, ReadMode::Stream, ("sub-ada", ADA_TIDES), cx);
    let drag = view.read_with(cx, |v, _| {
        let r = v
            .reading
            .rows
            .iter()
            .find(|r| r.remote_id == LIN_GARDENS)
            .unwrap();
        DraggedQuote::row(r, "Gardens".into())
    });
    thread(&view, cx, Kind::Thread, "# Pair\n\nFirst line.");
    view.update_in(cx, |v, window, cx| {
        v.drop_quote_in_editor(&drag, None, window, cx)
    });
    settle(cx);
    let t = text(&view, cx);
    assert!(t.contains(&format!("\n![[{LIN_GARDENS} | ")), "{t}");

    thread(&view, cx, Kind::Fragment, "Short one.");
    view.update_in(cx, |v, window, cx| {
        v.drop_quote_in_editor(&drag, None, window, cx)
    });
    settle(cx);
    let t = text(&view, cx);
    assert!(t.contains(&format!("[[{LIN_GARDENS} | ")), "{t}");
    assert!(
        !t.contains("![["),
        "a fragment gets a link, not a quote: {t}"
    );
}

#[gpui_kit::test]
fn dropping_on_the_omnibar_starts_a_thread_and_feeds_are_refused(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    reading_on(&view, ReadMode::Stream, ("sub-ada", ADA_TIDES), cx);
    let (tides, feed) = view.read_with(cx, |v, _| {
        let find = |id: &str| {
            v.reading
                .rows
                .iter()
                .find(|r| r.remote_id == id)
                .unwrap()
                .clone()
        };
        (
            DraggedQuote::row(&find(ADA_TIDES), "Tides".into()),
            DraggedQuote::row(&find(OMAR_YEAR), "Year".into()),
        )
    });
    let before = fake.items().len();
    view.update_in(cx, |v, window, cx| {
        v.drop_quote_on_omnibar(&feed, window, cx)
    });
    settle(cx);
    assert_eq!(fake.items().len(), before, "a feed's post can't be quoted");
    view.update_in(cx, |v, window, cx| {
        v.drop_quote_on_omnibar(&tides, window, cx)
    });
    settle(cx);
    let cur = view.read_with(cx, |v, _| v.current.clone()).unwrap();
    assert_eq!(cur.kind, Kind::Thread);
    assert!(cur.content_md.starts_with(&format!("![[{ADA_TIDES} | ")));
}

#[gpui_kit::test]
fn dropping_on_the_compose_box_quotes_it_there(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    reading_on(&view, ReadMode::Stream, ("sub-ada", ADA_TIDES), cx);
    let drag = view.read_with(cx, |v, _| {
        let r = v
            .reading
            .rows
            .iter()
            .find(|r| r.remote_id == LIN_GARDENS)
            .unwrap();
        DraggedQuote::row(r, "Gardens".into())
    });
    view.update_in(cx, |v, window, cx| {
        v.drop_quote_in_compose(&drag, window, cx)
    });
    settle(cx);
    let box_text = view.read_with(cx, |v, cx| v.reading.compose.text(cx));
    assert!(
        box_text.contains(&format!("![[{LIN_GARDENS}")),
        "{box_text}"
    );
}

#[gpui_kit::test]
fn a_drop_lands_on_the_line_under_the_pointer(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    let drag = DraggedQuote {
        id: ADA_TIDES.into(),
        title: "Tides".into(),
        label: Some("Tides".into()),
        key: None,
        from_tray: false,
    };
    thread(&view, cx, Kind::Thread, "One.\nTwo.\nThree.");
    // The caret is at the end; the pointer is over "Two.".
    let pos = view.read_with(cx, |v, cx| {
        v.editor
            .read(cx)
            .range_to_bounds(&(5..9))
            .map(|b| b.center())
    });
    let pos = pos.expect("the editor is laid out");
    view.update_in(cx, |v, window, cx| {
        v.drop_quote_in_editor(&drag, Some(pos), window, cx)
    });
    settle(cx);
    assert_eq!(
        text(&view, cx),
        format!("One.\nTwo.\n![[{ADA_TIDES} | Tides]]\nThree.")
    );
}

#[test]
fn the_tour_tray_is_never_written() {
    let dir = std::env::temp_dir().join(format!("burrow-tray-tour-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut t = Tray::load(Some(dir.clone()), super::tray::TOUR.into());
    t.add(entry("a", "A"));
    let again = Tray::load(Some(dir.clone()), super::tray::TOUR.into());
    assert!(again.items.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
