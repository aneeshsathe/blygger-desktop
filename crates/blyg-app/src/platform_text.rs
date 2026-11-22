//! Platform wording: the one place the app's text is chosen per platform.
//!
//! Text is written in macOS terms (`⌘`, `⌥`, `⇧`, `⌃`, "this Mac") and shown
//! as this platform spells it: unchanged on macOS, respelled on Windows the
//! way the bindings are (`keymap::windows_hint`: `⌥⌘B` → `Ctrl+Shift+A`,
//! "this Mac" → "this PC"). So a key in the text follows the keymap,
//! `WINDOWS_OVERRIDES` included, with no second spelling to keep up.
//!
//! A string literal with a modifier glyph reaches the screen one of two ways:
//! - at the point of use, through `keymap::hint` (or `hint_owned` for
//!   `format!`ed text);
//! - as a [`Text`] (`ui_text`, or `ui_text_per_platform` when Windows says
//!   something else, not just other keys), for tables and constants, read
//!   with [`Text::get`].
//!
//! `tests::literal_modifier_glyphs_go_through_the_builder` scans the crate
//! and fails on any other literal glyph, naming these.

/// The platform whose wording the app uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Mac,
    Windows,
}

impl Platform {
    /// The platform this build runs on (Linux builds speak macOS).
    pub const HERE: Platform = if cfg!(target_os = "windows") {
        Platform::Windows
    } else {
        Platform::Mac
    };

    /// The platform whose spelling the app's text uses: [`Self::HERE`].
    /// Tests can pick either ([`spelled_as`] for one test,
    /// `BLYGGER_TEST_SPELLING=windows` for the whole suite), so a macOS run
    /// checks Windows' wording too. Outside tests it's the constant.
    #[inline]
    pub fn spelling() -> Platform {
        #[cfg(test)]
        {
            test_spelling::current()
        }
        #[cfg(not(test))]
        {
            Self::HERE
        }
    }
}

/// Run `f` with the app's text spelled as `p` on this thread (tests only):
/// `spelled_as(Platform::Windows, || toggled_toast(true))` is
/// "Added to Saved · Ctrl+B" on any machine. The previous spelling comes
/// back afterwards, panic or not. Only the wording changes, not the key
/// bindings.
#[cfg(test)]
pub fn spelled_as<R>(p: Platform, f: impl FnOnce() -> R) -> R {
    let _restore = test_spelling::Override::set(p);
    f()
}

/// The spelling tests use: a per-thread override (`spelled_as`) over a
/// process-wide default (`BLYGGER_TEST_SPELLING=windows|mac`, read once),
/// which every thread sees, GPUI's and spawned ones alike.
#[cfg(test)]
mod test_spelling {
    use super::Platform;
    use std::cell::Cell;
    use std::sync::OnceLock;

    thread_local! {
        static OVERRIDE: Cell<Option<Platform>> = const { Cell::new(None) };
    }

    fn default() -> Platform {
        static DEFAULT: OnceLock<Platform> = OnceLock::new();
        *DEFAULT.get_or_init(|| {
            crate::dev_flags::TestEnv::from_env()
                .spelling
                .unwrap_or(Platform::HERE)
        })
    }

    pub fn current() -> Platform {
        OVERRIDE.with(Cell::get).unwrap_or_else(default)
    }

    /// Puts the previous override back when dropped (also when unwinding).
    pub struct Override(Option<Platform>);

    impl Override {
        pub fn set(p: Platform) -> Self {
            Self(OVERRIDE.with(|o| o.replace(Some(p))))
        }
    }

    impl Drop for Override {
        fn drop(&mut self) {
            OVERRIDE.with(|o| o.set(self.0));
        }
    }
}

/// App text in macOS terms, shown as each platform spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Text {
    mac: &'static str,
    /// Windows' own wording (still respelled), when it differs by more
    /// than keys: a step a Windows user does another way.
    windows: Option<&'static str>,
}

/// Text whose Windows wording is the macOS text respelled.
pub const fn ui_text(mac: &'static str) -> Text {
    Text { mac, windows: None }
}

/// Text Windows says differently (not just with other keys). The tests
/// refuse a `windows` that the respelling would have produced anyway.
pub const fn ui_text_per_platform(mac: &'static str, windows: &'static str) -> Text {
    Text {
        mac,
        windows: Some(windows),
    }
}

