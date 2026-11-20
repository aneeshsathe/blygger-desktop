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
//! It reads `cargo metadata --no-deps` (the workspace's own manifests), so it
//! needs no downloads: a full resolve wants every platform's packages, which
//! CI's cache doesn't hold. "Reaches" is followed through workspace crates;
//! a non-app crate may not name a UI crate directly, and no third-party crate
//! here pulls one in.

use std::collections::{BTreeMap, BTreeSet};
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
