//! --- extensions --- The config lines consent and Settings write, and the
//! window driving a real `Host` running the real bundled markdown-notes
//! over a temporary vault of invented notes: consent, the ⇧⌘P palette,
//! the notes library in the drawer (browse, search, open, edit, save, a
//! save refused when the file changed on disk, a new note), copying into a
//! post, "Save selection to notes", and the quote picker's notes chip.
//!
//! The extension runs as a child process, as in the app. The app runs it
//! as `blygger +ext markdown-notes`; a unit test can't name the `blygger`
//! binary, so the child is this test binary, running only
//! [`markdown_notes_child`] (which serves the extension over stdio, the
//! same `blyg_ext_notes::run_stdio` that `+ext` calls). The test harness's
//! own lines on stdout aren't JSON and the host skips them.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, Kind, Status};
use blyg_ext::Capability;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::sheets::Overlay;
use super::{
    Launch, NOTES, Notice, allow_changes, configured_vaults, host_config, vault_add_changes,
    vault_remove_changes, vault_text,
};
use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

// ------------------------------------------------------------ the child

/// A child test's name, as the harness filters by it.
fn child_test(name: &str) -> String {
    let m = module_path!();
    let m = m.split_once("::").map(|(_, rest)| rest).unwrap_or(m);
    format!("{m}::{name}")
}

fn child_name() -> String {
    child_test("markdown_notes_child")
}

/// Not a test: the bundled cross-post extension's process (as
/// [`markdown_notes_child`]), for the macro tests below.
#[test]
#[ignore = "the cross-post child process of the extension tests"]
fn cross_post_child() {
    let me = child_test("cross_post_child");
    if std::env::args().any(|a| a == me) {
        let _ = blyg_ext_crosspost::run_stdio();
    }
}

fn child_args(name: String) -> Vec<String> {
    vec![
        "--exact".into(),
        name,
        "--ignored".into(),
        "--quiet".into(),
        "--test-threads=1".into(),
    ]
}

/// Not a test: the bundled extension's process for the tests below (run
/// with `--exact <this test> --ignored`). Run any other way it does nothing.
#[test]
#[ignore = "the markdown-notes child process of the extension tests"]
fn markdown_notes_child() {
    let me = child_name();
    if std::env::args().any(|a| a == me) {
        let _ = blyg_ext_notes::run_stdio();
    }
}

fn launch(data_dir: PathBuf) -> Launch {
    Launch {
        program: std::env::current_exe().expect("the test binary"),
        args: child_args(child_name()),
        crosspost_args: child_args(child_test("cross_post_child")),
        data_dir,
        timing: blyg_ext::Timing::default(),
    }
}

// ------------------------------------------------------------ config lines

#[test]
fn consent_and_settings_write_the_extension_lines() {
    let store = ConfigStore::in_memory(
        "# mine\nextension = other\nextension-setting = markdown-notes folder=Inbox\n\
         extension-setting = markdown-notes vault=~/Old\n",
    );
    let caps = [Capability::Fs("~/Notes".into()), Capability::Ui];
    let changes = allow_changes(store.config(), NOTES, &caps);
    let mut s = ConfigStore::in_memory(store.text().unwrap());
    s.set(&changes).unwrap();
    let text = s.text().unwrap();
    assert!(text.starts_with("# mine\n"), "comments kept: {text}");
    assert!(text.contains("extension = other\nextension = markdown-notes"));
    assert!(text.contains("extension-allow = markdown-notes fs:~/Notes"));
    assert!(text.contains("extension-allow = markdown-notes ui"));
    // Granting again adds nothing.
    assert!(
        allow_changes(s.config(), NOTES, &caps).is_empty(),
        "already there"
    );

    // A new folder is one more vault; the first stays, as do the others.
    let (changes, keys) = vault_add_changes(s.config(), &["~/Vault".to_string()]);
    assert_eq!(keys, ["vault-vault"]);
    s.set(&changes).unwrap();
    let cfg = s.config();
    assert_eq!(cfg.extension_settings(NOTES)["vault"], "~/Old");
    assert_eq!(cfg.extension_settings(NOTES)["vault-vault"], "~/Vault");
    assert_eq!(cfg.extension_settings(NOTES)["folder"], "Inbox");
    assert_eq!(
        cfg.list("extension-setting")
            .iter()
            .filter(|l| l.contains("vault"))
            .count(),
        2
    );
    assert_eq!(cfg.extensions_enabled(), ["other", "markdown-notes"]);
}

#[test]
fn folders_under_home_are_written_with_a_tilde() {
    if let Some(home) = blyg_ext::capability::home_dir() {
        assert_eq!(
            vault_text(&home.join("Notes").join("Vault")),
            "~/Notes/Vault"
        );
        assert_eq!(vault_text(&home), "~");
    }
    let elsewhere = if cfg!(windows) {
        Path::new("Z:\\vaults\\one")
    } else {
        Path::new("/vaults/one")
    };
    assert_eq!(vault_text(elsewhere), elsewhere.to_string_lossy());
}

#[test]
fn the_host_config_runs_the_bundled_notes_from_the_launch() {
    let store = ConfigStore::in_memory(
        "extension = markdown-notes\nextension-allow = markdown-notes ui\n\
         extension-setting = markdown-notes vault=~/Vault\n",
    );
    let l = launch(PathBuf::from("/data"));
    let hc = host_config(&store, &l);
    assert_eq!(hc.enabled, [NOTES]);
    assert!(hc.grants.allows(NOTES, &Capability::Ui));
    assert_eq!(hc.data_dir, PathBuf::from("/data"));
    let m = &hc.bundled[0].manifest;
    assert_eq!(m.capabilities[0], Capability::Fs("~/Vault".into()));
    match &hc.bundled[0].origin {
        blyg_ext::Origin::Bundled { program, args } => {
            assert_eq!(program, &l.program);
            assert_eq!(args, &l.args);
        }
        o => panic!("{o:?}"),
    }
}

#[test]
fn copy_into_post_quotes_or_copies() {
    use super::library::copy_block;
    assert_eq!(
        copy_block("High water at 6:12\n\nLow at noon.\n", "Tide tables", true),
        "> High water at 6:12\n>\n> Low at noon.\n>\n> — Tide tables"
    );
    assert_eq!(
        copy_block("\nHigh water at 6:12\n", "Tide tables", false),
        "High water at 6:12"
    );
    assert!(
        !copy_block("x", "T", true).contains("![["),
        "never a transclusion"
    );
}

// ------------------------------------------------------------ the window

struct Vault {
    _dir: tempfile::TempDir,
    root: PathBuf,
    data: PathBuf,
}

/// Invented notes: one with frontmatter, one in a folder.
fn vault() -> Vault {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Vault");
    std::fs::create_dir_all(root.join("Garden")).unwrap();
    std::fs::write(
        root.join("Tide tables.md"),
        "---\ntitle: Tide tables\ntags: [sea]\n---\nHigh water at 6:12.\n\nThe harbour bench floods at spring tides.\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Garden").join("Benches.md"),
        "# Benches\n\nA bench is a promise that someone will sit.\n",
    )
    .unwrap();
    let data = dir.path().join("data");
    Vault {
        _dir: dir,
        root,
        data,
    }
}

fn setup<'a>(
    cx: &'a mut TestAppContext,
    v: &Vault,
    config: &str,
) -> (
    Entity<MainView>,
    Arc<FakeBackend>,
    &'a mut VisualTestContext,
) {
    setup_launch(cx, config, launch(v.data.clone()))
}

fn setup_launch<'a>(
    cx: &'a mut TestAppContext,
    config: &str,
    l: Launch,
) -> (
    Entity<MainView>,
    Arc<FakeBackend>,
    &'a mut VisualTestContext,
) {
    let config = format!("blyg-url = https://blyg.example.com\n{config}");
    let prefs = Prefs::from_config(ConfigStore::in_memory(&config).config());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(&config),
            Arc::new(MemoryTokenStore::default()),
            None,
            cx,
        );
        cx.set_global(l);
    });
    let fake = Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache());
    let backend: Arc<dyn Backend> = fake.clone();
    let f2 = fake.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(backend, Some(f2), prefs, Instant::now(), window, cx)
    });
    cx.run_until_parked();
    (view, fake, cx)
}

