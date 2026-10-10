//! What changed, per release: the first launch after an update opens the
//! tutorial on a card listing it, with Take the tour, Start at what's new,
//! and Skip tutorial. Add an entry when a release adds something the tour
//! teaches, and set its version to the release's (CHANGELOG.md says when).
//!
//! The rule (every release): a new feature gets a tour step that lands on
//! it, with `since` set to the release, existing steps are edited when
//! what they show changed, and the entry's `first_step` is the first of the
//! release's own steps. The tests below hold every entry from 0.10.0 on to
//! that, and fail when the crate's version is an x.y.0 with no entry.

/// One release's news.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Release {
    pub version: &'static str,
    /// Short lines, plain text.
    pub items: &'static [&'static str],
    /// The tour step that teaches the first new thing (`steps::STEPS` id).
    pub first_step: Option<&'static str>,
}

pub const RELEASES: &[Release] = &[
    Release {
        version: "0.11.0",
        items: &[
            "reading-time and inspect, two bundled extensions (off until you turn them on): \
             “· 3 min” on each post you read, and ⋯ › inspect for the record behind it.",
            "The lineage glyph, ⌘J and the ring now count: how many posts a post draws on, \
             and how many draw on it.",
            "Bring your feeds from another reader: File › Import Subscriptions from OPML… (and \
             Export). They land in an Imported feeds folder in the Reader.",
            "markdown-notes takes several folders, and none until you choose one: Add folder… \
             in the drawer's Notes tab, or Settings › Notes folders.",
            "cross-post runs in the side pane, and you can fold the pane mid-run: the status \
             bar says “macro running · show pane”.",
            "⇧⌘P › Manage extensions can Turn off an extension or Forget its permissions; ⌘⌫ \
             turns one off from the permission sheet.",
            "Sign in again once after this update so read state keeps syncing between your Macs \
             (studio 0.39 or later): Burrow › Disconnect…, keep the local copy, then Sign in \
             with browser. An API token needs a new one with reading:state.",
        ],
        first_step: Some("slots"),
    },
    Release {
        version: "0.10.0",
        items: &[
            "Extensions: ⇧⌘P lists what they offer; Burrow asks before one may do anything, \
             and none can publish or see your sign-in.",
            "markdown-notes opens a folder of Markdown notes in the Notes drawer: search, edit, \
             and quote a note into your post.",
            "cross-post puts a published post on Substack Notes from the browser pane, after \
             you check the text twice.",
            "⌘N opens an empty editor; it's kept on this Mac until ⌘D makes it a draft.",
            "✂ Clip (⇧⌘C) in the browser pane quotes the page into a draft.",
        ],
        first_step: Some("new"),
    },
    Release {
        version: "0.9.0",
        items: &[
            "Sign in with your browser: Burrow asks your studio for access and renews it.",
            "In the Reader, ⌘-click, ⇧-click and ⌘A pick posts; r and u mark them read or \
             unread.",
            "Reply starts a stub quoting the whole post; choose a passage in the stub with \
             quote a passage instead.",
            "[[ and ![[ open a full-text picker over your posts and the ones you read.",
            "Rename a subscription, or hand it back to its own name; ⇧⌘R checks every feed.",
        ],
        first_step: Some("reader"),
    },
    Release {
        version: "0.8.0",
        items: &[
            "Lineage: a glyph beside each post's name shows which kinds of post it draws on and \
         which draw on it (fork, reply, quote).",
            "⌘J opens a post's lineage: what it draws on above, its responses below. Walk it one \
         step at a time with ⏎ and ⌫.",
            "Space on the post in the middle opens its actions on a ring: fork, reply, quote, link \
         post, versions, open.",
            "One response per post: a reply that also quotes is listed once, as a reply, and a \
         reply to a passage says so.",
        ],
        first_step: Some("lineage"),
    },
];

#[cfg(test)]
thread_local! {
    /// The version a test's window launches as (`running_version`).
    pub static TEST_VERSION: std::cell::RefCell<Option<&'static str>> =
        const { std::cell::RefCell::new(None) };
}

/// The version this launch counts as for what's new: the crate's. A debug
/// build takes `BLYGGER_VERSION_AS` instead (screenshots of an update's
/// card before the version is bumped), and a test its `TEST_VERSION`.
pub fn running_version() -> String {
    #[cfg(test)]
    if let Some(v) = TEST_VERSION.with(|v| *v.borrow()) {
        return v.to_string();
    }
    if cfg!(debug_assertions)
        && let Ok(v) = std::env::var("BLYGGER_VERSION_AS")
        && parse(&v).is_some()
    {
        return v;
    }
    env!("CARGO_PKG_VERSION").to_string()
}

