//! Every environment variable the app reads, parsed once at startup into
//! typed values. The rest of the crate asks [`get`]; `clippy.toml` bans
//! `std::env::var`/`var_os` everywhere else in blyg-app.
//!
//! A process-wide value rather than a field threaded through GPUI entities:
//! the readers include free functions with no `App` at hand
//! (`delight::reduce_motion`, `browser::teardown_after`, the update check,
//! the command line before GPUI starts), and the environment is fixed for
//! the life of the process anyway.
//!
//! Variable names never change (CLAUDE.md, "Naming").

use std::ffi::OsString;
use std::sync::OnceLock;

use crate::scenes::Scene;

/// What the environment asks of this run.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DevFlags {
    pub fake: Fake,
    /// `BLYGGER_TEST_TOKEN`: an in-memory owner token for the configured
    /// blyg (a local studio), and no Keychain.
    pub test_token: Option<String>,
    /// `BLYGGER_DATA_DIR` is set (`blyg_core::config::data_dir` resolves it):
    /// no migration of the legacy data folder.
    pub data_dir_override: bool,
    /// `BLYGGER_TIMING`: print timings and milestones for automation.
    pub timing: bool,
    /// `BLYGGER_NO_ACTIVATE`: never take focus from the frontmost app.
    pub no_activate: bool,
    /// `BLYGGER_REDUCE_MOTION`: `Some(true)` unless it is `0`; unset follows
    /// the system setting.
    pub reduce_motion: Option<bool>,
    /// `BLYGGER_THEME=<name>`: a theme override for screenshots.
    pub theme: Option<String>,
    /// `BLYGGER_VERSION_AS=<version>` (debug builds): what's new counts this
    /// launch as that version.
    pub version_as: Option<String>,
    /// `BLYGGER_TEST_SPELLING=windows|mac`: the platform whose wording the
    /// app's text uses. Tests start every thread with it
    /// (`platform_text::spelled_as` still picks one for a test); a debug
    /// build shows it, so a scene on a Mac shows Windows' wording. Release
    /// builds ignore it. Unset: this machine's.
    pub spelling: Option<crate::platform_text::Platform>,
    pub update: Update,
    pub browser: Browser,
    pub snapshot: Snapshot,
    pub demo: Demo,
}

/// Fake mode and how the fake server behaves.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Fake {
    /// `BLYGGER_FAKE`: sample data, an in-memory token store, no blyg.
    pub on: bool,
    /// `BLYGGER_FAKE_NO_READ`: a server without the read extensions.
    pub no_read: bool,
    /// `BLYGGER_FAKE_NO_PROVENANCE`: a Worker without the provenance extension.
    pub no_provenance: bool,
    /// `BLYGGER_FAKE_STUDIO_LATEST`: the studio release the fake reports.
    pub studio_latest: Option<String>,
}

/// The auto-updater's switches.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Update {
    /// `BLYGGER_NO_UPDATE`: no update checks or downloads.
    pub off: bool,
    /// `BLYGGER_UPDATE_URL` (debug builds): another release feed.
    pub url: Option<String>,
    /// `BLYGGER_UPDATE_PUBKEY` (debug builds): another signing key.
    pub pubkey: Option<String>,
    /// `BLYGGER_UPDATE_SMOKE`: what to do once an update is ready.
    pub smoke: Option<UpdateSmoke>,
}

/// `BLYGGER_UPDATE_SMOKE=restart|quit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateSmoke {
    Restart,
    Quit,
}

/// The in-app browser's test switches.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Browser {
    /// `BLYGGER_BROWSER_TEARDOWN_MS` (debug builds): close the idle web
    /// process sooner.
    pub teardown_ms: Option<u64>,
    /// `BLYGGER_BROWSER_EPHEMERAL` (debug builds): no persistent cookies.
    pub ephemeral: bool,
    /// `BLYGGER_NO_BLOCKLIST_DOWNLOAD`: never download the block lists.
    pub no_blocklist_download: bool,
    /// `BLYGGER_MACRO_TRACE=1`: `macro.log` gets the page's outline.
    pub macro_trace: bool,
}

