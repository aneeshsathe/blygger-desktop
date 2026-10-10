//! The host against a real extension process (`bxp-test-ext`) and a real
//! `LiveBackend` on a temp dir with the sync worker off: everything the
//! extension can reach is local (`items`, `create_draft`, `save`, …), so
//! nothing dials the (unused) server address and no mock is needed.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use blyg_core::{Backend, Kind, LiveBackend, PublishOutcome, Status, SyncOptions, content_hash};
use blyg_ext::protocol::*;
use blyg_ext::spawn::KEEP_ENV;
use blyg_ext::*;
use serde_json::{Value, json};

const NAME: &str = "bxp-test";
const WAIT: Duration = Duration::from_secs(15);

struct Env {
    dir: tempfile::TempDir,
    backend: Arc<LiveBackend>,
    host: Host,
    events: Receiver<ExtEvent>,
}

fn manifest(capabilities: &[&str]) -> String {
    let exe = env!("CARGO_BIN_EXE_bxp-test-ext");
    let caps: Vec<String> = capabilities.iter().map(|c| format!("{c:?}")).collect();
    format!(
        r#"name = "{NAME}"
version = "0.0.1"
protocol = 1
description = "test extension"
command = ['{exe}']
capabilities = [{}]

[[commands]]
id = "echo"
title = "Echo"

[[commands]]
id = "only-in-editor"
title = "Editor only"
when = "editor"

[[libraries]]
id = "notes"
title = "Notes"
writable = true
{}"#,
        caps.join(", "),
        if capabilities.contains(&SOCIAL) {
            BROWSER_TABLES
        } else {
            ""
        }
    )
}

/// A manifest asking for this gets a site and a macro on it.
const SOCIAL: &str = "browser.automate:https://social.example.com";

const BROWSER_TABLES: &str = r#"
[[sites]]
id = "social"
title = "Social"
origin = "https://social.example.com"
home = "https://social.example.com/notes"

[[macros]]
id = "cross-post"
title = "Cross-post to Social…"
site = "social"
steps = [
  { do = "open", url = "https://social.example.com/notes" },
  { do = "insert", selector = "textarea" },
  { do = "submit", selector = "button", text = "Post" },
  { do = "done", text = "Posted" },
]
"#;

fn timing() -> Timing {
    Timing {
        initialize: Duration::from_secs(5),
        command: Duration::from_secs(5),
        shutdown: Duration::from_secs(5),
        backoff: vec![Duration::from_millis(50), Duration::from_millis(100)],
        ..Timing::default()
    }
}

fn config(dir: &Path, grants: &[&str], settings: &[&str]) -> HostConfig {
    let mut c = HostConfig::new(dir.join("data"), "0.0.0-test");
    c.enabled = vec![NAME.into()];
    let lines: Vec<String> = grants.iter().map(|g| format!("{NAME} {g}")).collect();
    c.grants = Grants::from_allow_lines(&lines).0;
    let lines: Vec<String> = settings.iter().map(|s| format!("{NAME} {s}")).collect();
    c.settings = settings_from_lines(&lines).0;
    c.extensions_dir = Some(dir.join("extensions"));
    c.timing = timing();
    c
}

fn setup_with(
    caps: &[&str],
    grants: &[&str],
    settings: &[&str],
    tweak: impl FnOnce(&mut HostConfig),
) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let ext_dir = dir.path().join("extensions").join(NAME);
    std::fs::create_dir_all(&ext_dir).unwrap();
    std::fs::write(ext_dir.join(MANIFEST_FILE), manifest(caps)).unwrap();
    std::fs::create_dir_all(dir.path().join("db")).unwrap();
    let backend = Arc::new(
        LiveBackend::open_with(
            &dir.path().join("db"),
            "http://127.0.0.1:9",
            "unused-test-credential",
            SyncOptions {
                start_worker: false,
                ..SyncOptions::default()
            },
        )
        .unwrap(),
    );
    let mut cfg = config(dir.path(), grants, settings);
    tweak(&mut cfg);
    let (tx, events) = channel();
    let host = Host::new(cfg, Arc::new(BackendApi::new(backend.clone())), move |e| {
        let _ = tx.send(e);
    });
    host.start();
    Env {
        dir,
        backend,
        host,
        events,
    }
}

