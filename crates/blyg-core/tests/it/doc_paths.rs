//! Every repo file a development doc names in backticks exists.
//!
//! The instructions an agent works from (`CLAUDE.md`, `AGENTS.md`, every
//! Markdown file under `skills/`) name files to read, run or copy. When a
//! file moves, the doc still names the old place, and the next agent goes
//! looking for it. This test reads each inline code span and, if it looks
//! like a path, checks that the repo has it.
//!
//! - A span is a path when it has no spaces or placeholder characters
//!   (`<>*{}$~?`), isn't absolute or a URL, and either has a `/` and ends in
//!   a file extension or starts at one of the repo's top-level folders, or
//!   is a bare file name with a known extension (`keymap_docs.rs`).
//!   Fenced code blocks are skipped: they hold commands, not references.
//! - It matches when the path exists from the repo root, from the doc's own
//!   folder, or as the tail of a repo path (`blyg-core/src/order.rs` for
//!   `crates/blyg-core/src/order.rs`; a bare name matches a file's name).
//! - A path git ignores (local-only, such as `docs/private/`) is skipped.
//! - A path in another repo is followed by `(other repo: <name>)`, which
//!   exempts that span only (spaces and closing punctuation may come between).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The marker that exempts the span before it: it lives in another repo.
const OTHER_REPO: &str = "(other repo:";

/// Extensions that make a slash-separated span a file path.
const EXTENSIONS: &[&str] = &[
    "rs", "md", "sh", "toml", "json", "yml", "yaml", "mjs", "js", "ts", "html", "css", "expected",
    "txt", "lock",
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn git(args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(root())
        .output()
        .expect("git runs")
}

/// Every path the repo has (tracked or new, not ignored), with each folder.
fn repo_paths() -> BTreeSet<String> {
    let out = git(&["ls-files", "--cached", "--others", "--exclude-standard"]);
    assert!(out.status.success(), "git ls-files failed");
    let mut paths = BTreeSet::new();
    for file in String::from_utf8_lossy(&out.stdout).lines() {
        let mut p = file;
        paths.insert(p.to_string());
        while let Some((dir, _)) = p.rsplit_once('/') {
            paths.insert(dir.to_string());
            p = dir;
        }
    }
    paths
}

fn ignored(path: &str) -> bool {
    git(&["check-ignore", "-q", "--no-index", path])
        .status
        .success()
}

/// The docs to check, relative to the root.
fn docs(paths: &BTreeSet<String>) -> Vec<String> {
    paths
        .iter()
        .filter(|p| {
            *p == "CLAUDE.md"
                || *p == "AGENTS.md"
                || (p.starts_with("skills/") && p.ends_with(".md"))
        })
        .cloned()
        .collect()
}

/// An inline code span, with its line number and whether the
/// `(other repo: …)` marker exempts it.
#[derive(Debug, PartialEq)]
struct Span<'a> {
    line: usize,
    code: &'a str,
    other_repo: bool,
}

/// Inline code spans on each line outside fenced blocks.
fn spans(text: &str) -> Vec<Span<'_>> {
    let mut out = Vec::new();
    let mut fenced = false;
    for (i, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut parts = line.split('`');
        parts.next();
        while let (Some(code), Some(after)) = (parts.next(), parts.next()) {
            // The marker belongs to the span it follows, with only spaces
            // and closing punctuation between them.
            let other_repo = after
                .trim_start_matches(|c: char| c.is_whitespace() || ")],;:.".contains(c))
                .starts_with(OTHER_REPO);
            out.push(Span {
                line: i + 1,
                code,
                other_repo,
            });
        }
        // An unclosed span's text is dropped by the pairing above.
    }
    out
}

/// Whether a span reads as a repo path (see the module docs).
fn is_path(span: &str, top_level: &BTreeSet<&str>) -> bool {
    if span
        .chars()
        .any(|c| c.is_whitespace() || "<>*{}$~?".contains(c))
        || span.starts_with('/')
        || span.contains("://")
    {
        return false;
    }
    let trimmed = span.trim_start_matches("./").trim_start_matches("../");
    let first = trimmed.split('/').next().unwrap_or("");
    let last = span.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    let has_ext = last
        .rsplit_once('.')
        .is_some_and(|(stem, ext)| !stem.is_empty() && EXTENSIONS.contains(&ext));
    if !span.contains('/') {
        // A bare file name (`keymap_docs.rs`) must be some repo file's name.
        return has_ext
            && span
                .chars()
                .all(|c| c.is_alphanumeric() || "._-".contains(c));
    }
    has_ext || top_level.contains(first)
}

