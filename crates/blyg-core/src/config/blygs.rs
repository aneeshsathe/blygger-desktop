//! Several blygs, one active (the blyg switcher).
//!
//! **Where the list lives.** In the config file, as a repeatable key:
//!
//! ```text
//! blyg-url = https://blyg.example.com
//! blyg = https://blyg.example.com
//! blyg = https://notes.example.org
//! ```
//!
//! `blyg-url` keeps meaning "the blyg Burrow writes to now", so existing
//! configs, the CLI and everything else that reads it are unchanged. `blyg`
//! lists every blyg you're signed in to, in the switcher's order (⌃⌘1…⌃⌘9).
//! The active blyg is always in the list, written or not, and the list is only
//! written once there are two: a one-blyg config never grows a `blyg` line.
//!
//! **Identity.** A blyg is its host, as the Keychain keys its sign-in
//! ([`super::token_account`]): two entries with the same host are one blyg.
//!
//! **Data.** Each blyg has its own data folder holding the same files
//! (`blygger.db`, its WAL, the theme cache): the blyg that already owns the
//! data dir's database (named in [`DB_OWNER_FILE`]) keeps it there, exactly as
//! before; every other blyg gets `<data dir>/blygs/<host>/` ([`data_dir_for`]).
//! Two blygs' drafts, reading, outbox or read state never share a database.

use std::path::{Path, PathBuf};

use super::Change;
use super::parse::Config;

/// The repeatable key listing every signed-in blyg.
pub const KEY: &str = "blyg";
/// The active blyg.
pub const ACTIVE_KEY: &str = "blyg-url";
/// Which blyg the database in a data folder belongs to (its URL).
pub const DB_OWNER_FILE: &str = "blygger.db.blyg-url";
/// Where blygs other than the data dir's own keep their data.
pub const BLYGS_DIR: &str = "blygs";
/// The database file name (as `live::DB_FILE`).
const DB_FILE: &str = "blygger.db";

/// A blyg's identity: its host, lowercased (the Keychain account).
pub fn identity(url: &str) -> String {
    super::token_account(url).to_ascii_lowercase()
}

/// Whether two URLs name the same blyg.
pub fn same(a: &str, b: &str) -> bool {
    identity(a) == identity(b)
}

/// Every signed-in blyg, in order: the `blyg` lines (one per host, first
/// wins), plus `blyg-url` at the end when no line names it.
pub fn known(cfg: &Config) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for u in cfg.list(KEY) {
        if !out.iter().any(|o| same(o, &u)) {
            out.push(u);
        }
    }
    if let Some(active) = cfg.blyg_url()
        && !out.iter().any(|o| same(o, active))
    {
        out.push(active.to_string());
    }
    out
}

/// The active blyg's position in [`known`].
pub fn active_index(cfg: &Config) -> Option<usize> {
    let active = cfg.blyg_url()?;
    known(cfg).iter().position(|u| same(u, active))
}

/// The `blyg` lines for `list`: written with two or more, removed below.
fn list_change(list: Vec<String>) -> Change {
    if list.len() >= 2 {
        Change::List(list)
    } else {
        Change::Remove
    }
}

/// Make `url` the active blyg, adding it to the list if it's new (signing in
/// to another blyg keeps the others). A URL naming a known blyg replaces
/// that entry's spelling.
pub fn activate(cfg: &Config, url: &str) -> Vec<(&'static str, Change)> {
    let mut list = known(cfg);
    match list.iter().position(|u| same(u, url)) {
        Some(i) => list[i] = url.to_string(),
        None => list.push(url.to_string()),
    }
    vec![
        (ACTIVE_KEY, Change::Set(url.to_string())),
        (KEY, list_change(list)),
    ]
}

