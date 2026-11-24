//! Every subscription an e2e test makes is dropped by a guard (`Unsub`, or
//! `ApiUnsub` for one made through the owner API) bound right after it, and
//! the guard names the subscription it drops. All tests share blyg `a` and
//! subscribe to the same blyg `b`, so a test that panics before its own
//! `unsubscribe` left the subscription behind, and every later test's
//! subscribe failed with 409 "already subscribed" (twelve knock-on failures
//! from one in the studio 0.68 run).
//!
//! This test needs no studio: it reads the e2e sources (subdirectories too)
//! and runs in the normal `cargo test`. Comments and string literals are
//! blanked before scanning, so a `.subscribe(` mentioned in either is not a
//! call. Every `.subscribe(` in these files is a blyg subscription (none is
//! an event or observer subscribe), so there is no allow-list.

use std::path::{Path, PathBuf};

/// How many lines after the end of a subscribe statement (its `;`) its guard
/// may sit.
const WITHIN: usize = 3;

/// Blanks comments and the insides of string and char literals with spaces,
/// keeping every newline, so offsets and line numbers match the source.
fn code_only(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied();
        let raw_string = c == 'r' && (next == Some('"') || next == Some('#')) && {
            let mut j = i + 1;
            while b.get(j) == Some(&'#') {
                j += 1;
            }
            b.get(j) == Some(&'"')
        };
        if c == '/' && next == Some('/') {
            while i < b.len() && b[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < b.len() {
                if b[i] == '/' && b.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if b[i] == '*' && b.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(b[i]));
                    i += 1;
                }
            }
        } else if raw_string {
            // r#"..."#
            let mut hashes = 0;
            let mut j = i + 1;
            while b[j] == '#' {
                hashes += 1;
                j += 1;
            }
            out.push_str(&" ".repeat(j + 1 - i));
            i = j + 1;
            while i < b.len() {
                if b[i] == '"' && (1..=hashes).all(|k| b.get(i + k) == Some(&'#')) {
                    out.push_str(&" ".repeat(1 + hashes));
                    i += 1 + hashes;
                    break;
                }
                out.push(blank(b[i]));
                i += 1;
            }
        } else if c == '"' {
            out.push(' ');
            i += 1;
            while i < b.len() && b[i] != '"' {
                if b[i] == '\\' && i + 1 < b.len() {
                    out.push(blank(b[i]));
                    i += 1;
                }
                out.push(blank(b[i]));
                i += 1;
            }
            if i < b.len() {
                out.push(' ');
                i += 1;
            }
        } else if c == '\'' {
            // A char literal ('x', '\n', '"'); anything else is a lifetime.
            let len = if next == Some('\\') {
                b[i + 2..].iter().position(|&x| x == '\'').map(|p| p + 3)
            } else if b.get(i + 2) == Some(&'\'') {
                Some(3)
            } else {
                None
            };
            match len {
                Some(n) => {
                    out.push_str(&" ".repeat(n));
                    i += n;
                }
                None => {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `name` occurs in `text` as a whole word.
fn mentions(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(at, _)| {
        !text[..at].chars().next_back().is_some_and(is_word)
            && !text[at + name.len()..].chars().next().is_some_and(is_word)
    })
}

/// The name a `let NAME = …` statement binds, given the code before the
/// `.subscribe(` back to the start of its statement.
fn bound_name(before: &str) -> Option<String> {
    let start = before.rfind([';', '{', '}']).map_or(0, |p| p + 1);
    let stmt = before[start..].trim_start();
    let rest = stmt.strip_prefix("let ")?.trim_start();
    let rest = rest.strip_prefix("mut ").unwrap_or(rest).trim_start();
    let name: String = rest.chars().take_while(|&c| is_word(c)).collect();
    let after = rest[name.len()..].trim_start();
    (!name.is_empty() && (after.starts_with('=') || after.starts_with(':'))).then_some(name)
}

/// The `file:line: text` of every `.subscribe(` call in `source` that is not
/// bound with `let NAME = …;` and followed, within [`WITHIN`] lines of the
/// statement's `;`, by an `Unsub(…)` or `ApiUnsub(…)` naming `NAME`.
fn unguarded_subscribes(file: &str, source: &str) -> Vec<String> {
    let code = code_only(source);
    let originals: Vec<&str> = source.lines().collect();
    let mut flagged = Vec::new();
    for (at, _) in code.match_indices(".subscribe(") {
        let line = code[..at].matches('\n').count();
        let guarded = bound_name(&code[..at]).is_some_and(|name| {
            // The end of the statement: the first `;` outside any brackets.
            let mut depth = 0i32;
            let end = code[at..].char_indices().find_map(|(k, c)| match c {
                '(' | '[' | '{' => {
                    depth += 1;
                    None
                }
                ')' | ']' | '}' => {
                    depth -= 1;
                    None
                }
                ';' if depth <= 0 => Some(at + k),
                _ => None,
            });
            end.is_some_and(|end| {
                code[end + 1..].lines().take(1 + WITHIN).any(|l| {
                    l.match_indices("Unsub(")
                        .any(|(g, m)| mentions(&l[g + m.len()..], &name))
                })
            })
        });
        if !guarded {
            flagged.push(format!("{file}:{}: {}", line + 1, originals[line].trim()));
        }
    }
    flagged
}

fn rust_files(dir: &Path, into: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read an e2e directory") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            rust_files(&path, into);
        } else if path.extension().is_some_and(|x| x == "rs")
            && !path.ends_with("subscribe_guard.rs")
        {
            into.push(path);
        }
    }
}

