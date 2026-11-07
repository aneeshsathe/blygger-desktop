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
        version: "0.18.0",
        items: &[
            "The shelf: ⌥⌘B keeps a post you keep updating (a /now page, a reading log) at the \
             top of the list, marked 📚. ⌘B opens the shelf; 1–9 opens a post on it, ⌥↑/⌥↓ \
             reorder it. It's kept on this Mac for now.",
            if cfg!(target_os = "windows") {
                "Lineage while you write: a quiet line under the editor shows the open post's \
                 lineage glyph, what it draws on and what draws on it. A click opens the \
                 lists; Ctrl+J opens the lineage sheet for your own post."
            } else {
                "Lineage while you write: a quiet line under the editor shows the open post's \
                 lineage glyph, what it draws on and what draws on it. A click opens the \
                 lists; ⌘J opens the lineage sheet for your own post."
            },
            "The lineage sheet zooms out: − shows the neighbourhood two hops each way, coloured \
             by relation. ⏎ re-centres on a post, ⌫ walks back, + returns to the list.",
            if cfg!(target_os = "windows") {
                "Hover cards: rest the pointer on a [[…]] or ![[…]] in the editor (or a quote \
                 or link in the Stream) to see what it is; “label it” names an unlabelled \
                 link. Ctrl-click opens the post."
            } else {
                "Hover cards: rest the pointer on a [[…]] or ![[…]] in the editor (or a quote \
                 or link in the Stream) to see what it is; “label it” names an unlabelled \
                 link. ⌘-click opens the post."
            },
            // --- chat ---
            if cfg!(target_os = "windows") {
                "#thicket (Ctrl+Shift+I), an experimental chat tab: talk with other Burrow users on \
                 Libera.Chat (IRC), with unread and mention marks and a notification when \
                 you're mentioned. Nothing connects until you set it up, Settings hides it, \
                 and it may be removed."
            } else {
                "#thicket (⇧⌘I), an experimental chat tab: talk with other Burrow users on \
                 Libera.Chat (IRC), with unread and mention marks and a notification when \
                 you're mentioned. Nothing connects until you set it up, Settings hides it, \
                 and it may be removed."
            },
            // --- tangles ---
            if cfg!(target_os = "windows") {
                "Trails: t in Reading adds the post to a trail, with a word on why; Shift+T \
                 starts another. Trails are listed with your sources, and Ctrl+Alt+T writes \
                 one as a thread of quotes in a new draft. Nothing is published."
            } else {
                "Trails: t in Reading adds the post to a trail, with a word on why; ⇧T starts \
                 another. Trails are listed with your sources, and ⌥⌘T writes one as a \
                 thread of quotes in a new draft. Nothing is published."
            },
            if cfg!(target_os = "windows") {
                "Drag a post into what you're writing to quote it (a link in a fragment), or \
                 onto the search box to start a thread. q or Ctrl+Alt+K keeps it on the quote \
                 tray above the status bar, and Ctrl+K lists the tray first."
            } else {
                "Drag a post into what you're writing to quote it (a link in a fragment), or \
                 onto the omnibar to start a thread. q or ⌥⌘K keeps it on the quote tray \
                 above the status bar, and ⌘K lists the tray first."
            },
            if cfg!(target_os = "windows") {
                "Split view: Ctrl+Alt+Enter opens the selected post beside the one you're \
                 writing (yours editable, anyone else's read-only); Ctrl+Alt+[ and Ctrl+Alt+] \
                 move between them."
            } else {
                "Split view: ⌥⌘⏎ opens the selected post beside the one you're writing (yours \
                 editable, anyone else's read-only); ⌥⌘[ and ⌥⌘] move between them."
            },
        ],
        first_step: Some("shelf"),
    },
    Release {
        version: "0.17.0",
        items: &[
            "Burrow stays quick with thousands of posts in Reading: a sync that finds nothing \
             new no longer holds up the window, and lists, search and marking posts read are \
             many times faster.",
            "Pick several posts and press h to put them all in a hopper at once. A hopper \
             lists its posts in its own order, the most recently added first.",
            "The reader follows Burrow's theme, not the Mac's. bracket-picker = quotes (or off) \
             in the config stops typing [[ from opening the post picker.",
            "Switching blygs is safer: nothing from the blyg you left reaches the one you're \
             on, and the switcher keys work on Windows.",
        ],
        first_step: Some("hopper-several"),
    },
    Release {
        version: "0.16.0",
        items: &[
            "Burrow now needs blygger-studio 0.49.12 or later. On an older blyg, nothing syncs \
             until it's updated; your posts stay on this computer and are sent afterwards.",
            "A post edited in two places is never overwritten: when it changed on your blyg \
             too, Burrow shows both copies and you choose which to keep, or keep both.",
            "Long reading lists come in pages, and nothing is removed from this computer until \
             the whole list has been read.",
        ],
        first_step: Some("conflict"),
    },
    Release {
        version: "0.15.0",
        items: &[
            "Hoppers: your blyg's collections of posts, yours and others', are listed under \
             Hoppers in the Reader's sources. Pick one to list its posts.",
            "h on the reading screen, ⌥⌘P, or Hopper › on a post puts it in a hopper or takes \
             it out; New Hopper… makes one and adds the post. Adds made offline are sent \
             later.",
            "Right-click a hopper to rename it, describe it, make it public (it gets a page on \
             your blyg) or private, or delete it. Deleting a hopper never touches its posts.",
        ],
        first_step: Some("hoppers"),
    },
    Release {
        version: "0.14.0",
        items: &[
            if cfg!(target_os = "windows") {
                "Several blygs: click the blyg's name in the title bar, or press Ctrl+Shift+L, \
                 to switch, add one or sign out of one; Ctrl+Shift+1–9 jump straight to one. \
                 Each keeps its own posts and reading."
            } else {
                "Several blygs: click the blyg's name in the title bar, or press ⌃⌘0, to \
                 switch, add one or sign out of one; ⌃⌘1–⌃⌘9 jump straight to one. Each keeps \
                 its own posts and reading."
            },
            "Burrow opens on Reading now. ⌘R goes to your posts and back.",
            "Write while you read: the box at the foot of Reading posts a fragment (⌘⏎) or \
             keeps a draft (⌘D). c or ⇧⌘K puts the caret in it; esc keeps your text.",
            "A revised post leads with what changed in the stream: “V3: …”, from its author's \
             changelog.",
            "Follow the thicket: in the reader, a quote, a stub's “responds to” line or a link \
             to a blyg post opens that post. The breadcrumb shows the path; ⌘[ steps back.",
            "⇧⌘H lists where following links has taken you. It's private, and never \
             published.",
            "Withdrawn posts are listed under Background in the Reader, and nowhere else.",
            "A stub's passage is chosen in the preview: select it inside the quote, then ❝ \
             quote only this or ⇧⌘D. The separate chooser is gone.",
        ],
        first_step: Some("switcher"),
    },
    Release {
        version: "0.13.0",
        items: &[
            "Links have labels: [[ and ![[ write [[id | label]], the post's title (or its \
             heading). The label is the link's text on the page, so edit the words after | \
             to rename it; a quote's note is yours only. Rows lead with the title.",
            "Discard changes: a published post with unpublished edits goes back to the version \
             it published with ⌥⌘Z, Post › Discard Changes…, the toolbar's eraser or ⌘Y. It \
             asks first; the public post never changes.",
            "Your own posts: Your blyg is a Reading source, never unread or counted. A \
             published post in Posts offers Reply · new stub, Quote into a thread and Link \
             post above its text; Fork this pin in ⌘Y forks one.",
            "Labelled links need blygger-studio 0.47. On an older blyg only posts with them \
             wait: Burrow says “Your blyg needs updating”, links to how, and publishing offers \
             to write the links bare.",
        ],
        first_step: Some("discard"),
    },
    Release {
        version: "0.12.0",
        items: &[
            "Burrow works with a stock blyg: an unmodified blygger-studio 0.40 or later, no \
             server patches. Reading, read marks, lineage and citations all sync.",
            "Sign in with browser, or with a token from Studio › More › Client access (allow \
             reading:state so read marks sync). The old server owner token is gone from \
             sign-in.",
            "If your blyg refuses Burrow's sign-in, Burrow stops syncing, keeps every change \
             waiting, and opens the sign-in sheet to say why.",
            "Publishing warns when generated text left a ![[…]] on its own line that its \
             instruction didn't ask for: it would quote that post and notify its author. \
             The preview shows the quote.",
            "In the Reader, text a blyg marks as generated says, on hover and at the end of \
             the post, that the mark is the publisher's own disclosure, not verified.",
            "A post that drew on yours for generated text is listed as “drew on this”.",
        ],
        first_step: Some("echoed-quote"),
    },
    Release {
        version: "0.11.0",
        items: &[
            "reading-time and inspect, two bundled extensions (off until you turn them on): \
             “· 3 min” on each post you read, and ⋯ › inspect for the record behind it.",
            "The lineage glyph, ⌘J and the ring now count: how many posts a post draws on, \
             and how many draw on it.",
            "Bring your feeds from another reader: Blyg › Import Subscriptions from OPML… (and \
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
    fn a_launch_after_0_13_shows_0_14() {
        let news = since(Some("0.13.1"), "0.14.0");
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].version, "0.14.0");
        assert_eq!(news[0].first_step, Some("switcher"));
        let from_0_12: Vec<_> = since(Some("0.12.0"), "0.14.0")
            .iter()
            .map(|r| r.first_step)
            .collect();
        assert_eq!(from_0_12, [Some("switcher"), Some("discard")]);
    }

    #[test]
    fn a_launch_after_0_12_shows_0_13() {
        let news = since(Some("0.12.0"), "0.13.0");
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].version, "0.13.0");
        assert_eq!(news[0].first_step, Some("discard"));
        let from_0_11: Vec<_> = since(Some("0.11.0"), "0.13.0")
            .iter()
            .map(|r| r.first_step)
            .collect();
        assert_eq!(from_0_11, [Some("discard"), Some("echoed-quote")]);
    }

    #[test]
    fn a_launch_after_0_11_shows_0_12() {
        let news = since(Some("0.11.0"), "0.12.0");
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].version, "0.12.0");
        assert_eq!(news[0].first_step, Some("echoed-quote"));
        let from_0_10: Vec<_> = since(Some("0.10.0"), "0.12.0")
            .iter()
            .map(|r| r.first_step)
            .collect();
        assert_eq!(from_0_10, [Some("echoed-quote"), Some("slots")]);
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
