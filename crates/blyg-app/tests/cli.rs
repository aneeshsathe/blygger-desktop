//! The `blygger` executable's extension actions, as real processes over a
//! real temp config: `+ext markdown-notes` and `+ext cross-post` served
//! through the real host and over raw stdio, `+ext` with a wrong name, and
//! `+list-extensions`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use blyg_ext::protocol::*;
use blyg_ext::*;

const BLYGGER: &str = env!("CARGO_BIN_EXE_blygger");

/// `blygger <args>` with only this temp dir's config and data.
fn blygger(dir: &Path, args: &[&str]) -> Output {
    Command::new(BLYGGER)
        .args(args)
        .env("BLYGGER_CONFIG", dir.join("config"))
        .env("BLYGGER_DATA_DIR", dir.join("data"))
        .env_remove("BLYGGER_EXTENSIONS_DIR")
        .env_remove("BLYGGER_FAKE")
        .output()
        .expect("run blygger")
}

fn write(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

#[test]
fn ext_serves_markdown_notes_through_the_host() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("Vault");
    write(&vault.join("Tide tables.md"), "The moon pulls the sea.\n");
    let vault_str = vault.to_string_lossy().into_owned();
    let name = blyg_ext_notes::NAME;
    let settings: BTreeMap<String, String> = [("vault".to_string(), vault_str.clone())].into();

    let mut c = HostConfig::new(dir.path().join("data"), "0.0.0-test");
    c.bundled = vec![blyg_ext_notes::bundled(
        PathBuf::from(BLYGGER),
        vec!["+ext".into(), name.into()],
        &settings,
    )];
    c.enabled = vec![name.into()];
    c.grants =
        Grants::from_allow_lines(&[format!("{name} ui"), format!("{name} fs:{vault_str}")]).0;
    c.settings = [(name.to_string(), settings)].into();
    let (tx, events) = channel();
    let host = Host::new(c, Arc::new(NoBlyg), move |e| {
        let _ = tx.send(e);
    });
    host.start();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(ExtEvent::Started { name: n, .. }) => {
                assert_eq!(n, name);
                break;
            }
            Ok(ExtEvent::Failed {
                message, stderr, ..
            }) => panic!("{message}: {stderr:?}"),
            Ok(_) => {}
            Err(_) => panic!("blygger +ext {name} didn't start"),
        }
    }

    let listed = host
        .library_list(
            name,
            &LibraryListParams {
                library: None,
                path: None,
            },
        )
        .unwrap();
    assert_eq!(
        listed.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["Tide tables.md"]
    );
    let written = host
        .library_write(
            name,
            &LibraryWriteParams {
                library: None,
                id: None,
                title: Some("From the CLI".into()),
                folder: None,
                markdown: "Written through blygger +ext.\n".into(),
                base_hash: None,
            },
        )
        .unwrap();
    let doc = host
        .library_read(
            name,
            &LibraryReadParams {
                library: None,
                id: written.id.clone(),
            },
        )
        .unwrap();
    assert!(doc.markdown.contains("Written through blygger +ext."));
    assert!(vault.join(&written.id).is_file(), "{}", written.id);
    host.shutdown();
}

#[test]
fn ext_with_an_unknown_name_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let o = blygger(dir.path(), &["+ext", "no-such-thing"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(o.stdout.is_empty(), "stdout stays clean");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains(
            "`no-such-thing` isn't a bundled extension \
             (bundled: markdown-notes, cross-post, reading-time, inspect)"
        ),
        "{err}"
    );
    let o = blygger(dir.path(), &["+ext"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("usage: blygger +ext <name>"));
}

#[test]
fn ext_answers_initialize_and_shutdown_on_stdio() {
    use std::io::{BufRead, BufReader, Write};
    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(BLYGGER)
        .args(["+ext", blyg_ext_notes::NAME])
        .env("BLYGGER_CONFIG", dir.path().join("config"))
        .env("BLYGGER_DATA_DIR", dir.path().join("data"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let params = InitializeParams {
        protocol_version: PROTOCOL_VERSION,
        host: HostInfo {
            name: "Burrow".into(),
            version: "0.0.0-test".into(),
            platform: std::env::consts::OS.into(),
        },
        granted: vec![],
        settings: [(
            "vault".to_string(),
            dir.path().join("Vault").to_string_lossy().into_owned(),
        )]
        .into(),
        storage_dir: dir.path().join("data"),
        blyg_origin: None,
    };
    let init = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": params,
    });
    writeln!(stdin, "{init}").unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).expect("one JSON line, nothing else");
    assert_eq!(v["id"], 1, "{line}");
    assert_eq!(v["result"]["name"], blyg_ext_notes::NAME, "{line}");
    assert_eq!(v["result"]["protocolVersion"], PROTOCOL_VERSION, "{line}");
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"shutdown"}}"#).unwrap();
    drop(stdin);
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("blygger +ext didn't exit after shutdown");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success(), "{status:?}");
}

