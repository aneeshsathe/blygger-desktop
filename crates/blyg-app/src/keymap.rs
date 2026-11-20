//! Every key binding the app has, in one table: the key, where it applies,
//! what it does, and its menu. `bind_keys` registers the table,
//! `blygger +list-keybinds` prints it, and the tests below refuse clashes.
//!
//! Where a binding applies (`Scope`):
//! - `Global`: anywhere in the app.
//! - `Main`: the main window (key context `Blygger`). gpui-base's text
//!   inputs sit *inside* it with their own, deeper `Input` context, so an
//!   `Input` binding on the same key would win while an input has focus.
//! - `MainInput`: `Blygger > Input`, a binding that deliberately out-ranks
//!   gpui-base's own binding of the same key inside our inputs (⌘⏎, which
//!   gpui-base binds to "submit").
//! - `MacroSheet` / `MacroSheetInput`: a browser macro's Preview and Post
//!   sheets (`MacroSheet`, and `MacroSheet > Input` for the Preview's text
//!   box). Deeper than, or as deep as and later than, the main window's
//!   rows, so ⌘⏎ continues there instead of reaching Publish.

use gpui_kit::{
    Action, App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate,
    PlatformKeyboardMapper,
};

use crate::app::ShowCapture; // --- buttons ---

use crate::ai::{AiGenerate, AiShorten}; // --- AI ---

use crate::app::{
    Disconnect, FakeConflict, FakeToggleOffline, FocusSearch, FontBigger, FontReset, FontSmaller,
    NewDraft, OpenConfigFile, OpenPermalink, OpenSettings, Publish, Quit, ReloadConfig, ToggleKind,
    TogglePreview,
};
// --- scratch notes ---
use crate::app::scratch::{KeepCapture, MakeDraft};
// --- full editor ---
use crate::app::studio::{ViewSplit, ViewStudio, ViewWrite};
// --- onboarding ---
use crate::app::onboarding::{ShowTutorial, TutorialBack, TutorialNext};
// --- reading & versions ---
use crate::app::reading::{
    QuotePicker, ShowLineage, ShowMentions, ShowReading, ShowSubscriptions, ShowVersions,
    SiteSettings, SubscribeTo,
};
// --- stream --- (the reading screen's Stream | Reader toggle)
use crate::app::reading::stream::{ReaderMode, StreamMode};
// --- reader folders ---
use crate::app::reading::sources::{CheckFeeds, MarkAllRead, NewFolder, ToggleSources};
// --- OPML ---
use crate::app::reading::discover::ShowDiscover; // --- discover ---
use crate::app::reading::opml::{ExportOpml, ImportOpml};
// --- catch up ---
use crate::app::reading::catchup::CatchUp;
// --- browse ---
use crate::app::reading::browse::{ReaderBack, ToggleBrowseHistory};
// --- reading compose ---
use crate::app::reading::compose::FocusCompose;
// --- own posts ---
use crate::app::reading::own::{LinkToPost, QuotePost, ReplyToPost};
// --- hoppers ---
use crate::app::reading::hoppers::{HopperPicker, NewHopper};
// --- contacts ---
use crate::app::reading::contacts::{AddAuthorToContacts, ShowContacts};

// --- site ---
use crate::app::site::ShowSite;
// --- profiles ---
use crate::app::profiles::{MyProfile, OpenProfile, ShowProfile};
// --- delete & withdraw ---
use crate::app::discard::{DeleteDraft, DiscardChanges, Withdraw};
// --- auto-update ---
use crate::update::CheckForUpdates;
// --- about ---
use crate::about::ShowAbout;
// --- composer ---
use crate::composer::ToggleSpellcheck;
// --- notes ---
use crate::app::notes::{QuoteToDraft, ToggleNotes};
// --- extensions ---
use crate::app::extensions::ShowExtensions;
// --- shelf ---
use crate::app::shelf::{ShowShelf, ToggleShelf};
// --- access-ai ---
use crate::app::access::ShowClientAccess;
use crate::app::note_ai::DraftChangeNote;
use crate::app::server_ai::ShowServerAi;
// --- upkeep ---
use crate::app::syndication::ShowCopiesElsewhere;
use crate::app::upkeep::{CopyLink, ShowSignals, ShowStaleQuotes}; // --- syndication ---
// --- trails ---
use crate::app::trails::PublishTrail;
// --- tray ---
use crate::app::tray::AddToTray;
// --- split ---
use crate::app::split::{FocusLeftPost, FocusRightPost, OpenBeside};
// --- nearby ---
use crate::app::nearby::{NearbyDown, NearbyLink, NearbyQuote, NearbyUp, ToggleNearby};
// --- blyg switcher ---
use crate::app::switcher::{
    AddBlyg, ConnectBlyg, ManageBlygs, ShowBlygSwitcher, SwitchBlyg1, SwitchBlyg2, SwitchBlyg3,
    SwitchBlyg4, SwitchBlyg5, SwitchBlyg6, SwitchBlyg7, SwitchBlyg8, SwitchBlyg9,
};
// --- browser ---
use crate::app::browser::{
    BrowserAddress, BrowserBack, BrowserForward, BrowserReload, ClipPage, OpenBrowser,
    ToggleBrowser,
};
// --- browser macros ---
use crate::app::browser::automation::{MacroCancel, MacroContinue, MacroPost};
// --- flow ---
use crate::app::command_palette::ShowPalette;
use crate::app::focus::ToggleFocus;
use crate::app::quiet::ToggleQuietWriting; // --- quiet ---

pub const MAIN: &str = crate::app::CONTEXT;
/// Our own inputs inside the main window (deeper than gpui-base's `Input`).
pub const MAIN_INPUT: &str = "Blygger > Input";
/// A context that never matches: bindings here exist only so the native
/// menu shows the right key equivalent (see `Keybind::menu_key`).
const MENU_ONLY: &str = "BlyggerMenuKeyEquivalent";
/// --- browser --- The browser pane (its own place: its keys may reuse
/// main-window keys, which it out-ranks while it has focus).
pub const BROWSER: &str = "Blygger > Browser";
/// --- browser macros --- A macro's Preview and Post sheets.
pub const MACRO_SHEET: &str = crate::app::browser::automation::SHEET_CONTEXT;
/// --- browser macros --- The Preview sheet's text box.
pub const MACRO_SHEET_INPUT: &str = "MacroSheet > Input";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Main,
    MainInput,
    /// --- browser --- `Blygger > Browser`: the browser pane.
    Browser,
    /// --- browser macros --- `MacroSheet`: a macro's Preview or Post sheet.
    MacroSheet,
    /// --- browser macros --- `MacroSheet > Input`: the Preview's text box
    /// (out-ranks gpui-base's and the main window's keys there).
    MacroSheetInput,
}

impl Scope {
    fn context(self) -> Option<&'static str> {
        match self {
            Scope::Global => None,
            Scope::Main => Some(MAIN),
            Scope::MainInput => Some(MAIN_INPUT),
            Scope::Browser => Some(BROWSER), // --- browser ---
            Scope::MacroSheet => Some(MACRO_SHEET),
            Scope::MacroSheetInput => Some(MACRO_SHEET_INPUT),
        }
    }

    /// Where the key is live, for clash checks: everything in the main
    /// window counts as one place.
    #[cfg(test)]
    fn place(self) -> &'static str {
        match self {
            Scope::Browser => "browser pane", // --- browser ---
            Scope::MacroSheet | Scope::MacroSheetInput => "macro sheet",
            _ => "main window",
        }
    }
}

pub struct Keybind {
    /// GPUI keystroke syntax (`cmd-shift-,`).
    pub key: &'static str,
    pub scope: Scope,
    /// The action's name, for `+list-keybinds`.
    pub action: &'static str,
    /// What it does, in the user's words.
    pub label: &'static str,
    /// The menu item that shows this key, if any.
    pub menu: Option<&'static str>,
    /// When GPUI can't turn `key` into an NSMenu key equivalent (it passes
    /// "enter" through as the letter ⌘E), the key it should show instead,
    /// bound in a context that never matches.
    pub menu_key: Option<&'static str>,
    make: Option<MakeBinding>,
    // --- buttons ---
    /// The toolbar icon (a name in `app::toolbar::ICONS`), when the row has a button.
    pub icon: Option<&'static str>,
    /// The toolbar button's short label (`show-buttons`); its tooltip is this
    /// plus the key (`tooltip`), so the buttons teach the keys.
    pub button: Option<&'static str>,
    /// Builds the action, so a button dispatches exactly what the key does.
    pub new_action: Option<fn() -> Box<dyn Action>>,
}

/// Builds a row's binding: its key, its context, the keyboard mapper.
type MakeBinding = fn(&str, Option<&str>, &dyn PlatformKeyboardMapper) -> KeyBinding;

macro_rules! kb {
    ($key:expr, $scope:ident, $action:ident, $label:expr, $menu:expr) => {
        Keybind {
            key: $key,
            scope: Scope::$scope,
            action: stringify!($action),
            label: $label,
            menu: $menu,
            menu_key: None,
            make: Some(|k, c, m| binding(k, $action, c, m)),
            icon: None,
            button: None,
            new_action: Some(|| Box::new($action)),
        }
    };
}

/// A row with no key: it's in a menu (and maybe the toolbar) only.
macro_rules! menu_only {
    ($action:ident, $label:expr, $menu:expr) => {
        Keybind {
            key: "",
            scope: Scope::Global,
            action: stringify!($action),
            label: $label,
            menu: Some($menu),
            menu_key: None,
            make: None,
            icon: None,
            button: None,
            new_action: Some(|| Box::new($action)),
        }
    };
}

/// A toolbar button for a row: `button!(kb!(…), "icon", "Label")`.
macro_rules! button {
    ($row:expr, $icon:expr, $label:expr) => {
        Keybind {
            icon: Some($icon),
            button: Some($label),
            ..$row
        }
    };
}

/// The one table, as this platform binds it. Order matters only for
/// display. Keys are written in macOS terms (`mac_table`); `platform_keys`
/// respells them for Windows.
pub fn table() -> Vec<Keybind> {
    platform_keys(mac_table())
}