/// Saving frames for screenshots (`blygger_snap` builds).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Snapshot {
    /// `BLYGGER_SNAPSHOT=<png>`: save the last frame here.
    pub path: Option<String>,
    /// `BLYGGER_SNAPSHOT_DIR`: where macro demos save their frames.
    pub dir: Option<String>,
    /// `BLYGGER_SNAPSHOT_WIDTH` / `_HEIGHT`: the window size.
    pub width: Option<f32>,
    pub height: Option<f32>,
}

/// `BLYGGER_DEMO=<scene>` and the variables that tune a scene.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Demo {
    /// `BLYGGER_DEMO`: the scene to play (an unknown name is an error).
    pub scene: Option<Scene>,
    /// `BLYGGER_DEMO_URL`: the page or blyg a scene opens.
    pub url: Option<String>,
    /// `BLYGGER_DEMO_URL2`: a second host (the browser shield scene).
    pub url2: Option<String>,
    /// `BLYGGER_DEMO_SITE_URL`: the author's site for the opened post.
    pub site_url: Option<String>,
    /// `BLYGGER_DEMO_NOTE`: the note the notes extension opens.
    pub note: Option<String>,
    /// `BLYGGER_DEMO_FROM=<version>`: what's new since that version.
    pub from: Option<String>,
    /// `BLYGGER_DEMO_HOVER=<action>`: the toolbar button to hover.
    pub hover: Option<String>,
    /// `BLYGGER_DEMO_SCROLL=bottom`: scroll the page to its end.
    pub scroll_bottom: bool,
    /// `BLYGGER_DEMO_PREVIEW=<theme>|font:<name>|…`: what settings-preview tries.
    pub preview: Option<String>,
    /// `BLYGGER_DEMO_SLOT=dark`: preview into the dark theme slot.
    pub slot_dark: bool,
    /// `BLYGGER_DEMO_HOLD_MS`: how long a macro sheet stays up.
    pub hold_ms: u64,
    /// `BLYGGER_DEMO_FOLD`: fold the Post sheet's pane while it is held.
    pub fold: bool,
}

/// Environment values only tests read (`cargo test`, never the app).
#[cfg(test)]
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TestEnv {
    /// `BLYGGER_LISTS_DIR`: block lists already downloaded.
    pub lists_dir: Option<std::path::PathBuf>,
    /// `BLYG_E2E_URL`, `BLYG_E2E_TOKEN`: blyg `a` of the switcher e2e.
    pub e2e_url: Option<String>,
    pub e2e_token: Option<String>,
    /// `BLYG_E2E_SWITCH_URL`, `BLYG_E2E_SWITCH_TOKEN`: blyg `b`.
    pub e2e_switch_url: Option<String>,
    pub e2e_switch_token: Option<String>,
    /// `BLYG_E2E_URL_B`: a second blyg at `a`'s host.
    pub e2e_url_b: Option<String>,
    /// `BLYGGER_UPDATE_DOCS`: the generated-docs tests rewrite their file
    /// instead of comparing (`scripts/gen-docs.sh`).
    pub update_docs: bool,
}

type Lookup<'a> = &'a dyn Fn(&str) -> Option<OsString>;

fn is_set(get: Lookup, key: &str) -> bool {
    get(key).is_some()
}

fn text(get: Lookup, key: &str) -> Option<String> {
    get(key).and_then(|v| v.into_string().ok())
}

fn number<T: std::str::FromStr>(get: Lookup, key: &str) -> Option<T> {
    text(get, key).and_then(|v| v.parse().ok())
}

