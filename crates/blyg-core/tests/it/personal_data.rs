//! No personal data in the repo (it's public, and history can't be taken
//! back): every file git tracks is scanned for
//!
//! - home folders: `/Users/<name>`, `C:\Users\<name>`;
//! - email addresses, other than at reserved example domains and GitHub's
//!   noreply addresses;
//! - blyg hosts: a URL whose host names a blyg must be an example host
//!   (`blyg.example.com`, `*.example`, `*.test`, …) or the platform's own
//!   site;
//! - GitHub owners (`github.com/<owner>/<repo>`, `<owner>.github.io`):
//!   a person's handle is personal data, so each one is listed.
//!
//! Anything else must match an entry in `tests/fixtures/personal-data-allow.txt`
//! exactly, with its reason. Invented sample values go there, never a real
//! person's details: fix the file instead (use `blyg.example.com`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What a scan can find.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Home,
    Email,
    BlygHost,
    GitHub,
}

impl Kind {
    fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "home" => Kind::Home,
            "email" => Kind::Email,
            "blyg-host" => Kind::BlygHost,
            "github" => Kind::GitHub,
            _ => return None,
        })
    }
}

/// The allowlist: exact values, and files a kind isn't scanned in.
#[derive(Default)]
struct Allow {
    values: BTreeSet<(Kind, String)>,
    files: BTreeSet<(Kind, String)>,
}

impl Allow {
    /// `<kind> <exact value> | <reason>`, or `file <path> <kind>,… | <reason>`.
    fn parse(text: &str) -> Allow {
        let mut allow = Allow::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (entry, reason) = line
                .split_once(" | ")
                .unwrap_or_else(|| panic!("allowlist line {}: no ` | <reason>`", n + 1));
            assert!(
                !reason.trim().is_empty(),
                "allowlist line {}: empty reason",
                n + 1
            );
            let mut parts = entry.split_whitespace();
            match parts.next() {
                Some("file") => {
                    let path = parts.next().expect("file <path> <kinds>");
                    for k in parts.next().expect("file <path> <kinds>").split(',') {
                        let k =
                            Kind::parse(k).unwrap_or_else(|| panic!("line {}: kind {k}", n + 1));
                        allow.files.insert((k, path.to_string()));
                    }
                }
                Some(k) => {
                    let k = Kind::parse(k).unwrap_or_else(|| panic!("line {}: kind {k}", n + 1));
                    let value = parts.next().expect("<kind> <value>");
                    allow.values.insert((k, value.to_string()));
                }
                None => unreachable!(),
            }
        }
        allow
    }
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// The run of name characters starting at `at` (by byte index).
fn name_at(text: &str, at: usize) -> &str {
    let rest = &text[at..];
    let end = rest.find(|c: char| !is_name_char(c)).unwrap_or(rest.len());
    &rest[..end]
}

/// `/Users/<name>` and `C:\Users\<name>` (backslashes doubled or not).
fn home_paths(text: &str, out: &mut Vec<String>) {
    for (i, _) in text.match_indices("Users") {
        let before = &text[..i];
        let after = &text[i + "Users".len()..];
        let (prefix, sep) = if before.ends_with('/') {
            ("/", "/")
        } else if before.ends_with('\\') {
            let drive = before.trim_end_matches('\\');
            if !drive.ends_with(':') {
                continue;
            }
            ("C:\\", "\\")
        } else {
            continue;
        };
        let rest = after.trim_start_matches(sep);
        if rest.len() == after.len() {
            continue;
        }
        let name = name_at(rest, 0);
        if name.is_empty() {
            continue;
        }
        out.push(format!("{prefix}Users{sep}{name}"));
    }
}

fn is_local_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-')
}

