//! --- nearby --- Nearby thoughts on a real store (SQLite through a
//! `LiveBackend` that never syncs): the ranking with no embedder (phrases)
//! and with a deterministic one (meaning), the index kept in the store and
//! brought up to date incrementally, then the rail in a headless window.
//! No test downloads a model: `ConceptEmbedder` stands in for one.
//!
//!     cargo test -p blyg-app --release nearby -- --nocapture   (speed numbers)

use std::sync::Arc;
use std::time::{Duration, Instant};

use blyg_core::api::wire::WireItem;
use blyg_core::config::MemoryTokenStore;
use blyg_core::nearby::{own_key, reading_key};
use blyg_core::store::Store;
use blyg_core::{
    Backend, ConfigStore, Kind, LiveBackend, LocalId, ReadingItem, Subscription, SubscriptionKind,
    SyncOptions, synth,
};
use blyg_embed::Embedder;
use blyg_embed::testing::ConceptEmbedder;
use gpui_kit::{Entity, Focusable as _, TestAppContext, VisualTestContext};

use super::engine::{self, Ask, Index, Progress};
use super::{DEBOUNCE, NearbyGlobal};
use crate::app::MainView;
use crate::prefs::Prefs;

const POOLS: &str = "01j9pq0000000000000000000a";
const GARDEN: &str = "01j9gd0000000000000000000b";
const R1: &str = "01j9rk0000000000000000000c";

fn published(id: &str, kind: &str, md: &str) -> WireItem {
    WireItem {
        id: id.into(),
        kind: kind.into(),
        status: "public".into(),
        version: 1,
        created: "2030-01-01T00:00:00Z".into(),
        updated: "2030-01-01T00:00:00Z".into(),
        content_md: md.into(),
        ..Default::default()
    }
}

