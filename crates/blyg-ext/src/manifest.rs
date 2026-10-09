//! `extension.toml`: everything static about an extension, so the host can
//! show consent, the palette and the notes panel without starting it.
//!
//! ```toml
//! name = "hello"                    # kebab-case; the key used in the config
//! version = "0.1.0"
//! protocol = 1
//! description = "Says hello."
//! command = ["python3", "main.py"]  # run from this folder
//! capabilities = ["ui"]
//!
//! [[commands]]
//! id = "hello"
//! title = "Say hello"
//! when = "always"                   # always | editor | reading | notes | published
//!
//! [[libraries]]
//! id = "notes"
//! title = "Notes"
//! writable = true
//!
//! [[settings]]
//! key = "greeting"
//! kind = "text"                     # text | path | bool | number
//! docs = "What to say."
//!
//! [[sites]]                         # needs "browser.automate:<origin>"
//! id = "social"
//! title = "Social"
//! origin = "https://social.example.com"
//! home = "https://social.example.com/notes"
//!
//! [[macros]]                        # see crate::recipe for the steps
//! id = "cross-post"
//! title = "Cross-post to Social…"
//! site = "social"
//! steps = [ … ]
//! ```
//!
//! Unknown keys are ignored (a newer manifest still loads); invalid values
//! are errors, reported per file. A site whose origin isn't declared as a
//! `browser.automate:` capability, or a macro that breaks a recipe rule
//! ([`crate::recipe::check_macro`]), makes the whole manifest invalid.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::capability::Capability;
use crate::protocol::{
    CommandSpec, LibrarySpec, MacroSpec, PROTOCOL_VERSION, SiteSpec, SourceSpec,
};
use crate::recipe::{check_macro, check_site};

pub const MANIFEST_FILE: &str = "extension.toml";

/// The longest extension name.
pub const NAME_MAX: usize = 40;

/// A setting the extension reads (`extension-setting = <name> key=value`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SettingSpec {
    pub key: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub docs: String,
    #[serde(default)]
    pub default: Option<String>,
}

fn default_kind() -> String {
    "text".into()
}

#[derive(Debug, Deserialize)]
struct Raw {
    name: String,
    version: String,
    protocol: u32,
    #[serde(default)]
    description: String,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    commands: Vec<CommandSpec>,
    #[serde(default)]
    sources: Vec<SourceSpec>,
    #[serde(default)]
    libraries: Vec<LibrarySpec>,
    #[serde(default)]
    settings: Vec<SettingSpec>,
    #[serde(default)]
    sites: Vec<SiteSpec>,
    #[serde(default)]
    macros: Vec<MacroSpec>,
}

/// A validated manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub protocol: u32,
    pub description: String,
    /// argv. Empty for a bundled extension (the host knows how to run it).
    pub command: Vec<String>,
    /// What it asks for, in manifest order, without duplicates.
    pub capabilities: Vec<Capability>,
    pub commands: Vec<CommandSpec>,
    pub sources: Vec<SourceSpec>,
    pub libraries: Vec<LibrarySpec>,
    pub settings: Vec<SettingSpec>,
    /// Websites its macros run on, origins normalised; each one's origin
    /// is declared as `browser.automate:<origin>`.
    pub sites: Vec<SiteSpec>,
    /// Browser macros, checked ([`crate::recipe::check_macro`]).
    pub macros: Vec<MacroSpec>,
}

impl Manifest {
    /// The site a macro runs on.
    pub fn site(&self, id: &str) -> Option<&SiteSpec> {
        self.sites.iter().find(|s| s.id == id)
    }

