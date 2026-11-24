//! The one-line installer is how unnotarized Burrow builds get past
//! Gatekeeper, so every place that tells someone how to install Burrow gives
//! the same command: the README, the install page, and the release notes the
//! release workflow writes. The release notes used to say only "the one-line
//! Terminal installer in the README", and at the very bottom of long notes, so
//! readers missed it.

use std::path::PathBuf;

const COMMAND: &str = "curl -fsSL https://raw.githubusercontent.com/aneeshsathe/blygger-desktop/main/scripts/install.sh | bash";

fn read(rel: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn the_readme_and_install_page_give_the_installer() {
    for f in ["README.md", "site/src/install.md"] {
        assert!(read(f).contains(COMMAND), "{f} no longer gives `{COMMAND}`");
    }
}

/// The unsigned-build note carries the command itself, and goes at the top of
/// the notes (written before the changelog section is appended), so it's the
/// first thing a reader sees.
#[test]
fn the_release_notes_lead_with_the_installer() {
    let wf = read(".github/workflows/release.yml");
    let step = wf
        .split("- name: ")
        .find(|s| s.starts_with("Note an unsigned build"))
        .expect("the release workflow's unsigned-build step");
    assert!(
        step.contains(COMMAND),
        "the unsigned-build note must give the installer command itself"
    );
    assert!(
        step.contains("cat \"$RUNNER_TEMP/notes.md\""),
        "the unsigned-build note must be put before the existing notes, not appended after them"
    );
}
