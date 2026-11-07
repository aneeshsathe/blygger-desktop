//! A real pty: a trivial command's output comes back, and the child's
//! environment is scrubbed. No network; only processes these tests start.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use blyg_term::pty::{Output, Pty, ShellSpec};

/// Run `spec` to its end and return what it printed.
fn run(spec: &ShellSpec) -> String {
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let _pty = Pty::spawn(spec, 120, 40, move |o| {
        let _ = tx.lock().unwrap().send(o);
    })
    .expect("spawn");
    let mut out = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Output::Bytes(b)) => out.extend(b),
            Ok(Output::Exited) => {
                // Drain what the reader still holds.
                while let Ok(Output::Bytes(b)) = rx.recv_timeout(Duration::from_millis(300)) {
                    out.extend(b);
                }
                break;
            }
            Err(_) => {}
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(unix)]
fn echo(text: &str) -> ShellSpec {
    ShellSpec::command("/bin/echo", &[text])
}

#[cfg(windows)]
fn echo(text: &str) -> ShellSpec {
    ShellSpec::command("cmd.exe", &["/c", "echo", text])
}

#[test]
fn a_command_runs_on_the_pty_and_its_output_comes_back() {
    let out = run(&echo("burrow-pty-ok"));
    assert!(out.contains("burrow-pty-ok"), "{out:?}");
}

#[test]
fn output_feeds_the_engine() {
    let out = run(&echo("cells-from-a-pty"));
    let mut t = blyg_term::new_engine(80, 5);
    t.feed(out.as_bytes());
    let lines = t.snapshot().lines();
    assert!(
        lines.iter().any(|l| l.contains("cells-from-a-pty")),
        "{lines:?}"
    );
}

#[cfg(unix)]
#[test]
fn the_child_never_sees_burrows_variables() {
    let mut spec = ShellSpec::command("/usr/bin/env", &[]);
    // As if Burrow had them: the scrub runs over these.
    spec.env = blyg_term::env::scrub(
        [
            ("PATH", "/usr/bin:/bin"),
            ("BLYGGER_TEST_TOKEN", "tok-not-for-the-pane"),
            ("BLYGGER_DATA_DIR", "/nowhere"),
            ("GPUI_DISABLE_DIRECT_COMPOSITION", "1"),
            ("KEEP_ME", "yes"),
        ]
        .map(|(k, v)| (k.into(), v.into())),
    );
    let out = run(&spec);
    assert!(out.contains("KEEP_ME=yes"), "{out:?}");
    assert!(out.contains("TERM=xterm-256color"), "{out:?}");
    assert!(!out.contains("BLYGGER_"), "{out:?}");
    assert!(!out.contains("tok-not-for-the-pane"), "{out:?}");
    assert!(!out.contains("GPUI_"), "{out:?}");
}

#[cfg(windows)]
#[test]
fn the_child_never_sees_burrows_variables() {
    let mut spec = ShellSpec::command("cmd.exe", &["/c", "set"]);
    let mut vars: Vec<(std::ffi::OsString, std::ffi::OsString)> = std::env::vars_os()
        .filter(|(k, _)| !blyg_term::env::dropped(k))
        .collect();
    vars.push(("BLYGGER_TEST_TOKEN".into(), "tok-not-for-the-pane".into()));
    vars.push(("KEEP_ME".into(), "yes".into()));
    spec.env = blyg_term::env::scrub(vars);
    let out = run(&spec);
    assert!(out.contains("KEEP_ME=yes"), "{out:?}");
    assert!(!out.contains("BLYGGER_"), "{out:?}");
    assert!(!out.contains("tok-not-for-the-pane"), "{out:?}");
}

#[test]
fn the_login_shell_spec_is_scrubbed_too() {
    let spec = ShellSpec::login(None);
    assert!(!spec.program.is_empty());
    assert!(spec.env.iter().all(|(k, _)| {
        !k.to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("BLYGGER_")
    }));
    let o = ShellSpec::login(Some("/bin/sh -i"));
    assert_eq!(
        (o.program.as_str(), o.args.as_slice()),
        ("/bin/sh", &["-i".to_string()][..])
    );
    assert_eq!(o.name(), "sh");
    assert_eq!(
        ShellSpec::command("C:\\Windows\\System32\\cmd.exe", &[]).name(),
        "cmd"
    );
}