/// Run the window until `done` (the extension answers from another
/// process, in real time).
fn wait(
    view: &Entity<MainView>,
    cx: &mut VisualTestContext,
    what: &str,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) {
    let t = Instant::now();
    loop {
        cx.run_until_parked();
        view.update_in(cx, |v, window, cx| v.ext_pump(window, cx));
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        assert!(
            t.elapsed() < Duration::from_secs(20),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn config_text(cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        crate::settings::get(cx)
            .store
            .text()
            .unwrap_or_default()
            .to_string()
    })
}

fn root_text(v: &Vault) -> String {
    v.root.to_string_lossy().into_owned()
}

/// Enabled, consented to and running.
fn running<'a>(
    cx: &'a mut TestAppContext,
    v: &Vault,
) -> (
    Entity<MainView>,
    Arc<FakeBackend>,
    &'a mut VisualTestContext,
) {
    let r = root_text(v);
    let config = format!(
        "extension = markdown-notes\nextension-allow = markdown-notes fs:{r}\n\
         extension-allow = markdown-notes ui\nextension-setting = markdown-notes vault={r}\n"
    );
    let (view, fake, cx) = setup(cx, v, &config);
    view.update_in(cx, |v, window, cx| v.ext_start(window, cx));
    wait(&view, cx, "the library", |cx| {
        view.read_with(cx, |v, _| v.ext_library().is_some())
    });
    (view, fake, cx)
}

#[gpui_kit::test]
fn first_start_asks_and_allow_writes_the_grants(cx: &mut TestAppContext) {
    let v = vault();
    let r = root_text(&v);
    let config = format!(
        "# test\nextension = markdown-notes\nextension-setting = markdown-notes vault={r}\n"
    );
    let (view, _, cx) = setup(cx, &v, &config);
    // Nothing runs before the start.
    assert!(view.read_with(cx, |v, _| v.ext_library().is_none()));
    view.update_in(cx, |v, window, cx| v.ext_start(window, cx));
    wait(&view, cx, "the consent sheet", |cx| {
        view.read_with(cx, |v, _| {
            matches!(v.ext.overlay, Some(Overlay::Consent { .. }))
        })
    });
    view.read_with(cx, |v, _| {
        let Some(Overlay::Consent { name, caps, .. }) = &v.ext.overlay else {
            unreachable!()
        };
        assert_eq!(name, NOTES);
        let caps: Vec<&Capability> = caps.iter().map(|c| &c.0).collect();
        assert_eq!(caps, [&Capability::Fs(r.clone()), &Capability::Ui]);
    });
    assert!(cx.debug_bounds("ext-allow").is_some(), "drawn");
    // Untick `ui` (key 2), then allow: only the folder is granted.
    cx.simulate_keystrokes("2");
    cx.simulate_keystrokes("enter");
    wait(&view, cx, "the library", |cx| {
        view.read_with(cx, |v, _| v.ext_library().is_some())
    });
    let text = config_text(cx);
    assert!(text.contains("# test\n"), "comments kept: {text}");
    assert!(
        text.contains(&format!("extension-allow = markdown-notes fs:{r}")),
        "{text}"
    );
    assert!(!text.contains("markdown-notes ui"), "{text}");
    assert!(view.read_with(cx, |v, _| v.ext.overlay.is_none()));
}

#[gpui_kit::test]
fn not_now_leaves_a_notice_that_asks_again(cx: &mut TestAppContext) {
    let v = vault();
    let r = root_text(&v);
    let config =
        format!("extension = markdown-notes\nextension-setting = markdown-notes vault={r}\n");
    let (view, _, cx) = setup(cx, &v, &config);
    view.update_in(cx, |v, window, cx| v.ext_start(window, cx));
    wait(&view, cx, "the consent sheet", |cx| {
        view.read_with(cx, |v, _| v.ext.overlay.is_some())
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        assert!(v.ext.overlay.is_none());
        assert_eq!(v.ext.notices.get(NOTES), Some(&Notice::NeedsPermission));
    });
    assert!(!config_text(cx).contains("extension-allow"));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("ext-notice").is_some(), "in the status bar");
    view.update_in(cx, |v, window, cx| v.ext_ask_again(NOTES, window, cx));
    assert!(view.read_with(cx, |v, _| matches!(
        v.ext.overlay,
        Some(Overlay::Consent { .. })
    )));
}

#[gpui_kit::test]
fn the_palette_lists_commands_libraries_and_manage(cx: &mut TestAppContext) {
    let v = vault();
    let (view, _, cx) = running(cx, &v);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-p"));
    cx.run_until_parked();
    let rows = view.read_with(cx, |v, _| match &v.ext.overlay {
        Some(Overlay::Palette { rows, .. }) => rows.clone(),
        _ => panic!("no palette"),
    });
    let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Save selection to notes",
            "Browse Vault",
            "Manage extensions…"
        ]
    );
    assert_eq!(rows[0].from.as_deref(), Some(NOTES));
    assert!(cx.debug_bounds("ext-pal-2").is_some());
    // The last row: Manage extensions, with the extension running.
    cx.simulate_keystrokes("3");
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| matches!(
        v.ext.overlay,
        Some(Overlay::Manage { .. })
    )));
    let st = view.read_with(cx, |v, _| v.ext.host.as_ref().unwrap().status());
    let notes = st.iter().find(|s| s.name == NOTES).unwrap();
    assert_eq!(notes.state, blyg_ext::ExtState::Running);
    assert!(notes.missing.is_empty());
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |v, _| v.ext.overlay.is_none()));
    // ⇧⌘P twice closes it.
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-p"));
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-p"));
    assert!(view.read_with(cx, |v, _| v.ext.overlay.is_none()));
}