fn setup(caps: &[&str], grants: &[&str], settings: &[&str]) -> Env {
    setup_with(caps, grants, settings, |_| {})
}

impl Env {
    /// The next event matching `f`, skipping others.
    fn wait_for(&self, what: &str, f: impl Fn(&ExtEvent) -> bool) -> ExtEvent {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(e) if f(&e) => return e,
                Ok(_) => continue,
                Err(_) => panic!("no {what} event within {WAIT:?}"),
            }
        }
    }

    fn started(&self) {
        self.wait_for("Started", |e| matches!(e, ExtEvent::Started { .. }));
    }

    fn storage(&self) -> PathBuf {
        storage_dir(&self.dir.path().join("data"), NAME)
    }

    fn command(&self, id: &str, selection: Option<&str>) -> Result<CommandResult, ExtError> {
        command(&self.host, id, selection)
    }

    fn call_host(&self, method: &str, params: Value) -> Value {
        call_host(&self.host, method, params)
    }
}

fn command(host: &Host, id: &str, selection: Option<&str>) -> Result<CommandResult, ExtError> {
    let ctx = CommandContext {
        selection: selection.map(str::to_string),
        ..Default::default()
    };
    host.command(NAME, id, ctx)
}

/// Have the extension call `method` on the host; `{"ok": result}` or `{"err": error}`.
fn call_host(host: &Host, method: &str, params: Value) -> Value {
    let req = json!({ "method": method, "params": params }).to_string();
    let r = command(host, "call-host", Some(&req)).unwrap();
    serde_json::from_str(&r.toast.unwrap()).unwrap()
}