fn row(sub: &str, rid: &str, state: &str, body: &str) -> ReadingItem {
    ReadingItem {
        subscription_id: sub.into(),
        remote_id: rid.into(),
        subscription_title: if sub == "lin" { "Lin" } else { "A feed" }.into(),
        origin: format!("https://{sub}.example.com/"),
        kind: Kind::Thread,
        state: state.into(),
        version: 2,
        created: Some("2030-01-02T00:00:00Z".into()),
        updated: Some("2030-01-02T00:00:00Z".into()),
        observed_at: "2030-01-02T00:00:00Z".into(),
        content_md: body.into(),
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

fn sub(id: &str, kind: SubscriptionKind) -> Subscription {
    Subscription {
        id: id.into(),
        kind,
        origin: format!("https://{id}.example.com/"),
        feed_url: format!("https://{id}.example.com/feed.xml"),
        title: id.into(),
        status: "active".into(),
        in_blogroll: true,
        title_follows_source: true,
        poll: Default::default(),
    }
}

/// Your posts, Lin's, a feed's, and two drafts being written (a thread
/// and a fragment).
fn seed(dir: &std::path::Path) -> (LocalId, LocalId) {
    let s = Store::open(&dir.join("blygger.db")).unwrap();
    s.replace_subscriptions(&[
        sub("lin", SubscriptionKind::Blyg),
        sub("feed", SubscriptionKind::Rss),
    ])
    .unwrap();
    s.merge_all(&[
        published(
            POOLS,
            "thread",
            "# Tide pools at dusk\n\nWhat the tide leaves behind is what the pool has to \
             work with until the next one.",
        ),
        published(
            GARDEN,
            "fragment",
            "# Gardens, not streams\n\nA blyg is tended, not fed. Weeding is honest.",
        ),
    ])
    .unwrap();
    let kinds = [
        ("lin".to_string(), SubscriptionKind::Blyg),
        ("feed".to_string(), SubscriptionKind::Rss),
    ]
    .into_iter()
    .collect();
    s.merge_reading(
        &[
            row(
                "lin",
                R1,
                "current",
                "# Rock pool economics\n\nScarcity with a schedule: the shore refills \
                 twice a day, and every rockpool lives on it.",
            ),
            row(
                "feed",
                "01j9fd0000000000000000000d",
                "current",
                "Tide pools at dusk, a feed",
            ),
            row("lin", "01j9wd0000000000000000000e", "tombstone", ""),
        ],
        true,
        &kinds,
    )
    .unwrap();
    let thread = s
        .create_draft(Kind::Thread, "# New thoughts\n\nSomething.", 0)
        .unwrap();
    let fragment = s.create_draft(Kind::Fragment, "Short one.", 0).unwrap();
    (thread, fragment)
}

fn live(dir: &std::path::Path) -> Arc<LiveBackend> {
    Arc::new(
        LiveBackend::open_with(
            dir,
            // Nothing listens here, and nothing is sent.
            "http://127.0.0.1:9",
            "nearby-test",
            SyncOptions {
                start_worker: false,
                ..SyncOptions::default()
            },
        )
        .unwrap(),
    )
}

fn corpus_of(b: &dyn Backend) -> engine::Corpus {
    engine::Corpus {
        own: Arc::new(engine::own_docs(&b.items())),
        held: Arc::new(engine::held_docs(&b.reading(), &b.subscriptions())),
    }
}

fn ask(text: &str, exclude: Option<&LocalId>) -> Ask {
    let at = text.len();
    Ask {
        text: text.into(),
        para: engine::paragraph_at(text, at).unwrap(),
        exclude: exclude.map(|l| own_key(&l.0)),
        linked: engine::linked_ids(text),
        titles_only: false,
    }
}

fn rank(
    b: &dyn Backend,
    c: &engine::Corpus,
    a: &Ask,
    index: Option<(&Index, &dyn Embedder)>,
) -> Vec<engine::Suggestion> {
    let phrases = engine::phrases(&a.text, &a.para);
    let needles: Vec<String> = phrases.iter().map(|p| p.needle.clone()).collect();
    let hits = b.nearby_phrase_hits(&needles);
    let q = index.map(|(_, e)| {
        e.embed(&[&engine::paragraph_plain(&a.text, &a.para)])
            .unwrap()
            .remove(0)
    });
    engine::rank(
        a,
        c,
        index.map(|(i, _)| i).zip(q.as_deref()),
        &hits,
        &phrases,
        0.45,
    )
}

fn ids(v: &[engine::Suggestion]) -> Vec<&str> {
    v.iter().map(|s| s.id.as_str()).collect()
}

// ------------------------------------------------------------ the engine

#[test]
fn with_no_embedder_shared_phrases_find_your_posts_and_held_ones() {
    let dir = tempfile::tempdir().unwrap();
    let (thread, _) = seed(dir.path());
    let b = live(dir.path());
    let c = corpus_of(&*b);
    // Yours and Lin's; never the feed's (a link can't name it), never the
    // withdrawn one, never the draft being written.
    assert_eq!(c.own.len(), 2);
    assert_eq!(c.held.len(), 1);
    assert_eq!(c.held[0].who, "Lin");

    let text = "# New thoughts\n\nI walked out to the tide pools at dusk again";
    let got = rank(&*b, &c, &ask(text, Some(&thread)), None);
    assert_eq!(ids(&got), vec![POOLS], "{got:?}");
    let s = &got[0];
    assert!(s.own && !s.semantic);
    assert_eq!(s.phrase.as_ref().unwrap().needle, "tide pools at dusk");
    assert_eq!(
        &text[s.phrase.as_ref().unwrap().range.clone()],
        "tide pools at dusk"
    );
    // Lin's post shares only "tide"-less words with it: no phrase, no row.
    assert!(!ids(&got).contains(&R1));

    // Already linked: not offered again.
    let linked = format!("# New\n\nSee [[{POOLS}]]. The tide pools at dusk again");
    assert!(rank(&*b, &c, &ask(&linked, Some(&thread)), None).is_empty());
}

#[test]
fn an_embedder_finds_posts_that_say_it_in_other_words() {
    let dir = tempfile::tempdir().unwrap();
    let (thread, _) = seed(dir.path());
    let b = live(dir.path());
    let c = corpus_of(&*b);
    let e = ConceptEmbedder::default();
    let (idx, built) =
        engine::build_index(&*b, &e, &c, None, &Progress::default(), &|| false).unwrap();
    assert_eq!(built.posts, 3);
    assert!(built.embedded >= 6, "{built:?}");

    // No word in common with Lin's post: "sea" and "coast" are its "shore".
    let text = "# New\n\nWalking the coast, I keep thinking about what the sea gives back";
    let got = rank(&*b, &c, &ask(text, Some(&thread)), Some((&idx, &e)));
    let found = ids(&got);
    assert!(found.contains(&R1), "{got:?}");
    assert!(found.contains(&POOLS));
    assert!(!found.contains(&GARDEN), "a garden isn't the sea");
    let lin = got.iter().find(|s| s.id == R1).unwrap();
    assert!(lin.semantic && lin.phrase.is_none());
    assert!(lin.why.contains("Rock pool") || lin.why.contains("shore"));

    // Titles only: the title pieces alone.
    let mut a = ask(text, Some(&thread));
    a.titles_only = true;
    let phrases = engine::phrases(&a.text, &a.para);
    let q = e
        .embed(&[&engine::paragraph_plain(&a.text, &a.para)])
        .unwrap()
        .remove(0);
    let t = engine::rank(&a, &c, Some((&idx, &q)), &[], &phrases, 0.45);
    assert!(t.iter().all(|s| s.semantic));
}

#[test]
fn the_index_is_kept_in_the_store_and_updated_post_by_post() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let e = ConceptEmbedder::default();
    let b = live(dir.path());
    let c = corpus_of(&*b);
    let p = Progress::default();
    let (idx, first) = engine::build_index(&*b, &e, &c, None, &p, &|| false).unwrap();
    // Again from the last index: nothing to embed.
    let (idx2, again) = engine::build_index(&*b, &e, &c, Some(&idx), &p, &|| false).unwrap();
    assert_eq!(again.embedded, 0);
    assert_eq!(idx2.pieces(), idx.pieces());
    drop(b);

    // A new session: the vectors come back from SQLite, not the model.
    let b = live(dir.path());
    let calls = e.calls();
    let (idx3, reopened) = engine::build_index(&*b, &e, &c, None, &p, &|| false).unwrap();
    assert_eq!(reopened.embedded, 0, "{reopened:?}");
    assert_eq!(e.calls(), calls);
    assert_eq!(idx3.pieces(), first.pieces);

    // Lin edits her post: only its changed pieces are embedded again.
    let s = Store::open(&dir.path().join("blygger.db")).unwrap();
    let kinds = [("lin".to_string(), SubscriptionKind::Blyg)]
        .into_iter()
        .collect();
    s.merge_reading(
        &[row(
            "lin",
            R1,
            "current",
            "# Rock pool economics\n\nScarcity with a schedule, rewritten.",
        )],
        true,
        &kinds,
    )
    .unwrap();
    let c2 = corpus_of(&*b);
    e.texts.lock().unwrap().clear();
    let (idx4, edited) = engine::build_index(&*b, &e, &c2, Some(&idx3), &p, &|| false).unwrap();
    assert_eq!(edited.embedded, 1, "the title piece is unchanged");
    assert_eq!(
        *e.texts.lock().unwrap(),
        vec!["Scarcity with a schedule, rewritten.".to_string()]
    );

    // Then withdraws it: its vectors go, in memory and in the store.
    s.merge_reading(&[row("lin", R1, "tombstone", "")], true, &kinds)
        .unwrap();
    assert!(
        b.nearby_vectors(&e.id())
            .iter()
            .all(|v| v.doc != reading_key("lin", R1)),
        "the store's trigger"
    );
    let c3 = corpus_of(&*b);
    let (idx5, _) = engine::build_index(&*b, &e, &c3, Some(&idx4), &p, &|| false).unwrap();
    assert!(!idx5.keys.contains(&reading_key("lin", R1)));
    let posts: std::collections::HashSet<String> = b
        .nearby_vectors(&e.id())
        .into_iter()
        .map(|v| v.doc)
        .collect();
    assert_eq!(posts.len(), 2, "{posts:?}");

    // Another model: the old one's vectors are dropped.
    let other = ConceptEmbedder {
        id: "another-model".into(),
        ..Default::default()
    };
    engine::build_index(&*b, &other, &c3, None, &p, &|| false).unwrap();
    assert!(b.nearby_vectors(&e.id()).is_empty());
}

/// The numbers: building the index and asking it, over a few thousand
/// held posts (invented text), and brute-force cosine at 10k pieces of a
/// 256-dimension model (potion-base-8M's size).
#[test]
fn speed_at_scale() {
    let n = if cfg!(debug_assertions) { 600 } else { 4_000 };
    let dir = tempfile::tempdir().unwrap();
    {
        let s = Store::open(&dir.path().join("blygger.db")).unwrap();
        s.replace_subscriptions(&synth::subscriptions()).unwrap();
        s.merge_reading(&synth::reading(n, 3), true, &synth::kinds())
            .unwrap();
    }
    let b = live(dir.path());
    let e = ConceptEmbedder::default();
    let t = Instant::now();
    let c = corpus_of(&*b);
    let corpus_ms = t.elapsed().as_millis();
    let (idx, built) =
        engine::build_index(&*b, &e, &c, None, &Progress::default(), &|| false).unwrap();
    let t = Instant::now();
    let (_, again) =
        engine::build_index(&*b, &e, &c, Some(&idx), &Progress::default(), &|| false).unwrap();
    let again_ms = t.elapsed().as_millis();
    let text = "# New\n\nThe quiet harbour keeps its lanterns lit while the tide comes in";
    let a = ask(text, None);
    let t = Instant::now();
    let got = rank(&*b, &c, &a, Some((&idx, &e)));
    let query_ms = t.elapsed().as_secs_f64() * 1e3;
    let lex = Instant::now();
    let _ = rank(&*b, &c, &a, None);
    let lexical_ms = lex.elapsed().as_secs_f64() * 1e3;

    // Brute force at 10k × 256.
    let dim = 256;
    let mut big = Index {
        model: "x".into(),
        dim,
        ..Default::default()
    };
    let mut x: u32 = 1;
    for i in 0..10_000u32 {
        big.keys.push(format!("k{i}"));
        big.meta.push((i, 0, String::new()));
        for _ in 0..dim {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            big.vecs.push((x >> 8) as f32 / (1u32 << 24) as f32 - 0.5);
        }
    }
    let q: Vec<f32> = big.vecs[..dim].to_vec();
    let t = Instant::now();
    let top = big.nearest(&q, false, 10);
    let brute_ms = t.elapsed().as_secs_f64() * 1e3;
    assert_eq!(top[0].0, "k0");

    eprintln!(
        "nearby: {} posts → {} pieces; corpus {corpus_ms} ms, first index {} ms ({} embedded), \
         again {again_ms} ms ({} embedded); query {query_ms:.1} ms ({} rows), phrases only \
         {lexical_ms:.1} ms; cosine over 10k×256 {brute_ms:.2} ms; stored {} KB as f16 at 256 dims",
        built.posts,
        built.pieces,
        built.millis,
        built.embedded,
        again.embedded,
        got.len(),
        built.pieces * 256 * 2 / 1024,
    );
    assert_eq!(again.embedded, 0);
    let slack = if cfg!(debug_assertions) { 20.0 } else { 1.0 };
    assert!(brute_ms < 25.0 * slack, "{brute_ms} ms");
    assert!(query_ms < 150.0 * slack, "{query_ms} ms");
}

// ------------------------------------------------------------ the window

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup<'a>(
    dir: &std::path::Path,
    embedder: Option<Arc<ConceptEmbedder>>,
    cx: &'a mut TestAppContext,
) -> (
    Entity<MainView>,
    Arc<LiveBackend>,
    (LocalId, LocalId),
    &'a mut VisualTestContext,
) {
    setup_with(dir, embedder.map(|e| e as Arc<dyn Embedder>), cx)
}