#[gpui_kit::test]
fn the_notes_library_browses_edits_and_never_clobbers(cx: &mut TestAppContext) {
    let v = vault();
    let (view, _, cx) = running(cx, &v);
    view.update_in(cx, |v, window, cx| v.ext_lib_show(window, cx));
    wait(&view, cx, "the top folder", |cx| {
        view.read_with(cx, |v, _| v.ext.lib.entries.len() == 2)
    });
    view.read_with(cx, |v, _| {
        let e = &v.ext.lib.entries;
        assert!(
            e[0].is_dir && e[0].title == "Garden",
            "folders first: {e:?}"
        );
        assert_eq!(e[1].title, "Tide tables");
        assert!(v.notes.open, "in the drawer");
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("notes-tab-library").is_some());
    assert!(cx.debug_bounds("lib-entry-1").is_some());

    // Into the folder and back.
    view.update_in(cx, |v, window, cx| {
        v.ext_lib_list(Some("Garden".into()), window, cx)
    });
    wait(&view, cx, "Garden", |cx| {
        view.read_with(cx, |v, _| {
            v.ext.lib.path.as_deref() == Some("Garden") && v.ext.lib.entries.len() == 1
        })
    });

    // Search as you type (titles and text).
    cx.simulate_input("harbour");
    wait(&view, cx, "a hit", |cx| {
        view.read_with(cx, |v, _| {
            v.ext.lib.hits.as_ref().is_some_and(|h| h.len() == 1)
        })
    });
    let id = view.read_with(cx, |v, _| v.ext.lib.hits.as_ref().unwrap()[0].id.clone());
    assert_eq!(id, "Tide tables.md");

    // Open it: the body without its frontmatter.
    view.update_in(cx, |v, window, cx| v.ext_lib_open(id.clone(), window, cx));
    wait(&view, cx, "the note", |cx| {
        view.read_with(cx, |v, _| v.ext.lib.note.is_some())
    });
    let body = view.read_with(cx, |v, cx| {
        v.ext
            .lib
            .editor
            .as_ref()
            .unwrap()
            .read(cx)
            .value()
            .to_string()
    });
    assert!(body.starts_with("High water at 6:12."), "{body:?}");

    // Edit and save: the frontmatter is kept byte for byte.
    view.update_in(cx, |v, window, cx| {
        v.ext.lib.editor.as_ref().unwrap().update(cx, |s, cx| {
            s.set_value("High water at 6:40 today.\n", window, cx)
        });
        assert!(v.ext.lib.dirty(cx));
        v.ext_lib_save(window, cx);
    });
    let file = v.root.join("Tide tables.md");
    wait(&view, cx, "the save", |cx| {
        view.read_with(cx, |v, cx| !v.ext.lib.dirty(cx))
    });
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "---\ntitle: Tide tables\ntags: [sea]\n---\nHigh water at 6:40 today.\n"
    );

    // Changed on disk meanwhile: the save is refused, nothing is clobbered.
    std::fs::write(
        &file,
        "---\ntitle: Tide tables\n---\nSomeone else's edit.\n",
    )
    .unwrap();
    view.update_in(cx, |v, window, cx| {
        v.ext
            .lib
            .editor
            .as_ref()
            .unwrap()
            .update(cx, |s, cx| s.set_value("My edit.\n", window, cx));
        v.ext_lib_save(window, cx);
    });
    wait(&view, cx, "the refusal", |cx| {
        view.read_with(cx, |v, _| v.ext.lib.note.as_ref().is_some_and(|n| n.stale))
    });
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "---\ntitle: Tide tables\n---\nSomeone else's edit.\n"
    );
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("lib-reload").is_some(), "offers a reload");
    // Keep mine as a new note beside it.
    view.update_in(cx, |v, window, cx| v.ext_lib_save_as_new(window, cx));
    let kept = v.root.join("Tide tables (my edit).md");
    wait(&view, cx, "the kept edit", |_| kept.exists());
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), "My edit.\n");
    // The panel is on the kept copy now; the original shows what's on disk.
    wait(&view, cx, "the copy open", |cx| {
        view.read_with(cx, |v, _| {
            v.ext.lib.note.as_ref().and_then(|n| n.id.as_deref())
                == Some("Tide tables (my edit).md")
        })
    });
    view.update_in(cx, |v, window, cx| {
        v.ext_lib_open("Tide tables.md".into(), window, cx)
    });
    wait(&view, cx, "the reload", |cx| {
        view.read_with(cx, |v, cx| {
            v.ext.lib.editor.as_ref().unwrap().read(cx).value().as_ref() == "Someone else's edit.\n"
        })
    });

    // A new note, named from its first line, in the folder on screen.
    view.update_in(cx, |v, window, cx| {
        v.ext.lib.note = None;
        v.ext.lib.path = Some("Garden".into());
        v.ext_lib_new(window, cx);
        v.ext.lib.editor.as_ref().unwrap().update(cx, |s, cx| {
            s.set_value("Moss on the north side\n\nAlways.\n", window, cx)
        });
        v.ext_lib_save(window, cx);
    });
    let new = v.root.join("Garden").join("Moss on the north side.md");
    wait(&view, cx, "the new note", |_| new.exists());
    assert_eq!(
        std::fs::read_to_string(&new).unwrap(),
        "Moss on the north side\n\nAlways.\n",
        "no frontmatter added"
    );
}

#[gpui_kit::test]
fn copy_into_post_goes_to_the_draft_at_its_caret(cx: &mut TestAppContext) {
    let v = vault();
    let (view, fake, cx) = running(cx, &v);
    let draft = fake
        .create_draft(Kind::Fragment, "My take.\n\nMore.")
        .unwrap();
    view.update_in(cx, |v, window, cx| {
        v.open(&draft, window, cx);
        v.editor.update(cx, |s, cx| s.set_selected_range(8..8, cx));
        v.ext_lib_show(window, cx);
        v.ext_lib_open("Garden/Benches.md".into(), window, cx);
    });
    wait(&view, cx, "the note", |cx| {
        view.read_with(cx, |v, _| v.ext.lib.note.is_some())
    });
    // The selection, quoted with the note's name.
    view.update_in(cx, |v, window, cx| {
        let e = v.ext.lib.editor.clone().unwrap();
        let text = e.read(cx).value().to_string();
        let at = text.find("A bench").unwrap();
        e.update(cx, |s, cx| s.set_selected_range(at..at + 13, cx));
        let _ = window;
    });
    view.update_in(cx, |v, window, cx| v.ext_lib_copy(true, window, cx));
    cx.run_until_parked();
    let item = fake.item(&draft).unwrap();
    assert_eq!(
        item.content_md,
        "My take.\n\n> A bench is a\n>\n> — Benches\n\nMore."
    );
    // Nothing selected, verbatim: the whole note.
    view.update_in(cx, |v, _, cx| {
        let e = v.ext.lib.editor.clone().unwrap();
        e.update(cx, |s, cx| s.set_selected_range(0..0, cx));
    });
    view.update_in(cx, |v, window, cx| v.ext_lib_copy(false, window, cx));
    cx.run_until_parked();
    let item = fake.item(&draft).unwrap();
    assert!(
        item.content_md
            .contains("# Benches\n\nA bench is a promise that someone will sit."),
        "{}",
        item.content_md
    );
    assert!(!item.content_md.contains("![["));

    // No draft open: a new one.
    view.update_in(cx, |v, _, _| v.current = None);
    let before = fake.items().len();
    view.update_in(cx, |v, window, cx| v.ext_lib_copy(true, window, cx));
    cx.run_until_parked();
    assert_eq!(fake.items().len(), before + 1);
    let cur = view.read_with(cx, |v, _| v.current.clone().unwrap());
    assert_eq!(cur.status, Status::Draft);
    assert!(
        cur.content_md.starts_with("> # Benches"),
        "{}",
        cur.content_md
    );
}

#[gpui_kit::test]
fn save_selection_to_notes_makes_a_plain_note(cx: &mut TestAppContext) {
    let v = vault();
    let (view, fake, cx) = running(cx, &v);
    let draft = fake
        .create_draft(Kind::Fragment, "Lanterns on the pier at dusk.")
        .unwrap();
    view.update_in(cx, |v, window, cx| {
        v.open(&draft, window, cx);
        v.editor.update(cx, |s, cx| s.set_selected_range(0..20, cx));
    });
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-p"));
    cx.run_until_parked();
    cx.simulate_keystrokes("1"); // Save selection to notes
    let note = v.root.join("Lanterns on the pier.md");
    wait(&view, cx, "the note", |_| note.exists());
    assert_eq!(
        std::fs::read_to_string(&note).unwrap(),
        "Lanterns on the pier"
    );
    wait(&view, cx, "the toast", |cx| {
        view.read_with(cx, |v, _| {
            v.toast
                .as_ref()
                .is_some_and(|t| t.text.contains("Saved to notes"))
        })
    });
    // Its name is on the toast.
    view.read_with(cx, |v, _| {
        assert_eq!(v.toast.as_ref().unwrap().sub.as_deref(), Some(NOTES));
    });
}

#[gpui_kit::test]
fn the_quote_pickers_notes_chip_quotes_as_text(cx: &mut TestAppContext) {
    let v = vault();
    let (view, fake, cx) = running(cx, &v);
    let thread = fake.create_draft(Kind::Thread, "# On tides\n\n").unwrap();
    view.update_in(cx, |v, window, cx| v.open(&thread, window, cx));
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-k"));
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("pick-src-notes").is_some(), "the chip");
    view.update_in(cx, |v, _, cx| v.ext_picker_toggle(String::new(), cx));
    cx.simulate_input("bench");
    wait(&view, cx, "note hits", |cx| {
        view.read_with(cx, |v, _| !v.ext.lib.picker_hits.is_empty())
    });
    cx.simulate_keystrokes("enter");
    wait(&view, cx, "the quote", |cx| {
        view.read_with(cx, |v, _| {
            v.current
                .as_ref()
                .is_some_and(|c| c.content_md.contains("> — "))
        })
    });
    let text = fake.item(&thread).unwrap().content_md;
    assert!(text.starts_with("# On tides\n\n> "), "{text}");
    assert!(!text.contains("![["), "never a transclusion: {text}");
}

