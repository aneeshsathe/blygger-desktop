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
//!   field (`tangled_from` for `tangledFrom`) when that differs. A name that
//!   is one plain lowercase word (`why`, `raw`, `id`) is too common for that:
//!   the `"why"` of an unrelated JSON row once counted as
//!   `SpellcheckIssue.why`, so its honest exemption failed as stale and was
//!   deleted. Such a name needs its quoted form within 6 lines of the schema
//!   or operation it lives in (its name or snake_case, each as a whole
//!   identifier: `AgenticEditionInput` doesn't name `AgenticEdition`; or an
//!   operation's path) or of a sibling property that is itself distinctive
//!   (not a plain word of under 5 letters, nor a common one in `COMMON`:
//!   `origin`, `version` and `id` sit together in unrelated rows);
//! - `enum:{where}={value}`: a test holds `"value"` within 3 lines of the
//!   property or parameter it belongs to (a bare `"link"` is too common);
//! - `param:{operationId}/{in}:{name}`: a test holds `"name"` in the
//!   contract's own case, or, for a query parameter, sends it in a query
//!   string (`?name=` or `&name=`). Case matters: the Raw tab's label `"Raw"`
//!   once counted as the query parameter `raw`, which Burrow never sends, so
//!   the parameter's honest exemption failed and was deleted;
//! - `header:{operationId}:{name}` (response headers) and a parameter sent in
//!   a header: a test holds `"name"`, any case, as HTTP reads header names.
//!
//! A parameter isn't asked to sit near its route, as an enum value is near
//! its property: requests are built by helpers, so a test seldom names both
//! within a few lines (at studio 0.69 that rule left nearly every checked
//! name uncovered, this one only the two `raw`s).
//!
//! A covered item says where and by what (`Tests::why`: file, line and the
//! anchor that counted), and a stale entry's message prints it: twice an honest
//! exemption was nearly deleted because the message only said "stale", when
//! the cover was a coincidence in some other test.
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

/// Lines either side of a plain-word property's name that may hold its anchor.
const WINDOW: usize = 6;
/// A sibling property anchors a plain-word property unless it is itself a
/// plain lowercase word shorter than this, or one of `COMMON`.
const MIN_SIBLING: usize = 5;

/// Plain words too common as JSON keys to vouch for a sibling: they sit
/// together in unrelated rows (a reading row's `origin`, `version`, `source`,
/// a lineage node's `partial`, `depth`), which once made
/// `CatchUp.cards[].id` look tested.
const COMMON: &[&str] = &[
    "origin", "version", "source", "label", "title", "author", "created", "updated", "content",
    "status", "items", "cursor", "count", "value", "partial", "depth",
];

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
    Property(Prop),
    /// value, and the property or parameter it belongs to.
    Enum(String, Option<String>),
    /// A header name (a response header, or a parameter sent in a header),
    /// any case.
    Name(String),
    /// A path or query parameter's name, in the contract's case; `true` for
    /// a query parameter, which a query string in a test also covers.
    Param(String, bool),
}

/// A schema property, with the places that vouch for a plain-word name.
#[derive(Debug, Clone)]
struct Prop {
    name: String,
    /// The schema's or operation's name: the first segment of `{where}`.
    owner: String,
    /// The owner's path, when it is an operation.
    path: Option<Vec<String>>,
    /// The other properties of the same parent object.
    siblings: Vec<String>,
}

/// One plain lowercase word: a bare quoted string of it is too common to
/// prove anything.
fn plain_word(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase())
}

impl Rule {
    fn property(name: &str, owner: &str, path: Option<Vec<String>>, siblings: &[&str]) -> Rule {
        Rule::Property(Prop {
            name: name.to_string(),
            owner: owner.to_string(),
            path,
            siblings: siblings.iter().map(|s| s.to_string()).collect(),
        })
    }
}

impl Prop {
    fn plain(&self) -> bool {
        plain_word(&self.name)
    }

