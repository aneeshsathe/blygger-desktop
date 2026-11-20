//! The workspace is split by runtime context, and the split is a check:
//! `cargo metadata` must show only the dependencies [`ALLOWED`] lets each
//! crate have, and no crate but the app may reach GPUI at all (directly or
//! through anything it depends on).
//!
//! - `blyg-render`, `blyg-embed`: leaves (pure rendering; the local model).
//! - `blyg-core`: no UI; renders with `blyg-render`.
//! - `blyg-ai`, `blyg-ext`: on top of `blyg-core`.
//! - the `blyg-ext-*` extensions: on `blyg-ext` (and what it stands on).
//! - `blyg-app`: the only GPUI crate, and the only one that may depend on
//!   `blyg-ai`, `blyg-embed` or another crate's UI.
//!
//! A new crate fails here until it's given a place in [`ALLOWED`].
//!
//! Only the crates in [`NETWORK`] may depend on an HTTP client: a new
//! network caller anywhere else fails here, so it gets looked at.
//!
//! Every crate also takes the shared lint config: its own `clippy.toml`
//! (clippy reads the bans per crate, so a crate without one has none) and
//! `[lints] workspace = true` (the root's `[workspace.lints]`). Every crate
//! directory git tracks under `crates/` is a workspace member, so
//! `--workspace` clippy and tests reach it.
//!
//! It reads `cargo metadata --no-deps` (the workspace's own manifests), so it
//! needs no downloads: a full resolve wants every platform's packages, which
//! CI's cache doesn't hold. "Reaches" is followed through workspace crates;
//! a non-app crate may not name a UI crate directly, and no third-party crate
//! here pulls one in.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// Each workspace crate and the workspace crates it may depend on (any
/// kind: normal, build or dev). `*` is every workspace crate.
const ALLOWED: &[(&str, &[&str])] = &[
    ("blyg-render", &[]),
    ("blyg-embed", &[]),
    ("blyg-core", &["blyg-render"]),
    ("blyg-ai", &["blyg-core", "blyg-render"]),
    ("blyg-ext", &["blyg-core", "blyg-render"]),
    ("blyg-ext-*", &["blyg-ext", "blyg-core", "blyg-render"]),
    ("blyg-app", &["*"]),
];

/// The crates that may talk to the network (a direct dependency on an HTTP
/// client): the blyg and studio API (`blyg-core`), AI providers (`blyg-ai`),
/// the embedding model's one-time download (`blyg-embed`) and the app
/// (update checks, block lists).
const NETWORK: &[&str] = &["blyg-core", "blyg-ai", "blyg-embed", "blyg-app"];

/// HTTP client crates.
fn is_http_client(package: &str) -> bool {
    matches!(
        package,
        "ureq" | "reqwest" | "hyper" | "isahc" | "curl" | "attohttpc" | "minreq" | "surf" | "ehttp"
    )
}

/// The UI toolkit: only `blyg-app` may have any of these anywhere in its
/// dependency tree.
fn is_ui(package: &str) -> bool {
    package.starts_with("gpui") || package == "blyg-app"
}

fn allowed(name: &str) -> Option<&'static [&'static str]> {
    ALLOWED
        .iter()
        .find(|(pat, _)| match pat.strip_suffix('*') {
            Some(prefix) => name.starts_with(prefix),
            None => *pat == name,
        })
        .map(|(_, deps)| *deps)
}

fn metadata() -> Value {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let out = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .args(["--manifest-path", manifest])
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("cargo metadata is JSON")
}

/// Every broken rule, as lines naming the crate and the dependency.
fn violations(m: &Value) -> Vec<String> {
    // Each workspace crate and the names of everything it depends on (any
    // kind, any platform).
    let deps: BTreeMap<&str, Vec<&str>> = m["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let names = p["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| d["name"].as_str().unwrap())
                .collect();
            (p["name"].as_str().unwrap(), names)
        })
        .collect();

    let mut bad = Vec::new();
    for (&name, direct) in &deps {
        for &dep in direct {
            if is_http_client(dep) && !NETWORK.contains(&name) {
                bad.push(format!(
                    "{name} depends on {dep}, an HTTP client: only {} may (NETWORK)",
                    NETWORK.join(", ")
                ));
            }
        }
        let Some(ok) = allowed(name) else {
            bad.push(format!(
                "{name}: a new workspace crate; give it a place in ALLOWED"
            ));
            continue;
        };
        // Direct dependencies on other workspace crates.
        for &dep in direct {
            if deps.contains_key(dep) && !ok.contains(&"*") && !ok.contains(&dep) {
                bad.push(format!("{name} depends on {dep}"));
            }
        }
        if ok.contains(&"*") {
            continue;
        }
        // Anything it reaches, through workspace crates.
        let mut seen = BTreeSet::new();
        let mut todo = vec![name];
        while let Some(at) = todo.pop() {
            for &dep in deps.get(at).into_iter().flatten() {
                if seen.insert(dep) && deps.contains_key(dep) {
                    todo.push(dep);
                }
            }
        }
        let ui: Vec<&str> = seen.into_iter().filter(|n| is_ui(n)).collect();
        if !ui.is_empty() {
            bad.push(format!("{name} reaches UI crates: {}", ui.join(", ")));
        }
    }
    bad
}

#[test]
fn crates_depend_only_on_what_their_runtime_allows() {
    let bad = violations(&metadata());
    assert!(
        bad.is_empty(),
        "crate boundaries broken (tests/it/crate_graph.rs):\n  {}",
        bad.join("\n  ")
    );
}

/// Every gap in the lint config, as lines naming the crate.
fn lint_config_gaps(m: &Value) -> Vec<String> {
    let mut bad = Vec::new();
    let mut member_dirs = BTreeSet::new();
    for p in m["packages"].as_array().unwrap() {
        let name = p["name"].as_str().unwrap();
        let manifest = Path::new(p["manifest_path"].as_str().unwrap());
        let dir = manifest.parent().unwrap();
        member_dirs.insert(dir.to_path_buf());
        if !dir.join("clippy.toml").is_file() {
            bad.push(format!(
                "{name}: no clippy.toml (clippy reads its bans per crate: copy a sibling's)"
            ));
        }
        let text = std::fs::read_to_string(manifest).unwrap();
        let doc: toml::Table = toml::from_str(&text).unwrap();
        let takes_workspace = doc
            .get("lints")
            .and_then(|l| l.get("workspace"))
            .and_then(|w| w.as_bool())
            == Some(true);
        if !takes_workspace {
            bad.push(format!(
                "{name}: Cargo.toml has no `[lints] workspace = true`"
            ));
        }
    }
    // Crate directories git tracks (an untracked leftover isn't a crate).
    let root = Path::new(m["workspace_root"].as_str().unwrap());
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--", "crates/*/Cargo.toml"])
        .output();
    match out {
        Ok(out) if out.status.success() => {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let dir = root.join(line).parent().unwrap().to_path_buf();
                if !member_dirs.contains(&dir) {
                    bad.push(format!(
                        "{line}: not a workspace member (add it to the root Cargo.toml's members)"
                    ));
                }
            }
        }
        _ => eprintln!("skipped the members check: no git checkout"),
    }
    bad
}

#[test]
fn every_crate_takes_the_lint_config() {
    let bad = lint_config_gaps(&metadata());
    assert!(
        bad.is_empty(),
        "lint config missing (tests/it/crate_graph.rs):\n  {}",
        bad.join("\n  ")
    );
}