    /// Each macro with its site, in manifest order.
    pub fn macros_with_sites(&self) -> impl Iterator<Item = (&MacroSpec, &SiteSpec)> {
        self.macros
            .iter()
            .filter_map(|m| self.site(&m.site).map(|s| (m, s)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError(pub String);

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ManifestError {}

fn err<T>(m: impl Into<String>) -> Result<T, ManifestError> {
    Err(ManifestError(m.into()))
}

/// `[a-z0-9]+(-[a-z0-9]+)*`, at most [`NAME_MAX`].
pub fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= NAME_MAX
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A command, source, library or setting id: `[A-Za-z0-9._-]+`.
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

impl Manifest {
    /// Parse and validate. `bundled` manifests may omit `command`.
    pub fn parse(text: &str, bundled: bool) -> Result<Manifest, ManifestError> {
        let raw: Raw = toml::from_str(text).map_err(|e| ManifestError(e.to_string()))?;
        if !valid_name(&raw.name) {
            return err(format!(
                "name {:?} must be lower-case letters, digits and single dashes",
                raw.name
            ));
        }
        if raw.version.trim().is_empty() {
            return err("version is empty");
        }
        if raw.protocol == 0 {
            return err("protocol must be 1");
        }
        if raw.protocol > PROTOCOL_VERSION {
            return err(format!(
                "needs a newer Burrow (protocol {}; this one speaks {PROTOCOL_VERSION})",
                raw.protocol
            ));
        }
        if !bundled && raw.command.first().is_none_or(|c| c.trim().is_empty()) {
            return err("command is empty");
        }
        let mut capabilities: Vec<Capability> = vec![];
        for c in &raw.capabilities {
            let Some(cap) = Capability::parse(c) else {
                return err(format!("unknown capability {c:?}"));
            };
            if !capabilities.iter().any(|x| cap.covered_by(x)) {
                capabilities.push(cap);
            }
        }
        unique_ids("command", raw.commands.iter().map(|c| c.id.as_str()))?;
        unique_ids("source", raw.sources.iter().map(|c| c.id.as_str()))?;
        unique_ids("library", raw.libraries.iter().map(|c| c.id.as_str()))?;
        unique_ids("setting", raw.settings.iter().map(|c| c.key.as_str()))?;
        for c in &raw.commands {
            if c.title.trim().is_empty() {
                return err(format!("command {:?} has no title", c.id));
            }
        }
        for s in &raw.settings {
            if !matches!(s.kind.as_str(), "text" | "path" | "bool" | "number") {
                return err(format!(
                    "setting {:?}: kind must be text, path, bool or number",
                    s.key
                ));
            }
        }
        unique_ids("site", raw.sites.iter().map(|s| s.id.as_str()))?;
        unique_ids("macro", raw.macros.iter().map(|m| m.id.as_str()))?;
        let mut sites = raw.sites;
        for s in &mut sites {
            s.origin = check_site(s).map_err(|e| ManifestError(e.to_string()))?;
            let cap = Capability::BrowserAutomate(s.origin.clone());
            if !capabilities.iter().any(|c| cap.covered_by(c)) {
                return err(format!(
                    "site {:?}: its origin must be declared in capabilities as \"{cap}\"",
                    s.id
                ));
            }
        }
        for m in &raw.macros {
            check_macro(m, &sites).map_err(|e| ManifestError(e.to_string()))?;
        }
        Ok(Manifest {
            name: raw.name,
            version: raw.version,
            protocol: raw.protocol,
            description: raw.description,
            command: raw.command,
            capabilities,
            commands: raw.commands,
            sources: raw.sources,
            libraries: raw.libraries,
            settings: raw.settings,
            sites,
            macros: raw.macros,
        })
    }
}

fn unique_ids<'a>(what: &str, ids: impl Iterator<Item = &'a str>) -> Result<(), ManifestError> {
    let mut seen = HashSet::new();
    for id in ids {
        if !valid_id(id) {
            return err(format!(
                "{what} id {id:?} must be letters, digits, '.', '_' or '-'"
            ));
        }
        if !seen.insert(id) {
            return err(format!("{what} id {id:?} appears twice"));
        }
    }
    Ok(())
}

/// Where an extension came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Built into Burrow; run as `program args…`.
    Bundled { program: PathBuf, args: Vec<String> },
    /// A folder in the extensions directory.
    Installed { dir: PathBuf },
}

/// An extension Burrow knows about (enabled or not).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub manifest: Manifest,
    pub origin: Origin,
}

/// A problem found while discovering extensions (shown by
/// `+list-extensions` and `+validate-config`; never fatal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub message: String,
}