/// The table in macOS terms, unrespelled: what the documentation site's
/// shortcut reference shows (`keymap_docs`).
pub fn mac_table() -> Vec<Keybind> {
    vec![
        kb!(
            "cmd-l",
            Main,
            FocusSearch,
            "Search / the omnibar",
            Some("Post › Find…")
        ),
        button!(
            kb!("cmd-n", Main, NewDraft, "New post", Some("Post › New Post")),
            "plus",
            "New"
        ),
        button!(
            Keybind {
                menu_key: Some("cmd-↩"),
                ..kb!(
                    "cmd-enter",
                    MainInput,
                    Publish,
                    "Publish…",
                    Some("Post › Publish…")
                )
            },
            "send",
            "Publish"
        ),
        kb!(
            "cmd-enter",
            Main,
            Publish,
            "Publish… (focus outside a text field)",
            None
        ),
        kb!(
            "cmd-t",
            Main,
            ToggleKind,
            "Fragment ⇄ thread",
            Some("Post › Fragment ⇄ Thread")
        ),
        kb!(
            "cmd-e",
            Main,
            TogglePreview,
            "Preview on/off, in place",
            Some("View › Preview")
        ),
        // --- composer --- (@-mentions and spellcheck in the editor and quick
        // capture). The mention popup's ↑/↓/⏎/⇥/esc and the spelling menu
        // (right-click or ctrl-click a flagged word) are the popups' own keys,
        // like the quote picker's; the toggle has no key.
        menu_only!(
            ToggleSpellcheck,
            "Check spelling while typing (on/off; the spellcheck config key)",
            "Edit › Spelling › Check Spelling While Typing"
        ),
        // --- end composer ---
        kb!(
            "cmd-o",
            Main,
            OpenPermalink,
            "Open the post on the web",
            Some("Post › Open on the Web")
        ),
        kb!(
            "cmd-=",
            Main,
            FontBigger,
            "Bigger text",
            Some("View › Bigger")
        ),
        kb!("cmd-+", Main, FontBigger, "Bigger text", None),
        kb!(
            "cmd--",
            Main,
            FontSmaller,
            "Smaller text",
            Some("View › Smaller")
        ),
        kb!(
            "cmd-0",
            Main,
            FontReset,
            "Actual size",
            Some("View › Actual Size")
        ),
        kb!(
            "cmd-,",
            Global,
            OpenSettings,
            "Settings",
            Some("Burrow › Settings…")
        ),
        kb!(
            "cmd-shift-,",
            Global,
            ReloadConfig,
            "Reload the config file",
            Some("Burrow › Reload Config")
        ),
        kb!(
            "cmd-<",
            Global,
            ReloadConfig,
            "Reload the config file",
            None
        ),
        kb!("cmd-q", Global, Quit, "Quit", Some("Burrow › Quit Burrow")),
        kb!(
            "ctrl-alt-cmd-o",
            Main,
            FakeToggleOffline,
            "Simulate offline (BLYGGER_FAKE only)",
            None
        ),
        kb!(
            "ctrl-alt-cmd-c",
            Main,
            FakeConflict,
            "Simulate a conflict (BLYGGER_FAKE only)",
            None
        ),
        // --- full editor ---
        button!(
            kb!(
                "cmd-1",
                Main,
                ViewWrite,
                "Write: list + editor",
                Some("View › Write")
            ),
            "panel-left",
            "Write"
        ),
        button!(
            kb!(
                "cmd-2",
                Main,
                ViewSplit,
                "List + editor + preview",
                Some("View › List + Editor + Preview")
            ),
            "columns-3",
            "Preview"
        ),
        button!(
            kb!(
                "cmd-3",
                Main,
                ViewStudio,
                "Full editor: editor + preview",
                Some("View › Full Editor")
            ),
            "columns-2",
            "Full editor"
        ),
        // --- end full editor ---
        // Menu-only (no key): listed so the table is the whole story.
        menu_only!(
            Disconnect,
            "Disconnect from this blyg",
            "Burrow › Disconnect…"
        ),
        menu_only!(
            OpenConfigFile,
            "Open the config file",
            "Burrow › Open Config File"
        ),
        // --- blyg switcher --- (switcher.rs; the blyg list is the `blyg` key)
        kb!(
            "ctrl-cmd-0",
            Main,
            ShowBlygSwitcher,
            "Blyg switcher: every blyg you're signed in to (or click the title)",
            Some("Blyg › Switch Blyg…")
        ),
        kb!(
            "ctrl-cmd-1",
            Main,
            SwitchBlyg1,
            "Switch to the first blyg in the switcher (Blyg › Switch To)",
            None
        ),
        kb!(
            "ctrl-cmd-2",
            Main,
            SwitchBlyg2,
            "Switch to the second blyg",
            None
        ),
        kb!(
            "ctrl-cmd-3",
            Main,
            SwitchBlyg3,
            "Switch to the third blyg",
            None
        ),
        kb!(
            "ctrl-cmd-4",
            Main,
            SwitchBlyg4,
            "Switch to the fourth blyg",
            None
        ),
        kb!(
            "ctrl-cmd-5",
            Main,
            SwitchBlyg5,
            "Switch to the fifth blyg",
            None
        ),
        kb!(
            "ctrl-cmd-6",
            Main,
            SwitchBlyg6,
            "Switch to the sixth blyg",
            None
        ),
        kb!(
            "ctrl-cmd-7",
            Main,
            SwitchBlyg7,
            "Switch to the seventh blyg",
            None
        ),
        kb!(
            "ctrl-cmd-8",
            Main,
            SwitchBlyg8,
            "Switch to the eighth blyg",
            None
        ),
        kb!(
            "ctrl-cmd-9",
            Main,
            SwitchBlyg9,
            "Switch to the ninth blyg",
            None
        ),
        menu_only!(
            ConnectBlyg,
            "Connect your blyg: the Connect sheet (address, then sign in or a token)",
            "Burrow › Connect…"
        ),
        menu_only!(
            AddBlyg,
            "Sign in to another blyg (the others stay signed in)",
            "Blyg › Add a Blyg…"
        ),
        menu_only!(
            ManageBlygs,
            "Manage blygs: sign out of one, keeping or deleting its posts on this Mac",
            "Blyg › Manage Blygs…"
        ),
        // --- end blyg switcher ---
        // --- auto-update ---
        menu_only!(
            CheckForUpdates,
            "Check for a new release now (whatever auto-update says)",
            "Burrow › Check for Updates…"
        ),
        // --- buttons --- The quick-capture hotkey is `capture-hotkey`, not a
        // binding here; its button's tooltip shows the configured one.
        button!(
            menu_only!(
                ShowCapture,
                "Quick capture (the global hotkey, capture-hotkey)",
                "Burrow › Quick Capture"
            ),
            "zap",
            "Capture"
        ),
        // --- end buttons ---
        // --- scratch notes ---
        // Global so the quick-capture panel (no key context) gets them too;
        // gpui-base's inputs bind neither key.
        button!(
            kb!(
                "cmd-d",
                Global,
                MakeDraft,
                "Make draft (a scratch note → a draft on the blyg)",
                Some("Post › Make Draft")
            ),
            "file-up",
            "Make draft"
        ),
        button!(
            kb!(
                "cmd-s",
                Global,
                KeepCapture,
                "Quick capture: keep it (scratch, or a draft per capture-default)",
                None
            ),
            "sticky-note",
            "Scratch"
        ),
        // --- end scratch notes ---
        // --- AI ---
        button!(
            kb!(
                "cmd-g",
                Main,
                AiGenerate,
                "Generate: fill the [TK] under the caret, or open the AI helpers",
                Some("Post › Generate (TK)")
            ),
            "sparkles",
            "Generate"
        ),
        kb!(
            "cmd-shift-g",
            Main,
            AiShorten,
            "Shorten to fit 1000 (AI; you accept or reject)",
            Some("Post › Shorten to Fit 1000")
        ),
        // --- end AI ---
        // --- reading & versions ---
        kb!(
            "cmd-r",
            Main,
            ShowReading,
            "Reading list (again: back to posts)",
            Some("Blyg › Reading")
        ),
        kb!(
            "cmd-shift-m",
            Main,
            ShowMentions,
            "Responses: who quoted, replied to or forked your posts",
            Some("Blyg › Responses")
        ),
        kb!(
            "cmd-shift-s",
            Main,
            ShowSubscriptions,
            "Subscriptions",
            Some("Blyg › Subscriptions")
        ),
        button!(
            kb!(
                "cmd-y",
                Main,
                ShowVersions,
                "Versions of this post",
                Some("Post › Versions…")
            ),
            "rotate-ccw-clock",
            "Versions"
        ),
        kb!(
            "cmd-j",
            Main,
            ShowLineage,
            "Lineage: what this post draws on and what draws on it",
            Some("Post › Lineage…")
        ),
        // --- catch up --- (⇧C on the reading screen is its own key, as
        // `t` is: a single key can't be bound while you type)
        menu_only!(
            CatchUp,
            "Catch up: the conversation leading to this post, oldest first (⇧C in Reading)",
            "Post › Catch Up…"
        ),
        kb!(
            "cmd-k",
            Main,
            QuotePicker,
            "Quote a post in this thread",
            Some("Post › Quote…")
        ),
        // --- own posts --- the reading actions on the post in front of you:
        // your published post in Posts, the open (or selected) post in Reading.
        kb!(
            "alt-cmd-r",
            Main,
            ReplyToPost,
            "Reply: a new thread of yours that replies to this post (yours too)",
            Some("Post › Reply")
        ),
        kb!(
            "alt-cmd-q",
            Main,
            QuotePost,
            "Quote this post into a thread (![[…]]; the draft open, or a new one)",
            Some("Post › Quote into a Thread")
        ),
        kb!(
            "alt-cmd-l",
            Main,
            LinkToPost,
            "Link post · new fragment: [[…]] to this post, not a reply",
            Some("Post › Link Post · New Fragment")
        ),
        // --- hoppers --- (⌥⌘H is the system's Hide Others; `h` on the
        // reading screen is the picker's own single key)
        kb!(
            "alt-cmd-p",
            Main,
            HopperPicker,
            "Hopper ›: put this post in one of your hoppers, or take it out (h on the reading screen)",
            Some("Post › Put in Hopper…")
        ),
        // --- shelf --- (posts kept to hand). ⌘B is free in gpui-base's
        // inputs, which bind no bold; ⇧⌘B is the browser, so the toggle is
        // ⌥⌘B (Ctrl+Shift+A on Windows, see WINDOWS_OVERRIDES).
        kb!(
            "cmd-b",
            Main,
            ShowShelf,
            "Shelf: the posts you keep to hand (1–9 opens one)",
            Some("Post › Shelf…")
        ),
        kb!(
            "alt-cmd-b",
            Main,
            ToggleShelf,
            "Add the open post to your shelf, or remove it",
            Some("Post › Add to Shelf / Remove from Shelf")
        ),
        // --- end own posts ---
        // --- trails --- (`t` / ⇧T on the reading screen are its own keys,
        // in reading_key_down; ⌥⌘T is free in gpui-base's inputs)
        kb!(
            "alt-cmd-t",
            Main,
            PublishTrail,
            "Publish trail…: the current trail as a thread draft (t / ⇧T in Reading add stops)",
            Some("Blyg › Publish Trail…")
        ),
        // --- end trails ---
        // --- tray --- (⇧⌘Q is the system's Log Out; ⌥⌘K sits by ⌘K, the
        // picker that lists the tray first. `q` on the reading screen is
        // its own key, in reading_key_down.)
        kb!(
            "alt-cmd-k",
            Main,
            AddToTray,
            "Quote tray: put this post on it, to quote later (⌘K lists it first; q in Reading)",
            Some("Post › Put on Quote Tray")
        ),
        // --- end tray ---
        // --- split --- (⌘[ / ⌘] are gpui-base's outdent / indent in every
        // editor and the reader's Back, so the sides are ⌥⌘[ / ⌥⌘])
        Keybind {
            menu_key: Some("alt-cmd-↩"),
            ..kb!(
                "alt-cmd-enter",
                Main,
                OpenBeside,
                "Split view: the selected post beside the one you're writing (again: close it)",
                Some("Post › Open Beside")
            )
        },
        kb!(
            "alt-cmd-[",
            Main,
            FocusLeftPost,
            "Split view: the keyboard to the left post",
            Some("View › Left Post")
        ),
        kb!(
            "alt-cmd-]",
            Main,
            FocusRightPost,
            "Split view: the keyboard to the right post",
            Some("View › Right Post")
        ),
        // --- end split ---
        // --- nearby --- the rail beside the editor. ⌥↑/⌥↓/⌥⏎/⌥⇧⏎ out-rank
        // the text field (gpui-base binds none of them) and pass through
        // when the rail has nothing to offer.
        kb!(
            "alt-cmd-u",
            Main,
            ToggleNearby,
            "Nearby thoughts: open or close the drawer of your posts that say what this paragraph says",
            Some("View › Nearby Thoughts")
        ),
        kb!(
            "alt-up",
            MainInput,
            NearbyUp,
            "Nearby thoughts: the row above (typing in the editor)",
            None
        ),
        kb!(
            "alt-down",
            MainInput,
            NearbyDown,
            "Nearby thoughts: the row below (typing in the editor)",
            None
        ),
        kb!(
            "alt-enter",
            MainInput,
            NearbyLink,
            "Nearby thoughts: link the chosen post over the phrase it shares ([[id | phrase]])",
            None
        ),
        kb!(
            "alt-shift-enter",
            MainInput,
            NearbyQuote,
            "Nearby thoughts: quote the chosen post on a new line (![[id | title]], threads only)",
            None
        ),
        // --- end nearby ---
        menu_only!(
            SubscribeTo,
            "Subscribe to a blyg or feed",
            "Blyg › Subscribe…"
        ),
        // --- OPML --- (no key: an occasional chore)
        menu_only!(
            ImportOpml,
            "Import subscriptions from another reader's OPML file",
            "Blyg › Import Subscriptions from OPML…"
        ),
        menu_only!(
            ExportOpml,
            "Save every subscription as an OPML file",
            "Blyg › Export Subscriptions as OPML…"
        ),
        // --- discover --- (no key: an occasional look around)
        menu_only!(
            ShowDiscover,
            "Discover blygs listed on blygger.com that you don't follow yet",
            "Blyg › Discover Blygs…"
        ),
        menu_only!(
            SiteSettings,
            "The blyg's site settings (title, bio, links)",
            "Blyg › Site Settings…"
        ),
        // --- access-ai --- client access and the blyg's own AI (studio 0.56)
        menu_only!(
            ShowClientAccess,
            "Client access: the apps and tokens that can act as you on your blyg; make a token, revoke any",
            "Blyg › Client Access…"
        ),
        menu_only!(
            ShowServerAi,
            "AI on your blyg: a model per function, house style, change notes, the change classifier",
            "Blyg › AI on Your Blyg…"
        ),
        kb!(
            "alt-cmd-n",
            MainInput,
            DraftChangeNote,
            "Publish sheet: draft the version note with your blyg's AI",
            Some("Post › Draft a Change Note")
        ),
        // --- end access-ai ---
        // --- site ---
        menu_only!(
            ShowSite,
            "The blyg's public site: theme, typeface, front page, menus, avatar, collection feeds, studio updates and extensions",
            "Blyg › Site…"
        ),
        // --- end site ---
        // --- upkeep --- (no keys: occasional chores, and ⌘B/⌘I and their
        // neighbours are taken; Windows would read ⌥ chords as AltGr)
        menu_only!(
            ShowStaleQuotes,
            "Stale quotes: your threads whose quoted posts have newer versions; refresh them",
            "Blyg › Stale Quotes…"
        ),
        menu_only!(
            ShowSignals,
            "Signals: your likes, dislikes and private activity log (never published)",
            "Blyg › Signals…"
        ),
        menu_only!(
            CopyLink,
            "Copy the link to the post in front of you",
            "Post › Copy Link"
        ),
        // --- end upkeep ---
        // --- syndication ---
        menu_only!(
            ShowCopiesElsewhere,
            "Copies elsewhere: where this published post is also posted (no new version)",
            "Post › Copies Elsewhere…"
        ),
        // --- stream --- (issue #1) the reading screen's two modes; the
        // stream's own keys (j/k, ⏎/Space, esc) live in reading_key_down.
        kb!(
            "alt-cmd-1",
            Main,
            StreamMode,
            "Reading as a stream (the default)",
            Some("Blyg › Stream")
        ),
        kb!(
            "alt-cmd-2",
            Main,
            ReaderMode,
            "Reading as a list with the post beside it",
            Some("Blyg › Reader")
        ),
        // --- end stream ---
        // --- reading compose --- the compose box on the reading screen. Its
        // single key, `c` (the list focused), lives in reading_key_down; in
        // the box ⌘⏎ posts, ⌘D saves a draft and esc goes back to the list.
        kb!(
            "cmd-shift-k",
            Main,
            FocusCompose,
            "Compose while reading: the post box at the foot of the reading screen (c there)",
            Some("Blyg › Compose While Reading")
        ),
        // --- end reading compose ---
        // --- reader folders --- the Reader's sources pane. Its own keys
        // (←/→ between panes, ↑/↓, j/k, Space, [ ], esc) live in
        // reading_key_down / sources::reader_key.
        kb!(
            "alt-cmd-s",
            Main,
            ToggleSources,
            "Show or hide the Reader's sources pane (smart feeds, folders)",
            Some("Blyg › Sources Pane")
        ),
        menu_only!(
            NewFolder,
            "A new folder for your subscriptions (on this Mac only)",
            "Blyg › New Folder…"
        ),
        // --- hoppers ---
        menu_only!(
            NewHopper,
            "A new hopper on your blyg: a collection of posts, private until you make it public",
            "Blyg › New Hopper…"
        ),
        // --- subscription checks --- (studio 0.30; one subscription's
        // "Check Now" is in its menu in the sources pane)
        kb!(
            "cmd-shift-r",
            Main,
            CheckFeeds,
            "Check all feeds now (your blyg polls every subscription)",
            Some("Blyg › Check All Feeds Now")
        ),
        // ⇧R on the reading screen is its own key (reading_key_down): a
        // single key can't be bound while you type.
        menu_only!(
            MarkAllRead,
            "Mark all read in this source… (⇧R on the reading screen; asks first)",
            "Blyg › Mark All Read…"
        ),
        // --- end reader folders ---
        // --- browse --- (studio 0.49) the walk from post to post inside the
        // reader. Its key, ⌘[, is the reading screen's own (handled in
        // reading_key_down, like ⌘F): gpui-base's inputs bind ⌘[ themselves,
        // and the browser pane's ⌘[ is the pane's.
        menu_only!(
            ReaderBack,
            "Reading pane: back along the quotes, replies and links you followed (⌘[ on the reading screen)",
            "Blyg › Back"
        ),
        kb!(
            "cmd-shift-h",
            Main,
            ToggleBrowseHistory,
            "Browsing history beside the reading pane: where following links took you (private)",
            Some("Blyg › Browsing History")
        ),
        // --- end reading & versions ---
        // --- delete & withdraw --- (docs/SPEC.md rule 5: published work is
        // withdrawn, never deleted). ⇧⌘⌫, not ⌘⌫: that's delete-to-line-start
        // in every text field. Withdraw is irreversible, so it has no key.
        button!(
            kb!(
                "cmd-shift-backspace",
                Main,
                DeleteDraft,
                "Delete this draft or scratch note (asks first)",
                Some("Post › Delete Draft…")
            ),
            "trash-2",
            "Delete"
        ),
        // --- discard changes --- (upstream's "discard changes": the
        // current published version back into the working copy). ⌥⌘Z, next
        // to ⌘Z / ⇧⌘Z; gpui-base's inputs don't bind it.
        button!(
            kb!(
                "alt-cmd-z",
                Main,
                DiscardChanges,
                "Discard unpublished changes: back to the published version (asks first)",
                Some("Post › Discard Changes…")
            ),
            "eraser",
            "Discard"
        ),
        button!(
            menu_only!(
                Withdraw,
                "Withdraw this published post (asks first; permanent and visible)",
                "Post › Withdraw…"
            ),
            "archive-x",
            "Withdraw"
        ),
        // --- end delete & withdraw ---
        // --- profiles --- (inside the sheet: esc, ↑/↓, ⏎, F, ⇥ and ← are
        // the sheet's own keys, like the reading screens' ↑/↓)
        kb!(
            "cmd-i",
            Main,
            ShowProfile,
            "Profile of this post's author (yours on your own post); again closes it",
            Some("Blyg › Profile")
        ),
        kb!(
            "cmd-shift-o",
            Main,
            OpenProfile,
            "Open a profile: paste a blyg, a post or a feed",
            Some("Blyg › Open Profile…")
        ),
        menu_only!(
            MyProfile,
            "Your own profile, as visitors see it",
            "Blyg › My Profile"
        ),
        // --- end profiles ---
        // --- onboarding ---
        kb!(
            "alt-cmd-right",
            Main,
            TutorialNext,
            "Tutorial: next step",
            None
        ),
        kb!(
            "alt-cmd-left",
            Main,
            TutorialBack,
            "Tutorial: previous step",
            None
        ),
        menu_only!(
            ShowTutorial,
            "The interactive tutorial (on sample data)",
            "Help › Burrow Tutorial"
        ),
        // --- end onboarding ---
        // --- about --- One action, two menus (the standard app-menu place
        // and Help), so two rows.
        menu_only!(
            ShowAbout,
            "Version, build, updates and connection info",
            "Burrow › About Burrow"
        ),
        menu_only!(
            ShowAbout,
            "Version, build, updates and connection info",
            "Help › About Burrow"
        ),
        // --- end about ---
        // --- browser --- (esc, which closes the pane, is the pane's own key)
        kb!(
            "cmd-shift-b",
            Main,
            ToggleBrowser,
            "Browser pane: close it, or bring back the last page (none yet: open it to type an address)",
            Some("View › Show/Hide Browser Pane")
        ),
        menu_only!(
            OpenBrowser,
            "Browser pane: open it with the address field ready (macOS and Windows)",
            "View › Open Browser…"
        ),
        kb!("cmd-[", Browser, BrowserBack, "Browser pane: back", None),
        kb!(
            "cmd-]",
            Browser,
            BrowserForward,
            "Browser pane: forward",
            None
        ),
        kb!(
            "cmd-r",
            Browser,
            BrowserReload,
            "Browser pane: reload (or stop)",
            None
        ),
        kb!(
            "cmd-l",
            Browser,
            BrowserAddress,
            "Browser pane: edit the address",
            None
        ),
        // --- capture ---
        kb!(
            "cmd-shift-c",
            Browser,
            ClipPage,
            "Browser pane: clip the page (or the passage selected on it) into the draft",
            Some("Post › Clip Page to Draft")
        ),
        // --- end browser ---
        // --- browser macros --- (after Publish's rows: on a tie in depth,
        // the later binding wins)
        kb!(
            "cmd-enter",
            MacroSheet,
            MacroContinue,
            "Macro Preview sheet: continue",
            None
        ),
        kb!(
            "cmd-enter",
            MacroSheetInput,
            MacroContinue,
            "Macro Preview sheet: continue (typing in the text)",
            None
        ),
        kb!(
            "enter",
            MacroSheet,
            MacroPost,
            "Macro Post sheet: post",
            None
        ),
        kb!(
            "escape",
            MacroSheet,
            MacroCancel,
            "Macro Preview or Post sheet: cancel",
            None
        ),
        kb!(
            "escape",
            MacroSheetInput,
            MacroCancel,
            "Macro Preview sheet: cancel (typing in the text)",
            None
        ),
        // --- end browser macros ---
        // --- notes --- (esc, which closes the drawer, is the drawer's own key)
        kb!(
            "cmd-shift-n",
            Main,
            ToggleNotes,
            "Notes pane: open it and move into it, or close it from inside (quotes a selected passage)",
            Some("View › Notes")
        ),
        // Universal quoting: from the browser pane, the reading pane or the
        // drawer, into the draft.
        kb!(
            "cmd-shift-d",
            Main,
            QuoteToDraft,
            "Quote the highlighted passage (a page, a post, your notes, or a reply's preview quote) in the draft",
            Some("Post › Quote Selection in Draft")
        ),
        // --- end notes ---
        // --- extensions --- the commands extensions add, their libraries,
        // and Manage extensions.
        // The commands, libraries and Manage extensions are the command
        // palette's Extensions group too (⇧⌘P).
        menu_only!(
            ShowExtensions,
            "Extensions: run an extension's command, browse its notes, manage them",
            "Post › Extensions…"
        ),
        // --- contacts --- the address book (Studio's Settings › contacts)
        // and "add author to address book"; menu items, no keys of their own.
        menu_only!(
            ShowContacts,
            "Contacts: your address book of @handles (add, edit, check, remove)",
            "Blyg › Contacts…"
        ),
        menu_only!(
            AddAuthorToContacts,
            "Add the author of the post you're reading to your contacts",
            "Post › Add Author to Contacts…"
        ),
        // --- end contacts ---
        // --- palette --- every action, theme, font, blyg, hopper and post
        kb!(
            "cmd-shift-p",
            Main,
            ShowPalette,
            "Command palette: every action, theme, font, blyg, hopper and post, by name",
            Some("View › Command Palette…")
        ),
        // --- focus --- (Ctrl+Shift+F on Windows, see WINDOWS_OVERRIDES)
        kb!(
            "alt-cmd-f",
            Main,
            ToggleFocus,
            "Focus mode: the line you're on stays put, the rest steps back (esc leaves)",
            Some("View › Focus")
        ),
        // --- quiet --- (no key: Settings and the menu)
        menu_only!(
            ToggleQuietWriting,
            "Quiet writing: hide the lineage line, loose ends and Nearby thoughts (or show them)",
            "View › Quiet Writing"
        ),
    ]
}

