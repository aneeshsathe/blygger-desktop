//! --- access-ai --- Client access, AI on the blyg and the publish sheet's
//! AI in a headless window: real keystrokes through GPUI's dispatch, against
//! a zero-latency FakeBackend (which signs in like a browser sign-in: client
//! access needs the studio password first).

use std::sync::Arc;

use blyg_core::access_ai::{AiFunction, ChangeCategory};
use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, LocalId, Status};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::access::{ShowClientAccess, Stage};
use super::note_ai::DraftChangeNote;
use super::server_ai::{AiLoad, ShowServerAi};
use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";
const PUBLIC: &str = "01J9PX1"; // the second row: public v1

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

fn settle(cx: &mut VisualTestContext, done: impl Fn(&mut VisualTestContext) -> bool) {
    crate::test_wait::until(cx, "the access sheet's next state", done);
}

fn keys(cx: &mut VisualTestContext, k: &str) {
    cx.simulate_keystrokes(&crate::keymap::keys(k));
    cx.run_until_parked();
}

/// The client access sheet's stage, as a word, and its row count.
fn access(view: &Entity<MainView>, cx: &mut VisualTestContext) -> (&'static str, usize) {
    view.read_with(cx, |v, _| {
        match v.sheet.get::<crate::app::access::ClientAccessSheet>() {
            Some(s) => (
                match s.stage {
                    Stage::Password => "password",
                    Stage::Loading => "loading",
                    Stage::List => "list",
                    Stage::Confirm(_) => "confirm",
                    Stage::NewToken { .. } => "new",
                    Stage::Token { .. } => "token",
                },
                s.rows.len(),
            ),
            _ => ("closed", 0),
        }
    })
}

