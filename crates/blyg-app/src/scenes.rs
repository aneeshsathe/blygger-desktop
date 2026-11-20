//! Every demo scene (`BLYGGER_DEMO=<scene>`), named once. `DevFlags`
//! parses the variable into a [`Scene`] at startup (an unknown name stops
//! the app, naming near matches), `MainView::demo_step` dispatches on its
//! [`Area`], and each area's handler matches on the variants. `blygger
//! +scenes` prints [`Scene::all`]; `scripts/verify/burrow-verify.sh scenes`
//! reads that, so the script and the app can't disagree.
//!
//! A new scene: a row below, then an arm in its area's handler.

use std::fmt;

use crate::app::onboarding::steps;

/// Which handler plays a scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Area {
    Main,
    Upkeep,
    Ai,
    Reading,
    Profiles,
    Onboarding,
    Composer,
    Toolbar,
    Browser,
    Notes,
    Side,
    Extensions,
    AccessAi,
    Site,
    Contacts,
}

macro_rules! scenes {
    ($( $(#[$doc:meta])* $area:ident { $( $variant:ident = $name:literal, )* } )*) => {
        /// A demo scene: the fixed ones below, and the tutorial's families.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum Scene {
            $( $( $variant, )* )*
            /// `tut-<step id>`: the tutorial at that step; `tut-<step id>+`:
            /// after its key.
            TutStep { id: &'static str, after_key: bool },
            /// `tut-tour-<id>`: a tour from its start; `tut-tour-<id>-end`:
            /// at its end card.
            TutTour { id: &'static str, end: bool },
        }

        /// The fixed scenes: name and area.
        const FIXED: &[(Scene, &str, Area)] = &[
            $( $( (Scene::$variant, $name, Area::$area), )* )*
        ];
    };
}

scenes! {
    /// demo.rs's own steps
    Main {
        Search = "search",
        Create = "create",
        NewPost = "new-post",
        NewPostTyped = "new-post-typed",
        Publish = "publish",
        Published = "published",
        Long = "long",
        Studio = "studio",
        StudioEdit = "studio-edit",
        Preview = "preview",
        Conflict = "conflict",
        Offline = "offline",
        Settings = "settings",
        SettingsFull = "settings-full",
        SettingsPreview = "settings-preview",
        Toast = "toast",
        Empty = "empty",
        Connect = "connect",
        SkippedSetup = "skipped-setup",
        Disconnect = "disconnect",
        Switcher = "switcher",
        Switched = "switched",
        SwitcherManage = "switcher-manage",
        Update = "update",
        About = "about",
        Capture = "capture",
        Image = "image",
        Edit = "edit",
        LivePublish = "live-publish",
        LiveOpen = "live-open",
        LiveImage = "live-image",
        LivePreview = "live-preview",
        ConnectCheck = "connect-check",
    }
    /// upkeep/mod.rs
    Upkeep {
        UpkeepStale = "upkeep-stale",
        UpkeepSignals = "upkeep-signals",
        UpkeepActivity = "upkeep-activity",
        UpkeepDiff = "upkeep-diff",
        UpkeepAlt = "upkeep-alt",
    }
    /// ai/view.rs
    Ai {
        AiFill = "ai-fill",
        AiPalette = "ai-palette",
        AiShorten = "ai-shorten",
        AiSettings = "ai-settings",
    }
    /// reading/demo.rs (rd-browse*, rd-stream-nested: reading/browse.rs)
    Reading {
        RdStream = "rd-stream",
        RdStreamQuote = "rd-stream-quote",
        RdStreamLink = "rd-stream-link",
        RdStreamCatchup = "rd-stream-catchup",
        RdStreamCatchupWhole = "rd-stream-catchup-whole",
        RdStreamPane = "rd-stream-pane",
        RdStreamSearch = "rd-stream-search",
        RdStreamMarkAll = "rd-stream-mark-all",
        RdMarkAllReader = "rd-mark-all-reader",
        RdStreamCompose = "rd-stream-compose",
        RdStreamComposeHold = "rd-stream-compose-hold",
        RdComposeReader = "rd-compose-reader",
        RdStreamOriginal = "rd-stream-original",
        RdStreamFetch = "rd-stream-fetch",
        RdStreamResponses = "rd-stream-responses",
        RdStreamLineage = "rd-stream-lineage",
        RdStreamLineageRing = "rd-stream-lineage-ring",
        RdThreePane = "rd-three-pane",
        RdHoppers = "rd-hoppers",
        RdHopperPicker = "rd-hopper-picker",
        RdPicks = "rd-picks",
        RdPostMenu = "rd-post-menu",
        RdMoreMenu = "rd-more-menu",
        RdSubMenu = "rd-sub-menu",
        RdSubRename = "rd-sub-rename",
        RdStub = "rd-stub",
        RdStubSelect = "rd-stub-select",
        RdStubPassage = "rd-stub-passage",
        RdFolderMenu = "rd-folder-menu",
        RdMentionsFeed = "rd-mentions-feed",
        RdSmartToday = "rd-smart-today",
        RdSourcesHidden = "rd-sources-hidden",
        RdSubsFolder = "rd-subs-folder",
        RdNewFolder = "rd-new-folder",
        RdReading = "rd-reading",
        RdAuthorSite = "rd-author-site",
        RdStreamAuthorSite = "rd-stream-author-site",
        RdChangelog = "rd-changelog",
        RdSummary = "rd-summary",
        RdSearch = "rd-search",
        RdNomatch = "rd-nomatch",
        RdNotes = "rd-notes",
        RdPill = "rd-pill",
        RdVersions = "rd-versions",
        RdPin = "rd-pin",
        RdMentions = "rd-mentions",
        RdSubs = "rd-subs",
        RdDiscover = "rd-discover",
        RdSubscribe = "rd-subscribe",
        RdEmpty = "rd-empty",
        RdStreamEmpty = "rd-stream-empty",
        RdAllRead = "rd-all-read",
        RdSubsEmpty = "rd-subs-empty",
        RdSite = "rd-site",
        RdOwn = "rd-own",
        RdOwnPost = "rd-own-post",
        RdOwnFork = "rd-own-fork",
        RdBackground = "rd-background",
        RdStreamLead = "rd-stream-lead",
        RdQuote = "rd-quote",
        RdBrowse = "rd-browse",
        RdBrowseHistory = "rd-browse-history",
        RdBrowsePeek = "rd-browse-peek",
        RdStreamNested = "rd-stream-nested",
    }
    /// profiles/demo.rs
    Profiles {
        PfAsk = "pf-ask",
        PfBlogroll = "pf-blogroll",
        PfConnections = "pf-connections",
        PfFeed = "pf-feed",
        PfOwn = "pf-own",
        PfPosts = "pf-posts",
        PfReading = "pf-reading",
    }
    /// onboarding/mod.rs (and the tut-<step> / tut-tour-<id> families)
    Onboarding {
        ObWelcome = "ob-welcome",
        ObConnect = "ob-connect",
        ObAi = "ob-ai",
        ObButtons = "ob-buttons",
        ObTour = "ob-tour",
        ObNews = "ob-news",
        ObNewsGo = "ob-news-go",
        TutTours = "tut-tours",
    }
    /// composer/demo.rs
    Composer {
        CmMention = "cm-mention",
        CmLink = "cm-link",
        CmQuote = "cm-quote",
        CmSpell = "cm-spell",
        CmSpellMenu = "cm-spell-menu",
    }
    /// toolbar.rs (tb-delete, tb-withdraw, tb-discard: discard.rs)
    Toolbar {
        TbMain = "tb-main",
        TbLong = "tb-long",
        TbSettings = "tb-settings",
        TbDelete = "tb-delete",
        TbWithdraw = "tb-withdraw",
        TbDiscard = "tb-discard",
        TbCapture = "tb-capture",
    }
    /// browser/view.rs
    Browser {
        BrBlock = "br-block",
        BrSlide = "br-slide",
        BrFull = "br-full",
        BrTeardown = "br-teardown",
        BrReader = "br-reader",
        BrMacro = "br-macro",
        BrMacroExt = "br-macro-ext",
    }
    /// notes/drawer.rs
    Notes {
        NotesOpen = "notes-open",
        NotesAfterAdd = "notes-after-add",
        NotesOverBrowser = "notes-over-browser",
        NotesPosts = "notes-posts",
        NotesMenu = "notes-menu",
    }
    /// side.rs
    Side {
        SideWriting = "side-writing",
        SidePreview = "side-preview",
    }
    /// extensions/mod.rs (ext-slots*: extensions/reading_slots.rs)
    Extensions {
        ExtConsent = "ext-consent",
        ExtPalette = "ext-palette",
        ExtManage = "ext-manage",
        ExtNotes = "ext-notes",
        ExtSlots = "ext-slots",
        ExtSlotsInspect = "ext-slots-inspect",
    }
    /// access.rs
    AccessAi {
        AaAccess = "aa-access",
        AaConfirm = "aa-confirm",
        AaToken = "aa-token",
        AaAi = "aa-ai",
        AaPublish = "aa-publish",
    }
    /// site/mod.rs
    Site {
        SiteAppearance = "site-appearance",
        SiteCustom = "site-custom",
        SiteHome = "site-home",
        SiteLinks = "site-links",
        SiteProfile = "site-profile",
        SiteNetwork = "site-network",
        SiteUpdates = "site-updates",
        SiteExtensions = "site-extensions",
        SiteNotice = "site-notice",
    }
    /// contacts/demo.rs
    Contacts {
        CtBook = "ct-book",
        CtAuthor = "ct-author",
        CtSent = "ct-sent",
        CtSubs = "ct-subs",
    }
    /// app/publish.rs (--- tangle ---)
    Main {
        RdTangle = "rd-tangle",
        RdTanglePreview = "rd-tangle-preview",
    }
}

impl Scene {
    /// The scene `BLYGGER_DEMO` names.
    pub(crate) fn parse(name: &str) -> Result<Self, UnknownScene> {
        if let Some(&(s, _, _)) = FIXED.iter().find(|(_, n, _)| *n == name) {
            return Ok(s);
        }
        if let Some(rest) = name.strip_prefix("tut-tour-") {
            let (id, end) = match rest.strip_suffix("-end") {
                Some(id) => (id, true),
                None => (rest, false),
            };
            if let Some(id) = tour_ids().find(|t| *t == id) {
                return Ok(Scene::TutTour { id, end });
            }
        } else if let Some(rest) = name.strip_prefix("tut-") {
            let (id, after_key) = match rest.strip_suffix('+') {
                Some(id) => (id, true),
                None => (rest, false),
            };
            if let Some(st) = steps::STEPS.iter().find(|st| st.id == id) {
                return Ok(Scene::TutStep {
                    id: st.id,
                    after_key,
                });
            }
        }
        Err(UnknownScene(name.to_string()))
    }

    /// Which handler plays it.
    pub(crate) fn area(self) -> Area {
        match self {
            Scene::TutStep { .. } | Scene::TutTour { .. } => Area::Onboarding,
            s => FIXED
                .iter()
                .find(|(f, _, _)| *f == s)
                .map(|(_, _, a)| *a)
                .expect("every fixed scene is in FIXED"),
        }
    }

    /// Every scene's name, as `BLYGGER_DEMO` takes it.
    pub(crate) fn all() -> Vec<String> {
        let mut all: Vec<String> = FIXED.iter().map(|(_, n, _)| n.to_string()).collect();
        for st in steps::STEPS {
            all.push(format!("tut-{}", st.id));
            all.push(format!("tut-{}+", st.id));
        }
        for t in tour_ids() {
            all.push(format!("tut-tour-{t}"));
            all.push(format!("tut-tour-{t}-end"));
        }
        all
    }
}

fn tour_ids() -> impl Iterator<Item = &'static str> {
    std::iter::once(steps::QUICK.id).chain(steps::TOURS.iter().map(|t| t.id))
}

impl fmt::Display for Scene {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scene::TutStep { id, after_key } => {
                write!(f, "tut-{id}{}", if *after_key { "+" } else { "" })
            }
            Scene::TutTour { id, end } => {
                write!(f, "tut-tour-{id}{}", if *end { "-end" } else { "" })
            }
            s => {
                let name = FIXED.iter().find(|(f, _, _)| f == s).map(|(_, n, _)| *n);
                f.write_str(name.expect("every fixed scene is in FIXED"))
            }
        }
    }
}

