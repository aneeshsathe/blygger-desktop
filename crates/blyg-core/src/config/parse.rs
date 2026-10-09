//! The config file syntax (Ghostty style):
//!
//! ```text
//! # a comment
//! key = value
//! key = "a quoted value, with  spaces kept"
//! repeatable-key = one
//! repeatable-key = two
//! config-file = ?optional/extra.config
//! ```
//!
//! One `key = value` per line. `#` starts a comment only at the beginning of
//! a line (so `#fff` in a value is fine). Values may be double-quoted; inside
//! quotes `\"`, `\\`, `\n` and `\t` are escapes. An empty value resets a key
//! to its default (a repeatable key to an empty list). Unknown keys are
//! warnings; malformed lines and bad values are errors. Neither stops the
//! rest of the file from loading.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use super::keys::{KEYS, KeySpec, ValueKind, spec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// A problem found while loading, tied to a file and line (line 0 = the file
/// as a whole).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: PathBuf,
    pub line: usize,
    pub severity: Severity,
    pub message: String,
}

impl Diagnostic {
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    /// `~/.config/blygger/config:12: unknown key …` (home shortened to `~`).
    pub fn short(&self) -> String {
        let f = super::paths::tilde(&self.file);
        if self.line == 0 {
            format!("{f}: {}", self.message)
        } else {
            format!("{f}:{}: {}", self.line, self.message)
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = match self.severity {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        if self.line == 0 {
            write!(f, "{}: {sev}: {}", self.file.display(), self.message)
        } else {
            write!(
                f,
                "{}:{}: {sev}: {}",
                self.file.display(),
                self.line,
                self.message
            )
        }
    }
}

/// One `key = value` line, as written (value unquoted, not yet validated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub value: String,
    pub file: PathBuf,
    pub line: usize,
}

/// Split one line into `(key, value)`. `Ok(None)` for blank lines and comments.
pub fn parse_line(line: &str) -> Result<Option<(String, String)>, String> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') {
        return Ok(None);
    }
    let Some((k, v)) = t.split_once('=') else {
        return Err(format!("expected `key = value`, found `{t}`"));
    };
    let k = k.trim();
    if k.is_empty() {
        return Err("missing key before `=`".into());
    }
    if !k
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(format!("`{k}` isn't a valid key name"));
    }
    Ok(Some((k.to_string(), unquote(v.trim())?)))
}

fn unquote(v: &str) -> Result<String, String> {
    let Some(inner) = v.strip_prefix('"') else {
        return Ok(v.to_string());
    };
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => return Err("unterminated quoted value".into()),
            },
            '"' => {
                let rest: String = chars.collect();
                if !rest.trim().is_empty() {
                    return Err(format!(
                        "unexpected `{}` after the closing quote",
                        rest.trim()
                    ));
                }
                return Ok(out);
            }
            c => out.push(c),
        }
    }
    Err("unterminated quoted value".into())
}

/// Write a value so `parse_line` reads it back unchanged: bare when that's
/// unambiguous, otherwise double-quoted with escapes.
pub fn quote(v: &str) -> String {
    let needs =
        v.is_empty() || v.trim() != v || v.starts_with('"') || v.contains('\n') || v.contains('\t');
    if !needs {
        return v.to_string();
    }
    let mut s = String::from("\"");
    for c in v.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\t' => s.push_str("\\t"),
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

/// Lex a whole file's text.
pub fn parse_text(text: &str, file: &Path) -> (Vec<Entry>, Vec<Diagnostic>) {
    let mut entries = Vec::new();
    let mut diags = Vec::new();
    for (i, line) in text.lines().enumerate() {
        match parse_line(line) {
            Ok(Some((key, value))) => entries.push(Entry {
                key,
                value,
                file: file.to_path_buf(),
                line: i + 1,
            }),
            Ok(None) => {}
            Err(message) => diags.push(Diagnostic {
                file: file.to_path_buf(),
                line: i + 1,
                severity: Severity::Error,
                message,
            }),
        }
    }
    (entries, diags)
}

/// Where the text of a config file comes from: the disk, or (tests,
/// in-memory stores) a lookup function.
pub trait Source {
    /// `Ok(None)` = the file doesn't exist.
    fn read(&self, path: &Path) -> std::io::Result<Option<String>>;
}

pub struct Disk;

impl Source for Disk {
    fn read(&self, path: &Path) -> std::io::Result<Option<String>> {
        match std::fs::read_to_string(path) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// The result of loading: the effective config, every line that went into
/// it, the files actually read, and what went wrong.
#[derive(Debug, Clone, Default)]
pub struct Loaded {
    pub config: Config,
    pub entries: Vec<Entry>,
    pub files: Vec<PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Loaded {
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.is_error())
    }

    /// The file that last set `key`, if any (write-back edits it there).
    pub fn last_file_for(&self, key: &str) -> Option<&Path> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.key == key)
            .map(|e| e.file.as_path())
    }
}