#[gpui_kit::test]
fn settings_point_markdown_notes_at_a_folder_and_ask(cx: &mut TestAppContext) {
    let v = vault();
    let (view, _, cx) = setup(cx, &v, "# test\n");
    view.update_in(cx, |v, window, cx| v.ext_start(window, cx));
    view.update_in(cx, |m, window, cx| {
        m.ext_add_vaults(std::slice::from_ref(&v.root), window, cx)
    });
    let text = config_text(cx);
    let r = vault_text(&v.root);
    assert!(text.contains("extension = markdown-notes"), "{text}");
    assert!(
        text.contains(&format!("extension-setting = markdown-notes vault={r}")),
        "{text}"
    );
    // The consent sheet follows, naming the folder.
    wait(&view, cx, "the consent sheet", |cx| {
        view.read_with(cx, |v, _| {
            matches!(v.ext.overlay, Some(Overlay::Consent { .. }))
        })
    });
    view.read_with(cx, |v, _| {
        let Some(Overlay::Consent { caps, .. }) = &v.ext.overlay else {
            unreachable!()
        };
        assert_eq!(caps[0].0, Capability::Fs(r.clone()));
    });
}

// ------------------------------------------------------------ browser macros

mod macros {
    //! The bundled cross-post extension, running for real (this test
    //! binary as its process): its ⇧⌘P row, `macro.prepare`, the Preview
    //! sheet; and the window's side of `burrow/browser.page` and
    //! `burrow/browser.open`. The browser pane is a stub surface; nothing
    //! loads a real website (the Preview sheet comes before any load, and
    //! the run is cancelled there).

    use std::cell::RefCell;
    use std::rc::Rc;

    use blyg_ext::protocol::{PageCapture, Screen};
    use blyg_ext::{ExtEvent, ExtState, UiReply};
    use gpui_kit::{Bounds, Pixels};

    use super::*;
    use crate::app::Sheet;
    use crate::app::browser::surface::{BrowserSurface, FactoryGlobal, PageState};

    const XP: &str = blyg_ext_crosspost::NAME;
    const ROW: &str = "Cross-post to Substack Notes… ↗ Substack Notes";
    const POST: &str = "Tide pools are small oceans that forget, twice a day, that they \
                        belong to a larger one.\n\nThe second paragraph stays home.\n";
    const TEMPLATE: &str = "{{excerpt}} (via {{permalink}})";

    /// Enabled, with `items.read` and `ui`, and Substack when `site`.
    fn config(site: bool) -> String {
        let mut c = format!(
            "extension = {XP}\nextension-allow = {XP} items.read\nextension-allow = {XP} ui\n\
             extension-setting = {XP} template={TEMPLATE}\nextension-setting = {XP} max-chars=110\n"
        );
        if site {
            c.push_str(&format!(
                "extension-allow = {XP} browser.automate:https://substack.com\n"
            ));
        }
        c
    }

    /// A browser pane that loads nothing: its address is what it was told,
    /// and the capture script answers `capture`.
    struct Stub {
        url: Rc<RefCell<String>>,
        capture: String,
    }

    impl BrowserSurface for Stub {
        fn set_frame(&mut self, _: Bounds<Pixels>) {}
        fn set_visible(&mut self, _: bool) {}
        fn load_url(&mut self, url: &str) {
            *self.url.borrow_mut() = url.to_string();
        }
        fn back(&mut self) {}
        fn forward(&mut self) {}
        fn reload(&mut self) {}
        fn stop(&mut self) {}
        fn state(&self) -> PageState {
            PageState {
                url: self.url.borrow().clone(),
                title: "Harbour log".into(),
                ..PageState::default()
            }
        }
        fn refresh_blocking(&mut self) {}
        fn focus_parent(&mut self) {}
        fn eval_json(&mut self, _js: &str, reply: async_channel::Sender<String>) {
            let _ = reply.try_send(self.capture.clone());
        }
    }