/// The table as this platform spells it. On Windows GPUI's `cmd` is the
/// Windows key, which the system owns, so every `cmd` becomes `ctrl`.
fn platform_keys(rows: Vec<Keybind>) -> Vec<Keybind> {
    #[cfg(target_os = "windows")]
    {
        let mut rows = drop_windows_keys(rows);
        for k in &mut rows {
            k.key = platform_key(k.key);
            k.menu_key = k.menu_key.map(platform_key);
        }
        rows
    }
    #[cfg(not(target_os = "windows"))]
    rows
}

/// The table as `p` binds it, on any platform: the tests check the
/// Windows keys on macOS too.
#[cfg(test)]
pub fn table_for(p: crate::platform_text::Platform) -> Vec<Keybind> {
    use crate::platform_text::Platform;
    match p {
        Platform::Mac => mac_table(),
        Platform::Windows => {
            let spell = |k: &str| -> &'static str {
                if k.is_empty() {
                    ""
                } else {
                    Box::leak(respell_for_windows(k).into_boxed_str())
                }
            };
            let mut rows = drop_windows_keys(mac_table());
            for k in &mut rows {
                k.key = spell(k.key);
                k.menu_key = k.menu_key.map(spell);
            }
            rows
        }
    }
}

/// Actions with a key on macOS and none on Windows. Discard changes is
/// ⌥⌘Z, which Windows would spell Ctrl+Alt+Z, and Windows reports AltGr as
/// Ctrl+Alt: on layouts where AltGr+Z types a letter (Polish programmer's
/// ż) typing it would open the Discard sheet, whose ⏎ throws the edits
/// away. There it's Menu › Post › Discard Changes… and the toolbar button,
/// as Withdraw is everywhere.
pub const WINDOWS_KEYLESS: &[&str] = &["DiscardChanges"];

