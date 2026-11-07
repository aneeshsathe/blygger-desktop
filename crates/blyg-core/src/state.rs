//! Small bits of app state that aren't configuration (things the app
//! remembers, not things the user sets): `state.json` in the data dir.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const STATE_FILE: &str = "state.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    /// The app has been launched (and shown its first-run flow) before.
    pub launched_before: bool,
    /// The "Sign in with ChatGPT is unofficial" notice has been shown.
    pub chatgpt_notice_shown: bool,
    /// The first-run onboarding was finished or skipped (Settings › Help
    /// can show it again).
    pub onboarded: bool,
    /// The app version last launched, once onboarded: a newer one opens the
    /// tutorial on what changed (`onboarding::whats_new`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen_version: Option<String>,
    /// The main window's last view mode (`write`, `split`, `studio`,
    /// `focus`): the full editor's ⌘1/⌘2/⌘3/⌘E, remembered per viewer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    /// When an update check last found nothing newer (Unix seconds), so a
    /// relaunch doesn't ask GitHub again within a day (`auto-update`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_update_check: Option<u64>,
    /// --- browser --- Hosts where the browser pane's shield is off
    /// (content blocking skipped), sorted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub browser_unblocked_hosts: Vec<String>,
    /// The reading screen's last mode (`stream` or `reader`, ⌥⌘1 / ⌥⌘2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_mode: Option<String>,
    // --- reader folders ---
    /// The Reader's sources pane was hidden (⌥⌘S).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reader_sources_hidden: bool,
    /// The sources pane's width in points, when the user dragged it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reader_sources_width: Option<u32>,
    /// --- notes --- The scratch note the notes drawer writes to (a local id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_note: Option<String>,
    /// --- extensions --- The notes drawer's chosen markdown-notes vault
    /// (a library id: `notes`, `notes.<label>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_vault: Option<String>,
    /// --- reading compose --- What's typed in the reading screen's compose
    /// box and not yet posted or saved (kept across views and restarts).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compose_draft: Option<String>,
    /// --- nearby --- The Nearby thoughts rail was hidden (⌥⌘U).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub nearby_hidden: bool,
    /// --- blyg switcher --- What each signed-in blyg looked like when it
    /// was last active (its name and avatar), by host, so the switcher can
    /// show blygs whose database isn't open.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub blyg_cards: std::collections::BTreeMap<String, BlygCard>,
    // --- flow ---
    /// The command palette's (⇧⌘P) last commands, newest first (ids such
    /// as `action:Publish`, `theme:cutaway`), shown when its field is empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub palette_recent: Vec<String>,
    /// Serendipity: the posts ⌘N's empty editor has shown, with when (Unix
    /// seconds), so one isn't shown again within a month.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub serendipity_seen: std::collections::BTreeMap<String, u64>,
    // --- end flow ---
    // --- tangles ---
    /// --- tray --- Posts picked up to quote and not placed yet, per blyg
    /// (by host), in the order they were picked up.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub quote_tray: std::collections::BTreeMap<String, Vec<TrayEntry>>,
    /// --- split --- The left post's share of the editor column in split
    /// view, in thousandths, when the user dragged the divider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_left: Option<u32>,
}

/// --- tray --- A post waiting in the quote tray: what the picker writes
/// for it (`![[id | label]]`), its title, and where it's from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrayEntry {
    pub id: String,
    pub label: String,
    pub title: String,
    /// The reading row's subscription (`~own` for yours).
    pub subscription_id: String,
}

/// --- blyg switcher --- A blyg as the switcher shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlygCard {
    /// The author's name, or the site title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The avatar's absolute URL (drawn from the image cache when it's there).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

impl AppState {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join(STATE_FILE)
    }

    /// Missing or unreadable → defaults (state is never worth a crash).
    pub fn load(data_dir: &Path) -> AppState {
        std::fs::read_to_string(Self::path(data_dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        let s = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        crate::config::edit::atomic_write(&Self::path(data_dir), &s)
    }

    /// Load, change, save.
    pub fn update(data_dir: &Path, f: impl FnOnce(&mut AppState)) -> std::io::Result<AppState> {
        let mut s = Self::load(data_dir);
        f(&mut s);
        s.save(data_dir)?;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(AppState::load(dir.path()), AppState::default());
        AppState::update(dir.path(), |s| s.launched_before = true).unwrap();
        assert!(AppState::load(dir.path()).launched_before);
        // --- blyg switcher ---
        AppState::update(dir.path(), |s| {
            s.blyg_cards.insert(
                "notes.example.org".into(),
                BlygCard {
                    name: Some("Field notes".into()),
                    avatar: None,
                },
            );
        })
        .unwrap();
        let back = AppState::load(dir.path());
        assert_eq!(
            back.blyg_cards["notes.example.org"].name.as_deref(),
            Some("Field notes")
        );
        std::fs::write(AppState::path(dir.path()), "{not json").unwrap();
        assert_eq!(AppState::load(dir.path()), AppState::default());
    }
}
