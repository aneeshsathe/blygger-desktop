//! Ghostty-style `+actions`. `blygger` with no `+action` starts the app.
//!
//! - `blygger +show-config [--default] [--docs] [--changes-only=false]`
//! - `blygger +validate-config`
//! - `blygger +list-fonts`
//! - `blygger +list-themes`
//! - `blygger +copy-theme <built-in> [name]`
//! - `blygger +list-keybinds`
//! - `blygger +list-extensions`
//! - `blygger +ext <bundled extension>` (run by Burrow itself, over stdio)
//! - `blygger +import-opml <file> [--dry-run]`, `blygger +export-opml <file>`
//! - `blygger +version`
//! - `blygger +help`

use std::process::ExitCode;

use blyg_core::ConfigStore;
use blyg_core::config::show::{ShowOptions, show_config};
use blyg_core::config::{Diagnostic, Severity};

use crate::prefs::{UI_FONTS, WRITING_FONTS};

/// What an action printed and how it exits.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub stdout: String,
    pub stderr: String,
    pub code: u8,
}

/// `None` when the arguments don't name a `+action` (launch the app).
pub fn run(args: &[String]) -> Option<ExitCode> {
    let first = args.iter().find(|a| a.starts_with('+'))?;
    let rest: Vec<&str> = args
        .iter()
        .skip_while(|a| *a != first)
        .skip(1)
        .map(String::as_str)
        .collect();
    // A bundled extension speaks BXP over the stdin/stdout Burrow gave it:
    // nothing else may touch them (no console attach, no help text), and it
    // starts before any config, GUI or Keychain work.
    if first == "+ext"
        && let Some(serve) = rest.first().and_then(|n| bundled_extension(n))
    {
        return Some(serve());
    }
    attach_parent_console();
    let out = exec(first, &rest, &mut ConfigStore::discover);
    // The docs name keys and places as macOS does; Windows respells them.
    print!("{}", crate::keymap::hint_owned(out.stdout));
    eprint!("{}", crate::keymap::hint_owned(out.stderr));
    Some(ExitCode::from(out.code))
}

/// A release build on Windows is a GUI program with no console of its own;
/// print to the terminal that ran `blygger +action`, if there is one.
fn attach_parent_console() {
    #[cfg(target_os = "windows")]
    {
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        unsafe extern "system" {
            fn AttachConsole(process_id: u32) -> i32;
        }
        // SAFETY: a plain Win32 call; failure (no parent console, or one
        // already attached in a debug build) just leaves output unseen.
        unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    }
}

const HELP: &str = "\
Usage: blygger [+action [options]]

With no action, Burrow starts. Actions:
  +show-config            the effective config (only what you changed)
      --default           show the defaults instead
      --docs              include each option's documentation
      --changes-only=false  show every option, not just the ones you set
  +validate-config        check the config file(s) for problems
  +list-fonts             fonts for font-family-writing / font-family-ui
  +list-themes            every theme, built in and yours
  +copy-theme <built-in> [name]
                          copy a built-in theme into the themes folder to edit
  +list-keybinds          every keyboard shortcut, and the ones reserved
  +list-extensions        every extension, bundled and installed: whether it's
                          on, what it's granted, what it still asks for, its
                          sites and macros, and problems with installed ones
  +ext <name>             run a bundled extension (markdown-notes, cross-post,
                          reading-time, inspect) over stdin/stdout; Burrow
                          starts it itself
  +import-opml <file> [--dry-run]
                          subscribe your blyg to every feed in another
                          reader's OPML export, slowly (about 30 a minute),
                          into the Reader folder Imported feeds; feeds you
                          already follow are left alone. --dry-run lists
                          what would be added
  +export-opml <file>     save every subscription as an OPML 2.0 file
  +version                print the version
  +help                   this help

Discover every option with: blygger +show-config --default --docs
";

