//! No feature is decided by reading a version number (skill
//! `capability-gating`). A gate that parsed the studio's `contractVersion`
//! ("5.7 or later serves contacts") lost the feature on a contract it
//! couldn't read ("next") and on any newer major it hadn't heard of; three
//! of them shipped (`serves_contacts`, `serves_changelog`, `serves_groups`)
//! before they moved to the operations the blyg lists (`api::offers`;
//! `capability_gates.rs` checks those).
//!
//! This scan reads the product code of every workspace crate (`src/`,
//! without test files and `#[cfg(test)]` items) and reports each function
//! that:
//!
//! - calls `contract_version()` or reads the `"contractVersion"` field: the
//!   only ways the contract's number reaches the code;
//! - takes a version as text (a string parameter named `version`,
//!   `contract` or `…_version`) and parses it: `.parse`, `parse_version`,
//!   `.split(` or `.starts_with(` in its body. This is the shape the three
//!   old gates had, and it also sees a studio's version being read.
//!
//! Each one found must be in [`ALLOWED`], with why it may read a version. A
//! new one is either a feature gate (ask `api::offers` for an operation
//! instead) or a new row there, which a reviewer sees.
//!
//! The scan reads text, not types: a version passed under another name
//! and compared without being parsed gets past it.

use std::collections::BTreeSet;

use crate::timing_scan::{close_of, code_only, crates_dir, is_ident, line_of, workspace_files};

/// A function that may read a version, and why.
struct Allowed {
    /// Relative to `crates/`.
    file: &'static str,
    func: &'static str,
    why: &'static str,
}

const ALLOWED: &[Allowed] = &[
    Allowed {
        file: "blyg-core/src/api/mod.rs",
        func: "contract_version",
        why: "the one reader of the field: it hands the string on, and decides nothing",
    },
    Allowed {
        file: "blyg-core/src/api/mod.rs",
        func: "contract",
        why: "the major check: whether Burrow speaks this contract at all (Supported, TooOld, Newer)",
    },
    Allowed {
        file: "blyg-core/src/api/mod.rs",
        func: "verify_connection",
        why: "connecting refuses a contract that is too old, through `contract`",
    },
    Allowed {
        file: "blyg-core/src/sync/mod.rs",
        func: "check_server",
        why: "a sync starts with the major check, through `contract`",
    },
    Allowed {
        file: "blyg-core/src/sync/newer.rs",
        func: "recheck_contract",
        why: "the major check again mid-session, before a list removes anything",
    },
    Allowed {
        file: "blyg-core/src/api/mod.rs",
        func: "takes_whole_blyg",
        why: "the floor rule: `wholeBlyg` has no operation to look for, and only contract 5.0 is \
              known to refuse it; anything unreadable is sent it",
    },
    Allowed {
        file: "blyg-core/src/api/mod.rs",
        func: "whole_blyg",
        why: "asks `takes_whole_blyg` once and remembers the answer",
    },
    Allowed {
        file: "blyg-core/src/contacts.rs",
        func: "natural_grammar",
        why: "the studio's version, not the contract's: no capability says how a studio reads \
              `@handle`s, and an unknown version keeps the older reading",
    },
    Allowed {
        file: "blyg-app/src/syndication_ui.rs",
        func: "parse_entry",
        why: "a post's version number, typed into the copies-elsewhere sheet",
    },
];

/// One product source file.
pub(crate) struct Source {
    /// Relative to `crates/`.
    pub path: String,
    pub src: String,
    /// `src` through [`product_code`].
    pub code: String,
}

/// Whether `path` is a test file by its name, as `scripts/test-first.sh`
/// reads it.
fn named_as_tests(path: &str) -> bool {
    path.ends_with("_tests.rs") || path.ends_with("/tests.rs") || path.contains("/tests/")
}

/// What marks an item as compiled for tests only.
const TEST_CFG: [&str; 2] = ["#[cfg(test)]", "#[cfg(all(test,"];

