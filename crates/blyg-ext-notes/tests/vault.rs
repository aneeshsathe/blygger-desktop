//! markdown-notes through the real host and a real extension process, with
//! **no blyg connected** (`NoBlyg`): the notes library works on its own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use blyg_ext::protocol::*;
use blyg_ext::*;
use blyg_ext_notes::{LIBRARY, NAME, SAVE_SELECTION, bundled, frontmatter};

const WAIT: Duration = Duration::from_secs(15);

struct Env {
    _dir: tempfile::TempDir,
    vault: PathBuf,
    host: Host,
    events: Receiver<ExtEvent>,
}

fn write(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// A vault with a few notes, and the extension started with `grants`.
fn setup(grant_fs: bool) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("Vault");
    write(
        &vault.join("Tide tables.md"),
        "# Tide tables\nThe moon pulls the sea twice a day.\n",
    );
    write(
        &vault.join("Inbox").join("Crlf note.md"),
        "---\r\ntitle: Written on Windows\r\ntags: [crlf]\r\n---\r\nLines end in CR LF.\r\nSecond line.\r\n",
    );
    write(
        &vault
            .join("Projects")
            .join("Deep")
            .join("Lighthouse.markdown"),
        "Keepers and lamps.\n",
    );
    write(&vault.join(".obsidian").join("hidden.md"), "never listed\n");
    write(&vault.join("image.png"), "not a note");

    let vault_str = vault.to_string_lossy().into_owned();
    let settings: BTreeMap<String, String> = [
        ("vault".to_string(), vault_str.clone()),
        ("folder".to_string(), "Inbox".to_string()),
        ("poll-ms".to_string(), "100".to_string()),
    ]
    .into();
    let mut c = HostConfig::new(dir.path().join("data"), "0.0.0-test");
    c.bundled = vec![bundled(
        PathBuf::from(env!("CARGO_BIN_EXE_burrow-markdown-notes")),
        vec![],
        &settings,
    )];
    c.enabled = vec![NAME.into()];
    let mut lines = vec![format!("{NAME} ui")];
    if grant_fs {
        lines.push(format!("{NAME} fs:{vault_str}"));
    }
    c.grants = Grants::from_allow_lines(&lines).0;
    c.settings = [(NAME.to_string(), settings)].into();
    let (tx, events) = channel();
    let host = Host::new(c, Arc::new(NoBlyg), move |e| {
        let _ = tx.send(e);
    });
    host.start();
    let env = Env {
        _dir: dir,
        vault,
        host,
        events,
    };
    env.started();
    env
}

impl Env {
    fn started(&self) {
        let deadline = Instant::now() + WAIT;
        loop {
            match self
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(ExtEvent::Started { .. }) => return,
                Ok(ExtEvent::Failed {
                    message, stderr, ..
                }) => panic!("{message}: {stderr:?}"),
                Ok(_) => {}
                Err(_) => panic!("markdown-notes didn't start"),
            }
        }
    }

    fn list(&self, path: Option<&str>) -> Vec<(String, bool)> {
        let p = LibraryListParams {
            library: None,
            path: path.map(str::to_string),
        };
        self.host
            .library_list(NAME, &p)
            .unwrap()
            .into_iter()
            .map(|e| (e.id, e.is_dir))
            .collect()
    }

    fn search(&self, q: &str) -> Vec<SourceEntry> {
        let p = LibrarySearchParams {
            library: Some(LIBRARY.into()),
            query: q.into(),
            limit: 20,
        };
        self.host.library_search(NAME, &p).unwrap()
    }

    fn read(&self, id: &str) -> Result<LibraryDocument, ExtError> {
        self.host.library_read(
            NAME,
            &LibraryReadParams {
                library: None,
                id: id.into(),
            },
        )
    }

    fn write(
        &self,
        id: Option<&str>,
        markdown: &str,
        base: Option<&str>,
    ) -> Result<LibraryWritten, ExtError> {
        let p = LibraryWriteParams {
            library: None,
            id: id.map(str::to_string),
            title: None,
            folder: None,
            markdown: markdown.into(),
            base_hash: base.map(str::to_string),
        };
        self.host.library_write(NAME, &p)
    }

    fn files(&self) -> Vec<String> {
        let mut out = vec![];
        fn walk(d: &Path, rel: &str, out: &mut Vec<String>) {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                let r = if rel.is_empty() {
                    n.clone()
                } else {
                    format!("{rel}/{n}")
                };
                if e.path().is_dir() {
                    walk(&e.path(), &r, out);
                } else {
                    out.push(r);
                }
            }
        }
        walk(&self.vault, "", &mut out);
        out.sort();
        out
    }
}