pub fn exec(action: &str, args: &[&str], load: &mut dyn FnMut() -> ConfigStore) -> Outcome {
    let mut o = Outcome::default();
    match action {
        "+version" => {
            o.stdout = format!("blygger {}\n", env!("CARGO_PKG_VERSION"));
        }
        "+help" => o.stdout = HELP.to_string(),
        // --- browser --- internal (not in +help): the app runs this as a
        // child process to convert the block lists (browser/driver.rs).
        "+convert-blocklists" => {
            let out_dir = args.first().map(std::path::PathBuf::from);
            let unless = args.iter().find_map(|a| a.strip_prefix("--unless-key="));
            let data_dir = blyg_core::config::data_dir();
            match out_dir
                .map(|d| crate::app::browser::blocklist::convert_to_dir(&data_dir, &d, unless))
            {
                Some(Ok(r)) => o.stdout = serde_json::to_string(&r).unwrap_or_default() + "\n",
                Some(Err(e)) => {
                    o.stderr = format!("blygger +convert-blocklists: {e}\n");
                    o.code = 1;
                }
                None => {
                    o.stderr = "usage: blygger +convert-blocklists <out dir>\n".into();
                    o.code = 2;
                }
            }
        }
        "+list-keybinds" => o.stdout = crate::keymap::list(),
        // `run` serves a bundled extension before it gets here; what's left
        // is a missing or unknown name.
        "+ext" => {
            let bundled = BUNDLED_EXTENSIONS.join(", ");
            o.stderr = match args.first() {
                Some(n) if bundled_extension(n).is_some() => {
                    format!("blygger +ext {n}: it speaks BXP over stdin/stdout; Burrow starts it\n")
                }
                Some(n) => {
                    format!("blygger +ext: `{n}` isn't a bundled extension (bundled: {bundled})\n")
                }
                None => format!("usage: blygger +ext <name>\nbundled: {bundled}\n"),
            };
            o.code = 2;
        }
        "+list-extensions" => o.stdout = list_extensions(&load()),
        // --- OPML ---
        "+import-opml" => import_opml(args, &load(), &mut o),
        "+export-opml" => export_opml(args, &load(), &mut o),
        "+show-config" => match ShowOptions::from_args(args.iter().copied()) {
            Ok(opts) => {
                let store = load();
                o.stdout = show_config(store.config(), opts);
                if !opts.default {
                    for d in diagnostics(&store) {
                        o.stderr.push_str(&format!("{d}\n"));
                    }
                }
            }
            Err(e) => {
                o.stderr = format!("blygger +show-config: {e}\n");
                o.code = 2;
            }
        },
        "+validate-config" => {
            let store = load();
            let diags = diagnostics(&store);
            for d in &diags {
                o.stdout.push_str(&format!("{d}\n"));
            }
            if diags.iter().any(|d| d.is_error()) {
                o.code = 1;
            } else if diags.is_empty() {
                let files: Vec<String> = store
                    .loaded()
                    .files
                    .iter()
                    .map(|f| blyg_core::config::paths::tilde(f))
                    .collect();
                o.stdout = if files.is_empty() {
                    format!(
                        "No config file yet (it would be {}); using the defaults.\n",
                        blyg_core::config::paths::tilde(store.primary())
                    )
                } else {
                    format!("OK: {}\n", files.join(", "))
                };
            }
        }
        "+list-themes" => {
            let store = load();
            let themes = themes_of(&store);
            let all = crate::theme::Themes::new(themes.clone());
            for (family, list) in all.grouped() {
                o.stdout.push_str(&format!("{}:\n", family.label()));
                if family == blyg_core::config::theme::Family::Plain {
                    o.stdout.push_str(&format!(
                        "  {:<14} follows macOS: light or dark\n",
                        "system"
                    ));
                }
                for t in list {
                    let whose = if t.builtin { "built-in" } else { "yours" };
                    o.stdout
                        .push_str(&format!("  {:<14} {} ({whose})\n", t.id, t.name));
                }
            }
            if let Some(d) = &themes.dir {
                o.stdout.push_str(&format!(
                    "\nYour themes live in {}\n",
                    blyg_core::config::paths::tilde(d)
                ));
            }
        }
        "+copy-theme" => match copy_theme(args, &load()) {
            Ok(msg) => o.stdout = msg,
            Err((code, e)) => {
                o.stderr = format!("blygger +copy-theme: {e}\n");
                o.code = code;
            }
        },
        "+list-fonts" => {
            let list = |title: &str, fonts: &[crate::prefs::FontChoice], o: &mut Outcome| {
                o.stdout.push_str(&format!("{title}\n"));
                for f in fonts {
                    let note = if f.bundled.is_some() {
                        "bundled"
                    } else if cfg!(target_os = "windows") {
                        "Windows"
                    } else {
                        "macOS"
                    };
                    o.stdout.push_str(&format!("  {:<20} {note}\n", f.label));
                }
            };
            list("font-family-writing:", WRITING_FONTS, &mut o);
            o.stdout.push('\n');
            list("font-family-ui:", UI_FONTS, &mut o);
        }
        other => {
            o.stderr = format!("blygger: unknown action {other}\n\n{HELP}");
            o.code = 2;
        }
    }
    o
}

// --- OPML ---

