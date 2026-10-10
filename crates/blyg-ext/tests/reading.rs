//! Reading slots over the wire: the host against a real extension process
//! (`bxp-slots-ext`) that echoes what it was sent. `extension/entry.byline`
//! and `extension/entry.action` carry the entry (camelCase) and the record
//! the client holds (snake_case), only to an extension granted
//! `reading.read`, and the answers come back as the host's types.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use blyg_core::ReadingItem;
use blyg_ext::protocol::codes;
use blyg_ext::reading::{EntryActionSpec, EntrySlots, ReadingEntry};
use blyg_ext::*;
use serde_json::{Value, json};

const NAME: &str = "bxp-slots";
const WAIT: Duration = Duration::from_secs(15);

fn manifest() -> String {
    let exe = env!("CARGO_BIN_EXE_bxp-slots-ext");
    format!(
        r#"name = "{NAME}"
version = "0.0.1"
protocol = 1
command = ['{exe}']
capabilities = ["reading.read", "ui"]
entry-byline = true

[[entry-actions]]
id = "echo"
title = "echo"
detail = "what the host sent"
icon = "◇"

[[entry-actions]]
id = "fail"
title = "fail"
"#
    )
}

struct Env {
    _dir: tempfile::TempDir,
    host: Host,
    events: Receiver<ExtEvent>,
}

fn setup(grants: &[&str], tweak: impl FnOnce(&mut HostConfig)) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let ext_dir = dir.path().join("extensions").join(NAME);
    std::fs::create_dir_all(&ext_dir).unwrap();
    std::fs::write(ext_dir.join(MANIFEST_FILE), manifest()).unwrap();
    let mut c = HostConfig::new(dir.path().join("data"), "0.0.0-test");
    c.enabled = vec![NAME.into()];
    let lines: Vec<String> = grants.iter().map(|g| format!("{NAME} {g}")).collect();
    c.grants = Grants::from_allow_lines(&lines).0;
    c.extensions_dir = Some(dir.path().join("extensions"));
    c.timing.shutdown = Duration::from_secs(5);
    tweak(&mut c);
    let (tx, events) = channel();
    let host = Host::new(c, Arc::new(NoBlyg), move |e| {
        let _ = tx.send(e);
    });
    host.start();
    Env {
        _dir: dir,
        host,
        events,
    }
}

impl Env {
    fn started(&self) {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(ExtEvent::Started { .. }) => return,
                Ok(_) => continue,
                Err(_) => panic!("not started within {WAIT:?}"),
            }
        }
    }
}

/// An invented reading row.
fn item(md: &str, version: u32) -> ReadingItem {
    serde_json::from_value(json!({
        "subscription_id": "sub-1", "remote_id": "r1", "subscription_title": "Rue",
        "origin": "https://blyg.example.com", "kind": "thread", "state": "current",
        "version": version, "created": "2026-10-01T09:00:00Z", "updated": null,
        "observed_at": "2026-10-01T09:05:00Z",
        "content_md": md, "content_html": format!("<p>{md}</p>"),
        "author": {"name": "Rue", "url": null}, "page": null, "thumb": null, "hoppers": [],
        "stub_of": {"origin": "https://other.example.com", "id": "p7", "version": 2},
    }))
    .unwrap()
}