fn toast(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, _| {
        v.toast
            .as_ref()
            .map(|t| format!("{} {}", t.text, t.sub.clone().unwrap_or_default()))
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn client_access_asks_for_the_password_then_lists_makes_and_revokes(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    cx.dispatch_action(ShowClientAccess);
    cx.run_until_parked();
    assert_eq!(access(&view, cx).0, "password");
    // A refused password stays on the password, and says so.
    cx.simulate_input("wrong");
    keys(cx, "enter");
    settle(cx, |cx| {
        view.read_with(
            cx,
            |v, _| matches!(v.sheet.get::<crate::app::access::ClientAccessSheet>(), Some(s) if s.error.is_some()),
        )
    });
    assert_eq!(access(&view, cx).0, "password");
    cx.simulate_input("studio password");
    keys(cx, "enter");
    settle(cx, |cx| access(&view, cx).0 == "list");
    assert_eq!(access(&view, cx), ("list", 3));
    // Burrow's own grant is the first; revoking it warns.
    keys(cx, "backspace");
    assert_eq!(access(&view, cx).0, "confirm");
    view.read_with(cx, |v, _| {
        let Some(s) = v.sheet.get::<crate::app::access::ClientAccessSheet>() else {
            panic!()
        };
        let (_, warn) = super::access::confirm_text(&s.rows, Some(0), s.own.as_deref());
        assert!(warn.unwrap().contains("the Burrow you're using"));
    });
    keys(cx, "escape");
    assert_eq!(access(&view, cx), ("list", 3), "esc keeps it");
    // Revoke the second.
    keys(cx, "down");
    keys(cx, "backspace");
    keys(cx, "enter");
    settle(cx, |cx| access(&view, cx) == ("list", 2));
    assert_eq!(access(&view, cx), ("list", 2));
    assert!(toast(&view, cx).starts_with("Revoked “Notebook assistant”"));
    assert!(
        fake.authorizations()
            .unwrap()
            .iter()
            .all(|a| a.id != "gr-mcp")
    );

    // A new token: shown once, copyable, gone from the sheet afterwards.
    keys(cx, "n");
    assert_eq!(access(&view, cx).0, "new");
    cx.simulate_input("Deploy script");
    keys(cx, "enter");
    settle(cx, |cx| access(&view, cx).0 == "token");
    let token = view.read_with(cx, |v, _| {
        match v.sheet.get::<crate::app::access::ClientAccessSheet>() {
            Some(s) => match &s.stage {
                Stage::Token { name, token } => {
                    assert_eq!(name, "Deploy script");
                    token.clone()
                }
                _ => panic!(),
            },
            _ => panic!(),
        }
    });
    assert!(token.starts_with("fake-token-"));
    keys(cx, "c");
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some(token.as_str())
    );
    keys(cx, "enter");
    assert_eq!(access(&view, cx), ("list", 3));
    let made = fake.authorizations().unwrap().pop().unwrap();
    assert_eq!((made.name.as_str(), made.manual), ("Deploy script", true));
    assert_eq!(made.scope, ["owner:read"], "read-only by default");

    // Revoke all, then esc closes.
    keys(cx, "shift-backspace");
    assert_eq!(access(&view, cx).0, "confirm");
    keys(cx, "enter");
    settle(cx, |cx| access(&view, cx) == ("list", 0));
    assert!(fake.authorizations().unwrap().is_empty());
    assert!(toast(&view, cx).contains("signs in again"));
    keys(cx, "escape");
    assert_eq!(access(&view, cx).0, "closed");
    // The session stays for this run: no password the second time.
    cx.dispatch_action(ShowClientAccess);
    settle(cx, |cx| access(&view, cx).0 == "list");
    assert_eq!(access(&view, cx).0, "list");
}

#[gpui_kit::test]
fn server_ai_changes_a_model_and_a_switch_and_saves(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    cx.dispatch_action(ShowServerAi);
    settle(cx, |cx| {
        view.read_with(
            cx,
            |v, _| matches!(v.sheet.get::<crate::app::server_ai::ServerAiSheet>(), Some(s) if matches!(s.load, AiLoad::Ready)),
        )
    });
    // TK: sonnet → the next text model (haiku).
    keys(cx, "right");
    // Change notes' automatic drafting (row 3): off → on.
    keys(cx, "down");
    keys(cx, "down");
    keys(cx, "space");
    // The classifier (row 4): clef-flash → none.
    keys(cx, "down");
    keys(cx, "left");
    view.read_with(cx, |v, _| {
        let Some(s) = v.sheet.get::<crate::app::server_ai::ServerAiSheet>() else {
            panic!()
        };
        assert_eq!(s.settings.model(AiFunction::Classify), Some(""));
        assert_eq!(s.other_for, None);
    });
    keys(cx, "cmd-enter");
    settle(cx, |cx| view.read_with(cx, |v, _| !v.sheet.is_open()));
    let s = fake.server_ai_settings().unwrap();
    assert_eq!(s.model(AiFunction::Tk), Some("claude-haiku-4-5"));
    assert_eq!(s.auto_change_notes, Some(true));
    assert_eq!(s.model(AiFunction::Classify), Some(""));
    assert_eq!(
        s.style_prompt.as_deref(),
        Some("Plain words, short sentences, no exclamation marks."),
        "the house style is kept"
    );
    assert!(toast(&view, cx).starts_with("AI settings saved"));
}

/// The sheet sends only what was changed in it: a setting changed in
/// Studio after the sheet read them survives a save here.
#[gpui_kit::test]
fn server_ai_keeps_a_change_made_elsewhere_since_it_opened(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    cx.dispatch_action(ShowServerAi);
    settle(cx, |cx| {
        view.read_with(
            cx,
            |v, _| matches!(v.sheet.get::<crate::app::server_ai::ServerAiSheet>(), Some(s) if matches!(s.load, AiLoad::Ready)),
        )
    });
    // Studio, meanwhile: a new house style and change-notes model.
    let mut elsewhere = blyg_core::access_ai::ServerAiSettings {
        style_prompt: Some("Short. Warm.".into()),
        ..Default::default()
    };
    *elsewhere.model_mut(AiFunction::Changelog) = Some("claude-sonnet-5-5".into());
    fake.save_server_ai_settings(&elsewhere).unwrap();
    // Here: TK's model only.
    keys(cx, "right");
    keys(cx, "cmd-enter");
    settle(cx, |cx| view.read_with(cx, |v, _| !v.sheet.is_open()));
    let s = fake.server_ai_settings().unwrap();
    assert_eq!(s.model(AiFunction::Tk), Some("claude-haiku-4-5"));
    assert_eq!(
        s.style_prompt.as_deref(),
        Some("Short. Warm."),
        "Studio's style kept"
    );
    assert_eq!(
        s.model(AiFunction::Changelog),
        Some("claude-sonnet-5-5"),
        "Studio's model kept"
    );
}

/// Open the public post and type an edit, so it has a v2 to publish.
fn edit_public(cx: &mut VisualTestContext) {
    keys(cx, "down");
    keys(cx, "enter");
    cx.simulate_input(" Then the rain came.");
    cx.run_until_parked();
}

fn note_text(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, cx| match v.sheet.get() {
        Some(crate::app::PublishSheet { note, .. }) => note.read(cx).value().to_string(),
        _ => String::new(),
    })
}