/// The configured blyg, opened without the sync worker (nothing pushed),
/// its subscriptions freshly pulled. `Err` says why not.
fn opml_backend(store: &ConfigStore) -> Result<blyg_core::LiveBackend, String> {
    use blyg_core::config::{MemoryTokenStore, TokenStore};
    let url = store
        .config()
        .blyg_url()
        .ok_or("no blyg is connected (open Burrow and connect one first)")?
        .to_string();
    // BLYGGER_TEST_TOKEN: an in-memory token (a local studio), no Keychain.
    let tokens: std::sync::Arc<dyn TokenStore> = match std::env::var("BLYGGER_TEST_TOKEN") {
        Ok(t) => {
            let m = MemoryTokenStore::default();
            let _ = m.set(&url, t.trim());
            std::sync::Arc::new(m)
        }
        Err(_) => std::sync::Arc::new(blyg_core::config::KeychainTokenStore),
    };
    let opts = blyg_core::SyncOptions {
        start_worker: false,
        ..Default::default()
    };
    let live =
        crate::connection::open_live_with(&blyg_core::config::data_dir(), &url, &tokens, opts)
            .map_err(|e| format!("couldn't open the local database: {e}"))?
            .ok_or_else(|| {
                format!("no owner credential for {url} in the Keychain; connect again in Burrow")
            })?;
    live.pull_now()
        .map_err(|e| format!("couldn't reach your blyg: {e}"))?;
    Ok(live)
}

fn import_opml(args: &[&str], store: &ConfigStore, o: &mut Outcome) {
    use blyg_core::Backend as _;
    use blyg_core::opml::{self, Outcome as Got, Pace, Progress, Summary};
    let dry = args.contains(&"--dry-run");
    let Some(file) = args.iter().find(|a| !a.starts_with("--")) else {
        o.stderr = "usage: blygger +import-opml <file> [--dry-run]\n".into();
        o.code = 2;
        return;
    };
    let parsed = match opml::read_file(std::path::Path::new(file)) {
        Ok(p) => p,
        Err(e) => {
            o.stderr = format!("blygger +import-opml: {e}\n");
            o.code = 1;
            return;
        }
    };
    let backend = match opml_backend(store) {
        Ok(b) => Some(b),
        Err(e) if dry => {
            o.stderr = format!(
                "blygger +import-opml: {e}; listing the file without checking what you follow\n"
            );
            None
        }
        Err(e) => {
            o.stderr = format!("blygger +import-opml: {e}\n");
            o.code = 1;
            return;
        }
    };
    let subs = backend
        .as_ref()
        .map(|b| b.subscriptions())
        .unwrap_or_default();
    let (already, new): (Vec<_>, Vec<_>) = parsed
        .feeds
        .iter()
        .partition(|f| opml::followed(f, &subs).is_some());
    let mut notes = vec![];
    if parsed.duplicates > 0 {
        notes.push(format!("{} duplicates collapsed", parsed.duplicates));
    }
    if parsed.without_feed > 0 {
        notes.push(format!(
            "{} without a feed address skipped",
            parsed.without_feed
        ));
    }
    let note = if notes.is_empty() {
        String::new()
    } else {
        format!(" ({})", notes.join(", "))
    };
    let line = |label: &str, f: &opml::OpmlFeed, extra: &str| {
        let folder = f
            .folder
            .as_deref()
            .map(|d| format!(" [{d}]"))
            .unwrap_or_default();
        format!("  {label:<18} {}{folder}  {}{extra}\n", f.title, f.xml_url)
    };
    if dry {
        o.stdout = format!("{} feeds in {file}{note}.\n", parsed.feeds.len());
        for f in &new {
            o.stdout.push_str(&line("would add", f, ""));
        }
        for f in &already {
            o.stdout.push_str(&line("already followed", f, ""));
        }
        o.stdout.push_str(&format!(
            "Would add {} (into the Reader folder \"{}\"); {} already followed. Nothing was changed.\n",
            new.len(),
            opml::IMPORT_FOLDER,
            already.len()
        ));
        return;
    }
    let Some(backend) = backend else { return };
    let feeds: Vec<opml::OpmlFeed> = new.into_iter().cloned().collect();
    let pace = Pace::default();
    eprintln!(
        "Importing {} of {} feeds from {file}{note}, about 30 a minute (each new subscription fetches its archive). Ctrl-C stops; what's done stays done.",
        feeds.len(),
        parsed.feeds.len()
    );
    let total = feeds.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let results = opml::run_import(
        &backend,
        &feeds,
        &pace,
        &std::sync::atomic::AtomicBool::new(false),
        &|p| match p {
            Progress::Waiting { seconds } => {
                eprintln!("  the blyg asked Burrow to slow down: waiting {seconds} s")
            }
            Progress::Done { index, outcome } => {
                let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                let what = match &outcome {
                    Got::Added { .. } => "added".to_string(),
                    Got::AlreadyFollowing => "already followed".to_string(),
                    Got::Failed { kind, reason } => format!("failed ({}): {reason}", kind.label()),
                };
                eprintln!("  {n}/{total} {}: {what}", feeds[index].xml_url);
            }
        },
    );
    let mut sum = Summary::of(&results);
    sum.already += already.len();
    for f in &already {
        o.stdout.push_str(&line("already followed", f, ""));
    }
    for (i, r) in results.iter().enumerate() {
        match r {
            Some(Got::Added { .. }) => o.stdout.push_str(&line("added", &feeds[i], "")),
            Some(Got::AlreadyFollowing) => {
                o.stdout.push_str(&line("already followed", &feeds[i], ""))
            }
            Some(Got::Failed { kind, reason }) => o.stdout.push_str(&line(
                &format!("failed ({})", kind.label()),
                &feeds[i],
                &format!(": {reason}"),
            )),
            None => {}
        }
    }
    o.stdout.push_str(&sum.line());
    if sum.added > 0 {
        o.stdout
            .push_str(&format!(". {}", crate::app::reading::opml::WHERE_THEY_GO));
    }
    o.stdout.push_str(".\n");
    if !sum.failed.is_empty() {
        o.code = 1;
    }
}

