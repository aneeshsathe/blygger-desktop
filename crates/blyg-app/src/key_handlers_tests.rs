//! Key and click modifiers handled outside the keymap.
//!
//! The keymap's tests (`windows_overrides_are_real_and_clash_with_nothing`,
//! `no_windows_key_is_an_altgr_character`) see only what is bound there. A
//! handler that reads `modifiers` by hand, in an `on_key_down`, a
//! `capture_key_down` or a mouse closure, escapes them, and two mistakes
//! came of it:
//!
//! - the Publish sheet took Alt+1 to Alt+9 itself, and on Windows the
//!   keymap binds Alt+1 and Alt+2 globally and won (`auto_crosspost_key`
//!   before the chip-keys-win merge);
//! - handlers tested `modifiers.platform` alone. `platform` is Cmd on a Mac
//!   and the Windows key on Windows, so such a chord is dead on Windows
//!   (About's close, the Reader's select-all, before the same merge).
//!
//! Two checks over the crate's non-test sources:
//!
//! - `no_chord_reads_the_platform_key_alone`: a condition that reads
//!   `modifiers.platform` also reads `modifiers.control` or `secondary()`,
//!   or negates the platform key (a guard).
//! - `modifier_handling_outside_the_keymap_is_on_a_reviewed_list`: every
//!   function that reads a modifier is a line of
//!   `fixtures/key-handlers-allow.txt` (`<file> <fn> | <class> | <reason>`),
//!   and each `chord` line's keys are checked against the keymap.
//!
//! The scan works on text, with these limits. A function is the innermost
//! `fn name` whose braces enclose the mention (closures belong to their
//! enclosing function; two functions of one name in one file share a line;
//! functions a macro writes are missed; a `#[cfg(test)]` item that is not a
//! `mod { }` tail is scanned). A modifier is read through a receiver named
//! `m`, `mods`, `modifiers` or `modifiers()`. A condition is the text
//! between `;`, `{` and `}`, so a match arm that follows another arm
//! without a block can borrow that arm's `control`.

use std::path::Path;

use crate::keymap;
use crate::platform_text::Platform;

const FIXTURE: &str = include_str!("../fixtures/key-handlers-allow.txt");
const FIXTURE_PATH: &str = "crates/blyg-app/fixtures/key-handlers-allow.txt";

// ==== SCAN ====

/// One fixture line: a function that reads a modifier, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    file: String,
    func: String,
    class: Class,
    reason: String,
    /// The line in the fixture, from 1.
    at: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Class {
    /// Only rejects or ignores modified keys (the bare-key idiom).
    Guard,
    /// A click or scroll modifier, not a key chord.
    Mouse,
    /// Handles its own chord(s), in keymap spelling as a Mac user presses them.
    Chord(Vec<String>),
    /// Honestly macOS-only code.
    MacOnly,
}