    fn stub_pane(cx: &mut VisualTestContext) -> Rc<RefCell<String>> {
        let url = Rc::new(RefCell::new(String::new()));
        let u = url.clone();
        cx.update(|_, cx| {
            cx.set_global(FactoryGlobal(Rc::new(move |_, _, _, _| {
                Ok(Box::new(Stub {
                    url: u.clone(),
                    capture: r#"{"url": "https://news.example.com/story", "title": "Harbour log",
                        "canonical": "", "selection": "low water",
                        "html": "<article><p>The tide went out.</p></article>"}"#
                        .into(),
                }) as Box<dyn BrowserSurface>)
            })));
        });
        url
    }

    /// Wait for cross-post to run.
    fn started(view: &Entity<MainView>, cx: &mut VisualTestContext) {
        view.update_in(cx, |v, window, cx| v.ext_start(window, cx));
        wait(view, cx, "cross-post running", |cx| {
            view.read_with(cx, |v, _| {
                v.ext.host.as_ref().is_some_and(|h| {
                    h.status()
                        .iter()
                        .any(|s| s.name == XP && s.state == ExtState::Running)
                })
            })
        });
    }

    /// A published post (the fake publishes at once) and a draft, the post open.
    fn published(
        view: &Entity<MainView>,
        fake: &FakeBackend,
        cx: &mut VisualTestContext,
    ) -> (blyg_core::LocalId, blyg_core::LocalId) {
        let post = fake.create_draft(Kind::Fragment, POST).unwrap();
        let out = fake.publish(&post, None).unwrap();
        assert!(!out.permalink.is_empty());
        let draft = fake.create_draft(Kind::Fragment, "Not yet.").unwrap();
        view.update_in(cx, |v, window, cx| v.open(&post, window, cx));
        cx.run_until_parked();
        (post, draft)
    }

    fn labels(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Vec<String> {
        view.update_in(cx, |v, window, cx| {
            v.ext_palette_rows(window, cx)
                .into_iter()
                .map(|r| r.label)
                .collect()
        })
    }

    fn toast(view: &Entity<MainView>, cx: &mut VisualTestContext) -> (String, String) {
        view.read_with(cx, |v, _| {
            v.toast
                .as_ref()
                .map(|t| {
                    (
                        t.text.to_string(),
                        t.sub.as_ref().map(|s| s.to_string()).unwrap_or_default(),
                    )
                })
                .unwrap_or_default()
        })
    }

    /// ⇧⌘P, then the macro's row.
    fn choose_macro(view: &Entity<MainView>, cx: &mut VisualTestContext) {
        view.update_in(cx, |v, window, cx| v.ext_toggle_palette(window, cx));
        let i = view.read_with(cx, |v, _| {
            let Some(Overlay::Palette { rows, .. }) = &v.ext.overlay else {
                panic!("no palette")
            };
            rows.iter().position(|r| r.label == ROW).expect("the row")
        });
        view.update_in(cx, |v, window, cx| v.ext_run_row(i, window, cx));
    }

    #[gpui_kit::test]
    fn the_palette_offers_the_macro_for_a_published_post_with_the_grant(cx: &mut TestAppContext) {
        let v = vault();
        let (view, fake, cx) = setup(cx, &v, &config(true));
        started(&view, cx);
        let (_, draft) = published(&view, &fake, cx);
        let rows = labels(&view, cx);
        assert!(rows.contains(&ROW.to_string()), "{rows:?}");
        // Its row says where it comes from.
        view.update_in(cx, |v, window, cx| {
            let r = v
                .ext_palette_rows(window, cx)
                .into_iter()
                .find(|r| r.label == ROW)
                .unwrap();
            assert_eq!(r.from.as_deref(), Some(XP));
            assert_eq!(v.ext_macro_rows(Screen::Reading).len(), 0, "posts only");
        });
        // A draft isn't published: no row.
        view.update_in(cx, |v, window, cx| v.open(&draft, window, cx));
        cx.run_until_parked();
        assert!(!labels(&view, cx).contains(&ROW.to_string()));
    }

    /// A granted site has an "Open <site>" row: its home in the pane, to
    /// sign in by hand. Without the grant, no row.
    #[gpui_kit::test]
    fn a_granted_site_opens_from_the_palette(cx: &mut TestAppContext) {
        const OPEN: &str = "Open Substack Notes";
        let v = vault();
        let (view, _fake, cx) = setup(cx, &v, &config(true));
        let url = stub_pane(cx);
        started(&view, cx);
        let rows = labels(&view, cx);
        assert_eq!(rows.iter().filter(|r| *r == OPEN).count(), 1, "{rows:?}");
        view.update_in(cx, |v, window, cx| v.ext_toggle_palette(window, cx));
        let i = view.read_with(cx, |v, _| {
            let Some(Overlay::Palette { rows, .. }) = &v.ext.overlay else {
                panic!("no palette")
            };
            rows.iter().position(|r| r.label == OPEN).expect("the row")
        });
        view.update_in(cx, |v, window, cx| v.ext_run_row(i, window, cx));
        cx.run_until_parked();
        assert_eq!(*url.borrow(), "https://substack.com/home");
        view.read_with(cx, |v, _| {
            assert!(v.browser.open);
            assert_eq!(v.browser.mode, crate::app::browser::OpenMode::Full);
        });
    }

    #[gpui_kit::test]
    fn without_the_site_grant_there_is_no_open_row(cx: &mut TestAppContext) {
        let v = vault();
        let (view, _fake, cx) = setup(cx, &v, &config(false));
        started(&view, cx);
        assert!(
            !labels(&view, cx)
                .iter()
                .any(|r| r.starts_with("Open Substack"))
        );
    }

    #[gpui_kit::test]
    fn without_the_site_grant_there_is_no_row(cx: &mut TestAppContext) {
        let v = vault();
        let (view, fake, cx) = setup(cx, &v, &config(false));
        started(&view, cx);
        published(&view, &fake, cx);
        assert!(!labels(&view, cx).contains(&ROW.to_string()));
        view.read_with(cx, |v, _| assert!(!v.macro_cross_post_hint()));
    }

    #[gpui_kit::test]
    fn choosing_it_prepares_the_text_and_opens_the_preview_sheet(cx: &mut TestAppContext) {
        let v = vault();
        let (view, fake, cx) = setup(cx, &v, &config(true));
        let pane = stub_pane(cx);
        started(&view, cx);
        let (post, _) = published(&view, &fake, cx);
        view.read_with(cx, |v, _| assert!(v.macro_cross_post_hint()));
        choose_macro(&view, cx);
        wait(&view, cx, "the Preview sheet", |cx| {
            view.read_with(cx, |v, _| matches!(v.sheet, Some(Sheet::MacroPreview(_))))
        });
        // The extension's own template and limit shaped it (macro.prepare
        // ran), from the post as published.
        let item = fake.item(&post).unwrap();
        let link = item.permalink.clone().unwrap();
        let want = blyg_ext_crosspost::text::prepare(
            Some(&item.content_md),
            &item.title(),
            &link,
            TEMPLATE,
            110,
            "",
        );
        view.read_with(cx, |v, cx| {
            let Some(Sheet::MacroPreview(s)) = &v.sheet else {
                unreachable!()
            };
            assert_eq!(s.info.payload, want.text);
            assert!(
                s.info.payload.ends_with(&format!(" (via {link})")),
                "{}",
                s.info.payload
            );
            assert!(
                s.info.payload.starts_with("Tide pools are"),
                "{}",
                s.info.payload
            );
            assert_eq!(s.info.note, want.note);
            assert!(s.info.note.is_some(), "cut to fit");
            assert_eq!(s.info.site, "Substack Notes");
            assert_eq!(s.info.tested, "2026-10-09");
            assert_eq!(s.editor.read(cx).value().to_string(), want.text);
            assert!(v.browser.automation.running());
        });
        // Nothing loaded before Continue.
        assert!(pane.borrow().is_empty());

        // While the run holds the pane, an extension can't read or move it.
        let (reply, page) = UiReply::<Option<PageCapture>>::pair();
        view.update_in(cx, |v, window, cx| {
            v.ext_event(
                ExtEvent::BrowserPage {
                    name: XP.into(),
                    reply,
                },
                window,
                cx,
            )
        });
        cx.run_until_parked();
        assert_eq!(page.try_recv(), Ok(None));
        let (reply, opened) = UiReply::<Result<String, String>>::pair();
        view.update_in(cx, |v, window, cx| {
            v.ext_event(
                ExtEvent::BrowserOpen {
                    name: XP.into(),
                    url: "https://substack.com/notes".into(),
                    reply,
                },
                window,
                cx,
            )
        });
        assert!(matches!(opened.try_recv(), Ok(Err(_))));

        // Cancel: nothing ran.
        view.update_in(cx, |v, window, cx| {
            v.macro_preview_answer(false, window, cx)
        });
        wait(&view, cx, "the run to end", |cx| {
            view.read_with(cx, |v, _| !v.browser.automation.running())
        });
        assert!(pane.borrow().is_empty());
        assert_eq!(toast(&view, cx).0, "Cancelled. Nothing was posted.");
    }

    #[gpui_kit::test]
    fn an_unpublished_post_gets_a_clear_message(cx: &mut TestAppContext) {
        let v = vault();
        let (view, fake, cx) = setup(cx, &v, &config(true));
        started(&view, cx);
        let (_, draft) = published(&view, &fake, cx);
        // The row was offered; then the post went back to being a draft.
        view.update_in(cx, |v, window, cx| v.open(&draft, window, cx));
        cx.run_until_parked();
        view.update_in(cx, |v, window, cx| {
            v.ext_run_macro(XP.into(), blyg_ext_crosspost::MACRO.into(), window, cx)
        });
        cx.run_until_parked();
        let (text, sub) = toast(&view, cx);
        assert_eq!(text, "Publish it first to cross-post it to Substack Notes");
        assert!(sub.contains("published post"), "{sub}");
        view.read_with(cx, |v, _| {
            assert!(v.sheet.is_none() && !v.browser.automation.running())
        });
        // A macro that isn't granted (or doesn't exist) runs nothing.
        view.update_in(cx, |v, window, cx| {
            v.ext_run_macro(XP.into(), "nope".into(), window, cx)
        });
        assert_eq!(
            toast(&view, cx).0,
            format!("{XP} can't post there any more")
        );
    }

    #[gpui_kit::test]
    fn a_prepare_error_is_a_toast_and_nothing_runs(cx: &mut TestAppContext) {
        let v = vault();
        let mut l = launch(v.data.clone());
        l.crosspost_args = child_args(child_test("macros::failing_cross_post_child"));
        let (view, fake, cx) = setup_launch(cx, &config(true), l);
        let pane = stub_pane(cx);
        started(&view, cx);
        published(&view, &fake, cx);
        choose_macro(&view, cx);
        wait(&view, cx, "the toast", |cx| {
            toast(&view, cx).0 == format!("{XP}: the template is broken")
        });
        assert_eq!(toast(&view, cx).1, "Nothing was posted to Substack Notes");
        view.read_with(cx, |v, _| {
            assert!(v.sheet.is_none(), "no Preview sheet");
            assert!(!v.browser.automation.running());
            assert!(!v.browser.open);
        });
        assert!(pane.borrow().is_empty());
    }

    /// Not a test: a cross-post whose `macro.prepare` fails, as the bundled
    /// extension's process for the test above.
    #[test]
    #[ignore = "a failing cross-post child process of the macro tests"]
    fn failing_cross_post_child() {
        use blyg_ext::ext::{Extension, HostClient, serve};
        use blyg_ext::protocol::*;
        struct Failing;
        impl Extension for Failing {
            fn initialize(
                &mut self,
                _: &HostClient,
                _: InitializeParams,
            ) -> Result<InitializeResult, RpcError> {
                Ok(InitializeResult {
                    protocol_version: blyg_ext::PROTOCOL_VERSION,
                    name: XP.into(),
                    version: "0.0.1".into(),
                    commands: vec![],
                    sources: vec![],
                    libraries: vec![],
                })
            }
            fn request(
                &mut self,
                _: &HostClient,
                method: &str,
                _: serde_json::Value,
            ) -> Result<serde_json::Value, RpcError> {
                Err(match method {
                    methods::MACRO_PREPARE => {
                        RpcError::new(codes::INTERNAL_ERROR, "the template is broken")
                    }
                    m => RpcError::method_not_found(m),
                })
            }
            fn notification(&mut self, _: &HostClient, _: &str, _: serde_json::Value) {}
        }
        let me = child_test("macros::failing_cross_post_child");
        if std::env::args().any(|a| a == me) {
            serve(std::io::stdin(), std::io::stdout(), &mut Failing);
        }
    }

    #[gpui_kit::test]
    fn browser_page_and_open_are_answered_from_the_pane(cx: &mut TestAppContext) {
        let v = vault();
        let (view, _, cx) = setup(cx, &v, "");
        let pane = stub_pane(cx);
        let page = |view: &Entity<MainView>, cx: &mut VisualTestContext| {
            let (reply, rx) = UiReply::<Option<PageCapture>>::pair();
            view.update_in(cx, |v, window, cx| {
                v.ext_event(
                    ExtEvent::BrowserPage {
                        name: "x".into(),
                        reply,
                    },
                    window,
                    cx,
                )
            });
            cx.run_until_parked();
            rx.try_recv().expect("answered")
        };
        // The pane is closed: no page.
        assert_eq!(page(&view, cx), None);
        // browser.open (its origin was checked by the host) opens the pane.
        let (reply, rx) = UiReply::<Result<String, String>>::pair();
        view.update_in(cx, |v, window, cx| {
            v.ext_event(
                ExtEvent::BrowserOpen {
                    name: "x".into(),
                    url: "https://news.example.com/story".into(),
                    reply,
                },
                window,
                cx,
            )
        });
        cx.run_until_parked();
        assert_eq!(
            rx.try_recv(),
            Ok(Ok("https://news.example.com/story".into()))
        );
        assert_eq!(*pane.borrow(), "https://news.example.com/story");
        view.read_with(cx, |v, _| {
            assert!(v.browser.open);
            assert_eq!(v.browser.mode, crate::app::browser::OpenMode::Full);
        });
        // Now the page is there: address, title, selection, the article.
        let got = page(&view, cx).expect("a page");
        assert_eq!(got.url, "https://news.example.com/story");
        assert_eq!(got.title, "Harbour log");
        assert_eq!(got.selection, "low water");
        assert_eq!(got.markdown.trim(), "The tide went out.");
    }
}