#[test]
fn slots_reach_an_extension_granted_reading_read() {
    let env = setup(&["reading.read", "ui"], |_| {});
    env.started();
    assert_eq!(
        env.host.entry_slots(),
        [EntrySlots {
            ext: NAME.into(),
            byline: true,
            actions: vec![
                EntryActionSpec {
                    id: "echo".into(),
                    title: "echo".into(),
                    detail: "what the host sent".into(),
                    icon: Some("◇".into()),
                },
                EntryActionSpec {
                    id: "fail".into(),
                    title: "fail".into(),
                    detail: String::new(),
                    icon: None,
                },
            ],
        }]
    );

    // The byline: the entry crossed as camelCase (version, ids, hash).
    let r = item("The harbour bench floods.", 3);
    let b = env
        .host
        .entry_byline(NAME, &ReadingEntry::of(&r))
        .unwrap()
        .unwrap();
    assert_eq!(b.text, "v3 r1");
    assert_eq!(
        b.tip.as_deref(),
        Some(blyg_core::content_hash("The harbour bench floods.").as_str())
    );
    // `null` is nothing to show; an error is the extension's.
    let silent = ReadingEntry::of(&item("silent", 1));
    assert_eq!(env.host.entry_byline(NAME, &silent).unwrap(), None);
    match env
        .host
        .entry_byline(NAME, &ReadingEntry::of(&item("broken", 1)))
    {
        Err(ExtError::Rpc(e)) => assert_eq!(e.message, "broken entry"),
        other => panic!("{other:?}"),
    }

    // A ⋯ row: the record went as Burrow stores it (snake_case, bodies in).
    let sheet = env.host.entry_action(NAME, "echo", &r).unwrap();
    assert_eq!(sheet.title, "echo");
    assert_eq!(sheet.text, "sub-1 r1");
    let keys: Vec<&str> = sheet.fields[0].value.split(',').collect();
    assert!(keys.contains(&"subscription_id") && keys.contains(&"content_html"));
    let record: Value = serde_json::from_str(sheet.code.as_deref().unwrap()).unwrap();
    assert_eq!(record["content_md"], "The harbour bench floods.");
    assert_eq!(record["stub_of"]["id"], "p7");
    assert_eq!(record["version"], 3);
    assert_eq!(sheet.copy_label(), "Copy JSON");
    match env.host.entry_action(NAME, "fail", &r) {
        Err(ExtError::Rpc(e)) => assert_eq!(e.message, "it didn't work"),
        other => panic!("{other:?}"),
    }
    // A row the manifest doesn't declare is never sent.
    match env.host.entry_action(NAME, "nope", &r) {
        Err(ExtError::Rpc(e)) => assert_eq!(e.code, codes::METHOD_NOT_FOUND),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        env.host
            .entry_byline("not-running", &ReadingEntry::of(&r))
            .unwrap_err(),
        ExtError::NotRunning
    );
    env.host.shutdown();
}

#[test]
fn without_reading_read_there_are_no_slots_and_no_calls() {
    let env = setup(&["ui"], |_| {});
    env.started();
    assert!(env.host.entry_slots().is_empty(), "nothing to draw");
    let r = item("The harbour bench floods.", 1);
    for e in [
        env.host
            .entry_byline(NAME, &ReadingEntry::of(&r))
            .unwrap_err(),
        env.host.entry_action(NAME, "echo", &r).unwrap_err(),
    ] {
        match e {
            ExtError::Rpc(e) => {
                assert_eq!(e.code, codes::PERMISSION_DENIED);
                assert_eq!(e.data.unwrap()["capability"], "reading.read");
            }
            other => panic!("{other:?}"),
        }
    }
    env.host.shutdown();
}

#[test]
fn a_slow_byline_times_out_and_the_next_one_answers() {
    let env = setup(&["reading.read"], |c| {
        c.timing.search = Duration::from_millis(400);
        c.timing.timeouts_before_restart = 100;
    });
    env.started();
    let slow = ReadingEntry::of(&item("slow", 1));
    assert_eq!(
        env.host.entry_byline(NAME, &slow).unwrap_err(),
        ExtError::Timeout
    );
    // The extension answers one request at a time: the next waits out
    // the nap, then answers.
    let t = Instant::now();
    let mut ok = None;
    while t.elapsed() < WAIT {
        match env
            .host
            .entry_byline(NAME, &ReadingEntry::of(&item("quick", 2)))
        {
            Ok(b) => {
                ok = b;
                break;
            }
            Err(ExtError::Timeout) => continue,
            Err(e) => panic!("{e:?}"),
        }
    }
    assert_eq!(ok.map(|b| b.text).as_deref(), Some("v2 r1"));
    env.host.shutdown();
}
