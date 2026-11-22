//! --- dictionary --- The blyg's dictionary in a headless window, on a
//! zero-latency FakeBackend and a fake spell checker: a failed read is
//! tried again (not counted as read), a word the blyg refuses is said, a
//! word learned offline waits quietly, and taking a word out of the blyg's
//! dictionary takes it out of the system's learned list too.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::{DictLoad, DictionarySheet};
use crate::app::MainView;
use crate::composer::{self, SpellService};
use crate::composer::spell::SpellEngine;
use crate::composer::spell::fake::FakeSpell;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup(
    offline: bool,
    cx: &mut TestAppContext,
) -> (
    Entity<MainView>,
    Arc<FakeBackend>,
    Arc<FakeSpell>,
    &mut VisualTestContext,
) {
    let prefs = Prefs::from_config(ConfigStore::in_memory(CONNECTED).config());
    let spell = Arc::new(FakeSpell::new(&[("tidepool", &[]), ("tabels", &["tables"])]));
    let engine = spell.clone();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(CONNECTED),
            Arc::new(MemoryTokenStore::default()),
            None,
            cx,
        );
        composer::init(Some(engine), true, cx);
    });
    let fake = Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache());
    fake.set_offline(offline);
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
    (view, fake, spell, cx)
}

fn accepts(word: &str, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| cx.global::<SpellService>().accepted.accepts(word))
}

fn toast(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, _| {
        v.toast
            .as_ref()
            .map(|t| format!("{} {}", t.text, t.sub.clone().unwrap_or_default()))
            .unwrap_or_default()
    })
}

/// A read that fails (offline at launch) doesn't count as a read: the next
/// refresh reads again, without waiting out `READ_EVERY`.
#[gpui_kit::test]
fn a_failed_read_is_tried_again_at_the_next_refresh(cx: &mut TestAppContext) {
    let (view, fake, _, cx) = setup(true, cx);
    assert!(!accepts("tidepool", cx), "offline at launch: nothing read");
    fake.set_offline(false);
    view.update(cx, |v, cx| v.dictionary_refresh(false, cx));
    crate::test_wait::until(cx, "the dictionary read", |cx| accepts("tidepool", cx));
}

/// A learned word the blyg refuses is said, in a toast; one learned
/// offline waits quietly and is sent later.
#[gpui_kit::test]
fn a_learned_word_the_blyg_refuses_is_said(cx: &mut TestAppContext) {
    let (view, fake, _, cx) = setup(false, cx);
    fake.set_offline(true);
    view.update(cx, |v, cx| v.dictionary_learn("harbourside".into(), cx));
    cx.run_until_parked();
    assert_eq!(toast(&view, cx), "", "offline: kept, nothing to say yet");
    fake.set_offline(false);
    view.update(cx, |v, cx| v.dictionary_learn("12345".into(), cx));
    crate::test_wait::until(cx, "the refusal's toast", |cx| {
        toast(&view, cx).contains("dictionary")
    });
    assert!(toast(&view, cx).contains("12345"), "{}", toast(&view, cx));
}

/// Taking a word out of the blyg's dictionary in the sheet takes it out of
/// the system's learned list too, so it's flagged again.
#[gpui_kit::test]
fn taking_a_word_out_unlearns_it_here_too(cx: &mut TestAppContext) {
    let (view, _, spell, cx) = setup(false, cx);
    spell.learn("tidepool");
    assert!(spell.check("tidepool").is_empty(), "learned");
    view.update_in(cx, |v, window, cx| v.open_dictionary(window, cx));
    crate::test_wait::until(cx, "the dictionary sheet", |cx| {
        view.read_with(cx, |v, _| {
            matches!(v.sheet.get::<DictionarySheet>(), Some(s) if matches!(s.load, DictLoad::Ready(_)))
        })
    });
    let ix = view.read_with(cx, |v, _| {
        let Some(DictionarySheet {
            load: DictLoad::Ready(d),
            ..
        }) = v.sheet.get::<DictionarySheet>()
        else {
            panic!("ready")
        };
        d.owner_words().iter().position(|w| w == "tidepool").unwrap()
    });
    view.update_in(cx, |v, window, cx| v.dictionary_remove(ix, window, cx));
    crate::test_wait::until(cx, "the word taken out", |cx| {
        view.read_with(cx, |v, _| {
            matches!(v.sheet.get::<DictionarySheet>(), Some(s)
                if matches!(&s.load, DictLoad::Ready(d) if !d.owner_words().iter().any(|w| w == "tidepool")))
        })
    });
    assert!(!spell.check("tidepool").is_empty(), "flagged again");
    assert!(!accepts("tidepool", cx));
}