fn setup_with<'a>(
    dir: &std::path::Path,
    embedder: Option<Arc<dyn Embedder>>,
    cx: &'a mut TestAppContext,
) -> (
    Entity<MainView>,
    Arc<LiveBackend>,
    (LocalId, LocalId),
    &'a mut VisualTestContext,
) {
    let drafts = seed(dir);
    let live = live(dir);
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
        cx.set_global(NearbyGlobal {
            embedder,
            models_dir: None,
        });
    });
    let backend: Arc<dyn Backend> = live.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(backend, None, prefs, Instant::now(), window, cx)
    });
    cx.run_until_parked();
    live.set_event_sink(Box::new(|_| {}));
    (view, live, drafts, cx)
}

/// Let the timers fire and the background work land.
fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.executor()
            .advance_clock(DEBOUNCE + Duration::from_millis(50));
        cx.run_until_parked();
    }
}

fn write(view: &Entity<MainView>, cx: &mut VisualTestContext, id: &LocalId, text: &str) {
    view.update_in(cx, |v, window, cx| {
        v.open(id, window, cx);
        v.set_editor_text(text, window, cx);
        let end = text.len();
        v.editor.update(cx, |s, cx| {
            s.set_selected_range(end..end, cx);
            s.focus(window, cx);
        });
        v.after_edit(cx);
        v.nearby_typed(cx);
    });
    settle(cx);
}

