//! A test that times the code under test fails on a slow, loaded or Windows
//! machine sooner or later, however generous its budget. So a test that
//! guards an algorithm counts work (queries, VM steps, renders, calls), and
//! the wall-clock budgets that remain are reports: each is
//! `#[ignore = "timing: run by the CI timing job"]`, goes through
//! `blyg_core::wait::Budget` (one scaling: debug and Windows factors), and
//! runs in CI's `timing` job, which never blocks a merge
//! (`scripts/check.sh timing`).
//!
//! This scan reads every Rust file of the workspace's crates, outside those
//! timing tests and the wait helpers themselves (`blyg-core/src/wait.rs`,
//! `blyg-app/src/test_wait.rs`), and refuses:
//!
//! - a local `fn budget(`: a budget of its own, scaled its own way;
//! - an `assert!` that bounds measured time from above: a comparison whose
//!   smaller side is measured (`t.elapsed() < d`, `per.as_micros() < 5000`,
//!   where `per` came from `.elapsed()` or `Instant::now()`), or whose
//!   larger side is a budget (`median < BUDGET * 4`), or a remaining time
//!   (`deadline - Instant::now() > d`);
//! - a timing test not named `timing_…`, which the CI job's filter misses.
//!
//! A lower bound (`started.elapsed() >= d`: the code under test waited, as
//! a pacing test checks) can't fail on a slow machine, and is allowed. So is
//! a deadline that only stops a polling loop (`while t.elapsed() < limit`):
//! that's a wait, not an assertion.

use std::path::Path;

fn crates_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

/// The wait helpers, which measure time to bound a wait.
const EXEMPT: &[&str] = &["blyg-core/src/wait.rs", "blyg-app/src/test_wait.rs"];

/// What marks a test as a timing report, run only by CI's timing job.
const TIMING_IGNORE: &str = "#[ignore = \"timing:";