/// What removing a blyg changes, and the blyg that's active after (the
/// next one in the list when it was the active one; `None`: none left).
pub fn remove(cfg: &Config, url: &str) -> (Vec<(&'static str, Change)>, Option<String>) {
    let list: Vec<String> = known(cfg).into_iter().filter(|u| !same(u, url)).collect();
    let was_active = cfg.blyg_url().is_some_and(|a| same(a, url));
    let mut changes = Vec::new();
    let next = if was_active {
        let next = list.first().cloned();
        changes.push((
            ACTIVE_KEY,
            match &next {
                Some(n) => Change::Set(n.clone()),
                None => Change::Remove,
            },
        ));
        next
    } else {
        cfg.blyg_url().map(str::to_string)
    };
    changes.push((KEY, list_change(list)));
    (changes, next)
}

/// A blyg's folder name under `blygs/`: its host, with anything but
/// letters, digits, `.` and `-` turned into `-`.
pub fn slug(url: &str) -> String {
    let s: String = identity(url)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('.').to_string();
    if s.is_empty() { "blyg".into() } else { s }
}

/// Who owns the database in `dir`, if anyone said.
fn owner(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join(DB_OWNER_FILE))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The folder holding `url`'s data under the data dir `root`:
/// 1. `root` itself, when its database belongs to this blyg (today's layout,
///    unchanged for every existing install);
/// 2. `root/blygs/<host>/`, when that already holds a database;
/// 3. `root`, when nothing owns it yet (the first blyg on a fresh install);
/// 4. otherwise `root/blygs/<host>/`.
pub fn data_dir_for(root: &Path, url: &str) -> PathBuf {
    let sub = root.join(BLYGS_DIR).join(slug(url));
    match owner(root) {
        Some(o) if same(&o, url) => root.to_path_buf(),
        _ if sub.join(DB_FILE).exists() => sub,
        // A database from before the owner file is the active blyg's, as it
        // always was.
        None => root.to_path_buf(),
        Some(_) => sub,
    }
}

/// Whether `dir` is a blyg's own folder under `root/blygs/` (deleting its
/// local data can remove the whole folder).
pub fn is_sub_dir(root: &Path, dir: &Path) -> bool {
    dir.parent() == Some(&root.join(BLYGS_DIR))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ConfigStore;

    const A: &str = "https://blyg.example.com";
    const B: &str = "https://notes.example.org";

    fn cfg(text: &str) -> Config {
        ConfigStore::in_memory(text).config().clone()
    }

    #[test]
    fn one_blyg_configs_list_just_it() {
        assert!(known(&cfg("")).is_empty());
        let c = cfg(&format!("blyg-url = {A}\n"));
        assert_eq!(known(&c), vec![A]);
        assert_eq!(active_index(&c), Some(0));
    }

    #[test]
    fn the_list_keeps_order_dedupes_by_host_and_includes_the_active_one() {
        let c = cfg(&format!(
            "blyg = {B}\nblyg = https://NOTES.example.org/\nblyg-url = {A}/\n"
        ));
        assert_eq!(known(&c), vec![B, A]);
        assert_eq!(active_index(&c), Some(1));
    }

    #[test]
    fn activating_adds_and_writes_the_list_only_with_two() {
        let mut s = ConfigStore::in_memory("# mine\n");
        s.set(&activate(s.config(), A)).unwrap();
        assert_eq!(s.text().unwrap(), format!("# mine\n\nblyg-url = {A}\n"));
        s.set(&activate(s.config(), B)).unwrap();
        assert_eq!(s.config().blyg_url(), Some(B));
        assert_eq!(known(s.config()), vec![A, B]);
        assert!(
            s.text()
                .unwrap()
                .contains(&format!("blyg = {A}\nblyg = {B}"))
        );
        // Switching back keeps the order.
        s.set(&activate(s.config(), A)).unwrap();
        assert_eq!(known(s.config()), vec![A, B]);
        assert_eq!(active_index(s.config()), Some(0));
    }

    #[test]
    fn removing_the_active_blyg_activates_the_next() {
        let mut s = ConfigStore::in_memory(&format!("blyg-url = {B}\nblyg = {A}\nblyg = {B}\n"));
        let (changes, next) = remove(s.config(), B);
        assert_eq!(next.as_deref(), Some(A));
        s.set(&changes).unwrap();
        assert_eq!(s.text().unwrap(), format!("blyg-url = {A}\n"));
        let (changes, next) = remove(s.config(), A);
        assert_eq!(next, None);
        s.set(&changes).unwrap();
        assert!(known(s.config()).is_empty());
        assert_eq!(s.config().blyg_url(), None);
    }

    #[test]
    fn removing_another_blyg_keeps_the_active_one() {
        let c = cfg(&format!("blyg-url = {A}\nblyg = {A}\nblyg = {B}\n"));
        let (changes, next) = remove(&c, B);
        assert_eq!(next.as_deref(), Some(A));
        assert_eq!(changes, vec![(KEY, Change::Remove)]);
    }

    #[test]
    fn slugs_are_folder_safe() {
        assert_eq!(slug(B), "notes.example.org");
        assert_eq!(slug("http://127.0.0.1:8787"), "127.0.0.1");
        assert_eq!(slug("https://Blyg.Example.com/sub/"), "blyg.example.com");
    }

    #[test]
    fn the_first_blyg_keeps_the_data_dir_and_others_get_their_own() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        // A fresh install: the first blyg takes the data dir itself.
        assert_eq!(data_dir_for(root, A), root);
        // Today's layout: the db and its owner file in the data dir.
        std::fs::write(root.join(DB_FILE), "").unwrap();
        std::fs::write(root.join(DB_OWNER_FILE), A).unwrap();
        assert_eq!(data_dir_for(root, A), root);
        assert_eq!(data_dir_for(root, &format!("{A}/")), root);
        let b = data_dir_for(root, B);
        assert_eq!(b, root.join("blygs").join("notes.example.org"));
        assert!(is_sub_dir(root, &b));
        assert!(!is_sub_dir(root, root));
        // Once B has data there, it stays there even if the data dir frees up.
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join(DB_FILE), "").unwrap();
        std::fs::remove_file(root.join(DB_OWNER_FILE)).unwrap();
        assert_eq!(data_dir_for(root, B), b);
        // A database from before the owner file stays the active blyg's.
        assert_eq!(data_dir_for(root, A), root);
    }
}