fn rail(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |v, _| {
        v.nearby.rows.iter().map(|s| s.id.clone()).collect()
    })
}

fn editor(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, cx| v.editor.read(cx).value().to_string())
}

#[gpui_kit::test]
fn the_rail_fills_after_a_pause_and_alt_enter_links_the_phrase(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, _live, (thread, _), cx) = setup(dir.path(), None, cx);
    let text = "# New thoughts\n\nI walked out to the tide pools at dusk again";
    write(&view, cx, &thread, text);
    assert_eq!(rail(&view, cx), vec![POOLS.to_string()]);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("nearby-rail").is_some());
    assert!(cx.debug_bounds("nearby-row-0").is_some());

    // ⌥⏎: the phrase becomes the link's label, in place.
    cx.simulate_keystrokes(&crate::keymap::keys("alt-enter"));
    cx.run_until_parked();
    assert_eq!(
        editor(&view, cx),
        format!("# New thoughts\n\nI walked out to the [[{POOLS} | tide pools at dusk]] again")
    );
    // Linked now: off the rail, and not offered again.
    assert!(rail(&view, cx).is_empty());
    cx.simulate_keystrokes("space");
    settle(cx);
    assert!(rail(&view, cx).is_empty());
}

#[gpui_kit::test]
fn alt_shift_enter_quotes_in_a_thread_and_explains_in_a_fragment(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let embedder = Arc::new(ConceptEmbedder::default());
    let (view, _live, (thread, fragment), cx) = setup(dir.path(), Some(embedder.clone()), cx);
    let text =
        "# New\n\nWalking the coast, I keep thinking about what the sea gives back\n\nLater.";
    let caret = text.find("\n\nLater").unwrap();
    write(&view, cx, &thread, text);
    view.update_in(cx, |v, window, cx| {
        v.editor
            .update(cx, |s, cx| s.set_selected_range(caret..caret, cx));
        let _ = window;
        v.nearby_typed(cx);
    });
    settle(cx);
    let rows = rail(&view, cx);
    assert!(rows.contains(&R1.to_string()), "{rows:?}");
    assert!(embedder.calls() > 0);
    // ⌥↓ to Lin's post, then ⌥⇧⏎.
    let at = rows.iter().position(|r| r == R1).unwrap();
    for _ in 0..at {
        cx.simulate_keystrokes(&crate::keymap::keys("alt-down"));
    }
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.nearby.sel), at);
    cx.simulate_keystrokes(&crate::keymap::keys("alt-shift-enter"));
    cx.run_until_parked();
    let after = editor(&view, cx);
    assert!(
        after.contains(&format!(
            "gives back\n![[{R1} | Rock pool economics]]\n\nLater."
        )),
        "{after}"
    );

    // A fragment: the toast, nothing written.
    let words = "Sea and shore, the coast at low tide, rockpools everywhere";
    write(&view, cx, &fragment, words);
    assert!(!rail(&view, cx).is_empty());
    cx.simulate_keystrokes(&crate::keymap::keys("alt-shift-enter"));
    cx.run_until_parked();
    assert_eq!(editor(&view, cx), words);
    let toast = view.read_with(cx, |v, _| v.toast.as_ref().map(|t| t.text.to_string()));
    assert_eq!(toast.as_deref(), Some("Quotes go in threads"));
}

