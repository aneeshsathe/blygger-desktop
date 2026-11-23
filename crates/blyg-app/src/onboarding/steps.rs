//! The tutorial's script: one row per step, in order, and the tours that
//! walk them. Edit the tables to change the tours; `tutorial.rs` only
//! interprets them.
//!
//! There is no single long tour. The chooser (the first tour screen, also
//! Settings › Help) offers the **quick tour** (`QUICK`: the essentials, a
//! couple of minutes) and the **feature tours** (`TOURS`). Every step but
//! the two end cards is in exactly one feature tour, in table order; the
//! quick tour borrows a few of them. A tour can start at any of its steps,
//! so a step's `setup` prepares everything it needs and never counts on the
//! step before it. What's new runs the steps a release added
//! (`news_run`), wherever they live.
//!
//! A step waits for one of its `keys` (a GPUI action the user triggers in
//! the real window, never synthetic input), then moves on. Next and Back
//! always work too. `setup` prepares the sample data for the step when it
//! is entered, from either direction.

use crate::platform_text::{Text, ui_text, ui_text_per_platform};

/// Something the user does that a step can wait for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// ↑ or ↓ in the omnibar.
    MoveSelection,
    /// ⏎ (the omnibar, or a sheet's field).
    Enter,
    /// Any edit in the editor (autosave).
    Typed,
    ToggleKind,
    MakeDraft,
    Publish,
    ViewWrite,
    ViewSplit,
    ViewStudio,
    AiGenerate,
    AiShorten,
    ShowVersions,
    QuotePicker,
    /// Discard Changes (⌥⌘Z, Post › Discard Changes…, the toolbar's
    /// eraser).
    DiscardChanges,
    /// Reply · new stub, Quote into a thread or Link post on your own
    /// published post (its chips, ⌥⌘R / ⌥⌘Q / ⌥⌘L, or the Post menu).
    OwnPostAction,
    // Seen on screen rather than as an action (`tutorial_observed`): each
    // counts when it becomes true after the step began.
    /// The stream's side pane opened (⏎, Space or "Read more").
    ReadMore,
    /// A quote's original post opened (a click on the quote box).
    OpenOriginal,
    /// A profile opened (a click on a name).
    OpenProfile,
    /// Reader mode is on (⌥⌘2 or the toggle).
    ReaderMode,
    /// The notes drawer is out (⇧⌘N or → Notes).
    Notes,
    /// The browser pane was closed (esc or ×).
    CloseBrowser,
    /// The @-mention popup is open.
    Mention,
    /// The spelling menu is open (a right-click on an underlined word).
    SpellMenu,
    /// The lineage sheet is open (⌘J or a click on the glyph).
    Lineage,
    /// Its ring of actions is open (Space on the centre).
    LineageRing,
    /// ⌘N (New Post): an empty editor.
    NewPost,
    /// ✂ Clip or ⇧⌘C in the browser pane.
    Clip,
    /// The ⇧⌘P Extensions palette is open.
    ExtPalette,
    /// The Manage extensions sheet is open.
    ExtManage,
    /// The consent sheet was answered (⏎, esc or ⌘⌫).
    ConsentAnswered,
    /// A reading entry's ⋯ sheet is open.
    SlotSheet,
    /// The notes drawer's Notes tab shows a folder other than the first.
    NotesVault,
    /// The browser pane was folded while a macro runs.
    MacroFolded,
    /// Import OPML… (the Subscriptions screen's chip, or Blyg › Import
    /// Subscriptions from OPML…).
    OpmlImport,
    /// The link or quote picker is open (typed `[[` or `![[`, or ⌘K).
    Picker,
    /// The reading screen's compose box has the keyboard (`c` or ⇧⌘K).
    Compose,
    /// A layer, a "responds to" line or a link in the open post was
    /// followed: the walk has a step to go back to.
    FollowThicket,
    /// The browsing history pane is open beside the post (⇧⌘H).
    BrowseHistory,
    /// The open stub quotes a passage, chosen in the preview ("❝ quote
    /// only this" or ⇧⌘D).
    StubPassage,
    /// The blyg switcher sheet is open (the title bar's blyg, or ⌃⌘0).
    BlygSwitcher,
    // --- hoppers ---
    /// The Hopper picker is open on a post (`h`, ⌥⌘P, or Hopper ›).
    HopperPicker,
    /// The Hopper picker is open on several picked posts at once.
    HopperSeveral,
    // --- shelf ---
    /// A post was put on the shelf or taken off (⌥⌘B).
    Shelf,
    // --- lineage ---
    /// On Posts, the lineage line's lists opened, or ⌘J opened the sheet.
    LineageStrip,
    /// The lineage sheet zoomed out to the neighbourhood map (`−`).
    LineageMap,
    // --- hover cards ---
    /// A hover card is showing over a `[[…]]` in the editor.
    HoverCard,
    // --- flow ---
    /// The command palette opened (⇧⌘P, View › Command Palette…).
    Palette,
    /// Focus mode turned on (⌥⌘F, View › Focus).
    Focus,
    /// Serendipity's card is showing in ⌘N's empty editor.
    Serendipity,
    // --- tangles ---
    /// A post is going on a trail (`t`: the why field is open), or ⌥⌘T.
    Trail,
    // --- studio parity (0.19.0) ---
    /// Blyg › Site… is open.
    SiteSheet,
    /// Blyg › Contacts… is open.
    ContactsSheet,
    /// Blyg › Stale Quotes… is open.
    StaleQuotes,
    /// Blyg › Client Access… is open.
    ClientAccess,
    /// A post went on the quote tray (`q` or ⌥⌘K).
    Tray,
    /// ⌥⌘⏎: a post opened beside the one being written.
    Split,
    // --- catch up (0.23.0) ---
    /// The Catch up sheet is open (⇧C, the Catch up chip, Post › Catch
    /// Up…, or ⇧C in the lineage sheet).
    CatchUp,
    // --- 0.24.0 --- (studio 0.61)
    /// The sample post went to Background (b, or its chip).
    Background,
    /// The sample post went on the shelf (+ Save).
    ShelfReading,
    /// Reading's Shelf lens is the source.
    ShelfLens,
    // --- 0.25.0 ---
    /// Reading's Mentions feed is the source.
    MentionsFeed,
    // --- 0.28.0 --- (studio 0.68)
    /// The Feed was switched to Chronological (its own control).
    FeedOrder,
    /// Reading's Raw is the source.
    RawTab,
    /// Reading's Agentic feed is the source (studio 0.69's edition).
    AgenticTab,
    /// --- studio 0.71 --- A Stream card's date was clicked: its post is
    /// open in the side pane.
    StreamDate,
    /// --- studio 0.71 --- ⌘G on the blyg stopped at the checkpoint: its
    /// sheet is up.
    TkCheckpoint,
    /// The Discover sheet opened.
    Discover,
    // --- nearby ---
    /// The Nearby rail was shown or hidden (⌥⌘U), or a row chosen or linked.
    Nearby,
    // --- 0.26.0 ---
    /// Quiet writing is on (Settings › Quiet writing, View › Quiet Writing).
    QuietWriting,
    /// The open post's ⋯ More menu is showing.
    MoreMenu,
    // --- 0.27.0 ---
    /// The tangle confirm step is showing ("This will publish N new items
    /// and this thread"): ⌘⏎, then ⏎ on the publish sheet.
    Tangle,
    /// Edit › Format › Formula (⌃⌘M) ran.
    Formula,
    /// The sample burst thread's "+N parts" are listed: unfolded under
    /// its stream card (`p` or a click), or a part of it is open.
    Parts,
    // --- 0.16.0 ---
    /// The sample conflict was answered: its post is no longer in conflict
    /// (1, 2 or 3 on the sheet).
    ConflictAnswered,
}

/// The part of the window a step highlights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Omnibar,
    /// The omnibar and the list together.
    Search,
    List,
    Editor,
    /// The status bar's left end: ◦ fragment / ≡ thread and the counter.
    Counter,
    /// The stream's column of posts (left of its side pane, if open).
    Stream,
    /// The selected post in the stream.
    StreamPost,
    /// The Stream | Reader toggle; in Reader mode, the sources pane.
    Reader,
    /// The Reader's post pane (header, responses and actions).
    ReaderPost,
    /// The browser pane's toolbar (back, address, 🛡, ↗, → Notes).
    BrowserChrome,
    /// A sheet dropping from the title bar.
    Sheet,
    /// The notes drawer, at the window's right edge.
    NotesDrawer,
    /// A macro's run: the browser pane's toolbar while it's open, the
    /// status bar once it's folded.
    MacroRun,
    /// A reading screen's header (Subscriptions: its chips).
    ScreenHeader,
    /// The compose box along the foot of the reading screen.
    Compose,
    /// The whole window (no ring).
    Whole,
    /// --- 0.26.0 --- Settings' Quiet writing row (Settings open).
    QuietSetting,
    /// --- 0.26.0 --- The ⋯ More chip under the open post (where the
    /// reading pane drew it).
    MoreChip,
}