fn parse_fixture(text: &str) -> Result<Vec<Entry>, String> {
    let mut out: Vec<Entry> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = n + 1;
        let mut parts = line.splitn(3, '|');
        let (head, class, reason) = (
            parts.next().unwrap_or(""),
            parts.next().unwrap_or("").trim(),
            parts.next().unwrap_or("").trim(),
        );
        let words: Vec<&str> = head.split_whitespace().collect();
        let [file, func] = words[..] else {
            return Err(format!(
                "{FIXTURE_PATH}:{at}: want `<file> <fn> | <class> | <reason>`, got `{line}`"
            ));
        };
        let class = match class {
            "guard" => Class::Guard,
            "mouse" => Class::Mouse,
            "mac-only" => Class::MacOnly,
            c if c.starts_with("chord ") => Class::Chord(
                c["chord ".len()..]
                    .split_whitespace()
                    .map(str::to_string)
                    .collect(),
            ),
            c => {
                return Err(format!(
                    "{FIXTURE_PATH}:{at}: class `{c}` is not guard, mouse, mac-only or `chord <keys>`"
                ));
            }
        };
        if reason.is_empty() {
            return Err(format!("{FIXTURE_PATH}:{at}: a line needs a reason"));
        }
        if out.iter().any(|e| e.file == file && e.func == func) {
            return Err(format!(
                "{FIXTURE_PATH}:{at}: {file} {func} is listed twice"
            ));
        }
        out.push(Entry {
            file: file.into(),
            func: func.into(),
            class,
            reason: reason.into(),
            at,
        });
    }
    Ok(out)
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `src` as chars with comments and string and char literals blanked
/// (newlines kept, so offsets and line numbers match the original).
fn blank(src: &str) -> Vec<char> {
    let s: Vec<char> = src.chars().collect();
    let mut o = s.clone();
    let n = s.len();
    let wipe = |o: &mut Vec<char>, a: usize, b: usize| {
        for c in &mut o[a..b.min(n)] {
            if *c != '\n' {
                *c = ' ';
            }
        }
    };
    let mut i = 0;
    while i < n {
        let c = s[i];
        let next = s.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            let mut j = i;
            while j < n && s[j] != '\n' {
                j += 1;
            }
            wipe(&mut o, i, j);
            i = j;
        } else if c == '/' && next == Some('*') {
            let (mut depth, mut j) = (1, i + 2);
            while j < n && depth > 0 {
                match (s[j], s.get(j + 1).copied()) {
                    ('/', Some('*')) => {
                        depth += 1;
                        j += 2;
                    }
                    ('*', Some('/')) => {
                        depth -= 1;
                        j += 2;
                    }
                    _ => j += 1,
                }
            }
            wipe(&mut o, i, j);
            i = j;
        } else if c == 'r' && (i == 0 || !is_ident(s[i - 1])) && raw_string_end(&s, i).is_some() {
            let end = raw_string_end(&s, i).unwrap_or(i + 1);
            wipe(&mut o, i, end);
            i = end;
        } else if c == '"' {
            let mut j = i + 1;
            while j < n && s[j] != '"' {
                j += if s[j] == '\\' { 2 } else { 1 };
            }
            wipe(&mut o, i, j + 1);
            i = j + 1;
        } else if c == '\'' {
            let end = match (next, s.get(i + 2).copied()) {
                (Some('\\'), _) => (i + 2..n).find(|&j| s[j] == '\'').map(|j| j + 1),
                (Some(x), Some('\'')) if x != '\'' => Some(i + 3),
                _ => None, // a lifetime
            };
            if let Some(end) = end {
                wipe(&mut o, i, end);
                i = end;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    o
}

/// Where the raw string starting at `s[i] == 'r'` ends, if one starts there.
fn raw_string_end(s: &[char], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while s.get(j) == Some(&'#') {
        j += 1;
    }
    let hashes = j - i - 1;
    if s.get(j) != Some(&'"') {
        return None;
    }
    let mut k = j + 1;
    while k < s.len() {
        if s[k] == '"' && (1..=hashes).all(|h| s.get(k + h) == Some(&'#')) {
            return Some(k + 1 + hashes);
        }
        k += 1;
    }
    Some(s.len())
}

/// The length to keep: everything before a `#[cfg(test)]` `mod … {` tail.
fn live_len(code: &[char]) -> usize {
    let text: String = code.iter().collect();
    let mut at = 0;
    let lines: Vec<&str> = text.split('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        let start = at;
        at += line.chars().count() + 1;
        if !line.trim_start().starts_with("#[cfg(test)]") {
            continue;
        }
        // The item the attribute sits on: up to its first `{` or `;`.
        let mut item = line.trim_start()["#[cfg(test)]".len()..].to_string();
        let mut j = i;
        while !item.contains(['{', ';']) && j + 1 < lines.len() {
            j += 1;
            item.push(' ');
            item.push_str(lines[j]);
        }
        let head = item.split(['{', ';']).next().unwrap_or("");
        let mut words = head.split_whitespace().filter(|w| !w.starts_with("#["));
        let first = words.next().unwrap_or("");
        let word = if first.starts_with("pub") {
            words.next().unwrap_or("")
        } else {
            first
        };
        let block = item.contains('{') && item.find('{') < item.find(';').or(Some(usize::MAX));
        if word == "mod" && block {
            return start;
        }
    }
    code.len()
}

/// The source as `(original, code)`: both cut before the test tail, the
/// second with comments and literals blanked.
fn prepare(src: &str) -> (Vec<char>, Vec<char>) {
    let code = blank(src);
    let keep = live_len(&code);
    let orig: Vec<char> = src.chars().take(keep).collect();
    (orig, code[..keep].to_vec())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Platform,
    Control,
    Secondary,
    Other,
}

#[derive(Debug, Clone, Copy)]
struct Mention {
    /// The `.` before the name.
    pos: usize,
    kind: Kind,
    /// Under a `!` (`!m.control`, `!(a || m.control)`): it rejects the
    /// modifier instead of using it.
    negated: bool,
}

/// The identifier ending just before `code[end]` (empty if none).
fn ident_before(code: &[char], end: usize) -> String {
    let mut a = end;
    while a > 0 && is_ident(code[a - 1]) {
        a -= 1;
    }
    code[a..end].iter().collect()
}

/// The receiver of the `.` at `dot`, if it is a modifiers value: `m`,
/// `mods`, `modifiers`, or `modifiers()`. Returns where the receiver's
/// expression starts.
fn modifiers_receiver(code: &[char], dot: usize) -> Option<usize> {
    let mut end = dot;
    if end >= 2 && code[end - 1] == ')' && code[end - 2] == '(' {
        end -= 2;
    }
    let name = ident_before(code, end);
    if !matches!(name.as_str(), "m" | "mods" | "modifiers") {
        return None;
    }
    // Back over `a.b.c` and `x()` to the start of the whole expression.
    let mut a = end - name.chars().count();
    loop {
        if a >= 1 && code[a - 1] == '.' {
            a -= 1;
            if a >= 2 && code[a - 1] == ')' && code[a - 2] == '(' {
                a -= 2;
            }
            while a > 0 && is_ident(code[a - 1]) {
                a -= 1;
            }
        } else {
            return Some(a);
        }
    }
}

fn mentions(code: &[char]) -> Vec<Mention> {
    let mut out = Vec::new();
    for (dot, &c) in code.iter().enumerate() {
        if c != '.' {
            continue;
        }
        let name: String = code[dot + 1..]
            .iter()
            .take_while(|&&c| is_ident(c))
            .collect();
        let end = dot + 1 + name.chars().count();
        let next = code.get(end).copied();
        let (kind, start) = match name.as_str() {
            "platform" | "control" | "alt" | "shift" | "function" if next != Some('.') => {
                let Some(start) = modifiers_receiver(code, dot) else {
                    continue;
                };
                let kind = match name.as_str() {
                    "platform" => Kind::Platform,
                    "control" => Kind::Control,
                    _ => Kind::Other,
                };
                (kind, start)
            }
            "secondary" if next == Some('(') => (Kind::Secondary, dot),
            "number_of_modifiers" if next == Some('(') => (Kind::Other, dot),
            // Only `modifiers.modified()`: `m.modified()` is a file's mtime.
            "modified" if next == Some('(') && ident_before(code, dot) == "modifiers" => {
                let Some(start) = modifiers_receiver(code, dot) else {
                    continue;
                };
                (Kind::Other, start)
            }
            _ => continue,
        };
        out.push(Mention {
            pos: dot,
            kind,
            negated: negated(code, start),
        });
    }
    out
}

/// Whether the expression starting at `start` sits under a `!`: directly
/// (`!m.control`) or in a group (`!(… m.control …)`).
fn negated(code: &[char], start: usize) -> bool {
    let before = |at: usize| {
        code[..at]
            .iter()
            .rev()
            .find(|c| !c.is_whitespace())
            .copied()
    };
    if before(start) == Some('!') {
        return true;
    }
    let mut depth = 0;
    let mut j = start;
    while j > 0 {
        j -= 1;
        match code[j] {
            ';' | '{' | '}' => return false,
            ')' => depth += 1,
            '(' if depth > 0 => depth -= 1,
            '(' if before(j) == Some('!') => return true,
            _ => {}
        }
    }
    false
}

/// The `fn` each position is in: the innermost `fn name` whose body holds
/// it (`<none>` outside any).
fn enclosing_fns(code: &[char], positions: &[usize]) -> Vec<String> {
    let mut found = vec![String::from("<none>"); positions.len()];
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut pending: Option<String> = None;
    let mut parens = 0i32;
    let mut i = 0;
    let mut p = 0;
    let n = code.len();
    let order = {
        let mut o: Vec<usize> = (0..positions.len()).collect();
        o.sort_by_key(|&k| positions[k]);
        o
    };
    while i < n {
        while p < order.len() && positions[order[p]] <= i {
            if let Some(name) = stack.iter().rev().find_map(Clone::clone) {
                found[order[p]] = name;
            }
            p += 1;
        }
        let c = code[i];
        if (c.is_alphabetic() || c == '_') && (i == 0 || !is_ident(code[i - 1])) {
            let mut e = i;
            while e < n && is_ident(code[e]) {
                e += 1;
            }
            if code[i..e].iter().collect::<String>() == "fn" {
                let mut a = e;
                while a < n && code[a].is_whitespace() {
                    a += 1;
                }
                let mut b = a;
                while b < n && is_ident(code[b]) {
                    b += 1;
                }
                if b > a {
                    pending = Some(code[a..b].iter().collect());
                    parens = 0;
                    i = b;
                    continue;
                }
            }
            i = e;
            continue;
        }
        match c {
            '(' | '[' if pending.is_some() => parens += 1,
            ')' | ']' if pending.is_some() => parens -= 1,
            '{' => stack.push(pending.take()),
            '}' => {
                stack.pop();
            }
            ';' if parens <= 0 => pending = None,
            _ => {}
        }
        i += 1;
    }
    while p < order.len() {
        if let Some(name) = stack.iter().rev().find_map(Clone::clone) {
            found[order[p]] = name;
        }
        p += 1;
    }
    found
}

fn line_of(code: &[char], pos: usize) -> usize {
    1 + code[..pos].iter().filter(|&&c| c == '\n').count()
}

/// A place that reads a modifier.
#[derive(Debug)]
struct Site {
    file: String,
    func: String,
    line: usize,
    text: String,
}

/// Every modifier read in `src` (a file called `file`, from `src/`).
fn sites(file: &str, src: &str) -> Vec<Site> {
    let (orig, code) = prepare(src);
    let ms = mentions(&code);
    let funcs = enclosing_fns(&code, &ms.iter().map(|m| m.pos).collect::<Vec<_>>());
    ms.iter()
        .zip(funcs)
        .map(|(m, func)| {
            let line = line_of(&code, m.pos);
            Site {
                file: file.into(),
                func,
                line,
                text: orig_line(&orig, line),
            }
        })
        .collect()
}

fn orig_line(orig: &[char], line: usize) -> String {
    let text: String = orig.iter().collect();
    text.lines().nth(line - 1).unwrap_or("").trim().to_string()
}

/// A condition that reads the platform key without the Ctrl key.
#[derive(Debug)]
struct Alone {
    func: String,
    line: usize,
    condition: String,
}

/// The conditions in `src` that use `modifiers.platform` and neither
/// `modifiers.control` nor `secondary()`.
fn platform_alone(src: &str) -> Vec<Alone> {
    let (orig, code) = prepare(src);
    let ms = mentions(&code);
    let positions: Vec<usize> = ms.iter().map(|m| m.pos).collect();
    let funcs = enclosing_fns(&code, &positions);
    let mut out = Vec::new();
    for (m, func) in ms.iter().zip(funcs) {
        if m.kind != Kind::Platform || m.negated {
            continue;
        }
        let mut a = m.pos;
        while a > 0 && !matches!(code[a - 1], ';' | '{' | '}') {
            a -= 1;
        }
        let mut b = m.pos;
        while b < code.len() && !matches!(code[b], ';' | '{' | '}') {
            b += 1;
        }
        let with_control = ms.iter().any(|o| {
            (a..b).contains(&o.pos)
                && matches!(o.kind, Kind::Control | Kind::Secondary)
                && !o.negated
        });
        if !with_control {
            let condition: String = orig[a..b].iter().collect();
            out.push(Alone {
                func,
                line: line_of(&code, m.pos),
                condition: condition.split_whitespace().collect::<Vec<_>>().join(" "),
            });
        }
    }
    out
}

/// Sources of the crate that are not tests, as `(path from src/, text)`.
fn crate_sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).expect("read src") {
            let p = e.expect("entry").path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if p.is_dir() {
                if name != "tests" {
                    dirs.push(p);
                }
                continue;
            }
            let test_file = name == "tests.rs"
                || name.ends_with("_tests.rs")
                || name.ends_with("_e2e.rs")
                || name == "test_wait.rs";
            if name.ends_with(".rs") && !test_file {
                let rel = p.strip_prefix(&root).expect("under src");
                let rel = rel.to_string_lossy().replace('\\', "/");
                out.push((rel, std::fs::read_to_string(&p).expect("read")));
            }
        }
    }
    out.sort();
    out
}

