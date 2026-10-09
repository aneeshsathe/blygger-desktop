//! Capabilities: what an extension may ask Burrow for, each granted on its
//! own (`extension-allow = <name> <capability>` in the config file).
//!
//! Two kinds, and the consent sheet must say which is which:
//! - **Enforced** by Burrow: everything that goes through Burrow (your posts,
//!   drafts, reading list, the blyg's address, messages, hooks, the browser
//!   pane). Without the grant the call is refused (`-32001 permission
//!   denied`).
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
    /// `browser.capture`: `burrow/browser.page` (the open page's address,
    /// title, selection and readable text) while one of the extension's
    /// commands runs. Never cookies, storage or script.
    BrowserCapture,
    /// `browser.automate:<origin>`: run the extension's macros against that
    /// one origin, and `burrow/browser.open` URLs on it. Holds the origin
    /// normalised ([`normalize_origin`]): `https://social.example.com`.
    BrowserAutomate(String),
    /// `fs:<path>`: declared, not enforced. Holds the path as written.
    Fs(String),
    /// `net`: declared, not enforced.
    Net,
}

impl Capability {
    /// Parse a capability as written in a manifest or an `extension-allow`
    /// line. `None` for anything unknown (or `fs:` with no path, or
    /// `browser.automate:` with anything but an origin).
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
            "browser.capture" => Capability::BrowserCapture,
            "net" => Capability::Net,
            _ => {
                if let Some(o) = s.strip_prefix(AUTOMATE_PREFIX) {
                    return normalize_origin(o).ok().map(Capability::BrowserAutomate);
                }
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
            Capability::BrowserCapture => "browser.capture".into(),
            Capability::BrowserAutomate(o) => format!("{AUTOMATE_PREFIX}{o}"),
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
    /// normalising separators and a trailing slash (`~\Notes` = `~/Notes/`);
    /// `browser.automate:` origins compare normalised, and exactly (no
    /// wildcards, no parent domains, scheme and port included).
    pub fn covered_by(&self, granted: &Capability) -> bool {
        match (self, granted) {
            (Capability::Fs(a), Capability::Fs(b)) => norm_path(a) == norm_path(b),
            (Capability::BrowserAutomate(a), Capability::BrowserAutomate(b)) => {
                match (normalize_origin(a), normalize_origin(b)) {
                    (Ok(a), Ok(b)) => a == b,
                    _ => false,
                }
            }
            _ => self == granted,
        }
    }

    /// For `browser.automate:`: the (normalised) origin.
    pub fn automate_origin(&self) -> Option<&str> {
        match self {
            Capability::BrowserAutomate(o) => Some(o),
            _ => None,
        }
    }

    /// `browser.automate:` for the origin of `url` (any http(s) URL with a
    /// host; `None` otherwise): the grant a page at that URL needs.
    pub fn automate_for_url(url: &str) -> Option<Capability> {
        blyg_core::config::parse::url_origin(url).map(Capability::BrowserAutomate)
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

/// The written prefix of [`Capability::BrowserAutomate`].
pub const AUTOMATE_PREFIX: &str = blyg_core::config::keys::EXTENSION_AUTOMATE_PREFIX;

/// A website origin, normalised (lower-case host, `xn--` for international
/// names, default port dropped, no path, no wildcards); `Err` says what's
/// wrong. The config file's own rule, so a grant and a manifest agree.
pub fn normalize_origin(s: &str) -> Result<String, String> {
    blyg_core::config::parse::normalize_origin(s)
}

/// How an origin reads in plain words: `social.example.com` for https on
/// the default port, the whole origin otherwise (`http://127.0.0.1:8123`,
/// `https://social.example.com:8443`), so a plain-http or odd-port grant is
/// visible as such.
pub fn origin_label(origin: &str) -> String {
    match origin.strip_prefix("https://") {
        Some(host) if !host.contains(':') || host.ends_with(']') => host.to_string(),
        _ => origin.to_string(),
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
        Capability::BrowserCapture => "read the page open in the browser pane when you run one \
            of its commands (the address, title, selected text and the article's text; never \
            cookies or sign-ins)"
            .to_string(),
        Capability::BrowserAutomate(o) => format!(
            "fill in and, when you confirm, click Post on {}, signed in as you",
            origin_label(o)
        ),
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
            "browser.capture",
            "browser.automate:https://social.example.com",
            "browser.automate:http://127.0.0.1:8123",
            "net",
            "fs:~/Notes",
        ] {
            assert_eq!(Capability::parse(s).unwrap().as_string(), s);
        }
        assert_eq!(Capability::parse("items.publish"), None);
        assert_eq!(Capability::parse("fs:"), None);
        assert_eq!(Capability::parse("hooks:itemWithdrawn"), None);
        assert_eq!(Capability::parse("browser.automate:"), None);
        assert_eq!(
            Capability::parse("browser.automate:https://social.example.com/notes"),
            None
        );
        assert_eq!(
            Capability::parse("browser.automate:https://*.example.com"),
            None
        );
        assert_eq!(
            Capability::parse("browser.automate:social.example.com"),
            None
        );
        assert_eq!(Capability::parse("browser.eval"), None);
        assert_eq!(Capability::parse("browser.cookies"), None);
    }

    #[test]
    fn fs_paths_compare_across_separators() {
        let a = Capability::parse("fs:~/Notes").unwrap();
        assert!(a.covered_by(&Capability::parse("fs:~\\Notes\\").unwrap()));
        assert!(!a.covered_by(&Capability::parse("fs:~/Other").unwrap()));
        assert!(!a.covered_by(&Capability::Net));
    }

    #[test]
    fn automate_origins_are_normalised_and_compared_exactly() {
        let a = Capability::parse("browser.automate:HTTPS://Social.Example.com:443/").unwrap();
        assert_eq!(
            a,
            Capability::BrowserAutomate("https://social.example.com".into())
        );
        assert_eq!(a.automate_origin(), Some("https://social.example.com"));
        for same in [
            "browser.automate:https://social.example.com",
            "browser.automate:https://social.example.com/",
            "browser.automate:https://SOCIAL.example.com:443",
        ] {
            assert!(a.covered_by(&Capability::parse(same).unwrap()), "{same}");
        }
        for other in [
            "browser.automate:http://social.example.com",
            "browser.automate:https://social.example.com:8443",
            "browser.automate:https://www.social.example.com",
            "browser.automate:https://example.com",
        ] {
            assert!(!a.covered_by(&Capability::parse(other).unwrap()), "{other}");
        }
        assert!(!a.covered_by(&Capability::BrowserCapture));
        // A value built by hand, unnormalised, still compares by origin.
        assert!(Capability::BrowserAutomate("https://Social.Example.com/".into()).covered_by(&a));
        assert!(!Capability::BrowserAutomate("junk".into()).covered_by(&a));
        let idn = Capability::parse("browser.automate:https://BÜCHER.example").unwrap();
        assert_eq!(
            idn.as_string(),
            "browser.automate:https://xn--bcher-kva.example"
        );
        assert_eq!(
            Capability::automate_for_url("https://social.example.com/notes?draft=1#x"),
            Some(a.clone())
        );
        assert_eq!(Capability::automate_for_url("file:///etc/hosts"), None);
        assert!(a.is_enforced() && Capability::BrowserCapture.is_enforced());
    }

    #[test]
    fn browser_words_name_the_site_and_rule_out_cookies() {
        let a = Capability::parse("browser.automate:https://social.example.com").unwrap();
        assert_eq!(
            describe(&a),
            "fill in and, when you confirm, click Post on social.example.com, signed in as you"
        );
        let local = Capability::parse("browser.automate:http://127.0.0.1:8123").unwrap();
        assert!(describe(&local).contains("on http://127.0.0.1:8123,"));
        assert_eq!(
            origin_label("https://social.example.com:8443"),
            "https://social.example.com:8443"
        );
        assert_eq!(origin_label("https://[::1]"), "[::1]");
        let c = describe(&Capability::BrowserCapture);
        assert!(
            c.starts_with("read the page open in the browser pane"),
            "{c}"
        );
        assert!(c.contains("never cookies or sign-ins"), "{c}");
        assert!(!c.contains("promise"));
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
