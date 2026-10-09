//! The `blygger` executable's extension actions, as real processes over a
//! real temp config: `+ext markdown-notes` served through the real host,
//! `+ext` with a wrong name, and `+list-extensions`.

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
        err.contains("`no-such-thing` isn't a bundled extension (bundled: markdown-notes)"),
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
    assert!(out.contains("\nProblems:\n"), "{out}");
    assert!(out.contains("broken"), "{out}");
    assert!(
        out.contains("Install an extension by copying its folder into"),
        "{out}"
    );
    assert!(!dir.path().join("data").exists(), "listing starts nothing");
}