/// What a step prepares on the sample data when it's entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setup {
    /// Posts, ⌘1, an empty omnibar with the focus in it.
    Search,
    /// A query with no match in the omnibar, caret at the end.
    Query(&'static str),
    /// Make sure a post is open in the editor.
    EnsureOpen,
    /// A fresh scratch note (as quick capture would keep it), opened.
    Scratch(&'static str),
    /// A draft with something to publish, opened.
    Publishable,
    /// The publish sheet, if it isn't already open.
    PublishSheet,
    /// The sample thread (a quote, a TK and a video), opened in ⌘1.
    StudioSample,
    /// `StudioSample`, in the full editor (⌘3).
    FullEditor,
    /// The draft with a TK scope appended, caret inside it.
    Tk(&'static str),
    /// --- studio 0.71 --- The draft with a gap naming a link the tour's
    /// blyg can't read and one it can, caret inside, in ⌘2; ⌘G goes to the
    /// blyg for this step only (the canned AI everywhere else).
    TkOnBlyg,
    /// A thread draft whose generated text echoed a quote its instruction
    /// didn't name, in the full editor (⌘3).
    EchoedQuote,
    /// The draft pushed over 1000 characters.
    Long(&'static str),
    /// A published post with a few versions, opened.
    Versioned,
    /// The published post with this sentence appended, so it has
    /// unpublished edits.
    Edited(&'static str),
    /// The sample garden notes opened, then edited "on the blyg" under
    /// them: the conflict sheet opens.
    Conflict,
    /// Reading, in the Reader, with the first two posts it lists picked.
    HopperSeveral,
    /// Back to Posts.
    Posts,
    /// A thread draft opened (quotes go in threads).
    Thread,
    /// The draft with this sentence appended (a misspelling in it), caret
    /// at the end.
    Mentions(&'static str),
    /// Reading, in the Stream, the side pane closed, the newest post
    /// selected.
    Stream,
    /// Reading, in the Stream, a revised post (it leads with "V2: …")
    /// selected, the side pane closed.
    StreamLead,
    /// Reading, in the Stream, the post with a quote selected.
    StreamQuote,
    /// Reading, in the Stream, the post with text marked as generated
    /// selected, the side pane closed.
    StreamGenerated,
    /// Reading, in the Stream, the quoting post's lineage open (⌘J).
    Lineage,
    /// Reading, in the Stream, the last post of the sample four-post quote
    /// chain selected, the side pane closed.
    StreamCatchUp,
    /// Reading, in Reader mode, a post open.
    Reader,
    /// Reading, in Reader mode, the quoting post open (a walk starts at it).
    Thicket,
    /// `Thicket`, with its quote followed: a walk of two posts.
    Walked,
    /// Reading, in Reader mode, the sample "Slow media" hopper picked and
    /// its first post open.
    Hoppers,
    /// A stub replying to a reading post, open beside its preview.
    Stub,
    /// The browser pane on a blank sample page (nothing is loaded).
    Browser,
    /// The notes drawer on its Notes tab, showing the tour's first sample
    /// folder of Markdown notes.
    NotesLibrary,
    /// The browser pane on its sample page, with a sample macro "running"
    /// in it (nothing runs).
    CrossPost,
    /// The ⇧⌘P Extensions palette, open.
    Palette,
    /// --- auto cross-post --- Posts, the sample's published post open and
    /// set to cross-post to Bluesky (for the session; the tour's sample
    /// cross-post lists its sites).
    AutoCrossPost,
    /// A sample consent sheet for markdown-notes (answering it writes
    /// nothing).
    Consent,
    /// The Subscriptions screen (⇧⌘S).
    Subscriptions,
    /// --- tangles --- Posts, the sample draft open and then the versioned
    /// post (so ⌥⌘⏎ has a post to put beside).
    Pair,
    /// --- 0.24.0 --- Reader, Ada's "finished media" post open and in the
    /// foreground (no choice of yours on it).
    Background,
    /// Reader, Rue's post on trust open and off the shelf.
    ShelfAdd,
    /// Reader on All, with Rue's post on trust on the shelf.
    ShelfLens,
    /// Reader on All, with posts that quote, reply to, link to and name you.
    MentionsFeed,
    /// --- 0.28.0 --- Reader on the Feed, pace-layered.
    FeedOrder,
    /// --- 0.26.0 --- The published sample post open (its lineage line
    /// under it), with Settings open over it at Quiet writing.
    QuietWriting,
    /// --- 0.27.0 --- A new thread draft holding a `![[+ …]]` passage and a
    /// `[[+ …]]` idea, its preview beside it.
    Tangle,
    /// A new draft with a formula in it, its preview beside it, and this
    /// TeX selected.
    Maths(&'static str),
    /// Reading, in the Stream, Lin's thread published with two parts
    /// selected ("+2 parts"), the side pane closed.
    Parts,
}

/// One step of the tour.
#[derive(Debug, Clone, Copy)]
pub struct Step {
    pub id: &'static str,
    /// The release whose feature this step was added to teach. A what's-new
    /// entry starts at the first step of its own release (`whats_new`'s
    /// tests check it), so a release with something new adds a step for it.
    pub since: &'static str,
    pub title: Text,
    /// One or two short sentences. `{hotkey}` becomes the quick-capture key.
    pub caption: Text,
    /// Any of these moves on. Empty: only Next does.
    pub keys: &'static [Key],
    /// How the key is shown on the card, e.g. "⌘T".
    pub key_label: Text,
    pub region: Region,
    /// The matching toolbar button's label, pointed at when buttons are on.
    pub button: Option<&'static str>,
    pub setup: Setup,
    /// After the key: how long the card shows "✓" before moving on.
    pub pause_ms: u64,
    /// Also wait until sheets, AI proposals and generations are done
    /// (so the user sees what the key did).
    pub settle: bool,
    /// After the key, stay until Next: the key opened something to look at
    /// or use (a pane, a popup, a mode), so the tour doesn't pull it away.
    pub stay: bool,
}

impl Step {
    pub fn accepts(&self, key: Key) -> bool {
        self.keys.contains(&key)
    }
}

/// Sample post ids in the tutorial's FakeBackend.
pub const DRAFT: &str = "01J9QK3";
pub const VERSIONED: &str = "01J9M2A";
pub const THREAD: &str = "01J9H4C";
/// The sample reading post that quotes another (and stubs it).
pub const QUOTING: &str = crate::fake::reading_seed::ADA_REPLY;
/// The post it quotes (it lists the quoting post as a response).
pub const QUOTED: &str = crate::fake::reading_seed::LIN_BENCH;
/// The sample reading post with a sentence its blyg marks as generated.
pub const GENERATED: &str = crate::fake::reading_seed::LIN_FORK;
/// A revised sample reading post (it leads with its changelog note).
pub const REVISED: &str = crate::fake::reading_seed::ADA_TIDES;
/// The published sample post the conflict step edits "elsewhere".
pub const CONFLICTED: &str = "01J9K7T";
/// The sample reading post the stub step replies to.
pub const STUBBED: &str = crate::fake::reading_seed::LIN_GARDENS;
/// The sample reading post at the end of a four-post quote chain (Catch
/// up's sample conversation).
pub const CATCH_UP: &str = crate::fake::reading_seed::TALK_D;
/// --- 0.24.0 --- The post the Background step moves.
pub const BACKGROUND: &str = crate::fake::reading_seed::ADA_FINISHED;
/// The post the shelf steps put on the shelf.
pub const SHELVED: &str = crate::fake::reading_seed::RUE_TRUST;

pub const STEPS: &[Step] = &[
    Step {
        id: "search",
        since: "0.1.0",
        title: ui_text("Search is the interface"),
        caption: ui_text(
            "Type to filter every post as you go. ↑ and ↓ move the selection, and the \
                  editor previews each post as you pass it.",
        ),
        keys: &[Key::MoveSelection],
        key_label: ui_text("↑ ↓"),
        region: Region::Search,
        button: None,
        setup: Setup::Search,
        pause_ms: 900,
        settle: false,
        stay: false,
    },
    Step {
        id: "create",
        since: "0.1.0",
        title: ui_text("No match? ⏎ starts writing"),
        caption: ui_text(
            "Nothing matches “tide pools”, so ⏎ creates a new draft seeded with it, \
                  with the caret at the end.",
        ),
        keys: &[Key::Enter],
        key_label: ui_text("⏎"),
        region: Region::Omnibar,
        // (The toolbar's New is ⌘N, the next step.)
        button: None,
        setup: Setup::Query("tide pools"),
        pause_ms: 500,
        settle: false,
        stay: false,
    },
    Step {
        id: "new",
        since: "0.10.0",
        title: ui_text("⌘N: a blank page"),
        caption: ui_text(
            "⌘N opens an empty editor and leaves your search as it was. What you type is \
                  a scratch note, kept on this Mac until ⌘D makes it a draft (⌘⏎ publishes \
                  it). Leave without typing and nothing is kept.",
        ),
        keys: &[Key::NewPost],
        key_label: ui_text("⌘N"),
        region: Region::Editor,
        button: Some("New"),
        setup: Setup::Search,
        pause_ms: 1400,
        settle: false,
        stay: true,
    },
    Step {
        id: "autosave",
        since: "0.1.0",
        title: ui_text("Just write"),
        caption: ui_text(
            "Every keystroke is saved on this Mac and pushed to your blyg a moment after \
                  you stop. There's no save button. Type a few words.",
        ),
        keys: &[Key::Typed],
        key_label: ui_text("type"),
        region: Region::Editor,
        button: None,
        setup: Setup::EnsureOpen,
        pause_ms: 1600,
        settle: false,
        stay: false,
    },
    Step {
        id: "kinds",
        since: "0.1.0",
        title: ui_text("Fragments and threads"),
        caption: ui_text(
            "A fragment is short: 1000 characters at most, counted down here. A thread \
                  has no limit and can quote other posts. ⌘T switches between them.",
        ),
        keys: &[Key::ToggleKind],
        key_label: ui_text("⌘T"),
        region: Region::Counter,
        button: None,
        setup: Setup::EnsureOpen,
        pause_ms: 1500,
        settle: false,
        stay: false,
    },
    Step {
        id: "capture",
        since: "0.1.0",
        title: ui_text("Quick capture keeps scratch notes"),
        caption: ui_text(
            "{hotkey} opens quick capture over any app. What you jot down stays on this \
                  Mac as a scratch note, listed here with its scratch pill and never synced. \
                  ⌘D makes it a draft; ⌘⏎ publishes it.",
        ),
        keys: &[Key::MakeDraft],
        key_label: ui_text("⌘D"),
        region: Region::List,
        button: Some("Make draft"),
        setup: Setup::Scratch(
            "Tide tables are poems nobody reads aloud: caught with quick capture.",
        ),
        pause_ms: 1400,
        settle: false,
        stay: false,
    },
    Step {
        id: "publish",
        since: "0.1.0",
        title: ui_text("⌘⏎ publishes"),
        caption: ui_text(
            "Publishing asks for an optional version note first. This is sample data, so \
                  nothing leaves your Mac.",
        ),
        keys: &[Key::Publish],
        key_label: ui_text("⌘⏎"),
        region: Region::Editor,
        button: Some("Publish"),
        setup: Setup::Publishable,
        pause_ms: 150,
        settle: false,
        stay: false,
    },
    Step {
        id: "publish-note",
        since: "0.1.0",
        title: ui_text("Say what changed"),
        caption: ui_text(
            "Type a note such as “first version” (or leave it empty) and press ⏎. \
                  esc would cancel.",
        ),
        keys: &[Key::Enter, Key::Publish],
        key_label: ui_text("⏎"),
        region: Region::Sheet,
        button: None,
        setup: Setup::PublishSheet,
        pause_ms: 1200,
        settle: false,
        stay: false,
    },
    Step {
        id: "full-editor",
        since: "0.1.0",
        title: ui_text("The full editor"),
        caption: ui_text(
            "⌘2 adds a live preview beside the list; ⌘3 is the full editor: the source \
                  and the page exactly as it will be published.",
        ),
        keys: &[Key::ViewStudio, Key::ViewSplit],
        key_label: ui_text("⌘3"),
        region: Region::Whole,
        button: Some("Full editor"),
        setup: Setup::StudioSample,
        pause_ms: 2200,
        settle: false,
        stay: true,
    },
    Step {
        id: "write-view",
        since: "0.1.0",
        title: ui_text("Back to writing"),
        caption: ui_text("⌘1 brings back the plain list and editor."),
        keys: &[Key::ViewWrite],
        key_label: ui_text("⌘1"),
        region: Region::Whole,
        button: Some("Write"),
        setup: Setup::FullEditor,
        pause_ms: 600,
        settle: false,
        stay: false,
    },
    Step {
        id: "tk",
        since: "0.1.0",
        title: ui_text("Fill a gap with AI"),
        caption: ui_text(
            "Write [TK]an instruction[/TK] and press ⌘G with the caret inside: the gap is \
                  filled, tinted as generated, and disclosed when published. (A canned demo: \
                  no AI is called.)",
        ),
        keys: &[Key::AiGenerate],
        key_label: ui_text("⌘G"),
        region: Region::Editor,
        button: Some("Generate"),
        setup: Setup::Tk("one short sentence about fog, in my voice"),
        pause_ms: 1200,
        settle: true,
        stay: false,
    },
    // --- 0.28.0 --- studio 0.71: ⌘G on the blyg stops at a link it can't
    // read, and lists what it read.
    Step {
        id: "tk-checkpoint",
        since: "0.28.0",
        title: ui_text("When your blyg can't read a link"),
        caption: ui_text(
            "With your blyg as the AI provider (blygger-studio 0.70 or later), ⌘G has it read \
                  every link in the gap first. One of these two can't be read, so it stops, \
                  saves nothing and asks: “Details” (d) says which link and why, ⏎ generates \
                  anyway, esc leaves the draft as it was. Once it fills the gap, what it read \
                  is listed under the preview, and again in the Publish sheet. Press ⌘G.",
        ),
        keys: &[Key::TkCheckpoint],
        key_label: ui_text("⌘G"),
        region: Region::Editor,
        button: Some("Generate"),
        setup: Setup::TkOnBlyg,
        pause_ms: 1200,
        settle: false,
        stay: true,
    },
    Step {
        id: "shorten",
        since: "0.1.0",
        title: ui_text("Too long? Shorten to fit"),
        caption: ui_text(
            "This fragment is over 1000. ⇧⌘G proposes a shorter version; you accept or \
                  reject it. (Canned, too.)",
        ),
        keys: &[Key::AiShorten],
        key_label: ui_text("⇧⌘G"),
        region: Region::Counter,
        button: None,
        setup: Setup::Long(
            "The log records wind, swell and visibility, hour after hour, in the same hand. ",
        ),
        pause_ms: 600,
        settle: true,
        stay: false,
    },
    Step {
        id: "versions",
        since: "0.1.0",
        title: ui_text("Versions and pins"),
        caption: ui_text(
            "⌘Y shows every version of your post here, to compare or restore one. Pinning \
                  a version serves it forever: a pin can't be undone.",
        ),
        keys: &[Key::ShowVersions],
        key_label: ui_text("⌘Y"),
        region: Region::Editor,
        button: Some("Versions"),
        setup: Setup::Versioned,
        pause_ms: 2800,
        settle: false,
        stay: true,
    },
    Step {
        id: "discard",
        since: "0.13.0",
        title: ui_text("Changed your mind? Discard"),
        caption: ui_text_per_platform(
            "This published post has unpublished edits: hover “unpublished edits” below. \
             ⌘⏎ would publish them as v4. ⌥⌘Z (or Post › Discard Changes…, or the \
             toolbar's eraser) goes back to v3 as published, after asking. The public post \
             never changes.",
            "This published post has unpublished edits: hover “unpublished edits” below. \
             ⌘⏎ would publish them as v4. Post › Discard Changes… or the toolbar's eraser \
             goes back to v3 as published, after asking. The public post never changes.",
        ),
        keys: &[Key::DiscardChanges],
        key_label: ui_text_per_platform("⌥⌘Z", "click"),
        region: Region::Editor,
        button: Some("Discard"),
        setup: Setup::Edited("Then I wrote this down, and thought better of it."),
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- shelf ---
    Step {
        id: "shelf",
        since: "0.18.0",
        title: ui_text("Keep it in Saved"),
        caption: ui_text_per_platform(
            "Some posts you keep coming back to: a /now page, a reading log, a running list. \
             ⌥⌘B saves the open post: 📚 marks it and it stays at the top of the list. ⌘B \
             opens Saved (it was the shelf), and 1–9 opens a post in it. With blygger-studio \
             0.61 Saved syncs with your blyg; otherwise it's kept on this Mac.",
            "Some posts you keep coming back to: a /now page, a reading log, a running list. \
             Ctrl+Shift+A saves the open post: 📚 marks it and it stays at the top of the \
             list. Ctrl+B opens Saved (it was the shelf), and 1–9 opens a post in it. With \
             blygger-studio 0.61 Saved syncs with your blyg; otherwise it's kept on this \
             computer.",
        ),
        keys: &[Key::Shelf],
        key_label: ui_text("⌥⌘B"),
        region: Region::List,
        button: None,
        setup: Setup::EnsureOpen,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- lineage ---
    Step {
        id: "lineage-writing",
        since: "0.18.0",
        title: ui_text("Lineage while you write"),
        caption: ui_text(
            "Under the editor, one quiet line: this post's lineage glyph, what it draws on and \
             what draws on it (Rue replied to it, Ada quoted it). A click opens the lists, and \
             a row opens that post. ⌘J opens the full lineage, as in Reading.",
        ),
        keys: &[Key::LineageStrip],
        key_label: ui_text("⌘J"),
        region: Region::Editor,
        button: None,
        setup: Setup::Versioned,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "neighbourhood",
        since: "0.18.0",
        title: ui_text("Zoom out to the neighbourhood"),
        caption: ui_text(
            "In the lineage sheet, − zooms out two hops: what the neighbours draw on and \
                  what draws on them, coloured fork, reply, quote; a dashed one is a hop \
                  further, a shaded one is yours. Arrows move, ⏎ makes a post the centre, ⌫ \
                  walks back, + returns to the list.",
        ),
        keys: &[Key::LineageMap],
        key_label: ui_text("−"),
        region: Region::Whole,
        button: None,
        setup: Setup::Lineage,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- hover cards ---
    Step {
        id: "hover-cards",
        since: "0.18.0",
        title: ui_text("What a link is, on hover"),
        caption: ui_text(
            "Rest the pointer on a [[…]] or ![[…]] in the editor: a card says what it is (title, \
             author, version, first lines), and an unlabelled link offers “label it”. ⌘-click \
             opens the post: yours here, anyone else's in Reading. Quotes and links in the \
             Stream show the same card.",
        ),
        keys: &[Key::HoverCard],
        key_label: ui_text("hover"),
        region: Region::Editor,
        button: None,
        setup: Setup::StudioSample,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- flow ---
    Step {
        id: "palette",
        since: "0.18.0",
        title: ui_text("Every command, by name"),
        caption: ui_text_per_platform(
            "⇧⌘P opens the command palette: every action, theme, font, blyg and hopper, and \
             your posts by title. Type a few letters (“undo edits” finds Discard Changes); \
             each row shows its key, and a theme or font row shows itself on the window as you \
             move to it. ⏎ runs it, ⌘⏎ runs it and keeps the palette open, and the last ten \
             come first.",
            "Ctrl+Shift+P opens the command palette: every action, theme, font, blyg and \
             hopper, and your posts by title. Type a few letters (“undo edits” finds Discard \
             Changes); each row shows its key, and a theme or font row shows itself on the window \
             as you move to it. Enter runs it, Ctrl+Enter runs it and keeps the palette open, \
             and the last ten come first.",
        ),
        keys: &[Key::Palette],
        key_label: ui_text("⇧⌘P"),
        region: Region::Whole,
        button: None,
        setup: Setup::Posts,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "focus",
        since: "0.18.0",
        title: ui_text("Focus: the line stays put"),
        caption: ui_text_per_platform(
            "⌥⌘F is focus mode: the editor alone, the line you're typing on held a little \
             above the middle, the paragraphs you're not in dimmed, and a hairline with the \
             sync dot and this sitting's words. esc brings everything back; focus-line and \
             focus-fade in the config file tune it.",
            "Ctrl+Shift+F is focus mode: the editor alone, the line you're typing on held \
             a little above the middle, the paragraphs you're not in dimmed, and a hairline \
             with the sync dot and this sitting's words. Esc brings everything back; \
             focus-line and focus-fade in the config file tune it.",
        ),
        keys: &[Key::Focus],
        key_label: ui_text("⌥⌘F"),
        region: Region::Whole,
        button: None,
        setup: Setup::EnsureOpen,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "serendipity",
        since: "0.18.0",
        title: ui_text("An old thought, if you like"),
        caption: ui_text_per_platform(
            "Serendipity is off until you turn it on: Settings (⌘,) › Serendipity, or \
             serendipity = own in the config file. Then ⌘N's empty editor shows one of your \
             older posts, often one nothing links to yet: q quotes it, l links it, o opens \
             it, r shows another, and typing sends it away. Press ⌘N to see one from the \
             samples.",
            "Serendipity is off until you turn it on: Settings (Ctrl+,) › Serendipity, or \
             serendipity = own in the config file. Then a new post's empty editor shows one \
             of your older posts, often one nothing links to yet: q quotes it, l links it, o \
             opens it, r shows another, and typing sends it away. Press Ctrl+N to see one \
             from the samples.",
        ),
        keys: &[Key::Serendipity],
        key_label: ui_text("⌘N"),
        region: Region::Editor,
        button: None,
        setup: Setup::Posts,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- end flow ---
    // --- tangles --- trails, the quote tray, split view.
    Step {
        id: "trails",
        since: "0.18.0",
        title: ui_text("Keep a trail"),
        caption: ui_text(
            "A trail is a path through posts you chose to keep. t adds the selected post, with a \
             word on why (⏎ keeps it, esc skips); ⇧T starts another trail. The trail bar walks \
             it, and ⌥⌘T writes it as a thread of quotes in a new draft: nothing is published \
             until you publish it.",
        ),
        keys: &[Key::Trail],
        key_label: ui_text("t"),
        region: Region::Whole,
        button: None,
        setup: Setup::StreamQuote,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "quote-tray",
        since: "0.18.0",
        title: ui_text("Pick it up, quote it later"),
        caption: ui_text_per_platform(
            "q (or ⌥⌘K) puts the post on the quote tray, the strip above the status bar. ⌘K \
             lists the tray first when you quote, and you can drag a tray chip, a stream post or \
             a Reader row straight into a post: a quote in a thread, a link in a fragment. \
             Dropped on the omnibar, it starts a new thread.",
            "q (or Ctrl+Shift+T) puts the post on the quote tray, the strip above the status bar. \
             Ctrl+K lists the tray first when you quote, and you can drag a tray chip, a stream \
             post or a Reader row straight into a post: a quote in a thread, a link in a \
             fragment. Dropped on the search box, it starts a new thread.",
        ),
        keys: &[Key::Tray],
        key_label: ui_text("q"),
        region: Region::Whole,
        button: None,
        setup: Setup::StreamQuote,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "split-view",
        since: "0.18.0",
        title: ui_text("Two posts side by side"),
        caption: ui_text(
            "⌥⌘⏎ puts the selected post beside the one you were writing: yours in a second \
             editor that saves on its own, anyone else's read-only. ⌥⌘[ and ⌥⌘] move between \
             them, the divider drags, × closes it.",
        ),
        keys: &[Key::Split],
        key_label: ui_text("⌥⌘⏎"),
        region: Region::Whole,
        button: None,
        setup: Setup::Pair,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- end tangles ---
    // --- nearby --- the rail beside the sample thread.
    Step {
        id: "nearby",
        since: "0.18.0",
        title: ui_text("Nearby thoughts, while you write"),
        caption: ui_text_per_platform(
            "⌥⌘U, or the Nearby tab at the right edge, opens a drawer beside the editor with your posts, and posts you hold, that say what the paragraph under the caret says. ⌥↑/⌥↓ choose, ⌥⏎ links the phrase you share, ⌥⇧⏎ quotes the post (threads). It stays open until you close it; Settings › Nearby can add a small model on this Mac that matches meaning, not just words.",
            "Ctrl+Shift+U, or the Nearby tab at the right edge, opens a drawer beside the editor with your posts, and posts you hold, that say what the paragraph under the caret says. Alt+Up/Alt+Down choose, Alt+Enter links the phrase you share, Alt+Shift+Enter quotes the post (threads). It stays open until you close it; Settings › Nearby can add a small model on this computer that matches meaning, not just words.",
        ),
        keys: &[Key::Nearby],
        key_label: ui_text("⌥⌘U"),
        region: Region::Editor,
        button: None,
        setup: Setup::StudioSample,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- capture --- loose ends, capture from anywhere, contacts.
    Step {
        id: "loose-ends",
        since: "0.18.0",
        title: ui_text("Loose ends"),
        caption: ui_text(
            "The search reads operators: is:draft lists your drafts, is:orphan the \
                  published posts (a week old or more) nothing points at, has:tk the ungenerated TKs, is:stale \
                  drafts untouched for a month; links: and quotes: take an id or a host, and \
                  kind:thread, v:2 work too. Other words search as ever. With the search \
                  empty, a quiet line over the list says what's loose, in words.",
        ),
        keys: &[Key::Enter],
        key_label: ui_text("⏎"),
        region: Region::Omnibar,
        button: None,
        setup: Setup::Query("is:draft"),
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- 0.26.0 --- quiet writing: the setting itself.
    Step {
        id: "quiet-writing",
        since: "0.26.0",
        title: ui_text("Quiet writing"),
        caption: ui_text(
            "The lineage line, the loose-ends line and Nearby thoughts each sit around the \
                  editor on their own. Quiet writing hides all three, for a plainer page; ⌘J, \
                  is:orphan and ⌥⌘U (for this sitting) still reach them. Click On, then esc to \
                  look; Off brings them back. It's a real setting, kept as you leave it, and \
                  View › Quiet Writing toggles it too.",
        ),
        keys: &[Key::QuietWriting],
        key_label: ui_text("click"),
        region: Region::QuietSetting,
        button: None,
        setup: Setup::QuietWriting,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "capture-link",
        since: "0.18.0",
        title: ui_text("Capture from anywhere"),
        caption: ui_text_per_platform(
            "Paste a web address into the search and ⏎ starts a fragment with that link, \
             titled from the page (a blyg post you hold is quoted instead). Anywhere on your \
             Mac, select text and choose Services › Capture in Burrow: the quick capture \
             panel opens with it quoted, and the page's address under it when the app gives \
             one.",
            "Paste a web address into the search and ⏎ starts a fragment with that link, \
             titled from the page (a blyg post you hold is quoted instead). In the quick \
             capture panel, text you copied in the last ten seconds is offered as a chip, \
             \"quote clipboard\"; it's never put in for you.",
        ),
        keys: &[Key::Enter],
        key_label: ui_text("⏎"),
        region: Region::Omnibar,
        button: None,
        setup: Setup::Query("https://lin.example.org/t/rock-pools"),
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "contacts",
        since: "0.18.0",
        title: ui_text("Your contacts, first"),
        caption: ui_text(
            "Type @: the people in your blyg's address book come first, then the blygs \
                  you read. Where your blyg turns @handle into a link when it publishes, \
                  picking a contact writes @handle; elsewhere, the plain link. In Reading, \
                  Mentions under All unread lists the posts that mention you: dots, never a \
                  count.",
        ),
        keys: &[Key::Mention],
        key_label: ui_text("@"),
        region: Region::Editor,
        button: None,
        setup: Setup::Mentions("Tea at the harbour with "),
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- end capture ---
    Step {
        id: "conflict",
        since: "0.16.0",
        title: ui_text("Edited in two places? You choose"),
        caption: ui_text(
            "Burrow saves against the version of a post it last saw. When the post \
                  changed on your blyg as well (in the studio, or on another computer), \
                  nothing is overwritten: Burrow shows both. 1 keeps yours, 2 takes the \
                  server's, 3 keeps both, yours as a new draft.",
        ),
        keys: &[Key::ConflictAnswered],
        key_label: ui_text("1 2 3"),
        // The sheet is taller than `Region::Sheet`'s ring.
        region: Region::Whole,
        button: None,
        setup: Setup::Conflict,
        pause_ms: 1600,
        settle: false,
        stay: false,
    },
    Step {
        id: "own-post",
        since: "0.13.0",
        title: ui_text("Reply to yourself"),
        caption: ui_text(
            "Your published post has the Reading view's actions above its text: Reply \
                  (⌥⌘R), Quote into a thread (⌥⌘Q) and Link post · new fragment \
                  (⌥⌘L). Forking your own post is in ⌘Y: Fork this pin, on a pinned version. \
                  Try Reply.",
        ),
        keys: &[Key::OwnPostAction],
        key_label: ui_text("⌥⌘R"),
        region: Region::Editor,
        button: None,
        setup: Setup::Versioned,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "quotes",
        since: "0.1.0",
        title: ui_text("Links and quotes"),
        caption: ui_text(
            "⌘K links a post you hold; in a thread it (or ![[ on a new line) quotes \
                  one, a snapshot taken when you publish. Rows lead with each post's title, \
                  and ⏎ writes [[id | label]]: the label is the link's text on the page, so \
                  edit the words after | to rename it. A quote's note is yours only. (A typed \
                  [[ stays text unless bracket-picker = on.)",
        ),
        keys: &[Key::QuotePicker, Key::Picker],
        key_label: ui_text("⌘K"),
        region: Region::Editor,
        button: None,
        setup: Setup::Thread,
        pause_ms: 800,
        settle: true,
        stay: false,
    },
    // --- 0.27.0 --- tangling and formulas (studio 0.62–0.64.1).
    Step {
        id: "tangle",
        since: "0.27.0",
        title: ui_text("New items from a thread"),
        caption: ui_text(
            "In a thread, ![[+ …]] around a passage (from a line's start to a line's end) \
                  makes it a post of its own, which the thread quotes; [[+ an idea]] in a \
                  sentence makes one the thread links. The preview marks each as a new item. \
                  Press ⌘⏎, then ⏎: Burrow lists what it will publish first, and ⏎ there \
                  publishes all, esc goes back.",
        ),
        keys: &[Key::Tangle],
        key_label: ui_text("⌘⏎"),
        region: Region::Whole,
        button: Some("Publish"),
        setup: Setup::Tangle,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "maths",
        since: "0.27.0",
        title: ui_text("Formulas"),
        caption: ui_text(
            "Write LaTeX between dollars: $x^2$ in a sentence, $$…$$ on lines of their own \
                  ($5 and $10 stay money). The preview draws each one, a broken one shows red, \
                  and publishing waits until it parses. ⌃⌘M (Edit › Format › Formula) makes \
                  the selection a formula; ⌃⌘B and ⌃⌘I make it bold or italic.",
        ),
        keys: &[Key::Formula],
        key_label: ui_text("⌃⌘M"),
        region: Region::Whole,
        button: None,
        setup: Setup::Maths("A = \\pi r^2"),
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "echoed-quote",
        since: "0.12.0",
        title: ui_text("A quote the AI added"),
        caption: ui_text(
            "This gap's text ends with ![[…]] on its own line, which its instruction \
                  never asked for. Publishing makes that a real quote and notifies its \
                  author, so the preview already shows the quote, and ⌘⏎'s sheet warns \
                  before you publish. Delete the line, or name the post in the \
                  instruction.",
        ),
        keys: &[Key::Publish],
        key_label: ui_text("⌘⏎"),
        region: Region::Whole,
        button: Some("Publish"),
        setup: Setup::EchoedQuote,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "mentions",
        since: "0.4.0",
        title: ui_text("Mentions and spelling"),
        caption: ui_text(
            "Type @ and a name to link a blyg you know (↑ ↓ choose, ⏎ inserts). It's a \
                  plain link, so it notifies no one. A red underline marks a misspelling: \
                  right-click it for suggestions.",
        ),
        keys: &[Key::Mention, Key::SpellMenu],
        key_label: ui_text("@"),
        region: Region::Editor,
        button: None,
        setup: Setup::Mentions("The fog horn sounded twice this mornign. Ask "),
        pause_ms: 1800,
        settle: false,
        stay: true,
    },
    Step {
        id: "switcher",
        since: "0.14.0",
        title: ui_text("Your blygs"),
        caption: ui_text(
            "The name in the title bar is the blyg you're writing to. Click it, or press ⌃⌘0, \
             to switch blygs or add another; ⌃⌘1 and ⌃⌘2 jump straight to them. Each blyg \
             keeps its own posts and reading. (The tour stays on its sample blyg.)",
        ),
        keys: &[Key::BlygSwitcher],
        key_label: ui_text("⌃⌘0"),
        region: Region::Whole,
        button: None,
        setup: Setup::Posts,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "stream",
        since: "0.4.0",
        title: ui_text("Reading, in the Stream"),
        caption: ui_text(
            "Burrow opens here, on Reading (⌘R goes to your posts and back): posts from \
                  the blygs you follow, newest first by their own date. j/k move, and a post a \
                  second on screen counts as read. A revised post leads with what changed, \
                  “V2: …”, from its author's changelog. ⏎, Space, a double-click or Read more \
                  opens it in a side pane.",
        ),
        keys: &[Key::ReadMore],
        key_label: ui_text("⏎"),
        region: Region::Stream,
        button: None,
        setup: Setup::StreamLead,
        pause_ms: 1800,
        settle: false,
        stay: true,
    },
    Step {
        id: "compose",
        since: "0.14.0",
        title: ui_text("Post while you read"),
        caption: ui_text(
            "The box at the foot of Reading writes a post without leaving the one you're \
                  reading. c puts the caret in it (⇧⌘K from anywhere), ⌘⏎ posts a fragment \
                  after 3 seconds (esc cancels, ⌘⏎ again posts now), ⌘D keeps a draft, and esc goes back to the stream with your text kept. With \
                  a post open, ❝ Quote quotes it.",
        ),
        keys: &[Key::Compose],
        key_label: ui_text("c"),
        region: Region::Compose,
        button: None,
        setup: Setup::Stream,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "slots",
        since: "0.11.0",
        title: ui_text("Reading time, and what's behind a post"),
        caption: ui_text(
            "Two bundled extensions, off until you turn them on: reading-time ends a \
                  post's byline with “· 3 min”, and inspect adds ⋯ to its actions, with the \
                  record Burrow holds for it (ids, versions, references). Click ⋯, then \
                  inspect.",
        ),
        keys: &[Key::SlotSheet],
        key_label: ui_text("click"),
        region: Region::StreamPost,
        button: None,
        setup: Setup::Stream,
        pause_ms: 1800,
        settle: false,
        stay: true,
    },
    Step {
        id: "generated",
        since: "0.12.0",
        title: ui_text("“Generated” is the publisher's word"),
        caption: ui_text(
            "Lin's blyg marks a sentence of this post as AI-generated. Open it (⏎): \
                  the marked text is tinted, hovering it says so, and the post ends with a \
                  note that the mark is the publisher's own disclosure, not verified.",
        ),
        keys: &[Key::ReadMore],
        key_label: ui_text("⏎"),
        region: Region::StreamPost,
        button: None,
        setup: Setup::StreamGenerated,
        pause_ms: 1800,
        settle: false,
        stay: true,
    },
    Step {
        id: "original",
        since: "0.4.0",
        title: ui_text("Quotes lead to the original"),
        caption: ui_text(
            "The grey box quotes another post: click its text to open the original beside \
                  it, or the name for the author's profile. Its pane lists who quoted, replied \
                  to or forked it.",
        ),
        keys: &[Key::OpenOriginal, Key::OpenProfile],
        key_label: ui_text("click"),
        region: Region::StreamPost,
        button: None,
        setup: Setup::StreamQuote,
        pause_ms: 2600,
        settle: false,
        stay: true,
    },
    // --- catch up (0.23.0) ---
    Step {
        id: "catch-up",
        since: "0.23.0",
        title: ui_text("Catch up on a conversation"),
        caption: ui_text(
            "Rue's post quotes Lin, who quotes Ada, who quotes Rue. ⇧C (or the Catch up \
                  chip under the post) lays the conversation out oldest first: each post's \
                  author, date and own words. j/k move, ⏎ opens one, w adds the side replies, \
                  and s asks your AI for a short summary, never put in a post.",
        ),
        keys: &[Key::CatchUp],
        key_label: ui_text("⇧C"),
        region: Region::StreamPost,
        button: None,
        setup: Setup::StreamCatchUp,
        pause_ms: 1800,
        settle: false,
        stay: true,
    },
    Step {
        id: "lineage",
        since: "0.8.0",
        title: ui_text("Lineage: where a post comes from"),
        caption: ui_text(
            "The glyph beside a post's name shows the kinds of post it draws on (left) \
                  and that draw on it (right): fork, reply, quote. “1 · 2” beside it counts \
                  them: up · down. ⌘J, or a click on it, opens the map, with the counts by the \
                  post: arrows move, ⏎ makes a neighbour the centre, ⌫ walks back, and ⇧C \
                  catches up on the conversation.",
        ),
        keys: &[Key::Lineage],
        key_label: ui_text("⌘J"),
        region: Region::StreamPost,
        button: None,
        setup: Setup::StreamQuote,
        pause_ms: 2400,
        settle: false,
        stay: true,
    },
    Step {
        id: "ring",
        since: "0.8.0",
        title: ui_text("Actions on a ring"),
        caption: ui_text(
            "With the post in the middle selected, Space opens its actions in fixed \
                  places: F fork, R reply, Q quote, L link post, V versions, O open; “R · 2” \
                  says two replies are known already. A letter previews what it would make; ⏎ \
                  does it, esc backs out. (Sample data: nothing is sent.)",
        ),
        keys: &[Key::LineageRing],
        key_label: ui_text("Space"),
        region: Region::Whole,
        button: None,
        setup: Setup::Lineage,
        pause_ms: 2400,
        settle: false,
        stay: true,
    },
    // --- author's site (0.21.0) ---
    Step {
        id: "author-site",
        since: "0.21.0",
        title: ui_text("Read it as its author made it"),
        caption: ui_text(
            "In Reader mode, Settings › Reading › Open posts on the author's site shows \
                  an opened post's own page, styled by its author. It fills the reading pane \
                  under the browser bar: Burrow reader | Author's site, back and forward, the \
                  address, 🛡 and ↗. j/k still move and Space pages down, and a subscription's \
                  menu can always or never do it. Press ⌥⌘2.",
        ),
        keys: &[Key::ReaderMode],
        key_label: ui_text("⌥⌘2"),
        region: Region::Reader,
        button: None,
        setup: Setup::Stream,
        pause_ms: 1600,
        settle: false,
        stay: false,
    },
    // --- 0.26.0 --- the open post's less common actions, behind ⋯ More.
    Step {
        id: "more-menu",
        since: "0.26.0",
        title: ui_text("⋯ More for this post"),
        caption: ui_text(
            "Under an open post, one row keeps the actions you use most: quote, reply, \
                  catch up and Background. The rest wait behind ⋯ More: a hopper, the shelf, \
                  a trail, fork, link post, an AI reply, → Notes and open on web. Their keys \
                  haven't changed. Click ⋯ More.",
        ),
        keys: &[Key::MoreMenu],
        key_label: ui_text("click"),
        region: Region::MoreChip,
        button: None,
        setup: Setup::Reader,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "reader",
        since: "0.4.0",
        title: ui_text("Reader: sources, posts, post"),
        caption: ui_text(
            "⌥⌘2 switches to Reader. On the left: smart feeds (All unread, Today, Thumbed, \
                  the Feed, Saved, Background, where set-aside and withdrawn posts go, and Raw), Your blyg \
                  (your own posts, never unread or counted), your hoppers, your folders, which \
                  stay on this Mac, and subscriptions. [ and ] step through a post's versions; ⌥⌘1 is the \
                  Stream again.",
        ),
        keys: &[Key::ReaderMode],
        key_label: ui_text("⌥⌘2"),
        region: Region::Reader,
        button: None,
        setup: Setup::Stream,
        pause_ms: 3000,
        settle: false,
        stay: true,
    },
    // --- 0.28.0 --- studio 0.68's reading tabs: the pace-layered Feed, Raw.
    Step {
        id: "feed-order",
        since: "0.28.0",
        title: ui_text("The Feed, pace-layered"),
        caption: ui_text(
            "Reading has blygger-studio 0.68's tabs in the sources: “Feed”, Mentions, \
                  “Agentic feed”, “Saved” (the shelf, renamed), Background and “Raw”. The Feed \
                  is pace-layered: new posts arrive on top and sink as others come, more slowly \
                  the more their blyg pinned them, and a new version climbs one place. Click \
                  “Chronological” for newest first; your blyg remembers the choice.",
        ),
        keys: &[Key::FeedOrder],
        key_label: ui_text("click"),
        region: Region::Reader,
        button: None,
        setup: Setup::FeedOrder,
        pause_ms: 1800,
        settle: false,
        stay: true,
    },
    Step {
        id: "raw-tab",
        since: "0.28.0",
        title: ui_text("Raw: what came in"),
        caption: ui_text(
            "“Raw” lists what came in from the blygs you read, newest first, as it arrived: \
                  nothing moved to Background, nothing folded under a thread. Small edits, \
                  refreshed quotes and posts their blyg followed within 15 minutes go to \
                  Background now; Settings (⌘,) has a switch for each rule. Click Raw.",
        ),
        keys: &[Key::RawTab],
        key_label: ui_text("click"),
        region: Region::Reader,
        button: None,
        setup: Setup::FeedOrder,
        pause_ms: 1800,
        settle: false,
        stay: true,
    },
    // --- 0.28.0 --- studio 0.71: a card's date opens it; when an entry
    // counts as read.
    Step {
        id: "count-read",
        since: "0.28.0",
        title: ui_text("Open a post from its date"),
        caption: ui_text(
            "Click the date on the highlighted card: its post opens in full beside the \
                  Stream (in the Reader, a row's date opens it too). On a blyg with \
                  blygger-studio 0.70 or later, Settings (⌘,) › READ MARKS has “Count an entry \
                  as read”: when a card has been on screen for about a second, when you open \
                  it, or only when you mark it.",
        ),
        keys: &[Key::StreamDate],
        key_label: ui_text("click"),
        region: Region::StreamPost,
        button: None,
        setup: Setup::Stream,
        pause_ms: 1800,
        settle: false,
        stay: true,
    },
    // --- 0.28.0 --- studio 0.69: the Agentic feed's edition.
    Step {
        id: "agentic-edition",
        since: "0.28.0",
        title: ui_text("Your assistant's edition"),
        caption: ui_text(
            "On blygger-studio 0.69 an assistant can write you an edition: your reading as a \
                  digest, in sections, each post with a headline, a short summary and why it's \
                  there. The Agentic feed tab shows it, under the name your blyg gives it (here, \
                  Morning paper). Click it; j/k step through its posts and ⏎ opens one.",
        ),
        keys: &[Key::AgenticTab],
        key_label: ui_text("click"),
        region: Region::Reader,
        button: None,
        setup: Setup::FeedOrder,
        pause_ms: 1800,
        settle: false,
        stay: true,
    },
    // --- 0.24.0 --- Background, and the shelf on the reading side.
    Step {
        id: "background",
        since: "0.24.0",
        title: ui_text("Set a post aside: Background"),
        caption: ui_text(
            "b (or ↓ Background under the post) moves it to Background: out of the \
                  stream, the feeds, Mentions and the counts, and nothing is marked read. \
                  ↑ Foreground brings it back; ⋯ More under the post has ↺ Clear my choice \
                  (the rules decide again) and ⇊, which moves a whole blyg. Your blyg keeps the list, so the studio shows the same \
                  (blygger-studio 0.61).",
        ),
        keys: &[Key::Background],
        key_label: ui_text("b"),
        region: Region::ReaderPost,
        button: None,
        setup: Setup::Background,
        pause_ms: 1800,
        settle: false,
        stay: true,
    },
    // --- 0.27.0 --- Background says why; a thread's parts (studio 0.64).
    Step {
        id: "parts",
        since: "0.27.0",
        title: ui_text("A thread and its parts"),
        caption: ui_text(
            "Lin published this thread together with the posts it quotes. The parts go to \
                  Background, each with a badge saying why, “published with ‘…’”, and the thread says \
                  “+2 parts”. Press p (or click it) to unfold them under the card, then 1 or 2 \
                  to open one. Settings › BACKGROUND has the rules, and turning “Parts \
                  published with a thread” off puts the parts back in your lists.",
        ),
        keys: &[Key::Parts],
        key_label: ui_text("p"),
        region: Region::Stream,
        button: None,
        setup: Setup::Parts,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "shelf-add",
        since: "0.24.0",
        title: ui_text("Save anyone's post"),
        caption: ui_text(
            "“+ Save”, in ⋯ More under the post, keeps it to hand in Saved: someone else's \
                  post or yours, up to 50, in your order. With blygger-studio 0.61 Saved \
                  syncs with your blyg, so the studio and your other computers share it.",
        ),
        keys: &[Key::ShelfReading],
        key_label: ui_text("click"),
        region: Region::ReaderPost,
        button: None,
        setup: Setup::ShelfAdd,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "shelf-lens",
        since: "0.24.0",
        title: ui_text("Saved"),
        caption: ui_text(
            "“Saved”, in the sources, lists everything you saved in your order, as the \
                  studio's Saved tab does (it was the Shelf). A saved post stays here even \
                  after it goes to Background. Click Saved.",
        ),
        keys: &[Key::ShelfLens],
        key_label: ui_text("click"),
        region: Region::Reader,
        button: None,
        setup: Setup::ShelfLens,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- 0.25.0 --- the Mentions feed, tagged.
    Step {
        id: "mentions-feed",
        since: "0.25.0",
        title: ui_text("Mentions, and why"),
        caption: ui_text(
            "Mentions, under All unread, lists the posts that point at you: the ones \
                  your blyg says name you, and the ones Burrow finds that quote, reply to, \
                  fork or link to your blyg. A tag on each says which. Click Mentions.",
        ),
        keys: &[Key::MentionsFeed],
        key_label: ui_text("click"),
        region: Region::Reader,
        button: None,
        setup: Setup::MentionsFeed,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- hoppers ---
    Step {
        id: "hoppers",
        since: "0.15.0",
        title: ui_text("Hoppers: posts you keep together"),
        caption: ui_text(
            "Under Hoppers on the left are your blyg's hoppers, collections of posts, yours \
                  and others'. Pick one to list its posts. h (or Hopper › in ⋯ More under a post) puts the \
                  open post in a hopper or takes it out; a public one, marked with a globe, has its own page.",
        ),
        keys: &[Key::HopperPicker],
        key_label: ui_text("h"),
        region: Region::Reader,
        button: None,
        setup: Setup::Hoppers,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "hopper-several",
        since: "0.17.0",
        title: ui_text("Several posts into a hopper"),
        caption: ui_text(
            "Two posts are picked (⌘-click picks one more, ⇧-click a run, ⌘A the whole \
                  list). h now acts on all of them: choosing a hopper puts in the ones it \
                  doesn't hold yet, and a dash marks a hopper holding only some. A hopper lists \
                  its posts as your blyg does, the most recently added first.",
        ),
        keys: &[Key::HopperSeveral],
        key_label: ui_text("h"),
        region: Region::Reader,
        button: None,
        setup: Setup::HopperSeveral,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "thicket",
        since: "0.14.0",
        title: ui_text("Follow the thicket"),
        caption: ui_text(
            "Ada's reply quotes Lin's note. Click the quote to open it here; quotes inside \
                  quotes, the “↳ reply to” line and links to blyg posts open the same \
                  way. The breadcrumb above the post shows the path, and ⌘[ or ‹ back steps \
                  back.",
        ),
        keys: &[Key::FollowThicket],
        key_label: ui_text("click"),
        region: Region::ReaderPost,
        button: None,
        setup: Setup::Thicket,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "history",
        since: "0.14.0",
        title: ui_text("Where you've been"),
        caption: ui_text(
            "⇧⌘H, or history above the post, lists this walk and earlier ones beside the \
                  reader, as the tree they were; each step opens its post. It's private: kept \
                  in your blyg's own browsing log, or on this Mac, and never published.",
        ),
        keys: &[Key::BrowseHistory],
        key_label: ui_text("⇧⌘H"),
        region: Region::ReaderPost,
        button: None,
        setup: Setup::Walked,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "stub-passage",
        since: "0.14.0",
        title: ui_text("Reply to a passage"),
        caption: ui_text(
            "Reply made this draft, quoting the whole of Lin's post. To answer one passage, \
                  select it inside the quote in the preview on the right, then press ❝ quote \
                  only this (or ⇧⌘D): it goes under that quote. Quote another passage adds one \
                  more to select from.",
        ),
        keys: &[Key::StubPassage],
        key_label: ui_text("⇧⌘D"),
        region: Region::Editor,
        button: None,
        setup: Setup::Stub,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "opml",
        since: "0.11.0",
        title: ui_text("Moving from another reader?"),
        caption: ui_text(
            "Import OPML… (here on Subscriptions, ⇧⌘S, or Blyg › Import Subscriptions \
                  from OPML…) reads its export: Burrow lists the feeds, ticks the new ones, and \
                  files them in an Imported feeds folder in the Reader, out of your public \
                  blogroll. Export OPML… saves yours. Try the sample file.",
        ),
        keys: &[Key::OpmlImport],
        key_label: ui_text("click"),
        region: Region::ScreenHeader,
        button: None,
        setup: Setup::Subscriptions,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- discover --- (desktop only)
    Step {
        id: "discover",
        since: "0.24.0",
        title: ui_text("Find new blygs to read"),
        caption: ui_text(
            "Discover… (here, or Blyg › Discover Blygs…) lists the blygs on blygger.com's \
                  public directory that you don't follow yet, newest first, with how active \
                  each is. Ones listed since you last looked say new. j/k move, ⏎ \
                  subscribes, o opens one in your browser. (The tour's list is a sample.)",
        ),
        keys: &[Key::Discover],
        key_label: ui_text("click"),
        region: Region::ScreenHeader,
        button: None,
        setup: Setup::Subscriptions,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "notes",
        since: "0.4.0",
        title: ui_text("Notes while you read"),
        caption: ui_text(
            "⇧⌘N, or the Notes tab at the right edge, opens the notes beside what you're \
                  reading or writing; they stay open until you close them, and esc takes you \
                  back to your draft. Reading notes are running notes, kept on this Mac as a \
                  scratch note; → Notes, in ⋯ More at the bottom of a post, adds the post to them. Select \
                  a passage in a post: ⇧⌘D, or the pill beside it, quotes it in your draft. The \
                  pane's Notes tab is next.",
        ),
        keys: &[Key::Notes],
        key_label: ui_text("⇧⌘N"),
        region: Region::ReaderPost,
        button: None,
        setup: Setup::Reader,
        pause_ms: 2200,
        settle: false,
        stay: true,
    },
    Step {
        id: "notes-library",
        since: "0.11.0",
        title: ui_text("Your Markdown notes, in the drawer"),
        caption: ui_text(
            "With markdown-notes on, the Notes tab opens folders of Markdown notes \
                  (Obsidian vaults, say): a chip per folder, Add folder… for more, and Remove \
                  from Burrow, which leaves the folder alone. Open a note to edit it or quote it \
                  into your post. Choose Garden.",
        ),
        keys: &[Key::NotesVault],
        key_label: ui_text("click"),
        region: Region::NotesDrawer,
        button: None,
        setup: Setup::NotesLibrary,
        pause_ms: 2200,
        settle: false,
        stay: true,
    },
    Step {
        id: "browser",
        since: "0.4.0",
        title: ui_text("Links open beside your reading"),
        // No content blocking on Windows yet: no word about 🛡 there.
        caption: ui_text_per_platform(
            "A link in a post opens in this pane (a blank sample here); ⌘-click opens it \
             over the whole area. 🛡 turns ad and tracker blocking off for one site; ↗, or \
             ⌥-click on the link, uses your default browser. esc closes it; ⇧⌘B brings it \
             back.",
            "A link in a post opens in this pane (a blank sample here); Ctrl+click opens it \
             over the whole area. ↗, or Alt+click on the link, uses your default browser. \
             esc closes it; Ctrl+Shift+B brings it back.",
        ),
        keys: &[Key::CloseBrowser],
        key_label: ui_text("esc"),
        region: Region::BrowserChrome,
        button: None,
        setup: Setup::Browser,
        pause_ms: 700,
        settle: false,
        stay: false,
    },
    // --- browser on Windows (0.22.0) --- the pane and cross-post on Windows.
    Step {
        id: "browser-windows",
        since: "0.22.0",
        title: ui_text_per_platform(
            "The browser pane runs on Windows too",
            "The browser pane, on Windows",
        ),
        caption: ui_text_per_platform(
            "The same pane, and the same cross-post macros, now run on Windows too. Press \
             ⇧⌘B to hide the pane; it brings it back too.",
            "Links in posts now open in this pane on Windows, and cross-post's macros run \
             here too. Sign-ins are kept in the pane's own profile, so they survive a \
             relaunch. Ad and tracker blocking isn't on Windows yet. Press Ctrl+Shift+B to \
             hide the pane; it brings it back too.",
        ),
        keys: &[Key::CloseBrowser],
        key_label: ui_text("⇧⌘B"),
        region: Region::BrowserChrome,
        button: None,
        setup: Setup::Browser,
        pause_ms: 700,
        settle: false,
        stay: false,
    },
    // --- end browser on Windows ---
    Step {
        id: "clip",
        since: "0.10.0",
        title: ui_text("✂ Clip a page into your draft"),
        caption: ui_text(
            "⇧⌘C, or ✂ Clip in the pane's bar, quotes the page (or the passage you \
                  selected on it) in your draft, with its title and link. Clip the sample.",
        ),
        keys: &[Key::Clip],
        key_label: ui_text("⇧⌘C"),
        region: Region::BrowserChrome,
        button: None,
        setup: Setup::Browser,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "cross-post",
        since: "0.11.0",
        title: ui_text("cross-post runs beside you"),
        caption: ui_text(
            "The bundled cross-post (off until you turn it on) puts a published post on \
                  Substack Notes, Bluesky or Farcaster from this pane: you check the text, it \
                  fills in the composer, and only you click Post. The copy's link goes on the \
                  post's Copies Elsewhere list. esc folds the pane mid-run: the status bar says \
                  “macro running · show pane”. It works on Windows as well. (A sample: nothing \
                  runs.)",
        ),
        keys: &[Key::MacroFolded],
        key_label: ui_text("esc"),
        region: Region::MacroRun,
        button: None,
        setup: Setup::CrossPost,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- auto cross-post (0.28.0) ---
    Step {
        id: "auto-crosspost",
        since: "0.28.0",
        title: ui_text("Cross-post a post each time you publish it"),
        caption: ui_text(
            "With cross-post on, the Publish sheet has “Then cross-post to”: tick a site (⌥ and \
                  its number), and each time you publish that post Burrow starts its \
                  cross-post by itself, one site after another. Press ⇧⌘P: this post has an \
                  on/off row per site there too, Bluesky on. It only starts the run: you \
                  still check the text and click Post. (A sample: nothing runs.)",
        ),
        keys: &[Key::Palette],
        key_label: ui_text("⇧⌘P"),
        region: Region::Whole,
        button: None,
        setup: Setup::AutoCrossPost,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "extensions",
        since: "0.10.0",
        title: ui_text("Extensions"),
        caption: ui_text(
            "Post › Extensions… lists what your extensions offer here: their commands, \
                  Browse for each notes folder, and Manage extensions…; the command palette \
                  has them too. Burrow asks before one may do anything, and none can publish \
                  or see your sign-in.",
        ),
        keys: &[Key::ExtPalette],
        key_label: ui_text("Post menu"),
        region: Region::Whole,
        button: None,
        setup: Setup::Posts,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "manage",
        since: "0.11.0",
        title: ui_text("Turn off, or forget"),
        caption: ui_text(
            "Choose Manage extensions…: what's installed, on and allowed. Turn off stops \
                  one and keeps its permissions for next time; Forget permissions drops them \
                  too, so it asks again. (The tour changes nothing in your config file.)",
        ),
        keys: &[Key::ExtManage],
        key_label: ui_text("click"),
        region: Region::Whole,
        button: None,
        setup: Setup::Palette,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "consent",
        since: "0.11.0",
        title: ui_text("Burrow asks first"),
        caption: ui_text(
            "The first time an extension starts, Burrow lists what it wants. ⏎ allows \
                  what's ticked (1–9 untick), esc is Not now, and ⌘⌫ turns it off. Try ⌘⌫. \
                  (A sample: nothing is saved.)",
        ),
        keys: &[Key::ConsentAnswered],
        key_label: ui_text("⌘⌫"),
        region: Region::Whole,
        button: None,
        setup: Setup::Consent,
        pause_ms: 1200,
        settle: false,
        stay: false,
    },
    // --- studio parity (0.19.0) --- running the blyg from Burrow.
    Step {
        id: "site",
        since: "0.19.0",
        title: ui_text("Your blyg's public site"),
        caption: ui_text(
            "Your blyg's public pages have their own look. Blyg › Site… picks the \
                  theme (or designs your own, with a preview), the typeface, a front page, \
                  the menu on every page and your avatar. Nothing changes on your blyg \
                  until you press Save.",
        ),
        keys: &[Key::SiteSheet],
        key_label: ui_text("Blyg › Site…"),
        region: Region::Whole,
        button: None,
        setup: Setup::Posts,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "contacts-book",
        since: "0.19.0",
        title: ui_text("Your contacts, kept here"),
        caption: ui_text(
            "Blyg › Contacts… is your blyg's address book: add, edit, check and \
                  remove the people you write about as @handles. Post › Add Author to \
                  Contacts… adds whoever wrote the post in front of you, and subscribing \
                  to a blyg offers to add it.",
        ),
        keys: &[Key::ContactsSheet],
        key_label: ui_text("Blyg › Contacts…"),
        region: Region::Whole,
        button: None,
        setup: Setup::Posts,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "stale-quotes",
        since: "0.19.0",
        title: ui_text("Quotes that fell behind"),
        caption: ui_text(
            "When a post you quoted gets a new version, your thread shows ↻ in the \
                  list. Blyg › Stale Quotes… lists those threads and refreshes their \
                  quotes: a new version of your thread, with a note you can edit.",
        ),
        keys: &[Key::StaleQuotes],
        key_label: ui_text("Blyg › Stale Quotes…"),
        region: Region::Whole,
        button: None,
        setup: Setup::Posts,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "client-access",
        since: "0.19.0",
        title: ui_text("Who can act as you"),
        caption: ui_text(
            "Blyg › Client Access… lists every app and token that can act as you on \
                  your blyg, Burrow included. Make a token (shown once) or revoke any of \
                  them. If your blyg has its own AI, Blyg › AI on Your Blyg… sets it up, \
                  and the publish sheet can draft a version note for you to edit.",
        ),
        keys: &[Key::ClientAccess],
        key_label: ui_text("Blyg › Client Access…"),
        region: Region::Whole,
        button: None,
        setup: Setup::Posts,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- end studio parity ---
    // --- 0.21.0 --- copies elsewhere (decision #98) and the author's site.
    Step {
        id: "copies-elsewhere",
        since: "0.21.0",
        title: ui_text("Copies elsewhere"),
        caption: ui_text(
            "Posted it somewhere else too? Post › Copies Elsewhere… keeps that list on \
                  your blyg, with no new version, and a cross-post adds its link for you; the \
                  versions (⌘Y) show it, and readers see “the author says it is also at …”. (It shows once \
                  your blyg's studio supports it.) Press ⌘Y.",
        ),
        keys: &[Key::ShowVersions],
        key_label: ui_text("⌘Y"),
        region: Region::Whole,
        button: None,
        setup: Setup::Versioned,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- end 0.21.0 ---
    // --- side dock (0.20.0) --- notes and Nearby beside the draft.
    Step {
        id: "side-dock",
        since: "0.20.0",
        title: ui_text("Notes beside your draft"),
        caption: ui_text(
            "The tabs at the right edge open your notes and Nearby beside the editor, and \
             they stay open while you write. ⇧⌘N moves into the notes, esc comes back to \
             your draft. Drag a pane's edge, or the line between the editor and the \
             preview, to resize; a double-click resets it. Press ⇧⌘N.",
        ),
        keys: &[Key::Notes],
        key_label: ui_text("⇧⌘N"),
        region: Region::Whole,
        button: None,
        setup: Setup::Posts,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- end side dock ---
    // --- tours --- the end cards: no key, only Next (and the chooser).
    Step {
        id: QUICK_END,
        since: "0.1.0",
        title: ui_text("That's the quick tour"),
        caption: ui_text(
            "Everything you did here was on sample data. The feature tours below go \
                  further, a few minutes each: pick one now, or any time from Settings (⌘,) \
                  › Help.",
        ),
        keys: &[],
        key_label: ui_text(""),
        region: Region::Whole,
        button: None,
        setup: Setup::Posts,
        pause_ms: 0,
        settle: false,
        stay: false,
    },
    Step {
        id: END,
        since: "0.1.0",
        title: ui_text("That's the tour"),
        caption: ui_text(
            "Everything you did here was on sample data. Your own posts and reading come \
                  back when you finish. Back to the tours picks another; Settings (⌘,) › Help \
                  opens them again.",
        ),
        keys: &[],
        key_label: ui_text(""),
        region: Region::Whole,
        button: None,
        setup: Setup::Posts,
        pause_ms: 0,
        settle: false,
        stay: false,
    },
];

// ================================================================ tours

/// The quick tour's end card: it lists the feature tours.
pub const QUICK_END: &str = "quick-done";
/// The end card of a feature tour and of what's new.
pub const END: &str = "done";

/// A tour: a few steps of `STEPS`, by id, in table order. Its end card
/// follows the last (`run`).
#[derive(Debug, Clone, Copy)]
pub struct Tour {
    pub id: &'static str,
    pub title: &'static str,
    /// One short line for the chooser.
    pub blurb: &'static str,
    pub steps: &'static [&'static str],
}

/// The essentials: what a new user needs first.
pub const QUICK: Tour = Tour {
    id: "quick",
    title: "Quick tour",
    blurb: "The essentials: find, write, publish and read, in a couple of minutes.",
    steps: &[
        "search",
        "new",
        "autosave",
        "kinds",
        "publish",
        "publish-note",
        "versions",
        "stream",
        "side-dock",
    ],
};

/// The feature tours, in the chooser's order (keys 1–9).
pub const TOURS: &[Tour] = &[
    Tour {
        id: "writing",
        title: "Writing",
        blurb: "Search, drafts, kinds, links and mentions.",
        steps: &[
            "search",
            "create",
            "new",
            "autosave",
            "kinds",
            "full-editor",
            "write-view",
            "shelf",
            "hover-cards",
            "loose-ends",
            "quiet-writing",
            "quotes",
            "tangle",
            "maths",
            "mentions",
        ],
    },
    Tour {
        id: "ai",
        title: "AI and TK",
        blurb: "Fill a gap, a link it can't read, shorten, an added quote.",
        steps: &["tk", "tk-checkpoint", "shorten", "echoed-quote"],
    },
    Tour {
        id: "publishing",
        title: "Publishing and versions",
        blurb: "Versions, pins, discard, conflicts, copies.",
        steps: &[
            "publish",
            "publish-note",
            "versions",
            "discard",
            "conflict",
            "own-post",
            "copies-elsewhere",
        ],
    },
    Tour {
        id: "reading",
        title: "Reading",
        blurb: "Stream, catch up, Reader, the Feed, Background, Saved, Discover.",
        steps: &[
            "stream",
            "compose",
            "slots",
            "generated",
            "original",
            "catch-up",
            "author-site",
            "more-menu",
            "reader",
            "feed-order",
            "raw-tab",
            "count-read",
            "agentic-edition",
            "background",
            "parts",
            "shelf-add",
            "shelf-lens",
            "mentions-feed",
            "opml",
            "discover",
        ],
    },
    Tour {
        id: "lineage",
        title: "Hoppers, lineage and quotes",
        blurb: "Lineage, trails, the quote tray, hoppers.",
        steps: &[
            "lineage-writing",
            "neighbourhood",
            "trails",
            "quote-tray",
            "lineage",
            "ring",
            "hoppers",
            "hopper-several",
            "thicket",
            "history",
            "stub-passage",
        ],
    },
    Tour {
        id: "notes",
        title: "Notes, browser and capture",
        blurb: "Quick capture, notes, the browser pane.",
        steps: &[
            "capture",
            "capture-link",
            "notes",
            "notes-library",
            "browser",
            "browser-windows",
            "clip",
        ],
    },
    Tour {
        id: "extensions",
        title: "Extensions and cross-post",
        blurb: "Cross-post, and what extensions may do.",
        steps: &[
            "cross-post",
            "auto-crosspost",
            "extensions",
            "manage",
            "consent",
        ],
    },
    Tour {
        id: "blyg",
        title: "Your blyg",
        blurb: "Site, contacts, client access, switching.",
        steps: &[
            "contacts",
            "switcher",
            "site",
            "contacts-book",
            "stale-quotes",
            "client-access",
        ],
    },
    Tour {
        id: "workspace",
        title: "Workspace",
        blurb: "Palette, focus, split, dock.",
        steps: &[
            "palette",
            "focus",
            "serendipity",
            "split-view",
            "nearby",
            "side-dock",
        ],
    },
];

/// Which tour is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Quick,
    /// `TOURS[i]`.
    Feature(usize),
    /// What's new: the steps the news's releases added.
    News,
}

impl Pick {
    pub fn tour(self) -> Option<&'static Tour> {
        match self {
            Pick::Quick => Some(&QUICK),
            Pick::Feature(i) => TOURS.get(i),
            Pick::News => None,
        }
    }
}

/// The step `id`'s index in `STEPS`.
pub fn index(id: &str) -> Option<usize> {
    STEPS.iter().position(|s| s.id == id)
}

/// The feature tour that holds step `id`.
pub fn tour_of(id: &str) -> Option<usize> {
    TOURS.iter().position(|t| t.steps.contains(&id))
}

/// A tour's steps as `STEPS` indices, its end card last.
pub fn run(tour: &Tour) -> Vec<usize> {
    let end = if tour.id == QUICK.id { QUICK_END } else { END };
    tour.steps
        .iter()
        .copied()
        .chain([end])
        .filter_map(index)
        .collect()
}

/// What's new: the steps whose `since` is one of `versions`, in table
/// order, then the end card.
pub fn news_run(versions: &[&str]) -> Vec<usize> {
    STEPS
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.keys.is_empty() && versions.contains(&s.since))
        .map(|(i, _)| i)
        .chain(index(END))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sound() {
        let mut ids: Vec<_> = STEPS.iter().map(|s| s.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), STEPS.len(), "duplicate step id");
        for s in STEPS {
            assert!(
                !s.title.get().is_empty() && !s.caption.get().is_empty(),
                "{}",
                s.id
            );
            // Each tour ends on its own end card; only those wait for Next.
            let end = s.id == END || s.id == QUICK_END;
            assert_eq!(
                s.keys.is_empty(),
                end,
                "{}: only an end card waits for Next",
                s.id
            );
            // A step that moves on by itself shows its ✓ first.
            assert!(end || s.stay || s.pause_ms > 0, "{}: pause_ms 0", s.id);
            assert_eq!(s.key_label.get().is_empty(), s.keys.is_empty(), "{}", s.id);
        }
        // Every topic in the spec is covered.
        for k in [
            Key::MoveSelection,
            Key::Enter,
            Key::Typed,
            Key::ToggleKind,
            Key::MakeDraft,
            Key::Publish,
            Key::ViewStudio,
            Key::ViewWrite,
            Key::AiGenerate,
            Key::AiShorten,
            Key::ShowVersions,
            Key::QuotePicker,
            Key::Mention,
            Key::ReadMore,
            Key::OpenOriginal,
            Key::Lineage,
            Key::LineageRing,
            Key::ReaderMode,
            Key::Notes,
            Key::CloseBrowser,
            Key::NewPost,
            Key::Clip,
            Key::ExtPalette,
            Key::ExtManage,
            Key::ConsentAnswered,
            Key::SlotSheet,
            Key::NotesVault,
            Key::MacroFolded,
            Key::OpmlImport,
            Key::DiscardChanges,
            Key::OwnPostAction,
            Key::Picker,
            Key::Compose,
            Key::FollowThicket,
            Key::BrowseHistory,
            Key::StubPassage,
            Key::BlygSwitcher,
            Key::HopperPicker,
            Key::HopperSeveral,
            Key::Shelf,
            Key::LineageStrip,
            Key::LineageMap,
            Key::HoverCard,
            Key::Palette,     // --- flow ---
            Key::Focus,       // --- flow ---
            Key::Serendipity, // --- flow ---
            Key::Trail,       // --- tangles ---
            Key::Tray,
            Key::Split,
            Key::Nearby,  // --- nearby ---
            Key::CatchUp, // --- catch up ---
            Key::Background,
            Key::ShelfReading,
            Key::ShelfLens,
            Key::Discover,
            Key::MentionsFeed,
            Key::FeedOrder, // --- 0.28.0 ---
            Key::RawTab,
            Key::AgenticTab,
            Key::StreamDate, // --- studio 0.71 ---
            Key::TkCheckpoint,
            Key::MoreMenu, // --- 0.26.0 ---
            Key::Tangle,   // --- 0.27.0 ---
            Key::Formula,
            Key::Parts,
        ] {
            assert!(STEPS.iter().any(|s| s.accepts(k)), "{k:?} isn't taught");
        }
    }

    /// Windows' wording, read on any platform: no macOS glyph survives the
    /// respelling, and a step words Windows its own way only when the
    /// respelling wouldn't already say the same.
    #[test]
    fn windows_wording_is_derived_unless_it_differs() {
        use crate::platform_text::Platform;
        let mut bad = Vec::new();
        for s in STEPS {
            for (field, t) in [
                ("title", s.title),
                ("caption", s.caption),
                ("key_label", s.key_label),
            ] {
                let win = t.on(Platform::Windows);
                if win.contains(['⌘', '⌥', '⌃', '⇧']) {
                    bad.push(format!("  {}.{field} still has a macOS key: {win}", s.id));
                }
                if let Some(own) = t.windows_source()
                    && crate::keymap::windows_hint(own)
                        == crate::keymap::windows_hint(t.source(Platform::Mac))
                {
                    bad.push(format!(
                        "  {}.{field}: its Windows wording is the macOS one respelled; \
                         use ui_text(mac) alone",
                        s.id
                    ));
                }
            }
        }
        assert!(bad.is_empty(), "Windows wording:\n{}", bad.join("\n"));
    }

    #[test]
    fn every_step_is_in_exactly_one_feature_tour() {
        for s in STEPS {
            let n = TOURS.iter().filter(|t| t.steps.contains(&s.id)).count();
            let end = s.id == END || s.id == QUICK_END;
            assert_eq!(n, usize::from(!end), "{} is in {n} feature tours", s.id);
        }
        for t in TOURS.iter().chain([&QUICK]) {
            assert!(!t.title.is_empty() && !t.blurb.is_empty(), "{}", t.id);
            assert!(!t.steps.is_empty(), "{}", t.id);
            // Every id exists, and the tour keeps the table's order.
            let ix: Vec<usize> = t
                .steps
                .iter()
                .map(|id| index(id).unwrap_or_else(|| panic!("{}: no step {id}", t.id)))
                .collect();
            assert!(ix.windows(2).all(|w| w[0] < w[1]), "{}: out of order", t.id);
            let r = run(t);
            assert_eq!(r.len(), t.steps.len() + 1, "{}", t.id);
            assert!(STEPS[*r.last().unwrap()].keys.is_empty(), "{}", t.id);
        }
        let mut ids: Vec<_> = TOURS.iter().map(|t| t.id).chain([QUICK.id]).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), TOURS.len() + 1, "duplicate tour id");
        // The chooser's number keys are 1–9.
        assert!(TOURS.len() <= 9);
    }

    #[test]
    fn the_quick_tour_is_quick() {
        let r = run(&QUICK);
        assert!(r.len() <= 10, "{} cards", r.len());
        assert_eq!(STEPS[*r.last().unwrap()].id, QUICK_END);
        assert_eq!(QUICK.steps[0], "search", "it starts where Burrow does");
    }

    #[test]
    fn whats_new_runs_only_the_releases_steps() {
        let ids = |r: Vec<usize>| r.into_iter().map(|i| STEPS[i].id).collect::<Vec<_>>();
        assert_eq!(
            ids(news_run(&["0.21.0"])),
            ["author-site", "copies-elsewhere", END]
        );
        assert_eq!(ids(news_run(&["0.23.0"])), ["catch-up", END]);
        assert_eq!(
            ids(news_run(&["0.26.0"])),
            ["quiet-writing", "more-menu", END]
        );
        assert_eq!(
            ids(news_run(&["0.27.0"])),
            ["tangle", "maths", "parts", END]
        );
        assert_eq!(ids(news_run(&["0.8.0"])), ["lineage", "ring", END]);
        assert_eq!(ids(news_run(&["9.9.9"])), [END]);
        assert_eq!(tour_of("author-site"), Some(3));
        assert_eq!(tour_of(END), None);
    }

    #[test]
    fn every_step_says_which_release_it_teaches() {
        for s in STEPS {
            let v: Vec<_> = s.since.split('.').collect();
            assert!(
                v.len() == 3 && v.iter().all(|n| n.parse::<u32>().is_ok()),
                "{}: since = {:?}",
                s.id,
                s.since
            );
        }
    }
}