/// `action` has no key on this platform although the table gives it one
/// (`WINDOWS_KEYLESS`). The tests' question about bindings; app text asks
/// `keyless_on(action, Platform::spelling())`.
#[cfg(test)]
pub fn keyless_here(action: &str) -> bool {
    keyless_on(action, crate::platform_text::Platform::HERE)
}

/// `action` has no key on `p`. Text that names the key or its stand-in
/// asks with `Platform::spelling()`, so its wording follows the spelling
/// tests pick (`platform_text::spelled_as`); bindings use `keyless_here`.
pub fn keyless_on(action: &str, p: crate::platform_text::Platform) -> bool {
    p == crate::platform_text::Platform::Windows && WINDOWS_KEYLESS.contains(&action)
}

/// The `WINDOWS_KEYLESS` rows without their key: menu (and button) only.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
fn drop_windows_keys(mut rows: Vec<Keybind>) -> Vec<Keybind> {
    for k in &mut rows {
        if WINDOWS_KEYLESS.contains(&k.action) {
            k.key = "";
            k.menu_key = None;
            k.make = None;
        }
    }
    rows
}

/// `cmd-shift-,` → `ctrl-shift-,` (and `ctrl-alt-cmd-t` → `ctrl-alt-t`).
/// Interned, so the table can keep `&'static str` keys.
#[cfg(target_os = "windows")]
pub fn platform_key(key: &'static str) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static SPELLED: OnceLock<Mutex<HashMap<&'static str, &'static str>>> = OnceLock::new();
    if !key.split('-').any(|p| p == "cmd") {
        return key;
    }
    let mut spelled = SPELLED
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    spelled
        .entry(key)
        .or_insert_with(|| Box::leak(respell_for_windows(key).into_boxed_str()))
}

/// Keys whose plain respelling would take a Windows convention: Ctrl+Y is
/// Redo in every Windows text field (and gpui-base's `Input` binds it).
const WINDOWS_OVERRIDES: &[(&str, &str)] = &[
    ("cmd-y", "ctrl-shift-y"),
    // --- focus --- Ctrl+Alt+F is AltGr+F, which types a bracket on some
    // layouts (Hungarian `[`): focus mode is Ctrl+Shift+F there.
    ("alt-cmd-f", "ctrl-shift-f"),
    // --- shelf --- Ctrl+Alt+B is quick capture's default hotkey there, and
    // AltGr+B types `{` on Hungarian layouts: shelving is Ctrl+Shift+A.
    ("alt-cmd-b", "ctrl-shift-a"),
    // --- blyg switcher --- ⌃⌘<digit> would fold into Ctrl+<digit>, the views.
    // Ctrl+Shift+0 never reaches an app on Windows set up with more than one
    // input language (it's the system's language hotkey; GPUI's Windows
    // backend notes it can't be triggered), so the switcher sheet is
    // Ctrl+Shift+L ("list") there; the blygs themselves are Ctrl+Shift+1…9.
    ("ctrl-cmd-0", "ctrl-shift-l"),
    ("ctrl-cmd-1", "ctrl-shift-1"),
    ("ctrl-cmd-2", "ctrl-shift-2"),
    ("ctrl-cmd-3", "ctrl-shift-3"),
    ("ctrl-cmd-4", "ctrl-shift-4"),
    ("ctrl-cmd-5", "ctrl-shift-5"),
    ("ctrl-cmd-6", "ctrl-shift-6"),
    ("ctrl-cmd-7", "ctrl-shift-7"),
    ("ctrl-cmd-8", "ctrl-shift-8"),
    ("ctrl-cmd-9", "ctrl-shift-9"),
    // --- access-ai --- Ctrl+Alt+N is AltGr+N, which types a letter on some
    // layouts (Polish ń): drafting a change note is Ctrl+Shift+E there.
    ("alt-cmd-n", "ctrl-shift-e"),
    // Ctrl+Alt+<key> is AltGr+<key> on Windows, and these type a character
    // on a common layout (`ALTGR_TYPES`): German @ (Q), Polish ł and ś (L,
    // S), Hungarian ł and € (K, U), Hungarian ~ and German ² (1, 2).
    // --- own posts --- Quote into a thread, Link post.
    ("alt-cmd-q", "ctrl-shift-q"),
    ("alt-cmd-l", "ctrl-shift-i"),
    // --- tray --- (Ctrl+Shift+K is the picker's neighbour, ⇧⌘K compose.)
    ("alt-cmd-k", "ctrl-shift-t"),
    // --- nearby ---
    ("alt-cmd-u", "ctrl-shift-u"),
    // --- stream --- and --- reader folders --- the reading screen's layout:
    // Alt alone (Windows has no menu bar for Alt to open).
    ("alt-cmd-1", "alt-1"),
    ("alt-cmd-2", "alt-2"),
    ("alt-cmd-s", "alt-s"),
    // BLYGGER_FAKE's two (Polish ó and ć): function keys type nothing.
    ("ctrl-alt-cmd-o", "ctrl-shift-f11"),
    ("ctrl-alt-cmd-c", "ctrl-shift-f12"),
];

/// The pure part of `platform_key`.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub fn respell_for_windows(key: &str) -> String {
    if let Some((_, win)) = WINDOWS_OVERRIDES.iter().find(|(mac, _)| *mac == key) {
        return win.to_string();
    }
    let (body, minus) = match key.strip_suffix("--") {
        Some(b) => (b, true),
        None => (key, false),
    };
    let mut out: Vec<&str> = Vec::new();
    for p in body.split('-') {
        let p = if p == "cmd" { "ctrl" } else { p };
        if !(matches!(p, "ctrl" | "alt" | "shift") && out.contains(&p)) {
            out.push(p);
        }
    }
    let mut s = out.join("-");
    if minus {
        s.push_str("--");
    }
    s
}