fn export_opml(args: &[&str], store: &ConfigStore, o: &mut Outcome) {
    use blyg_core::Backend as _;
    let Some(file) = args.first() else {
        o.stderr = format!(
            "usage: blygger +export-opml <file>   (e.g. {})\n",
            blyg_core::opml::EXPORT_FILE_NAME
        );
        o.code = 2;
        return;
    };
    let subs = match opml_backend(store) {
        Ok(b) => b.subscriptions(),
        Err(e) => {
            o.stderr = format!("blygger +export-opml: {e}\n");
            o.code = 1;
            return;
        }
    };
    match std::fs::write(file, blyg_core::opml::export(&subs)) {
        Ok(()) => o.stdout = format!("Saved {} subscriptions to {file}\n", subs.len()),
        Err(e) => {
            o.stderr = format!("blygger +export-opml: couldn't write {file}: {e}\n");
            o.code = 1;
        }
    }
}

/// The config's problems: the settings' own, then the extensions'.
fn diagnostics(store: &ConfigStore) -> Vec<Diagnostic> {
    let mut out = crate::settings::diagnostics(store.loaded(), &themes_of(store));
    out.extend(extension_diagnostics(store));
    out
}

// --- extensions ---

/// The extensions built into the app, each run as `blygger +ext <name>`.
const BUNDLED_EXTENSIONS: &[&str] = &[
    blyg_ext_notes::NAME,
    blyg_ext_crosspost::NAME,
    blyg_ext_reading_time::NAME, // --- reading slots ---
    blyg_ext_inspect::NAME,
];

/// How to serve a bundled extension over this process's stdin/stdout.
fn bundled_extension(name: &str) -> Option<fn() -> ExitCode> {
    match name {
        blyg_ext_notes::NAME => Some(blyg_ext_notes::run_stdio),
        blyg_ext_crosspost::NAME => Some(blyg_ext_crosspost::run_stdio),
        // --- reading slots ---
        blyg_ext_reading_time::NAME => Some(blyg_ext_reading_time::run_stdio),
        blyg_ext_inspect::NAME => Some(blyg_ext_inspect::run_stdio),
        _ => None,
    }
}

/// The extension host's view of this config: the `extension`,
/// `extension-allow` and `extension-setting` lines, the installed folder,
/// and the bundled extensions run as this executable's `+ext <name>`.
/// A `Host` built from it starts nothing until `Host::start`.
pub(crate) fn host_config(store: &ConfigStore) -> blyg_ext::HostConfig {
    let cfg = store.config();
    let mut c = blyg_ext::HostConfig::new(blyg_core::config::data_dir(), env!("CARGO_PKG_VERSION"));
    c.enabled = cfg.extensions_enabled();
    c.grants = blyg_ext::Grants::from_allow_lines(&cfg.list("extension-allow")).0;
    c.settings = blyg_ext::settings_from_lines(&cfg.list("extension-setting")).0;
    c.extensions_dir = store.extensions_dir();
    let exe = std::env::current_exe().unwrap_or_else(|_| "blygger".into());
    let notes = c
        .settings
        .get(blyg_ext_notes::NAME)
        .cloned()
        .unwrap_or_default();
    // Both are off until the config names them (`extension = …`).
    c.bundled = vec![
        blyg_ext_notes::bundled(
            exe.clone(),
            vec!["+ext".into(), blyg_ext_notes::NAME.into()],
            &notes,
        ),
        blyg_ext_crosspost::bundled(
            exe.clone(),
            vec!["+ext".into(), blyg_ext_crosspost::NAME.into()],
        ),
        // --- reading slots ---
        blyg_ext_reading_time::bundled(
            exe.clone(),
            vec!["+ext".into(), blyg_ext_reading_time::NAME.into()],
        ),
        blyg_ext_inspect::bundled(exe, vec!["+ext".into(), blyg_ext_inspect::NAME.into()]),
    ];
    c
}

/// What the extension lines and the manifests say, nothing started.
struct ExtView {
    status: Vec<blyg_ext::ExtensionStatus>,
    installed: Vec<blyg_ext::Installed>,
    problems: Vec<blyg_ext::Diagnostic>,
    /// Where each extension's storage (and `macro.log`) lives.
    data_dir: std::path::PathBuf,
}