const ACTION_HELP: &str = "bind the shortcut as an action in keymap.rs, so the Windows clash \
     and AltGr tests see it; if it truly can't be an action, add a line to \
     crates/blyg-app/fixtures/key-handlers-allow.txt with its class and reason";

/// Functions that read a modifier and have no line in `list`.
fn unlisted(sites: &[Site], list: &[Entry]) -> Vec<String> {
    let mut seen: Vec<(&str, &str)> = Vec::new();
    let mut out = Vec::new();
    for s in sites {
        if list.iter().any(|e| e.file == s.file && e.func == s.func)
            || seen.contains(&(&s.file, &s.func))
        {
            continue;
        }
        seen.push((&s.file, &s.func));
        out.push(format!(
            "  {} fn {}: reads a modifier at line {}: {}",
            s.file, s.func, s.line, s.text
        ));
    }
    out
}

/// Fixture lines whose function reads no modifier any more.
fn stale(sites: &[Site], list: &[Entry]) -> Vec<String> {
    list.iter()
        .filter(|e| !sites.iter().any(|s| s.file == e.file && s.func == e.func))
        .map(|e| {
            format!(
                "  {FIXTURE_PATH}:{}: no function {} in {} reads a modifier now: delete the line",
                e.at, e.func, e.file
            )
        })
        .collect()
}