impl DevFlags {
    /// Parse the variables `get` returns (the process environment in the
    /// app; a table in tests).
    pub fn parse(get: Lookup) -> Result<Self, String> {
        Ok(Self {
            fake: Fake {
                on: is_set(get, "BLYGGER_FAKE"),
                no_read: is_set(get, "BLYGGER_FAKE_NO_READ"),
                no_provenance: is_set(get, "BLYGGER_FAKE_NO_PROVENANCE"),
                studio_latest: text(get, "BLYGGER_FAKE_STUDIO_LATEST"),
            },
            test_token: text(get, "BLYGGER_TEST_TOKEN"),
            data_dir_override: is_set(get, "BLYGGER_DATA_DIR"),
            timing: is_set(get, "BLYGGER_TIMING"),
            no_activate: is_set(get, "BLYGGER_NO_ACTIVATE"),
            reduce_motion: get("BLYGGER_REDUCE_MOTION").map(|v| v != "0"),
            theme: text(get, "BLYGGER_THEME"),
            version_as: text(get, "BLYGGER_VERSION_AS"),
            spelling: text(get, "BLYGGER_TEST_SPELLING")
                .map(|v| {
                    use crate::platform_text::Platform;
                    match v.to_ascii_lowercase().as_str() {
                        "windows" => Ok(Platform::Windows),
                        "mac" | "macos" => Ok(Platform::Mac),
                        other => Err(format!("BLYGGER_TEST_SPELLING={other}: say windows or mac")),
                    }
                })
                .transpose()?,
            update: Update {
                off: is_set(get, "BLYGGER_NO_UPDATE"),
                url: text(get, "BLYGGER_UPDATE_URL"),
                pubkey: text(get, "BLYGGER_UPDATE_PUBKEY"),
                smoke: match text(get, "BLYGGER_UPDATE_SMOKE").as_deref() {
                    Some("restart") => Some(UpdateSmoke::Restart),
                    Some("quit") => Some(UpdateSmoke::Quit),
                    _ => None,
                },
            },
            browser: Browser {
                teardown_ms: number(get, "BLYGGER_BROWSER_TEARDOWN_MS"),
                ephemeral: is_set(get, "BLYGGER_BROWSER_EPHEMERAL"),
                no_blocklist_download: is_set(get, "BLYGGER_NO_BLOCKLIST_DOWNLOAD"),
                macro_trace: text(get, "BLYGGER_MACRO_TRACE").as_deref() == Some("1"),
            },
            snapshot: Snapshot {
                path: text(get, "BLYGGER_SNAPSHOT"),
                dir: text(get, "BLYGGER_SNAPSHOT_DIR"),
                width: number(get, "BLYGGER_SNAPSHOT_WIDTH"),
                height: number(get, "BLYGGER_SNAPSHOT_HEIGHT"),
            },
            demo: Demo {
                scene: text(get, "BLYGGER_DEMO")
                    .map(|s| Scene::parse(&s))
                    .transpose()
                    .map_err(|e| e.to_string())?,
                url: text(get, "BLYGGER_DEMO_URL"),
                url2: text(get, "BLYGGER_DEMO_URL2"),
                site_url: text(get, "BLYGGER_DEMO_SITE_URL"),
                note: text(get, "BLYGGER_DEMO_NOTE"),
                from: text(get, "BLYGGER_DEMO_FROM"),
                hover: text(get, "BLYGGER_DEMO_HOVER"),
                scroll_bottom: text(get, "BLYGGER_DEMO_SCROLL").as_deref() == Some("bottom"),
                preview: text(get, "BLYGGER_DEMO_PREVIEW"),
                slot_dark: text(get, "BLYGGER_DEMO_SLOT").as_deref() == Some("dark"),
                hold_ms: number(get, "BLYGGER_DEMO_HOLD_MS").unwrap_or(0),
                fold: is_set(get, "BLYGGER_DEMO_FOLD"),
            },
        })
    }

    /// The process environment, parsed. The only reader of it in blyg-app.
    #[expect(
        clippy::disallowed_methods,
        reason = "the one reader of the environment"
    )]
    fn from_env() -> Result<Self, String> {
        Self::parse(&|k| std::env::var_os(k))
    }
}

