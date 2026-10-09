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
use super::{Launch, NOTES, Notice, allow_changes, host_config, vault_changes, vault_text};
use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

// ------------------------------------------------------------ the child

/// This test's name, as the harness filters by it.
fn child_name() -> String {
    let m = module_path!();
    let m = m.split_once("::").map(|(_, rest)| rest).unwrap_or(m);
    format!("{m}::markdown_notes_child")
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
        args: vec![
            "--exact".into(),
            child_name(),
            "--ignored".into(),
            "--quiet".into(),
            "--test-threads=1".into(),
        ],
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

    // A new vault replaces the vault line and keeps the others.
    let changes = vault_changes(s.config(), "~/Vault");
    s.set(&changes).unwrap();
    let cfg = s.config();
    assert_eq!(cfg.extension_settings(NOTES)["vault"], "~/Vault");
    assert_eq!(cfg.extension_settings(NOTES)["folder"], "Inbox");
    assert_eq!(
        cfg.list("extension-setting")
            .iter()
            .filter(|l| l.contains("vault="))
            .count(),
        1
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
    let config = format!("blyg-url = https://blyg.example.com\n{config}");
    let prefs = Prefs::from_config(ConfigStore::in_memory(&config).config());
    let l = launch(v.data.clone());
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
            "Browse Notes",
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
    view.update_in(cx, |m, window, cx| m.ext_set_vault(&v.root, window, cx));
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