/// Bound keys, as `(key, action)`.
type Bound = Vec<(String, String)>;

/// The keymap as the Mac and Windows tables bind it.
fn tables() -> (Bound, Bound) {
    let live = |p: Platform| -> Bound {
        keymap::table_for(p)
            .into_iter()
            .filter(|k| !k.key.is_empty())
            .map(|k| (k.key.to_string(), k.action.to_string()))
            .collect()
    };
    (live(Platform::Mac), live(Platform::Windows))
}

/// `chord` lines whose keys the keymap binds too, on the Mac (as written)
/// or on Windows (respelled the way `keymap::keys` does). A line whose
/// reason says `shares <context>` names where the keymap binds it and why
/// the handler is not live there.
fn chord_clashes(list: &[Entry], mac: &Bound, windows: &Bound) -> Vec<String> {
    let mut out = Vec::new();
    for e in list {
        let Class::Chord(chords) = &e.class else {
            continue;
        };
        if e.reason.contains("shares ") {
            continue;
        }
        for chord in chords {
            let on_mac = keymap::normalize(chord);
            let on_windows = keymap::normalize(&keymap::respell_for_windows(chord));
            for (key, action) in mac {
                if keymap::normalize(key) == on_mac {
                    out.push(format!(
                        "  {} {}: chord {chord} is also bound on the Mac ({key}, {action})",
                        e.file, e.func
                    ));
                }
            }
            for (key, action) in windows {
                if keymap::normalize(key) == on_windows {
                    out.push(format!(
                        "  {} {}: chord {chord} is bound on Windows as {key} ({action})",
                        e.file, e.func
                    ));
                }
            }
        }
    }
    out
}

