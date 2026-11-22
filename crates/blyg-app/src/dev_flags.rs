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
    /// `BLYGGER_TEST_SPELLING=windows|mac`: the platform whose wording every
    /// test thread starts with (`platform_text::spelled_as` still picks one
    /// for a test). Unset: this machine's.
    pub spelling: Option<crate::platform_text::Platform>,
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
            spelling: get("BLYGGER_TEST_SPELLING").map(|v| {
                use crate::platform_text::Platform;
                match v.to_ascii_lowercase().as_str() {
                    "windows" => Platform::Windows,
                    "mac" | "macos" => Platform::Mac,
                    other => panic!("BLYGGER_TEST_SPELLING={other}: say windows or mac"),
                }
            }),
            update_docs: std::env::var_os("BLYGGER_UPDATE_DOCS").is_some(),
        }
    }
}

static FLAGS: OnceLock<DevFlags> = OnceLock::new();

/// Parse the environment (first thing in `main`). An error names the
/// variable and what's wrong with it.
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
}