/// The index of the first byte at or after `i` that isn't whitespace.
pub(crate) fn skip_space(code: &str, mut i: usize) -> usize {
    let b = code.as_bytes();
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// The identifier that starts at `i` (empty when none does).
pub(crate) fn ident_at(code: &str, i: usize) -> &str {
    let b = code.as_bytes();
    let mut end = i;
    while end < b.len() && is_ident(b[end]) {
        end += 1;
    }
    &code[i..end]
}

/// The byte range of each test-only item in `code` (`src` through
/// `code_only`): the attribute, any attributes after it, and the item up to
/// the end of its body (an inline `mod tests { … }`, a helper `fn`) or its
/// `;` (a `use`, a `mod x;`).
fn test_only_ranges(code: &str) -> Vec<(usize, usize)> {
    let b = code.as_bytes();
    let mut out = vec![];
    for marker in TEST_CFG {
        for (at, _) in code.match_indices(marker) {
            let mut i = close_of(b, at + 1);
            loop {
                i = skip_space(code, i);
                if !code[i..].starts_with("#[") {
                    break;
                }
                i = close_of(b, i + 1);
            }
            let rest = &code[i..];
            let end = match (rest.find('{'), rest.find(';')) {
                (Some(open), semi) if semi.is_none_or(|s| open < s) => close_of(b, i + open),
                (_, Some(semi)) => i + semi + 1,
                _ => code.len(),
            };
            out.push((at, end));
        }
    }
    out
}

/// `src` as product code: comments, string and char literals and test-only
/// items blanked to spaces (byte for byte, newlines kept), so offsets and
/// line numbers still match.
pub(crate) fn product_code(src: &str) -> String {
    let code = code_only(src);
    let tests = test_only_ranges(&code);
    let mut out = code.into_bytes();
    for (from, to) in tests {
        for c in &mut out[from..to] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    }
    String::from_utf8(out).unwrap()
}

/// Where `needle` (a string literal, with its quotes) stands in `src`'s
/// product code: in a string, outside test-only items and `//` comments. A
/// literal that follows a `//` on its line (after a URL in a string, say)
/// is taken for a comment and missed.
pub(crate) fn string_literals(src: &str, needle: &str) -> Vec<usize> {
    let plain = code_only(src);
    let tests = test_only_ranges(&plain);
    src.match_indices(needle)
        .map(|(at, _)| at)
        .filter(|&at| plain.as_bytes()[at] == b' ')
        .filter(|&at| !tests.iter().any(|&(from, to)| from <= at && at < to))
        .filter(|&at| {
            let line = src[..at].rfind('\n').map_or(0, |n| n + 1);
            !src[line..at].contains("//")
        })
        .collect()
}

/// A module kept in another file (`mod name;`).
struct ModFile {
    name: String,
    /// Its `#[path = "…"]`.
    path: Option<String>,
    /// Declared under `#[cfg(test)]`.
    test: bool,
}

/// The `mod name;` declarations of a file (`code`: `src` through
/// `code_only`), each with the attributes written before it.
fn mod_files(src: &str, code: &str) -> Vec<ModFile> {
    let b = code.as_bytes();
    let mut out = vec![];
    for (at, _) in code.match_indices("mod ") {
        if at > 0 && is_ident(b[at - 1]) {
            continue;
        }
        let name_at = skip_space(code, at + 4);
        let name = ident_at(code, name_at);
        if name.is_empty() || b.get(skip_space(code, name_at + name.len())) != Some(&b';') {
            continue;
        }
        // Its attributes and visibility: everything since the item before it.
        let head = code[..at].rfind([';', '{', '}']).map_or(0, |end| end + 1);
        let attributes = &code[head..at];
        let path = attributes
            .find("#[path")
            .and_then(|p| src[head + p..at].split('"').nth(1))
            .map(str::to_string);
        out.push(ModFile {
            name: name.to_string(),
            path,
            test: TEST_CFG.iter().any(|cfg| attributes.contains(*cfg)),
        });
    }
    out
}

/// Where the module `m`, declared in `file`, may be kept (relative to
/// `crates/`).
fn module_paths(file: &str, m: &ModFile) -> Vec<String> {
    let (dir, base) = file.rsplit_once('/').unwrap_or(("", file));
    if let Some(path) = &m.path {
        return vec![format!("{dir}/{path}")];
    }
    let stem = base.trim_end_matches(".rs");
    let home = if matches!(stem, "mod" | "lib" | "main") {
        dir.to_string()
    } else {
        format!("{dir}/{stem}")
    };
    vec![
        format!("{home}/{}.rs", m.name),
        format!("{home}/{}/mod.rs", m.name),
    ]
}

/// The test files among `files`: named as tests, declared under
/// `#[cfg(test)]`, or declared by a test file.
fn test_files(files: &[String], srcs: &[String]) -> BTreeSet<String> {
    let mods: Vec<Vec<ModFile>> = srcs
        .iter()
        .map(|src| mod_files(src, &code_only(src)))
        .collect();
    let mut tests: BTreeSet<String> = files
        .iter()
        .filter(|f| named_as_tests(f))
        .cloned()
        .collect();
    loop {
        let before = tests.len();
        for (file, mods) in files.iter().zip(&mods) {
            let in_tests = tests.contains(file);
            for m in mods.iter().filter(|m| in_tests || m.test) {
                tests.extend(module_paths(file, m));
            }
        }
        if tests.len() == before {
            return tests;
        }
    }
}

/// Every workspace crate's product code: its `src/` without test files.
pub(crate) fn product_sources() -> Vec<Source> {
    let root = crates_dir();
    let files: Vec<String> = workspace_files(root)
        .into_iter()
        .filter(|f| f.contains("/src/"))
        .collect();
    let srcs: Vec<String> = files
        .iter()
        .map(|f| std::fs::read_to_string(root.join(f)).unwrap())
        .collect();
    let tests = test_files(&files, &srcs);
    let sources: Vec<Source> = files
        .into_iter()
        .zip(srcs)
        .filter(|(path, _)| !tests.contains(path))
        .map(|(path, src)| {
            let code = product_code(&src);
            Source { path, src, code }
        })
        .collect();
    assert!(
        sources.len() > 50,
        "only {} product files found under crates/*/src: is the scan looking in the right place?",
        sources.len()
    );
    sources
}

/// A function with a body.
pub(crate) struct Func<'a> {
    pub name: &'a str,
    /// Between its parentheses.
    pub params: &'a str,
    /// From its `{` to past its `}`.
    pub body: (usize, usize),
}