#[gpui_kit::test]
fn alt_cmd_u_hides_the_rail(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, _live, (thread, _), cx) = setup(dir.path(), None, cx);
    write(
        &view,
        cx,
        &thread,
        "# A\n\nThe tide pools at dusk once more",
    );
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("nearby-rail").is_some());
    cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-u"));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("nearby-rail").is_none());
    // ⌥⏎ with no rail is left alone (nothing linked).
    cx.simulate_keystrokes(&crate::keymap::keys("alt-enter"));
    cx.run_until_parked();
    assert!(!editor(&view, cx).contains("[["));
    cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-u"));
    settle(cx);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("nearby-rail").is_some());
    assert_eq!(rail(&view, cx), vec![POOLS.to_string()]);
}

// ------------------------------------------------------------ 0.18.1 fixes

/// The middle of a debug-selected element.
fn centre(cx: &mut VisualTestContext, sel: &'static str) -> gpui_kit::Point<gpui_kit::Pixels> {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.debug_bounds(sel)
        .unwrap_or_else(|| panic!("{sel} isn't drawn"))
        .center()
}

/// The owner's report (0.18.0): the keys did nothing. Typed for real
/// (keystrokes through the text field, not `set_editor_text`), then a row
/// clicked with the mouse, as one does: the click used to hand the
/// keyboard to the window (its `track_focus`), so ⌥↓/⌥⏎ no longer reached
/// the rail (`Blygger > Input` needs the editor focused).
#[gpui_kit::test]
fn typing_then_clicking_a_row_keeps_the_keys_working(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, _live, (thread, _), cx) = setup(dir.path(), None, cx);
    write(&view, cx, &thread, "# New thoughts\n\n");
    cx.simulate_input("I walked out to the tide pools at dusk again");
    settle(cx);
    assert_eq!(rail(&view, cx), vec![POOLS.to_string()]);

    let row = centre(cx, "nearby-row-0");
    cx.simulate_click(row, gpui_kit::Modifiers::none());
    cx.run_until_parked();
    let focused = view.update_in(cx, |v, window, cx| {
        v.editor.read(cx).focus_handle(cx).is_focused(window)
    });
    assert!(focused, "a click on a row leaves the editor the keyboard");
    assert_eq!(view.read_with(cx, |v, _| v.nearby.sel), 0);

    cx.simulate_keystrokes(&crate::keymap::keys("alt-down"));
    cx.simulate_keystrokes(&crate::keymap::keys("alt-enter"));
    cx.run_until_parked();
    assert_eq!(
        editor(&view, cx),
        format!("# New thoughts\n\nI walked out to the [[{POOLS} | tide pools at dusk]] again")
    );
}

