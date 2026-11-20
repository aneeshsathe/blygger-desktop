//! Every item in the newest studio's contract (`openapi-current.json`) is
//! named by a test, or listed in `tests/fixtures/contract-coverage.toml`
//! with a reason. A studio sync starts by vendoring the new contract and
//! moving `[target]`; this test then lists what's new, so the tests are
//! written before the feature (studio 0.64's mention relation `link` reached
//! Burrow only when a late e2e run failed).
//!
//! Items (each key names where in the contract it is):
//! - `operation:{operationId}`: named by its operationId, or a test string
//!   holds its path (`/api/reading/placement`, `{param}` = any segment);
//! - `property:{where}.{name}`: a test holds `"name"`, or the snake_case
//!   field (`tangled_from` for `tangledFrom`) when that differs;
//! - `enum:{where}={value}`: a test holds `"value"` within 3 lines of the
//!   property or parameter it belongs to (a bare `"link"` is too common);
//! - `param:{operationId}/{in}:{name}` and `header:{operationId}:{name}`
//!   (response headers): a test holds `"name"`, any case.
//!
//! Tests are `crates/*/tests/**.rs` and, in `src/`, `*_tests.rs`, `tests.rs`,
//! files under a `tests/` directory, a `#[cfg(test)] mod x;` file and the rest
//! of a file from its `#[cfg(test)] mod x {`.
//!
//! The toml's states: `[[exempt]]` (a reason; fails once a test covers it),
//! `[[pending]]` (a work package is building it: a warning while uncovered, a
//! failure once covered, so the entry can't linger) and the generated
//! `baseline = true` group (everything in the contract at the last studio
//! synced before this check; never re-checked). Entries may use `*`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

const HINT: &str = "write a test that names it (e2e against a real studio, see skills/real-server-testing), or add it to crates/blyg-core/tests/fixtures/contract-coverage.toml with a reason";

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn toml_path() -> PathBuf {
    manifest().join("tests/fixtures/contract-coverage.toml")
}

fn current() -> Value {
    serde_json::from_str(include_str!("../fixtures/openapi-current.json")).expect("contract JSON")
}

// ---- the toml ----

#[derive(Deserialize)]
struct Coverage {
    target: Target,
    #[serde(default)]
    exempt: Vec<Exempt>,
    #[serde(default)]
    pending: Vec<Pending>,
    #[serde(default)]
    group: Vec<Group>,
}

#[derive(Deserialize)]
struct Target {
    studio: String,
    contract: String,
}

#[derive(Deserialize)]
struct Exempt {
    item: String,
    reason: String,
}

#[derive(Deserialize)]
struct Pending {
    item: String,
    pending: String,
}

#[derive(Deserialize)]
struct Group {
    #[serde(default)]
    baseline: bool,
    reason: String,
    items: Vec<String>,
}

fn coverage() -> Coverage {
    toml::from_str(&std::fs::read_to_string(toml_path()).unwrap()).expect("contract-coverage.toml")
}