/// The NAMES of the process environment's variables, never their values
/// (values can be tokens). Only for [`refuse_ignored`].
#[expect(
    clippy::disallowed_methods,
    reason = "the one reader of the environment's names"
)]
fn env_names() -> Vec<String> {
    std::env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .collect()
}

#[cfg(test)]
impl TestEnv {
    #[expect(
        clippy::disallowed_methods,
        reason = "the one reader of the tests' environment"
    )]
    pub fn from_env() -> Self {
        let get = |k: &str| std::env::var(k).ok();
        Self {
            lists_dir: std::env::var_os("BLYGGER_LISTS_DIR").map(Into::into),
            e2e_url: get("BLYG_E2E_URL"),
            e2e_token: get("BLYG_E2E_TOKEN"),
            e2e_switch_url: get("BLYG_E2E_SWITCH_URL"),
            e2e_switch_token: get("BLYG_E2E_SWITCH_TOKEN"),
            e2e_url_b: get("BLYG_E2E_URL_B"),
            update_docs: std::env::var_os("BLYGGER_UPDATE_DOCS").is_some(),
        }
    }
}

static FLAGS: OnceLock<DevFlags> = OnceLock::new();

/// Parse the environment (first thing in `main`). An error names the
/// variable and what's wrong with its value. It does not look for ignored
/// variables ([`refuse_ignored`] does, once `main` knows this is an app
/// start), so `--version` and `--help` are never held up by them.
pub fn init() -> Result<&'static DevFlags, String> {
    if let Some(f) = FLAGS.get() {
        return Ok(f);
    }
    let f = DevFlags::from_env()?;
    Ok(FLAGS.get_or_init(|| f))
}

/// This run's flags (parsed on first use where `main` didn't run: tests).
pub fn get() -> &'static DevFlags {
    FLAGS.get_or_init(|| DevFlags::from_env().unwrap_or_else(|e| panic!("blygger: {e}")))
}

/// Windows: GPUI draws through a topmost DirectComposition visual, which
/// would cover the WebView2 child window that shows posts. Its plain
/// swap-chain path leaves child windows visible above the app.
#[cfg(target_os = "windows")]
#[expect(
    clippy::disallowed_methods,
    reason = "GPUI reads this variable, so it is set here, before GPUI starts"
)]
#[expect(
    unsafe_code,
    reason = "std::env::set_var is unsafe in edition 2024; called first thing in main, before other threads"
)]
pub fn prefer_swap_chain() {
    if std::env::var_os("GPUI_DISABLE_DIRECT_COMPOSITION").is_none() {
        // SAFETY: first thing in main, before any other thread exists.
        unsafe { std::env::set_var("GPUI_DISABLE_DIRECT_COMPOSITION", "1") };
    }
}

// ---- classes of every name ----
// `never_rename_tests.rs` scans the names this file reads down to this line
// only, and `every_blygger_name_has_a_class` holds those names to the table.

/// Who reads a `BLYGGER_*` variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// The app reads it in every build.
    Always,
    /// The app reads it in debug builds only.
    DebugOnly,
    /// Only the tests (`TestEnv` or a test) read it; the running app
    /// ignores it.
    TestsOnly,
    /// A real name the app's process may see but dev_flags.rs does not act
    /// on: another crate reads it, or a script exports it into the
    /// environment the app inherits.
    Elsewhere,
}

use Class::{Always, DebugOnly, Elsewhere, TestsOnly};