/// Email addresses: `<local>@<domain>.<tld>`.
fn emails(text: &str, out: &mut Vec<String>) {
    for (i, _) in text.match_indices('@') {
        let start = text[..i]
            .char_indices()
            .rev()
            .take_while(|&(_, c)| is_local_char(c))
            .last()
            .map_or(i, |(j, _)| j);
        let local = &text[start..i];
        let domain = name_at(text, i + 1).trim_end_matches(['.', '-']);
        let Some((_, tld)) = domain.rsplit_once('.') else {
            continue;
        };
        if local.is_empty() || tld.len() < 2 || !tld.chars().all(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        out.push(format!("{local}@{domain}"));
    }
}

/// Hosts of `scheme://` URLs that contain "blyg".
fn blyg_hosts(text: &str, out: &mut Vec<String>) {
    for (i, _) in text.match_indices("://") {
        let host = name_at(text, i + 3).trim_end_matches(['.', '-']);
        // A dotless "host" is a template or a pattern (`blyg-{i}`), not a host.
        if host.contains('.') && host.to_ascii_lowercase().contains("blyg") {
            out.push(host.to_ascii_lowercase());
        }
    }
}

/// `github.com/<owner>[/<repo>]` (not `api.github.com`) and `<owner>.github.io`.
fn github_owners(text: &str, out: &mut Vec<String>) {
    for (i, _) in text.match_indices("github.com/") {
        if text[..i].ends_with(|c: char| is_name_char(c)) {
            continue;
        }
        let rest = &text[i + "github.com/".len()..];
        let owner = name_at(rest, 0);
        if owner.is_empty() {
            continue;
        }
        let mut value = format!("github.com/{owner}");
        if let Some(repo) = rest[owner.len()..].strip_prefix('/') {
            let repo = name_at(repo, 0).trim_end_matches('.');
            if !repo.is_empty() {
                value = format!("{value}/{repo}");
            }
        }
        out.push(value);
    }
    for (i, _) in text.match_indices(".github.io") {
        let start = text[..i]
            .char_indices()
            .rev()
            .take_while(|&(_, c)| c.is_ascii_alphanumeric() || c == '-')
            .last()
            .map_or(i, |(j, _)| j);
        if start < i {
            out.push(format!("{}.github.io", &text[start..i]));
        }
    }
}

/// Reserved for examples (RFC 2606, RFC 6761), and GitHub's noreply.
fn is_example_domain(domain: &str) -> bool {
    let d = domain.to_ascii_lowercase();
    let d = d.trim_end_matches('.');
    let labels: Vec<&str> = d.split('.').collect();
    let last = *labels.last().unwrap();
    matches!(last, "example" | "test" | "invalid" | "localhost")
        || labels
            .windows(2)
            .any(|w| w[0] == "example" && matches!(w[1], "com" | "org" | "net"))
        || d == "users.noreply.github.com"
}

/// Every finding in one file, as `kind value`.
fn scan(path: &str, text: &str, allow: &Allow) -> Vec<(Kind, String)> {
    let mut found = Vec::new();
    let mut add = |kind: Kind, values: Vec<String>, ok: &dyn Fn(&str) -> bool| {
        if allow.files.contains(&(kind, path.to_string())) {
            return;
        }
        for v in values {
            if !ok(&v) && !allow.values.contains(&(kind, v.clone())) {
                found.push((kind, v));
            }
        }
    };
    let mut v = Vec::new();
    home_paths(text, &mut v);
    add(Kind::Home, std::mem::take(&mut v), &|_| false);
    emails(text, &mut v);
    add(Kind::Email, std::mem::take(&mut v), &|e| {
        is_example_domain(e.rsplit_once('@').unwrap().1)
    });
    blyg_hosts(text, &mut v);
    add(Kind::BlygHost, std::mem::take(&mut v), &is_example_domain);
    github_owners(text, &mut v);
    add(Kind::GitHub, std::mem::take(&mut v), &|_| false);
    found.sort();
    found.dedup();
    found
}

/// Folders not scanned: screenshots (binary images) and the clickable HTML
/// mocks, which CI skips too (`docs/prototype/` holds invented sample
/// content only).
const NOT_SCANNED: &[&str] = &["docs/screenshots/", "docs/prototype/"];

/// The files git lists under `root`; `None` without git or a checkout.
fn files_to_scan(root: &Path) -> Option<Vec<String>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn allowlist() -> Allow {
    Allow::parse(include_str!("../fixtures/personal-data-allow.txt"))
}

#[test]
fn no_personal_data_in_tracked_files() {
    let Some(files) = files_to_scan(&root()) else {
        eprintln!("skipped: no git checkout");
        return;
    };
    let allow = allowlist();
    let mut bad = Vec::new();
    for path in &files {
        let path = path.as_str();
        if NOT_SCANNED.iter().any(|d| path.starts_with(d)) {
            continue;
        }
        // Binary files (images, fonts, the model) and deleted files are skipped.
        let Ok(text) = std::fs::read_to_string(root().join(path)) else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            for (kind, value) in scan(path, line, &allow) {
                bad.push(format!("{path}:{}: {kind:?} {value}", n + 1));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "personal data in tracked files (tests/it/personal_data.rs). Use invented \
         values (blyg.example.com, someone@example.com); list an invented one that \
         has to stay in tests/fixtures/personal-data-allow.txt:\n  {}",
        bad.join("\n  ")
    );
}

/// The allowlist is exact: values next to an allowed one still fail.
#[test]
fn near_misses_still_fail() {
    let allow = allowlist();
    let found = |line: &str| scan("docs/x.md", line, &allow);

    // This repo is allowed; another repo of its owner, or another user's
    // Pages site, is not.
    let manifest: toml::Table = toml::from_str(include_str!("../../../../Cargo.toml")).unwrap();
    let repo = manifest["workspace"]["package"]["repository"]
        .as_str()
        .unwrap()
        .trim_start_matches("https://")
        .trim_end_matches('/');
    assert!(found(&format!("https://{repo}")).is_empty(), "{repo}");
    let owner = repo.rsplit_once('/').unwrap().0;
    assert_eq!(
        found(&format!("https://{owner}/not-this-repo")),
        [(Kind::GitHub, format!("{owner}/not-this-repo"))]
    );
    assert_eq!(
        found(concat!("https://someone-else.", "github.io/notes")),
        [(
            Kind::GitHub,
            concat!("someone-else.", "github.io").to_string()
        )]
    );
    assert_eq!(
        found(concat!("https://github", ".com/someone-else")),
        [(
            Kind::GitHub,
            concat!("github", ".com/someone-else").to_string()
        )]
    );

    // Home folders other than the invented fixtures.
    assert_eq!(
        found(concat!("open /Us", "ers/someone-else/notes.md")),
        [(Kind::Home, concat!("/Us", "ers/someone-else").to_string())]
    );
    assert_eq!(
        found(concat!(r"vault = C:\Us", r"ers\someone-else\Notes")),
        [(
            Kind::Home,
            concat!(r"C:\Us", r"ers\someone-else").to_string()
        )]
    );

    // An email at a real domain, and a real-looking blyg host. (Each value
    // is split in the source, so this file's own scan doesn't find it.)
    assert_eq!(
        found(concat!("mail someone", "@mail-provider.com")),
        [(
            Kind::Email,
            concat!("someone", "@mail-provider.com").to_string()
        )]
    );
    assert_eq!(
        found(concat!("see https://someones.", "blyg.space/post")),
        [(
            Kind::BlygHost,
            concat!("someones.", "blyg.space").to_string()
        )]
    );

    // And what's meant to pass does.
    assert!(found("https://ada.blyg.example.com/p and someone@example.com").is_empty());
    assert!(found("12345+someone@users.noreply.github.com").is_empty());
}

/// A file not yet `git add`ed is scanned too (so the local gate catches it
/// before it's ever committed); one git ignores is not.
#[test]
fn untracked_files_are_scanned_and_ignored_ones_are_not() {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    std::fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();
    std::fs::write(dir.path().join("tracked.txt"), "x").unwrap();
    git(&["add", ".gitignore", "tracked.txt"]);
    std::fs::write(dir.path().join("untracked.txt"), "x").unwrap();
    std::fs::write(dir.path().join("ignored.txt"), "x").unwrap();
    let mut files = files_to_scan(dir.path()).unwrap();
    files.sort();
    assert_eq!(files, [".gitignore", "tracked.txt", "untracked.txt"]);
}
