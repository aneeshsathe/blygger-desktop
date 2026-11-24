//! A list read from a studio whose contract is newer than Burrow knows
//! removes nothing held here (`api::Prune`, `sync/newer.rs`). The store
//! function that replaced the subscriptions table deleted every row and
//! took no such decision, so three callers in `live.rs` (refresh,
//! subscribe, rename) wiped the table from a served list, past the policy.
//!
//! This scan reads the product sources of the workspace's crates and finds
//! each function that removes rows of a list table: one that runs `DELETE
//! FROM <table>`, and, when such a function is private, each function near
//! it (its directory) that calls it. Every one of them must do one of:
//!
//! - name the decision (`Prune` or `lists_prune` in its signature or body);
//! - be private, so the functions that call it answer instead;
//! - have a line in `tests/fixtures/list-prune-allow.txt` saying why it may
//!   remove without asking (a removal the user made here, say).
//!
//! It reads names, not types: it can't tell that a function naming `Prune`
//! acts on it. It refuses the shape that went wrong, a removal anyone can
//! call with no decision in sight.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The tables a blyg's lists fill here.
const TABLES: &[&str] = &[
    "subscriptions",
    "hoppers",
    "hopper_items",
    "reading",
    "items",
    "versions",
    "shelf",
];

const ALLOW_FILE: &str = "crates/blyg-core/tests/fixtures/list-prune-allow.txt";

/// A function: `(file under crates/, name)`.
type Key = (String, String);

struct Func {
    /// Any `pub`, `pub(crate)` included: callable from outside its module.
    public: bool,
    /// Its text from the `fn` line to the next one, comments cut.
    body: String,
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// A `fn` line: whether it's public, and where its name is in `line`.
fn header(line: &str) -> Option<(bool, std::ops::Range<usize>)> {
    let mut rest = line.trim_start();
    let mut public = false;
    if let Some(r) = rest.strip_prefix("pub") {
        let r = match r.strip_prefix('(') {
            Some(inner) => &inner[inner.find(')')? + 1..],
            None => r,
        };
        if !r.starts_with(' ') {
            return None;
        }
        public = true;
        rest = r.trim_start();
    }
    for word in ["const ", "async ", "unsafe "] {
        if let Some(r) = rest.strip_prefix(word) {
            rest = r.trim_start();
        }
    }
    let name = rest.strip_prefix("fn ")?.trim_start();
    let len = name.chars().take_while(|c| is_ident(*c)).count();
    if len == 0 {
        return None;
    }
    let start = line.len() - name.len();
    Some((public, start..start + len))
}

/// The functions of one file, by name (a method and a free function of one
/// name are read as one). Stops at an inline `#[cfg(test)] mod … {`.
fn functions(text: &str) -> BTreeMap<String, Func> {
    let mut out: BTreeMap<String, Func> = BTreeMap::new();
    let mut current: Option<String> = None;
    let lines: Vec<&str> = text.lines().collect();
    for (n, raw) in lines.iter().enumerate() {
        let inline_tests = raw.starts_with("#[cfg(test)]")
            && lines
                .get(n + 1)
                .is_some_and(|l| l.starts_with("mod ") && l.trim_end().ends_with('{'));
        if inline_tests {
            break;
        }
        let mut line = match raw.find("//") {
            Some(i) => raw[..i].to_string(),
            None => raw.to_string(),
        };
        if let Some((public, name)) = header(&line) {
            let key = line[name.clone()].to_string();
            let f = out.entry(key.clone()).or_insert(Func {
                public: false,
                body: String::new(),
            });
            f.public |= public;
            current = Some(key);
            // Its own name isn't a call of it.
            line.replace_range(name, "");
        }
        if let Some(f) = current.as_ref().and_then(|k| out.get_mut(k)) {
            f.body.push_str(&line);
            f.body.push('\n');
        }
    }
    out
}

/// The list table `body` deletes from, if any.
fn deletes(body: &str) -> Option<&'static str> {
    const DELETE: &str = "DELETE FROM ";
    let mut from = 0;
    while let Some(i) = body[from..].find(DELETE) {
        let at = from + i + DELETE.len();
        let table: String = body[at..].chars().take_while(|c| is_ident(*c)).collect();
        if let Some(t) = TABLES.iter().copied().find(|t| *t == table) {
            return Some(t);
        }
        from = at;
    }
    None
}

