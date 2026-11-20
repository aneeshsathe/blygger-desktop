//! `scripts/verify/burrow-verify.sh selftest`: the verification script
//! still reads the app's demo scenes (from this build's `blygger +scenes`,
//! the registry in `src/scenes.rs`) and still refuses what would touch real
//! data: a release or installed build, overriding the data/config/token
//! variables, a config naming a real blyg. It starts no app.

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
