//! `scripts/changelog-stitch.py`: changes land as fragments in
//! `changes/unreleased/`, and the release stitches them into a new
//! CHANGELOG.md section. Run on throwaway folders, plus `check` on the
//! repo's own fragments, so a malformed one fails here and not at release.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn script() -> PathBuf {
    root().join("scripts/changelog-stitch.py")
}

const CHANGELOG: &str = "# Changelog

Intro text.

## [Unreleased]

Pending changes are fragments in `changes/unreleased/`.

## [0.1.0] - 2026-01-01

### Added
- The first thing.
";

struct Dir(tempfile::TempDir);

impl Dir {
    fn new() -> Dir {
        let d = Dir(tempfile::tempdir().unwrap());
        d.write("CHANGELOG.md", CHANGELOG);
        d.write("changes/unreleased/README.md", "The format.\n");
        d
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn write(&self, file: &str, text: &str) {
        let p = self.path().join(file);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn read(&self, file: &str) -> String {
        std::fs::read_to_string(self.path().join(file)).unwrap()
    }

    fn exists(&self, file: &str) -> bool {
        self.path().join(file).exists()
    }

    fn fragment(&self, slug: &str, text: &str) {
        self.write(&format!("changes/unreleased/{slug}.md"), text);
    }

    fn run(&self, args: &[&str]) -> (bool, String) {
        let out = Command::new("python3")
            .arg(script())
            .arg("--root")
            .arg(self.path())
            .args(args)
            .output()
            .expect("python3 runs");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.success(), text)
    }

    fn stitch(&self) -> (bool, String) {
        self.run(&["stitch", "0.2.0", "--date", "2026-02-03"])
    }
}

#[test]
fn stitch_writes_a_section_in_order_and_deletes_the_fragments() {
    let d = Dir::new();
    d.fragment("zebra", "section: Fixed\n\n- **Sync:** a fix.\n");
    d.fragment("beta", "section: Added\n\n- **Reading:** beta.\n");
    d.fragment(
        "alpha",
        "section: Added\n\n- **Writing:** alpha,\n  over two lines.\n- A second one.\n\n",
    );
    d.fragment(
        "tools",
        "section: Development\n\n- `scripts/x.sh` does a thing.\n",
    );
    let (ok, out) = d.stitch();
    assert!(ok, "{out}");
    let want = "# Changelog

Intro text.

## [Unreleased]

Pending changes are fragments in `changes/unreleased/`.

## [0.2.0] - 2026-02-03

### Added
- **Writing:** alpha,
  over two lines.
- A second one.
- **Reading:** beta.

### Fixed
- **Sync:** a fix.

### Development
- `scripts/x.sh` does a thing.

## [0.1.0] - 2026-01-01

### Added
- The first thing.
";
    assert_eq!(d.read("CHANGELOG.md"), want);
    for slug in ["alpha", "beta", "zebra", "tools"] {
        assert!(
            !d.exists(&format!("changes/unreleased/{slug}.md")),
            "{slug}"
        );
    }
    assert!(d.exists("changes/unreleased/README.md"), "the README stays");
}

#[test]
fn dry_run_prints_the_section_and_changes_nothing() {
    let d = Dir::new();
    d.fragment("one", "section: Changed\n\n- One.\n");
    let (ok, out) = d.run(&["stitch", "0.2.0", "--date", "2026-02-03", "--dry-run"]);
    assert!(ok, "{out}");
    assert_eq!(out, "## [0.2.0] - 2026-02-03\n\n### Changed\n- One.\n");
    assert_eq!(d.read("CHANGELOG.md"), CHANGELOG);
    assert!(d.exists("changes/unreleased/one.md"));
}

/// Each refusal leaves the changelog and every fragment as they were.
fn refuses(d: &Dir, args: &[&str], why: &str) {
    let before: Vec<_> = std::fs::read_dir(d.path().join("changes/unreleased"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    let (ok, out) = d.run(args);
    assert!(!ok, "should refuse: {out}");
    assert!(out.contains(why), "wanted {why:?} in: {out}");
    let after: Vec<_> = std::fs::read_dir(d.path().join("changes/unreleased"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(before.len(), after.len(), "a fragment went missing: {out}");
}

const STITCH: &[&str] = &["stitch", "0.2.0", "--date", "2026-02-03"];

#[test]
fn an_unknown_section_is_refused() {
    let d = Dir::new();
    d.fragment("good", "section: Added\n\n- Fine.\n");
    d.fragment("bad", "section: Improvements\n\n- Not a section.\n");
    refuses(&d, STITCH, "bad.md: unknown section 'Improvements'");
    refuses(&d, &["check"], "bad.md: unknown section");
    assert_eq!(d.read("CHANGELOG.md"), CHANGELOG);
}

#[test]
fn a_fragment_without_its_header_or_list_is_refused() {
    let cases = [
        ("- No header.\n", "first line must be 'section: <name>'"),
        (
            "section: Added\n- No blank line.\n",
            "second line must be blank",
        ),
        ("section: Added\n\n", "no entry"),
        (
            "section: Added\n\nProse, not a list.\n",
            "must start with '- '",
        ),
        ("section: Added\n\n- One.\n\n### Fixed\n- Two.\n", "heading"),
    ];
    for (text, why) in cases {
        let d = Dir::new();
        d.fragment("f", text);
        refuses(&d, STITCH, why);
        assert_eq!(d.read("CHANGELOG.md"), CHANGELOG, "{text:?}");
    }
}

#[test]
fn a_stray_file_is_refused() {
    let d = Dir::new();
    d.fragment("good", "section: Added\n\n- Fine.\n");
    d.write("changes/unreleased/notes.txt", "x\n");
    refuses(&d, STITCH, "notes.txt: not a fragment");
}

#[test]
fn no_fragments_is_refused() {
    let d = Dir::new();
    refuses(&d, STITCH, "no fragments");
    // `check` is fine with none: between releases there may be nothing yet.
    let (ok, out) = d.run(&["check"]);
    assert!(ok, "{out}");
}

#[test]
fn a_version_already_there_is_refused() {
    let d = Dir::new();
    d.fragment("one", "section: Added\n\n- One.\n");
    refuses(
        &d,
        &["stitch", "0.1.0", "--date", "2026-02-03"],
        "already has a section for [0.1.0]",
    );
    refuses(&d, &["stitch", "v0.2", "--date", "2026-02-03"], "version");
}

#[test]
fn entries_written_under_unreleased_are_refused() {
    let d = Dir::new();
    d.write(
        "CHANGELOG.md",
        &CHANGELOG.replace(
            "Pending changes are fragments in `changes/unreleased/`.\n",
            "### Added\n- Written by hand.\n",
        ),
    );
    d.fragment("one", "section: Added\n\n- One.\n");
    refuses(&d, STITCH, "[Unreleased] has entries");
}

/// `check` (run on every PR through the test below) refuses an entry
/// written into CHANGELOG.md by hand, with or without fragments; a prose
/// note under [Unreleased] stays allowed (`Dir::new`'s, checked in
/// `no_fragments_is_refused`).
#[test]
fn check_refuses_entries_under_unreleased() {
    for line in ["- Written by hand.\n", "### Added\n- Written by hand.\n"] {
        let d = Dir::new();
        d.write(
            "CHANGELOG.md",
            &CHANGELOG.replace(
                "Pending changes are fragments in `changes/unreleased/`.\n",
                line,
            ),
        );
        refuses(&d, &["check"], "[Unreleased] has entries");
        d.fragment("one", "section: Added\n\n- One.\n");
        refuses(&d, &["check"], "[Unreleased] has entries");
    }
}

/// The repo's own fragments are well-formed, and CHANGELOG.md has no
/// entries under [Unreleased], so the release stitch won't stop on either.
#[test]
fn the_repos_fragments_pass_check() {
    let out = Command::new("python3")
        .arg(script())
        .arg("check")
        .output()
        .expect("python3 runs");
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