/// Load `files` in order (later ones override earlier ones). Missing files
/// are skipped silently: every default location is optional. `config-file`
/// includes are loaded right after the file that names them.
pub fn load(files: &[PathBuf], src: &dyn Source) -> Loaded {
    let mut out = Loaded::default();
    let mut seen = HashSet::new();
    for f in files {
        load_one(f, false, None, src, &mut seen, &mut out);
    }
    out.config = Config::from_entries(&out.entries, &mut out.diagnostics);
    out
}

fn load_one(
    path: &Path,
    required: bool,
    named_at: Option<(&Path, usize)>,
    src: &dyn Source,
    seen: &mut HashSet<PathBuf>,
    out: &mut Loaded,
) {
    let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let (diag_file, diag_line) = match named_at {
        Some((f, l)) => (f.to_path_buf(), l),
        None => (path.to_path_buf(), 0),
    };
    if !seen.insert(key) {
        out.diagnostics.push(Diagnostic {
            file: diag_file,
            line: diag_line,
            severity: Severity::Error,
            message: format!("{} is already loaded (include cycle?)", path.display()),
        });
        return;
    }
    let text = match src.read(path) {
        Ok(Some(t)) => t,
        Ok(None) => {
            if required {
                out.diagnostics.push(Diagnostic {
                    file: diag_file,
                    line: diag_line,
                    severity: Severity::Error,
                    message: format!(
                        "config-file {} doesn't exist (write ?{} to make it optional)",
                        path.display(),
                        path.display()
                    ),
                });
            }
            return;
        }
        Err(e) => {
            out.diagnostics.push(Diagnostic {
                file: diag_file,
                line: diag_line,
                severity: Severity::Error,
                message: format!("couldn't read {}: {e}", path.display()),
            });
            return;
        }
    };
    out.files.push(path.to_path_buf());
    let (entries, diags) = parse_text(&text, path);
    out.diagnostics.extend(diags);
    let includes: Vec<(String, usize)> = entries
        .iter()
        .filter(|e| e.key == "config-file" && !e.value.trim().is_empty())
        .map(|e| (e.value.trim().to_string(), e.line))
        .collect();
    out.entries.extend(entries);
    let dir = path.parent().unwrap_or(Path::new("."));
    for (value, line) in includes {
        let (optional, p) = match value.strip_prefix('?') {
            Some(rest) => (true, rest.trim()),
            None => (false, value.as_str()),
        };
        let target = resolve_path(p, dir);
        load_one(&target, !optional, Some((path, line)), src, seen, out);
    }
}

/// `~/x` → `$HOME/x`; relative → relative to `dir`.
pub fn resolve_path(p: &str, dir: &Path) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/")
        && let Some(home) = super::paths::home_var()
    {
        return PathBuf::from(home).join(rest);
    }
    let pb = PathBuf::from(p);
    if pb.is_absolute() { pb } else { dir.join(pb) }
}

// ------------------------------------------------------------------ Config

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    #[default]
    Side,
    Stacked,
}

// --- scratch notes ---

/// `capture-default`: what quick capture keeps on esc / ⌘S / click-away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptureDefault {
    #[default]
    Scratch,
    Draft,
}

/// `new-note`: what ⌘N (`Config::new_post`) and the omnibar's create
/// (`Config::new_note`) make. Unset, they differ: ⌘N starts a scratch note,
/// the omnibar a draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewNote {
    #[default]
    Draft,
    Scratch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditedPosts {
    #[default]
    Top,
    Stay,
}

/// The effective configuration: validated values for the keys the user set;
/// everything else comes from the key table's defaults.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Config {
    values: BTreeMap<&'static str, Vec<String>>,
}