/// Every `BLYGGER_*` name, and who reads it. A name not here is ignored by
/// every build and said so at startup ([`ignored`]).
/// `tests/cli.rs` parses this table (one `("NAME", Class)` entry per line) to
/// find the `TestsOnly` names it removes from the environment of every
/// `blygger` it spawns: keep that shape.
pub const CLASSES: &[(&str, Class)] = &[
    // Read by `DevFlags::parse`, in every build.
    ("BLYGGER_FAKE", Always),
    ("BLYGGER_FAKE_NO_READ", Always),
    ("BLYGGER_FAKE_NO_PROVENANCE", Always),
    ("BLYGGER_FAKE_STUDIO_LATEST", Always),
    ("BLYGGER_TEST_TOKEN", Always),
    ("BLYGGER_TIMING", Always),
    ("BLYGGER_NO_ACTIVATE", Always),
    ("BLYGGER_REDUCE_MOTION", Always),
    ("BLYGGER_THEME", Always),
    ("BLYGGER_NO_UPDATE", Always),
    ("BLYGGER_NO_BLOCKLIST_DOWNLOAD", Always),
    ("BLYGGER_MACRO_TRACE", Always),
    ("BLYGGER_SNAPSHOT", Always),
    ("BLYGGER_SNAPSHOT_DIR", Always),
    ("BLYGGER_SNAPSHOT_WIDTH", Always),
    ("BLYGGER_SNAPSHOT_HEIGHT", Always),
    ("BLYGGER_DEMO", Always),
    ("BLYGGER_DEMO_URL", Always),
    ("BLYGGER_DEMO_URL2", Always),
    ("BLYGGER_DEMO_SITE_URL", Always),
    ("BLYGGER_DEMO_NOTE", Always),
    ("BLYGGER_DEMO_FROM", Always),
    ("BLYGGER_DEMO_HOVER", Always),
    ("BLYGGER_DEMO_SCROLL", Always),
    ("BLYGGER_DEMO_PREVIEW", Always),
    ("BLYGGER_DEMO_SLOT", Always),
    ("BLYGGER_DEMO_HOLD_MS", Always),
    ("BLYGGER_DEMO_FOLD", Always),
    // Used only under `cfg!(debug_assertions)` / `#[cfg(debug_assertions)]`.
    // `platform_text.rs:48` (`Platform::spelling`), which `keymap::hint` asks.
    ("BLYGGER_TEST_SPELLING", DebugOnly),
    // `onboarding/whats_new.rs:130`.
    ("BLYGGER_VERSION_AS", DebugOnly),
    // `update/mod.rs:298` (`debug_overrides`; the release twin at `:315`
    // reads neither).
    ("BLYGGER_UPDATE_URL", DebugOnly),
    ("BLYGGER_UPDATE_PUBKEY", DebugOnly),
    // `update/mod.rs:596` (`smoke_hook`).
    ("BLYGGER_UPDATE_SMOKE", DebugOnly),
    // `browser/mod.rs:82` (`teardown_after`).
    ("BLYGGER_BROWSER_TEARDOWN_MS", DebugOnly),
    // `browser/surface.rs:431` and `:1028`.
    ("BLYGGER_BROWSER_EPHEMERAL", DebugOnly),
    // Read by `TestEnv` or a test; the running app never looks.
    ("BLYGGER_LISTS_DIR", TestsOnly),
    ("BLYGGER_UPDATE_DOCS", TestsOnly),
    // `crates/blyg-core/tests/perf.rs:353`.
    ("BLYGGER_PERF_DB", TestsOnly),
    // Read by another crate (`blyg-core`: `config/paths.rs`, `discover.rs`).
    ("BLYGGER_CONFIG", Elsewhere),
    ("BLYGGER_DATA_DIR", Elsewhere),
    ("BLYGGER_EXTENSIONS_DIR", Elsewhere),
    ("BLYGGER_THEMES_DIR", Elsewhere),
    ("BLYGGER_DISCOVER_URL", Elsewhere),
    // Read by scripts, and inherited by the app they launch from the
    // caller's environment (`scripts/browser-block-check.sh`,
    // `scripts/verify/burrow-verify.sh`, `scripts/install.sh`).
    ("BLYGGER_BIN", Elsewhere),
    ("BLYGGER_RUNS", Elsewhere),
    ("BLYGGER_REAL_LISTS", Elsewhere),
    ("BLYGGER_DEST", Elsewhere),
    ("BLYGGER_BASE_URL", Elsewhere),
    ("BLYGGER_VERSION", Elsewhere),
    ("BLYGGER_NO_VERIFY", Elsewhere),
];