/// The functions in `code` (closures belong to the function around them).
pub(crate) fn functions(code: &str) -> Vec<Func<'_>> {
    let b = code.as_bytes();
    let mut out = vec![];
    for (at, _) in code.match_indices("fn ") {
        if at > 0 && is_ident(b[at - 1]) {
            continue;
        }
        let name_at = skip_space(code, at + 3);
        let name = ident_at(code, name_at);
        if name.is_empty() {
            continue;
        }
        // Past its generics (`<F: Fn(&str) -> bool>`), to its parameters.
        let mut open = name_at + name.len();
        if b.get(open) == Some(&b'<') {
            let mut depth = 0i32;
            while open < b.len() {
                match b[open] {
                    b'<' => depth += 1,
                    b'>' if b[open - 1] != b'-' => depth -= 1,
                    _ => {}
                }
                open += 1;
                if depth == 0 {
                    break;
                }
            }
        }
        if b.get(open) != Some(&b'(') {
            continue;
        }
        let close = close_of(b, open);
        let rest = &code[close..];
        let Some(body) = rest.find('{') else {
            continue;
        };
        if rest.find(';').is_some_and(|semi| semi < body) {
            continue;
        }
        out.push(Func {
            name,
            params: &code[open + 1..(close - 1).max(open + 1)],
            body: (close + body, close_of(b, close + body)),
        });
    }
    out
}

/// The name of the innermost function around `at`.
pub(crate) fn enclosing<'a>(funcs: &[Func<'a>], at: usize) -> &'a str {
    funcs
        .iter()
        .filter(|f| f.body.0 <= at && at < f.body.1)
        .min_by_key(|f| f.body.1 - f.body.0)
        .map_or("(no function)", |f| f.name)
}