impl Config {
    /// Apply entries in order. Unknown keys → warning; bad values → error
    /// (the key keeps its previous value).
    pub fn from_entries(entries: &[Entry], diags: &mut Vec<Diagnostic>) -> Config {
        let mut c = Config::default();
        for e in entries {
            let Some(spec) = spec(&e.key) else {
                let hint = suggest(&e.key)
                    .map(|s| format!(" (did you mean `{s}`?)"))
                    .unwrap_or_default();
                diags.push(Diagnostic {
                    file: e.file.clone(),
                    line: e.line,
                    severity: Severity::Warning,
                    message: format!("unknown key `{}`{hint}; ignored", e.key),
                });
                continue;
            };
            match validate(spec, &e.value) {
                Ok((v, warning)) => {
                    if let Some(message) = warning {
                        diags.push(Diagnostic {
                            file: e.file.clone(),
                            line: e.line,
                            severity: Severity::Warning,
                            message,
                        });
                    }
                    c.apply(spec, v);
                }
                Err(message) => diags.push(Diagnostic {
                    file: e.file.clone(),
                    line: e.line,
                    severity: Severity::Error,
                    message: format!("{}: {message}", spec.name),
                }),
            }
        }
        c
    }

    fn apply(&mut self, spec: &'static KeySpec, v: Option<String>) {
        match (spec.repeatable, v) {
            // Empty value: scalar → back to default; list → empty list.
            (false, None) => {
                self.values.remove(spec.name);
            }
            (true, None) => {
                self.values.insert(spec.name, Vec::new());
            }
            (false, Some(v)) => {
                self.values.insert(spec.name, vec![v]);
            }
            // The first user value replaces the default list.
            (true, Some(v)) => self.values.entry(spec.name).or_default().push(v),
        }
    }

    /// Set a key in memory (already-validated value). `None` = back to default.
    pub fn set(&mut self, name: &str, value: Option<&str>) {
        if let Some(spec) = spec(name) {
            match value {
                Some(v) => {
                    self.values.insert(spec.name, vec![v.to_string()]);
                }
                None => {
                    self.values.remove(spec.name);
                }
            }
        }
    }

    pub fn set_list(&mut self, name: &str, values: Vec<String>) {
        if let Some(spec) = spec(name) {
            self.values.insert(spec.name, values);
        }
    }