/// Whether `span`, named in `doc`, matches a repo path.
fn resolves(span: &str, doc: &str, paths: &BTreeSet<String>) -> bool {
    let span = span.trim_end_matches('/');
    let doc_dir = Path::new(doc).parent().unwrap_or(Path::new(""));
    let from_doc = normalize(&doc_dir.join(span));
    if paths.contains(span) || from_doc.as_deref().is_some_and(|p| paths.contains(p)) {
        return true;
    }
    if span.starts_with("./") || span.starts_with("../") {
        return false;
    }
    let tail = format!("/{span}");
    paths.iter().any(|p| p.ends_with(&tail))
}

/// `a/b/../c` → `a/c`, with `/` separators; `None` if it climbs out of the root.
fn normalize(path: &Path) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for c in path.components() {
        match c {
            std::path::Component::ParentDir => {
                parts.pop()?;
            }
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
        }
    }
    Some(parts.join("/"))
}

#[test]
fn every_path_a_dev_doc_names_exists() {
    let paths = repo_paths();
    let top_level: BTreeSet<&str> = paths
        .iter()
        .filter_map(|p| p.split_once('/').map(|(first, _)| first))
        .collect();
    let mut missing = Vec::new();
    for doc in docs(&paths) {
        let text = std::fs::read_to_string(root().join(&doc)).expect("doc reads");
        for Span {
            line,
            code,
            other_repo,
        } in spans(&text)
        {
            if other_repo || !is_path(code, &top_level) {
                continue;
            }
            if !resolves(code, &doc, &paths) && !ignored(code) {
                missing.push(format!("{doc}:{line}: `{code}`"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "these docs name paths the repo doesn't have. Fix the path, or, if it lives \
         in another repo, add `{OTHER_REPO} <name>)` right after it:\n{}",
        missing.join("\n")
    );
}

#[test]
fn the_path_heuristic_skips_code_and_keeps_paths() {
    let top: BTreeSet<&str> = ["crates", "skills"].into_iter().collect();
    for code in [
        "application/x-tex",
        "importer/sanitize",
        "blygger/blygger-spec",
        "/api/capabilities",
        "GET /api/changes",
        "items/{id}.json",
        "<run dir>/shots/x.png",
        "~/.config/blygger/config",
        "https://blyg.example.com/a.md",
        "a/b",
        "blyg.example.com",
        "0.27.0",
        "ReadingItem::is_unread",
        "self.sheet",
        "*_vm.rs",
        "protocol-v*.md",
    ] {
        assert!(!is_path(code, &top), "{code} isn't a path");
    }
    for path in [
        "crates/blyg-ai",
        "skills/verify-burrow",
        "blyg-core/src/order.rs",
        "../SKILL.md",
        ".github/workflows/ci.yml",
        "keymap_docs.rs",
        "openapi-current.json",
        "releases.md",
    ] {
        assert!(is_path(path, &top), "{path} is a path");
    }
}

#[test]
fn a_path_resolves_from_the_root_the_doc_or_as_a_tail() {
    let paths: BTreeSet<String> = [
        "crates/blyg-core/src/order.rs",
        "skills/verify-burrow/features/README.md",
        "skills/verify-burrow/SKILL.md",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let doc = "skills/verify-burrow/SKILL.md";
    assert!(resolves("crates/blyg-core/src/order.rs", doc, &paths));
    assert!(resolves("blyg-core/src/order.rs", doc, &paths));
    assert!(resolves("features/README.md", doc, &paths));
    assert!(resolves(
        "../SKILL.md",
        "skills/verify-burrow/features/README.md",
        &paths
    ));
    assert!(
        !resolves("core/src/order.rs", doc, &paths),
        "a tail starts at a /"
    );
    assert!(!resolves("crates/blyg-core/tests/it/e2e.rs", doc, &paths));
}

#[test]
fn the_marker_exempts_only_the_span_it_follows() {
    let exempt = |text| -> Vec<(&str, bool)> {
        spans(text)
            .into_iter()
            .map(|s| (s.code, s.other_repo))
            .collect()
    };
    assert_eq!(
        exempt("Check `crates/a.rs` against `feeds/` (other repo: spec)."),
        [("crates/a.rs", false), ("feeds/", true)]
    );
    assert_eq!(
        exempt("an adapter (`schemas/A.md`) (other repo: spec) and `b/c.rs`"),
        [("schemas/A.md", true), ("b/c.rs", false)]
    );
    assert_eq!(
        exempt("`a/b.md`, then words, then (other repo: spec)"),
        [("a/b.md", false)],
        "words between the span and the marker break the link"
    );
    assert_eq!(
        exempt("mark it with `(other repo: <name>)` after `a/b.md`"),
        [("(other repo: <name>)", false), ("a/b.md", false)],
        "a marker inside a code span is an example, not a marker"
    );
}