/// The parts of `list` between commas outside brackets, trimmed; an empty
/// part (a trailing comma) is left out.
pub(crate) fn top_level(list: &str) -> Vec<&str> {
    let b = list.as_bytes();
    let mut depth = 0i32;
    let mut from = 0;
    let mut out = vec![];
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'>' if i > 0 && b[i - 1] != b'-' => depth -= 1,
            b',' if depth == 0 => {
                out.push(list[from..i].trim());
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(list[from..].trim());
    out.retain(|part| !part.is_empty());
    out
}

/// Whether `params` hold a version as text: a string parameter named
/// `version`, `contract` or `…_version` (a post's version is a number).
fn takes_a_version(params: &str) -> bool {
    top_level(params).into_iter().any(|param| {
        let Some((name, kind)) = param.split_once(':') else {
            return false;
        };
        let name = name.trim();
        let name = name.strip_prefix("mut ").unwrap_or(name).trim();
        (matches!(name, "version" | "contract") || name.ends_with("_version"))
            && (kind.contains("str") || kind.contains("String"))
    })
}

/// The call that reads the contract's version.
const READ_CALL: &str = "contract_version(";

/// The field the contract's version is served in.
const FIELD: &str = "\"contractVersion\"";

/// What reading a version's parts looks like in a function body.
const PARSES: [&str; 4] = [".parse", "parse_version", ".split(", ".starts_with("];

/// A function found reading a version.
#[derive(Debug)]
struct Reader {
    line: usize,
    func: String,
    what: &'static str,
}

/// Each function in one file's product code that reads a version.
fn readers(src: &str) -> Vec<Reader> {
    let code = product_code(src);
    let b = code.as_bytes();
    let funcs = functions(&code);
    let mut out = vec![];
    for (at, _) in code.match_indices(READ_CALL) {
        let another_name = at > 0 && is_ident(b[at - 1]);
        let its_definition = code[..at].trim_end().ends_with("fn");
        if !another_name && !its_definition {
            out.push(Reader {
                line: line_of(&code, at),
                func: enclosing(&funcs, at).to_string(),
                what: "calls contract_version()",
            });
        }
    }
    for at in string_literals(src, FIELD) {
        out.push(Reader {
            line: line_of(&code, at),
            func: enclosing(&funcs, at).to_string(),
            what: "reads the \"contractVersion\" field",
        });
    }
    for f in &funcs {
        let body = &code[f.body.0..f.body.1];
        if takes_a_version(f.params) && PARSES.iter().any(|p| body.contains(*p)) {
            out.push(Reader {
                line: line_of(&code, f.body.0),
                func: f.name.to_string(),
                what: "parses a version it is handed",
            });
        }
    }
    out.sort_by_key(|r| r.line);
    out
}

#[test]
fn no_feature_is_gated_on_a_parsed_version() {
    let mut found = vec![];
    let mut used = BTreeSet::new();
    for s in product_sources() {
        for r in readers(&s.src) {
            match ALLOWED
                .iter()
                .position(|a| a.file == s.path && a.func == r.func)
            {
                Some(i) => {
                    used.insert(i);
                }
                None => found.push(format!(
                    "crates/{}:{}: fn {} {}",
                    s.path, r.line, r.func, r.what
                )),
            }
        }
    }
    assert!(
        found.is_empty(),
        "A feature gated on a version number is lost on a contract Burrow can't read (\"next\")\n\
         and on a newer one it hasn't heard of. Gate it on an operation the blyg's capabilities\n\
         list (blyg_core::api::offers, Api::offers_all; skill capability-gating). If this isn't\n\
         a feature gate, add the function to ALLOWED in version_gates.rs with why.\n\n{}",
        found.join("\n")
    );
    let stale: Vec<String> = ALLOWED
        .iter()
        .enumerate()
        .filter(|(i, _)| !used.contains(i))
        .map(|(_, a)| format!("{} fn {} ({})", a.file, a.func, a.why))
        .collect();
    assert!(
        stale.is_empty(),
        "ALLOWED in version_gates.rs lists functions that no longer read a version; remove them:\n{}",
        stale.join("\n")
    );
}

/// `serves_contacts` as it was before the gates moved to operations
/// (commit 1639901): the gate in `blyg-core/src/contacts.rs`, and its
/// caller in `blyg-core/src/api/contacts.rs`.
const OLD_GATE: &str = r#"
/// Whether a studio's `contractVersion` serves contacts, the `@handle`
/// rewrite and the mentions lens (5.7.0, studio 0.56).
pub fn serves_contacts(version: &str) -> bool {
    let mut parts = version.trim().split('.').map(|p| p.parse::<u64>().ok());
    match (parts.next().flatten(), parts.next().flatten()) {
        (Some(5), Some(minor)) => minor >= 7,
        (Some(major), _) => major > 5,
        _ => false,
    }
}

impl Api {
    /// Whether this studio serves contacts (asks its capabilities).
    pub fn serves_contacts(&self) -> Result<bool> {
        Ok(self
            .contract_version()?
            .as_deref()
            .is_some_and(serves_contacts))
    }
}
"#;

#[test]
fn the_old_contacts_gate_is_caught() {
    let found: Vec<(usize, &str)> = readers(OLD_GATE).iter().map(|r| (r.line, r.what)).collect();
    assert_eq!(
        found,
        [
            (4, "parses a version it is handed"),
            (17, "calls contract_version()"),
        ],
        "{:?}",
        readers(OLD_GATE)
    );
    assert!(
        readers(OLD_GATE)
            .iter()
            .all(|r| r.func == "serves_contacts"),
        "neither is in ALLOWED: {:?}",
        readers(OLD_GATE)
    );
    assert!(!ALLOWED.iter().any(|a| a.func == "serves_contacts"));
}

#[test]
fn a_gate_on_operations_and_test_code_are_left_alone() {
    let src = r#"
pub const CONTACTS_OPERATION: &str = "listContacts";

impl Api {
    pub fn serves_contacts(&self) -> Result<bool> {
        self.offers_all(&[CONTACTS_OPERATION])
    }

    // if self.contract_version()? >= "5.7" { … }
    pub fn version_label(&self, item: &Item) -> String {
        format!("v{}", item.version.to_string().split('.').count())
    }

    pub fn keep_contract_version(&self, v: &str) {}
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_reads() {
        let v = json!({"contractVersion": "5.9.0"});
        api.contract_version().unwrap();
    }
}
"#;
    assert!(readers(src).is_empty(), "{:?}", readers(src));
}

/// The readers the allow-list names are seen by the scan (it is the list
/// that lets them through, not a blind spot), in today's code.
#[test]
fn the_allowed_readers_are_seen_in_todays_code() {
    let read = |file: &str| -> Vec<String> {
        let src = std::fs::read_to_string(crates_dir().join(file)).unwrap();
        readers(&src).into_iter().map(|r| r.func).collect()
    };
    let api = read("blyg-core/src/api/mod.rs");
    for func in ["contract", "takes_whole_blyg", "contract_version"] {
        assert!(api.iter().any(|f| f == func), "{func}: {api:?}");
    }
    assert_eq!(read("blyg-core/src/contacts.rs"), ["natural_grammar"]);
}

#[test]
fn test_files_are_found_by_name_and_by_declaration() {
    let files: Vec<String> = [
        "k/src/lib.rs",
        "k/src/a.rs",
        "k/src/a_tests.rs",
        "k/src/a/checks.rs",
        "k/src/a/checks/more.rs",
        "k/src/b.rs",
        "k/src/ui.rs",
    ]
    .map(str::to_string)
    .to_vec();
    let srcs: Vec<String> = [
        "mod a;\npub mod b;\n#[cfg(test)]\n#[path = \"ui.rs\"]\nmod ui_checks;\n",
        "#[cfg(test)]\nmod checks;\nfn f() {}\n",
        "",
        "mod more;\n",
        "",
        "",
        "",
    ]
    .map(str::to_string)
    .to_vec();
    let tests = test_files(&files, &srcs);
    let product: Vec<&str> = files
        .iter()
        .filter(|f| !tests.contains(*f))
        .map(String::as_str)
        .collect();
    assert_eq!(product, ["k/src/lib.rs", "k/src/a.rs", "k/src/b.rs"]);
}
