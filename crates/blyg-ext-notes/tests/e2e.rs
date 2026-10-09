//! markdown-notes beside a REAL local blyg Worker (`wrangler dev`), the two
//! flows that cross between notes and posts. Every test is `#[ignore]`; run
//! them with `scripts/e2e-local.sh`, which sets `BLYG_E2E_URL` and
//! `BLYG_E2E_TOKEN` (see `crates/blyg-core/tests/e2e.rs`). They only ever
//! run against a local Worker.
//!
//! The extension itself never talks to the blyg (it holds no `items.*`
//! capability); these check that what the notes panel does through the host
//! ("copy into post", "save selection to notes") works end to end.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use blyg_core::api::Api;
use blyg_core::api::auth::Credential;
use blyg_core::{Backend, Kind, LiveBackend, Status, SyncOptions};
use blyg_ext::protocol::*;
use blyg_ext::*;
use blyg_ext_notes::{NAME, SAVE_SELECTION, bundled};

fn e2e() -> (String, Credential) {
    let url = std::env::var("BLYG_E2E_URL")
        .expect("BLYG_E2E_URL not set: run scripts/e2e-local.sh")
        .trim_end_matches('/')
        .to_string();
    assert!(
        url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost"),
        "e2e tests only ever run against a local Worker, not {url}"
    );
    let token = std::env::var("BLYG_E2E_TOKEN").expect("BLYG_E2E_TOKEN not set");
    (url, Credential::Token(token))
}

struct Env {
    _dir: tempfile::TempDir,
    vault: PathBuf,
    backend: Arc<LiveBackend>,
    host: Host,
    url: String,
    cred: Credential,
}

fn setup() -> Env {
    let (url, cred) = e2e();
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("Vault");
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(
        vault.join("Field notes.md"),
        "---\ntags: [e2e]\n---\nA note that becomes a post.\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("db")).unwrap();
    let backend = Arc::new(
        LiveBackend::open_with(
            &dir.path().join("db"),
            &url,
            cred.clone(),
            SyncOptions {
                debounce: Duration::from_millis(150),
                ..SyncOptions::default()
            },
        )
        .unwrap(),
    );
    let v = vault.to_string_lossy().into_owned();
    let settings: BTreeMap<String, String> = [("vault".to_string(), v.clone())].into();
    let mut c = HostConfig::new(dir.path().join("data"), "0.0.0-e2e");
    c.bundled = vec![bundled(
        PathBuf::from(env!("CARGO_BIN_EXE_burrow-markdown-notes")),
        vec![],
        &settings,
    )];
    c.enabled = vec![NAME.into()];
    c.grants = Grants::from_allow_lines(&[format!("{NAME} ui"), format!("{NAME} fs:{v}")]).0;
    c.settings = [(NAME.to_string(), settings)].into();
    let (tx, rx) = channel();
    let host = Host::new(c, Arc::new(BackendApi::new(backend.clone())), move |e| {
        let _ = tx.send(e);
    });
    host.start();
    loop {
        match rx
            .recv_timeout(Duration::from_secs(15))
            .expect("markdown-notes didn't start")
        {
            ExtEvent::Started { .. } => break,
            ExtEvent::Failed { message, .. } => panic!("{message}"),
            _ => {}
        }
    }
    Env {
        _dir: dir,
        vault,
        backend,
        host,
        url,
        cred,
    }
}

fn eventually(what: &str, f: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !f() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
#[ignore]
fn a_note_copied_into_a_post_reaches_the_blyg() {
    let env = setup();
    let doc = env
        .host
        .library_read(
            NAME,
            &LibraryReadParams {
                library: None,
                id: "Field notes.md".into(),
            },
        )
        .unwrap();
    // "Copy into post": the host inserts the body (no frontmatter) into a draft.
    let id = env.backend.create_draft(Kind::Fragment, &doc.body).unwrap();
    let api = Api::new(&env.url, env.cred.clone());
    eventually("the draft on the server", || {
        api.list_items()
            .map(|items| {
                items
                    .iter()
                    .any(|i| i.content_md == "A note that becomes a post.\n")
            })
            .unwrap_or(false)
    });
    let item = env.backend.item(&id).unwrap();
    assert_eq!(item.status, Status::Draft);
    assert!(
        !item.content_md.contains("tags:"),
        "frontmatter stays in the note"
    );
}

#[test]
#[ignore]
fn published_text_saved_to_notes_is_a_plain_note() {
    let env = setup();
    let text = format!("Published from the e2e at {:?}", Instant::now());
    let id = env.backend.create_draft(Kind::Fragment, &text).unwrap();
    let out = env.backend.publish(&id, None).unwrap();
    assert!(!out.permalink.is_empty());
    let ctx = CommandContext {
        selection: Some(text.clone()),
        ..Default::default()
    };
    let r = env.host.command(NAME, SAVE_SELECTION, ctx).unwrap();
    assert!(r.toast.unwrap().starts_with("Saved to notes: "));
    let saved: Vec<String> = std::fs::read_dir(&env.vault)
        .unwrap()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter(|t| t == &text)
        .collect();
    assert_eq!(saved.len(), 1, "one plain note, no burrow_* frontmatter");
}