impl Text {
    /// As this platform shows it. Interned on Windows, so it lives as long
    /// as the literal did.
    pub fn get(self) -> &'static str {
        crate::keymap::hint(self.source(Platform::spelling()))
    }

    /// As `p` shows it, on any platform: the tests read Windows' wording on
    /// macOS.
    #[cfg_attr(not(test), expect(dead_code, reason = "tests use it"))]
    pub fn on(self, p: Platform) -> String {
        match p {
            Platform::Mac => self.mac.to_string(),
            Platform::Windows => crate::keymap::windows_hint(self.source(p)),
        }
    }

    /// The text as written for `p`, before respelling.
    pub fn source(self, p: Platform) -> &'static str {
        match p {
            Platform::Mac => self.mac,
            Platform::Windows => self.windows.unwrap_or(self.mac),
        }
    }

    /// The Windows wording, when it's its own.
    #[cfg_attr(not(test), expect(dead_code, reason = "tests use it"))]
    pub fn windows_source(self) -> Option<&'static str> {
        self.windows
    }
}

#[cfg(test)]
pub(crate) mod scan {
    //! A small Rust tokenizer: enough to find string literals, what call
    //! they sit in, and `#[cfg(test)]` items, without a parser dependency.

    /// A string or char literal in the source.
    pub struct Literal {
        pub line: usize,
        pub text: String,
        /// The innermost enclosing call's name, for each enclosing `(`,
        /// innermost last (`hint`, `format!`, …; `""` for a bare paren).
        pub callers: Vec<String>,
        pub is_char: bool,
    }

    enum Tok {
        Ident(String),
        Punct(char),
        Lit(Literal),
    }