fn fixture() -> Vec<Entry> {
    parse_fixture(FIXTURE).unwrap_or_else(|e| panic!("{e}"))
}

/// A chord on the platform key alone is dead on Windows.
#[test]
fn no_chord_reads_the_platform_key_alone() {
    let list = fixture();
    let mut bad = Vec::new();
    for (file, src) in crate_sources() {
        for a in platform_alone(&src) {
            let mac_only = list
                .iter()
                .any(|e| e.file == file && e.func == a.func && e.class == Class::MacOnly);
            if !mac_only {
                bad.push(format!("  {file}:{}: {}", a.line, a.condition));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "these conditions read modifiers.platform without modifiers.control, so the \
         chord is dead on Windows, where `platform` is the Windows key (the system's):\n{}\n\
         Write modifiers.secondary() (Cmd on a Mac, Ctrl elsewhere), or accept \
         `platform || control`, or bind an action in keymap.rs, where the Windows \
         spelling is checked. Code that is honestly macOS-only goes on the list in \
         {FIXTURE_PATH} as `mac-only`.",
        bad.join("\n")
    );
}

/// The ratchet: a modifier read outside the keymap is a reviewed line.
#[test]
fn modifier_handling_outside_the_keymap_is_on_a_reviewed_list() {
    let list = fixture();
    let all: Vec<Site> = crate_sources()
        .iter()
        .flat_map(|(file, src)| sites(file, src))
        .collect();
    let bad = unlisted(&all, &list);
    assert!(
        bad.is_empty(),
        "functions that read a keyboard modifier, with no line on the reviewed list:\n{}\n{ACTION_HELP}.",
        bad.join("\n")
    );
    let old = stale(&all, &list);
    assert!(old.is_empty(), "stale lines:\n{}", old.join("\n"));
    let (mac, windows) = tables();
    let clashes = chord_clashes(&list, &mac, &windows);
    assert!(
        clashes.is_empty(),
        "chords a handler takes that the keymap binds as well:\n{}\nOne of the two never \
         sees the key where both are live. Move the handler into the keymap, or pick \
         another key; if the two are never live at once, say `shares <context>` in the \
         line's reason.",
        clashes.join("\n")
    );
}

// ==== END SCAN ====

// ==== SCAN: test code ====

/// STUB (the scan comes in the next commit).
fn bare_platform_hits(_file: &str, _src: &str) -> Vec<String> {
    Vec::new()
}

// ==== END SCAN: test code ====

/// The handler before the chip-keys-win merge: Alt+1 to Alt+9 ticked the
/// Publish sheet's chips (`extensions/auto_crosspost.rs`, from
/// `capture_key_down` in `sheet/publish.rs`).
const ALT_DIGITS: &str = r#"
impl MainView {
    pub(crate) fn auto_crosspost_key(
        &mut self,
        id: &LocalId,
        ks: &Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let m = &ks.modifiers;
        if !m.alt || m.control || m.platform || m.shift || m.function {
            return false;
        }
        let Some(n) = ks.key.parse::<usize>().ok() else {
            return false;
        };
        true
    }
}
"#;

/// About's close before the same merge.
const OLD_ABOUT: &str = r#"
fn about(cx: &mut Context<Self>) {
    div().on_key_down(cx.listener(|_, ev: &KeyDownEvent, window, cx| {
        let k = &ev.keystroke;
        if k.key == "escape" || (k.modifiers.platform && k.key == "w") {
            cx.stop_propagation();
            window.remove_window();
        }
    }))
}
"#;

/// The Reader's select-all before the same merge.
const OLD_PICK_ALL: &str = r#"
fn on_key(&mut self, k: &Keystroke) {
    if self.reading.view == View::Reading
        && self.reading.mode == stream_vm::ReadMode::Reader
        && k.modifiers.platform
        && !(k.modifiers.control || k.modifiers.alt || k.modifiers.shift)
        && k.key == "a"
    {
        self.pick_all(cx);
    }
}
"#;

fn entries(text: &str) -> Vec<Entry> {
    parse_fixture(text).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_alt_digit_handler_is_unlisted_and_then_clashes_on_windows() {
    let file = "extensions/auto_crosspost.rs";
    let all = sites(file, ALT_DIGITS);
    assert!(
        all.iter().all(|s| s.func == "auto_crosspost_key"),
        "{all:?}"
    );
    assert_eq!(
        all.len(),
        5,
        "alt, control, platform, shift, function: {all:?}"
    );
    // The guard names control too, so the platform scan lets it be.
    assert!(platform_alone(ALT_DIGITS).is_empty());

    let bad = unlisted(&all, &[]);
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert!(
        bad[0].contains("auto_crosspost_key") && bad[0].contains("line 10"),
        "{bad:?}"
    );

    // Listed as the chord it is, the Windows keymap already has alt-1.
    let line = format!("{file} auto_crosspost_key | chord alt-1 | taken by hand");
    let list = entries(&line);
    assert!(unlisted(&all, &list).is_empty());
    let (mac, windows) = tables();
    let clashes = chord_clashes(&list, &mac, &windows);
    assert!(
        clashes
            .iter()
            .any(|c| c.contains("bound on Windows as alt-1")),
        "{clashes:?}"
    );
    assert!(
        !clashes.iter().any(|c| c.contains("on the Mac")),
        "{clashes:?}"
    );
    // A reason that says what it shares excuses it.
    let line = format!("{file} auto_crosspost_key | chord alt-1 | shares the sheet context |");
    assert!(chord_clashes(&entries(&line), &mac, &windows).is_empty());
}

#[test]
fn a_chord_the_mac_table_binds_clashes_too() {
    let list = entries("x.rs f | chord cmd-w | a handler for the closing key");
    let (mac, windows) = tables();
    let clashes = chord_clashes(&list, &mac, &windows);
    assert!(
        clashes.iter().any(|c| c.contains("on the Mac")),
        "{clashes:?}"
    );
    assert!(
        clashes.iter().any(|c| c.contains("on Windows as ctrl-w")),
        "{clashes:?}"
    );
}

#[test]
fn the_old_about_line_reads_the_platform_key_alone() {
    let bad = platform_alone(OLD_ABOUT);
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert_eq!(bad[0].func, "about");
    assert_eq!(bad[0].line, 5);
    assert_eq!(
        bad[0].condition,
        "if k.key == \"escape\" || (k.modifiers.platform && k.key == \"w\")"
    );
}

#[test]
fn the_old_select_all_names_control_only_to_reject_it() {
    let bad = platform_alone(OLD_PICK_ALL);
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert_eq!(bad[0].line, 5);
}

#[test]
fn platform_with_control_or_secondary_passes() {
    for ok in [
        "fn f(k: K) { if k.modifiers.platform || k.modifiers.control { go(); } }",
        "fn f(k: K) { if k.modifiers.control || k.modifiers.platform { go(); } }",
        "fn f(k: K) { if k.modifiers.secondary() { go(); } }",
        "fn f(k: K) { if m.platform && m.secondary() { go(); } }",
        "fn f(k: K) { if ev.modifiers().platform || ev.modifiers().control { go(); } }",
    ] {
        assert!(platform_alone(ok).is_empty(), "{ok}");
    }
    for bad in [
        "fn f(k: K) { if m.platform && !m.shift { go(); } }",
        "fn f(k: K) { if m.platform && !m.control { go(); } }",
        "fn f(w: W) { let m = w.modifiers(); (m.platform, m.alt) }",
    ] {
        assert_eq!(platform_alone(bad).len(), 1, "{bad}");
    }
}

#[test]
fn a_guard_over_two_lines_passes() {
    let src = "fn f(k: K) {\n    if !(k.modifiers.platform\n        || k.modifiers.control\n        || k.modifiers.alt)\n    {\n        go();\n    }\n}\n";
    assert!(platform_alone(src).is_empty());
    let src = "fn f(k: K) {\n    if k.modifiers.platform\n        || k.modifiers.control\n        || k.modifiers.alt\n    {\n        return;\n    }\n}\n";
    assert!(platform_alone(src).is_empty());
    let src = "fn f(k: K) { if page && !k.modifiers.platform && !k.modifiers.alt { go(); } }";
    assert!(
        platform_alone(src).is_empty(),
        "a negated platform key is a guard"
    );
}

#[test]
fn longer_names_are_not_mentions() {
    let src = "fn f(m: M) { m.platform_text(); m.controller.go(); self.alt.as_deref(); \
               x.shift_by(1); m.platform.name(); other.platform; y.function_name(); \
               m.modified_at(); w.modified(); }";
    assert!(sites("a.rs", src).is_empty(), "{:?}", sites("a.rs", src));
    assert!(platform_alone(src).is_empty());
    // The same words as real reads are mentions.
    let src = "fn f(m: M) { if m.platform || m.shift {} }";
    assert_eq!(sites("a.rs", src).len(), 2);
}

#[test]
fn comments_strings_and_test_tails_are_ignored() {
    let src = r##"
fn live() {
    // k.modifiers.platform is a comment here
    /* and k.modifiers.platform in a block /* nested */ */
    let s = "k.modifiers.platform";
    let r = r#"k.modifiers.platform "quoted""#;
    let c = '{';
    let x: &'static str = "";
}

#[cfg(test)]
mod tests {
    fn t(k: K) { if k.modifiers.platform { go(); } }
}
"##;
    assert!(sites("a.rs", src).is_empty(), "{:?}", sites("a.rs", src));
    assert!(platform_alone(src).is_empty());
    // A `#[cfg(test)] mod x;` declaration is no tail; the code after it counts.
    let src = "#[cfg(test)]\nmod x_tests;\nfn f(m: M) { if m.platform { go(); } }\n";
    assert_eq!(platform_alone(src).len(), 1);
}

#[test]
fn the_function_is_the_innermost_that_holds_the_read() {
    let src = "impl V {\n    fn outer(&self) {\n        fn inner(m: M) -> bool { m.alt }\n        let f = |m: M| m.shift;\n        if m.control {}\n    }\n    fn next(&self) { x.modifiers.function; }\n}\n";
    let got: Vec<(String, usize)> = sites("a.rs", src)
        .into_iter()
        .map(|s| (s.func, s.line))
        .collect();
    assert_eq!(
        got,
        [
            ("inner".to_string(), 3),
            ("outer".to_string(), 4),
            ("outer".to_string(), 5),
            ("next".to_string(), 7),
        ]
    );
}

#[test]
fn a_stale_fixture_line_fails() {
    let src = "fn f(m: M) { if m.alt {} }";
    let all = sites("a.rs", src);
    let list = entries("a.rs f | guard | bare keys only\na.rs gone | guard | was renamed\n");
    assert!(unlisted(&all, &list).is_empty());
    let old = stale(&all, &list);
    assert_eq!(old.len(), 1, "{old:?}");
    assert!(old[0].contains("gone") && old[0].contains(":2:"), "{old:?}");
}

#[test]
fn the_fixture_format_is_checked() {
    for bad in [
        "a.rs f | guard",
        "a.rs | guard | r",
        "a.rs f | chord | r",
        "a.rs f | sometimes | r",
        "a.rs f | guard | r\na.rs f | mouse | r",
    ] {
        assert!(parse_fixture(bad).is_err(), "{bad}");
    }
    let ok = entries("# c\n\na.rs f | chord cmd-backspace alt-up | why\nb.rs g | mac-only | r");
    assert_eq!(
        ok[0].class,
        Class::Chord(vec!["cmd-backspace".into(), "alt-up".into()])
    );
    assert_eq!(ok[1].class, Class::MacOnly);
}

// ---- the platform key in test code ----

/// The hoppers picker test's click before 74ed1d0: the platform key is the
/// Windows key on Windows, so `pick_click` there ignored it.
const OLD_HOPPERS_CLICK: &str = r##"view.update(cx, |v, cx| {
    let m = gpui_kit::Modifiers {
        platform: true,
        ..Default::default()
    };
    assert!(v.pick_click(&ada, m, cx));
});
"##;

/// The same test after the fix.
const NEW_HOPPERS_CLICK: &str = r##"view.update(cx, |v, cx| {
    assert!(v.pick_click(&ada, gpui_kit::Modifiers::secondary_key(), cx));
});
"##;