    /// Whether the user set this key (in any loaded file).
    pub fn is_set(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    /// The effective single value (user's or default).
    pub fn get(&self, name: &str) -> Option<&str> {
        match self.values.get(name) {
            Some(v) => v.last().map(String::as_str),
            None => spec(name).and_then(|s| s.default),
        }
    }

    /// The effective list for a repeatable key.
    pub fn list(&self, name: &str) -> Vec<String> {
        match self.values.get(name) {
            Some(v) => v.clone(),
            None => spec(name)
                .map(|s| s.default_list.iter().map(|d| d.to_string()).collect())
                .unwrap_or_default(),
        }
    }

    /// The user-set values, in key-table order.
    pub fn user_values(&self) -> impl Iterator<Item = (&'static str, &[String])> {
        KEYS.iter()
            .filter_map(|k| self.values.get(k.name).map(|v| (k.name, v.as_slice())))
    }

    // ---- typed accessors

    pub fn blyg_url(&self) -> Option<&str> {
        self.get("blyg-url")
    }

    /// `theme`: `system`, `light`, `dark`, a built-in or a user theme's name.
    pub fn theme(&self) -> &str {
        self.get("theme").unwrap_or(super::theme::SYSTEM)
    }

    /// `theme-dark`: the theme to use while macOS is dark, if set.
    pub fn theme_dark(&self) -> Option<&str> {
        self.get("theme-dark")
    }

    pub fn layout(&self) -> Layout {
        match self.get("layout") {
            Some("stacked") => Layout::Stacked,
            _ => Layout::Side,
        }
    }

    // --- buttons ---
    pub fn show_buttons(&self) -> bool {
        self.get("show-buttons") != Some("false")
    }

    // --- spellcheck ---
    pub fn spellcheck(&self) -> bool {
        self.get("spellcheck") != Some("false")
    }

    pub fn font_family_writing(&self) -> &str {
        self.get("font-family-writing").unwrap_or("Literata")
    }

    pub fn font_family_ui(&self) -> &str {
        self.get("font-family-ui").unwrap_or("Inter")
    }

    pub fn font_size(&self) -> f32 {
        self.get("font-size")
            .and_then(|s| s.parse().ok())
            .unwrap_or(19.0)
    }

    pub fn capture_hotkey(&self) -> &str {
        self.get("capture-hotkey").unwrap_or("ctrl+alt+b")
    }

    // --- scratch notes ---
    pub fn capture_default(&self) -> CaptureDefault {
        match self.get("capture-default") {
            Some("draft") => CaptureDefault::Draft,
            _ => CaptureDefault::Scratch,
        }
    }

    /// What the omnibar's create makes: a draft unless `new-note = scratch`.
    pub fn new_note(&self) -> NewNote {
        match self.get("new-note") {
            Some("scratch") => NewNote::Scratch,
            _ => NewNote::Draft,
        }
    }

    /// What ⌘N (New Post) starts: a scratch note unless `new-note = draft`.
    pub fn new_post(&self) -> NewNote {
        match self.get("new-note") {
            Some("draft") => NewNote::Draft,
            _ => NewNote::Scratch,
        }
    }

    pub fn edited_posts(&self) -> EditedPosts {
        match self.get("edited-posts") {
            Some("stay") => EditedPosts::Stay,
            _ => EditedPosts::Top,
        }
    }

    /// `None` for `none`.
    pub fn ai_provider(&self) -> Option<&str> {
        self.get("ai-provider").filter(|p| *p != "none")
    }

    pub fn ai_model(&self) -> Option<&str> {
        self.get("ai-model")
    }

    /// `ai-provider-model` entries as `(provider, model)`; later ones win.
    pub fn ai_provider_models(&self) -> Vec<(String, String)> {
        self.list("ai-provider-model")
            .iter()
            .filter_map(|pm| {
                pm.split_once('=')
                    .map(|(p, m)| (p.trim().to_string(), m.trim().to_string()))
            })
            .collect()
    }

    pub fn ai_enabled(&self) -> Vec<String> {
        self.list("ai-enable")
    }

    pub fn cloudflare_account_id(&self) -> Option<&str> {
        self.get("cloudflare-account-id")
    }

    pub fn ai_style_prompt(&self) -> Option<&str> {
        self.get("ai-style-prompt")
    }

    pub fn tutorial_on_launch(&self) -> bool {
        self.get("tutorial-on-launch") == Some("true")
    }

    // --- extensions ---

    /// `extension`: the extensions to run, in the order first named,
    /// without duplicates. Empty by default: nothing runs unless named.
    pub fn extensions_enabled(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.list("extension")
            .into_iter()
            .filter(|n| seen.insert(n.clone()))
            .collect()
    }

    /// `extension-allow`: every extension's granted capabilities, by name.
    /// `fs:<path>` grants are returned as written (`~` not expanded).
    /// Extensions with no grants are absent.
    pub fn extension_allows(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for line in self.list("extension-allow") {
            if let Some((name, cap)) = split_extension_line(&line) {
                out.entry(name.to_string())
                    .or_default()
                    .insert(cap.to_string());
            }
        }
        out
    }

    /// `extension-setting`: one extension's settings as `key -> value`;
    /// a later line for the same key wins.
    pub fn extension_settings(&self, name: &str) -> BTreeMap<String, String> {
        self.list("extension-setting")
            .iter()
            .filter_map(|line| split_extension_line(line))
            .filter(|(n, _)| *n == name)
            .filter_map(|(_, kv)| kv.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
}

// ------------------------------------------------------------ extensions

/// An extension name (or setting key): lowercase kebab-case
/// (`markdown-notes`), ASCII letters and digits in `-`-separated words.
pub fn valid_extension_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|w| {
            !w.is_empty()
                && w.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// A capability `extension-allow` accepts: one of
/// [`EXTENSION_CAPABILITIES`](super::keys::EXTENSION_CAPABILITIES) (exact
/// case), `fs:` followed by a path, or `browser.automate:` followed by an
/// origin [`normalize_origin`] accepts.
pub fn valid_capability(cap: &str) -> bool {
    if let Some(origin) = cap.strip_prefix(super::keys::EXTENSION_AUTOMATE_PREFIX) {
        return origin.trim() == origin && normalize_origin(origin).is_ok();
    }
    match cap.strip_prefix(super::keys::EXTENSION_FS_PREFIX) {
        Some(path) => !path.trim().is_empty() && path.trim() == path,
        None => super::keys::EXTENSION_CAPABILITIES.contains(&cap),
    }
}

/// A website origin, normalised: `http` or `https`, a host, and nothing
/// after it but an optional `/` (no path, query, fragment or sign-in).
/// The host is lower-cased (an international name becomes its `xn--`
/// form) and a default port is dropped, so `HTTPS://Social.Example.com:443/`
/// is `https://social.example.com`. `Err` says what's wrong.
pub fn normalize_origin(s: &str) -> Result<String, String> {
    let s = s.trim();
    let u = url::Url::parse(s).map_err(|e| format!("`{s}` isn't a URL ({e})"))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err(format!("`{s}` must start with https:// or http://"));
    }
    match u.host() {
        None => return Err(format!("`{s}` has no host")),
        Some(url::Host::Domain("")) => {
            return Err(format!("`{s}` has no host"));
        }
        Some(url::Host::Domain(d))
            if !d
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.')) =>
        {
            return Err(format!(
                "`{s}` must name one exact host (letters, digits, '-' and '.'; no wildcards)"
            ));
        }
        Some(_) => {}
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err(format!("`{s}` must not carry a user name or password"));
    }
    if u.path() != "/" || u.query().is_some() || u.fragment().is_some() {
        return Err(format!(
            "`{s}` must be an origin only (scheme and host, e.g. \
             https://social.example.com), with no path, query or #fragment"
        ));
    }
    Ok(u.origin().ascii_serialization())
}

/// The normalised origin of any http(s) URL with a host (`None` for
/// anything else): what a URL is checked against a
/// `browser.automate:<origin>` grant with.
pub fn url_origin(s: &str) -> Option<String> {
    let u = url::Url::parse(s.trim()).ok()?;
    if !matches!(u.scheme(), "http" | "https") || u.host_str().is_none_or(str::is_empty) {
        return None;
    }
    Some(u.origin().ascii_serialization())
}

/// `<name> <rest>` → `(name, rest)`, split at the first run of whitespace.
fn split_extension_line(line: &str) -> Option<(&str, &str)> {
    let (name, rest) = line.trim().split_once(char::is_whitespace)?;
    Some((name, rest.trim()))
}

fn check_extension_name(name: &str) -> Result<(), String> {
    if valid_extension_name(name) {
        Ok(())
    } else {
        Err(format!(
            "`{name}` isn't an extension name (lowercase kebab-case, e.g. markdown-notes)"
        ))
    }
}

fn validate_allow(v: &str) -> Result<String, String> {
    let Some((name, cap)) = split_extension_line(v) else {
        return Err(format!(
            "`{v}` must be <name> <capability>, e.g. markdown-notes items.read"
        ));
    };
    check_extension_name(name)?;
    if let Some(origin) = cap.strip_prefix(super::keys::EXTENSION_AUTOMATE_PREFIX) {
        let origin = normalize_origin(origin).map_err(|e| format!("browser.automate: {e}"))?;
        return Ok(format!(
            "{name} {}{origin}",
            super::keys::EXTENSION_AUTOMATE_PREFIX
        ));
    }
    if !valid_capability(cap) {
        let caps = super::keys::EXTENSION_CAPABILITIES;
        let hint = caps
            .iter()
            .find(|c| c.eq_ignore_ascii_case(cap))
            .map(|c| format!(" (did you mean `{c}`?)"))
            .unwrap_or_default();
        return Err(format!(
            "unknown capability `{cap}`{hint}; one of {}, fs:<path> or \
             browser.automate:<origin>",
            caps.join(", ")
        ));
    }
    Ok(format!("{name} {cap}"))
}

fn validate_setting(v: &str) -> Result<String, String> {
    let Some((name, kv)) = split_extension_line(v) else {
        return Err(format!(
            "`{v}` must be <name> key=value, e.g. markdown-notes vault=~/Notes"
        ));
    };
    check_extension_name(name)?;
    let Some((k, val)) = kv.split_once('=') else {
        return Err(format!("`{kv}` must be key=value"));
    };
    let k = k.trim();
    if !valid_extension_name(k) {
        return Err(format!(
            "`{k}` isn't a setting key (lowercase kebab-case, e.g. export-dir)"
        ));
    }
    Ok(format!("{name} {k}={}", val.trim()))
}

/// The line grammar of the extension keys. `None` for any other key;
/// otherwise the normalised value (single spaces) or what's wrong.
fn validate_extension_line(key: &str, v: &str) -> Option<Result<String, String>> {
    Some(match key {
        "extension" => check_extension_name(v).map(|()| v.to_string()),
        "extension-allow" => validate_allow(v),
        "extension-setting" => validate_setting(v),
        _ => return None,
    })
}

/// Validate and normalise one value. `Ok(None)` = empty (reset);
/// the second element is a warning to report alongside an accepted value.
pub fn validate(spec: &KeySpec, raw: &str) -> Result<(Option<String>, Option<String>), String> {
    let v = raw.trim();
    if raw.is_empty() {
        return Ok((None, None));
    }
    let ok = |s: String| Ok((Some(s), None));
    match spec.kind {
        ValueKind::Text => match validate_extension_line(spec.name, v) {
            Some(r) => ok(r?),
            None => ok(raw.to_string()),
        },
        ValueKind::Url => {
            let u = url::Url::parse(v).map_err(|e| format!("`{v}` isn't a URL ({e})"))?;
            if !matches!(u.scheme(), "http" | "https") || u.host_str().is_none() {
                return Err(format!("`{v}` must be an http(s) URL with a host"));
            }
            ok(v.trim_end_matches('/').to_string())
        }
        ValueKind::Bool => match v.to_ascii_lowercase().as_str() {
            "true" => ok("true".into()),
            "false" => ok("false".into()),
            _ => Err(format!("`{v}` must be true or false")),
        },
        ValueKind::Number { min, max } => {
            let n: f32 = v
                .parse()
                .ok()
                .filter(|n: &f32| n.is_finite())
                .ok_or_else(|| format!("`{v}` isn't a number"))?;
            if n < min || n > max {
                let c = n.clamp(min, max);
                Ok((
                    Some(fmt_num(c)),
                    Some(format!(
                        "{}: {v} is outside {}–{}; using {}",
                        spec.name,
                        fmt_num(min),
                        fmt_num(max),
                        fmt_num(c)
                    )),
                ))
            } else {
                ok(fmt_num(n))
            }
        }
        ValueKind::Choice(opts) => {
            let l = v.to_ascii_lowercase();
            if opts.contains(&l.as_str()) {
                ok(l)
            } else {
                Err(format!("`{v}` must be one of {}", opts.join(", ")))
            }
        }
        ValueKind::Hotkey => {
            let parts: Vec<&str> = v.split('+').map(str::trim).collect();
            if parts.iter().any(|p| p.is_empty())
                || !v
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == ' ')
            {
                return Err(format!("`{v}` isn't a key combination like ctrl+alt+b"));
            }
            ok(parts.join("+").to_ascii_lowercase())
        }
        ValueKind::Path => ok(v.to_string()),
        ValueKind::ThemeName => {
            let l = v.to_ascii_lowercase();
            if super::theme::valid_name(&l) {
                ok(l)
            } else {
                Err(format!(
                    "`{v}` isn't a theme name (system, light, dark, a built-in such as \
                     cutaway, or a file in the themes folder)"
                ))
            }
        }
        ValueKind::ProviderModel => {
            let Some((p, m)) = v.split_once('=') else {
                return Err(format!("`{v}` must be provider=model"));
            };
            let p = p.trim().to_ascii_lowercase();
            if !super::keys::AI_PROVIDERS.contains(&p.as_str()) {
                return Err(format!(
                    "unknown provider `{p}` (one of {})",
                    super::keys::AI_PROVIDERS.join(", ")
                ));
            }
            if m.trim().is_empty() {
                return Err(format!("`{v}` has no model after ="));
            }
            ok(format!("{p}={}", m.trim()))
        }
    }
}

pub fn fmt_num(n: f32) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// The closest known key by edit distance, for typos.
fn suggest(unknown: &str) -> Option<&'static str> {
    let u = unknown.to_ascii_lowercase().replace('_', "-");
    KEYS.iter()
        .map(|k| (levenshtein(&u, k.name), k.name))
        .filter(|(d, _)| *d <= 3)
        .min_by_key(|(d, _)| *d)
        .map(|(_, n)| n)
}

pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != *cb);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}