    fn tokens(src: &str) -> Vec<Tok> {
        let c: Vec<char> = src.chars().collect();
        let mut out = Vec::new();
        let (mut i, mut line) = (0, 1);
        let ident_char = |ch: char| ch.is_alphanumeric() || ch == '_';
        while i < c.len() {
            let ch = c[i];
            if ch == '\n' {
                line += 1;
                i += 1;
            } else if ch.is_whitespace() {
                i += 1;
            } else if ch == '/' && c.get(i + 1) == Some(&'/') {
                while i < c.len() && c[i] != '\n' {
                    i += 1;
                }
            } else if ch == '/' && c.get(i + 1) == Some(&'*') {
                let mut depth = 0;
                while i < c.len() {
                    if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                        depth += 1;
                        i += 2;
                    } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        if c[i] == '\n' {
                            line += 1;
                        }
                        i += 1;
                    }
                }
            } else if ch == '"'
                || ((ch == 'r' || ch == 'b')
                    && (i == 0 || !ident_char(c[i - 1]))
                    && raw_or_byte_start(&c[i..]).is_some())
            {
                let start_line = line;
                let (hashes, open) = if ch == '"' {
                    (None, 1)
                } else {
                    raw_or_byte_start(&c[i..]).expect("checked")
                };
                i += open;
                let mut text = String::new();
                while let Some(&d) = c.get(i) {
                    match hashes {
                        Some(h)
                            if d == '"'
                                && c.len() > i + h
                                && c[i + 1..=i + h].iter().all(|x| *x == '#') =>
                        {
                            i += 1 + h;
                            break;
                        }
                        None if d == '\\' => {
                            if let Some(&e) = c.get(i + 1) {
                                if e == '\n' {
                                    line += 1;
                                }
                                text.push('\\');
                                text.push(e);
                            }
                            i += 2;
                            continue;
                        }
                        None if d == '"' => {
                            i += 1;
                            break;
                        }
                        _ => {}
                    }
                    if d == '\n' {
                        line += 1;
                    }
                    text.push(d);
                    i += 1;
                }
                out.push(Tok::Lit(Literal {
                    line: start_line,
                    text,
                    callers: Vec::new(),
                    is_char: false,
                }));
            } else if ch == '\'' {
                // A char literal ('x', '\n', '\u{..}') or a lifetime.
                let close = if c.get(i + 1) == Some(&'\\') {
                    c[i + 3..]
                        .iter()
                        .position(|x| *x == '\'')
                        .map(|p| i + 3 + p)
                } else if c.get(i + 2) == Some(&'\'') {
                    Some(i + 2)
                } else {
                    None
                };
                match close {
                    Some(end) => {
                        out.push(Tok::Lit(Literal {
                            line,
                            text: c[i + 1..end].iter().collect(),
                            callers: Vec::new(),
                            is_char: true,
                        }));
                        i = end + 1;
                    }
                    None => i += 1,
                }
            } else if ident_char(ch) {
                let start = i;
                while i < c.len() && ident_char(c[i]) {
                    i += 1;
                }
                let mut name: String = c[start..i].iter().collect();
                if c.get(i) == Some(&'!') && c.get(i + 1) != Some(&'=') {
                    name.push('!');
                    i += 1;
                }
                out.push(Tok::Ident(name));
            } else {
                out.push(Tok::Punct(ch));
                i += 1;
            }
        }
        out
    }

    /// `r"`, `r#"`, `b"`, `br#"`: (hashes for a raw string, prefix length).
    fn raw_or_byte_start(c: &[char]) -> Option<(Option<usize>, usize)> {
        let mut i = 0;
        if c.first() == Some(&'b') {
            i += 1;
        }
        if c.get(i) == Some(&'r') {
            i += 1;
            let mut h = 0;
            while c.get(i) == Some(&'#') {
                h += 1;
                i += 1;
            }
            return (c.get(i) == Some(&'"')).then_some((Some(h), i + 1));
        }
        (i == 1 && c.get(1) == Some(&'"')).then_some((None, 2))
    }

    /// Every literal outside `#[cfg(test)]` items, with its enclosing calls.
    pub fn literals(src: &str) -> Vec<Literal> {
        let toks = tokens(src);
        let mut out = Vec::new();
        // One entry per open bracket: the call it belongs to, if a paren.
        let mut stack: Vec<Option<String>> = Vec::new();
        let mut i = 0;
        while i < toks.len() {
            if is_cfg_test(&toks[i..]) {
                i = skip_item(&toks, i + 7);
                continue;
            }
            match &toks[i] {
                Tok::Punct(p @ ('(' | '[' | '{')) => {
                    let caller = match (p, i.checked_sub(1).map(|j| &toks[j])) {
                        ('(', Some(Tok::Ident(name))) => Some(name.clone()),
                        ('(', _) => Some(String::new()),
                        _ => None,
                    };
                    stack.push(caller);
                }
                Tok::Punct(')' | ']' | '}') => {
                    stack.pop();
                }
                Tok::Lit(l) => out.push(Literal {
                    line: l.line,
                    text: l.text.clone(),
                    callers: stack.iter().flatten().cloned().collect(),
                    is_char: l.is_char,
                }),
                _ => {}
            }
            i += 1;
        }
        out
    }

    fn is_cfg_test(t: &[Tok]) -> bool {
        let want = ["#", "[", "cfg", "(", "test", ")", "]"];
        t.len() >= want.len()
            && t.iter().zip(want).all(|(t, w)| match t {
                Tok::Punct(p) => w.len() == 1 && w.starts_with(*p),
                Tok::Ident(s) => s == w,
                Tok::Lit(_) => false,
            })
    }

    /// The index after the item starting at `i`: through its `;`, or its
    /// first brace block.
    fn skip_item(t: &[Tok], mut i: usize) -> usize {
        let mut depth = 0usize;
        while i < t.len() {
            match t[i] {
                Tok::Punct('{' | '(' | '[') => depth += 1,
                Tok::Punct('}' | ')' | ']') => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 && matches!(t[i], Tok::Punct('}')) {
                        return i + 1;
                    }
                }
                Tok::Punct(';') if depth == 0 => return i + 1,
                _ => {}
            }
            i += 1;
        }
        i
    }

    /// The crate's non-test sources: `(path from src/, contents)`.
    pub fn crate_sources() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        let mut dirs = vec![root.clone()];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).expect("read src") {
                let p = e.expect("entry").path();
                if p.is_dir() {
                    dirs.push(p);
                    continue;
                }
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
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
}