/// Every `.rs` file under `dir`, relative to `crates/`.
fn rust_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            rust_files(root, &p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            let rel = p.strip_prefix(root).unwrap();
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// The workspace members' sources, tests, examples, benches and build
/// scripts, relative to `crates/` (a leftover crate directory isn't built).
fn workspace_files(root: &Path) -> Vec<String> {
    let manifest: toml::Table =
        toml::from_str(&std::fs::read_to_string(root.join("../Cargo.toml")).unwrap()).unwrap();
    let members = manifest["workspace"]["members"].as_array().unwrap();
    let mut files = Vec::new();
    for m in members {
        let krate = root.join(m.as_str().unwrap().trim_start_matches("crates/"));
        for sub in ["src", "tests", "examples", "benches"] {
            if krate.join(sub).is_dir() {
                rust_files(root, &krate.join(sub), &mut files);
            }
        }
        if krate.join("build.rs").is_file() {
            let rel = krate.join("build.rs");
            let rel = rel.strip_prefix(root).unwrap();
            files.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    files
}

/// `src` with comments, string and char literals blanked to spaces (byte
/// for byte, newlines kept), so offsets and line numbers still match.
fn code_only(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let blank = |out: &mut Vec<u8>, from: usize, to: usize| {
        for c in &mut out[from..to] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    };
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = b[i..]
                    .iter()
                    .position(|&c| c == b'\n')
                    .map_or(b.len(), |p| i + p);
                blank(&mut out, i, end);
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let mut depth = 0;
                let mut j = i;
                while j < b.len() {
                    if b[j..].starts_with(b"/*") {
                        depth += 1;
                        j += 2;
                    } else if b[j..].starts_with(b"*/") {
                        depth -= 1;
                        j += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        j += 1;
                    }
                }
                blank(&mut out, i, j);
                i = j;
            }
            b'r' if (i == 0 || !is_ident(b[i - 1]))
                && matches!(b.get(i + 1), Some(b'"') | Some(b'#')) =>
            {
                let hashes = b[i + 1..].iter().take_while(|&&c| c == b'#').count();
                let open = i + 1 + hashes;
                if b.get(open) != Some(&b'"') {
                    i += 1;
                    continue;
                }
                let mut close = b"\"".to_vec();
                close.extend(std::iter::repeat_n(b'#', hashes));
                let end = b[open + 1..]
                    .windows(close.len())
                    .position(|w| w == close.as_slice())
                    .map_or(b.len(), |p| open + 1 + p + close.len());
                blank(&mut out, i, end);
                i = end;
            }
            b'"' => {
                let mut j = i + 1;
                while j < b.len() && b[j] != b'"' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                let end = (j + 1).min(b.len());
                blank(&mut out, i, end);
                i = end;
            }
            b'\'' => {
                // A char literal ('x', '\n', '\u{1F600}', 'é'), not a lifetime.
                let rest = &src[i + 1..];
                let len = if rest.starts_with('\\') {
                    rest.find('\'').map(|p| p + 1)
                } else {
                    rest.chars()
                        .next()
                        .filter(|_| rest.chars().nth(1) == Some('\''))
                        .map(|c| c.len_utf8() + 1)
                };
                match len {
                    Some(l) => {
                        blank(&mut out, i, i + 1 + l);
                        i += 1 + l;
                    }
                    None => i += 1,
                }
            }
            _ => i += 1,
        }
    }
    String::from_utf8(out).unwrap()
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// The index just past the bracket that closes the one at `open`.
fn close_of(code: &[u8], open: usize) -> usize {
    let mut depth = 0i32;
    for (i, &c) in code.iter().enumerate().skip(open) {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    code.len()
}

/// Where each `#[ignore = "timing: …"]` attribute starts: in code, not a
/// comment or a string (`code` is `src` through [`code_only`]).
fn timing_attributes(src: &str, code: &str) -> Vec<usize> {
    src.match_indices(TIMING_IGNORE)
        .map(|(at, _)| at)
        .filter(|&at| code.as_bytes()[at] == b'#')
        .collect()
}

/// `code` with each timing test's function blanked (an attribute
/// `#[ignore = "timing: …"]`, read from the original `src`).
fn without_timing_tests(src: &str, code: &str) -> String {
    let mut out = code.as_bytes().to_vec();
    for at in timing_attributes(src, code) {
        let Some(f) = code[at..].find("fn ") else {
            continue;
        };
        let Some(b) = code[at + f..].find('{') else {
            continue;
        };
        let open = at + f + b;
        let end = close_of(code.as_bytes(), open);
        for c in &mut out[at..end] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    }
    String::from_utf8(out).unwrap()
}

/// Whether `word` appears in `s` as a whole identifier.
fn has_word(s: &str, word: &str) -> bool {
    let b = s.as_bytes();
    s.match_indices(word).any(|(i, _)| {
        // Not a field, method or path segment of the same name.
        let before = i == 0 || !(is_ident(b[i - 1]) || b[i - 1] == b'.' || b[i - 1] == b':');
        let after = b.get(i + word.len()).is_none_or(|&c| !is_ident(c));
        before && after
    })
}

/// The body (`{` to past its `}`) of each function in `code`.
fn fn_bodies(code: &str) -> Vec<(usize, usize)> {
    let b = code.as_bytes();
    let mut out = vec![];
    for (i, _) in code.match_indices("fn ") {
        if i > 0 && is_ident(b[i - 1]) {
            continue;
        }
        let rest = &code[i..];
        if let Some(o) = rest.find('{')
            && rest.find(';').is_none_or(|s| o < s)
        {
            out.push((i + o, close_of(b, i + o)));
        }
    }
    out
}

/// A name holding a measured time, and the function body it's known in.
struct Measured {
    name: String,
    scope: (usize, usize),
}

/// The names that hold a measured time at `at`.
fn names_at(measured: &[Measured], at: usize) -> Vec<&str> {
    measured
        .iter()
        .filter(|m| m.scope.0 <= at && at < m.scope.1)
        .map(|m| m.name.as_str())
        .collect()
}

/// Names in `code` that hold a measured time, each in its function: bound
/// by a `let` (or pushed into, or added to) from `.elapsed()`,
/// `Instant::now()` or another such name.
fn measured_names(code: &str) -> Vec<Measured> {
    let bodies = fn_bodies(code);
    // The innermost function body around `at` (the file, outside any).
    let scope = |at: usize| {
        bodies
            .iter()
            .filter(|(o, c)| *o <= at && at < *c)
            .min_by_key(|(o, c)| c - o)
            .copied()
            .unwrap_or((0, code.len()))
    };
    let mut names: Vec<Measured> = vec![];
    // An instant taken to time something starts its `let` (`Instant::now()`
    // passed along as an argument, a window's launch time, isn't one).
    let measured = |s: &str, at: usize, names: &[Measured]| {
        let s = s.trim_start();
        s.contains(".elapsed()")
            || s.starts_with("Instant::now()")
            || s.starts_with("std::time::Instant::now()")
            || names_at(names, at).iter().any(|n| has_word(s, n))
    };
    let add = |names: &mut Vec<Measured>, name: &str, at: usize| {
        let scope = scope(at);
        if !names.iter().any(|m| m.name == name && m.scope == scope) {
            names.push(Measured {
                name: name.to_string(),
                scope,
            });
        }
    };
    loop {
        let before = names.len();
        // `let [mut] name [: T] = rhs;` and `let (a, b) = rhs;`
        for (i, _) in code.match_indices("let ") {
            if i > 0 && is_ident(code.as_bytes()[i - 1]) {
                continue;
            }
            let rest = &code[i + 4..];
            let Some(eq) = rest.find('=') else { continue };
            if rest[eq..].starts_with("==") {
                continue;
            }
            let pat = &rest[..eq];
            let rhs_start = i + 4 + eq + 1;
            let end = statement_end(code.as_bytes(), rhs_start);
            if !measured(&code[rhs_start..end], i, &names) {
                continue;
            }
            let pat = pat.split(':').next().unwrap_or(pat);
            for w in pat.split(|c: char| !c.is_alphanumeric() && c != '_') {
                if !w.is_empty() && w != "mut" && w != "ref" && !w.starts_with(char::is_numeric) {
                    add(&mut names, w, i);
                }
            }
        }
        // `name.push(x)`, `name.0.push(x)`, `name += x`
        for op in [".push(", ".0.push(", " += "] {
            for (i, _) in code.match_indices(op) {
                let b = code.as_bytes();
                let mut s = i;
                while s > 0 && is_ident(b[s - 1]) {
                    s -= 1;
                }
                let name = &code[s..i];
                if name.is_empty() || name.starts_with(char::is_numeric) {
                    continue;
                }
                let end = statement_end(b, i + op.len());
                if measured(&code[i + op.len()..end], i, &names) {
                    add(&mut names, name, i);
                }
            }
        }
        if names.len() == before {
            return names;
        }
    }
}

/// The end of the statement starting at `from`: the first `;` (or the
/// unmatched closing bracket) outside brackets.
fn statement_end(code: &[u8], from: usize) -> usize {
    let mut depth = 0i32;
    for (i, &c) in code.iter().enumerate().skip(from) {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth < 0 {
                    return i;
                }
            }
            b';' if depth == 0 => return i,
            _ => {}
        }
    }
    code.len()
}