    /// Siblings whose names say something about where this property is.
    fn distinctive_siblings(&self) -> impl Iterator<Item = &str> {
        self.siblings
            .iter()
            .map(String::as_str)
            .filter(|s| !(plain_word(s) && (s.len() < MIN_SIBLING || COMMON.contains(s))))
    }

    /// What a test must hold for an uncovered plain-word name, for the
    /// failure message.
    fn hint(&self) -> Option<String> {
        if !self.plain() {
            return None;
        }
        let mut near = format!("{} or {}", self.owner, snake(&self.owner));
        if self.path.is_some() {
            near.push_str(", or its path");
        }
        let siblings: Vec<String> = self
            .distinctive_siblings()
            .map(|s| format!("\"{s}\""))
            .collect();
        if !siblings.is_empty() {
            near.push_str(&format!(", or the sibling {}", siblings.join(" or ")));
        }
        Some(format!(
            "\"{}\" is a plain word, so a test must hold it quoted within {WINDOW} lines of {near}",
            self.name
        ))
    }

    /// `window` (a few lines of one test file) names a place this property
    /// lives in; says which.
    fn anchor_in(&self, window: &str) -> Option<String> {
        let lower = window.to_ascii_lowercase();
        if has_word(&lower, &self.owner.to_ascii_lowercase())
            || has_word(&lower, &snake(&self.owner))
        {
            return Some(format!("owner {}", self.owner));
        }
        if let Some(i) = self.path.as_ref().and_then(|p| path_at(window, p)) {
            let found = window[i..]
                .split(|c: char| matches!(c, '"' | '?' | '#') || c.is_whitespace())
                .next()
                .unwrap_or("");
            return Some(format!("path {found}"));
        }
        self.distinctive_siblings()
            .find(|s| {
                window.contains(&format!("\"{s}\""))
                    || (snake(s) != *s && has_word(window, &snake(s)))
            })
            .map(|s| format!("sibling \"{s}\""))
    }

    /// The first line (1-based) of `text` that covers this property, and the
    /// anchor that made it count.
    fn cover_in(&self, text: &str) -> Option<(usize, String)> {
        let quoted = format!("\"{}\"", self.name);
        if !self.plain() {
            let snake = snake(&self.name);
            if let Some(i) = text.find(&quoted) {
                return Some((line_of(text, i), "the bare quoted name".into()));
            }
            return word_at(text, &snake)
                .filter(|_| snake != self.name)
                .map(|i| (line_of(text, i), format!("the bare name {snake}")));
        }
        let lines: Vec<&str> = text.lines().collect();
        lines.iter().enumerate().find_map(|(i, l)| {
            let near = &lines[i.saturating_sub(WINDOW)..(i + WINDOW + 1).min(lines.len())];
            if !l.contains(&quoted) {
                return None;
            }
            let anchor = self.anchor_in(&near.join("\n"))?;
            Some((i + 1, format!("\"{}\" near {anchor}", self.name)))
        })
    }
}

#[derive(Debug, Clone)]
struct Item {
    rule: Rule,
    /// The route it's on, for the failure message (empty for schemas).
    at: String,
}

/// The items by key, and the path of the operation being walked (the owner
/// of its schemas' properties; `None` in `components`).
#[derive(Default)]
struct Items(BTreeMap<String, Item>, Option<Vec<String>>);

impl Items {
    fn add(&mut self, key: String, rule: Rule, at: String) {
        self.0.entry(key).or_insert(Item { rule, at });
    }