fn read_lines(p: &Path) -> Vec<String> {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn handshake_passes_grants_settings_and_storage_but_no_origin() {
    let env = setup(
        &["ui", "items.read"],
        &["ui", "items.read"],
        &["vault=~/Notes"],
    );
    env.started();
    let starts = read_lines(&env.storage().join("starts.log"));
    assert_eq!(starts.len(), 1);
    let init: Value = serde_json::from_str(&starts[0]).unwrap();
    assert_eq!(init["protocolVersion"], 1);
    assert_eq!(init["granted"], json!(["ui", "items.read"]));
    assert_eq!(init["settings"]["vault"], "~/Notes");
    assert_eq!(
        PathBuf::from(init["storageDir"].as_str().unwrap()),
        env.storage()
    );
    assert!(
        init.get("blygOrigin").is_none(),
        "no blyg.identity, no origin"
    );

    let st = env.host.status();
    assert_eq!(st.len(), 1);
    assert_eq!(st[0].state, ExtState::Running);
    assert!(st[0].missing.is_empty());

    let ids = |screen, has_item| -> Vec<String> {
        env.host
            .palette(screen, has_item)
            .into_iter()
            .map(|p| p.command.id)
            .collect()
    };
    assert_eq!(ids(Screen::Posts, true), ["echo", "only-in-editor"]);
    assert_eq!(ids(Screen::Reading, false), ["echo"]);
    assert_eq!(env.host.libraries()[0].library.id, "notes");
    assert_eq!(
        env.command("echo", Some("hi there"))
            .unwrap()
            .toast
            .as_deref(),
        Some("hi there")
    );
}

#[test]
fn identity_grant_passes_the_origin_only() {
    let env = setup(&["blyg.identity"], &["blyg.identity"], &[]);
    env.started();
    let init: Value =
        serde_json::from_str(&read_lines(&env.storage().join("starts.log"))[0]).unwrap();
    assert_eq!(init["blygOrigin"], "http://127.0.0.1:9");
    let text = std::fs::read_to_string(env.storage().join("starts.log")).unwrap();
    assert!(
        !text.contains("unused-test-credential"),
        "never the credential"
    );
}

#[test]
fn calls_without_a_grant_are_denied() {
    // Asks for items.read and ui, granted only ui.
    let env = setup(&["ui", "items.read", "items.write"], &["ui"], &[]);
    env.started();
    for m in [
        methods::LIST_ITEMS,
        methods::GET_ITEM,
        methods::CREATE_DRAFT,
        methods::SAVE_ITEM,
        methods::LIST_READING,
    ] {
        let r = env.call_host(m, json!({"id": "x", "contentMd": "x"}));
        assert_eq!(r["err"]["code"], codes::PERMISSION_DENIED, "{m}: {r}");
    }
    assert!(env.backend.items().is_empty());
    // There is no publish method at all.
    let r = env.call_host("burrow/publish", json!({"id": "x"}));
    assert_eq!(r["err"]["code"], codes::METHOD_NOT_FOUND);
}

#[test]
fn macros_are_listed_only_with_their_site_granted() {
    let env = setup(
        &["ui", SOCIAL, "browser.capture"],
        &["ui", SOCIAL, "browser.capture"],
        &[],
    );
    env.started();
    let macros = env.host.macros();
    assert_eq!(macros.len(), 1, "{macros:?}");
    assert_eq!(macros[0].ext, NAME);
    assert_eq!(macros[0].site.origin, "https://social.example.com");
    assert_eq!(macros[0].spec.when, When::Published);
    assert!(env.host.macro_entry(NAME, "cross-post").is_some());
    assert!(env.host.macro_entry(NAME, "nope").is_none());
    // The test extension has no macro.prepare: use the template.
    let item = ItemSummary {
        id: "L1".into(),
        server_id: None,
        kind: "fragment".into(),
        status: "public".into(),
        version: 1,
        dirty: false,
        created: String::new(),
        updated: String::new(),
        permalink: Some("https://blyg.example.com/p/1".into()),
        title: "Hi".into(),
        content_hash: content_hash("Hi"),
        content_md: Some("Hi".into()),
    };
    let prepared = env.host.macro_prepare(
        NAME,
        &MacroPrepareParams {
            macro_id: "cross-post".into(),
            item,
            permalink: "https://blyg.example.com/p/1".into(),
            text: "Hi".into(),
        },
    );
    assert_eq!(prepared, Ok(None));
    let st = env.host.status();
    assert_eq!(st[0].macros.len(), 1);
    assert_eq!(st[0].sites[0].id, "social");
}

/// `call_host` on another thread, so this one can answer the window's
/// side of the call.
fn call_host_bg(env: &Env, method: &'static str, params: Value) -> std::thread::JoinHandle<Value> {
    let host = env.host.clone();
    std::thread::spawn(move || call_host(&host, method, params))
}

/// The events that came in by now, without waiting.
fn drain(env: &Env) -> Vec<ExtEvent> {
    std::iter::from_fn(|| env.events.try_recv().ok()).collect()
}

#[test]
fn browser_open_hops_to_the_window_for_a_granted_origin_only() {
    let env = setup(&["ui", SOCIAL], &["ui", SOCIAL], &[]);
    env.started();
    // Another origin never reaches the window.
    let r = env.call_host(
        methods::BROWSER_OPEN,
        json!({"url": "https://other.example.com/"}),
    );
    assert_eq!(r["err"]["code"], codes::PERMISSION_DENIED, "{r}");
    assert!(
        !drain(&env)
            .iter()
            .any(|e| matches!(e, ExtEvent::BrowserOpen { .. })),
        "refused before the window"
    );
    // The granted one: the window opens the pane and answers the URL.
    let t = call_host_bg(
        &env,
        methods::BROWSER_OPEN,
        json!({"url": "https://social.example.com/notes"}),
    );
    let ExtEvent::BrowserOpen { name, url, reply } =
        env.wait_for("BrowserOpen", |e| matches!(e, ExtEvent::BrowserOpen { .. }))
    else {
        unreachable!()
    };
    assert_eq!(
        (name.as_str(), url.as_str()),
        (NAME, "https://social.example.com/notes")
    );
    reply.answer(Ok(url));
    let r = t.join().unwrap();
    assert_eq!(r["ok"]["url"], "https://social.example.com/notes", "{r}");
    // The window says no (a macro is running there): refused, with why.
    let t = call_host_bg(
        &env,
        methods::BROWSER_OPEN,
        json!({"url": "https://social.example.com/"}),
    );
    let ExtEvent::BrowserOpen { reply, .. } =
        env.wait_for("BrowserOpen", |e| matches!(e, ExtEvent::BrowserOpen { .. }))
    else {
        unreachable!()
    };
    reply.answer(Err("a macro is running in the browser pane".into()));
    let r = t.join().unwrap();
    assert_eq!(r["err"]["code"], codes::REFUSED, "{r}");
    assert_eq!(
        r["err"]["message"],
        "a macro is running in the browser pane"
    );
}

#[test]
fn browser_page_is_answered_only_while_a_command_runs() {
    let env = setup(
        &["ui", SOCIAL, "browser.capture"],
        &["ui", SOCIAL, "browser.capture"],
        &[],
    );
    env.started();
    // During a command: the window reads the pane.
    let t = call_host_bg(&env, methods::BROWSER_PAGE, json!({}));
    let ExtEvent::BrowserPage { name, reply } =
        env.wait_for("BrowserPage", |e| matches!(e, ExtEvent::BrowserPage { .. }))
    else {
        unreachable!()
    };
    assert_eq!(name, NAME);
    reply.answer(Some(PageCapture {
        url: "https://news.example.com/a".into(),
        title: "Tide pools".into(),
        selection: "small oceans".into(),
        markdown: "Tide pools are small oceans.".into(),
        ..PageCapture::default()
    }));
    let r = t.join().unwrap();
    assert_eq!(r["ok"]["url"], "https://news.example.com/a", "{r}");
    assert_eq!(r["ok"]["selection"], "small oceans");
    assert_eq!(r["ok"]["markdown"], "Tide pools are small oceans.");
    // No page in the pane (or the window let go of the reply): -32004.
    for answer in [true, false] {
        let t = call_host_bg(&env, methods::BROWSER_PAGE, json!({}));
        let ExtEvent::BrowserPage { reply, .. } =
            env.wait_for("BrowserPage", |e| matches!(e, ExtEvent::BrowserPage { .. }))
        else {
            unreachable!()
        };
        if answer {
            reply.answer(None);
        } else {
            drop(reply);
        }
        let r = t.join().unwrap();
        assert_eq!(r["err"]["code"], codes::NO_PAGE, "{r}");
    }
    // Outside a command (here, from macro.prepare): -32004, and the window
    // is never asked.
    let item = ItemSummary {
        id: "L1".into(),
        server_id: None,
        kind: "fragment".into(),
        status: "public".into(),
        version: 1,
        dirty: false,
        created: String::new(),
        updated: String::new(),
        permalink: None,
        title: "Hi".into(),
        content_hash: content_hash("Hi"),
        content_md: None,
    };
    let prepared = env
        .host
        .macro_prepare(
            NAME,
            &MacroPrepareParams {
                macro_id: "page".into(),
                item,
                permalink: String::new(),
                text: String::new(),
            },
        )
        .unwrap()
        .expect("answered");
    let r: Value = serde_json::from_str(&prepared.text).unwrap();
    assert_eq!(r["err"]["code"], codes::NO_PAGE, "{r}");
    assert!(
        !drain(&env)
            .iter()
            .any(|e| matches!(e, ExtEvent::BrowserPage { .. })),
        "the window wasn't asked"
    );
}

#[test]
fn an_ungranted_site_hides_its_macros_and_the_page() {
    let env = setup(&["ui", SOCIAL, "browser.capture"], &["ui"], &[]);
    env.started();
    assert!(env.host.macros().is_empty());
    let r = env.call_host(methods::BROWSER_PAGE, json!({}));
    assert_eq!(r["err"]["code"], codes::PERMISSION_DENIED, "{r}");
    assert_eq!(r["err"]["data"]["capability"], "browser.capture");
    let r = env.call_host(
        methods::BROWSER_OPEN,
        json!({"url": "https://social.example.com/notes"}),
    );
    assert_eq!(r["err"]["code"], codes::PERMISSION_DENIED, "{r}");
}

#[test]
fn create_draft_and_scratch_land_in_the_backend() {
    let env = setup(
        &["items.read", "items.write"],
        &["items.read", "items.write"],
        &[],
    );
    env.started();
    let r = env.call_host(
        methods::CREATE_DRAFT,
        json!({"contentMd": "From an extension"}),
    );
    let id = r["ok"]["id"].as_str().unwrap().to_string();
    assert_eq!(r["ok"]["contentHash"], content_hash("From an extension"));
    let r = env.call_host(
        methods::CREATE_DRAFT,
        json!({"contentMd": "a scratch", "scratch": true}),
    );
    let scratch = r["ok"]["id"].as_str().unwrap().to_string();

    let items = env.backend.items();
    let draft = items.iter().find(|i| i.local_id.0 == id).unwrap();
    assert_eq!((draft.status, draft.kind), (Status::Draft, Kind::Fragment));
    assert_eq!(draft.content_md, "From an extension");
    let s = items.iter().find(|i| i.local_id.0 == scratch).unwrap();
    assert_eq!(s.status, Status::Scratch);

    let r = env.call_host(methods::GET_ITEM, json!({"id": id}));
    assert_eq!(r["ok"]["contentMd"], "From an extension");
    assert_eq!(r["ok"]["title"], "From an extension");
    let r = env.call_host(methods::LIST_ITEMS, json!({"status": ["scratch"]}));
    assert_eq!(r["ok"].as_array().unwrap().len(), 1);
    assert!(
        r["ok"][0].get("contentMd").is_none(),
        "no content unless asked"
    );
    let r = env.call_host(methods::SEARCH_ITEMS, json!({"query": "extension"}));
    assert_eq!(r["ok"][0]["id"], id.as_str());
    let r = env.call_host(methods::GET_ITEM, json!({"id": "missing"}));
    assert_eq!(r["err"]["code"], codes::REFUSED);
}

#[test]
fn save_item_refuses_a_stale_base_hash() {
    let env = setup(&["items.write"], &["items.write"], &[]);
    env.started();
    let id = env.backend.create_draft(Kind::Fragment, "v1").unwrap();
    env.backend.save(&id, "v2, typed in Burrow").unwrap();

    let r = env.call_host(
        methods::SAVE_ITEM,
        json!({"id": id.0, "contentMd": "from the extension", "baseHash": content_hash("v1")}),
    );
    assert_eq!(r["err"]["code"], codes::STALE, "{r}");
    assert_eq!(
        r["err"]["data"]["currentHash"],
        content_hash("v2, typed in Burrow")
    );
    assert_eq!(
        env.backend.item(&id).unwrap().content_md,
        "v2, typed in Burrow"
    );

    let r = env.call_host(
        methods::SAVE_ITEM,
        json!({"id": id.0, "contentMd": "from the extension", "baseHash": content_hash("v2, typed in Burrow")}),
    );
    assert_eq!(r["ok"]["ok"], true, "{r}");
    assert_eq!(
        env.backend.item(&id).unwrap().content_md,
        "from the extension"
    );
}

#[test]
fn a_slow_command_times_out_and_repeated_timeouts_restart_it() {
    let env = setup_with(&[], &[], &[], |c| {
        c.timing.command = Duration::from_millis(200);
        c.timing.timeouts_before_restart = 2;
    });
    env.started();
    assert_eq!(env.command("sleep", None), Err(ExtError::Timeout));
    assert_eq!(ExtError::Timeout.message(NAME), "bxp-test didn't answer");
    assert_eq!(env.command("sleep", None), Err(ExtError::Timeout));
    let e = env.wait_for("Failed", |e| matches!(e, ExtEvent::Failed { .. }));
    let ExtEvent::Failed { message, .. } = e else {
        unreachable!()
    };
    assert!(message.contains("stopped answering"), "{message}");
    env.started();
    assert_eq!(read_lines(&env.storage().join("starts.log")).len(), 2);
}

#[test]
fn a_crash_restarts_with_backoff_and_keeps_its_stderr() {
    let env = setup(&[], &[], &[]);
    env.started();
    env.command("stderr", None).unwrap();
    assert!(env.command("crash", None).is_err());
    let e = env.wait_for("Failed", |e| matches!(e, ExtEvent::Failed { .. }));
    let ExtEvent::Failed {
        message, stderr, ..
    } = e
    else {
        unreachable!()
    };
    assert!(message.contains("stopped unexpectedly"), "{message}");
    assert!(message.contains("exit code 7"), "{message}");
    assert!(
        stderr.iter().any(|l| l == "second line to stderr"),
        "{stderr:?}"
    );
    env.started();
    assert_eq!(
        env.command("echo", Some("back")).unwrap().toast.as_deref(),
        Some("back")
    );
    let log = std::fs::read_to_string(env.storage().join("stderr.log")).unwrap();
    assert!(log.contains("first line to stderr"));
}

#[test]
fn three_failed_starts_stop_it_until_reload() {
    let env = setup(&[], &[], &["crash-init=1"]);
    let e = env.wait_for("Stopped", |e| matches!(e, ExtEvent::Stopped { .. }));
    let ExtEvent::Stopped { message, .. } = e else {
        unreachable!()
    };
    assert!(message.unwrap().contains("Reload Config to retry"));
    assert_eq!(read_lines(&env.storage().join("starts.log")).len(), 3);
    assert!(matches!(
        env.host.status()[0].state,
        ExtState::Failed { .. }
    ));
    assert_eq!(env.command("echo", None), Err(ExtError::NotRunning));

    // Reload with the setting fixed: it starts again.
    env.host.reload(config(env.dir.path(), &[], &[]));
    env.started();
    assert_eq!(env.host.status()[0].state, ExtState::Running);
}

#[test]
fn an_unanswered_initialize_or_a_newer_protocol_fails_the_start() {
    let env = setup_with(&[], &[], &["hang-init=1"], |c| {
        c.timing.initialize = Duration::from_millis(300);
        c.timing.max_failures = 1;
    });
    let e = env.wait_for("Failed", |e| matches!(e, ExtEvent::Failed { .. }));
    let ExtEvent::Failed { message, .. } = e else {
        unreachable!()
    };
    assert!(message.contains("no answer to initialize"), "{message}");

    let env = setup_with(&[], &[], &["protocol=99"], |c| c.timing.max_failures = 1);
    let e = env.wait_for("Failed", |e| matches!(e, ExtEvent::Failed { .. }));
    let ExtEvent::Failed { message, .. } = e else {
        unreachable!()
    };
    assert!(message.contains("protocol 99"), "{message}");
}

#[test]
fn shutdown_is_honoured_without_a_kill() {
    let env = setup(&[], &[], &[]);
    env.started();
    let t = Instant::now();
    env.host.shutdown();
    assert!(
        t.elapsed() < Duration::from_secs(3),
        "answered shutdown, wasn't killed after the grace"
    );
    env.wait_for("Stopped", |e| {
        matches!(e, ExtEvent::Stopped { message: None, .. })
    });
    assert!(env.host.status()[0].state != ExtState::Running);
}

#[test]
fn the_environment_is_scrubbed() {
    let env = setup(&[], &[], &[]);
    env.started();
    let keys: Vec<String> =
        serde_json::from_str(&env.command("env", None).unwrap().toast.unwrap()).unwrap();
    assert!(keys.iter().any(|k| k == "PATH"), "{keys:?}");
    for k in &keys {
        assert!(
            !k.starts_with("BLYGGER_") && !k.starts_with("CARGO"),
            "{k} leaked"
        );
        // macOS adds __CF_USER_TEXT_ENCODING to every process it starts.
        assert!(
            KEEP_ENV.contains(&k.as_str()) || k.starts_with("__CF"),
            "{k} isn't on the list"
        );
    }
}

#[test]
fn no_consent_no_start_then_reload_starts_it() {
    let env = setup(&["ui", "fs:~/Notes"], &[], &[]);
    let e = env.wait_for("NeedsConsent", |e| {
        matches!(e, ExtEvent::NeedsConsent { .. })
    });
    let ExtEvent::NeedsConsent { requested, .. } = e else {
        unreachable!()
    };
    assert_eq!(
        consent_sentence(NAME, &requested),
        "bxp-test wants to: show messages and select posts in the list · read and write files in \
         ~/Notes (its own promise; Burrow can't enforce this)"
    );
    assert_eq!(env.host.status()[0].state, ExtState::NeedsConsent);
    assert!(!env.storage().join("starts.log").exists(), "never started");

    env.host
        .reload(config(env.dir.path(), &["ui", "fs:~/Notes"], &[]));
    env.started();
}

#[test]
fn disabled_extensions_never_run_and_unknown_ones_are_missing() {
    let env = setup_with(&[], &[], &[], |c| c.enabled = vec!["not-installed".into()]);
    std::thread::sleep(Duration::from_millis(300));
    let st = env.host.status();
    let names: Vec<(&str, &ExtState)> = st.iter().map(|s| (s.name.as_str(), &s.state)).collect();
    assert_eq!(
        names,
        [
            (NAME, &ExtState::Disabled),
            ("not-installed", &ExtState::Missing)
        ]
    );
    assert!(!env.storage().join("starts.log").exists());
}

#[test]
fn hooks_reach_only_extensions_holding_them() {
    let publish = || PublishOutcome {
        version: 2,
        permalink: "https://blyg.example.com/f/X/".into(),
        warning: None,
    };
    let env = setup(&["hooks:itemPublished"], &["hooks:itemPublished"], &[]);
    env.started();
    let id = env
        .backend
        .create_draft(Kind::Fragment, "Published words")
        .unwrap();
    let item = env.backend.item(&id).unwrap();
    env.host.item_published(&item, &publish(), Some("a note"));
    env.host.item_saved(&item); // not granted
    let events = env.storage().join("events.jsonl");
    let deadline = Instant::now() + WAIT;
    while read_lines(&events).is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    // A round trip after the notifications, so anything else sent has arrived.
    env.command("echo", None).unwrap();
    let lines = read_lines(&events);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let v: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(v["method"], methods::ITEM_PUBLISHED);
    assert_eq!(v["params"]["permalink"], "https://blyg.example.com/f/X/");
    assert_eq!(v["params"]["contentMd"], "Published words");
    assert_eq!(v["params"]["note"], "a note");
    assert_eq!(v["params"]["item"]["id"], id.0.as_str());

    let env2 = setup(&["ui"], &["ui"], &[]);
    env2.started();
    env2.host.item_published(&item, &publish(), None);
    env2.command("echo", None).unwrap();
    assert!(read_lines(&env2.storage().join("events.jsonl")).is_empty());
}

#[test]
fn toasts_open_and_capability_requests_reach_the_ui() {
    let env = setup(&["ui", "items.read", "net"], &["ui"], &[]);
    env.started();
    let r = env.call_host(
        methods::TOAST,
        json!({"text": "Hello", "detail": "from a test"}),
    );
    assert_eq!(r["ok"], json!({}));
    let e = env.wait_for("Toast", |e| matches!(e, ExtEvent::Toast { .. }));
    let ExtEvent::Toast { text, detail, .. } = e else {
        unreachable!()
    };
    assert_eq!(
        (text.as_str(), detail.as_deref()),
        ("Hello", Some("from a test"))
    );
    env.call_host(methods::OPEN_ITEM, json!({"id": "L42"}));
    env.wait_for(
        "OpenItem",
        |e| matches!(e, ExtEvent::OpenItem { id, .. } if id.0 == "L42"),
    );

    // The UI answers yes on another thread while the extension waits.
    let host_events = std::thread::scope(|s| {
        let host = env.host.clone();
        let h = s.spawn(move || {
            call_host(
                &host,
                methods::REQUEST_CAPABILITY,
                json!({"capability": "items.read"}),
            )
        });
        let e = env.wait_for("CapabilityRequested", |e| {
            matches!(e, ExtEvent::CapabilityRequested { .. })
        });
        let ExtEvent::CapabilityRequested {
            capability, reply, ..
        } = e
        else {
            unreachable!()
        };
        assert_eq!(capability, Capability::ItemsRead);
        reply.answer(true);
        h.join().unwrap()
    });
    assert_eq!(host_events["ok"]["granted"], true);
    let r = env.call_host(methods::LIST_ITEMS, json!({}));
    assert!(r.get("ok").is_some(), "granted for the session: {r}");

    // Asking again for a refused one doesn't reopen the sheet.
    std::thread::scope(|s| {
        let host = env.host.clone();
        let h = s.spawn(move || {
            call_host(
                &host,
                methods::REQUEST_CAPABILITY,
                json!({"capability": "net"}),
            )
        });
        let e = env.wait_for("CapabilityRequested", |e| {
            matches!(e, ExtEvent::CapabilityRequested { .. })
        });
        let ExtEvent::CapabilityRequested { reply, .. } = e else {
            unreachable!()
        };
        reply.answer(false);
        assert_eq!(h.join().unwrap()["ok"]["granted"], false);
    });
    let r = env.call_host(methods::REQUEST_CAPABILITY, json!({"capability": "net"}));
    assert_eq!(r["ok"]["granted"], false);
    // Never one the manifest doesn't declare.
    let r = env.call_host(
        methods::REQUEST_CAPABILITY,
        json!({"capability": "items.write"}),
    );
    assert_eq!(r["ok"]["granted"], false);
}

#[test]
fn library_calls_map_stale_writes() {
    let env = setup(&[], &[], &[]);
    env.started();
    let doc = env
        .host
        .library_read(
            NAME,
            &LibraryReadParams {
                library: None,
                id: "a.md".into(),
            },
        )
        .unwrap();
    assert_eq!(doc.markdown, "hello");
    let w = LibraryWriteParams {
        library: None,
        id: Some("a.md".into()),
        title: None,
        folder: None,
        markdown: "x".into(),
        base_hash: Some("sha256:00".into()),
    };
    assert_eq!(
        env.host.library_write(NAME, &w),
        Err(ExtError::Stale {
            current_hash: "sha256:11".into()
        })
    );
}

#[test]
fn a_reload_without_its_extension_line_stops_it() {
    // Running: Burrow's Turn off removes `extension = <name>`, keeping its grants.
    let env = setup(&["ui"], &["ui"], &[]);
    env.started();
    let mut off = config(env.dir.path(), &["ui"], &[]);
    off.enabled.clear();
    env.host.reload(off);
    env.wait_for("Stopped", |e| {
        matches!(e, ExtEvent::Stopped { message: None, .. })
    });
    assert_eq!(env.host.status()[0].state, ExtState::Disabled);
    assert_eq!(env.command("echo", None), Err(ExtError::NotRunning));
    // Back on: its grants were kept, so it starts without asking.
    env.host.reload(config(env.dir.path(), &["ui"], &[]));
    env.started();

    // Waiting out a crash's backoff: it stops and isn't started again.
    let env = setup_with(&[], &[], &["crash-init=1"], |c| {
        c.timing.max_failures = 100;
        c.timing.backoff = vec![Duration::from_millis(400)];
    });
    env.wait_for("Failed", |e| matches!(e, ExtEvent::Failed { .. }));
    let mut off = config(env.dir.path(), &[], &["crash-init=1"]);
    off.enabled.clear();
    env.host.reload(off);
    env.wait_for("Stopped", |e| {
        matches!(e, ExtEvent::Stopped { message: None, .. })
    });
    let starts = read_lines(&env.storage().join("starts.log")).len();
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(read_lines(&env.storage().join("starts.log")).len(), starts);
    assert_eq!(env.host.status()[0].state, ExtState::Disabled);
}