/// The bundled extensions plus every `<dir>/<folder>/extension.toml`. A
/// broken manifest, a folder whose manifest is missing, or a name that is
/// already taken (on-disk never shadows bundled; the first folder in name
/// order wins otherwise) is a diagnostic and is skipped.
pub fn discover(bundled: &[Installed], dir: Option<&Path>) -> (Vec<Installed>, Vec<Diagnostic>) {
    let mut out: Vec<Installed> = bundled.to_vec();
    let mut diags = vec![];
    let Some(dir) = dir else {
        return (out, diags);
    };
    let Ok(rd) = std::fs::read_dir(dir) else {
        return (out, diags);
    };
    let mut folders: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.starts_with('.'))
        })
        .collect();
    folders.sort();
    for folder in folders {
        let path = folder.join(MANIFEST_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => {
                diags.push(Diagnostic {
                    path,
                    message: format!("no {MANIFEST_FILE}"),
                });
                continue;
            }
        };
        match Manifest::parse(&text, false) {
            Ok(m) => {
                if let Some(other) = out.iter().find(|i| i.manifest.name == m.name) {
                    let whose = match other.origin {
                        Origin::Bundled { .. } => "a bundled extension".to_string(),
                        Origin::Installed { ref dir } => dir.display().to_string(),
                    };
                    diags.push(Diagnostic {
                        path,
                        message: format!("the name {:?} is already taken by {whose}", m.name),
                    });
                    continue;
                }
                out.push(Installed {
                    manifest: m,
                    origin: Origin::Installed { dir: folder },
                });
            }
            Err(e) => diags.push(Diagnostic { path, message: e.0 }),
        }
    }
    (out, diags)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
name = "hello-world"
version = "0.1.0"
protocol = 1
description = "Says hello."
command = ["python3", "main.py"]
capabilities = ["ui", "fs:~/Notes", "ui", "fs:~\\Notes"]
future_key = { nested = true }

[[commands]]
id = "hello"
title = "Say hello"
when = "editor"
someday = 1

[[libraries]]
id = "notes"
title = "Notes"
writable = true

[[settings]]
key = "vault"
kind = "path"
"#;

    #[test]
    fn parses_a_good_manifest_and_ignores_unknown_keys() {
        let m = Manifest::parse(GOOD, false).unwrap();
        assert_eq!(m.name, "hello-world");
        assert_eq!(
            m.capabilities,
            vec![Capability::Ui, Capability::Fs("~/Notes".into())],
            "duplicates (including fs: spelled with \\) collapse"
        );
        assert_eq!(m.commands[0].when, crate::protocol::When::Editor);
        assert!(m.libraries[0].writable);
        assert_eq!(m.settings[0].kind, "path");
    }

    #[test]
    fn rejects_bad_names_and_values() {
        for (from, to) in [
            ("name = \"hello-world\"", "name = \"Hello\""),
            ("name = \"hello-world\"", "name = \"-x\""),
            ("name = \"hello-world\"", "name = \"a--b\""),
            ("name = \"hello-world\"", "name = \"a b\""),
            ("protocol = 1", "protocol = 2"),
            ("protocol = 1", "protocol = 0"),
            ("\"ui\", \"fs", "\"items.publish\", \"fs"),
            ("command = [\"python3\", \"main.py\"]", "command = []"),
            ("when = \"editor\"", "when = \"sometimes\""),
            ("kind = \"path\"", "kind = \"colour\""),
            ("id = \"hello\"", "id = \"he llo\""),
        ] {
            let text = GOOD.replace(from, to);
            assert_ne!(text, GOOD, "{from}");
            assert!(Manifest::parse(&text, false).is_err(), "accepted {to}");
        }
        let twice = format!("{GOOD}\n[[commands]]\nid = \"hello\"\ntitle = \"Again\"\n");
        assert!(
            Manifest::parse(&twice, false)
                .unwrap_err()
                .0
                .contains("twice")
        );
        assert!(
            Manifest::parse(&GOOD.replace("protocol = 1", "protocol = 2"), false)
                .unwrap_err()
                .0
                .contains("newer Burrow")
        );
    }

    const BROWSER: &str = r#"