#[test]
fn lists_the_tree_one_level_at_a_time_skipping_hidden_and_non_notes() {
    let env = setup(true);
    assert_eq!(
        env.list(None),
        [
            ("Inbox".to_string(), true),
            ("Projects".to_string(), true),
            ("Tide tables.md".to_string(), false)
        ]
    );
    assert_eq!(
        env.list(Some("Projects")),
        [("Projects/Deep".to_string(), true)]
    );
    assert_eq!(
        env.list(Some("Projects\\Deep")),
        [("Projects/Deep/Lighthouse.markdown".to_string(), false)]
    );
    assert!(matches!(
        env.host.library_list(NAME, &LibraryListParams { library: None, path: Some("../..".into()) }),
        Err(ExtError::Rpc(e)) if e.code == codes::INVALID_PARAMS
    ));
}

#[test]
fn searches_titles_and_bodies() {
    let env = setup(true);
    let hits = env.search("moon");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "Tide tables");
    assert!(hits[0].excerpt.contains("The moon pulls"));
    assert_eq!(hits[0].url, None, "never a file:// link");
    assert_eq!(
        env.search("windows")[0].id,
        "Inbox/Crlf note.md",
        "frontmatter title matches"
    );
    assert_eq!(
        env.search("keepers lamps")[0].id,
        "Projects/Deep/Lighthouse.markdown"
    );
    assert!(
        env.search("never listed").is_empty(),
        "hidden folders aren't indexed"
    );
    assert_eq!(env.search("").len(), 3, "empty = most recent");
}

#[test]
fn reads_crlf_notes_with_frontmatter_and_windows_style_ids() {
    let env = setup(true);
    let d = env.read("Inbox\\Crlf note.md").unwrap();
    assert_eq!(d.id, "Inbox/Crlf note.md");
    assert_eq!(d.title, "Written on Windows");
    assert_eq!(d.body, "Lines end in CR LF.\r\nSecond line.\r\n");
    assert!(d.markdown.starts_with("---\r\ntitle:"));
    assert_eq!(d.hash, blyg_core::content_hash(&d.markdown));
    assert!(
        matches!(env.read("../outside.md"), Err(ExtError::Rpc(e)) if e.code == codes::INVALID_PARAMS)
    );
    assert!(matches!(env.read("missing.md"), Err(ExtError::Rpc(e)) if e.code == codes::REFUSED));
}

#[test]
fn edits_keep_frontmatter_and_crlf_byte_for_byte() {
    let env = setup(true);
    let d = env.read("Inbox/Crlf note.md").unwrap();
    // The panel edits the body only; the frontmatter is carried through.
    let edited =
        frontmatter::with_body(&d.markdown, "Lines end in CR LF.\r\nEdited in Burrow.\r\n");
    let w = env.write(Some(&d.id), &edited, Some(&d.hash)).unwrap();
    let on_disk = std::fs::read(env.vault.join("Inbox").join("Crlf note.md")).unwrap();
    assert_eq!(
        on_disk,
        b"---\r\ntitle: Written on Windows\r\ntags: [crlf]\r\n---\r\nLines end in CR LF.\r\nEdited in Burrow.\r\n"
    );
    assert_eq!(w.hash, blyg_core::content_hash(&edited));
    // Unchanged round trip is byte-identical too.
    let d2 = env.read(&d.id).unwrap();
    env.write(Some(&d.id), &d2.markdown, Some(&d2.hash))
        .unwrap();
    assert_eq!(
        std::fs::read(env.vault.join("Inbox").join("Crlf note.md")).unwrap(),
        on_disk
    );
    assert!(
        !env.files().iter().any(|f| f.contains("burrow-tmp")),
        "{:?}",
        env.files()
    );
}