fn class_of(name: &str) -> Option<Class> {
    CLASSES.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// Why a variable is ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// Not a name Burrow knows: probably misspelt.
    Unknown { did_you_mean: Option<&'static str> },
    /// Only the test suite reads it.
    TestsOnly,
    /// Only debug builds read it, and this is not one.
    DebugOnly,
}

/// A `BLYGGER_*` variable present in the environment that this build does
/// not act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ignored {
    pub name: String,
    pub why: Why,
}

impl std::fmt::Display for Ignored {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: ", self.name)?;
        match &self.why {
            Why::Unknown {
                did_you_mean: Some(n),
            } => write!(f, "not a variable Burrow reads (did you mean {n}?)"),
            Why::Unknown { did_you_mean: None } => write!(f, "not a variable Burrow reads"),
            Why::TestsOnly => write!(f, "read only by the test suite"),
            Why::DebugOnly => write!(f, "read only by debug builds"),
        }
    }
}

/// `a` and `b` differ by at most one inserted, deleted or changed byte.
fn within_one_edit(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if long.len() - short.len() > 1 {
        return false;
    }
    let same = short.iter().zip(long).take_while(|(x, y)| x == y).count();
    if short.len() == long.len() {
        short.get(same + 1..) == long.get(same + 1..)
    } else {
        short[same..] == long[same + 1..]
    }
}