// ---- multiple vaults

#[test]
fn vaults_are_added_and_removed_as_config_lines() {
    let mut s = ConfigStore::in_memory(
        "# mine\nextension-setting = markdown-notes folder=Inbox\n\
         extension-allow = markdown-notes ui\n",
    );
    // The first folder is `vault=`, so a config that names one still reads.
    let (changes, keys) = vault_add_changes(s.config(), &["~/Notes".to_string()]);
    assert_eq!(keys, ["vault"]);
    s.set(&changes).unwrap();
    // Several at once; one already there is skipped.
    let (changes, keys) = vault_add_changes(
        s.config(),
        &[
            "~/Work/Vault".to_string(),
            "~/Notes/".to_string(),
            "~/Work/Notes".to_string(),
        ],
    );
    assert_eq!(keys, ["vault-vault", "vault-work-notes"]);
    s.set(&changes).unwrap();
    let text = s.text().unwrap().to_string();
    assert!(text.starts_with("# mine\n"), "comments kept: {text}");
    assert!(text.contains("extension = markdown-notes"), "{text}");
    assert!(text.contains("extension-setting = markdown-notes vault=~/Notes"));
    assert!(text.contains("extension-setting = markdown-notes vault-vault=~/Work/Vault"));
    let titles: Vec<String> = configured_vaults(s.config())
        .into_iter()
        .map(|v| v.title)
        .collect();
    assert_eq!(titles, ["Notes", "Vault", "work notes"]);
    assert!(
        vault_add_changes(s.config(), &["~/Notes".to_string()])
            .1
            .is_empty()
    );

    // Allowed, then removed: the line and its grant go, nothing else.
    s.set(&allow_changes(
        s.config(),
        NOTES,
        &[Capability::Fs("~/Work/Vault".into())],
    ))
    .unwrap();
    s.set(&vault_remove_changes(s.config(), "vault-vault"))
        .unwrap();
    let text = s.text().unwrap().to_string();
    assert!(!text.contains("~/Work/Vault"), "{text}");
    assert!(
        text.contains("extension-allow = markdown-notes ui"),
        "{text}"
    );
    assert!(text.contains("folder=Inbox"), "{text}");
    // Removing the first leaves the others; a new one becomes the first.
    s.set(&vault_remove_changes(s.config(), "vault")).unwrap();
    let keys: Vec<String> = configured_vaults(s.config())
        .into_iter()
        .map(|v| v.key)
        .collect();
    assert_eq!(keys, ["vault-work-notes"]);
    let (_, keys) = vault_add_changes(s.config(), &["~/Garden".to_string()]);
    assert_eq!(keys, ["vault"]);
    assert!(vault_remove_changes(s.config(), "vault.nope").is_empty());
}

#[test]
fn the_host_config_asks_for_each_vault_and_none_by_default() {
    let l = launch(PathBuf::from("/data"));
    let none = ConfigStore::in_memory("extension = markdown-notes\n");
    let m = &host_config(&none, &l).bundled[0].manifest;
    assert_eq!(m.capabilities, [Capability::Ui], "no silent ~/Notes");
    assert!(m.libraries.is_empty());
    let two = ConfigStore::in_memory(
        "extension = markdown-notes\nextension-setting = markdown-notes vault=~/Notes\n\
         extension-setting = markdown-notes vault-work=~/Work/Vault\n",
    );
    let m = &host_config(&two, &l).bundled[0].manifest;
    assert_eq!(
        m.capabilities,
        [
            Capability::Fs("~/Notes".into()),
            Capability::Fs("~/Work/Vault".into()),
            Capability::Ui
        ]
    );
    let ids: Vec<&str> = m.libraries.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids, ["notes", "notes.work"]);
}

/// A second vault beside [`vault`]'s, under the same temporary folder.
fn work_vault(v: &Vault) -> PathBuf {
    let root = v.root.parent().unwrap().join("Work").join("Vault");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Standup.md"),
        "# Standup\n\nThe bench repair ships Friday.\n",
    )
    .unwrap();
    root
}

/// Two vaults (the first `vault=`, the second `vault-work=`), both
/// allowed unless `allow_work` is false.
fn two_running<'a>(
    cx: &'a mut TestAppContext,
    v: &Vault,
    work: &Path,
    allow_work: bool,
) -> (
    Entity<MainView>,
    Arc<FakeBackend>,
    &'a mut VisualTestContext,
) {
    let r = root_text(v);
    let w = work.to_string_lossy().into_owned();
    let mut config = format!(
        "extension = markdown-notes\nextension-allow = markdown-notes fs:{r}\n\
         extension-allow = markdown-notes ui\nextension-setting = markdown-notes vault={r}\n\
         extension-setting = markdown-notes vault-work={w}\n"
    );
    if allow_work {
        config.push_str(&format!("extension-allow = markdown-notes fs:{w}\n"));
    }
    let (view, fake, cx) = setup(cx, v, &config);
    view.update_in(cx, |v, window, cx| v.ext_start(window, cx));
    wait(&view, cx, "both libraries", |cx| {
        view.read_with(cx, |v, _| v.ext_libraries().len() == 2)
    });
    (view, fake, cx)
}