#[test]
fn every_e2e_subscription_is_dropped_by_a_guard() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/it/e2e");
    let mut files = Vec::new();
    rust_files(&dir, &mut files);
    files.sort();
    assert!(!files.is_empty(), "no e2e sources found in {dir:?}");
    let mut bad = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).expect("read an e2e source");
        let name = path.strip_prefix(&dir).unwrap_or(path).to_string_lossy();
        bad.extend(unguarded_subscribes(&name, &text));
    }
    assert!(
        bad.is_empty(),
        "an e2e test subscribes without a drop guard that names the subscription:\n{}\n\n\
         Why it matters: all e2e tests share one test blyg. If a test panics after \
         subscribing and before unsubscribing, the subscription stays behind and every \
         later test's subscribe fails with \"already subscribed\" (one failure once caused \
         twelve more).\n\
         Fix: bind the result with `let sub = ….subscribe(…)…;` and, within {WITHIN} lines of \
         the end of that statement, write a guard that names it:\n  \
         let _unsub = Unsub(&backend, sub.id.clone());   // subscribed through a LiveBackend\n  \
         let _unsub = ApiUnsub(&api, sub.clone());       // subscribed through `Api`\n\
         Both are defined in `e2e/mod.rs`; each drops the subscription even when the test \
         panics. A subscribe whose result isn't bound with `let` can't be guarded: bind it.",
        bad.join("\n")
    );
}

// The scan itself, on source text.

/// `maths.rs` as of 944574d, before the guard: the subscribe and the two
/// lines after it (the test panicked here and left the subscription).
const PAST_UNGUARDED: &str = r#"
    let sub = me.subscribe(&src_url, Some("Maths")).unwrap();
    sync_until_held(&me, &[&rid]);
    let r = reading_item(&me, &rid).expect("held");
"#;

#[test]
fn the_past_unguarded_site_is_flagged() {
    let flagged = unguarded_subscribes("maths.rs", PAST_UNGUARDED);
    assert_eq!(
        flagged,
        ["maths.rs:2: let sub = me.subscribe(&src_url, Some(\"Maths\")).unwrap();"]
    );
}

#[test]
fn a_guard_naming_the_binding_passes() {
    let src = "let sub = me.subscribe(&src_url, None).unwrap();\n\
               let _unsub = Unsub(&me, sub.id.clone());\n";
    assert!(unguarded_subscribes("x.rs", src).is_empty());
}

#[test]
fn the_api_guard_passes() {
    let src = "let sub = api.subscribe(&url_b(), Some(\"Revision test\")).unwrap();\n\
               let _unsub = ApiUnsub(&api, sub.clone());\n";
    assert!(unguarded_subscribes("x.rs", src).is_empty());
}

#[test]
fn a_multi_line_subscribe_followed_by_its_guard_passes() {
    let src = "let seed = me\n    .subscribe(\n        &format!(\"{host}/a.xml\"),\n        \
               Some(\"Walk\"),\n    )\n    .expect(\"subscribe\");\n\
               let _seed = Unsub(&me, seed.id.clone());\n";
    assert!(unguarded_subscribes("x.rs", src).is_empty());
}

#[test]
fn two_subscribes_with_one_guard_flag_the_unguarded_one() {
    let src = "let a = me.subscribe(&u1, None).unwrap();\n\
               let b = me.subscribe(&u2, None).unwrap();\n\
               let _unsub = Unsub(&me, a.id.clone());\n";
    assert_eq!(
        unguarded_subscribes("x.rs", src),
        ["x.rs:2: let b = me.subscribe(&u2, None).unwrap();"]
    );
}

#[test]
fn a_guard_naming_another_binding_does_not_count() {
    let src = "let sub = me.subscribe(&u, None).unwrap();\n\
               let _unsub = Unsub(&me, other.id.clone());\n\
               let _x = Unsub(&me, subs.id.clone());\n";
    assert_eq!(unguarded_subscribes("x.rs", src).len(), 1);
}

#[test]
fn an_unbound_subscribe_is_flagged_but_comments_and_strings_are_not() {
    let src = "me.subscribe(&u, None).unwrap();\n\
               // me.subscribe(&u, None).unwrap();\n\
               let s = \".subscribe(\";\n\
               let c = '\"'; /* x.subscribe( */\n";
    assert_eq!(
        unguarded_subscribes("x.rs", src),
        ["x.rs:1: me.subscribe(&u, None).unwrap();"]
    );
}