/// `BLYGGER_DEMO` names no scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnknownScene(pub String);

impl fmt::Display for UnknownScene {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BLYGGER_DEMO={}: no such demo scene", self.0)?;
        let near = near_matches(&self.0);
        if !near.is_empty() {
            write!(f, "; did you mean {}?", near.join(", "))?;
        }
        write!(f, " (`blygger +scenes` lists them all)")
    }
}

/// Up to five scene names closest to `name`: a small edit distance, or the
/// same family (the part before the first `-`).
fn near_matches(name: &str) -> Vec<String> {
    let family = |s: &str| s.split('-').next().unwrap_or("").to_string();
    let mut scored: Vec<(usize, String)> = Scene::all()
        .into_iter()
        .map(|s| (edit_distance(name, &s), s))
        .filter(|(d, s)| *d <= 2.max(name.len() / 3) || family(s) == family(name))
        .collect();
    scored.sort();
    scored.into_iter().take(5).map(|(_, s)| s).collect()
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_parses_back_to_itself() {
        let all = Scene::all();
        for name in &all {
            let s = Scene::parse(name).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(&s.to_string(), name);
        }
        let mut unique = all.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), all.len(), "a scene name is listed twice");
    }

    #[test]
    fn families_parse() {
        assert_eq!(Scene::parse("rd-stream"), Ok(Scene::RdStream));
        assert_eq!(
            Scene::parse("tut-search+"),
            Ok(Scene::TutStep {
                id: "search",
                after_key: true
            })
        );
        assert_eq!(
            Scene::parse("tut-tour-quick-end"),
            Ok(Scene::TutTour {
                id: "quick",
                end: true
            })
        );
        assert_eq!(Scene::RdStream.area(), Area::Reading);
        assert_eq!(
            Scene::parse("tut-tours").map(Scene::area),
            Ok(Area::Onboarding)
        );
    }

    /// The verify skill's feature files name scenes in their tables'
    /// first column (`| \`rd-stream\` | ...`): each is a real one.
    #[test]
    fn the_feature_files_name_real_scenes() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../skills/verify-burrow/features");
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).expect("features dir") {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            for line in text.lines().filter(|l| l.starts_with("| `")) {
                let cell = line.split('|').nth(1).unwrap_or("");
                for name in cell.split('`').skip(1).step_by(2) {
                    if name.contains('=') {
                        continue; // a variable that tunes the scene
                    }
                    assert!(
                        Scene::parse(name).is_ok(),
                        "{}: `{name}` is no scene",
                        path.display()
                    );
                    seen += 1;
                }
            }
        }
        assert!(
            seen > 10,
            "found only {seen} scene names in {}",
            dir.display()
        );
    }

    #[test]
    fn an_unknown_name_names_near_matches() {
        let e = Scene::parse("rd-strem").unwrap_err().to_string();
        assert!(e.contains("no such demo scene"), "{e}");
        assert!(e.contains("rd-stream"), "{e}");
        assert!(Scene::parse("tut-no-such-step").is_err());
        assert!(Scene::parse("tut-tour-nope").is_err());
    }
}