#[test]
fn a_note_changed_on_disk_is_never_clobbered() {
    let env = setup(true);
    let d = env.read("Tide tables.md").unwrap();
    let path = env.vault.join("Tide tables.md");
    std::fs::write(&path, "# Tide tables\nEdited in Obsidian meanwhile.\n").unwrap();
    let r = env.write(Some(&d.id), "# Tide tables\nFrom Burrow.\n", Some(&d.hash));
    assert_eq!(
        r,
        Err(ExtError::Stale {
            current_hash: blyg_core::content_hash("# Tide tables\nEdited in Obsidian meanwhile.\n")
        })
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# Tide tables\nEdited in Obsidian meanwhile.\n"
    );
    // Overwriting without saying what it was based on is refused too.
    assert!(matches!(
        env.write(Some(&d.id), "x", None),
        Err(ExtError::Rpc(e)) if e.code == codes::INVALID_PARAMS
    ));
}

#[test]
fn save_selection_makes_a_plain_new_note_and_never_overwrites() {
    let env = setup(true);
    let ctx = || CommandContext {
        selection: Some("Tide tables, revisited\nA quote worth keeping.".into()),
        ..Default::default()
    };
    let r = env.host.command(NAME, SAVE_SELECTION, ctx()).unwrap();
    assert_eq!(
        r.toast.as_deref(),
        Some("Saved to notes: Tide tables, revisited")
    );
    let r2 = env.host.command(NAME, SAVE_SELECTION, ctx()).unwrap();
    assert_eq!(
        r2.toast.as_deref(),
        Some("Saved to notes: Tide tables, revisited 2")
    );
    let first =
        std::fs::read_to_string(env.vault.join("Inbox").join("Tide tables, revisited.md")).unwrap();
    assert_eq!(
        first, "Tide tables, revisited\nA quote worth keeping.",
        "verbatim, no frontmatter added"
    );
    assert!(
        env.vault
            .join("Inbox")
            .join("Tide tables, revisited 2.md")
            .exists()
    );

    // A new note named like an existing one gets a number, never replaces it.
    let w = env.write(None, "# Tide tables\nanother", None).unwrap();
    assert_eq!(w.id, "Inbox/Tide tables.md", "the default folder is Inbox");
    let w = env
        .host
        .library_write(
            NAME,
            &LibraryWriteParams {
                library: None,
                id: None,
                title: Some("Tide tables".into()),
                folder: Some("/".into()),
                markdown: "top level".into(),
                base_hash: None,
            },
        )
        .unwrap();
    assert_eq!(w.id, "Tide tables 2.md");
    assert_eq!(
        std::fs::read_to_string(env.vault.join("Tide tables.md")).unwrap(),
        "# Tide tables\nThe moon pulls the sea twice a day.\n"
    );
    let empty = CommandContext::default();
    assert!(env.host.command(NAME, SAVE_SELECTION, empty).is_err());
}

#[test]
fn nothing_is_ever_deleted() {
    let env = setup(true);
    let before = env.files();
    // The protocol has no delete; the only command is "save selection".
    let st = &env.host.status()[0];
    let ids: Vec<&str> = st.commands.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, [SAVE_SELECTION]);
    // Writing every note back unchanged and searching leaves every file in place.
    for id in [
        "Tide tables.md",
        "Inbox/Crlf note.md",
        "Projects/Deep/Lighthouse.markdown",
    ] {
        let d = env.read(id).unwrap();
        env.write(Some(id), &d.markdown, Some(&d.hash)).unwrap();
    }
    assert_eq!(env.files(), before);
}

#[test]
fn notes_added_on_disk_show_up_after_a_poll() {
    let env = setup(true);
    assert!(env.search("aurora").is_empty());
    write(
        &env.vault.join("New").join("Aurora.md"),
        "Green lights in the north.\n",
    );
    let deadline = Instant::now() + WAIT;
    while env.search("aurora").is_empty() {
        assert!(Instant::now() < deadline, "never indexed");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(env.search("aurora")[0].id, "New/Aurora.md");
}

#[test]
fn without_the_folder_grant_it_touches_nothing() {
    let env = setup(false);
    let before = env.files();
    let denied = |r: Result<_, ExtError>| matches!(r, Err(ExtError::Rpc(e)) if e.code == codes::PERMISSION_DENIED);
    assert!(denied(
        env.host
            .library_list(NAME, &LibraryListParams::default())
            .map(|_| ())
    ));
    assert!(denied(env.read("Tide tables.md").map(|_| ())));
    assert!(denied(env.write(None, "x", None).map(|_| ())));
    let st = &env.host.status()[0];
    assert_eq!(
        st.missing.len(),
        1,
        "the consent sheet can offer the folder"
    );
    assert!(describe(&st.missing[0]).starts_with("read and write files in "));
    assert_eq!(env.files(), before);
}