/// `cmd_click` from `reading/pick_all_keys_tests.rs`: the platform key sits in
/// the Mac arm.
const CMD_CLICK: &str = r##"fn cmd_click(p: Platform) -> Modifiers {
    match p {
        Platform::Mac => Modifiers {
            platform: true,
            ..Default::default()
        },
        Platform::Windows => Modifiers {
            control: true,
            ..Default::default()
        },
    }
}
"##;

/// The same helper with the two arms' bodies swapped.
const CMD_CLICK_SWAPPED: &str = r##"fn cmd_click(p: Platform) -> Modifiers {
    match p {
        Platform::Mac => Modifiers {
            control: true,
            ..Default::default()
        },
        Platform::Windows => Modifiers {
            platform: true,
            ..Default::default()
        },
    }
}
"##;

const HOPPERS: &str = "reading/hoppers_tests.rs";

#[test]
fn the_old_hoppers_click_holds_the_platform_key_bare() {
    let bad = bare_platform_hits(HOPPERS, OLD_HOPPERS_CLICK);
    assert_eq!(bad, [format!("{HOPPERS}:3: platform: true,")], "{bad:?}");
}

#[test]
fn the_secondary_key_passes() {
    assert!(bare_platform_hits(HOPPERS, NEW_HOPPERS_CLICK).is_empty());
}

