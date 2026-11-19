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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        crate::keymap::hint(self.source(Platform::HERE))
    }

    /// As `p` shows it, on any platform: the tests read Windows' wording on
    /// macOS.
    #[cfg_attr(not(test), allow(dead_code))]
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
    #[cfg_attr(not(test), allow(dead_code))]
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
        if Platform::HERE == Platform::Mac {
            assert_eq!(t.get(), "⌥⌘B shelves it on this Mac");
        }
    }
}
