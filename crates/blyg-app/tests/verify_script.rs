//! `scripts/verify/burrow-verify.sh selftest`: the verification script
//! still finds the app's demo scenes (it greps them from this crate's
//! source) and still refuses what would touch real data: a release or
//! installed build, overriding the data/config/token variables, a config
//! naming a real blyg. It starts no app.

#[cfg(target_os = "macos")]
#[test]
fn the_verify_script_passes_its_selftest() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = std::process::Command::new("bash")
        .arg(root.join("scripts/verify/burrow-verify.sh"))
        .arg("selftest")
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