#[gpui_kit::test]
fn the_drawer_switches_vaults_and_remembers(cx: &mut TestAppContext) {
    let v = vault();
    let work = work_vault(&v);
    let (view, fake, cx) = two_running(cx, &v, &work, true);
    view.update_in(cx, |v, window, cx| v.ext_lib_show(window, cx));
    wait(&view, cx, "the first vault", |cx| {
        view.read_with(cx, |v, _| v.ext.lib.entries.len() == 2)
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("lib-vault-0").is_some(), "the switcher");
    assert!(cx.debug_bounds("lib-vault-1").is_some());
    assert!(cx.debug_bounds("lib-vault-add").is_some());
    assert!(cx.debug_bounds("lib-vault-remove").is_some());

    // The work vault: its notes, its search.
    view.update_in(cx, |v, window, cx| {
        v.ext_lib_choose(Some("notes.work".into()), window, cx)
    });
    wait(&view, cx, "the work vault", |cx| {
        view.read_with(cx, |v, _| {
            v.ext.lib.entries.len() == 1 && v.ext.lib.entries[0].title == "Standup"
        })
    });
    cx.simulate_input("bench");
    wait(&view, cx, "a work hit", |cx| {
        view.read_with(cx, |v, _| {
            v.ext.lib.hits.as_ref().is_some_and(|h| h.len() == 1)
        })
    });
    let id = view.read_with(cx, |v, _| v.ext.lib.hits.as_ref().unwrap()[0].id.clone());
    assert_eq!(id, "Standup.md", "only the chosen vault is searched");

    // Quote into post from it.
    let draft = fake.create_draft(Kind::Fragment, "My take.").unwrap();
    view.update_in(cx, |v, window, cx| {
        v.open(&draft, window, cx);
        v.ext_lib_open(id.clone(), window, cx);
    });
    wait(&view, cx, "the note", |cx| {
        view.read_with(cx, |v, _| v.ext.lib.note.is_some())
    });
    view.update_in(cx, |v, window, cx| v.ext_lib_copy(true, window, cx));
    cx.run_until_parked();
    let text = fake.item(&draft).unwrap().content_md;
    assert!(text.contains("> The bench repair ships Friday."), "{text}");
    assert!(text.contains("> — Standup"), "{text}");

    // A new note lands in the chosen vault.
    view.update_in(cx, |v, window, cx| {
        v.ext.lib.note = None;
        v.ext_lib_new(window, cx);
        v.ext
            .lib
            .editor
            .as_ref()
            .unwrap()
            .update(cx, |s, cx| s.set_value("Retro\n\nWent well.\n", window, cx));
        v.ext_lib_save(window, cx);
    });
    let new = work.join("Retro.md");
    wait(&view, cx, "the new note", |_| new.exists());
    assert!(!v.root.join("Retro.md").exists());

    // Remembered in state.json.
    let st = blyg_core::state::AppState::load(&v.data);
    assert_eq!(st.notes_vault.as_deref(), Some("notes.work"));

    // Remove from Burrow: the config lines go, the folder stays.
    view.update_in(cx, |v, window, cx| {
        v.ext_remove_vault("vault-work", window, cx)
    });
    let text = config_text(cx);
    assert!(!text.contains("vault-work"), "{text}");
    assert!(
        !text.contains(&format!("fs:{}", work.to_string_lossy())),
        "{text}"
    );
    assert!(work.join("Standup.md").exists() && new.exists());
    assert_eq!(view.read_with(cx, |v, _| v.ext.lib.vault.clone()), None);
    wait(&view, cx, "one library", |cx| {
        view.read_with(cx, |v, _| v.ext_libraries().len() == 1)
    });
}

#[gpui_kit::test]
fn the_quote_picker_searches_every_vault(cx: &mut TestAppContext) {
    let v = vault();
    let work = work_vault(&v);
    let (view, _, cx) = two_running(cx, &v, &work, true);
    view.update_in(cx, |v, _, cx| {
        v.ext.lib.picker = true;
        v.ext_picker_search("bench".into(), cx);
    });
    wait(&view, cx, "hits from both", |cx| {
        view.read_with(cx, |v, _| v.ext.lib.picker_hits.len() == 3)
    });
    view.read_with(cx, |v, _| {
        let from: Vec<(&str, &str)> = v
            .ext
            .lib
            .picker_hits
            .iter()
            .map(|h| (h.lib.library.id.as_str(), h.hit.id.as_str()))
            .collect();
        assert!(from.contains(&("notes.work", "Standup.md")), "{from:?}");
        assert!(from.contains(&("notes", "Garden/Benches.md")), "{from:?}");
        assert!(from.contains(&("notes", "Tide tables.md")), "{from:?}");
    });
}

#[gpui_kit::test]
fn a_vault_without_permission_says_so_and_the_other_works(cx: &mut TestAppContext) {
    let v = vault();
    let work = work_vault(&v);
    let (view, _, cx) = two_running(cx, &v, &work, false);
    view.update_in(cx, |v, window, cx| {
        // The consent sheet came up for the missing folder: not now.
        v.ext.overlay = None;
        v.ext_lib_show(window, cx);
        v.ext_lib_choose(Some("notes.work".into()), window, cx);
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("lib-vault-allow").is_some(),
        "needs permission"
    );
    view.update_in(cx, |v, window, cx| {
        v.ext_lib_choose(Some("notes".into()), window, cx)
    });
    wait(&view, cx, "the first vault", |cx| {
        view.read_with(cx, |v, _| v.ext.lib.entries.len() == 2)
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("lib-vault-allow").is_none());
}

#[gpui_kit::test]
fn no_folder_offers_to_choose_one_and_adding_asks(cx: &mut TestAppContext) {
    let v = vault();
    let (view, _, cx) = setup(
        cx,
        &v,
        "extension = markdown-notes\nextension-allow = markdown-notes ui\n",
    );
    view.update_in(cx, |v, window, cx| v.ext_start(window, cx));
    wait(&view, cx, "markdown-notes running", |cx| {
        view.read_with(cx, |v, _| {
            v.ext
                .host
                .as_ref()
                .unwrap()
                .status()
                .iter()
                .any(|s| s.name == NOTES && s.state == blyg_ext::ExtState::Running)
        })
    });
    view.read_with(cx, |v, _| {
        assert!(v.ext_libraries().is_empty(), "no silent default folder");
        assert!(v.ext.overlay.is_none(), "nothing more to ask");
    });
    view.update_in(cx, |v, window, cx| v.ext_lib_show(window, cx));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("notes-tab-library").is_some(), "the tab");
    assert!(cx.debug_bounds("lib-choose").is_some(), "Choose a folder…");

    // Two folders picked at once: two vault lines, and consent for both.
    let work = work_vault(&v);
    view.update_in(cx, |m, window, cx| {
        m.ext_add_vaults(&[v.root.clone(), work.clone()], window, cx)
    });
    let text = config_text(cx);
    let (r, w) = (vault_text(&v.root), vault_text(&work));
    assert!(
        text.contains(&format!("extension-setting = markdown-notes vault={r}")),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "extension-setting = markdown-notes vault-work-vault={w}"
        )),
        "{text}"
    );
    wait(&view, cx, "the consent sheet", |cx| {
        view.read_with(cx, |v, _| {
            matches!(v.ext.overlay, Some(Overlay::Consent { .. }))
        })
    });
    view.read_with(cx, |v, _| {
        let Some(Overlay::Consent { caps, .. }) = &v.ext.overlay else {
            unreachable!()
        };
        let caps: Vec<&Capability> = caps.iter().map(|c| &c.0).collect();
        assert_eq!(caps, [&Capability::Fs(r), &Capability::Fs(w)]);
    });
}

// ---- turn off