/// The first argument of the macro call whose `(` is at `open`: up to the
/// first comma outside brackets.
fn first_arg(code: &str, open: usize) -> &str {
    let b = code.as_bytes();
    let end = close_of(b, open) - 1;
    let mut depth = 0i32;
    for i in open + 1..end {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => return &code[open + 1..i],
            _ => {}
        }
    }
    &code[open + 1..end]
}

/// The parts of `cond` joined by `&&` or `||` outside brackets.
fn conjuncts(cond: &str) -> Vec<&str> {
    let b = cond.as_bytes();
    let mut depth = 0i32;
    let mut out = vec![];
    let mut from = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'&' | b'|' if depth == 0 && b.get(i + 1) == Some(&b[i]) => {
                out.push(&cond[from..i]);
                from = i + 2;
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&cond[from..]);
    out
}

/// Why the assertion condition `cond` bounds measured time from above, if
/// it does. `measured`: the names holding a measured time there.
fn upper_bound(cond: &str, measured: &[&str]) -> Option<&'static str> {
    let cond = cond.split_whitespace().collect::<Vec<_>>().join(" ");
    conjuncts(&cond)
        .into_iter()
        .find_map(|c| compare_bound(&format!(" {} ", c.trim()), measured))
}

/// [`upper_bound`] for one comparison.
fn compare_bound(cond: &str, measured: &[&str]) -> Option<&'static str> {
    let b = cond.as_bytes();
    // A comparison outside brackets, spaced as rustfmt spaces it (so a
    // generic `Vec<Duration>` or a `->` isn't one).
    let mut depth = 0i32;
    let mut op = None;
    for i in 0..b.len() {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'<' | b'>' if depth == 0 && i > 0 && b[i - 1] == b' ' => {
                let eq = b.get(i + 1) == Some(&b'=');
                let next = if eq { i + 2 } else { i + 1 };
                if b.get(next) == Some(&b' ') {
                    op = Some((i, next, b[i] == b'<'));
                    break;
                }
            }
            _ => {}
        }
    }
    let (at, after, less) = op?;
    let (lhs, rhs) = (&cond[..at], &cond[after..]);
    let (small, large) = if less { (lhs, rhs) } else { (rhs, lhs) };
    let is_measured = |s: &str| {
        s.contains(".elapsed()")
            || s.contains("Instant::now()")
            || measured.iter().any(|n| has_word(s, n))
    };
    // A budget: `budget(…)` or a `BUDGET` constant (not a field that counts
    // pages, say).
    let budget = |s: &str| s.contains("budget(") || s.contains("BUDGET");
    if is_measured(small) {
        Some("measured time bounded from above")
    } else if budget(large) {
        Some("a budget bounds it from above")
    } else if large.replace(' ', "").contains("-Instant::now()") {
        Some("a remaining time (… - Instant::now()) bounded from below")
    } else {
        None
    }
}