/// Key hints written into the app's text (`"then ⌘G again"`), as this
/// platform spells them: unchanged on macOS; on Windows every run of
/// modifier glyphs and its key is respelled like the bindings
/// (`⇧⌘G` → `Ctrl+Shift+G`, `⌘Y` → `Ctrl+Shift+Y`). Interned, so it can
/// stand wherever the literal did.
pub fn hint(text: &'static str) -> &'static str {
    // Compiled on Windows, and in tests on every platform (a test can ask
    // for Windows' spelling: `platform_text::spelled_as`). A macOS build
    // is just `text`.
    #[cfg(any(target_os = "windows", test))]
    {
        use crate::platform_text::Platform;
        use std::collections::HashMap;
        use std::sync::{Mutex, OnceLock};
        type Spelled = HashMap<(Platform, &'static str), &'static str>;
        let p = Platform::spelling();
        if p == Platform::Mac {
            return text;
        }
        // Keyed by the spelling as well: a test process asks for both, and
        // must never get one platform's entry for the other's text.
        static SPELLED: OnceLock<Mutex<Spelled>> = OnceLock::new();
        let mut spelled = SPELLED
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        spelled
            .entry((p, text))
            .or_insert_with(|| Box::leak(windows_hint(text).into_boxed_str()))
    }
    #[cfg(not(any(target_os = "windows", test)))]
    text
}

/// `hint` for text built at runtime (`format!`).
pub fn hint_owned(text: String) -> String {
    if crate::platform_text::Platform::spelling() == crate::platform_text::Platform::Windows {
        windows_hint(&text)
    } else {
        text
    }
}

/// A piece of app text, read for its keys (`glyph_parts`).
enum GlyphPart {
    Char(char),
    /// Modifiers with no key after them ("⌘-click", "hold ⌘").
    Mods(Vec<&'static str>),
    /// A key, as the table spells it (`alt-cmd-b`; `cmd--` for minus).
    Key(String),
}

/// Split text into its keys (each run of macOS modifier glyphs `⌃⌥⇧⌘` and
/// the key after it) and everything else.
fn glyph_parts(text: &str) -> Vec<GlyphPart> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let mut mods = Vec::new();
        while i < chars.len() {
            match chars[i] {
                '⌃' => mods.push("ctrl"),
                '⌥' => mods.push("alt"),
                '⇧' => mods.push("shift"),
                '⌘' => mods.push("cmd"),
                _ => break,
            }
            i += 1;
        }
        if mods.is_empty() {
            out.push(GlyphPart::Char(chars[i]));
            i += 1;
            continue;
        }
        // The key: a named key, one glyph, or one character; none when the
        // modifiers stand alone ("⌘-click", "hold ⌘").
        let rest: String = chars[i..].iter().collect();
        let (key, used) = if rest.starts_with("Space") {
            (Some("space".to_string()), 5)
        } else if rest.starts_with("esc") {
            (Some("escape".to_string()), 3)
        } else {
            match chars.get(i) {
                Some('⏎') => (Some("enter".into()), 1),
                Some('⌫') => (Some("backspace".into()), 1),
                Some('⇥') => (Some("tab".into()), 1),
                Some('←') => (Some("left".into()), 1),
                Some('→') => (Some("right".into()), 1),
                Some('↑') => (Some("up".into()), 1),
                Some('↓') => (Some("down".into()), 1),
                Some('-') if chars.get(i + 1).is_some_and(|c| c.is_alphabetic()) => (None, 0),
                Some(c) if !c.is_whitespace() && !matches!(c, ')' | '·' | '/') => {
                    (Some(c.to_lowercase().collect()), 1)
                }
                _ => (None, 0),
            }
        };
        out.push(match key {
            Some(key) => GlyphPart::Key(format!("{}-{}", mods.join("-"), key)),
            None => GlyphPart::Mods(mods),
        });
        i += used;
    }
    out
}

/// The pure part of `hint`: rewrite each run of macOS modifier glyphs
/// (`⌃⌥⇧⌘`) and the key after it as Windows writes the respelled binding.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub fn windows_hint(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for part in glyph_parts(text) {
        match part {
            GlyphPart::Char(c) => out.push(c),
            // Modifiers alone: name them the Windows way.
            GlyphPart::Mods(mods) => {
                let names: Vec<&str> = mods
                    .iter()
                    .map(|m| match *m {
                        "cmd" | "ctrl" => "Ctrl",
                        "alt" => "Alt",
                        _ => "Shift",
                    })
                    .collect();
                out.push_str(&names.join("+"));
            }
            GlyphPart::Key(mac) => out.push_str(&windows_label(&respell_for_windows(&mac))),
        }
    }
    windows_words(&out)
}

/// The keys `text` (as `p` shows it) names, as `p`'s table spells them:
/// `alt-cmd-b` for "⌥⌘B" on macOS, `ctrl-shift-a` for "Ctrl+Shift+A" on
/// Windows. Modifiers with no key ("⌘-click") name none.
#[cfg(test)]
pub fn hint_keys(text: &str, p: crate::platform_text::Platform) -> Vec<String> {
    use crate::platform_text::Platform;
    if p == Platform::Mac {
        return glyph_parts(text)
            .into_iter()
            .filter_map(|part| match part {
                GlyphPart::Key(k) => Some(k),
                _ => None,
            })
            .collect();
    }
    const MODS: [(&str, &str); 4] = [
        ("Ctrl+", "ctrl"),
        ("Alt+", "alt"),
        ("Shift+", "shift"),
        ("Win+", "cmd"),
    ];
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = MODS.iter().filter_map(|(s, _)| rest.find(s)).min() {
        let boundary = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let mut s = &rest[at..];
        let mut mods = Vec::new();
        while let Some((name, m)) = MODS.iter().find(|(name, _)| s.starts_with(name)) {
            mods.push(*m);
            s = &s[name.len()..];
        }
        let word: String = s.chars().take_while(|c| c.is_alphanumeric()).collect();
        let (key, used) = if word.is_empty() {
            match s.chars().next() {
                Some(c) if !c.is_whitespace() => (c.to_string(), c.len_utf8()),
                _ => (String::new(), 0),
            }
        } else {
            let k = word.to_lowercase();
            (if k == "esc" { "escape".into() } else { k }, word.len())
        };
        if boundary && !key.is_empty() && key != "click" {
            out.push(format!("{}-{key}", mods.join("-")));
        }
        rest = &s[used..];
    }
    out
}

/// A key written in macOS terms, as `p`'s table spells it.
#[cfg(test)]
pub fn respell_for(key: &str, p: crate::platform_text::Platform) -> String {
    match p {
        crate::platform_text::Platform::Mac => key.to_string(),
        crate::platform_text::Platform::Windows => respell_for_windows(key),
    }
}

/// Mac words in the app's text, as Windows says them, longest first so a
/// phrase wins over the word inside it.
const WINDOWS_WORDS: &[(&str, &str)] = &[
    ("your macOS Keychain", "Windows Credential Manager"),
    ("the macOS Keychain", "Windows Credential Manager"),
    ("macOS Keychain", "Windows Credential Manager"),
    ("your Keychain", "Credential Manager"),
    ("the Keychain", "Credential Manager"),
    ("Keychain", "Credential Manager"),
    ("follow macOS", "follow Windows"),
    ("while macOS is dark", "while Windows is dark"),
    ("your other Macs", "your other PCs"),
    ("this Mac", "this PC"),
    ("your Mac", "your PC"),
    ("another Mac", "another PC"),
    // Windows has no menu bar; the Menu button lists the same items.
    ("the Burrow menu", "the Menu button"),
    ("the Blygger menu", "the Menu button"),
];

/// The wording half of `windows_hint`: Keychain → Credential Manager,
/// this Mac → this PC.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub fn windows_words(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'scan: while !rest.is_empty() {
        for (mac, win) in WINDOWS_WORDS {
            if let Some(after) = rest.strip_prefix(mac) {
                out.push_str(win);
                rest = after;
                continue 'scan;
            }
        }
        let c = rest.chars().next().expect("not empty");
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Tests write keystrokes in macOS terms, like the table. On Windows they
/// are respelled exactly as the bindings are (`respell_for_windows`).
#[cfg(test)]
pub fn keys(keystrokes: &str) -> String {
    if cfg!(target_os = "windows") {
        keystrokes
            .split(' ')
            .map(respell_for_windows)
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        keystrokes.to_string()
    }
}

/// One binding, its keys read through `mapper`.
fn binding<A: Action>(
    key: &str,
    action: A,
    context: Option<&str>,
    mapper: &dyn PlatformKeyboardMapper,
) -> KeyBinding {
    let predicate = context.map(|c| {
        KeyBindingContextPredicate::parse(c)
            .unwrap_or_else(|e| panic!("context {c}: {e}"))
            .into()
    });
    KeyBinding::load(key, Box::new(action), predicate, false, None, mapper)
        .unwrap_or_else(|e| panic!("key {key}: {e}"))
}

/// Register every binding. Call after `gpui_kit::init`, so that ours are
/// added later and win ties at the same depth.
///
/// On Windows the keys go through the platform's keyboard mapper. Windows
/// reports Shift with a digit or punctuation key as the character it types
/// on the current layout, without Shift (Ctrl+Shift+1 is `ctrl-!` on a US
/// layout, `ctrl-1` on AZERTY), so a plain `ctrl-shift-1` binding never
/// matches; the mapper spells each binding the way that layout reports it.
/// macOS reports the key itself, so it keeps the keys as written.
pub fn bind_keys(cx: &mut App) {
    // Both arms compile everywhere, so macOS builds check the Windows one.
    let mapper: std::rc::Rc<dyn PlatformKeyboardMapper> = if cfg!(target_os = "windows") {
        cx.keyboard_mapper().clone()
    } else {
        std::rc::Rc::new(DummyKeyboardMapper)
    };
    let mapper = &*mapper;
    let table = table();
    let mut out = Vec::new();
    // Menu-equivalent shims first: the native menu uses the *first* binding
    // of an action when none matches its default context.
    for k in &table {
        if let (Some(mk), Some(make)) = (k.menu_key, k.make) {
            out.push(make(mk, Some(MENU_ONLY), mapper));
        }
    }
    for k in &table {
        if let Some(make) = k.make {
            out.push(make(k.key, k.scope.context(), mapper));
        }
    }
    cx.bind_keys(out);
    // Menu-only actions have no keys; keep the compiler honest that they exist.
    let _ = (Disconnect, OpenConfigFile, SiteSettings, SubscribeTo);
    let _ = (ImportOpml, ExportOpml); // --- OPML ---
    let _ = ShowDiscover; // --- discover ---
    let _ = ShowTutorial; // --- onboarding ---
    let _ = MyProfile; // --- profiles ---
    let _ = Withdraw; // --- delete & withdraw ---
    let _ = ToggleSpellcheck; // --- composer ---
    let _ = (AddBlyg, ConnectBlyg, ManageBlygs); // --- blyg switcher ---
    let _ = ShowExtensions; // --- palette --- (Post › Extensions…)
    let _ = (ShowClientAccess, ShowServerAi); // --- access-ai ---
    let _ = ShowSite; // --- site ---
    let _ = (ShowStaleQuotes, ShowSignals, CopyLink); // --- upkeep ---
    let _ = ShowCopiesElsewhere; // --- syndication ---
}

