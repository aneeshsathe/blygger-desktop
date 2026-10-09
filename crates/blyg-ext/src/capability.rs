//! Capabilities: what an extension may ask Burrow for, each granted on its
//! own (`extension-allow = <name> <capability>` in the config file).
//!
//! Two kinds, and the consent sheet must say which is which:
//! - **Enforced** by Burrow: everything that goes through Burrow (your posts,
//!   drafts, reading list, the blyg's address, messages, hooks). Without the
//!   grant the call is refused (`-32001 permission denied`).
//! - **Declared**: `fs:<path>` and `net`. An extension is a program that runs
//!   as you, so Burrow can't stop it from reading other files or using the
//!   network. These are the extension's promise of what it does, shown so
//!   you can decide whether to trust it, not a sandbox.

use std::path::{Path, PathBuf};

/// One capability.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    /// `items.read`: list, get and search your own items.
    ItemsRead,
    /// `items.write`: create drafts and scratch notes, save working copies.
    /// Never publishes.
    ItemsWrite,
    /// `reading.read`: the posts held locally from subscriptions.
    ReadingRead,
    /// `blyg.identity`: the blyg's origin at `initialize` (never a token).
    BlygIdentity,
    /// `ui`: status-bar messages, selecting an item in the list.
    Ui,
    /// `hooks:itemPublished`.
    HookPublished,
    /// `hooks:itemSaved`.
    HookSaved,
    /// `hooks:itemCreated`.
    HookCreated,
    /// `fs:<path>`: declared, not enforced. Holds the path as written.
    Fs(String),
    /// `net`: declared, not enforced.
    Net,
}

impl Capability {
    /// Parse a capability as written in a manifest or an `extension-allow`
    /// line. `None` for anything unknown (or `fs:` with no path).
    pub fn parse(s: &str) -> Option<Capability> {
        let s = s.trim();
        Some(match s {
            "items.read" => Capability::ItemsRead,
            "items.write" => Capability::ItemsWrite,
            "reading.read" => Capability::ReadingRead,
            "blyg.identity" => Capability::BlygIdentity,
            "ui" => Capability::Ui,
            "hooks:itemPublished" => Capability::HookPublished,
            "hooks:itemSaved" => Capability::HookSaved,
            "hooks:itemCreated" => Capability::HookCreated,
            "net" => Capability::Net,
            _ => {
                let p = s.strip_prefix("fs:")?.trim();
                if p.is_empty() {
                    return None;
                }
                Capability::Fs(p.to_string())
            }
        })
    }

    /// The written form (what `parse` reads back).
    pub fn as_string(&self) -> String {
        match self {
            Capability::ItemsRead => "items.read".into(),
            Capability::ItemsWrite => "items.write".into(),
            Capability::ReadingRead => "reading.read".into(),
            Capability::BlygIdentity => "blyg.identity".into(),
            Capability::Ui => "ui".into(),
            Capability::HookPublished => "hooks:itemPublished".into(),
            Capability::HookSaved => "hooks:itemSaved".into(),
            Capability::HookCreated => "hooks:itemCreated".into(),
            Capability::Fs(p) => format!("fs:{p}"),
            Capability::Net => "net".into(),
        }
    }

    /// Whether Burrow enforces it (false for `fs:` and `net`, which are
    /// declarations).
    pub fn is_enforced(&self) -> bool {
        !matches!(self, Capability::Fs(_) | Capability::Net)
    }

    /// Whether `granted` covers this capability. `fs:` paths compare after
    /// normalising separators and a trailing slash (`~\Notes` = `~/Notes/`).
    pub fn covered_by(&self, granted: &Capability) -> bool {
        match (self, granted) {
            (Capability::Fs(a), Capability::Fs(b)) => norm_path(a) == norm_path(b),
            _ => self == granted,
        }
    }

    /// For `fs:`: the directory, with a leading `~` expanded against `home`.
    pub fn fs_path(&self, home: Option<&Path>) -> Option<PathBuf> {
        match self {
            Capability::Fs(p) => Some(expand_home(p, home)),
            _ => None,
        }
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_string())
    }
}