#[test]
fn list_extensions_shows_bundled_installed_and_broken() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("config"),
        "extension = markdown-notes\n\
         extension-allow = markdown-notes ui\n\
         extension = hello\n\
         extension = ghost\n",
    );
    write(
        &dir.path().join("extensions/hello/extension.toml"),
        "name = \"hello\"\nversion = \"0.2.0\"\nprotocol = 1\n\
         description = \"Says hello.\"\ncommand = [\"hello\"]\ncapabilities = [\"ui\"]\n",
    );
    write(
        &dir.path().join("extensions/broken/extension.toml"),
        "name = \"Broken Name\"\n",
    );
    // The last run of each macro, from its log.
    write(
        &dir.path().join("data/extensions/cross-post/macro.log"),
        "2026-10-01T09:00:00Z cross-post-note error step=2 do=waitFor reason=timeout selector=\"div\" after-submit=false\n\
         2026-10-02T09:00:00Z cross-post-note posted via=paste \"Posted to Substack Notes\"\n\
         2026-10-03T09:00:00Z other-macro cancelled after-submit=false\n",
    );
    let o = blygger(dir.path(), &["+list-extensions"]);
    assert_eq!(o.status.code(), Some(0));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        out.contains(
            "markdown-notes 0.1.0 (bundled): enabled; not everything it asks for is granted\n"
        ),
        "{out}"
    );
    assert!(out.contains("  asks for: fs:~/Notes, ui\n"), "{out}");
    assert!(
        out.contains("  granted:  ui\n  missing:  fs:~/Notes\n"),
        "{out}"
    );
    assert!(
        out.contains("hello 0.2.0 (installed): enabled; Burrow asks for consent when it starts it\n  Says hello.\n"),
        "{out}"
    );
    assert!(
        out.contains("ghost (not installed): enabled, but not installed\n"),
        "{out}"
    );
    assert!(
        out.contains(
            "cross-post 0.1.0 (bundled): off\n  \
             Cross-posts a published post to Substack Notes, in the browser pane, after you confirm.\n  \
             asks for: items.read, ui, browser.automate:https://substack.com\n  \
             site:     substack-notes (Substack Notes, https://substack.com)\n  \
             macro:    cross-post-note \"Cross-post to Substack Notes…\" on substack-notes, tested: 2026-10-09\n            \
             last run: 2026-10-02T09:00:00Z posted via=paste \"Posted to Substack Notes\"\n  \
             turn on:  extension = cross-post\n"
        ),
        "{out}"
    );
    assert!(out.contains("\nProblems:\n"), "{out}");
    assert!(out.contains("broken"), "{out}");
    assert!(
        out.contains("Install an extension by copying its folder into"),
        "{out}"
    );
    // Listing starts nothing: the data folder holds only the log above.
    let names = |p: &Path| -> Vec<String> {
        std::fs::read_dir(p)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    };
    assert_eq!(names(&dir.path().join("data")), ["extensions"]);
    assert_eq!(names(&dir.path().join("data/extensions")), ["cross-post"]);
    assert_eq!(
        names(&dir.path().join("data/extensions/cross-post")),
        ["macro.log"]
    );
}

/// One JSON line from `stdout`, which must hold nothing else.
fn read_json(stdout: &mut impl std::io::BufRead) -> serde_json::Value {
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line:?}"))
}

fn sample_item() -> ItemSummary {
    ItemSummary {
        id: "local-7".into(),
        server_id: Some("p7".into()),
        kind: "post".into(),
        status: "public".into(),
        version: 3,
        dirty: false,
        created: "2026-10-01T00:00:00Z".into(),
        updated: "2026-10-02T00:00:00Z".into(),
        permalink: Some("https://blyg.example.com/p/7".into()),
        title: "Tide tables".into(),
        content_hash: "sha256:00".into(),
        content_md: Some(
            "# Tide tables\r\n\r\n> Time and tide.\r\n\r\n![[img01]]\r\n\r\n\
             The **moon** pulls the sea 🌊\r\ntwice a day.\r\n\r\nMore.\r\n"
                .into(),
        ),
    }
}