    fn schema(&mut self, s: &Value, at: &str, parent: Option<&str>) {
        match s {
            Value::Array(list) => list.iter().for_each(|x| self.schema(x, at, parent)),
            Value::Object(o) => {
                if let Some(Value::Object(props)) = o.get("properties") {
                    let owner = at.split(['.', '/', '[']).next().unwrap_or(at);
                    for (name, sub) in props {
                        let here = format!("{at}.{name}");
                        let siblings: Vec<&str> = props
                            .keys()
                            .map(String::as_str)
                            .filter(|k| k != name)
                            .collect();
                        self.add(
                            format!("property:{here}"),
                            Rule::property(name, owner, self.1.clone(), &siblings),
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
            let segs: Vec<String> = path
                .trim_start_matches('/')
                .split('/')
                .map(str::to_string)
                .collect();
            out.1 = Some(segs.clone());
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
                let rule = match place {
                    "header" => Rule::Name(name.to_string()),
                    _ => Rule::Param(name.to_string(), place == "query"),
                };
                out.add(format!("param:{at}"), rule, route.clone());
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
    out.1 = None;
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

/// Where `word` is in `text` with no identifier character on either side.
fn word_at(text: &str, word: &str) -> Option<usize> {
    text.match_indices(word)
        .find(|(i, _)| {
            !text[..*i].chars().next_back().is_some_and(ident_char)
                && !text[i + word.len()..]
                    .chars()
                    .next()
                    .is_some_and(ident_char)
        })
        .map(|x| x.0)
}

fn has_word(text: &str, word: &str) -> bool {
    word_at(text, word).is_some()
}

/// The 1-based line of byte offset `at` in `text`.
fn line_of(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
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
    path_at(text, segs).is_some()
}

/// Where the path starts in `text` (at its leading `/`).
fn path_at(text: &str, segs: &[String]) -> Option<usize> {
    let first = format!("/{}/", segs[0]);
    text.match_indices(&first).map(|x| x.0).find(|&i| {
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

/// Where a query string in a test sends `name`: `?name=` or `&name=`.
fn query_at(text: &str, name: &str) -> Option<usize> {
    let sent = format!("{name}=");
    text.match_indices(&sent)
        .map(|x| x.0)
        .find(|&i| text[..i].ends_with(['?', '&']))
}

struct Tests {
    /// Each file's path from the repo root (`crates/…`, `/`-separated), for
    /// `why`; a placeholder for `of`.
    names: Vec<String>,
    files: Vec<String>,
    lower: Vec<String>,
}

impl Tests {
    fn load() -> Tests {
        let crates = manifest().join("..");
        let (names, files) = test_sources()
            .into_iter()
            .map(|(path, text)| {
                let rel = path.strip_prefix(&crates).unwrap_or(&path);
                let parts: Vec<_> = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect();
                (format!("crates/{}", parts.join("/")), text)
            })
            .unzip();
        Tests::named(names, files)
    }

    fn of(files: Vec<String>) -> Tests {
        Tests::named(vec!["<test>".to_string(); files.len()], files)
    }

    fn named(names: Vec<String>, files: Vec<String>) -> Tests {
        let lower = files.iter().map(|t| t.to_ascii_lowercase()).collect();
        Tests {
            names,
            files,
            lower,
        }
    }

    fn covers(&self, rule: &Rule) -> bool {
        self.why(rule).is_some()
    }

    /// Where and by what `rule` is covered: `file:line (what counted)`.
    fn why(&self, rule: &Rule) -> Option<String> {
        self.files.iter().enumerate().find_map(|(n, t)| {
            let (line, what) = match rule {
                Rule::Operation(id, segs) => {
                    let i = word_at(t, id).or_else(|| path_at(t, segs))?;
                    let found = t[i..]
                        .split(|c: char| matches!(c, '"' | '?' | '#') || c.is_whitespace())
                        .next()
                        .unwrap_or("");
                    (line_of(t, i), format!("the operation's id or path {found}"))
                }
                Rule::Property(p) => p.cover_in(t)?,
                Rule::Name(name) => {
                    let i = self.lower[n].find(&format!("\"{}\"", name.to_ascii_lowercase()))?;
                    (line_of(t, i), "the quoted header name".to_string())
                }
                Rule::Param(name, query) => {
                    let quoted = format!("\"{name}\"");
                    let i = t
                        .find(&quoted)
                        .or_else(|| query_at(t, name).filter(|_| *query))?;
                    (line_of(t, i), "the parameter name".to_string())
                }
                Rule::Enum(value, parent) => {
                    let quoted = format!("\"{value}\"");
                    let lines: Vec<&str> = t.lines().collect();
                    let i = lines.iter().enumerate().position(|(i, l)| {
                        l.contains(&quoted)
                            && parent.as_ref().is_none_or(|p| {
                                let near = lines[i.saturating_sub(3)..(i + 4).min(lines.len())]
                                    .join("\n")
                                    .to_ascii_lowercase();
                                near.contains(&p.to_ascii_lowercase()) || near.contains(&snake(p))
                            })
                    })?;
                    (i + 1, "the value near its property".to_string())
                }
            };
            Some(format!("{}:{line} ({what})", self.names[n]))
        })
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
                "{key} is pending ({}) and is now covered by {}: delete its [[pending]] entry {:?}, or if that line is about something else, this is a false cover: report it instead of deleting the entry",
                p.pending,
                tests.why(&item.rule).unwrap_or_default(),
                p.item
            )),
            (true, None, Some(e)) => errors.push(format!(
                "exempt {key:?} is now covered by {}: delete the entry {e:?}, or if that line is about something else, this is a false cover: report it instead of deleting the entry",
                tests.why(&item.rule).unwrap_or_default()
            )),
            (true, None, None) => {}
            (false, Some(p), _) => {
                *waiting
                    .entry((p.pending.as_str(), p.item.as_str()))
                    .or_default() += 1
            }
            (false, None, Some(_)) => {}
            (false, None, None) => {
                let mut line = format!("  {key}");
                if !item.at.is_empty() {
                    line.push_str(&format!("   ({})", item.at));
                }
                if let Rule::Property(p) = &item.rule
                    && let Some(hint) = p.hint()
                {
                    line.push_str(&format!("\n      {hint}"));
                }
                uncovered.push(line)
            }
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

/// The contract Burrow targets is one its own guard accepts. Commit 2bc36e8
/// ("Target studio v0.68.0 (contract 6.1.0)", release 0.28.0's first)
/// vendored contract 6.1.0 while `api::contract` accepted major 5 only:
/// every test passed, and the released Burrow refused every 0.67+ studio
/// (hotfix 0.27.3 widened the guard).
#[test]
fn burrow_accepts_the_vendored_contract() {
    let doc = current();
    let version = doc["info"]["version"]
        .as_str()
        .expect("openapi-current.json has info.version");
    let verdict = blyg_core::api::contract(Some(version));
    assert!(
        verdict == blyg_core::api::Contract::Supported,
        "openapi-current.json is contract {version}, which blyg_core::api::contract refuses ({verdict:?}): Burrow wouldn't sync with the studio it targets. Widen the guard (CONTRACT_MAJOR_NEWEST in crates/blyg-core/src/api/mod.rs) in the change that vendors the contract"
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
    let t = Tests::of(vec![
        "let m = mention();\nassert_eq!(m.relation, \"link\");".into(),
    ]);
    assert!(t.covers(&Rule::Enum("link".into(), Some("relation".into()))));
    let far = Tests::of(vec!["\"link\"\n\n\n\n\nrelation".into()]);
    assert!(!far.covers(&Rule::Enum("link".into(), Some("relation".into()))));
    // The past false cover: the Raw tab's label isn't the `raw` parameter.
    let raw = Rule::Param("raw".into(), true);
    let label = Tests::of(vec!["assert_eq!(tabs[4].label, \"Raw\");".into()]);
    assert!(!label.covers(&raw));
    let sent = Tests::of(vec!["api.get(\"/api/reading?limit=5&raw=true\")".into()]);
    assert!(sent.covers(&raw));
    assert!(Tests::of(vec!["(\"/api/reading?raw=true\")".into()]).covers(&raw));
    assert!(Tests::of(vec!["query.push((\"raw\", \"true\"));".into()]).covers(&raw));
    // Not a query string, and not this parameter's name.
    assert!(!Tests::of(vec!["let raw=true; \"/x?draw=1&raws=2\"".into()]).covers(&raw));
    // A path parameter is never read out of a query string.
    assert!(!sent.covers(&Rule::Param("raw".into(), false)));
    // Header names match in any case, as HTTP reads them.
    let etag = Tests::of(vec!["resp.header(\"etag\")".into()]);
    assert!(etag.covers(&Rule::Name("ETag".into())));
    assert!(!etag.covers(&Rule::Param("ETag".into(), true)));
    let inline = "fn a() {}\n#[cfg(test)]\nfn helper() {}\n#[cfg(test)]\nmod tests {\n}\n";
    assert_eq!(inline_test_mod(inline), inline.find("#[cfg(test)]\nmod"));
}

/// `SpellcheckIssue.why`, whose siblings in the contract are `find` and
/// `replace`.
fn issue_why() -> Rule {
    Rule::property("why", "SpellcheckIssue", None, &["find", "replace"])
}

#[test]
fn a_plain_word_property_needs_an_anchor_near_its_name() {
    // The past false cover: tests on another branch (web-restore-split) read
    // an unrelated JSON key "why", so the exempted `SpellcheckIssue.why`
    // counted as covered, failed as stale, and its exemption was deleted.
    let other_keys = Tests::of(vec![
        "    let rows = view[\"rows\"].as_array().unwrap();\n    // Stalest first: z (not held here), c (changed here), p.\n    assert_eq!(rows[0][\"sid\"], \"z000000000000000000000000z\");\n    assert_eq!(rows[0][\"why\"], \"This thread isn't in this browser yet\");\n    assert_eq!(\n        rows[1][\"why\"],\n        \"It has unpublished edits: publish or discard them first\"\n    );\n    assert_eq!(rows[2][\"why\"], Value::Null);\n    assert_eq!(rows[2][\"line\"], \"2 versions behind\");\n".into(),
        "    assert_eq!(x[\"on\"], true);\n    assert_eq!(x[\"placement\"], \"background\");\n    assert_eq!(x[\"why\"], \"you moved this\");\n    assert_eq!(v(b.wr_bg(\"b|b1\"))[\"why\"], \"blyg in Background\");\n".into(),
        "\"kind\": \"pending\", \"why\": why, \"heading\": \"Changelog\", \"rows\": rows(&lines),\n".into(),
    ]);
    assert!(!other_keys.covers(&issue_why()));
    // The name within 6 lines of the schema's name (any case, or snake_case).
    let near = Tests::of(vec![
        "let i = SpellcheckIssue::default();\n\n\nassert_eq!(j[\"why\"], \"x\");".into(),
    ]);
    assert!(near.covers(&issue_why()));
    let snake_near = Tests::of(vec!["fn spellcheck_issue() {\n    j[\"why\"]\n}".into()]);
    assert!(snake_near.covers(&issue_why()));
    // Or next to a distinctive sibling.
    let sibling = Tests::of(vec!["json!({ \"replace\": r, \"why\": w })".into()]);
    assert!(sibling.covers(&issue_why()));
    // A short plain sibling proves nothing.
    let short = Tests::of(vec!["json!({ \"id\": 1, \"find\": f, \"why\": w })".into()]);
    assert!(!short.covers(&issue_why()));
    // 6 lines either side count; 7 don't.
    let gap = |n: usize| format!("SpellcheckIssue\n{}\"why\"", "\n".repeat(n));
    assert!(Tests::of(vec![gap(5)]).covers(&issue_why()));
    assert!(!Tests::of(vec![gap(6)]).covers(&issue_why()));
    let after = format!("\"why\"\n{}SpellcheckIssue", "\n".repeat(5));
    assert!(Tests::of(vec![after]).covers(&issue_why()));
    let far_after = format!("\"why\"\n{}SpellcheckIssue", "\n".repeat(6));
    assert!(!Tests::of(vec![far_after]).covers(&issue_why()));
    // A distinctive name keeps the bare rule.
    let camel = Rule::property("ifNoneMatch", "Conditional", None, &["other"]);
    assert!(Tests::of(vec!["h.get(\"ifNoneMatch\")".into()]).covers(&camel));
    assert!(Tests::of(vec!["x.if_none_match".into()]).covers(&camel));
}

/// A property of a schema whose name is another's prefix: a test about
/// `AgenticEditionInput` doesn't anchor `AgenticEdition`'s properties.
#[test]
fn an_owner_anchors_only_as_a_whole_identifier() {
    let parent = |owner: &str| Rule::property("why", owner, None, &["find", "replace"]);
    for text in [
        "let i = AgenticEditionInput::default();\nassert_eq!(j[\"why\"], \"x\");",
        "fn agentic_edition_input() {\n    j[\"why\"]\n}",
    ] {
        let t = Tests::of(vec![text.into()]);
        assert!(!t.covers(&parent("AgenticEdition")), "{text}");
        assert!(t.covers(&parent("AgenticEditionInput")), "{text}");
    }
    let own = Tests::of(vec![
        "let i = AgenticEdition::default();\nassert_eq!(j[\"why\"], \"x\");".into(),
        "fn agentic_edition() {\n    j[\"why\"]\n}".into(),
    ]);
    assert!(own.covers(&parent("AgenticEdition")));
    assert!(!own.covers(&parent("AgenticEditionInput")));
}

/// Real lines of `crates/blyg-ext/tests/reading.rs` (a reading row) hold
/// `"id"`, `"origin"` and `"version"`, common words that sit together in
/// many unrelated rows: they don't make `CatchUp.cards[].id` or `.origin`
/// tested.
#[test]
fn common_word_siblings_do_not_anchor() {
    let all = items(&current()).0;
    let reading_row = Tests::of(vec![
        "        \"origin\": \"https://blyg.example.com\", \"kind\": \"thread\", \"state\": \"current\",\n        \"version\": version, \"created\": \"2026-10-01T09:00:00Z\", \"updated\": null,\n        \"observed_at\": \"2026-10-01T09:05:00Z\",\n        \"content_md\": md, \"content_html\": format!(\"<p>{md}</p>\"),\n        \"author\": {\"name\": \"Rue\", \"url\": null}, \"page\": null, \"thumb\": null, \"hoppers\": [],\n        \"stub_of\": {\"origin\": \"https://other.example.com\", \"id\": \"p7\", \"version\": 2},\n".into(),
    ]);
    for name in ["id", "origin", "version"] {
        let key = format!("property:CatchUp.cards[].{name}");
        assert!(!reading_row.covers(&all[&key].rule), "{key}");
    }
}

/// Real lines of another branch's `crates/blyg-web` tests, which read a JSON
/// card's `html` with `"origin"`, `"id"`, `"version"`, `"contentHtml"` and
/// `/api/peek` nearby, were suspected of covering the exempted
/// `CatchUp.cards[].html` (Burrow never calls catch-up). A card is a row of
/// `id`, `origin`, `sub`, `version`, `source`, `at`, `depth`, `partial`,
/// `label` and `html`: none of those anchors `html`, and no operation path is
/// its owner (a schema in `components` has none).
#[test]
fn another_posts_html_is_not_the_catch_up_cards() {
    let all = items(&current()).0;
    let html = &all["property:CatchUp.cards[].html"].rule;
    let Rule::Property(card) = html else {
        panic!("not a property")
    };
    assert_eq!(card.owner, "CatchUp");
    assert!(card.path.is_none(), "a components schema has no operation");
    let peek = "    let r = v(b.wr_do(\"peek\", &args, NOW));\n    assert_eq!(\n        r[\"req\"][\"path\"],\n        \"/api/peek?origin=https%3A%2F%2Ffar.example.org%2F&id=X1\"\n    );\n    let p = v(b.wr_done(\n        \"peek\",\n        &args,\n        200,\n        r#\"{\"origin\":\"https://far.example.org/\",\"id\":\"X1\",\"kind\":\"thread\",\"version\":2,\n            \"contentHtml\":\"<h1>Far away</h1><p>Hello <script>x()</script>there.</p>\",\n            \"page\":null,\"url\":\"https://far.example.org/t/X1/\",\"stubOf\":null,\"forkedFrom\":null,\"subscriptionId\":null}\"#,\n        NOW,\n    ));\n    assert_eq!(p[\"peek\"][\"title\"], \"Far away\");\n    assert!(!p[\"peek\"][\"html\"].as_str().unwrap().contains(\"script\"));\n    assert_eq!(p[\"peek\"][\"followed\"], false);\n    assert_eq!(p[\"fire\"][\"path\"], \"/api/browse\");\n";
    let opened = "    let post = v(b.open_reading(\"sub1|md1\", NOW));\n    let html = post[\"html\"].as_str().unwrap();\n    // A relative link would point at the owner's own blyg.\n    assert!(\n        html.contains(\"href=\\\"https://notes.example.com/api/items\\\"\"),\n";
    let post = "    let post: Value = serde_json::from_str(&b.open_reading(&unread_keys[1], NOW)).unwrap();\n    assert!(!post[\"html\"].as_str().unwrap().is_empty());\n    assert!(post[\"by\"].as_str().unwrap().contains(\" · v\"), \"{post}\");\n";
    for text in [peek, opened, post] {
        assert!(!Tests::of(vec![text.into()]).covers(html), "{text}");
    }
    // Still covered where the catch-up card is named.
    let named = "let c = CatchUp::default();\nassert!(c[\"cards\"][0][\"html\"].is_string());";
    assert!(Tests::of(vec![named.into()]).covers(html));
    let snake = "fn catch_up() {\n    c[\"html\"]\n}";
    assert!(Tests::of(vec![snake.into()]).covers(html));
}

/// A covered item says where and by what, so a stale `[[exempt]]` can be told
/// from a false cover at a glance.
#[test]
fn a_cover_says_where_and_by_what() {
    let at = |files: Vec<&str>, rule: &Rule| {
        Tests::named(
            vec!["crates/x/tests/a.rs".into(); files.len()],
            files.into_iter().map(String::from).collect(),
        )
        .why(rule)
    };
    assert_eq!(
        at(
            vec!["x\n\nlet i = SpellcheckIssue::default();\nj[\"why\"]"],
            &issue_why()
        )
        .as_deref(),
        Some("crates/x/tests/a.rs:4 (\"why\" near owner SpellcheckIssue)")
    );
    assert_eq!(
        at(
            vec!["", "json!({ \"replace\": r,\n \"why\": w })"],
            &issue_why()
        )
        .as_deref(),
        Some("crates/x/tests/a.rs:2 (\"why\" near sibling \"replace\")")
    );
    let by_path = Rule::property(
        "why",
        "listThings",
        Some(vec!["api".into(), "op".into()]),
        &[],
    );
    assert_eq!(
        at(vec!["get(\"/api/op\");\nj[\"why\"]"], &by_path).as_deref(),
        Some("crates/x/tests/a.rs:2 (\"why\" near path /api/op)")
    );
    let camel = Rule::property("ifNoneMatch", "Conditional", None, &[]);
    assert_eq!(
        at(vec!["\n\nh.get(\"ifNoneMatch\")"], &camel).as_deref(),
        Some("crates/x/tests/a.rs:3 (the bare quoted name)")
    );
    assert_eq!(at(vec!["j[\"why\"]"], &issue_why()), None);
}