/// Of `names` (the variable NAMES present in the environment) the
/// `BLYGGER_*` ones this build will not act on, in name order, each with
/// why. `Always`, `Elsewhere` and (in a debug build) `DebugOnly` names
/// return nothing; other prefixes are never looked at.
pub fn ignored(names: impl Iterator<Item = String>, debug: bool) -> Vec<Ignored> {
    let mut out: Vec<Ignored> = names
        .filter_map(|name| {
            // Windows looks variables up without regard to case; so do we.
            let key = if cfg!(windows) {
                name.to_ascii_uppercase()
            } else {
                name.clone()
            };
            if !key.starts_with("BLYGGER_") {
                return None;
            }
            let why = match class_of(&key) {
                Some(Always | Elsewhere) => return None,
                Some(DebugOnly) if debug => return None,
                Some(DebugOnly) => Why::DebugOnly,
                Some(TestsOnly) => Why::TestsOnly,
                None => Why::Unknown {
                    did_you_mean: CLASSES
                        .iter()
                        .map(|(n, _)| *n)
                        .find(|n| within_one_edit(n, &key)),
                },
            };
            Some(Ignored { name, why })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// The `init` error of a debug build: every ignored variable, why, and what
/// to do. `main` prints it after `blygger: `.
fn refusal(list: &[Ignored]) -> String {
    let mut text = String::from(
        "these environment variables are ignored by this build, so a debug build will not start with them set:",
    );
    for i in list {
        text.push_str("\n  ");
        text.push_str(&i.to_string());
    }
    text.push_str(
        "\nUnset them, or use what reads them: a debug build, or the test suite (`cargo test`).",
    );
    text
}

/// `main` calls this after `cli::run` has declined (an app start, not a
/// `--` option or `+action`), before any window or backend work. Not in
/// [`get`] or [`init`]: tests never call it. See [`report_ignored`].
pub fn refuse_ignored() -> Result<(), String> {
    report_ignored(env_names().into_iter(), cfg!(debug_assertions))
}

/// Debug build: any ignored variable is an `Err`. Release build: each is
/// said once on stderr and the app starts. Never prints a value.
fn report_ignored(names: impl Iterator<Item = String>, debug: bool) -> Result<(), String> {
    let list = ignored(names, debug);
    if list.is_empty() {
        return Ok(());
    }
    if debug {
        return Err(refusal(&list));
    }
    for i in &list {
        eprintln!("blygger: {i}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(vars: &[(&str, &str)]) -> DevFlags {
        DevFlags::parse(&|k| {
            vars.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| OsString::from(v))
        })
        .unwrap()
    }

    #[test]
    fn an_empty_environment_is_the_default() {
        assert_eq!(parse(&[]), DevFlags::default());
    }

    #[test]
    fn flags_parse_into_their_types() {
        let f = parse(&[
            ("BLYGGER_FAKE", "1"),
            ("BLYGGER_REDUCE_MOTION", "0"),
            ("BLYGGER_UPDATE_SMOKE", "quit"),
            ("BLYGGER_SNAPSHOT_WIDTH", "800"),
            ("BLYGGER_DEMO_SCROLL", "bottom"),
            ("BLYGGER_DEMO_SLOT", "light"),
            ("BLYGGER_DEMO_HOLD_MS", "x"),
            ("BLYGGER_MACRO_TRACE", "yes"),
        ]);
        assert!(f.fake.on);
        assert_eq!(f.reduce_motion, Some(false));
        assert_eq!(f.update.smoke, Some(UpdateSmoke::Quit));
        assert_eq!(f.snapshot.width, Some(800.0));
        assert!(f.demo.scroll_bottom);
        assert!(!f.demo.slot_dark);
        assert_eq!(f.demo.hold_ms, 0);
        assert!(!f.browser.macro_trace);
    }

    /// `BLYGGER_TEST_SPELLING` is read here, so a debug build shows
    /// Windows' wording on a Mac (`burrow-verify.sh` scenes), not only tests.
    #[test]
    fn the_test_spelling_is_a_dev_flag() {
        use crate::platform_text::Platform;
        assert_eq!(
            parse(&[("BLYGGER_TEST_SPELLING", "windows")]).spelling,
            Some(Platform::Windows)
        );
        assert_eq!(
            parse(&[("BLYGGER_TEST_SPELLING", "Mac")]).spelling,
            Some(Platform::Mac)
        );
        assert_eq!(parse(&[]).spelling, None);
        let bad =
            DevFlags::parse(&|k| (k == "BLYGGER_TEST_SPELLING").then(|| OsString::from("linux")));
        assert!(bad.is_err_and(|e| e.contains("BLYGGER_TEST_SPELLING=linux")));
    }

    // --- variables this build ignores ---

    fn run(names: &[&str], debug: bool) -> Vec<Ignored> {
        ignored(names.iter().map(|n| String::from(*n)), debug)
    }

    fn one(name: &str, why: Why) -> Vec<Ignored> {
        vec![Ignored {
            name: name.into(),
            why,
        }]
    }

    /// The real instance: `BLYGGER_TEST_SPELLING=windows` was read only by
    /// tests, so the app showed Mac wording and nobody was told. A name only
    /// the test suite reads is said in every build.
    #[test]
    fn a_name_only_tests_read_is_reported_in_every_build() {
        for debug in [true, false] {
            assert_eq!(
                run(&["BLYGGER_UPDATE_DOCS"], debug),
                one("BLYGGER_UPDATE_DOCS", Why::TestsOnly)
            );
        }
    }

    #[test]
    fn a_debug_only_name_is_reported_in_a_release_build_only() {
        assert_eq!(run(&["BLYGGER_UPDATE_URL"], true), vec![]);
        assert_eq!(
            run(&["BLYGGER_UPDATE_URL"], false),
            one("BLYGGER_UPDATE_URL", Why::DebugOnly)
        );
    }

    #[test]
    fn a_misspelt_name_is_unknown_and_says_what_it_may_mean() {
        for debug in [true, false] {
            assert_eq!(
                run(&["BLYGGER_FAKEE"], debug),
                one(
                    "BLYGGER_FAKEE",
                    Why::Unknown {
                        did_you_mean: Some("BLYGGER_FAKE")
                    }
                )
            );
        }
        assert_eq!(
            run(&["BLYGGER_NOTHING_LIKE_ANY_NAME"], true),
            one(
                "BLYGGER_NOTHING_LIKE_ANY_NAME",
                Why::Unknown { did_you_mean: None }
            )
        );
    }

    #[test]
    fn names_another_crate_or_a_script_reads_and_always_names_say_nothing() {
        for debug in [true, false] {
            assert_eq!(run(&["BLYGGER_CONFIG", "BLYGGER_BIN"], debug), vec![]);
            assert_eq!(run(&["BLYGGER_FAKE", "BLYGGER_DEMO"], debug), vec![]);
        }
    }

    #[test]
    fn only_blygger_names_are_looked_at() {
        let names = ["PATH", "BLYG_E2E_URL", "GPUI_DISABLE_DIRECT_COMPOSITION"];
        for debug in [true, false] {
            assert_eq!(run(&names, debug), vec![]);
        }
    }

    #[test]
    fn every_ignored_name_is_listed_once_in_name_order() {
        let got = run(&["BLYGGER_ZZ", "BLYGGER_AA", "BLYGGER_UPDATE_DOCS"], true);
        let names: Vec<&str> = got.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["BLYGGER_AA", "BLYGGER_UPDATE_DOCS", "BLYGGER_ZZ"]);
    }

    #[test]
    fn the_refusal_names_every_variable_why_and_what_to_do() {
        let list = run(&["BLYGGER_FAKEE", "BLYGGER_UPDATE_DOCS"], true);
        let text = refusal(&list);
        assert!(text.contains("BLYGGER_FAKEE"));
        assert!(text.contains("did you mean BLYGGER_FAKE"));
        assert!(text.contains("BLYGGER_UPDATE_DOCS: read only by the test suite"));
        assert!(text.contains("Unset"), "ends with what to do: {text}");
    }

    #[test]
    fn the_table_names_each_variable_once() {
        let mut seen = Vec::new();
        for &(name, _) in CLASSES {
            assert!(name.starts_with("BLYGGER_"), "{name}");
            assert!(!seen.contains(&name), "{name} is classed twice");
            seen.push(name);
        }
    }

    /// The names between quotes in `src` that look like `BLYGGER_*`
    /// variables.
    fn quoted_names(src: &str) -> Vec<&str> {
        src.split('"')
            .skip(1)
            .step_by(2)
            .filter(|s| {
                s.starts_with("BLYGGER_")
                    && s.chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            })
            .collect()
    }

    /// THE RULE: a new `BLYGGER_*` variable cannot be added without saying
    /// who reads it. Every name `never_rename_tests.rs` pins, and every one
    /// this file reads (above the table's marker comment, which the table
    /// and what follows it sit under), has a class.
    #[test]
    fn every_blygger_name_has_a_class() {
        let pinned = include_str!("never_rename_tests.rs")
            .split("const NAMES")
            .nth(1)
            .and_then(|rest| rest.split("];").next())
            .unwrap();
        let read_here = include_str!("dev_flags.rs")
            .split("\n// ---- classes of every name ----\n")
            .next()
            .unwrap();
        let mut unclassed: Vec<&str> = quoted_names(pinned)
            .into_iter()
            .chain(quoted_names(read_here))
            .filter(|n| !CLASSES.iter().any(|(c, _)| c == n))
            .collect();
        unclassed.sort_unstable();
        unclassed.dedup();
        assert!(
            unclassed.is_empty(),
            "add each to CLASSES in dev_flags.rs, with the class that says who reads it: {unclassed:?}"
        );
        assert!(!quoted_names(pinned).is_empty(), "found no pinned names");
        assert!(!quoted_names(read_here).is_empty(), "found no names read");
    }
}