#[test]
fn the_mac_arm_passes_and_the_windows_arm_fails() {
    let file = "reading/pick_all_keys_tests.rs";
    assert!(bare_platform_hits(file, CMD_CLICK).is_empty());
    let bad = bare_platform_hits(file, CMD_CLICK_SWAPPED);
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert!(bad[0].starts_with(&format!("{file}:8:")), "{bad:?}");
    // A single-line arm, and an arm for both platforms.
    let one = "fn f(p: Platform) -> M {\n    match p {\n        Platform::Mac => M { platform: true },\n        _ => M::default(),\n    }\n}\n";
    assert!(bare_platform_hits(file, one).is_empty());
    let both = "fn f(p: Platform) -> M {\n    match p {\n        Platform::Mac | Platform::Windows => M {\n            platform: true,\n        },\n    }\n}\n";
    assert_eq!(bare_platform_hits(file, both).len(), 1);
    // Not a `Platform` path: `MyPlatform::Mac`.
    let other = "fn f(p: Os) -> M {\n    match p {\n        MyPlatform::Mac => M {\n            platform: true,\n        },\n    }\n}\n";
    assert_eq!(bare_platform_hits(file, other).len(), 1);
}

#[test]
fn constructors_that_set_the_platform_key_fail() {
    for ctor in ["command", "command_shift", "windows", "super_key"] {
        let src = format!("fn t() {{ let m = Modifiers::{ctor}(); }}\n");
        assert_eq!(bare_platform_hits("a_tests.rs", &src).len(), 1, "{src}");
        let src = format!("fn t() {{ let m = gpui_kit::Modifiers::{ctor} (); }}\n");
        assert_eq!(bare_platform_hits("a_tests.rs", &src).len(), 1, "{src}");
    }
    for ok in [
        "let m = Modifiers::secondary_key();",
        "let m = Modifiers::control();",
        "let m = Modifiers::none();",
        "let m = Modifiers::command_ish();",
        "let m = MyModifiers::command();",
        "let m = Modifiers::command;",
    ] {
        let src = format!("fn t() {{ {ok} }}\n");
        assert!(bare_platform_hits("a_tests.rs", &src).is_empty(), "{src}");
    }
}

