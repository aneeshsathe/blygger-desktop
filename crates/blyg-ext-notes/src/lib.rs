//! markdown-notes: Burrow's bundled extension for folders of Markdown notes
//! (Obsidian vaults). A notes reader and writer of its own, separate from
//! the blyg: it lists, searches, reads, creates and edits notes, and never
//! needs a signed-in blyg, an item id or any `items.*` capability. Burrow's
//! notes panel shows the notes and copies text from them into a post (a
//! host action); "Save selection to notes" makes a new plain note.
//!
//! Any number of folders ("vaults"), each its own library:
//!
//! ```text
//! extension-setting = markdown-notes vault=~/Notes
//! extension-setting = markdown-notes vault-work=~/Work/Vault
//! ```
//!
//! `vault` is the first (library `notes`); each `vault-<label>` adds one
//! more (library `notes.<label>`). Setting keys are lowercase kebab-case
//! (the config checks that), so the label is too, and since each key is
//! its own, the config's "a later line for the same key wins" never drops
//! a vault. A vault is titled by its folder's name, or by its label when
//! the label isn't just that name. With no vault, the extension asks only
//! for `ui` and has no library: Burrow offers to choose a folder.
//!
//! - [`vault`]: the folder logic, usable without Burrow.
//! - [`frontmatter`]: the bit of YAML frontmatter it reads (never writes).
//! - [`run`]: the BXP extension around it (`blygger +ext markdown-notes`).

pub mod frontmatter;
pub mod run;
pub mod vault;

use std::collections::BTreeMap;
use std::path::PathBuf;

use blyg_ext::manifest::{Installed, Manifest, Origin};

pub use run::{NotesExt, run, run_stdio};

/// The extension's name (`extension = markdown-notes`).
pub const NAME: &str = "markdown-notes";
/// The first vault's library id (`vault=…`); `vault-<label>` is
/// `notes.<label>`.
pub const LIBRARY: &str = "notes";
/// The palette command that saves the selection as a new note.
pub const SAVE_SELECTION: &str = "save-selection";
/// The first vault's setting key; more are `vault-<label>`.
pub const VAULT_KEY: &str = "vault";
/// The longest `<label>` in `vault-<label>`.
pub const LABEL_MAX: usize = 40;

const MANIFEST: &str = r#"
name = "markdown-notes"
version = "0.1.0"
protocol = 1
description = "Reads and writes folders of Markdown notes (Obsidian vaults). Works without a blyg."
capabilities = [@CAPS@]

[[commands]]
id = "save-selection"
title = "Save selection to notes"
detail = "A new note in your first notes folder, with the selected text"
when = "always"

[[settings]]
key = "vault"
kind = "path"
docs = "A folder of Markdown notes. Add more as vault-<label>=<folder>."

[[settings]]
key = "folder"
kind = "text"
docs = "The folder inside each vault where new notes go (default: the top)."

[[settings]]
key = "poll-ms"
kind = "number"
docs = "How often to look for changed notes, in milliseconds (default 2000)."
"#;

/// One configured folder of notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultSpec {
    /// Its setting key: `vault` or `vault-<label>`.
    pub key: String,
    /// The folder as configured (unexpanded, `~/…`).
    pub path: String,
    /// Its library id: `notes` or `notes.<label>`.
    pub library: String,
    /// What the drawer calls it: the folder's name, or the label when
    /// it's more than that name (`vault-work` → "work").
    pub title: String,
}