/// The last `macro.log` line for macro `id` (`<time> <id> <outcome>`), as
/// `<time> <outcome>`.
fn last_macro_outcome(log: &str, id: &str) -> Option<String> {
    log.lines().rev().find_map(|l| {
        let mut w = l.splitn(3, ' ');
        let (time, mid, outcome) = (w.next()?, w.next()?, w.next()?);
        (mid == id).then(|| format!("{time} {outcome}"))
    })
}

fn extension_view(store: &ConfigStore) -> ExtView {
    let config = host_config(store);
    let data_dir = config.data_dir.clone();
    let (installed, _) = blyg_ext::discover(&config.bundled, config.extensions_dir.as_deref());
    // A host that is never started: its status comes from the manifests
    // and the config alone, and no process runs.
    let host = blyg_ext::Host::new(config, std::sync::Arc::new(blyg_ext::NoBlyg), |_| {});
    ExtView {
        status: host.status(),
        installed,
        problems: host.diagnostics(),
        data_dir,
    }
}

fn caps(list: &[blyg_ext::Capability]) -> String {
    list.iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// `+list-extensions`.
fn list_extensions(store: &ConfigStore) -> String {
    use blyg_ext::ExtState;
    let view = extension_view(store);
    let mut out = String::new();
    for e in &view.status {
        let whose = if e.bundled {
            "bundled"
        } else if e.state == ExtState::Missing {
            "not installed"
        } else {
            "installed"
        };
        let state = match &e.state {
            ExtState::Disabled => "off",
            ExtState::Missing => "enabled, but not installed",
            ExtState::NeedsConsent => "enabled; Burrow asks for consent when it starts it",
            _ if e.missing.is_empty() => "enabled",
            _ => "enabled; not everything it asks for is granted",
        };
        let version = if e.version.is_empty() {
            String::new()
        } else {
            format!(" {}", e.version)
        };
        out.push_str(&format!("{}{version} ({whose}): {state}\n", e.name));
        if !e.description.is_empty() {
            out.push_str(&format!("  {}\n", e.description));
        }
        if !e.requested.is_empty() {
            out.push_str(&format!("  asks for: {}\n", caps(&e.requested)));
        }
        if !e.granted.is_empty() {
            out.push_str(&format!("  granted:  {}\n", caps(&e.granted)));
        }
        if !e.missing.is_empty() && !e.granted.is_empty() {
            out.push_str(&format!("  missing:  {}\n", caps(&e.missing)));
        }
        for s in &e.sites {
            out.push_str(&format!(
                "  site:     {} ({}, {})\n",
                s.id, s.title, s.origin
            ));
        }
        let log = std::fs::read_to_string(
            blyg_ext::storage_dir(&view.data_dir, &e.name).join("macro.log"),
        )
        .unwrap_or_default();
        for m in &e.macros {
            out.push_str(&format!(
                "  macro:    {} \"{}\" on {}, tested: {}\n",
                m.id, m.title, m.site, m.tested
            ));
            if let Some(last) = last_macro_outcome(&log, &m.id) {
                out.push_str(&format!("            last run: {last}\n"));
            }
        }
        if !e.enabled {
            out.push_str(&format!("  turn on:  extension = {}\n", e.name));
        }
    }
    if !view.problems.is_empty() {
        out.push_str("\nProblems:\n");
        for d in &view.problems {
            out.push_str(&format!(
                "  {}: {}\n",
                blyg_core::config::paths::tilde(&d.path),
                d.message
            ));
        }
    }
    if let Some(d) = store.extensions_dir() {
        out.push_str(&format!(
            "\nInstall an extension by copying its folder into {}\n",
            blyg_core::config::paths::tilde(&d)
        ));
    }
    out
}

/// Warnings about the extension lines: a name no extension has, a grant
/// an enabled extension lacks or never asks for, a setting it doesn't
/// read, and broken manifests. Never errors: an extension problem never
/// stops Burrow from starting.
pub(crate) fn extension_diagnostics(store: &ConfigStore) -> Vec<Diagnostic> {
    let loaded = store.loaded();
    let view = extension_view(store);
    let warn = |file: &std::path::Path, line: usize, message: String| Diagnostic {
        file: file.to_path_buf(),
        line,
        severity: Severity::Warning,
        message,
    };
    let mut out: Vec<Diagnostic> = view
        .problems
        .iter()
        .map(|d| warn(&d.path, 0, format!("extension: {}", d.message)))
        .collect();
    let mut told = std::collections::HashSet::new();
    for e in &loaded.entries {
        let v = e.value.trim();
        let (name, rest) = match e.key.as_str() {
            "extension" => (v, ""),
            "extension-allow" | "extension-setting" => match v.split_once(char::is_whitespace) {
                Some((n, r)) => (n, r.trim()),
                None => continue,
            },
            _ => continue,
        };
        if name.is_empty() {
            continue;
        }
        let at = |message: String| warn(&e.file, e.line, message);
        let (Some(st), Some(inst)) = (
            view.status
                .iter()
                .find(|s| s.name == name && s.state != blyg_ext::ExtState::Missing),
            view.installed.iter().find(|i| i.manifest.name == name),
        ) else {
            out.push(at(format!(
                "{}: no extension named `{name}` is installed. See `blygger +list-extensions`",
                e.key
            )));
            continue;
        };
        match e.key.as_str() {
            "extension" if !st.missing.is_empty() && told.insert(name.to_string()) => {
                out.push(at(if st.granted.is_empty() {
                    format!(
                        "extension: `{name}` asks for {}; nothing is granted yet, so Burrow asks \
                         before it starts it",
                        caps(&st.missing)
                    )
                } else {
                    format!(
                        "extension: `{name}` asks for {}, not granted (add `extension-allow = \
                         {name} <capability>`, or it runs without them)",
                        caps(&st.missing)
                    )
                }));
            }
            "extension-allow" => {
                if let Some(cap) = blyg_ext::Capability::parse(rest)
                    && !st.requested.iter().any(|r| r.covered_by(&cap))
                {
                    let asks = if st.requested.is_empty() {
                        "it asks for nothing".to_string()
                    } else {
                        format!("it asks for {}", caps(&st.requested))
                    };
                    out.push(at(format!(
                        "extension-allow: `{name}` doesn't ask for {cap} ({asks})"
                    )));
                }
            }
            "extension-setting" => {
                let key = rest.split_once('=').map_or("", |(k, _)| k.trim());
                let keys: Vec<&str> = inst
                    .manifest
                    .settings
                    .iter()
                    .map(|s| s.key.as_str())
                    .collect();
                if !key.is_empty() && !keys.contains(&key) {
                    let has = if keys.is_empty() {
                        "it has no settings".to_string()
                    } else {
                        format!("its settings: {}", keys.join(", "))
                    };
                    out.push(at(format!(
                        "extension-setting: `{name}` has no setting `{key}` ({has})"
                    )));
                }
            }
            _ => {}
        }
    }
    out
}

/// The built-in themes plus the ones in this config's themes folder.
fn themes_of(store: &ConfigStore) -> blyg_core::config::theme::Registry {
    use blyg_core::config::theme::Registry;
    match store.themes_dir() {
        Some(d) => Registry::load(&d),
        None => Registry::builtin(),
    }
}

/// `+copy-theme <built-in> [name]`: write a built-in theme out as a file
/// in the themes folder (never over an existing one).
fn copy_theme(args: &[&str], store: &ConfigStore) -> Result<String, (u8, String)> {
    use blyg_core::config::theme::{BUILTIN, builtin_text, valid_name};
    let names = || {
        BUILTIN
            .iter()
            .map(|(n, _)| *n)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let usage = || {
        (
            2,
            format!(
                "usage: blygger +copy-theme <built-in> [name]\nbuilt-ins: {}",
                names()
            ),
        )
    };
    let from = args.first().ok_or_else(usage)?.to_ascii_lowercase();
    let text = builtin_text(&from).ok_or_else(|| {
        (
            2,
            format!("`{from}` isn't a built-in theme (one of {})", names()),
        )
    })?;
    let to = args
        .get(1)
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| from.clone());
    if !valid_name(&to) {
        return Err((
            2,
            format!("`{to}` isn't a theme name (use lowercase letters, digits and -)"),
        ));
    }
    let dir = store
        .themes_dir()
        .ok_or_else(|| (1, "no themes folder for this config".to_string()))?;
    let path = dir.join(&to);
    if path.exists() {
        return Err((
            1,
            format!(
                "{} already exists; pass another name",
                blyg_core::config::paths::tilde(&path)
            ),
        ));
    }
    let mut out = format!(
        "# Copied from the built-in {from} theme. Edit it and save: the app reloads it.\n\
         # Use it with `theme = {to}` in the config. The format: docs/THEMES.md.\n"
    );
    for line in text.lines() {
        if to != from && line.trim_start().starts_with("name ") {
            out.push_str(&format!("name = {to}\n"));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(&path, out))
        .map_err(|e| (1, format!("couldn't write {}: {e}", path.display())))?;
    Ok(format!(
        "Wrote {}\nUse it with: theme = {to}\n",
        blyg_core::config::paths::tilde(&path)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on_disk(dir: &std::path::Path) -> impl FnMut() -> ConfigStore + '_ {
        move || ConfigStore::open(blyg_core::config::ConfigFiles::single(dir.join("config")))
    }

    // --- OPML --- without a connected blyg (the e2e runs the real thing).
    #[test]
    fn opml_commands_without_a_blyg() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("feeds.opml");
        std::fs::write(
            &file,
            r#"<opml version="2.0"><body><outline text="Tech">
              <outline text="Kit" xmlUrl="https://kit.example.org/feed.xml"/>
              <outline text="Kit again" xmlUrl="https://kit.example.org/feed.xml"/>
            </outline></body></opml>"#,
        )
        .unwrap();
        let f = file.to_str().unwrap();
        let o = exec("+import-opml", &[f, "--dry-run"], &mut on_disk(dir.path()));
        assert_eq!(o.code, 0, "{}", o.stderr);
        assert!(o.stderr.contains("no blyg is connected"), "{}", o.stderr);
        assert!(o.stdout.contains("1 feeds in"), "{}", o.stdout);
        assert!(o.stdout.contains("1 duplicates collapsed"), "{}", o.stdout);
        assert!(
            o.stdout
                .contains("would add          Kit [Tech]  https://kit.example.org/feed.xml"),
            "{}",
            o.stdout
        );
        assert!(o.stdout.contains("Nothing was changed"), "{}", o.stdout);

        let o = exec("+import-opml", &[f], &mut on_disk(dir.path()));
        assert_eq!(o.code, 1);
        assert!(o.stderr.contains("no blyg is connected"), "{}", o.stderr);
        assert_eq!(exec("+import-opml", &[], &mut on_disk(dir.path())).code, 2);
        let missing = dir.path().join("nope.opml");
        let o = exec(
            "+import-opml",
            &[missing.to_str().unwrap()],
            &mut on_disk(dir.path()),
        );
        assert!(o.stderr.contains("Couldn't read the file"), "{}", o.stderr);

        let out = dir.path().join("out.opml");
        let o = exec(
            "+export-opml",
            &[out.to_str().unwrap()],
            &mut on_disk(dir.path()),
        );
        assert_eq!(o.code, 1);
        assert!(!out.exists());
        assert_eq!(exec("+export-opml", &[], &mut on_disk(dir.path())).code, 2);
        assert!(HELP.contains("+import-opml <file> [--dry-run]"));
        assert!(HELP.contains("+export-opml <file>"));
    }

    #[test]
    fn copy_theme_round_trips_every_builtin() {
        use blyg_core::config::theme::{BUILTIN, Registry};
        let dir = tempfile::tempdir().unwrap();
        let builtins = Registry::builtin();
        for (id, _) in BUILTIN {
            let name = format!("my-{id}");
            let o = exec("+copy-theme", &[id, &name], &mut on_disk(dir.path()));
            assert_eq!(o.code, 0, "{}", o.stderr);
            assert!(o.stdout.contains(&format!("theme = {name}")));
        }
        // And one under its own name (it then replaces the built-in).
        assert_eq!(
            exec("+copy-theme", &["kumiko"], &mut on_disk(dir.path())).code,
            0
        );
        let loaded = Registry::load(&dir.path().join("themes"));
        assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
        for (id, _) in BUILTIN {
            let copy = loaded.resolve(&format!("my-{id}")).unwrap();
            let orig = builtins.resolve(id).unwrap();
            assert_eq!(
                copy.with_identity(&orig.id, &orig.name, true),
                orig,
                "{id} round-trips"
            );
        }
        assert!(!loaded.resolve("kumiko").unwrap().builtin);

        let again = exec("+copy-theme", &["kumiko"], &mut on_disk(dir.path()));
        assert_eq!(again.code, 1);
        assert!(again.stderr.contains("already exists"));
        assert_eq!(exec("+copy-theme", &[], &mut on_disk(dir.path())).code, 2);
        assert_eq!(
            exec("+copy-theme", &["sepia"], &mut on_disk(dir.path())).code,
            2
        );
        assert_eq!(
            exec(
                "+copy-theme",
                &["aizome", "Bad Name"],
                &mut on_disk(dir.path())
            )
            .code,
            2
        );

        let l = exec("+list-themes", &[], &mut on_disk(dir.path()));
        assert!(l.stdout.contains("Woody:\n  cutaway"), "{}", l.stdout);
        assert!(l.stdout.contains("my-konkan"), "{}", l.stdout);
        assert!(l.stdout.contains("(yours)"));
    }

    fn with(text: &'static str) -> impl FnMut() -> ConfigStore {
        move || ConfigStore::in_memory(text)
    }

    #[test]
    fn actions() {
        let v = exec("+version", &[], &mut with(""));
        assert!(v.stdout.starts_with("blygger "));
        assert_eq!(v.code, 0);

        let f = exec("+list-fonts", &[], &mut with(""));
        assert!(f.stdout.contains("font-family-writing:\n  Literata"));
        assert!(f.stdout.contains("font-family-ui:\n  Inter"));

        let bad = exec("+frobnicate", &[], &mut with(""));
        assert_eq!(bad.code, 2);
        assert!(bad.stderr.contains("unknown action +frobnicate"));
        assert!(run(&["-psn_0_123".into()]).is_none(), "no +action: launch");
    }

    #[test]
    fn show_config_prints_changes_and_docs() {
        let s = exec("+show-config", &[], &mut with("# hi\ntheme = dark\n"));
        assert_eq!(s.stdout, "theme = dark\n");
        let d = exec(
            "+show-config",
            &["--default", "--docs"],
            &mut with("theme = dark\n"),
        );
        assert!(d.stdout.contains("\ntheme = system\n"));
        assert!(d.stdout.contains("# Colour theme"));
        assert_eq!(exec("+show-config", &["--nope"], &mut with("")).code, 2);
    }

    #[test]
    fn validate_config_warns_about_extension_lines() {
        let dir = tempfile::tempdir().unwrap();
        let ext = dir.path().join("extensions");
        std::fs::create_dir_all(ext.join("hello")).unwrap();
        std::fs::write(
            ext.join("hello").join("extension.toml"),
            "name = \"hello\"\nversion = \"0.1.0\"\nprotocol = 1\ncommand = [\"hello\"]\n\
             capabilities = [\"ui\", \"items.read\"]\n\
             [[settings]]\nkey = \"greeting\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(ext.join("broken")).unwrap();
        std::fs::write(ext.join("broken").join("extension.toml"), "name = 3\n").unwrap();
        std::fs::write(
            dir.path().join("config"),
            "extension = markdown-notes\n\
             extension = hello\n\
             extension-allow = hello ui\n\
             extension-allow = hello net\n\
             extension-setting = hello greeting=hi\n\
             extension-setting = hello colour=red\n\
             extension = typo-notes\n\
             extension-allow = typo-notes ui\n",
        )
        .unwrap();
        let o = exec("+validate-config", &[], &mut on_disk(dir.path()));
        assert_eq!(o.code, 0, "extension problems are warnings: {}", o.stdout);
        let lines: Vec<&str> = o.stdout.lines().collect();
        let has = |line: usize, text: &str| {
            assert!(
                lines
                    .iter()
                    .any(|l| l.contains(&format!("config:{line}: warning: {text}"))),
                "line {line}: {text}\n{}",
                o.stdout
            )
        };
        has(
            1,
            "extension: `markdown-notes` asks for ui; nothing is granted yet",
        );
        has(
            2,
            "extension: `hello` asks for items.read, not granted (add `extension-allow = hello <capability>`",
        );
        has(
            4,
            "extension-allow: `hello` doesn't ask for net (it asks for ui, items.read)",
        );
        has(
            6,
            "extension-setting: `hello` has no setting `colour` (its settings: greeting)",
        );
        has(
            7,
            "extension: no extension named `typo-notes` is installed. See `blygger +list-extensions`",
        );
        has(8, "extension-allow: no extension named `typo-notes`");
        assert!(
            o.stdout.contains("broken") && o.stdout.contains("warning: extension:"),
            "{}",
            o.stdout
        );
        assert!(!o.stdout.contains("config:3:"), "{}", o.stdout);
        assert!(!o.stdout.contains("config:5:"), "{}", o.stdout);

        // Fully granted and known: nothing to say.
        std::fs::remove_dir_all(ext.join("broken")).unwrap();
        std::fs::write(
            dir.path().join("config"),
            "extension = hello\nextension-allow = hello ui\nextension-allow = hello items.read\n",
        )
        .unwrap();
        let ok = exec("+validate-config", &[], &mut on_disk(dir.path()));
        assert!(ok.stdout.starts_with("OK: "), "{}", ok.stdout);

        let help = exec("+help", &[], &mut with(""));
        assert!(help.stdout.contains("+list-extensions"));
        assert!(help.stdout.contains("+ext <name>"));
        let bad = exec("+ext", &["nope"], &mut with(""));
        assert_eq!(bad.code, 2);
        assert!(bad.stderr.contains("`nope` isn't a bundled extension"));
    }

    #[test]
    fn validate_config_exit_codes() {
        let ok = exec("+validate-config", &[], &mut with("theme = dark\n"));
        assert_eq!(ok.code, 0);
        let warn = exec("+validate-config", &[], &mut with("fnot-size = 3\n"));
        assert_eq!(warn.code, 0);
        assert!(
            warn.stdout
                .contains("config:1: warning: unknown key `fnot-size`")
        );
        let err = exec(
            "+validate-config",
            &[],
            &mut with("theme = dark\ncapture-hotkey = ctrl+alt+nope\n"),
        );
        assert_eq!(err.code, 1);
        assert!(
            err.stdout.contains("config:2: error: capture-hotkey"),
            "{}",
            err.stdout
        );
    }
}