/// Whether `body` calls a function named `name`.
fn calls(body: &str, name: &str) -> bool {
    let mut from = 0;
    while let Some(i) = body[from..].find(name) {
        let at = from + i;
        let end = at + name.len();
        let starts_a_word = !body[..at].chars().next_back().is_some_and(is_ident);
        if starts_a_word && body[end..].trim_start().starts_with('(') {
            return true;
        }
        from = end;
    }
    false
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// What's wrong in `files` (path under `crates/` → source), given the
/// reviewed exceptions `allow`. One message per finding.
fn scan(files: &BTreeMap<String, String>, allow: &BTreeMap<Key, String>) -> Vec<String> {
    let fns: BTreeMap<&str, BTreeMap<String, Func>> = files
        .iter()
        .map(|(p, text)| (p.as_str(), functions(text)))
        .collect();
    // Each function that removes list rows, and how.
    let mut removers: BTreeMap<Key, String> = BTreeMap::new();
    for (path, in_file) in &fns {
        for (name, f) in in_file {
            if let Some(table) = deletes(&f.body) {
                removers.insert(
                    ((*path).to_string(), name.clone()),
                    format!("runs `DELETE FROM {table}`"),
                );
            }
        }
    }
    let mut bad = Vec::new();
    let mut seen: BTreeSet<Key> = BTreeSet::new();
    let mut queue: Vec<Key> = removers.keys().cloned().collect();
    while let Some(key) = queue.pop() {
        if !seen.insert(key.clone()) {
            continue;
        }
        let (path, name) = &key;
        let f = &fns[path.as_str()][name];
        if allow.contains_key(&key) || f.body.contains("Prune") || f.body.contains("lists_prune") {
            continue;
        }
        let how = removers[&key].clone();
        if f.public {
            let named_in: Vec<String> = fns
                .iter()
                .flat_map(|(p, in_file)| in_file.iter().map(move |(n, g)| (*p, n, g)))
                .filter(|(p, n, g)| (*p, *n) != (path.as_str(), name) && calls(&g.body, name))
                .map(|(p, n, _)| format!("crates/{p} `{n}`"))
                .collect();
            bad.push(format!(
                "crates/{path} `{name}` {how}, and nothing in it says whether the blyg's lists \
                 may remove what's held here. It is `pub`, so any caller can hand it a list the \
                 blyg served. On a contract newer than Burrow knows such a list must remove \
                 nothing (`api::Prune`, `sync/newer.rs`); a function shaped like this let three \
                 callers wipe the subscriptions table. Do one of: take an `api::Prune` argument \
                 (or read `Store::lists_prune()`) and remove only when it is `Prune::Allowed`; \
                 make `{name}` private, so only functions that do that can call it; or, if it \
                 removes only what the user removed here, add the line \
                 `{path} {name} | <why it needs no decision>` to {ALLOW_FILE}. \
                 Functions that name it now: {}.",
                if named_in.is_empty() {
                    "none".to_string()
                } else {
                    named_in.join(", ")
                }
            ));
            continue;
        }
        // Private: whoever calls it, from its directory, removes too.
        let dir = format!("{}/", dir_of(path));
        for (p, in_file) in &fns {
            if !p.starts_with(&dir) {
                continue;
            }
            for (n, g) in in_file {
                let caller = ((*p).to_string(), n.clone());
                if caller != key && calls(&g.body, name) && !removers.contains_key(&caller) {
                    removers.insert(caller.clone(), format!("calls `{name}`, which {how}"));
                    queue.push(caller);
                }
            }
        }
    }
    for (path, name) in allow.keys() {
        if !removers.contains_key(&(path.clone(), name.clone())) {
            bad.push(format!(
                "{ALLOW_FILE} lists `{path} {name}`, but no function of that name in that file \
                 removes list rows now (it was renamed, moved, or no longer deletes). Delete \
                 the line, or correct its path and name."
            ));
        }
    }
    bad.sort();
    bad
}

fn crates_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

/// A file of tests: `*_tests.rs`, `tests.rs`, or under a `tests/` directory.
fn is_test_file(rel: &str) -> bool {
    rel.ends_with("_tests.rs") || rel.ends_with("/tests.rs") || rel.contains("/tests/")
}

/// Every product `.rs` file under `dir`, by its path under `crates/`.
fn product_files(dir: &Path, out: &mut BTreeMap<String, String>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            product_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            let rel = p.strip_prefix(crates_dir()).unwrap();
            let rel = rel.to_string_lossy().replace('\\', "/");
            if !is_test_file(&rel) {
                out.insert(rel, std::fs::read_to_string(&p).unwrap());
            }
        }
    }
}

