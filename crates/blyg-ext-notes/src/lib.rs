//! markdown-notes: Burrow's bundled extension for a folder of Markdown notes
//! (an Obsidian vault). A notes reader and writer of its own, separate from
//! the blyg: it lists, searches, reads, creates and edits notes, and never
//! needs a signed-in blyg, an item id or any `items.*` capability. Burrow's
//! notes panel shows the notes and copies text from them into a post (a
//! host action); "Save selection to notes" makes a new plain note.
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
/// The library's id.
pub const LIBRARY: &str = "notes";
/// The palette command that saves the selection as a new note.
pub const SAVE_SELECTION: &str = "save-selection";
/// The vault when `extension-setting = markdown-notes vault=…` isn't set.
pub const DEFAULT_VAULT: &str = "~/Notes";

const MANIFEST: &str = r#"
name = "markdown-notes"
version = "0.1.0"
protocol = 1
description = "Reads and writes a folder of Markdown notes (an Obsidian vault). Works without a blyg."
capabilities = ["fs:@VAULT@", "ui"]

[[commands]]
id = "save-selection"
title = "Save selection to notes"
detail = "A new note in your notes folder, with the selected text"
when = "always"

[[libraries]]
id = "notes"
title = "Notes"
writable = true

[[settings]]
key = "vault"
kind = "path"
docs = "The folder of Markdown notes (default ~/Notes)."

[[settings]]
key = "folder"
kind = "text"
docs = "The folder inside it where new notes go (default: the top)."

[[settings]]
key = "poll-ms"
kind = "number"
docs = "How often to look for changed notes, in milliseconds (default 2000)."
"#;

/// The vault folder as configured (unexpanded), or [`DEFAULT_VAULT`].
pub fn vault_setting(settings: &BTreeMap<String, String>) -> String {
    settings
        .get("vault")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .unwrap_or(DEFAULT_VAULT)
        .to_string()
}

/// The manifest, asking for `fs:<vault>` so the consent sheet names the
/// actual folder.
pub fn manifest(vault: &str) -> Manifest {
    Manifest::parse(
        &MANIFEST.replace("@VAULT@", &vault.replace('\\', "\\\\").replace('"', "\\\"")),
        true,
    )
    .expect("the bundled manifest is valid")
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
        manifest: manifest(&vault_setting(settings)),
        origin: Origin::Bundled { program, args },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blyg_ext::Capability;

    #[test]
    fn the_manifest_asks_only_for_its_folder_and_ui() {
        let m = manifest("~/Notes");
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
        let w = manifest("C:\\Users\\someone\\Notes \"x\"");
        assert_eq!(
            w.capabilities[0],
            Capability::Fs("C:\\Users\\someone\\Notes \"x\"".into())
        );
        assert_eq!(m.commands[0].id, SAVE_SELECTION);
        assert_eq!(m.libraries[0].id, LIBRARY);
    }

    #[test]
    fn vault_setting_defaults() {
        assert_eq!(vault_setting(&BTreeMap::new()), DEFAULT_VAULT);
        let s: BTreeMap<String, String> = [("vault".to_string(), " ~/Vault ".to_string())].into();
        assert_eq!(vault_setting(&s), "~/Vault");
    }
}
