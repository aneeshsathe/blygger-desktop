//! --- dictionary --- The blyg's dictionary in a headless window, on a
//! zero-latency FakeBackend and a fake spell checker: a failed read is
//! tried again (not counted as read), a word the blyg refuses is said, a
//! word learned offline waits quietly, a read-only sign-in learns on this
//! computer only, and a word Burrow taught this computer (and only such a
//! word) is unlearned once it leaves the blyg's dictionary.

use std::sync::Arc;

use blyg_core::dictionary::Edit;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::{DictLoad, DictionarySheet};
use crate::app::MainView;
use crate::composer::spell::SpellEngine;
use crate::composer::spell::fake::FakeSpell;
use crate::composer::{self, SpellService};
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
    let spell = Arc::new(FakeSpell::new(&[
        ("tidepool", &[]),
        ("tabels", &["tables"]),
        ("harbourside", &[]),
        ("bollardish", &[]),
    ]));
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

/// Open Blyg › Dictionary… and wait for its read.
fn open_sheet(view: &Entity<MainView>, cx: &mut VisualTestContext) {
    view.update_in(cx, |v, window, cx| v.open_dictionary(window, cx));
    crate::test_wait::until(cx, "the dictionary sheet", |cx| {
        view.read_with(cx, |v, _| {
            matches!(v.sheet.get::<DictionarySheet>(), Some(s) if matches!(s.load, DictLoad::Ready(_)))
        })
    });
}

/// The sheet's dictionary as read.
fn sheet_dictionary(
    view: &Entity<MainView>,
    cx: &mut VisualTestContext,
) -> blyg_core::dictionary::Dictionary {
    view.read_with(cx, |v, _| {
        let Some(DictionarySheet {
            load: DictLoad::Ready(d),
            ..
        }) = v.sheet.get::<DictionarySheet>()
        else {
            panic!("ready")
        };
        d.clone()
    })
}

/// Learn Spelling in the editor: the system's list, then the blyg.
fn learn(word: &str, view: &Entity<MainView>, spell: &FakeSpell, cx: &mut VisualTestContext) {
    spell.learn(word);
    view.update(cx, |v, cx| v.dictionary_learn(word.into(), cx));
}

/// Taking a word out in the sheet unlearns it on this computer when
/// Burrow's Learn Spelling added it, and only then: a word this computer
/// learned some other way stays learned.
#[gpui_kit::test]
fn taking_a_word_out_unlearns_only_what_burrow_taught(cx: &mut TestAppContext) {
    let (view, fake, spell, cx) = setup(false, cx);
    learn("harbourside", &view, &spell, cx);
    crate::test_wait::until(cx, "the word recorded as taught", |_| {
        fake.dictionary_taught().unwrap() == ["harbourside"]
    });
    spell.learn("tidepool"); // learned elsewhere; in the blyg's words too
    open_sheet(&view, cx);
    for word in ["harbourside", "tidepool"] {
        let ix = sheet_dictionary(&view, cx)
            .owner_words()
            .iter()
            .position(|w| w == word)
            .unwrap();
        view.update_in(cx, |v, window, cx| v.dictionary_remove(ix, window, cx));
        crate::test_wait::until(cx, "the word taken out", |_| {
            !fake
                .dictionary()
                .unwrap()
                .unwrap()
                .owner_words()
                .iter()
                .any(|w| w == word)
        });
        cx.run_until_parked();
    }
    assert!(!spell.check("harbourside").is_empty(), "Burrow's: flagged again");
    assert!(fake.dictionary_taught().unwrap().is_empty(), "and forgotten");
    assert!(spell.check("tidepool").is_empty(), "never Burrow's to unlearn");
}

/// A word Burrow taught that leaves the blyg's words elsewhere (Studio
/// takes it out) is unlearned here at the next read; a word this computer
/// learned some other way, not in the blyg's words, is left alone.
#[gpui_kit::test]
fn a_taught_word_taken_out_elsewhere_is_unlearned_at_the_next_read(cx: &mut TestAppContext) {
    let (view, fake, spell, cx) = setup(false, cx);
    learn("harbourside", &view, &spell, cx);
    crate::test_wait::until(cx, "the word recorded as taught", |_| {
        !fake.dictionary_taught().unwrap().is_empty()
    });
    spell.learn("bollardish");
    fake.edit_dictionary(&Edit::Remove("harbourside".into()))
        .unwrap();
    view.update(cx, |v, cx| v.dictionary_refresh(true, cx));
    crate::test_wait::until(cx, "unlearned at the read", |_| {
        !spell.check("harbourside").is_empty()
    });
    assert!(fake.dictionary_taught().unwrap().is_empty());
    assert!(spell.check("bollardish").is_empty(), "not Burrow's");
}

/// A sign-in that may read the dictionary but not write it: Learn
/// Spelling keeps the word on this computer, says nothing, sends nothing,
/// and doesn't record it as taught (it never left this computer, so it's
/// never unlearned). The sheet says this sign-in can't add words.
#[gpui_kit::test]
fn a_read_only_sign_in_learns_on_this_computer_only(cx: &mut TestAppContext) {
    let (view, fake, spell, cx) = setup(false, cx);
    fake.set_dictionary_writable(false);
    learn("harbourside", &view, &spell, cx);
    cx.run_until_parked();
    assert_eq!(toast(&view, cx), "", "nothing to say");
    let d = fake.dictionary().unwrap().unwrap();
    assert!(!d.writable);
    assert!(!d.owner_words().iter().any(|w| w == "harbourside"), "not sent");
    assert!(fake.dictionary_taught().unwrap().is_empty());
    assert!(spell.check("harbourside").is_empty(), "learned here");
    open_sheet(&view, cx);
    assert!(!sheet_dictionary(&view, cx).writable);
    assert!(super::READ_ONLY_NOTE.contains("can't add words"));
    // The sheet's edits aren't tried either.
    let before = fake.dictionary().unwrap().unwrap();
    view.update_in(cx, |v, window, cx| v.dictionary_remove(0, window, cx));
    cx.run_until_parked();
    assert_eq!(fake.dictionary().unwrap().unwrap(), before);
}