/// The key a test sends for redo in a text field: Ctrl+Y on Windows, where
/// the field binds no Ctrl+Shift+Z, and Cmd+Shift+Z on the Mac. Tests never
/// spell redo as a literal (`tests_dont_send_macos_only_caret_keys`).
#[cfg(test)]
pub(crate) fn redo_keys() -> &'static str {
    if cfg!(target_os = "windows") {
        "ctrl-y"
    } else {
        "cmd-shift-z"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GLYPHS: [char; 4] = ['⌘', '⌥', '⇧', '⌃'];

    /// Calls a glyph literal may sit in: the respelling itself, or a `Text`.
    /// `kb!`/`menu_only!` are keymap rows, whose labels reach the screen
    /// through `keymap::hint` (the palette, `+list-keybinds`) and are the
    /// macOS shortcut reference's text (`keymap_docs`).
    const BUILDERS: &[&str] = &[
        "hint",
        "hint_owned",
        "ui_text",
        "ui_text_per_platform",
        "kb!",
        "menu_only!",
    ];

    /// Literal glyphs that are the builder's own parts, or never shown.
    /// (file, the literal, why).
    const ALLOWED: &[(&str, &str, &str)] = &[
        (
            "keymap.rs",
            "⌃",
            "the glyph tables of `windows_hint`/`mac_glyphs`",
        ),
        (
            "keymap.rs",
            "⌥",
            "the glyph tables of `windows_hint`/`mac_glyphs`",
        ),
        (
            "keymap.rs",
            "⇧",
            "the glyph tables of `windows_hint`/`mac_glyphs`",
        ),
        (
            "keymap.rs",
            "⌘",
            "the glyph tables of `windows_hint`/`mac_glyphs`",
        ),
        ("prefs.rs", "⌃", "the macOS hotkey glyphs (`hotkey_label`)"),
        ("prefs.rs", "⌥", "the macOS hotkey glyphs (`hotkey_label`)"),
        ("prefs.rs", "⇧", "the macOS hotkey glyphs (`hotkey_label`)"),
        ("prefs.rs", "⌘", "the macOS hotkey glyphs (`hotkey_label`)"),
    ];

    /// Every literal with a modifier glyph that isn't built for the platform.
    fn offenders() -> Vec<String> {
        let mut bad = Vec::new();
        for (path, src) in scan::crate_sources() {
            for l in scan::literals(&src) {
                if !l.text.contains(GLYPHS) {
                    continue;
                }
                if l.callers.iter().any(|c| BUILDERS.contains(&c.as_str())) {
                    continue;
                }
                let file = path.rsplit('/').next().unwrap_or(&path);
                if ALLOWED.iter().any(|(f, t, _)| *f == file && *t == l.text) {
                    continue;
                }
                let short: String = l.text.chars().take(60).collect();
                bad.push(format!("  src/{path}:{} {short:?}", l.line));
            }
        }
        bad
    }

    /// Text with ⌘ ⌥ ⇧ ⌃ shows those glyphs on Windows unless it goes through
    /// the respelling: 4cd9895 and 7e45653 found and wrapped such text by
    /// hand, twice. Now a new one fails here.
    #[test]
    fn literal_modifier_glyphs_go_through_the_builder() {
        let bad = offenders();
        assert!(
            bad.is_empty(),
            "string literals with ⌘ ⌥ ⇧ ⌃ that Windows would show as is:\n{}\n\
             Wrap the literal in crate::keymap::hint(\"…\") (hint_owned(format!(…)) for \
             built text), or make it a crate::platform_text::Text (ui_text(\"…\"), or \
             ui_text_per_platform(mac, windows) when Windows words it differently) and \
             read it with .get(). Only the glyph tables themselves are allowed \
             (platform_text.rs ALLOWED).",
            bad.join("\n")
        );
    }

    #[test]
    fn the_scanner_sees_literals_and_their_calls() {
        let src = r##"
            fn f() { a("⌘A"); hint("⌘B"); hint_owned(format!("⌘{}", x)); let c = '⌘'; }
            const T: Text = ui_text("⇧⌘C");
            // "⌘ in a comment"
            /* "⌘ in a block" */
            #[cfg(test)]
            mod tests { fn g() { a("⌘ in a test"); } }
            #[cfg(test)]
            const X: &str = "⌘ test const";
            fn h<'a>(s: &'a str) -> &'a str { b(r#"⌥ raw "quoted""#) }
        "##;
        let got: Vec<(String, Vec<String>)> = scan::literals(src)
            .into_iter()
            .filter(|l| l.text.contains(GLYPHS))
            .map(|l| (l.text, l.callers))
            .collect();
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            got,
            vec![
                ("⌘A".into(), s(&["a"])),
                ("⌘B".into(), s(&["hint"])),
                ("⌘{}".into(), s(&["hint_owned", "format!"])),
                ("⌘".into(), s(&[])),
                ("⇧⌘C".into(), s(&["ui_text"])),
                ("⌥ raw \"quoted\"".into(), s(&["b"])),
            ]
        );
    }

    #[test]
    fn text_respells_on_windows() {
        let t = ui_text("⌥⌘B shelves it on this Mac");
        assert_eq!(t.on(Platform::Mac), "⌥⌘B shelves it on this Mac");
        assert_eq!(
            t.on(Platform::Windows),
            "Ctrl+Shift+A shelves it on this PC"
        );
        let own = ui_text_per_platform("⌥⌘Z discards", "Post › Discard Changes… discards");
        assert_eq!(
            own.on(Platform::Windows),
            "Post › Discard Changes… discards"
        );
        spelled_as(Platform::Mac, || {
            assert_eq!(t.get(), "⌥⌘B shelves it on this Mac");
        });
        spelled_as(Platform::Windows, || {
            assert_eq!(t.get(), "Ctrl+Shift+A shelves it on this PC");
            assert_eq!(own.get(), "Post › Discard Changes… discards");
        });
    }

    #[test]
    fn spelled_as_restores_the_spelling_after_a_panic() {
        let before = Platform::spelling();
        let r = std::panic::catch_unwind(|| {
            spelled_as(Platform::Windows, || {
                spelled_as(Platform::Mac, || panic!("inside"));
            })
        });
        assert!(r.is_err());
        assert_eq!(Platform::spelling(), before);
        spelled_as(Platform::Windows, || {
            let r = std::panic::catch_unwind(|| spelled_as(Platform::Mac, || panic!("inside")));
            assert!(r.is_err());
            assert_eq!(Platform::spelling(), Platform::Windows, "the outer one");
        });
    }

    #[test]
    fn a_hint_is_interned_per_spelling() {
        let text = "⌥⌘B is interned per spelling";
        let win = spelled_as(Platform::Windows, || crate::keymap::hint(text));
        let mac = spelled_as(Platform::Mac, || crate::keymap::hint(text));
        assert_eq!(win, "Ctrl+Shift+A is interned per spelling");
        assert_eq!(mac, text);
        assert_eq!(
            spelled_as(Platform::Windows, || crate::keymap::hint(text)),
            win
        );
    }

    /// Whether `k` (one keystroke, `cmd-shift-left` style) is a macOS caret
    /// move: ⌘ with an arrow, or ⌥ with ← or →. Windows' text field binds
    /// Ctrl+End, Ctrl+Shift+← and so on for these, so the same keystroke
    /// does something else there.
    fn macos_caret_key(k: &str) -> bool {
        let parts: Vec<&str> = k.split('-').collect();
        let (Some(last), mods) = (parts.last(), &parts[..parts.len().saturating_sub(1)]) else {
            return false;
        };
        let arrow = matches!(*last, "left" | "right" | "up" | "down");
        let sideways = matches!(*last, "left" | "right");
        // ⌥⌘ chords (the tour's ⌥⌘← ⌥⌘→) are app bindings, not caret moves.
        let chord = mods.contains(&"cmd") && mods.contains(&"alt");
        arrow && !chord && (mods.contains(&"cmd") || (mods.contains(&"alt") && sideways))
    }

    /// A test that drives the editor's caret with macOS keys (⌘↓ for end of
    /// text, ⌥⇧← to select a word) does something else on Windows: 0.26.0's
    /// Windows CI caught tests with Mac glyphs, then 0.27.0's
    /// `the_format_keys_bold_italic_and_formula` bolded one letter there. App
    /// keys go through `keymap::keys` and are fine; caret keys aren't app
    /// bindings, so a test moves the caret another way.
    #[test]
    fn tests_dont_send_macos_only_caret_keys() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let this_file = root.join("platform_text.rs");
        let mut bad = Vec::new();
        let mut edit = Vec::new();
        let mut dirs = vec![root.clone()];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).expect("read src") {
                let p = e.expect("entry").path();
                if p.is_dir() {
                    dirs.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&p).expect("read");
                for (n, line) in text.lines().enumerate() {
                    // The helper's own file holds the literals it hides.
                    if p != this_file {
                        if let Some(k) = windows_edit_key(line) {
                            let helper = windows_edit_row(k).map_or("", |r| r.2);
                            let rel = p.strip_prefix(&root).expect("under src");
                            edit.push(format!(
                                "  {}:{}: {} (call {helper})",
                                rel.display(),
                                n + 1,
                                line.trim()
                            ));
                        }
                    }
                    for call in ["keys(cx, \"", "simulate_keystrokes(\""] {
                        let Some(at) = line.find(call) else { continue };
                        let lit = line[at + call.len()..].split('"').next().unwrap_or("");
                        if lit.split(' ').any(macos_caret_key) {
                            let rel = p.strip_prefix(&root).expect("under src");
                            bad.push(format!("  {}:{}: {}", rel.display(), n + 1, line.trim()));
                        }
                    }
                }
            }
        }
        edit.sort();
        assert!(
            edit.is_empty(),
            "tests sending an editing key the Windows text field binds differently, so \
             the test passes on the Mac and fails on Windows CI:\n{}\nSend it through the \
             helper named on each line (redo is Ctrl+Y on Windows, not Ctrl+Shift+Z).",
            edit.join("\n")
        );
        bad.sort();
        assert!(
            bad.is_empty(),
            "tests sending macOS-only caret keys (⌘+arrow, ⌥+←/→), which Windows' text \
             field reads differently:\n{}\nMove the caret another way: shift-left \
             repeated, type_over the text, or set the selection directly.",
            bad.join("\n")
        );
    }

    #[test]
    fn the_caret_key_rule_spares_app_keys() {
        assert!(macos_caret_key("alt-shift-left"));
        assert!(macos_caret_key("cmd-down"));
        assert!(macos_caret_key("cmd-shift-up"));
        // App bindings and plain moves pass.
        assert!(!macos_caret_key("alt-up"));
        assert!(!macos_caret_key("shift-left"));
        assert!(!macos_caret_key("cmd-enter"));
        assert!(!macos_caret_key("alt-cmd-left"));
    }

    /// Editing keys the text field binds per platform, where the Windows key
    /// is not the plain Ctrl respelling: (Mac key, naive Windows key, what a
    /// test calls instead). gpui-base 0.6.6, `input/base/state.rs`: redo is
    /// Cmd+Shift+Z on the Mac but only Ctrl+Y on Windows. Undo, cut, copy,
    /// paste and select-all respell plainly, so they are not here. Cmd+Backspace
    /// and Cmd+Delete also differ (Windows' Ctrl+Backspace deletes a word), but
    /// the app binds them on sheets, so a string alone cannot tell them apart.
    const WINDOWS_EDIT_KEYS: &[(&str, &str, &str)] = &[(
        "cmd-shift-z",
        "ctrl-shift-z",
        "crate::platform_text::redo_keys()",
    )];

    fn windows_edit_row(k: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
        WINDOWS_EDIT_KEYS
            .iter()
            .find(|&&(mac, win, _)| k == mac || k == win)
    }

    /// The first keystroke in a string literal after a `simulate_keystrokes(`
    /// or `keys(cx, ` call on `line` that is in `WINDOWS_EDIT_KEYS`. Sees a
    /// literal on the same line as its call, also through
    /// `keymap::keys("...")`; not one split across lines or held in a variable.
    fn windows_edit_key(line: &str) -> Option<&str> {
        let at = ["simulate_keystrokes(", "keys(cx, "]
            .iter()
            .filter_map(|call| line.find(call))
            .min()?;
        line[at..]
            .split('"')
            .skip(1)
            .step_by(2)
            .flat_map(|lit| lit.split(' '))
            .find(|k| windows_edit_row(k).is_some())
    }

    #[test]
    fn the_edit_key_rule_flags_keys_windows_reads_differently() {
        // The line that failed Windows CI at cb1def7, copied exactly.
        let real = "    cx.simulate_keystrokes(&crate::keymap::keys(\"cmd-shift-z\"));";
        assert_eq!(windows_edit_key(real), Some("cmd-shift-z"));
        assert_eq!(
            windows_edit_key("cx.simulate_keystrokes(\"cmd-shift-z\");"),
            Some("cmd-shift-z")
        );
        assert_eq!(
            windows_edit_key("keys(cx, \"ctrl-shift-z\");"),
            Some("ctrl-shift-z")
        );
        // The helper, an app key, and undo (Windows binds plain Ctrl+Z) pass.
        assert_eq!(windows_edit_key("cx.simulate_keystrokes(redo);"), None);
        assert_eq!(
            windows_edit_key("cx.simulate_keystrokes(crate::platform_text::redo_keys());"),
            None
        );
        assert_eq!(
            windows_edit_key("cx.simulate_keystrokes(&crate::keymap::keys(\"cmd-b\"));"),
            None
        );
        assert_eq!(
            windows_edit_key("cx.simulate_keystrokes(&crate::keymap::keys(\"cmd-z\"));"),
            None
        );
    }
}