name = "cross-post"
version = "0.1.0"
protocol = 1
command = ["cross-post"]
capabilities = ["items.read", "browser.automate:HTTPS://Social.Example.com/", "browser.capture"]

[[sites]]
id = "social"
title = "Social Notes"
origin = "https://social.example.com:443"
home = "https://social.example.com/notes"
signed-out = "a[href*='sign-in']"
content-blocking = false
min-interval = "60s"
someday = "ignored"

[[macros]]
id = "cross-post-note"
title = "Cross-post to Social Notes…"
site = "social"
when = "published"
template = "{{excerpt}}\r\n\r\n{{permalink}}"
tested = "unverified"
steps = [
  { do = "open", url = "https://social.example.com/notes" },
  { do = "waitFor", selector = "div.box[contenteditable='true']", timeout = "20s" },
  { do = "focus",  selector = "div.box[contenteditable='true']" },
  { do = "insert", selector = "div.box[contenteditable='true']" },
  { do = "submit", selector = "button", text = "Post" },
  { do = "waitFor", selector = "div.box[contenteditable='true']", empty = true, timeout = "15s" },
  { do = "done", text = "Posted to Social Notes" },
]
"#;

    #[test]
    fn sites_and_macros_load_against_declared_origins() {
        let m = Manifest::parse(BROWSER, false).unwrap();
        assert_eq!(
            m.capabilities[1],
            Capability::BrowserAutomate("https://social.example.com".into())
        );
        assert_eq!(m.sites.len(), 1);
        let s = &m.sites[0];
        assert_eq!(s.origin, "https://social.example.com", "normalised");
        assert!(!s.content_blocking);
        assert_eq!(s.min_interval(), std::time::Duration::from_secs(60));
        assert_eq!(s.signed_out.as_deref(), Some("a[href*='sign-in']"));
        let (mac, site) = m.macros_with_sites().next().unwrap();
        assert_eq!(site.id, "social");
        assert_eq!(mac.when, crate::protocol::When::Published);
        assert_eq!(mac.tested, "unverified");
        assert_eq!(mac.steps.len(), 7);
        assert_eq!(
            crate::recipe::expand(
                &mac.template,
                &crate::recipe::Vars {
                    title: String::new(),
                    excerpt: "Hi".into(),
                    permalink: "https://blyg.example.com/p/1".into(),
                }
            ),
            "Hi\n\nhttps://blyg.example.com/p/1"
        );
        // Defaults: content blocking on, when = published, tested = unverified.
        let bare = BROWSER
            .replace("content-blocking = false\n", "")
            .replace("when = \"published\"\n", "")
            .replace("tested = \"unverified\"\n", "")
            .replace("template = \"{{excerpt}}\\r\\n\\r\\n{{permalink}}\"\n", "");
        let m = Manifest::parse(&bare, false).unwrap();
        assert!(m.sites[0].content_blocking);
        assert_eq!(m.macros[0].when, crate::protocol::When::Published);
        assert_eq!(m.macros[0].tested, "unverified");
        assert_eq!(m.macros[0].template, crate::recipe::DEFAULT_TEMPLATE);
        // The camelCase spellings work too.
        let camel = BROWSER
            .replace("signed-out", "signedOut")
            .replace("content-blocking", "contentBlocking")
            .replace("min-interval", "minInterval");
        let m = Manifest::parse(&camel, false).unwrap();
        assert!(!m.sites[0].content_blocking && m.sites[0].signed_out.is_some());
    }

    #[test]
    fn browser_rules_are_manifest_errors() {
        let bad = |from: &str, to: &str| {
            let text = BROWSER.replace(from, to);
            assert_ne!(text, BROWSER, "{from}");
            Manifest::parse(&text, false).unwrap_err().0
        };
        let e = bad("\"browser.automate:HTTPS://Social.Example.com/\", ", "");
        assert!(
            e.contains("must be declared in capabilities as \"browser.automate:https://social.example.com\""),
            "{e}"
        );
        let e = bad(
            "browser.automate:HTTPS://Social.Example.com/",
            "browser.automate:https://www.social.example.com",
        );
        assert!(e.contains("must be declared"), "{e}");
        let e = bad(
            "browser.automate:HTTPS://Social.Example.com/",
            "browser.automate:https://social.example.com/notes",
        );
        assert!(e.contains("unknown capability"), "{e}");
        let e = bad("site = \"social\"", "site = \"elsewhere\"");
        assert!(e.contains("isn't one of this extension's [[sites]]"), "{e}");
        let e = bad(
            "home = \"https://social.example.com/notes\"",
            "home = \"https://evil.example/\"",
        );
        assert!(e.starts_with("site \"social\": home"), "{e}");
        let e = bad("min-interval = \"60s\"", "min-interval = \"soon\"");
        assert!(e.contains("min-interval"), "{e}");
        let e = bad("timeout = \"20s\"", "timeout = \"45s\"");
        assert!(
            e.contains("step 2 (waitFor): timeout \"45s\" is over the 30s limit"),
            "{e}"
        );
        let e = bad(
            "{ do = \"open\", url = \"https://social.example.com/notes\" }",
            "{ do = \"open\", url = \"https://elsewhere.example/notes\" }",
        );
        assert!(e.contains("step 1 (open)"), "{e}");
        let e = bad("{ do = \"done\", text = \"Posted to Social Notes\" },", "");
        assert!(e.contains("the last step must be done"), "{e}");
        let e = bad("{ do = \"submit\"", "{ do = \"click\"");
        assert!(e.contains("click after insert"), "{e}");
        let e = bad("do = \"focus\"", "do = \"eval\"");
        assert!(
            e.contains("eval"),
            "unknown step kinds are TOML errors: {e}"
        );
        let e = bad("when = \"published\"", "when = \"later\"");
        assert!(!e.is_empty());
        let twice = format!(
            "{BROWSER}\n[[sites]]\nid = \"social\"\ntitle = \"x\"\norigin = \"https://social.example.com\"\nhome = \"https://social.example.com/\"\n"
        );
        assert!(
            Manifest::parse(&twice, false)
                .unwrap_err()
                .0
                .contains("site id \"social\" appears twice")
        );
    }

    #[test]
    fn a_manifest_without_browser_tables_has_none() {
        let m = Manifest::parse(GOOD, false).unwrap();
        assert!(m.sites.is_empty() && m.macros.is_empty());
        assert_eq!(m.macros_with_sites().count(), 0);
    }

    #[test]
    fn bundled_manifests_need_no_command() {
        let text = GOOD.replace("command = [\"python3\", \"main.py\"]", "");
        assert!(Manifest::parse(&text, false).is_err());
        assert!(Manifest::parse(&text, true).is_ok());
    }

    #[test]
    fn discovery_skips_broken_and_shadowing_folders() {
        let dir = tempfile::tempdir().unwrap();
        let mk = |folder: &str, text: &str| {
            let d = dir.path().join(folder);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(MANIFEST_FILE), text).unwrap();
        };
        mk("a-good", GOOD);
        mk("b-broken", "name = ");
        mk("c-dup", GOOD);
        mk("d-shadow", &GOOD.replace("hello-world", "markdown-notes"));
        std::fs::create_dir_all(dir.path().join("e-empty")).unwrap();
        std::fs::create_dir_all(dir.path().join(".hidden")).unwrap();
        let bundled = Installed {
            manifest: Manifest::parse(
                &GOOD
                    .replace("hello-world", "markdown-notes")
                    .replace("command = [\"python3\", \"main.py\"]", ""),
                true,
            )
            .unwrap(),
            origin: Origin::Bundled {
                program: PathBuf::from("blygger"),
                args: vec!["+ext".into(), "markdown-notes".into()],
            },
        };
        let (found, diags) = discover(&[bundled], Some(dir.path()));
        let names: Vec<&str> = found.iter().map(|i| i.manifest.name.as_str()).collect();
        assert_eq!(names, ["markdown-notes", "hello-world"]);
        assert_eq!(diags.len(), 4, "{diags:?}");
        assert!(diags.iter().any(|d| d.message.contains("bundled")));
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("no extension.toml"))
        );
    }
}