#[gpui_kit::test]
fn the_publish_sheet_classifies_and_drafts_and_says_so_when_published(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    edit_public(cx);
    keys(cx, "cmd-enter");
    // The classifier's label leads the (empty) note.
    settle(cx, |cx| note_text(&view, cx) == "minor");
    assert_eq!(note_text(&view, cx), "minor");
    // Draft a note: it fills the field, led by the label.
    if crate::keymap::keyless_here("DraftChangeNote") {
        cx.dispatch_action(DraftChangeNote);
    } else {
        keys(cx, "alt-cmd-n");
    }
    settle(cx, |cx| note_text(&view, cx).len() > 6);
    assert_eq!(
        note_text(&view, cx),
        "minor: Tightened the opening and added a closing example."
    );
    // Choosing another label swaps it in the note.
    view.update_in(cx, |v, window, cx| {
        v.note_ai_choose(ChangeCategory::Typo, window, cx)
    });
    assert_eq!(
        note_text(&view, cx),
        "typo: Tightened the opening and added a closing example."
    );
    keys(cx, "enter");
    let id = LocalId(PUBLIC.into());
    settle(cx, |_| fake.item(&id).unwrap().version == 2);
    let item = fake.item(&id).unwrap();
    assert_eq!((item.status, item.version), (Status::Public, 2));
    let x = fake.last_publish_extras().unwrap();
    assert_eq!(x.change_category, Some(Some(ChangeCategory::Typo)));
    assert!(!x.note_generated, "the label was changed by hand");
}

#[gpui_kit::test]
fn an_unedited_draft_publishes_as_generated_and_no_ai_means_a_plain_publish(
    cx: &mut TestAppContext,
) {
    let (view, fake, cx) = setup(cx);
    // Classifier off, automatic notes on: the sheet drafts as it opens.
    fake.set_server_ai(|s| {
        s.model_classify = Some(String::new());
        s.auto_change_notes = Some(true);
    });
    edit_public(cx);
    keys(cx, "cmd-enter");
    settle(cx, |cx| !note_text(&view, cx).is_empty());
    assert_eq!(
        note_text(&view, cx),
        "Tightened the opening and added a closing example."
    );
    keys(cx, "enter");
    let id = LocalId(PUBLIC.into());
    settle(cx, |_| fake.item(&id).unwrap().version == 2);
    let x = fake.last_publish_extras().unwrap();
    assert!(x.note_generated);
    assert_eq!(x.change_category, None, "no classifier, no category");

    // A blyg with no change-notes model and no classifier: nothing shows,
    // and ⌥⌘N says where to set one up.
    fake.set_server_ai(|s| {
        s.model_changelog = Some(String::new());
        s.auto_change_notes = Some(false);
    });
    cx.simulate_input(" Later still.");
    cx.run_until_parked();
    keys(cx, "cmd-enter");
    cx.run_until_parked();
    view.read_with(cx, |v, _| match v.sheet.get() {
        Some(crate::app::PublishSheet { ai, .. }) => assert_eq!(ai.helpers, None),
        _ => panic!("no publish sheet"),
    });
    cx.dispatch_action(DraftChangeNote);
    cx.run_until_parked();
    assert!(toast(&view, cx).contains("change-notes model"));
}

/// User testing, 0.28: studio 0.69's rows (the text helpers' model, the
/// Agentic feed's tab name) were below the fold, and the header spoke of
/// models.json and wrangler. They come first now, under a header in plain
/// words.
#[gpui_kit::test]
fn server_ai_shows_the_new_rows_first_in_plain_words(cx: &mut TestAppContext) {
    use super::server_ai::{MODELS_FROM, MODELS_FROM_LOCAL, ROWS, Row};
    let at = |r: Row| ROWS.iter().position(|x| *x == r).unwrap();
    assert!(at(Row::Model(AiFunction::Helpers)) < at(Row::Model(AiFunction::Changelog)));
    assert!(at(Row::Model(AiFunction::Feed)) < at(Row::Model(AiFunction::Changelog)));
    for t in [MODELS_FROM, MODELS_FROM_LOCAL] {
        for jargon in ["models.json", "wrangler", "Worker"] {
            assert!(!t.contains(jargon), "{t}");
        }
    }
    let (view, fake, cx) = setup(cx);
    fake.set_server_ai(|s| {
        s.model_helpers = Some("claude-haiku-4-5".into());
        s.agentic_title = Some("Agentic".into());
    });
    cx.dispatch_action(ShowServerAi);
    settle(cx, |cx| {
        view.read_with(
            cx,
            |v, _| matches!(v.sheet.get::<crate::app::server_ai::ServerAiSheet>(), Some(s) if matches!(s.load, AiLoad::Ready)),
        )
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let notes = Box::leak(
        format!("server-ai-row-{}", at(Row::Model(AiFunction::Changelog))).into_boxed_str(),
    );
    let notes = cx.debug_bounds(notes).expect("the change notes' row");
    let tab = cx
        .debug_bounds("server-ai-agentic-title")
        .expect("the tab name");
    assert!(tab.top() < notes.top(), "{tab:?} above {notes:?}");
}
