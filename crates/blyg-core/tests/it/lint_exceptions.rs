//! The files that lift a reviewed ban, in one list: unsafe code
//! (`expect(unsafe_code`) and process spawning
//! (`expect(clippy::disallowed_types`, the `std::process::Command` ban),
//! inner or outer, plain or under `cfg_attr`. A new exception is then a
//! change to `tests/fixtures/lint-exceptions.txt`, which a reviewer sees,
//! rather than one attribute somewhere in a diff.

use std::collections::BTreeMap;
use std::path::Path;

/// The bans a file can lift, by the name the list uses.
const BANS: &[(&str, &str)] = &[
    ("unsafe_code", "expect(unsafe_code"),
    ("disallowed_types", "expect(clippy::disallowed_types"),
];

fn crates_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

/// Every `.rs` file under `dir`, relative to `crates/`.
fn rust_files(dir: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            let rel = p.strip_prefix(crates_dir()).unwrap();
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Each file that lifts a ban, and which (`unsafe_code,disallowed_types`).
fn found() -> BTreeMap<String, String> {
    // Workspace members only (a leftover crate directory isn't built).
    let manifest: toml::Table = toml::from_str(include_str!("../../../../Cargo.toml")).unwrap();
    let members = manifest["workspace"]["members"].as_array().unwrap();
    let mut files = Vec::new();
    for m in members {
        let krate = crates_dir().join(m.as_str().unwrap().trim_start_matches("crates/"));
        // A crate's sources, tests, examples and build script; not target/.
        for sub in ["src", "tests", "examples", "benches"] {
            if krate.join(sub).is_dir() {
                rust_files(&krate.join(sub), &mut files);
            }
        }
        let build = krate.join("build.rs");
        if build.is_file() {
            let rel = build.strip_prefix(crates_dir()).unwrap();
            files.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    let mut out = BTreeMap::new();
    for f in files {
        // This file names the attributes it looks for.
        if f.ends_with("tests/it/lint_exceptions.rs") {
            continue;
        }
        let text = std::fs::read_to_string(crates_dir().join(&f)).unwrap();
        // rustfmt may break `#[expect(` and the lint name across lines.
        let squeezed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let lifted: Vec<&str> = BANS
            .iter()
            .filter(|(_, pat)| squeezed.contains(pat))
            .map(|(name, _)| *name)
            .collect();
        if !lifted.is_empty() {
            out.insert(f, lifted.join(","));
        }
    }
    out
}

/// `<path under crates/> <bans> | <reason>` lines.
fn listed() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (n, line) in include_str!("../fixtures/lint-exceptions.txt")
        .lines()
        .enumerate()
    {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (entry, reason) = line
            .split_once(" | ")
            .unwrap_or_else(|| panic!("lint-exceptions.txt line {}: no ` | <reason>`", n + 1));
        assert!(!reason.trim().is_empty(), "line {}: empty reason", n + 1);
        let (path, bans) = entry
            .split_once(' ')
            .unwrap_or_else(|| panic!("line {}: `<path> <bans> | <reason>`", n + 1));
        out.insert(path.to_string(), bans.trim().to_string());
    }
    out
}

#[test]
fn ban_exceptions_are_the_reviewed_list() {
    let found = found();
    let listed = listed();
    let mut bad = Vec::new();
    for (f, bans) in &found {
        match listed.get(f) {
            None => bad.push(format!("{f} {bans}: not in the list")),
            Some(l) if l != bans => bad.push(format!("{f}: lifts {bans}, listed as {l}")),
            Some(_) => {}
        }
    }
    for f in listed.keys().filter(|f| !found.contains_key(*f)) {
        bad.push(format!(
            "{f}: listed, but lifts no ban now (delete its line)"
        ));
    }
    assert!(
        bad.is_empty(),
        "unsafe/process-spawning exceptions differ from tests/fixtures/lint-exceptions.txt \
         (tests/it/lint_exceptions.rs). A new one is reviewed: add its line with a reason.\n  {}",
        bad.join("\n  ")
    );
}