/// The key `action` has on this platform, as glyphs (`⌃⌘1`, or
/// `Ctrl+Shift+1` on Windows); `None` when it has none here.
pub fn action_glyphs(action: &str) -> Option<String> {
    table()
        .into_iter()
        .find(|k| k.action == action && !k.key.is_empty())
        .map(|k| glyphs(k.key))
}

/// `Ctrl+Shift+,` for `ctrl-shift-,` (Windows spells keys out).
#[cfg(target_os = "windows")]
pub fn glyphs(key: &str) -> String {
    if key.is_empty() {
        return "—".into();
    }
    windows_label(key)
}

/// A key as Windows menus and tooltips write it: `Ctrl+Shift+Enter`.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub fn windows_label(key: &str) -> String {
    let (mods, last) = normalize(key);
    let mut parts: Vec<String> = [
        ("ctrl", "Ctrl"),
        ("alt", "Alt"),
        ("shift", "Shift"),
        ("cmd", "Win"),
    ]
    .iter()
    .filter(|(m, _)| mods.contains(m))
    .map(|(_, name)| name.to_string())
    .collect();
    parts.push(match last.as_str() {
        "enter" => "Enter".into(),
        "space" => "Space".into(),
        "escape" => "Esc".into(),
        "tab" => "Tab".into(),
        "backspace" => "Backspace".into(),
        "up" | "down" | "left" | "right" => {
            let mut c = last.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        }
        k => k.to_uppercase(),
    });
    parts.join("+")
}

/// `⌘⇧,` for `cmd-shift-,`.
#[cfg(not(target_os = "windows"))]
pub fn glyphs(key: &str) -> String {
    mac_glyphs(key)
}

/// `⌘⇧,` for `cmd-shift-,` on every platform (the documentation site's
/// shortcut reference describes the macOS app).
#[cfg_attr(all(target_os = "windows", not(test)), allow(dead_code))]
pub fn mac_glyphs(key: &str) -> String {
    if key.is_empty() {
        return "—".into();
    }
    let mut mods = String::new();
    let mut last = "";
    let parts: Vec<&str> = match key.strip_suffix("--") {
        Some(stripped) => {
            let mut p: Vec<&str> = stripped.split('-').collect();
            p.push("-");
            p
        }
        None => key.split('-').collect(),
    };
    for p in &parts {
        match *p {
            "ctrl" => mods.push('⌃'),
            "alt" => mods.push('⌥'),
            "shift" => mods.push('⇧'),
            "cmd" => mods.push('⌘'),
            k => last = k,
        }
    }
    // macOS order: ⌃⌥⇧⌘
    let order = ['⌃', '⌥', '⇧', '⌘'];
    let mut sorted: String = order.iter().filter(|c| mods.contains(**c)).collect();
    let k = match last {
        "enter" => "⏎".to_string(),
        "space" => "Space".to_string(),
        "escape" => "esc".to_string(),
        "tab" => "⇥".to_string(),
        "backspace" => "⌫".to_string(),
        k => k.to_uppercase(),
    };
    sorted.push_str(&k);
    sorted
}

// --- buttons ---

/// The title-bar toolbar (`show-buttons`): groups of actions, left to right.
/// Each must be a table row with a `button`.
pub const TOOLBAR: &[&[&str]] = &[
    &["NewDraft", "MakeDraft", "Publish"],
    &["ViewWrite", "ViewSplit", "ViewStudio"],
    &["ShowVersions", "AiGenerate"],
    // One slot: Delete for a draft or scratch note, Withdraw for a published
    // post (`toolbar::visible`). Discard changes sits beside it: it drops
    // edits, where Delete and Withdraw drop the post.
    &["DiscardChanges", "DeleteDraft", "Withdraw"],
    &["ShowCapture"],
];

/// Quick capture's row (`show-buttons`): the action and the panel's own,
/// shorter, label. Each must be a table row with a key.
pub const CAPTURE_ROW: &[(&str, &str)] = &[
    ("KeepCapture", "Scratch"),
    ("MakeDraft", "Draft"),
    ("Publish", "Publish"),
];

/// The row a button comes from: the first row of `action` that has a
/// button (or, failing that, a key).
pub fn button_row(action: &str) -> Option<Keybind> {
    let rows: Vec<Keybind> = table().into_iter().filter(|k| k.action == action).collect();
    let pick = rows
        .iter()
        .position(|k| k.button.is_some())
        .or_else(|| rows.iter().position(|k| !k.key.is_empty()))?;
    rows.into_iter().nth(pick)
}

/// The key a button teaches: the row's key, or for quick capture the
/// configured `capture-hotkey`.
pub fn shortcut(k: &Keybind, capture_hotkey: &str) -> String {
    match (k.key, k.action) {
        ("", "ShowCapture") => crate::prefs::hotkey_glyphs(capture_hotkey),
        ("", _) => String::new(),
        (key, _) => glyphs(key),
    }
}

/// A button's tooltip: "Publish  ⌘⏎".
pub fn tooltip(label: &str, k: &Keybind, capture_hotkey: &str) -> String {
    match shortcut(k, capture_hotkey) {
        s if s.is_empty() => label.to_string(),
        s => format!("{label}  {s}"),
    }
}
// --- end buttons ---

/// Normalise a key in either our syntax (`cmd-shift-g`) or the capture
/// hotkey's (`ctrl+alt+B`) to a comparable (modifiers, key) pair.
pub fn normalize(key: &str) -> (Vec<&'static str>, String) {
    let key = key.trim();
    let parts: Vec<String> = if let Some(stripped) = key.strip_suffix("--") {
        let mut p: Vec<String> = stripped.split(['-', '+']).map(str::to_string).collect();
        p.push("-".into());
        p
    } else {
        key.split(['-', '+']).map(str::to_string).collect()
    };
    let mut mods = Vec::new();
    let mut last = String::new();
    for p in parts {
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods.push("ctrl"),
            "alt" | "option" | "opt" => mods.push("alt"),
            "shift" => mods.push("shift"),
            "cmd" | "command" | "super" | "meta" => mods.push("cmd"),
            k => {
                // global-hotkey spells letters `KeyB`, digits `Digit1`.
                let k = k
                    .strip_prefix("key")
                    .filter(|r| r.len() == 1)
                    .or_else(|| k.strip_prefix("digit").filter(|r| r.len() == 1))
                    .unwrap_or(k);
                last = match k {
                    "return" => "enter".into(),
                    "esc" => "escape".into(),
                    k => k.to_string(),
                };
            }
        }
    }
    mods.sort_unstable();
    mods.dedup();
    (mods, last)
}

/// The app binding (if any) the capture hotkey `hotkey` would shadow.
pub fn clash_with_hotkey(hotkey: &str) -> Option<Keybind> {
    let want = normalize(hotkey);
    table()
        .into_iter()
        .find(|k| !k.key.is_empty() && normalize(k.key) == want)
}

/// Windows shortcuts the system (or every Windows app) owns, in the
/// respelled form `table()` produces there.
#[cfg(all(test, target_os = "windows"))]
pub const RESERVED: &[(&str, &str, Option<&str>)] = WINDOWS_RESERVED;
#[cfg(test)]
pub const WINDOWS_RESERVED: &[(&str, &str, Option<&str>)] = &[
    ("ctrl-q", "Quit", Some("Quit")),
    ("alt-f4", "Close window", None),
    ("alt-tab", "App switcher", None),
    ("alt-shift-tab", "App switcher", None),
    ("ctrl-escape", "Start menu", None),
    ("ctrl-shift-escape", "Task Manager", None),
    ("ctrl-alt-delete", "Security screen", None),
    ("alt-space", "Window menu", None),
    ("ctrl-c", "Copy", None),
    ("ctrl-v", "Paste", None),
    ("ctrl-x", "Cut", None),
    ("ctrl-z", "Undo", None),
    ("ctrl-y", "Redo", None),
    ("ctrl-shift-z", "Redo", None),
    ("ctrl-a", "Select all", None),
    // The input-language hotkey on many setups; GPUI never sees it.
    ("ctrl-shift-0", "Switch input language", None),
];

/// macOS shortcuts the system (or every Mac app) owns. An entry's action is
/// the only one of ours allowed on that key (⌘Q is our Quit).
#[cfg(all(test, not(target_os = "windows")))]
pub const RESERVED: &[(&str, &str, Option<&str>)] = &[
    ("cmd-q", "Quit", Some("Quit")),
    ("cmd-w", "Close window", None),
    ("cmd-h", "Hide app", None),
    ("cmd-alt-h", "Hide others", None),
    ("cmd-m", "Minimize", None),
    ("cmd-tab", "App switcher", None),
    ("cmd-shift-tab", "App switcher", None),
    ("cmd-`", "Cycle windows", None),
    ("cmd-space", "Spotlight", None),
    ("ctrl-cmd-space", "Emoji & symbols", None),
    ("cmd-alt-escape", "Force quit", None),
    ("ctrl-cmd-q", "Lock screen", None),
    ("ctrl-cmd-f", "Full screen", None),
    ("cmd-shift-3", "Screenshot", None),
    ("cmd-shift-4", "Screenshot", None),
    ("cmd-shift-5", "Screenshot", None),
    ("cmd-alt-d", "Dock", None),
    ("cmd-shift-/", "Help menu", None),
    ("cmd-c", "Copy", None),
    ("cmd-v", "Paste", None),
    ("cmd-x", "Cut", None),
    ("cmd-z", "Undo", None),
    ("cmd-shift-z", "Redo", None),
    ("cmd-a", "Select all", None),
];

/// Toolbar buttons with no key on purpose: Withdraw is permanent, so it's
/// never one keystroke away (docs/SPEC.md § Buttons).
#[cfg(test)]
pub const KEYLESS_BUTTONS: &[&str] = &["Withdraw"];

/// Named keys the native menu does turn into their key equivalent
/// (gpui-pre-macos `key_to_native`), unlike "enter".
#[cfg(all(test, not(target_os = "windows")))]
pub const NATIVE_MENU_KEYS: &[&str] = &["backspace"];

/// Keys we bind on purpose over one of gpui-base's own `Input` bindings.
#[cfg(all(test, not(target_os = "windows")))]
pub const OUTRANKS_INPUT: &[&str] = &["cmd-enter"];
#[cfg(all(test, target_os = "windows"))]
pub const OUTRANKS_INPUT: &[&str] = &["ctrl-enter"];

