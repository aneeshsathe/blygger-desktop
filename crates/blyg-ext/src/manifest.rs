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
//! when = "always"                   # always | editor | reading | notes
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
//! ```
//!
//! Unknown keys are ignored (a newer manifest still loads); invalid values
//! are errors, reported per file.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::capability::Capability;
use crate::protocol::{CommandSpec, LibrarySpec, PROTOCOL_VERSION, SourceSpec};

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