/// ⌥⇧⏎ after clicking a row: the quote is written too.
#[gpui_kit::test]
fn clicking_a_row_then_alt_shift_enter_quotes_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, _live, (thread, _), cx) = setup(dir.path(), None, cx);
    write(
        &view,
        cx,
        &thread,
        "# New thoughts\n\nI walked out to the tide pools at dusk again",
    );
    assert_eq!(rail(&view, cx), vec![POOLS.to_string()]);
    let row = centre(cx, "nearby-row-0");
    cx.simulate_click(row, gpui_kit::Modifiers::none());
    cx.simulate_keystrokes(&crate::keymap::keys("alt-shift-enter"));
    cx.run_until_parked();
    let after = editor(&view, cx);
    assert!(
        after.contains(&format!("dusk again\n![[{POOLS} | Tide pools at dusk]]")),
        "{after}"
    );
}

/// Resting the mouse on a row shows the post: its title, who, and its
/// first lines; moving off puts the card away.
#[gpui_kit::test]
fn hovering_a_row_shows_the_post(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, _live, (thread, _), cx) = setup(dir.path(), None, cx);
    write(
        &view,
        cx,
        &thread,
        "# New thoughts\n\nI walked out to the tide pools at dusk again",
    );
    let row = centre(cx, "nearby-row-0");
    cx.simulate_mouse_move(row, None, gpui_kit::Modifiers::none());
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |v, _| v.nearby.hover.is_none()),
        "not at once"
    );
    cx.executor()
        .advance_clock(crate::app::hover_card::DELAY + Duration::from_millis(50));
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |v, _| v.nearby.hover.clone()),
        Some(POOLS.to_string())
    );
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let card = cx.debug_bounds("nearby-card").expect("the card is drawn");
    let rail_box = cx.debug_bounds("nearby-rail").unwrap();
    assert!(
        card.right() <= rail_box.left(),
        "left of the rail: {card:?} {rail_box:?}"
    );
    let c = view.read_with(cx, |v, _| v.nearby_card(&v.nearby.rows[0]));
    assert_eq!(c.title, "Tide pools at dusk");
    assert_eq!(c.meta, "You · ≡ thread");
    assert_eq!(
        c.excerpt.as_deref(),
        Some("What the tide leaves behind is what the pool has to work with until the next one.")
    );

    // Off the row: the card goes.
    cx.simulate_mouse_move(
        gpui_kit::point(gpui_kit::px(5.), gpui_kit::px(5.)),
        None,
        gpui_kit::Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.nearby.hover.is_none()));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("nearby-card").is_none());
}