fn prepare_params() -> MacroPrepareParams {
    MacroPrepareParams {
        macro_id: blyg_ext_crosspost::MACRO.into(),
        item: sample_item(),
        permalink: "https://blyg.example.com/p/7".into(),
        text: "The host's own text".into(),
    }
}

#[test]
fn ext_cross_post_prepares_a_note_on_stdio() {
    use std::io::{BufReader, Write};
    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(BLYGGER)
        .args(["+ext", blyg_ext_crosspost::NAME])
        .env("BLYGGER_CONFIG", dir.path().join("config"))
        .env("BLYGGER_DATA_DIR", dir.path().join("data"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let params = InitializeParams {
        protocol_version: PROTOCOL_VERSION,
        host: HostInfo {
            name: "Burrow".into(),
            version: "0.0.0-test".into(),
            platform: std::env::consts::OS.into(),
        },
        granted: vec![
            "items.read".into(),
            "browser.automate:https://substack.com".into(),
        ],
        settings: [("max-chars".to_string(), "60".to_string())].into(),
        storage_dir: dir.path().join("data"),
        blyg_origin: None,
    };
    let send = |stdin: &mut std::process::ChildStdin, v: serde_json::Value| {
        writeln!(stdin, "{v}").unwrap();
    };
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": params}),
    );
    let v = read_json(&mut stdout);
    assert_eq!(v["id"], 1, "{v}");
    assert_eq!(v["result"]["name"], blyg_ext_crosspost::NAME, "{v}");
    assert_eq!(v["result"]["protocolVersion"], PROTOCOL_VERSION, "{v}");

    send(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": methods::MACRO_PREPARE,
            "params": prepare_params(),
        }),
    );
    let v = read_json(&mut stdout);
    assert_eq!(v["id"], 2, "{v}");
    let r: MacroPrepareResult = serde_json::from_value(v["result"].clone()).expect("a result");
    // 60 characters in all: the link (28) and the blank line (2) leave 30.
    assert_eq!(
        r.text,
        "The moon pulls the sea 🌊…\n\nhttps://blyg.example.com/p/7"
    );
    assert_eq!(r.note.as_deref(), Some("Cut to 60 characters (max-chars)"));

    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "extension/command",
                           "params": {"id": "nope", "context": {}}}),
    );
    let v = read_json(&mut stdout);
    assert_eq!(v["error"]["code"], codes::METHOD_NOT_FOUND, "{v}");

    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 4, "method": "shutdown"}),
    );
    let v = read_json(&mut stdout);
    assert_eq!(v["id"], 4, "{v}");
    drop(stdin);
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("blygger +ext cross-post didn't exit after shutdown");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success(), "{status:?}");
}

#[test]
fn ext_cross_post_runs_through_the_host() {
    let dir = tempfile::tempdir().unwrap();
    let name = blyg_ext_crosspost::NAME;
    let mut c = HostConfig::new(dir.path().join("data"), "0.0.0-test");
    c.bundled = vec![blyg_ext_crosspost::bundled(
        PathBuf::from(BLYGGER),
        vec!["+ext".into(), name.into()],
    )];
    c.enabled = vec![name.into()];
    c.grants = Grants::from_allow_lines(&[
        format!("{name} items.read"),
        format!("{name} ui"),
        format!("{name} browser.automate:https://substack.com"),
    ])
    .0;
    let (tx, events) = channel();
    let host = Host::new(c, Arc::new(NoBlyg), move |e| {
        let _ = tx.send(e);
    });
    host.start();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(ExtEvent::Started { name: n, .. }) => {
                assert_eq!(n, name);
                break;
            }
            Ok(ExtEvent::Failed {
                message, stderr, ..
            }) => panic!("{message}: {stderr:?}"),
            Ok(_) => {}
            Err(_) => panic!("blygger +ext {name} didn't start"),
        }
    }
    let entry = host
        .macro_entry(name, blyg_ext_crosspost::MACRO)
        .expect("the granted macro is offered");
    assert_eq!(entry.site.origin, "https://substack.com");
    assert_eq!(entry.spec.tested, "2026-10-09");
    let r = host
        .macro_prepare(name, &prepare_params())
        .unwrap()
        .expect("cross-post implements macro.prepare");
    assert_eq!(
        r.text,
        "The moon pulls the sea 🌊 twice a day.\n\nhttps://blyg.example.com/p/7"
    );
    assert_eq!(r.note, None);
    host.shutdown();
}