/// `<path under crates/> <function> | <reason>` lines.
fn parse_allow(text: &str) -> BTreeMap<Key, String> {
    let mut out = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (entry, reason) = line
            .split_once(" | ")
            .unwrap_or_else(|| panic!("list-prune-allow.txt line {}: no ` | <reason>`", n + 1));
        assert!(!reason.trim().is_empty(), "line {}: empty reason", n + 1);
        let (path, name) = entry
            .split_once(' ')
            .unwrap_or_else(|| panic!("line {}: `<path> <function> | <reason>`", n + 1));
        out.insert(
            (path.to_string(), name.trim().to_string()),
            reason.trim().to_string(),
        );
    }
    out
}

#[test]
fn removals_on_a_lists_word_ask_the_prune_decision() {
    // Workspace members only (a leftover crate directory isn't built).
    let manifest: toml::Table = toml::from_str(include_str!("../../../../Cargo.toml")).unwrap();
    let mut files = BTreeMap::new();
    for m in manifest["workspace"]["members"].as_array().unwrap() {
        let src = crates_dir()
            .join(m.as_str().unwrap().trim_start_matches("crates/"))
            .join("src");
        if src.is_dir() {
            product_files(&src, &mut files);
        }
    }
    let allow = parse_allow(include_str!("../fixtures/list-prune-allow.txt"));
    let bad = scan(&files, &allow);
    assert!(
        bad.is_empty(),
        "tests/it/list_prune_scan.rs found {} problem(s):\n\n{}\n",
        bad.len(),
        bad.join("\n\n")
    );
}

fn sources(files: &[(&str, &str)]) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|(p, t)| (p.to_string(), t.to_string()))
        .collect()
}

/// The store's replace as it was when the callers below got past the
/// policy (commit 1639901, `store/mod.rs`).
const OLD_STORE: &str = r#"
impl Store {
    pub fn replace_subscriptions(&self, subs: &[Subscription]) -> Result<bool> {
        if self.subscriptions() == subs {
            return Ok(false);
        }
        let mut c = self.conn();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM subscriptions", [])?;
        tx.commit()?;
        Ok(true)
    }

    pub fn subscriptions(&self) -> Vec<Subscription> {
        vec![]
    }
}
"#;

/// Its three callers that took the served list as it came (commit fe08ad5,
/// `live.rs`, cut to the calls).
const OLD_LIVE: &str = r#"
impl Backend for LiveBackend {
    fn refresh_subscriptions(&self) -> Result<()> {
        let subs = self.e().track(self.e().api.list_subscriptions())?;
        if self.e().store.replace_subscriptions(&subs)? {
            self.e().emit(CoreEvent::ReadingChanged);
        }
        Ok(())
    }

    fn subscribe(&self, url: &str, title: Option<&str>) -> Result<Subscription> {
        let id = self.e().track(self.e().api.subscribe(url, title))?;
        let subs = self.e().api.list_subscriptions().unwrap_or_default();
        self.e().store.replace_subscriptions(&subs)?;
        todo!()
    }

    fn rename_subscription(&self, sub_id: &SubId, title: Option<&str>) -> Result<()> {
        match self.e().api.list_subscriptions() {
            Ok(subs) => {
                self.e().store.replace_subscriptions(&subs)?;
            }
            Err(_) => {}
        }
        Ok(())
    }
}
"#;