/// `blygger +list-keybinds`.
pub fn list() -> String {
    let mut out = String::from("# key        action              where        what\n");
    for k in table() {
        if k.key.is_empty() && k.menu.is_none() {
            continue;
        }
        let place = match k.scope {
            Scope::Global => "anywhere",
            Scope::Main => "main window",
            Scope::MainInput => "text fields",
            Scope::Browser => "browser", // --- browser ---
            Scope::MacroSheet | Scope::MacroSheetInput => "macro sheet",
        };
        let mut line = format!(
            "{:<12} {:<19} {:<12} {}",
            glyphs(k.key),
            k.action,
            place,
            hint(k.label)
        );
        if let Some(m) = k.menu {
            line.push_str(&format!("  [{m}]"));
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform_text::Platform;
    use std::collections::HashMap;

    /// Keys that type a character with AltGr on a common Windows layout.
    /// Windows reports AltGr as Ctrl+Alt, so a Ctrl+Alt+<key> binding eats
    /// that character there. (key, layout, what AltGr+key types)
    const ALTGR_TYPES: &[(&str, &str, &str)] = &[
        // Polish (programmer's), the default Polish layout.
        ("a", "Polish", "ą"),
        ("c", "Polish", "ć"),
        ("e", "Polish", "ę"),
        ("l", "Polish", "ł"),
        ("n", "Polish", "ń"),
        ("o", "Polish", "ó"),
        ("s", "Polish", "ś"),
        ("x", "Polish", "ź"),
        ("z", "Polish", "ż"),
        // German.
        ("q", "German", "@"),
        ("m", "German", "µ"),
        ("2", "German", "²"),
        ("3", "German", "³"),
        ("7", "German", "{"),
        ("8", "German", "["),
        ("9", "German", "]"),
        ("0", "German", "}"),
        // French (AZERTY).
        ("4", "French", "{"),
        ("5", "French", "["),
        ("6", "French", "|"),
        // Hungarian.
        ("1", "Hungarian", "~"),
        ("b", "Hungarian", "{"),
        ("f", "Hungarian", "["),
        ("g", "Hungarian", "]"),
        ("k", "Hungarian", "ł"),
        ("u", "Hungarian", "€"),
        ("v", "Hungarian", "@"),
        ("w", "Hungarian", "|"),
    ];

    fn windows_live() -> Vec<Keybind> {
        table_for(Platform::Windows)
            .into_iter()
            .filter(|k| !k.key.is_empty())
            .collect()
    }

    /// Checked on every platform, so a Mac-only change can't ship a Windows
    /// key that eats a character (⌥⌘B shipped as Ctrl+Alt+B, AltGr+B's `{`,
    /// until 2bbe972; ⌥⌘Z needed WINDOWS_KEYLESS, 4b0b9d9).
    #[test]
    fn no_windows_key_is_an_altgr_character() {
        let mut bad = Vec::new();
        for k in windows_live() {
            let (mods, last) = normalize(k.key);
            if !(mods.contains(&"ctrl") && mods.contains(&"alt")) {
                continue;
            }
            for (key, layout, ch) in ALTGR_TYPES.iter().filter(|(c, _, _)| *c == last) {
                bad.push(format!(
                    "  {} ({}) is {} on Windows: AltGr+{} types {ch} on {layout} layouts",
                    k.action,
                    mac_glyphs(mac_key(k.action)),
                    windows_label(k.key),
                    key.to_uppercase()
                ));
            }
        }
        assert!(
            bad.is_empty(),
            "Windows keys that AltGr typing would trigger:\n{}\nGive each a Windows key \
             in keymap::WINDOWS_OVERRIDES (or list the action in WINDOWS_KEYLESS).",
            bad.join("\n")
        );
    }

    fn mac_key(action: &str) -> &'static str {
        mac_table()
            .into_iter()
            .find(|k| k.action == action)
            .map_or("", |k| k.key)
    }

    /// The overrides respell keys that exist, and the Windows table they
    /// make has no clashes: not between our actions, not with the default
    /// quick-capture hotkey, not with keys Windows keeps for itself.
    #[test]
    fn windows_overrides_are_real_and_clash_with_nothing() {
        let mac = mac_table();
        for (from, to) in WINDOWS_OVERRIDES {
            assert!(
                mac.iter().any(|k| k.key == *from),
                "WINDOWS_OVERRIDES respells {from}, which no binding has: drop it"
            );
            assert!(
                !to.split('-').any(|p| p == "cmd"),
                "{from} → {to}: Windows has no ⌘"
            );
            let n = WINDOWS_OVERRIDES.iter().filter(|(f, _)| f == from).count();
            assert_eq!(n, 1, "{from} is overridden {n} times");
        }
        for a in WINDOWS_KEYLESS {
            assert!(
                mac.iter().any(|k| k.action == *a),
                "WINDOWS_KEYLESS: no {a}"
            );
        }
        let mut bad = Vec::new();
        let mut seen: HashMap<(&str, (Vec<&str>, String)), &str> = HashMap::new();
        let hotkey = normalize(crate::prefs::DEFAULT_HOTKEY);
        for k in windows_live() {
            let n = normalize(k.key);
            let slot = (k.scope.place(), n.clone());
            if let Some(prev) = seen.get(&slot)
                && *prev != k.action
            {
                bad.push(format!("  {} is both {prev} and {}", k.key, k.action));
            }
            seen.insert(slot, k.action);
            if n == hotkey {
                bad.push(format!(
                    "  {} ({}) is quick capture's default hotkey",
                    k.key, k.action
                ));
            }
            if let Some((r, what, allowed)) =
                WINDOWS_RESERVED.iter().find(|(r, _, _)| normalize(r) == n)
                && *allowed != Some(k.action)
            {
                bad.push(format!(
                    "  {} ({}) is Windows' {what} ({r})",
                    k.key, k.action
                ));
            }
        }
        assert!(
            bad.is_empty(),
            "the Windows key table clashes:\n{}\nPick another Windows key in \
             keymap::WINDOWS_OVERRIDES.",
            bad.join("\n")
        );
    }

    fn live() -> Vec<Keybind> {
        table().into_iter().filter(|k| !k.key.is_empty()).collect()
    }

    #[test]
    fn no_duplicate_keys_in_one_place() {
        let mut seen: HashMap<(&str, (Vec<&str>, String)), &str> = HashMap::new();
        for k in live() {
            // Main and MainInput are the same place; the MainInput row is the
            // deliberate twin of a Main row with the same action.
            let slot = (k.scope.place(), normalize(k.key));
            if let Some(prev) = seen.get(&slot) {
                assert_eq!(
                    *prev, k.action,
                    "{} is bound to both {prev} and {}",
                    k.key, k.action
                );
            }
            seen.insert(slot, k.action);
        }
    }

    #[test]
    fn nothing_shadows_or_is_shadowed_by_gpui_base_inputs() {
        // gpui-base's Input bindings, as registered by gpui_kit::init.
        let input_keys = {
            let cx = gpui_kit::TestAppContext::single();
            cx.update(|cx| {
                gpui_kit::init(cx);
                let km = cx.key_bindings();
                let km = km.borrow();
                km.bindings()
                    .filter(|b| {
                        b.predicate()
                            .is_some_and(|p| format!("{p}").contains("Input"))
                    })
                    .filter_map(|b| {
                        let ks = b.keystrokes();
                        (ks.len() == 1).then(|| ks[0].unparse())
                    })
                    .map(|s| normalize(&s))
                    .collect::<Vec<_>>()
            })
        };
        assert!(!input_keys.is_empty(), "read gpui-base's bindings");
        // --- browser --- The pane's keys (⌘[ ⌘] ⌘R ⌘L) yield to its address
        // field on purpose while you type an address, so they're exempt.
        for k in live()
            .iter()
            // --- browser macros --- The sheets' keys out-rank Input on
            // purpose (their own `MacroSheet > Input` rows).
            .filter(|k| {
                !matches!(
                    k.scope,
                    Scope::Global | Scope::Browser | Scope::MacroSheet | Scope::MacroSheetInput
                )
            })
        {
            let shadowed = input_keys.contains(&normalize(k.key));
            let deliberate = OUTRANKS_INPUT.contains(&k.key);
            assert!(
                !shadowed || deliberate,
                "{} ({}) is also a gpui-base Input key: it would stop working while a text \
                 field has focus. Pick another key, or out-rank Input and list it in OUTRANKS_INPUT",
                k.key,
                k.action
            );
            if deliberate {
                // Out-ranking needs a MainInput binding for the same key.
                assert!(
                    live()
                        .iter()
                        .any(|o| o.key == k.key && o.scope == Scope::MainInput),
                    "{} needs a MainInput binding to out-rank Input",
                    k.key
                );
            }
        }
    }

    #[test]
    fn capture_hotkey_is_free() {
        assert!(
            clash_with_hotkey(crate::prefs::DEFAULT_HOTKEY).is_none(),
            "the default quick-capture hotkey clashes with an app binding"
        );
        // Windows respells ⌘ as Ctrl (`platform_keys`).
        let cmd = if cfg!(target_os = "windows") {
            "ctrl"
        } else {
            "cmd"
        };
        assert_eq!(
            clash_with_hotkey(&format!("{cmd}+T")).map(|k| k.action),
            Some("ToggleKind")
        );
        assert_eq!(
            clash_with_hotkey(&format!("{}+KeyL", cmd.to_uppercase())).map(|k| k.action),
            Some("FocusSearch")
        );
    }

    #[test]
    fn no_reserved_macos_shortcuts() {
        for k in live() {
            if let Some((_, what, allowed)) = RESERVED
                .iter()
                .find(|(r, _, _)| normalize(r) == normalize(k.key))
            {
                assert_eq!(
                    Some(k.action),
                    *allowed,
                    "{} is macOS's {what}; don't bind {} to it",
                    k.key,
                    k.action
                );
            }
        }
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn planned_keys_are_reserved() {
        assert!(table().iter().any(|k| k.key == "cmd-l"));
        // ⌘Y graduated from planned to the version browser.
        assert!(
            table()
                .iter()
                .any(|k| k.key == "cmd-y" && k.action == "ShowVersions")
        );
        // ⌘. was pressed by a tester expecting something; it stays unbound.
        assert!(!table().iter().any(|k| k.key == "cmd-."));
    }

    #[test]
    fn discard_changes_has_no_key_on_windows() {
        // Every platform checks the Windows table's one exception.
        let mac = mac_table();
        let win = drop_windows_keys(mac_table());
        assert_eq!(WINDOWS_KEYLESS, &["DiscardChanges"]);
        let row = |rows: &[Keybind], a: &str| {
            rows.iter()
                .position(|k| k.action == a)
                .unwrap_or_else(|| panic!("{a}"))
        };
        let d = row(&win, "DiscardChanges");
        assert_eq!(mac[d].key, "alt-cmd-z", "⌥⌘Z on macOS");
        assert!(win[d].key.is_empty() && win[d].make.is_none() && win[d].menu_key.is_none());
        assert_eq!(
            win[d].menu,
            Some("Post › Discard Changes…"),
            "still in the menu"
        );
        assert_eq!(win[d].button, Some("Discard"), "and on the toolbar");
        // Nothing else lost its key.
        for (m, w) in mac.iter().zip(&win) {
            if m.action != "DiscardChanges" {
                assert_eq!(m.key, w.key, "{}", m.action);
            }
        }
        assert_eq!(keyless_here("DiscardChanges"), cfg!(target_os = "windows"));
        assert!(!keyless_here("Publish"));
    }

    #[test]
    fn keys_respell_for_windows() {
        assert_eq!(respell_for_windows("cmd-l"), "ctrl-l");
        assert_eq!(respell_for_windows("cmd-shift-,"), "ctrl-shift-,");
        assert_eq!(respell_for_windows("ctrl-alt-cmd-t"), "ctrl-alt-t");
        assert_eq!(respell_for_windows("cmd--"), "ctrl--");
        assert_eq!(respell_for_windows("cmd-shift--"), "ctrl-shift--");
        assert_eq!(respell_for_windows("alt-up"), "alt-up");
        // Ctrl+Y is Redo on Windows, so Versions moves over.
        assert_eq!(respell_for_windows("cmd-y"), "ctrl-shift-y");
        // --- blyg switcher --- Ctrl+Shift+<digit> for the blygs; the sheet
        // can't be Ctrl+Shift+0 (the system's input-language key).
        assert_eq!(respell_for_windows("ctrl-cmd-1"), "ctrl-shift-1");
        assert_eq!(respell_for_windows("ctrl-cmd-9"), "ctrl-shift-9");
        assert_eq!(respell_for_windows("ctrl-cmd-0"), "ctrl-shift-l");
        assert_eq!(windows_label("ctrl-shift-1"), "Ctrl+Shift+1");
        assert_eq!(windows_label("ctrl-shift-,"), "Ctrl+Shift+,");
        assert_eq!(windows_label("ctrl-enter"), "Ctrl+Enter");
        assert_eq!(windows_label("ctrl--"), "Ctrl+-");
        assert_eq!(windows_label("ctrl+alt+b"), "Ctrl+Alt+B");
        assert_eq!(windows_label("alt-up"), "Alt+Up");
    }

    #[test]
    fn hints_respell_for_windows() {
        assert_eq!(windows_hint("then ⌘G again"), "then Ctrl+G again");
        assert_eq!(windows_hint("⇧⌘G"), "Ctrl+Shift+G");
        assert_eq!(windows_hint("⌘⏎ publishes"), "Ctrl+Enter publishes");
        assert_eq!(windows_hint("Settings (⌘,)"), "Settings (Ctrl+,)");
        assert_eq!(windows_hint("⌘Y versions"), "Ctrl+Shift+Y versions");
        assert_eq!(windows_hint("⌃⌥B"), "Ctrl+Alt+B");
        assert_eq!(windows_hint("⇧⌘Space"), "Ctrl+Shift+Space");
        assert_eq!(windows_hint("⌘-click"), "Ctrl-click");
        assert_eq!(windows_hint("hold ⌘ and"), "hold Ctrl and");
        assert_eq!(windows_hint("⌘1 · ⌘2"), "Ctrl+1 · Ctrl+2");
        assert_eq!(windows_hint("no keys here ⏎"), "no keys here ⏎");
        assert_eq!(
            windows_hint("The token is kept in your macOS Keychain; the address"),
            "The token is kept in Windows Credential Manager; the address"
        );
        assert_eq!(
            windows_hint("saved in your Keychain"),
            "saved in Credential Manager"
        );
        assert_eq!(
            windows_hint("scratch · only on this Mac"),
            "scratch · only on this PC"
        );
        assert_eq!(
            windows_hint("Posts you read here still show as unread on your other Macs."),
            "Posts you read here still show as unread on your other PCs."
        );
        assert_eq!(windows_hint("Macintosh"), "Macintosh");
        // macOS text is untouched; Windows' is respelled, owned or not.
        use crate::platform_text::{Platform, spelled_as};
        spelled_as(Platform::Mac, || {
            assert_eq!(hint("then ⌘G"), "then ⌘G");
            assert_eq!(hint_owned("then ⌘G".into()), "then ⌘G");
        });
        spelled_as(Platform::Windows, || {
            assert_eq!(hint("then ⌘G"), "then Ctrl+G");
            assert_eq!(hint_owned("then ⌘G".into()), "then Ctrl+G");
        });
    }

    #[test]
    fn hint_keys_reads_both_spellings() {
        assert_eq!(
            hint_keys("⌥⌘B shelves, ⌘-click picks, ⇧⌘, and ⌘⏎", Platform::Mac),
            ["alt-cmd-b", "shift-cmd-,", "cmd-enter"]
        );
        assert_eq!(
            hint_keys(
                "Ctrl+Shift+A shelves, Ctrl-click and Ctrl+click pick, Ctrl+, and Alt+Up, Esc+X",
                Platform::Windows
            ),
            ["ctrl-shift-a", "ctrl-,", "alt-up"]
        );
    }

    /// On Windows every key is respelled, `⌘` never survives, and the
    /// table still has no clashes (`no_duplicate_keys_in_one_place`).
    #[test]
    #[cfg(target_os = "windows")]
    fn windows_keys_use_ctrl() {
        assert!(live().iter().all(|k| !k.key.split('-').any(|p| p == "cmd")));
        assert!(table().iter().any(|k| k.key == "ctrl-l"));
        assert!(
            table()
                .iter()
                .any(|k| k.key == "ctrl-shift-y" && k.action == "ShowVersions")
        );
        assert_eq!(glyphs("ctrl-enter"), "Ctrl+Enter");
        assert!(list().contains("Publish"));
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn glyphs_and_listing() {
        assert_eq!(glyphs("cmd-shift-,"), "⇧⌘,");
        assert_eq!(glyphs("cmd-enter"), "⌘⏎");
        assert_eq!(glyphs("ctrl-alt-cmd-o"), "⌃⌥⌘O");
        assert_eq!(glyphs("cmd--"), "⌘-");
        assert_eq!(glyphs("cmd-shift-backspace"), "⇧⌘⌫");
        let l = list();
        assert!(l.contains("⌘⏎") && l.contains("Publish"), "{l}");
        assert_eq!(
            normalize("ctrl+alt+B"),
            (vec!["alt", "ctrl"], "b".to_string())
        );
        assert_eq!(normalize("cmd--"), (vec!["cmd"], "-".to_string()));
    }

    // --- buttons ---

    /// Buttons come from the table: every toolbar / capture-row action is a
    /// row with a button (or key), its action is really bound to its key, a
    /// click builds the same action, and the tooltip teaches the key.
    #[test]
    fn every_button_is_a_bound_action_with_its_key_in_the_tooltip() {
        let cx = gpui_kit::TestAppContext::single();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_keys(cx);
            let km = cx.key_bindings();
            let km = km.borrow();
            let toolbar = TOOLBAR.iter().flat_map(|g| g.iter().copied());
            let capture = CAPTURE_ROW.iter().map(|(a, _)| *a);
            let mut seen = 0;
            for action in toolbar.chain(capture) {
                let k = button_row(action).unwrap_or_else(|| panic!("{action}: no row"));
                seen += 1;
                let make = k
                    .new_action
                    .unwrap_or_else(|| panic!("{action}: no action"));
                let built = make();
                assert_eq!(
                    built.name().rsplit("::").next(),
                    Some(action),
                    "{action}'s button builds {}",
                    built.name()
                );
                if TOOLBAR.iter().any(|g| g.contains(&action)) {
                    assert!(k.button.is_some() && k.icon.is_some(), "{action}");
                    assert!(
                        crate::app::toolbar::icon_svg(k.icon.unwrap()).is_some(),
                        "{action}: icon {:?} isn't bundled",
                        k.icon
                    );
                }
                let tip = tooltip(k.button.unwrap_or(action), &k, "ctrl+alt+b");
                if k.key.is_empty() {
                    if action == "ShowCapture" {
                        // Quick capture: the configured global hotkey.
                        let want = if cfg!(target_os = "windows") {
                            "  Ctrl+Alt+B"
                        } else {
                            "  ⌃⌥B"
                        };
                        assert!(tip.ends_with(want), "{tip}");
                    } else {
                        // Keyless on purpose (Withdraw is irreversible), or
                        // only on Windows (Discard's Ctrl+Alt+Z is AltGr+Z):
                        // menu only, and the tooltip shows no key.
                        assert!(
                            KEYLESS_BUTTONS.contains(&action) || keyless_here(action),
                            "{action} has no key"
                        );
                        assert!(k.menu.is_some(), "{action}: a keyless button needs a menu");
                        assert_eq!(tip, k.button.unwrap(), "{tip}");
                    }
                    continue;
                }
                assert!(tip.contains(&glyphs(k.key)), "{tip} lacks {}", k.key);
                // Really bound: the key, in the keymap, runs this action.
                let bound = km.bindings_for_action(&*built).any(|b| {
                    let ks = b.keystrokes();
                    ks.len() == 1 && normalize(&ks[0].unparse()) == normalize(k.key)
                });
                assert!(
                    bound,
                    "{action}'s button teaches {}, which isn't bound to it",
                    k.key
                );
            }
            assert_eq!(seen, 15);
            // Only listed buttons exist (no orphan labels in the table).
            for k in table().iter().filter(|k| k.button.is_some()) {
                assert!(
                    TOOLBAR.iter().any(|g| g.contains(&k.action))
                        || CAPTURE_ROW.iter().any(|(a, _)| *a == k.action),
                    "{} has a button but no place",
                    k.action
                );
            }
        });
        assert_eq!(
            tooltip("Publish", &button_row("Publish").unwrap(), ""),
            if cfg!(target_os = "windows") {
                "Publish  Ctrl+Enter"
            } else {
                "Publish  ⌘⏎"
            }
        );
    }

    /// The native menu shows the first binding of an action whose context
    /// isn't its default one (gpui-pre-macos `create_menu_item`), and turns
    /// its key into a key equivalent verbatim: "enter" became the letter e
    /// (⌘E on Publish). The first Publish binding must be the ⌘↩ shim.
    #[test]
    #[cfg(not(target_os = "windows"))]
    fn the_publish_menu_shows_cmd_return() {
        let cx = gpui_kit::TestAppContext::single();
        cx.update(|cx| {
            gpui_kit::init(cx);
            bind_keys(cx);
            let km = cx.key_bindings();
            let km = km.borrow();
            let first = km.bindings_for_action(&Publish).next().expect("bound");
            let ks = first.keystrokes();
            assert_eq!(ks.len(), 1);
            assert_eq!(ks[0].key(), "↩");
            assert!(ks[0].modifiers().platform);
            // And no binding of any action puts a multi-letter key first
            // where the menu would read only its first letter.
            for k in table()
                .iter()
                .filter(|k| k.menu.is_some() && !k.key.is_empty())
            {
                let key = k.menu_key.unwrap_or(k.key);
                let last = normalize(key).1;
                assert!(
                    last.chars().count() == 1
                        || k.menu_key.is_some()
                        || NATIVE_MENU_KEYS.contains(&last.as_str()),
                    "{} ({}) would show as ⌘{} in the menu; give it a menu_key",
                    k.key,
                    k.action,
                    last.chars().next().unwrap_or(' ').to_uppercase()
                );
            }
        });
    }
}
