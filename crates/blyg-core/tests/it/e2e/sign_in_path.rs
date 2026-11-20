//! The e2e suite signs in to the studio in one place (`studio_login`) and
//! waits out a 429 in one place (`retry_429`). Every password sign-in in a
//! run spends the same studio sign-in budget, so a test that signs in on its
//! own, or copies its own 429 loop, is how runs started failing each other.
//!
//! This test needs no studio: it reads the e2e sources (this directory and
//! blyg-app's `switcher_e2e.rs`) and runs in the normal `cargo test`.

use std::path::{Path, PathBuf};

/// Signing in: allowed only inside `studio_login`.
const SIGN_IN: &[&str] = &["auth::login(", "/studio/login"];
/// Matching or comparing a 429: allowed only inside `retry_429`.
const RETRY_429: &[&str] = &[
    "status: 429",
    "== 429",
    "!= 429",
    "429 =>",
    "Some(429)",
    "TOO_MANY_REQUESTS",
];

/// The sources the check reads, relative to the workspace root.
fn sources() -> Vec<PathBuf> {
    let dir = Path::new("crates/blyg-core/tests/it/e2e");
    let mut files: Vec<PathBuf> = std::fs::read_dir(workspace().join(dir))
        .expect("tests/it/e2e")
        .map(|e| dir.join(e.unwrap().file_name()))
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        // This file names the patterns it looks for.
        .filter(|p| !p.ends_with("sign_in_path.rs"))
        .collect();
    files.push("crates/blyg-app/src/switcher_e2e.rs".into());
    files.sort();
    files
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The 1-based line span of `fn <name>`'s body, if the file defines it.
fn fn_span(lines: &[&str], name: &str) -> Option<(usize, usize)> {
    let head = format!("fn {name}");
    let start = lines.iter().position(|l| {
        let l = l.trim_start();
        l.starts_with(&head) || l.contains(&format!(" {head}"))
    })?;
    let mut depth = 0i32;
    let mut opened = false;
    for (i, l) in lines.iter().enumerate().skip(start) {
        for c in l.chars() {
            match c {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        if opened && depth == 0 {
            return Some((start + 1, i + 1));
        }
    }
    Some((start + 1, lines.len()))
}

/// Each line using one of `patterns` outside `fn <owner>`.
fn strays(path: &Path, src: &str, owner: &str, patterns: &[&str]) -> Vec<String> {
    let lines: Vec<&str> = src.lines().collect();
    let span = fn_span(&lines, owner);
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.trim_start().starts_with("//"))
        .filter(|(i, _)| !span.is_some_and(|(a, b)| (a..=b).contains(&(i + 1))))
        .filter(|(_, l)| patterns.iter().any(|p| l.contains(p)))
        .map(|(i, l)| format!("{}:{}: {}", path.display(), i + 1, l.trim()))
        .collect()
}

#[test]
fn one_sign_in_path() {
    let mut sign_ins = Vec::new();
    let mut retries = Vec::new();
    for path in sources() {
        let src = std::fs::read_to_string(workspace().join(&path))
            .unwrap_or_else(|e| panic!("{path:?}: {e}"));
        sign_ins.extend(strays(&path, &src, "studio_login", SIGN_IN));
        retries.extend(strays(&path, &src, "retry_429", RETRY_429));
    }
    assert!(
        sign_ins.is_empty() && retries.is_empty(),
        "e2e signs in only through `studio_login` and waits out a 429 only \
         through `retry_429` (both in tests/it/e2e/mod.rs); the studio's \
         sign-in budget is shared by the whole run.\n\
         sign-ins outside studio_login:\n  {}\n\
         429 handling outside retry_429:\n  {}",
        sign_ins.join("\n  "),
        retries.join("\n  "),
    );
}