#[test]
fn turning_off_removes_only_the_extension_line() {
    use super::disable_changes;
    let store = ConfigStore::in_memory(
        "# mine\nextension = other\nextension = markdown-notes\n\
         extension-allow = markdown-notes ui\nextension-allow = other ui\n\
         extension-allow = markdown-notes fs:~/Notes\n\
         extension-setting = markdown-notes vault=~/Notes\n",
    );
    let mut s = ConfigStore::in_memory(store.text().unwrap());
    s.set(&disable_changes(store.config(), NOTES, false))
        .unwrap();
    let text = s.text().unwrap().to_string();
    assert!(text.starts_with("# mine\n"), "comments kept: {text}");
    assert_eq!(s.config().extensions_enabled(), ["other"]);
    assert!(
        text.contains("extension-allow = markdown-notes ui"),
        "{text}"
    );
    assert!(text.contains("extension-allow = markdown-notes fs:~/Notes"));
    assert!(text.contains("extension-setting = markdown-notes vault=~/Notes"));
    // Already off: nothing to write.
    assert!(disable_changes(s.config(), NOTES, false).is_empty());

    // Forget permissions: its grants go, the other extension's stay.
    let changes = disable_changes(s.config(), NOTES, true);
    s.set(&changes).unwrap();
    let cfg = s.config();
    assert_eq!(cfg.list("extension-allow"), ["other ui"]);
    assert_eq!(cfg.extension_settings(NOTES)["vault"], "~/Notes");
    assert!(disable_changes(cfg, NOTES, true).is_empty());

    // The last extension line goes without leaving `extension =` behind.
    let store = ConfigStore::in_memory("extension = markdown-notes\nextension = markdown-notes\n");
    let mut s = ConfigStore::in_memory(store.text().unwrap());
    s.set(&disable_changes(store.config(), NOTES, true))
        .unwrap();
    let text = s.text().unwrap().to_string();
    assert!(!text.contains("extension"), "{text}");
    assert!(s.config().extensions_enabled().is_empty());
}

/// Click `selector` once the sheet has stopped sliding in (its animation
/// runs in real time, so the bounds move until it's done).
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let t = Instant::now();
    let mut last = None;
    let b = loop {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let b = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} isn't drawn"));
        if last == Some(b) {
            break b;
        }
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "{selector} kept moving"
        );
        last = Some(b);
        std::thread::sleep(Duration::from_millis(40));
    };
    cx.simulate_click(b.center(), gpui_kit::Modifiers::none());
    cx.run_until_parked();
}

fn notes_state(view: &Entity<MainView>, cx: &mut VisualTestContext) -> blyg_ext::ExtState {
    view.read_with(cx, |v, _| {
        let st = v.ext.host.as_ref().unwrap().status();
        st.into_iter().find(|s| s.name == NOTES).unwrap().state
    })
}

fn toast_text(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, _| {
        v.toast
            .as_ref()
            .map(|t| t.text.to_string())
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn turn_off_in_manage_stops_it_and_turn_on_brings_it_back(cx: &mut TestAppContext) {
    let v = vault();
    let r = root_text(&v);
    let (view, _, cx) = running(cx, &v);
    view.update_in(cx, |v, window, cx| v.ext_open_manage(window, cx));
    click(cx, "ext-off-markdown-notes");
    wait(&view, cx, "it to stop", |cx| {
        notes_state(&view, cx) == blyg_ext::ExtState::Disabled
    });
    view.read_with(cx, |v, _| {
        assert!(v.ext_library().is_none(), "the child is gone");
        assert!(v.ext.notices.is_empty());
        // Manage stays up, showing it off.
        assert!(matches!(v.ext.overlay, Some(Overlay::Manage { .. })));
    });
    assert_eq!(toast_text(&view, cx), "markdown-notes turned off");
    let text = config_text(cx);
    assert!(!text.contains("extension = markdown-notes"), "{text}");
    assert!(text.contains(&format!("extension-allow = markdown-notes fs:{r}")));
    assert!(text.contains("extension-allow = markdown-notes ui"));
    assert!(text.contains(&format!("extension-setting = markdown-notes vault={r}")));

    // Turn on: its grants are still there, so it runs without asking.
    click(cx, "ext-on-markdown-notes");
    wait(&view, cx, "the library again", |cx| {
        view.read_with(cx, |v, _| v.ext_library().is_some())
    });
    assert!(view.read_with(cx, |v, _| matches!(
        v.ext.overlay,
        Some(Overlay::Manage { .. })
    )));
}

#[gpui_kit::test]
fn forget_permissions_turns_it_off_and_turn_on_asks_again(cx: &mut TestAppContext) {
    let v = vault();
    let r = root_text(&v);
    let (view, _, cx) = running(cx, &v);
    view.update_in(cx, |v, window, cx| v.ext_open_manage(window, cx));
    click(cx, "ext-forget-markdown-notes");
    wait(&view, cx, "it to stop", |cx| {
        notes_state(&view, cx) == blyg_ext::ExtState::Disabled
    });
    assert_eq!(
        toast_text(&view, cx),
        "markdown-notes turned off, permissions forgotten"
    );
    let text = config_text(cx);
    assert!(!text.contains("extension = markdown-notes"), "{text}");
    assert!(!text.contains("extension-allow"), "{text}");
    assert!(text.contains(&format!("extension-setting = markdown-notes vault={r}")));
    // Nothing left to forget: no chip.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("ext-forget-markdown-notes").is_none());

    // Turn on asks again, once Manage closes.
    click(cx, "ext-on-markdown-notes");
    wait(&view, cx, "the question", |cx| {
        view.read_with(cx, |v, _| {
            v.ext.notices.get(NOTES) == Some(&Notice::NeedsPermission)
        })
    });
    cx.simulate_keystrokes("escape");
    wait(&view, cx, "the consent sheet", |cx| {
        view.read_with(cx, |v, _| {
            matches!(v.ext.overlay, Some(Overlay::Consent { .. }))
        })
    });
}

#[gpui_kit::test]
fn the_consent_sheet_can_turn_it_off(cx: &mut TestAppContext) {
    let v = vault();
    let r = root_text(&v);
    let config = format!(
        "# test\nextension = markdown-notes\nextension-setting = markdown-notes vault={r}\n"
    );
    let (view, _, cx) = setup(cx, &v, &config);
    view.update_in(cx, |v, window, cx| v.ext_start(window, cx));
    wait(&view, cx, "the consent sheet", |cx| {
        view.read_with(cx, |v, _| {
            matches!(v.ext.overlay, Some(Overlay::Consent { .. }))
        })
    });
    assert!(cx.debug_bounds("ext-turn-off").is_some(), "drawn");
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-backspace"));
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        assert!(v.ext.overlay.is_none());
        assert!(v.ext.notices.is_empty(), "no nagging notice");
    });
    assert_eq!(toast_text(&view, cx), "markdown-notes turned off");
    let text = config_text(cx);
    assert!(text.contains("# test\n"), "comments kept: {text}");
    assert!(!text.contains("extension = markdown-notes"), "{text}");
    assert!(!text.contains("extension-allow"), "{text}");
    assert!(text.contains(&format!("extension-setting = markdown-notes vault={r}")));
    wait(&view, cx, "the reload", |cx| {
        notes_state(&view, cx) == blyg_ext::ExtState::Disabled
    });
    view.read_with(cx, |v, _| {
        assert!(v.ext.overlay.is_none(), "not asked again");
        assert!(v.ext.notices.is_empty());
    });
}

#[gpui_kit::test]
fn turn_off_after_not_now_clears_the_notice(cx: &mut TestAppContext) {
    let v = vault();
    let r = root_text(&v);
    let config =
        format!("extension = markdown-notes\nextension-setting = markdown-notes vault={r}\n");
    let (view, _, cx) = setup(cx, &v, &config);
    view.update_in(cx, |v, window, cx| v.ext_start(window, cx));
    wait(&view, cx, "the consent sheet", |cx| {
        view.read_with(cx, |v, _| v.ext.overlay.is_some())
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |v, _| v.ext.notices.get(NOTES).cloned()),
        Some(Notice::NeedsPermission)
    );
    // The notice reopens the sheet; its Turn off, clicked, backs out.
    view.update_in(cx, |v, window, cx| v.ext_ask_again(NOTES, window, cx));
    click(cx, "ext-turn-off");
    view.read_with(cx, |v, _| {
        assert!(v.ext.overlay.is_none());
        assert!(v.ext.notices.is_empty());
    });
    assert!(!config_text(cx).contains("extension = markdown-notes"));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("ext-notice").is_none(),
        "gone from the status bar"
    );
}