/// What a capability lets an extension do, in plain words, for the consent
/// sheet and `+list-extensions`: "read and write files in ~/Notes". Lower
/// case, no full stop, so a list can join them ("… wants to: a · b · c").
/// Declared capabilities say so ([`DECLARED_SUFFIX`]).
pub fn describe(cap: &Capability) -> String {
    let base = match cap {
        Capability::ItemsRead => "read your posts, drafts and scratch notes".to_string(),
        Capability::ItemsWrite => "create and edit drafts (it can never publish)".to_string(),
        Capability::ReadingRead => "read the posts held from your subscriptions".to_string(),
        Capability::BlygIdentity => "know your blyg's address (never your sign-in)".to_string(),
        Capability::Ui => "show messages and select posts in the list".to_string(),
        Capability::HookPublished => "be told when you publish, with the post's text".to_string(),
        Capability::HookSaved => "be told when you edit a post, with its text".to_string(),
        Capability::HookCreated => "be told when you create a post, with its text".to_string(),
        Capability::Fs(p) => format!("read and write files in {p}"),
        Capability::Net => "use the network".to_string(),
    };
    if cap.is_enforced() {
        base
    } else {
        format!("{base}{DECLARED_SUFFIX}")
    }
}

/// Appended to a declared capability's words.
pub const DECLARED_SUFFIX: &str = " (its own promise; Burrow can't enforce this)";

/// The consent sheet's caveat, shown under the list of capabilities.
pub const CONSENT_CAVEAT: &str = "An extension is a program that runs as you. Burrow hands it \
only what you allow here, and it can never publish, delete or see your sign-in. Files and \
network access are the extension's own promise: Burrow can't stop it from reading other \
files or going online, so only allow extensions you trust.";

/// "markdown-notes wants to: a · b · c", for the consent sheet's heading.
pub fn consent_sentence(name: &str, caps: &[Capability]) -> String {
    let words: Vec<String> = caps.iter().map(describe).collect();
    if words.is_empty() {
        format!("{name} doesn't ask for anything")
    } else {
        format!("{name} wants to: {}", words.join(" · "))
    }
}

fn norm_path(p: &str) -> String {
    let p = p.trim().replace('\\', "/");
    let t = p.trim_end_matches('/');
    if t.is_empty() { "/".into() } else { t.into() }
}

/// `~`, `~/x` and `~\x` against `home`; anything else as written.
pub fn expand_home(p: &str, home: Option<&Path>) -> PathBuf {
    let p = p.trim();
    if let Some(h) = home {
        if p == "~" {
            return h.to_path_buf();
        }
        if let Some(rest) = p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
            let mut out = h.to_path_buf();
            for part in rest.split(['/', '\\']).filter(|s| !s.is_empty()) {
                out.push(part);
            }
            return out;
        }
    }
    PathBuf::from(p)
}

/// The user's home directory from the environment: `HOME`, else
/// `USERPROFILE` (Windows).
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|h| !h.is_empty()))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_writes_back() {
        for s in [
            "items.read",
            "items.write",
            "reading.read",
            "blyg.identity",
            "ui",
            "hooks:itemPublished",
            "hooks:itemSaved",
            "hooks:itemCreated",
            "net",
            "fs:~/Notes",
        ] {
            assert_eq!(Capability::parse(s).unwrap().as_string(), s);
        }
        assert_eq!(Capability::parse("items.publish"), None);
        assert_eq!(Capability::parse("fs:"), None);
        assert_eq!(Capability::parse("hooks:itemWithdrawn"), None);
    }

    #[test]
    fn fs_paths_compare_across_separators() {
        let a = Capability::parse("fs:~/Notes").unwrap();
        assert!(a.covered_by(&Capability::parse("fs:~\\Notes\\").unwrap()));
        assert!(!a.covered_by(&Capability::parse("fs:~/Other").unwrap()));
        assert!(!a.covered_by(&Capability::Net));
    }

    #[test]
    fn expands_home_both_ways() {
        let home = Path::new("/home/someone");
        assert_eq!(
            expand_home("~/Notes/Vault", Some(home)),
            home.join("Notes").join("Vault")
        );
        assert_eq!(expand_home("~\\Notes", Some(home)), home.join("Notes"));
        assert_eq!(expand_home("/abs", Some(home)), PathBuf::from("/abs"));
        assert_eq!(expand_home("~/x", None), PathBuf::from("~/x"));
    }

    #[test]
    fn plain_words_are_honest_about_enforcement() {
        let fs = Capability::parse("fs:~/Notes").unwrap();
        assert_eq!(
            describe(&fs),
            "read and write files in ~/Notes (its own promise; Burrow can't enforce this)"
        );
        assert!(describe(&Capability::Net).ends_with(DECLARED_SUFFIX));
        assert!(!describe(&Capability::ItemsWrite).contains("promise"));
        assert!(describe(&Capability::ItemsWrite).contains("never publish"));
        assert_eq!(
            consent_sentence("markdown-notes", &[Capability::Ui]),
            "markdown-notes wants to: show messages and select posts in the list"
        );
    }
}