/// `*` matches any run of characters; everything else is literal.
fn glob(pat: &str, s: &str) -> bool {
    let parts: Vec<&str> = pat.split('*').collect();
    if parts.len() == 1 {
        return pat == s;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !s.starts_with(first) || s.len() < first.len() + last.len() || !s.ends_with(last) {
        return false;
    }
    let mut rest = &s[first.len()..s.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

// ---- the contract's items ----

#[derive(Debug, Clone)]
enum Rule {
    /// operationId, and path template segments (with the server's `/api`).
    Operation(String, Vec<String>),
    Property(String),
    /// value, and the property or parameter it belongs to.
    Enum(String, Option<String>),
    /// A parameter or header name, any case.
    Name(String),
}

#[derive(Debug, Clone)]
struct Item {
    rule: Rule,
    /// The route it's on, for the failure message (empty for schemas).
    at: String,
}

#[derive(Default)]
struct Items(BTreeMap<String, Item>);

impl Items {
    fn add(&mut self, key: String, rule: Rule, at: String) {
        self.0.entry(key).or_insert(Item { rule, at });
    }

    fn schema(&mut self, s: &Value, at: &str, parent: Option<&str>) {
        match s {
            Value::Array(list) => list.iter().for_each(|x| self.schema(x, at, parent)),
            Value::Object(o) => {
                if let Some(Value::Object(props)) = o.get("properties") {
                    for (name, sub) in props {
                        let here = format!("{at}.{name}");
                        self.add(
                            format!("property:{here}"),
                            Rule::Property(name.clone()),
                            String::new(),
                        );
                        self.schema(sub, &here, Some(name));
                    }
                }
                let consts = o
                    .get("enum")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for v in consts.iter().chain(o.get("const")) {
                    if let Some(v) = v.as_str() {
                        self.add(
                            format!("enum:{at}={v}"),
                            Rule::Enum(v.to_string(), parent.map(str::to_string)),
                            String::new(),
                        );
                    }
                }
                if let Some(items) = o.get("items") {
                    self.schema(items, &format!("{at}[]"), parent);
                }
                for k in [
                    "additionalProperties",
                    "not",
                    "allOf",
                    "anyOf",
                    "oneOf",
                    "prefixItems",
                ] {
                    if let Some(sub) = o.get(k) {
                        self.schema(sub, at, parent);
                    }
                }
            }
            _ => {}
        }
    }
}

fn resolve<'a>(doc: &'a Value, v: &'a Value) -> &'a Value {
    match v.get("$ref").and_then(Value::as_str) {
        Some(r) => doc.pointer(r.trim_start_matches('#')).unwrap_or(v),
        None => v,
    }
}

fn items(doc: &Value) -> Items {
    let mut out = Items::default();
    for (name, s) in doc["components"]["schemas"]
        .as_object()
        .into_iter()
        .flatten()
    {
        out.schema(s, name, None);
    }
    let mut ops = vec![];
    for (path, item) in doc["paths"].as_object().into_iter().flatten() {
        ops.push((format!("/api{path}"), item));
    }
    for (name, item) in doc["webhooks"].as_object().into_iter().flatten() {
        ops.push((format!("webhook {name}"), item));
    }
    for (path, item) in ops {
        let shared = item["parameters"].as_array().cloned().unwrap_or_default();
        for (method, op) in item.as_object().into_iter().flatten() {
            if method == "parameters" || !op.is_object() {
                continue;
            }
            let id = op["operationId"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("{method} {path}"));
            let segs = path
                .trim_start_matches('/')
                .split('/')
                .map(str::to_string)
                .collect();
            let route = format!("{} {path}", method.to_ascii_uppercase());
            out.add(
                format!("operation:{id}"),
                Rule::Operation(id.clone(), segs),
                route.clone(),
            );
            let params = shared
                .iter()
                .chain(op["parameters"].as_array().into_iter().flatten());
            for p in params {
                let p = resolve(doc, p);
                let (Some(name), Some(place)) = (p["name"].as_str(), p["in"].as_str()) else {
                    continue;
                };
                let at = format!("{id}/{place}:{name}");
                out.add(
                    format!("param:{at}"),
                    Rule::Name(name.to_string()),
                    route.clone(),
                );
                out.schema(&p["schema"], &at, Some(name));
            }
            for c in op["requestBody"]["content"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|x| x.1)
            {
                out.schema(&c["schema"], &format!("{id}/request"), None);
            }
            for (status, r) in op["responses"].as_object().into_iter().flatten() {
                let r = resolve(doc, r);
                for h in r["headers"].as_object().into_iter().flatten().map(|x| x.0) {
                    let at = format!("{id}:{h}");
                    out.add(format!("header:{at}"), Rule::Name(h.clone()), route.clone());
                }
                for c in r["content"].as_object().into_iter().flatten().map(|x| x.1) {
                    out.schema(&c["schema"], &format!("{id}/{status}"), None);
                }
            }
        }
    }
    out
}

// ---- the tests' text ----

/// Every test source in the workspace, one string per file.
fn test_sources() -> Vec<(PathBuf, String)> {
    let crates = manifest().join("..");
    // This file names items in its own checks.
    let me = Path::new("blyg-core/tests/it/contract_coverage.rs");
    let mut out = vec![];
    for krate in std::fs::read_dir(&crates)
        .unwrap()
        .flatten()
        .map(|e| e.path())
    {
        for f in rs_files(&krate.join("tests")) {
            if !f.ends_with(me) && !f.components().any(|c| c.as_os_str() == "fixtures") {
                let text = std::fs::read_to_string(&f).unwrap();
                out.push((f, text));
            }
        }
        let src = krate.join("src");
        let all = rs_files(&src);
        let mut tests: BTreeSet<PathBuf> = all
            .iter()
            .filter(|f| {
                let name = f.file_name().unwrap().to_string_lossy();
                name.ends_with("_tests.rs")
                    || name == "tests.rs"
                    || f.strip_prefix(&src)
                        .unwrap()
                        .components()
                        .any(|c| c.as_os_str() == "tests")
            })
            .cloned()
            .collect();
        for f in &all {
            let text = std::fs::read_to_string(f).unwrap();
            for m in test_mod_files(f, &text) {
                tests.insert(m);
            }
            if tests.contains(f) {
                continue;
            }
            if let Some(i) = inline_test_mod(&text) {
                out.push((f.clone(), text[i..].to_string()));
            }
        }
        for f in tests {
            if let Ok(text) = std::fs::read_to_string(&f) {
                out.push((f, text));
            }
        }
    }
    out
}

fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for p in rd.flatten().map(|e| e.path()) {
        if p.is_dir() {
            out.extend(rs_files(&p));
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// `#[cfg(test)]` and the line after it, for each such attribute.
fn cfg_test_lines(text: &str) -> Vec<(usize, &str)> {
    let mut offset = 0;
    let lines: Vec<(usize, &str)> = text
        .split_inclusive('\n')
        .map(|l| {
            let at = offset;
            offset += l.len();
            (at, l)
        })
        .collect();
    lines
        .windows(2)
        .filter(|w| w[0].1.trim() == "#[cfg(test)]")
        .map(|w| (w[0].0, w[1].1.trim()))
        .collect()
}

fn mod_decl(line: &str) -> Option<&str> {
    let line = line
        .strip_prefix("pub(crate) ")
        .or_else(|| line.strip_prefix("pub "))
        .unwrap_or(line);
    line.strip_prefix("mod ")
}

/// Where the first inline `#[cfg(test)] mod x {` starts (to the file's end).
fn inline_test_mod(text: &str) -> Option<usize> {
    cfg_test_lines(text)
        .into_iter()
        .find(|(_, next)| mod_decl(next).is_some_and(|m| m.ends_with('{')))
        .map(|(at, _)| at)
}

/// Files declared as `#[cfg(test)] mod x;` in `file`.
fn test_mod_files(file: &Path, text: &str) -> Vec<PathBuf> {
    let dir = file.parent().unwrap();
    let stem = file.file_stem().unwrap().to_string_lossy();
    let base = if ["mod", "lib", "main"].contains(&stem.as_ref()) {
        dir.to_path_buf()
    } else {
        dir.join(&*stem)
    };
    cfg_test_lines(text)
        .into_iter()
        .filter_map(|(_, next)| mod_decl(next)?.strip_suffix(';').map(str::to_string))
        .flat_map(|m| [base.join(format!("{m}.rs")), base.join(&m).join("mod.rs")])
        .filter(|p| p.exists())
        .collect()
}

fn ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `word` with no identifier character on either side.
fn has_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(i, _)| {
        !text[..i].chars().next_back().is_some_and(ident_char)
            && !text[i + word.len()..]
                .chars()
                .next()
                .is_some_and(ident_char)
    })
}

fn snake(s: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in s.chars() {
        if c.is_ascii_uppercase() && prev_lower {
            out.push('_');
        }
        prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// A test string holding the path: `/api/a/b`, a `{param}` matching one
/// segment, and nothing path-like right after it.
fn has_path(text: &str, segs: &[String]) -> bool {
    let first = format!("/{}/", segs[0]);
    text.match_indices(&first).any(|(i, _)| {
        let mut rest = &text[i + 1..];
        for (n, seg) in segs.iter().enumerate() {
            if seg.starts_with('{') {
                let len = rest
                    .find(|c: char| matches!(c, '/' | '"' | '?' | '#') || c.is_whitespace())
                    .unwrap_or(rest.len());
                if len == 0 {
                    return false;
                }
                rest = &rest[len..];
            } else if let Some(r) = rest.strip_prefix(seg.as_str()) {
                rest = r;
            } else {
                return false;
            }
            if n + 1 < segs.len() {
                match rest.strip_prefix('/') {
                    Some(r) => rest = r,
                    None => return false,
                }
            }
        }
        !rest.starts_with(|c: char| ident_char(c) || c == '-' || c == '/')
    })
}

struct Tests {
    files: Vec<String>,
    lower: Vec<String>,
}

impl Tests {
    fn load() -> Tests {
        let files: Vec<String> = test_sources().into_iter().map(|x| x.1).collect();
        let lower = files.iter().map(|t| t.to_ascii_lowercase()).collect();
        Tests { files, lower }
    }

    fn covers(&self, rule: &Rule) -> bool {
        match rule {
            Rule::Operation(id, segs) => self
                .files
                .iter()
                .any(|t| has_word(t, id) || has_path(t, segs)),
            Rule::Property(name) => {
                let quoted = format!("\"{name}\"");
                let snake = snake(name);
                self.files
                    .iter()
                    .any(|t| t.contains(&quoted) || (snake != *name && has_word(t, &snake)))
            }
            Rule::Name(name) => {
                let quoted = format!("\"{}\"", name.to_ascii_lowercase());
                self.lower.iter().any(|t| t.contains(&quoted))
            }
            Rule::Enum(value, parent) => {
                let quoted = format!("\"{value}\"");
                self.files.iter().any(|t| {
                    let lines: Vec<&str> = t.lines().collect();
                    lines.iter().enumerate().any(|(i, l)| {
                        l.contains(&quoted)
                            && parent.as_ref().is_none_or(|p| {
                                let near = lines[i.saturating_sub(3)..(i + 4).min(lines.len())]
                                    .join("\n")
                                    .to_ascii_lowercase();
                                near.contains(&p.to_ascii_lowercase()) || near.contains(&snake(p))
                            })
                    })
                })
            }
        }
    }
}

// ---- the checks ----

#[test]
fn every_contract_item_is_tested_or_exempted() {
    let doc = current();
    let cov = coverage();
    let all = items(&doc).0;
    let tests = Tests::load();
    let baseline: BTreeSet<&str> = cov
        .group
        .iter()
        .filter(|g| g.baseline)
        .flat_map(|g| g.items.iter().map(String::as_str))
        .collect();
    let others: Vec<&str> = cov
        .group
        .iter()
        .filter(|g| !g.baseline)
        .flat_map(|g| g.items.iter().map(String::as_str))
        .chain(cov.exempt.iter().map(|e| e.item.as_str()))
        .collect();
    let mut errors = vec![];
    for g in &cov.group {
        if g.reason.trim().is_empty() {
            errors.push("a [[group]] has no reason".to_string());
        }
    }
    for e in &cov.exempt {
        if e.reason.trim().is_empty() {
            errors.push(format!("exempt {:?} has no reason", e.item));
        }
    }

    let mut uncovered = vec![];
    let mut waiting: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut matched: BTreeSet<&str> = BTreeSet::new();
    for (key, item) in &all {
        let pending = cov.pending.iter().find(|p| glob(&p.item, key));
        let exempt = others.iter().find(|p| glob(p, key));
        for p in pending
            .map(|p| p.item.as_str())
            .into_iter()
            .chain(exempt.copied())
        {
            matched.insert(p);
        }
        if pending.is_none() && exempt.is_none() && baseline.contains(key.as_str()) {
            continue;
        }
        match (tests.covers(&item.rule), pending, exempt) {
            (true, Some(p), _) => errors.push(format!(
                "{key} is pending ({}) but a test names it now: delete its [[pending]] entry {:?}",
                p.pending, p.item
            )),
            (true, None, Some(e)) => errors.push(format!(
                "{key} is exempted but a test names it now: delete the entry {e:?}"
            )),
            (true, None, None) => {}
            (false, Some(p), _) => {
                *waiting
                    .entry((p.pending.as_str(), p.item.as_str()))
                    .or_default() += 1
            }
            (false, None, Some(_)) => {}
            (false, None, None) if item.at.is_empty() => uncovered.push(format!("  {key}")),
            (false, None, None) => uncovered.push(format!("  {key}   ({})", item.at)),
        }
    }
    for p in cov
        .pending
        .iter()
        .map(|p| p.item.as_str())
        .chain(others.iter().copied())
    {
        if !matched.contains(p) {
            errors.push(format!(
                "{p:?} in contract-coverage.toml matches nothing in the contract: delete it"
            ));
        }
    }
    for ((who, pattern), n) in &waiting {
        eprintln!("warning: pending, no test names it yet ({who}): {pattern} ({n} matching)");
    }
    if !uncovered.is_empty() {
        errors.push(format!(
            "{} items in openapi-current.json (contract {}, studio v{}) are named by no test and not listed in contract-coverage.toml:\n{}\nFor each, {HINT}.",
            uncovered.len(),
            cov.target.contract,
            cov.target.studio,
            uncovered.join("\n")
        ));
    }
    assert!(
        errors.is_empty(),
        "contract coverage:\n{}",
        errors.join("\n")
    );
}

/// `[target]` names the studio Burrow follows; the vendored contract is
/// that studio's, and the docs name the same pair. With `BLYG_WORKER_DIR`
/// set to a studio checkout (as `scripts/e2e-local.sh` takes it), the file
/// must be that studio's `openapi.json` at tag `v{studio}`, byte for byte.
#[test]
fn the_vendored_contract_is_the_targets() {
    let cov = coverage();
    let Target { studio, contract } = &cov.target;
    let doc = current();
    assert_eq!(
        doc["info"]["version"].as_str(),
        Some(contract.as_str()),
        "openapi-current.json is not contract {contract} (studio v{studio}, contract-coverage.toml [target]): vendor that studio's openapi.json, or move [target]"
    );
    let pair = format!("{contract}, studio v{studio}");
    for doc in ["../../docs/SPEC.md", "tests/fixtures/README.md"] {
        let text = std::fs::read_to_string(manifest().join(doc)).unwrap();
        assert!(
            text.contains(&pair),
            "{doc} should name the target studio as \"{pair}\""
        );
    }
    let Some(dir) = std::env::var_os("BLYG_WORKER_DIR") else {
        return;
    };
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["show", &format!("v{studio}:openapi.json")])
        .output()
        .unwrap();
    if !out.status.success() {
        eprintln!("skipped: no tag v{studio} in BLYG_WORKER_DIR");
        return;
    }
    assert!(
        out.stdout == include_bytes!("../fixtures/openapi-current.json"),
        "openapi-current.json differs from studio v{studio}'s openapi.json: copy it over"
    );
}

const BEGIN: &str = "# BEGIN baseline";
const END: &str = "# END baseline";

/// Rewrites the toml's baseline group from another contract:
/// `BLYG_BASELINE_CONTRACT=<openapi.json> BLYG_BASELINE_STUDIO=0.61.0
/// cargo test -p blyg-core --test it contract_coverage::regenerate_baseline -- --ignored`
#[test]
#[ignore = "rewrites contract-coverage.toml; run by hand (tests/fixtures/README.md)"]
fn regenerate_baseline() {
    let path =
        std::env::var("BLYG_BASELINE_CONTRACT").expect("BLYG_BASELINE_CONTRACT=<openapi.json>");
    let studio =
        std::env::var("BLYG_BASELINE_STUDIO").expect("BLYG_BASELINE_STUDIO=<studio version>");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let version = doc["info"]["version"].as_str().unwrap();
    let mut block = format!(
        "{BEGIN} (generated by contract_coverage::regenerate_baseline; don't edit)\n[[group]]\nbaseline = true\nreason = \"In the contract at studio v{studio} (contract {version}), before this check: synced by review, not by a test list.\"\nitems = [\n"
    );
    for key in items(&doc).0.keys() {
        block.push_str(&format!("  {},\n", toml::Value::String(key.clone())));
    }
    block.push_str("]\n");
    block.push_str(END);
    let text = std::fs::read_to_string(toml_path()).unwrap();
    let start = text.find(BEGIN).expect("BEGIN marker");
    let end = text.find(END).expect("END marker") + END.len();
    std::fs::write(
        toml_path(),
        format!("{}{block}{}", &text[..start], &text[end..]),
    )
    .unwrap();
}

#[test]
fn the_matchers_read_tests_as_meant() {
    assert!(glob("header:*:ETag", "header:getItem:ETag"));
    assert!(!glob("header:*:ETag", "header:getItem:ETags"));
    assert!(glob("a*b*c", "a-x-b-y-c") && !glob("a*b*c", "a-c"));
    assert_eq!(snake("tangledFrom"), "tangled_from");
    assert_eq!(snake("ifNoneMatch"), "if_none_match");
    assert!(
        has_word("x.tangled_from;", "tangled_from") && !has_word("x.tangled_from2", "tangled_from")
    );
    let segs = |p: &str| p.split('/').map(str::to_string).collect::<Vec<_>>();
    assert!(has_path(
        "\"/api/subscriptions/abc\"",
        &segs("api/subscriptions/{id}")
    ));
    assert!(!has_path(
        "\"/api/subscriptions/abc/refresh\"",
        &segs("api/subscriptions/{id}")
    ));
    assert!(has_path(
        "format!(\"/api/items/{id}/publish\")",
        &segs("api/items/{id}/publish")
    ));
    let t = Tests {
        files: vec!["let m = mention();\nassert_eq!(m.relation, \"link\");".into()],
        lower: vec![],
    };
    assert!(t.covers(&Rule::Enum("link".into(), Some("relation".into()))));
    let far = Tests {
        files: vec!["\"link\"\n\n\n\n\nrelation".into()],
        lower: vec![],
    };
    assert!(!far.covers(&Rule::Enum("link".into(), Some("relation".into()))));
    let inline = "fn a() {}\n#[cfg(test)]\nfn helper() {}\n#[cfg(test)]\nmod tests {\n}\n";
    assert_eq!(inline_test_mod(inline), inline.find("#[cfg(test)]\nmod"));
}