/// An embedder that fails until told otherwise (an endpoint that's down).
struct Flaky {
    up: std::sync::atomic::AtomicBool,
    inner: ConceptEmbedder,
}

impl Embedder for Flaky {
    fn id(&self) -> String {
        self.inner.id()
    }
    fn embed(&self, texts: &[&str]) -> blyg_embed::Result<Vec<Vec<f32>>> {
        if self.up.load(std::sync::atomic::Ordering::Relaxed) {
            self.inner.embed(texts)
        } else {
            Err(blyg_embed::EmbedError::Network("connection refused".into()))
        }
    }
}

/// A failed index build says so and stays said (it used to flip to "By
/// meaning" at the next pause, with no index behind it), and is tried
/// again later rather than never.
#[gpui_kit::test]
fn a_failed_index_says_so_and_is_tried_again(cx: &mut TestAppContext) {
    use super::Status;
    let dir = tempfile::tempdir().unwrap();
    let flaky = Arc::new(Flaky {
        up: false.into(),
        inner: ConceptEmbedder::default(),
    });
    let (view, _live, (thread, _), cx) = setup_with(dir.path(), Some(flaky.clone()), cx);
    let words = view.read_with(cx, |v, _| v.nearby_status_words());
    assert_eq!(words, "Starts when you write in a post");
    let text = "# New\n\nWalking the coast, I keep thinking about what the sea gives back";
    write(&view, cx, &thread, text);
    let status = view.read_with(cx, |v, _| v.nearby.status.clone());
    assert!(matches!(status, Status::Failed(_)), "{status:?}");
    // Another pause: still failed, not "By meaning".
    view.update(cx, |v, cx| v.nearby_typed(cx));
    settle(cx);
    let status = view.read_with(cx, |v, _| v.nearby.status.clone());
    assert!(matches!(status, Status::Failed(_)), "{status:?}");
    assert!(
        view.read_with(cx, |v, _| v.nearby_status_words())
            .starts_with("Shared phrases only: ")
    );

    // The endpoint is back; after the retry delay the next pause indexes.
    flaky.up.store(true, std::sync::atomic::Ordering::Relaxed);
    cx.executor()
        .advance_clock(super::RETRY_INDEX + Duration::from_secs(1));
    view.update(cx, |v, cx| v.nearby_typed(cx));
    settle(cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.nearby.status.clone()),
        Status::Ready
    );
    let rows = rail(&view, cx);
    assert!(rows.contains(&R1.to_string()), "{rows:?}");
}