/// A label usable in `vault-<label>`: lowercase letters and digits in
/// `-`-separated words (a config setting key's grammar).
pub fn valid_label(label: &str) -> bool {
    label.len() <= LABEL_MAX
        && !label.is_empty()
        && label.split('-').all(|w| {
            !w.is_empty()
                && w.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// The last part of a folder (`~/Work/Vault` → `Vault`), else "Notes".
pub fn folder_name(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|n| !n.is_empty() && *n != "~")
        .unwrap_or("Notes")
        .to_string()
}

/// A folder written two ways (`~/Notes/`, `~/Notes`) is one folder.
pub fn same_folder(a: &str, b: &str) -> bool {
    a.trim().trim_end_matches(['/', '\\']) == b.trim().trim_end_matches(['/', '\\'])
}

/// `s` as a label: lowercase, runs of anything but letters and digits
/// become one `-` (`My vault (2024)` → `my-vault-2024`).
fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let mut out: String = out
        .trim_end_matches('-')
        .chars()
        .take(LABEL_MAX - 4)
        .collect();
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// The configured vaults, in order: `vault` first, then each
/// `vault-<label>` by label. Empty values, bad labels and a folder already
/// listed are left out (with none, there's no library).
pub fn vaults(settings: &BTreeMap<String, String>) -> Vec<VaultSpec> {
    let mut out: Vec<VaultSpec> = vec![];
    let first = settings.get(VAULT_KEY).map(|v| (VAULT_KEY, None, v));
    let rest = settings.iter().filter_map(|(k, v)| {
        let label = k.strip_prefix(VAULT_KEY)?.strip_prefix('-')?;
        Some((k.as_str(), Some(label), v))
    });
    for (key, label, value) in first.into_iter().chain(rest) {
        let path = value.trim();
        if path.is_empty() || out.iter().any(|v| same_folder(&v.path, path)) {
            continue;
        }
        let name = folder_name(path);
        let (library, title) = match label {
            None => (LIBRARY.to_string(), name),
            Some(l) if valid_label(l) => {
                let title = if slug(&name) == l {
                    name
                } else {
                    l.replace('-', " ")
                };
                (format!("{LIBRARY}.{l}"), title)
            }
            Some(_) => continue,
        };
        out.push(VaultSpec {
            key: key.to_string(),
            path: path.to_string(),
            library,
            title,
        });
    }
    out
}

/// The setting key a new folder `path` gets: `vault` when there's no first
/// vault yet, else `vault-<label>` from the folder's name, made unique
/// (`~/Work/Notes` beside `~/Notes` is `vault-work-notes`). `None` when the
/// folder is already configured.
pub fn new_vault_key(settings: &BTreeMap<String, String>, path: &str) -> Option<String> {
    let have = vaults(settings);
    if have.iter().any(|v| same_folder(&v.path, path)) {
        return None;
    }
    if settings.get(VAULT_KEY).is_none_or(|v| v.trim().is_empty()) {
        return Some(VAULT_KEY.into());
    }
    let parts: Vec<&str> = path
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != "~")
        .collect();
    let mut base = slug(parts.first().copied().unwrap_or(""));
    if base.is_empty() {
        base = "notes".into();
    }
    // Taken: the key is used, or a vault already goes by that name.
    let taken = |l: &str| {
        settings.contains_key(&format!("{VAULT_KEY}-{l}"))
            || have.iter().any(|v| slug(&v.title) == l)
    };
    let mut label = base.clone();
    if taken(&label)
        && let Some(parent) = parts.get(1).map(|p| slug(p)).filter(|p| !p.is_empty())
    {
        label = slug(&format!("{parent}-{base}"));
    }
    let start = label.clone();
    let mut n = 2;
    while taken(&label) {
        label = format!("{start}-{n}");
        n += 1;
    }
    Some(format!("{VAULT_KEY}-{label}"))
}

/// A TOML basic string.
fn toml_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The manifest for these vaults: `fs:<folder>` for each (so the consent
/// sheet names each folder) and `ui`, a library per vault, and a
/// `vault-<label>` setting for each labelled one (so the config check
/// knows it). No vaults: just `ui`.
pub fn manifest(vaults: &[VaultSpec]) -> Manifest {
    let mut caps: Vec<String> = vaults
        .iter()
        .map(|v| toml_str(&format!("fs:{}", v.path)))
        .collect();
    caps.push(toml_str("ui"));
    let mut text = MANIFEST.replace("@CAPS@", &caps.join(", "));
    for v in vaults {
        text.push_str(&format!(
            "\n[[libraries]]\nid = {}\ntitle = {}\nwritable = true\n",
            toml_str(&v.library),
            toml_str(&v.title)
        ));
        if v.key != VAULT_KEY {
            text.push_str(&format!(
                "\n[[settings]]\nkey = {}\nkind = \"path\"\ndocs = \"Another folder of Markdown notes.\"\n",
                toml_str(&v.key)
            ));
        }
    }
    Manifest::parse(&text, true).expect("the bundled manifest is valid")
}

/// The bundled extension for the host's `HostConfig::bundled`: run as
/// `program args…` (the app passes its own executable and
/// `["+ext", "markdown-notes"]`); `settings` are this extension's
/// `extension-setting` values.
pub fn bundled(
    program: PathBuf,
    args: Vec<String>,
    settings: &BTreeMap<String, String>,
) -> Installed {
    Installed {
        manifest: manifest(&vaults(settings)),
        origin: Origin::Bundled { program, args },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blyg_ext::Capability;

    fn s(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn the_manifest_asks_only_for_its_folders_and_ui() {
        let m = manifest(&vaults(&s(&[("vault", "~/Notes")])));
        assert_eq!(m.name, NAME);
        assert_eq!(
            m.capabilities,
            vec![Capability::Fs("~/Notes".into()), Capability::Ui]
        );
        assert!(m.capabilities.iter().all(|c| !matches!(
            c,
            Capability::ItemsRead
                | Capability::ItemsWrite
                | Capability::ReadingRead
                | Capability::HookPublished
        )));
        let w = manifest(&vaults(&s(&[("vault", "C:\\Users\\someone\\Notes \"x\"")])));
        assert_eq!(
            w.capabilities[0],
            Capability::Fs("C:\\Users\\someone\\Notes \"x\"".into())
        );
        assert_eq!(m.commands[0].id, SAVE_SELECTION);
        assert_eq!(m.libraries[0].id, LIBRARY);
        assert_eq!(m.libraries[0].title, "Notes");
    }

    #[test]
    fn no_folder_asks_only_for_ui_and_has_no_library() {
        for settings in [s(&[]), s(&[("vault", "  ")]), s(&[("folder", "Inbox")])] {
            let m = manifest(&vaults(&settings));
            assert_eq!(m.capabilities, vec![Capability::Ui], "{settings:?}");
            assert!(m.libraries.is_empty());
            assert_eq!(m.commands[0].id, SAVE_SELECTION);
        }
    }

    #[test]
    fn several_vaults_are_several_libraries_and_grants() {
        let settings = s(&[
            ("vault-work", "~/Work/Vault"),
            ("vault", "~/Notes"),
            ("vault-garden-notes", "~/Garden notes/"),
            ("folder", "Inbox"),
        ]);
        let v = vaults(&settings);
        let keys: Vec<&str> = v.iter().map(|v| v.key.as_str()).collect();
        assert_eq!(keys, ["vault", "vault-garden-notes", "vault-work"]);
        let libs: Vec<&str> = v.iter().map(|v| v.library.as_str()).collect();
        assert_eq!(libs, ["notes", "notes.garden-notes", "notes.work"]);
        let titles: Vec<&str> = v.iter().map(|v| v.title.as_str()).collect();
        assert_eq!(titles, ["Notes", "Garden notes", "work"]);
        let m = manifest(&v);
        assert_eq!(
            m.capabilities,
            vec![
                Capability::Fs("~/Notes".into()),
                Capability::Fs("~/Garden notes/".into()),
                Capability::Fs("~/Work/Vault".into()),
                Capability::Ui
            ]
        );
        assert_eq!(m.libraries.len(), 3);
        assert!(m.libraries.iter().all(|l| l.writable));
        // Each labelled vault is a declared setting (so the config check
        // doesn't call it unknown).
        let keys: Vec<&str> = m.settings.iter().map(|s| s.key.as_str()).collect();
        assert!(keys.contains(&"vault-garden-notes") && keys.contains(&"vault-work"));
    }

    #[test]
    fn labelled_vaults_work_without_a_first_one() {
        let v = vaults(&s(&[("vault-work", "~/Work/Vault")]));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].library, "notes.work");
        assert_eq!(manifest(&v).libraries[0].id, "notes.work");
    }

    #[test]
    fn bad_labels_empty_folders_and_repeats_are_left_out() {
        let v = vaults(&s(&[
            ("vault", "~/Notes"),
            ("vault-", "~/A"),
            ("vault-Upper", "~/B"),
            ("vault-again", "~/Notes/"),
            ("vault-empty", ""),
            ("vaults", "~/C"),
            ("vault-ok", "~/D"),
        ]));
        let keys: Vec<&str> = v.iter().map(|v| v.key.as_str()).collect();
        assert_eq!(keys, ["vault", "vault-ok"]);
    }

    #[test]
    fn titles_come_from_the_folder_name() {
        assert_eq!(folder_name("~/Work/Vault"), "Vault");
        assert_eq!(folder_name("~/Work/Vault/"), "Vault");
        assert_eq!(folder_name("C:\\Users\\someone\\Brain"), "Brain");
        assert_eq!(folder_name("~"), "Notes");
        assert_eq!(folder_name("/"), "Notes");
        assert_eq!(slug("My vault (2024)"), "my-vault-2024");
        assert_eq!(slug("  Ünïcode  "), "n-code");
    }

    #[test]
    fn a_new_folder_gets_a_free_key() {
        let none = s(&[]);
        assert_eq!(new_vault_key(&none, "~/Notes").as_deref(), Some("vault"));
        let one = s(&[("vault", "~/Notes")]);
        assert_eq!(new_vault_key(&one, "~/Notes/"), None, "already there");
        assert_eq!(
            new_vault_key(&one, "~/Work/Vault").as_deref(),
            Some("vault-vault")
        );
        assert_eq!(
            new_vault_key(&one, "~/Work/My vault (2024)").as_deref(),
            Some("vault-my-vault-2024")
        );
        // Named like the first one: its parent tells them apart.
        assert_eq!(
            new_vault_key(&one, "~/Work/Notes").as_deref(),
            Some("vault-work-notes")
        );
        let two = s(&[("vault", "~/Notes"), ("vault-work-notes", "~/Work/Notes")]);
        assert_eq!(
            new_vault_key(&two, "~/Other/Work/Notes").as_deref(),
            Some("vault-work-notes-2")
        );
        // Only labelled ones left: the next one is the first again.
        let labelled = s(&[("vault-work", "~/Work/Vault")]);
        assert_eq!(
            new_vault_key(&labelled, "~/Notes").as_deref(),
            Some("vault")
        );
        // Every key it makes is a vault, titled by its folder's name when
        // it can be.
        for (settings, path, title) in [
            (&one, "~/Work/Vault", "Vault"),
            (&one, "~/Work/My vault (2024)", "My vault (2024)"),
            (&two, "~/x/Work/Notes", "work notes 2"),
        ] {
            let key = new_vault_key(settings, path).unwrap();
            let mut with = settings.clone();
            with.insert(key.clone(), path.into());
            let v = vaults(&with);
            let got = v.iter().find(|v| v.key == key).expect("a vault");
            assert_eq!(got.title, title, "{key}");
        }
    }
}
