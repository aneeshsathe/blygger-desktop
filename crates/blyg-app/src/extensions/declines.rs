//! --- extensions --- Consent answers kept across launches: what the owner
//! declined on an extension's consent sheet (unticked when allowing the
//! rest, or every capability on a sheet closed with Not now). A declined
//! capability isn't asked about again at launch. The record is dropped
//! when the extension's manifest asks for a different capability list (a
//! new version, a new notes folder), so anything new is asked about. Undo:
//! Manage extensions › Allow…, or the `extension-allow` line by hand;
//! turning the extension off forgets its declines too.
//!
//! Kept in the data folder (`extension-declines.json`), beside
//! `state.json`, never in the config file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use blyg_ext::Capability;
use serde::{Deserialize, Serialize};

const FILE: &str = "extension-declines.json";

/// One extension's declines, and the capability list they answered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Record {
    /// The manifest's whole capability list when declined, sorted.
    requested: Vec<String>,
    /// What was declined, sorted.
    declined: Vec<String>,
}

/// Every extension's declines, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Declines(BTreeMap<String, Record>);

fn strings(caps: &[Capability]) -> Vec<String> {
    let mut v: Vec<String> = caps.iter().map(|c| c.to_string()).collect();
    v.sort();
    v.dedup();
    v
}

impl Declines {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join(FILE)
    }

    /// Missing or unreadable: nothing declined (asking again is safe).
    pub fn load(data_dir: &Path) -> Declines {
        std::fs::read_to_string(Self::path(data_dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let s = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        blyg_core::config::edit::atomic_write(&Self::path(data_dir), &s)
    }

    /// Load, change, save.
    pub fn update(data_dir: &Path, f: impl FnOnce(&mut Declines)) -> std::io::Result<()> {
        let mut d = Self::load(data_dir);
        let before = d.clone();
        f(&mut d);
        if d == before {
            return Ok(());
        }
        d.save(data_dir)
    }

    /// `name`, whose manifest asks for `requested`, was refused `declined`.
    /// A record made for another capability list is replaced.
    pub fn record(&mut self, name: &str, requested: &[Capability], declined: &[Capability]) {
        if declined.is_empty() {
            return;
        }
        let requested = strings(requested);
        let r = self.0.entry(name.to_string()).or_default();
        if r.requested != requested {
            *r = Record {
                requested,
                declined: vec![],
            };
        }
        r.declined.extend(strings(declined));
        r.declined.sort();
        r.declined.dedup();
    }

    /// Forget everything `name` was refused (turned off, or forgotten).
    pub fn forget(&mut self, name: &str) {
        self.0.remove(name);
    }

    /// Of `missing`, what to ask `name` about: all of it when the manifest
    /// asks for another list than the one declined against, else what
    /// wasn't declined.
    pub fn to_ask(
        &self,
        name: &str,
        requested: &[Capability],
        missing: &[Capability],
    ) -> Vec<Capability> {
        match self.0.get(name) {
            Some(r) if r.requested == strings(requested) => missing
                .iter()
                .filter(|c| !r.declined.contains(&c.to_string()))
                .cloned()
                .collect(),
            _ => missing.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps() -> Vec<Capability> {
        vec![
            Capability::ItemsRead,
            Capability::Ui,
            Capability::BrowserAutomate("https://social.example.com".into()),
            Capability::BrowserAutomate("https://cast.example.com".into()),
        ]
    }

    #[test]
    fn a_decline_is_kept_across_loads_and_only_new_things_are_asked() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let all = caps();
        let missing = &all[2..];
        assert_eq!(Declines::load(&data).to_ask("x", &all, missing), missing);

        Declines::update(&data, |d| d.record("x", &all, &missing[..1])).unwrap();
        // A later launch: the declined one isn't asked about, the other is.
        let d = Declines::load(&data);
        assert_eq!(d.to_ask("x", &all, missing), &missing[1..]);
        assert_eq!(d.to_ask("other", &all, missing), missing);

        Declines::update(&data, |d| d.record("x", &all, &missing[1..])).unwrap();
        assert!(Declines::load(&data).to_ask("x", &all, missing).is_empty());

        // Same list in another order: still the same question.
        let mut shuffled = all.clone();
        shuffled.reverse();
        assert!(
            Declines::load(&data)
                .to_ask("x", &shuffled, missing)
                .is_empty()
        );
    }

    #[test]
    fn a_new_capability_list_asks_again() {
        let dir = tempfile::tempdir().unwrap();
        let all = caps();
        Declines::update(dir.path(), |d| d.record("x", &all, &all[2..])).unwrap();
        let mut grown = all.clone();
        grown.push(Capability::Net);
        let missing = &grown[2..];
        assert_eq!(
            Declines::load(dir.path()).to_ask("x", &grown, missing),
            missing
        );
        // A decline against the new list replaces the old record.
        Declines::update(dir.path(), |d| d.record("x", &grown, &[Capability::Net])).unwrap();
        let d = Declines::load(dir.path());
        assert_eq!(d.to_ask("x", &grown, missing), &grown[2..4]);
    }

    #[test]
    fn forgetting_and_a_bad_file_ask_again() {
        let dir = tempfile::tempdir().unwrap();
        let all = caps();
        Declines::update(dir.path(), |d| d.record("x", &all, &all[2..])).unwrap();
        Declines::update(dir.path(), |d| d.forget("x")).unwrap();
        assert_eq!(
            Declines::load(dir.path()).to_ask("x", &all, &all[2..]),
            &all[2..]
        );
        std::fs::write(Declines::path(dir.path()), "not json").unwrap();
        assert_eq!(
            Declines::load(dir.path()).to_ask("x", &all, &all[2..]),
            &all[2..]
        );
    }
}