#[test]
fn the_scan_refuses_the_replace_that_three_callers_got_past() {
    let files = sources(&[
        ("blyg-core/src/store/mod.rs", OLD_STORE),
        ("blyg-core/src/live.rs", OLD_LIVE),
    ]);
    let bad = scan(&files, &BTreeMap::new());
    assert_eq!(bad.len(), 1, "{bad:#?}");
    let said = &bad[0];
    assert!(
        said.contains("crates/blyg-core/src/store/mod.rs `replace_subscriptions`"),
        "{said}"
    );
    assert!(said.contains("DELETE FROM subscriptions"), "{said}");
    for caller in ["refresh_subscriptions", "subscribe", "rename_subscription"] {
        assert!(
            said.contains(&format!("crates/blyg-core/src/live.rs `{caller}`")),
            "names the caller {caller}: {said}"
        );
    }
}

/// The shape that passes: the raw write private, the served way in taking
/// the decision, a local edit on the reviewed list.
const FIXED_STORE: &str = r#"
impl Store {
    pub fn take_served_subscriptions(&self, subs: &[Subscription], prune: Prune) -> Result<bool> {
        self.write_subscriptions(subs)
    }

    fn write_subscriptions(&self, subs: &[Subscription]) -> Result<bool> {
        tx.execute("DELETE FROM subscriptions", [])?;
        Ok(true)
    }

    pub fn edit_subscription(&self, id: &str) -> Result<()> {
        self.write_subscriptions(&subs)?;
        Ok(())
    }
}
"#;

#[test]
fn a_private_write_passes_when_its_callers_ask_or_are_listed() {
    let files = sources(&[("blyg-core/src/store/mod.rs", FIXED_STORE)]);
    let allow = parse_allow("blyg-core/src/store/mod.rs edit_subscription | an edit made here");
    assert_eq!(scan(&files, &allow), Vec::<String>::new());

    // Without its line, the local edit is a public removal with no decision.
    let bad = scan(&files, &BTreeMap::new());
    assert_eq!(bad.len(), 1, "{bad:#?}");
    assert!(bad[0].contains("`edit_subscription`"), "{}", bad[0]);
    assert!(bad[0].contains("calls `write_subscriptions`"), "{}", bad[0]);
}

#[test]
fn a_private_write_called_from_a_sibling_file_is_followed() {
    let files = sources(&[
        ("blyg-core/src/store/mod.rs", FIXED_STORE),
        (
            "blyg-core/src/store/reading.rs",
            "impl Store {\n    pub fn forget_all(&self) -> Result<bool> {\n        \
             self.write_subscriptions(&[])\n    }\n}\n",
        ),
    ]);
    let allow = parse_allow("blyg-core/src/store/mod.rs edit_subscription | an edit made here");
    let bad = scan(&files, &allow);
    assert_eq!(bad.len(), 1, "{bad:#?}");
    assert!(
        bad[0].contains("crates/blyg-core/src/store/reading.rs `forget_all`"),
        "{}",
        bad[0]
    );
}

#[test]
fn other_tables_comments_and_tests_are_not_removals() {
    let text = r#"
fn tidy(tx: &Connection) {
    // DELETE FROM subscriptions would be wrong here.
    tx.execute("DELETE FROM reading_refs", [])?;
    tx.execute("DELETE FROM outbox WHERE seq = ?1", [seq])?;
}

#[cfg(test)]
mod tests {
    pub fn wipe(tx: &Connection) {
        tx.execute("DELETE FROM reading", [])?;
    }
}
"#;
    let files = sources(&[("blyg-core/src/store/x.rs", text)]);
    assert_eq!(scan(&files, &BTreeMap::new()), Vec::<String>::new());
}

#[test]
fn a_listed_function_that_no_longer_removes_is_reported() {
    let files = sources(&[("blyg-core/src/store/mod.rs", FIXED_STORE)]);
    let allow = parse_allow(
        "blyg-core/src/store/mod.rs edit_subscription | an edit made here\n\
         blyg-core/src/store/mod.rs gone_since | it was removed",
    );
    let bad = scan(&files, &allow);
    assert_eq!(bad.len(), 1, "{bad:#?}");
    assert!(
        bad[0].contains("`blyg-core/src/store/mod.rs gone_since`"),
        "{}",
        bad[0]
    );
}
