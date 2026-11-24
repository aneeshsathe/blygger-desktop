//! `scripts/verify/burrow-verify.sh selftest`: the verification script
//! still reads the app's demo scenes (from this build's `blygger +scenes`,
//! the registry in `src/scenes.rs`) and still refuses what would touch real
//! data: a release or installed build, overriding the data/config/token
//! variables, a config naming a real blyg. It starts no app.

#![cfg_attr(
    target_os = "macos",
    expect(clippy::disallowed_types, reason = "tests run the verification script")
)]

#[cfg(target_os = "macos")]
#[test]
fn the_verify_script_passes_its_selftest() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = std::process::Command::new("bash")
        .arg(root.join("scripts/verify/burrow-verify.sh"))
        .arg("selftest")
        .env("BLYGGER_BIN", env!("CARGO_BIN_EXE_blygger"))
        .output()
        .expect("run bash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("rd-stream among them"), "{stdout}");
}

/// The 0.28 user-testing pass took its shots while the Mac was locked and
/// read nothing wrong into them. The script refuses a locked console
/// (`IOConsoleLocked` in `ioreg -n Root -d1 -a`) and fails a shot that is
/// one flat colour; its selftest feeds each check both answers.
#[cfg(target_os = "macos")]
#[test]
fn the_verify_script_refuses_a_locked_screen_and_a_blank_shot() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = std::process::Command::new("bash")
        .arg(root.join("scripts/verify/burrow-verify.sh"))
        .arg("selftest")
        .env("BLYGGER_BIN", env!("CARGO_BIN_EXE_blygger"))
        .output()
        .expect("run bash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in [
        "ok: refused a locked console",
        "ok: allowed an unlocked console",
        "ok: refused a blank shot",
        "ok: allowed a drawn shot",
    ] {
        assert!(
            stdout.contains(line),
            "missing '{line}':\n{stdout}{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