fn parse(v: &str) -> Option<(u32, u32, u32)> {
    let mut it = v.trim().trim_start_matches('v').split('.');
    let n = |s: Option<&str>| s.and_then(|s| s.parse().ok());
    Some((n(it.next())?, n(it.next())?, n(it.next()).unwrap_or(0)))
}

/// The releases after `seen` (none seen: all) up to `current`, newest
/// first.
pub fn since(seen: Option<&str>, current: &str) -> Vec<&'static Release> {
    let Some(now) = parse(current) else {
        return vec![];
    };
    let from = seen.and_then(parse).unwrap_or((0, 0, 0));
    let mut out: Vec<&Release> = RELEASES
        .iter()
        .filter(|r| parse(r.version).is_some_and(|v| v > from && v <= now))
        .collect();
    out.sort_by_key(|r| std::cmp::Reverse(parse(r.version)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn news_is_what_shipped_since_you_last_looked() {
        assert!(since(Some("0.8.0"), "0.8.0").is_empty());
        assert!(
            since(Some("0.7.0"), "0.7.0").is_empty(),
            "not before it ships"
        );
        assert_eq!(since(Some("0.7.0"), "0.8.0")[0].version, "0.8.0");
        assert_eq!(since(None, "0.8.1").len(), 1);
        assert!(since(Some("0.8.0"), "0.8.2").is_empty());
        assert_eq!(since(Some("0.8.0"), "0.9.0")[0].version, "0.9.0");
        let both: Vec<_> = since(Some("0.7.0"), "0.9.0")
            .iter()
            .map(|r| r.version)
            .collect();
        assert_eq!(both, ["0.9.0", "0.8.0"], "newest first");
        assert!(since(Some("junk"), "junk").is_empty());
    }

    #[test]
    fn every_entry_names_a_real_step() {
        for r in RELEASES {
            assert!(parse(r.version).is_some(), "{}", r.version);
            assert!(!r.items.is_empty());
            if let Some(id) = r.first_step {
                assert!(
                    super::super::steps::STEPS.iter().any(|s| s.id == id),
                    "{id}"
                );
            }
        }
    }

    /// From this release on, every entry starts at a step of its own.
    const GUARDED_FROM: (u32, u32, u32) = (0, 10, 0);

    /// The guard against a what's-new card that starts the tour somewhere
    /// unrelated (0.10.0's started at the old reading-notes step): each
    /// entry's `first_step` is a step added for that release, and the first
    /// of them in the tour, so "Show me what's new" lands on the news.
    #[test]
    fn each_release_starts_at_a_step_of_its_own() {
        use super::super::steps::STEPS;
        for r in RELEASES {
            let v = parse(r.version).unwrap();
            if v < GUARDED_FROM {
                continue;
            }
            let own: Vec<&str> = STEPS
                .iter()
                .filter(|s| parse(s.since) == Some(v))
                .map(|s| s.id)
                .collect();
            assert!(
                !own.is_empty(),
                "{}: no tour step teaches it (add one with since = {:?})",
                r.version,
                r.version
            );
            assert_eq!(
                r.first_step,
                Some(own[0]),
                "{}: what's new should start at its first own step",
                r.version
            );
        }
        // No step claims a release that has no entry (a step added for a
        // release whose card would never send anyone to it).
        for s in STEPS {
            let v = parse(s.since).unwrap();
            if v >= GUARDED_FROM {
                assert!(
                    RELEASES.iter().any(|r| parse(r.version) == Some(v)),
                    "{}: since {} has no what's-new entry",
                    s.id,
                    s.since
                );
            }
        }
    }

    /// Cutting x.y.0 without a what's-new entry (and so without a tour step
    /// for what it adds) fails here.
    #[test]
    fn this_minor_release_has_an_entry() {
        let now = parse(env!("CARGO_PKG_VERSION")).unwrap();
        if now.2 == 0 && now >= GUARDED_FROM {
            assert!(
                RELEASES.iter().any(|r| parse(r.version) == Some(now)),
                "{} has no what's-new entry in RELEASES",
                env!("CARGO_PKG_VERSION")
            );
        }
    }

    #[test]
    fn a_launch_after_0_10_shows_0_11() {
        let news = since(Some("0.10.0"), "0.11.0");
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].version, "0.11.0");
        assert_eq!(news[0].first_step, Some("slots"));
        // Coming from 0.9, the tour starts at 0.10.0's first step.
        let both: Vec<_> = since(Some("0.9.0"), "0.11.0")
            .iter()
            .map(|r| r.first_step)
            .collect();
        assert_eq!(both, [Some("slots"), Some("new")]);
    }
}