#[test]
fn longer_names_comments_and_strings_are_not_hits() {
    let src = r##"fn t() {
    let a = M { some_platform: true };
    let b = M { platform_key: true };
    let c = M { platform: false };
    let d = M { platform: truest };
    // platform: true in a comment, and Modifiers::command()
    /* platform: true */
    let s = "platform: true";
    let r = r#"Modifiers::command()"#;
}
"##;
    let bad = bare_platform_hits("a_tests.rs", src);
    assert!(bad.is_empty(), "{bad:?}");
    // Whitespace around the colon still hits.
    let src = "fn t() { let a = M { platform:true }; let b = M { platform :  true }; }\n";
    assert_eq!(bare_platform_hits("a_tests.rs", src).len(), 1, "one line");
}

#[test]
fn the_marker_makes_a_hit_pass() {
    let same = "fn t() {\n    let m = M { platform: true }; // the platform key itself: no chord reads it alone\n}\n";
    assert!(bare_platform_hits("a_tests.rs", same).is_empty());
    let above = "fn t() {\n    // the platform key itself, pressed on purpose\n    let m = Modifiers::command();\n}\n";
    assert!(bare_platform_hits("a_tests.rs", above).is_empty());
    // Two lines above, or in code instead of a comment, does not count.
    let far = "fn t() {\n    // the platform key itself\n    let x = 1;\n    let m = Modifiers::command();\n}\n";
    assert_eq!(bare_platform_hits("a_tests.rs", far).len(), 1);
    let code = "fn t() {\n    let the_platform_key_itself = M { platform: true };\n}\n";
    assert_eq!(bare_platform_hits("a_tests.rs", code).len(), 1);
}

#[test]
fn only_test_code_is_scanned() {
    let product = "fn live() -> M { M { platform: true } }\n";
    for file in ["lib.rs", "reading/hoppers.rs", "app/testsish.rs"] {
        assert!(bare_platform_hits(file, product).is_empty(), "{file}");
    }
    for file in [
        "a_tests.rs",
        "tests.rs",
        "update/tests.rs",
        "tests/cli.rs",
        "x/tests/y.rs",
        "a_e2e.rs",
    ] {
        assert_eq!(bare_platform_hits(file, product).len(), 1, "{file}");
    }
    // The tail of a product file is test code; what is above it is not.
    let tail = format!(
        "{product}\n#[cfg(test)]\nmod tests {{\n    fn t() -> M {{ M {{ platform: true }} }}\n}}\n"
    );
    let bad = bare_platform_hits("lib.rs", &tail);
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert!(bad[0].starts_with("lib.rs:5:"), "{bad:?}");
}

#[test]
fn hand_built_keystrokes_with_the_platform_key_fail() {
    for bad in [
        r#"cx.simulate_keystrokes("cmd-n");"#,
        r#"cx.simulate_keystrokes("a cmd-shift-z");"#,
        r#"let k = Keystroke::parse("alt-cmd-left").unwrap();"#,
        r#"window.dispatch_keystroke("cmd-w", cx);"#,
    ] {
        let src = format!("fn t() {{ {bad} }}\n");
        assert_eq!(bare_platform_hits("a_tests.rs", &src).len(), 1, "{src}");
    }
    for ok in [
        r#"cx.simulate_keystrokes(&crate::keymap::keys("cmd-n"));"#,
        r#"cx.simulate_keystrokes("escape");"#,
        r#"let k = Keystroke::parse(&keys("cmd-n")).unwrap();"#,
        r#"let k = Keystroke::parse("escape").unwrap();"#,
        r#"key(cx, "cmd-n");"#,
    ] {
        let src = format!("fn t() {{ {ok} }}\n");
        assert!(bare_platform_hits("a_tests.rs", &src).is_empty(), "{src}");
    }
}