fn line_of(code: &str, at: usize) -> usize {
    code[..at].matches('\n').count() + 1
}

/// Each timing test whose name the CI job's filter (`timing_`) misses.
fn misnamed_timing_tests(src: &str, code: &str) -> Vec<(usize, String)> {
    let mut out = vec![];
    for at in timing_attributes(src, code) {
        let Some(f) = code[at..].find("fn ") else {
            continue;
        };
        let name: String = code[at + f + 3..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.starts_with("timing_") {
            out.push((
                line_of(code, at),
                format!(
                    "a timing test not named timing_… (`{name}`): CI's timing job won't run it"
                ),
            ));
        }
    }
    out
}

/// Each offence in one file's source: `line: what`.
fn offences(src: &str) -> Vec<(usize, String)> {
    let stripped = code_only(src);
    let mut out = misnamed_timing_tests(src, &stripped);
    let code = without_timing_tests(src, &stripped);
    for (i, _) in code.match_indices("fn budget(") {
        if i == 0 || !is_ident(code.as_bytes()[i - 1]) {
            out.push((line_of(&code, i), "a local `fn budget(`".to_string()));
        }
    }
    let measured = measured_names(&code);
    for mac in ["assert!(", "debug_assert!("] {
        for (i, _) in code.match_indices(mac) {
            if i > 0 && is_ident(code.as_bytes()[i - 1]) {
                continue;
            }
            let open = i + mac.len() - 1;
            if let Some(why) = upper_bound(first_arg(&code, open), &names_at(&measured, i)) {
                out.push((line_of(&code, i), format!("an assert!: {why}")));
            }
        }
    }
    out.sort();
    out
}

fn offenders(root: &Path) -> Vec<String> {
    let mut out = vec![];
    for f in workspace_files(root) {
        if EXEMPT.contains(&f.as_str()) {
            continue;
        }
        let src = std::fs::read_to_string(root.join(&f)).unwrap();
        for (line, what) in offences(&src) {
            out.push(format!("crates/{f}:{line}: {what}"));
        }
    }
    out
}

#[test]
fn tests_assert_time_only_through_wait_budget() {
    let found = offenders(crates_dir());
    assert!(
        found.is_empty(),
        "A wall-clock bound fails on a slow, loaded or Windows machine sooner or later.\n\
         Count work instead (queries, VM steps, renders, calls), or make it a timing\n\
         report: #[ignore = \"timing: run by the CI timing job\"] and blyg_core::wait::Budget.\n\n{}",
        found.join("\n")
    );
}

#[test]
fn the_scan_sees_each_kind_and_leaves_waits_alone() {
    let src = r#"
fn budget(ms: u64) -> Duration { Duration::from_millis(ms) }
fn a() {
    let t = Instant::now();
    work();
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
}
fn b() {
    let per = t0.elapsed() / 20;
    assert!(
        per.as_micros() < 5000,
        "took {per:?}"
    );
}
fn c() {
    let mut took = Vec::new();
    took.push(t.elapsed());
    let median = took[took.len() / 2];
    assert!(median < BUDGET * 4, "a < b");
    assert!(keys_median < budget(), "unmeasured, against a budget");
    assert!(e.retry_at().unwrap() - Instant::now() > initial * 8);
}
fn waits() {
    // assert!(t.elapsed() < d) in a comment
    let s = "assert!(t.elapsed() < d)";
    let started = Instant::now();
    run();
    assert!(started.elapsed() >= Duration::from_millis(950));
    let wait = retry_at - before;
    assert!(wait >= initial, "{wait:?}");
    while start.elapsed() < Duration::from_secs(10) {}
    let v: Vec<Duration> = vec![];
    assert!(v.len() > 2);
    assert!(rows.len() < 5_000);
    // A fake clock's time, and another function's `per`.
    let t = page.now();
    assert!(t >= Duration::from_secs(5) && t < Duration::from_secs(6));
    assert!(per < 3);
}
fn d() {
    let t = Instant::now();
    assert!(n > 0 && t.elapsed().as_millis() < 400);
}
#[test]
#[ignore = "timing: run by the CI timing job"]
fn timing_report() {
    let t = Instant::now();
    assert!(t.elapsed() < Duration::from_millis(10));
}
#[ignore = "timing: run by the CI timing job"]
fn slow_report() {}
"#;
    let found: Vec<usize> = offences(src).into_iter().map(|(l, _)| l).collect();
    assert_eq!(found, [2, 6, 10, 19, 20, 21, 42, 50], "{:?}", offences(src));
}
