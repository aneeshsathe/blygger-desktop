//! The tutorial's script: one row per step, in order. Edit the table to
//! change the tour; `tutorial.rs` only interprets it.
//!
//! A step waits for one of its `keys` (a GPUI action the user triggers in
//! the real window, never synthetic input), then moves on. Next and Back
//! always work too. `setup` prepares the sample data for the step when it
//! is entered, from either direction.

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
    // --- chat ---
    /// The #thicket chat tab came to the front (⇧⌘I or the tab).
    Chat,
    // --- terminal ---
    /// The terminal pane was shown or hidden (⌃`).
    Terminal,
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
}

/// What a step prepares on the sample data when it's entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setup {
    Nothing,
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
    /// The draft with a TK scope appended, caret inside it.
    Tk(&'static str),
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
    /// A sample consent sheet for markdown-notes (answering it writes
    /// nothing).
    Consent,
    /// The Subscriptions screen (⇧⌘S).
    Subscriptions,
}

/// One step of the tour.
#[derive(Debug, Clone, Copy)]
pub struct Step {
    pub id: &'static str,
    /// The release whose feature this step was added to teach. A what's-new
    /// entry starts at the first step of its own release (`whats_new`'s
    /// tests check it), so a release with something new adds a step for it.
    pub since: &'static str,
    pub title: &'static str,
    /// One or two short sentences. `{hotkey}` becomes the quick-capture key.
    pub caption: &'static str,
    /// Any of these moves on. Empty: only Next does.
    pub keys: &'static [Key],
    /// How the key is shown on the card, e.g. "⌘T".
    pub key_label: &'static str,
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

pub const STEPS: &[Step] = &[
    Step {
        id: "search",
        since: "0.1.0",
        title: "Search is the interface",
        caption: "Type to filter every post as you go. ↑ and ↓ move the selection, and the \
                  editor previews each post as you pass it.",
        keys: &[Key::MoveSelection],
        key_label: "↑ ↓",
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
        title: "No match? ⏎ starts writing",
        caption: "Nothing matches “tide pools”, so ⏎ creates a new draft seeded with it, \
                  with the caret at the end.",
        keys: &[Key::Enter],
        key_label: "⏎",
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
        title: "⌘N: a blank page",
        caption: "⌘N opens an empty editor and leaves your search as it was. What you type is \
                  a scratch note, kept on this Mac until ⌘D makes it a draft (⌘⏎ publishes \
                  it). Leave without typing and nothing is kept.",
        keys: &[Key::NewPost],
        key_label: "⌘N",
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
        title: "Just write",
        caption: "Every keystroke is saved on this Mac and pushed to your blyg a moment after \
                  you stop. There's no save button. Type a few words.",
        keys: &[Key::Typed],
        key_label: "type",
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
        title: "Fragments and threads",
        caption: "A fragment is short: 1000 characters at most, counted down here. A thread \
                  has no limit and can quote other posts. ⌘T switches between them.",
        keys: &[Key::ToggleKind],
        key_label: "⌘T",
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
        title: "Quick capture keeps scratch notes",
        caption: "{hotkey} opens quick capture over any app. What you jot down stays on this \
                  Mac as a scratch note, listed here with its scratch pill and never synced. \
                  ⌘D makes it a draft; ⌘⏎ publishes it.",
        keys: &[Key::MakeDraft],
        key_label: "⌘D",
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
        title: "⌘⏎ publishes",
        caption: "Publishing asks for an optional version note first. This is sample data, so \
                  nothing leaves your Mac.",
        keys: &[Key::Publish],
        key_label: "⌘⏎",
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
        title: "Say what changed",
        caption: "Type a note such as “first version” (or leave it empty) and press ⏎. \
                  esc would cancel.",
        keys: &[Key::Enter, Key::Publish],
        key_label: "⏎",
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
        title: "The full editor",
        caption: "⌘2 adds a live preview beside the list; ⌘3 is the full editor: the source \
                  and the page exactly as it will be published.",
        keys: &[Key::ViewStudio, Key::ViewSplit],
        key_label: "⌘3",
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
        title: "Back to writing",
        caption: "⌘1 brings back the plain list and editor.",
        keys: &[Key::ViewWrite],
        key_label: "⌘1",
        region: Region::Whole,
        button: Some("Write"),
        setup: Setup::Nothing,
        pause_ms: 600,
        settle: false,
        stay: false,
    },
    Step {
        id: "tk",
        since: "0.1.0",
        title: "Fill a gap with AI",
        caption: "Write [TK]an instruction[/TK] and press ⌘G with the caret inside: the gap is \
                  filled, tinted as generated, and disclosed when published. (A canned demo: \
                  no AI is called.)",
        keys: &[Key::AiGenerate],
        key_label: "⌘G",
        region: Region::Editor,
        button: Some("Generate"),
        setup: Setup::Tk("one short sentence about fog, in my voice"),
        pause_ms: 1200,
        settle: true,
        stay: false,
    },
    Step {
        id: "shorten",
        since: "0.1.0",
        title: "Too long? Shorten to fit",
        caption: "This fragment is over 1000. ⇧⌘G proposes a shorter version; you accept or \
                  reject it. (Canned, too.)",
        keys: &[Key::AiShorten],
        key_label: "⇧⌘G",
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
        title: "Versions and pins",
        caption: "⌘Y shows every version of your post here, to compare or restore one. Pinning \
                  a version serves it forever: a pin can't be undone.",
        keys: &[Key::ShowVersions],
        key_label: "⌘Y",
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
        title: "Changed your mind? Discard",
        caption: if cfg!(target_os = "windows") {
            "This published post has unpublished edits: hover “unpublished edits” below. \
             ⌘⏎ would publish them as v4. Post › Discard Changes… or the toolbar's eraser \
             goes back to v3 as published, after asking. The public post never changes."
        } else {
            "This published post has unpublished edits: hover “unpublished edits” below. \
             ⌘⏎ would publish them as v4. ⌥⌘Z (or Post › Discard Changes…, or the \
             toolbar's eraser) goes back to v3 as published, after asking. The public post \
             never changes."
        },
        keys: &[Key::DiscardChanges],
        key_label: if cfg!(target_os = "windows") {
            "click"
        } else {
            "⌥⌘Z"
        },
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
        title: "Keep it on your shelf",
        caption: if cfg!(target_os = "windows") {
            "Some posts you keep coming back to: a /now page, a reading log, a running list. \
             Ctrl+Alt+B puts the open post on your shelf: 📚 marks it and it stays at the top \
             of the list. Ctrl+B opens the shelf, and 1–9 opens a post on it. The shelf is \
             kept on this computer."
        } else {
            "Some posts you keep coming back to: a /now page, a reading log, a running list. \
             ⌥⌘B puts the open post on your shelf: 📚 marks it and it stays at the top of the \
             list. ⌘B opens the shelf, and 1–9 opens a post on it. The shelf is kept on this \
             Mac."
        },
        keys: &[Key::Shelf],
        key_label: if cfg!(target_os = "windows") {
            "Ctrl+Alt+B"
        } else {
            "⌥⌘B"
        },
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
        title: "Lineage while you write",
        caption: if cfg!(target_os = "windows") {
            "Under the editor, one quiet line: this post's lineage glyph, what it draws on and \
             what draws on it (Rue replied to it, Ada quoted it). A click opens the lists, and \
             a row opens that post. Ctrl+J opens the full lineage, as in Reading."
        } else {
            "Under the editor, one quiet line: this post's lineage glyph, what it draws on and \
             what draws on it (Rue replied to it, Ada quoted it). A click opens the lists, and \
             a row opens that post. ⌘J opens the full lineage, as in Reading."
        },
        keys: &[Key::LineageStrip],
        key_label: if cfg!(target_os = "windows") {
            "Ctrl+J"
        } else {
            "⌘J"
        },
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
        title: "Zoom out to the neighbourhood",
        caption: "In the lineage sheet, − zooms out two hops: what the neighbours draw on and \
                  what draws on them, coloured fork, reply, quote; a dashed one is a hop \
                  further, a shaded one is yours. Arrows move, ⏎ makes a post the centre, ⌫ \
                  walks back, + returns to the list.",
        keys: &[Key::LineageMap],
        key_label: "−",
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
        title: "What a link is, on hover",
        caption: if cfg!(target_os = "windows") {
            "Rest the pointer on a [[…]] or ![[…]] in the editor: a card says what it is (title, \
             author, version, first lines), and an unlabelled link offers “label it”. \
             Ctrl-click opens the post: yours here, anyone else's in Reading. Quotes and links \
             in the Stream show the same card."
        } else {
            "Rest the pointer on a [[…]] or ![[…]] in the editor: a card says what it is (title, \
             author, version, first lines), and an unlabelled link offers “label it”. ⌘-click \
             opens the post: yours here, anyone else's in Reading. Quotes and links in the \
             Stream show the same card."
        },
        keys: &[Key::HoverCard],
        key_label: "hover",
        region: Region::Editor,
        button: None,
        setup: Setup::StudioSample,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- chat --- the setup screen only: the tour never connects.
    Step {
        id: "chat",
        since: "0.18.0",
        title: "#thicket, an experimental chat",
        caption: if cfg!(target_os = "windows") {
            "Ctrl+Shift+I opens #thicket, a chat for Burrow users on Libera.Chat (IRC), \
             beside the views. It's experimental and may be removed. Sign in or create an \
             account there when you like: nothing connects until you do, and Settings hides \
             the tab."
        } else {
            "⇧⌘I opens #thicket, a chat for Burrow users on Libera.Chat (IRC), beside the \
             views. It's experimental and may be removed. Sign in or create an account there \
             when you like: nothing connects until you do, and Settings hides the tab."
        },
        keys: &[Key::Chat],
        key_label: if cfg!(target_os = "windows") {
            "Ctrl+Shift+I"
        } else {
            "⇧⌘I"
        },
        region: Region::Whole,
        button: None,
        setup: Setup::Posts,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- terminal --- the key showing the pane counts: the tour never
    // needs a shell to start (none does in the tests).
    Step {
        id: "terminal",
        since: "0.18.0",
        title: "A terminal at the foot of the window",
        caption: if cfg!(target_os = "windows") {
            "Ctrl+` opens a terminal under your posts (experimental): your own shell, for a \
             CLI agent like claude or codex, git or wrangler. It gets nothing of Burrow's, no \
             token and no BLYGGER_* variable. Ctrl+` again hides it; Ctrl+Shift+T sends the \
             editor's selection to it, quoted."
        } else {
            "⌃` opens a terminal under your posts (experimental): your own shell, for a CLI \
             agent like claude or codex, git or wrangler. It gets nothing of Burrow's, no \
             token and no BLYGGER_* variable. ⌃` again hides it; ⇧⌘T sends the editor's \
             selection to it, quoted."
        },
        keys: &[Key::Terminal],
        key_label: if cfg!(target_os = "windows") {
            "Ctrl+`"
        } else {
            "⌃`"
        },
        region: Region::Whole,
        button: None,
        setup: Setup::Posts,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    // --- end terminal ---
    // --- capture --- loose ends, capture from anywhere, contacts.
    Step {
        id: "loose-ends",
        since: "0.18.0",
        title: "Loose ends",
        caption: "The search reads operators: is:draft lists your drafts, is:orphan the \
                  published posts nothing points at, has:tk the ungenerated TKs, is:stale \
                  drafts untouched for a month; links: and quotes: take an id or a host, and \
                  kind:thread, v:2 work too. Other words search as ever. With the search \
                  empty, a quiet line over the list says what's loose, in words.",
        keys: &[Key::Enter],
        key_label: "⏎",
        region: Region::Omnibar,
        button: None,
        setup: Setup::Query("is:draft"),
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "capture-link",
        since: "0.18.0",
        title: "Capture from anywhere",
        caption: if cfg!(target_os = "windows") {
            "Paste a web address into the search and ⏎ starts a fragment with that link, \
             titled from the page (a blyg post you hold is quoted instead). In the quick \
             capture panel, text you copied in the last ten seconds is offered as a chip, \
             \"quote clipboard\"; it's never put in for you."
        } else {
            "Paste a web address into the search and ⏎ starts a fragment with that link, \
             titled from the page (a blyg post you hold is quoted instead). Anywhere on your \
             Mac, select text and choose Services › Capture in Burrow: the quick capture \
             panel opens with it quoted, and the page's address under it when the app gives \
             one."
        },
        keys: &[Key::Enter],
        key_label: "⏎",
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
        title: "Your contacts, first",
        caption: "Type @: the people in your blyg's address book come first, then the blygs \
                  you read. Where your blyg turns @handle into a link when it publishes, \
                  picking a contact writes @handle; elsewhere, the plain link. In Reading, \
                  Mentions under All unread lists the posts that mention you: dots, never a \
                  count.",
        keys: &[Key::Mention],
        key_label: "@",
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
        title: "Edited in two places? You choose",
        caption: "Burrow saves against the version of a post it last saw. When the post \
                  changed on your blyg as well (in the studio, or on another computer), \
                  nothing is overwritten: Burrow shows both. 1 keeps yours, 2 takes the \
                  server's, 3 keeps both, yours as a new draft.",
        keys: &[Key::ConflictAnswered],
        key_label: "1 2 3",
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
        title: "Reply to yourself",
        caption: "Your published post has the Reading view's actions above its text: Reply · \
                  new stub (⌥⌘R), Quote into a thread (⌥⌘Q) and Link post · new fragment \
                  (⌥⌘L). Forking your own post is in ⌘Y: Fork this pin, on a pinned version. \
                  Try Reply.",
        keys: &[Key::OwnPostAction],
        key_label: "⌥⌘R",
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
        title: "Links and quotes",
        caption: "Type [[ to link a post you hold; in a thread, ⌘K (or ![[ on a new line) \
                  quotes one, a snapshot taken when you publish. Rows lead with each post's \
                  title, and ⏎ writes [[id | label]]: the label is the link's text on the \
                  page, so edit the words after | to rename it. A quote's note is yours only.",
        keys: &[Key::QuotePicker, Key::Picker],
        key_label: "⌘K",
        region: Region::Editor,
        button: None,
        setup: Setup::Thread,
        pause_ms: 800,
        settle: true,
        stay: false,
    },
    Step {
        id: "echoed-quote",
        since: "0.12.0",
        title: "A quote the AI added",
        caption: "This gap's text ends with ![[…]] on its own line, which its instruction \
                  never asked for. Publishing makes that a real quote and notifies its \
                  author, so the preview already shows the quote, and ⌘⏎'s sheet warns \
                  before you publish. Delete the line, or name the post in the \
                  instruction.",
        keys: &[Key::Publish],
        key_label: "⌘⏎",
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
        title: "Mentions and spelling",
        caption: "Type @ and a name to link a blyg you know (↑ ↓ choose, ⏎ inserts). It's a \
                  plain link, so it notifies no one. A red underline marks a misspelling: \
                  right-click it for suggestions.",
        keys: &[Key::Mention, Key::SpellMenu],
        key_label: "@",
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
        title: "Your blygs",
        caption: if cfg!(target_os = "windows") {
            "The name in the title bar is the blyg you're writing to. Click it, or press \
             Ctrl+Shift+L, to switch blygs or add another; Ctrl+Shift+1 and Ctrl+Shift+2 jump \
             straight to them. Each blyg keeps its own posts and reading. (The tour stays on \
             its sample blyg.)"
        } else {
            "The name in the title bar is the blyg you're writing to. Click it, or press ⌃⌘0, \
             to switch blygs or add another; ⌃⌘1 and ⌃⌘2 jump straight to them. Each blyg \
             keeps its own posts and reading. (The tour stays on its sample blyg.)"
        },
        keys: &[Key::BlygSwitcher],
        key_label: if cfg!(target_os = "windows") {
            "Ctrl+Shift+L"
        } else {
            "⌃⌘0"
        },
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
        title: "Reading, in the Stream",
        caption: "Burrow opens here, on Reading (⌘R goes to your posts and back): posts from \
                  the blygs you follow, newest first by their own date. j/k move, and a post a \
                  second on screen counts as read. A revised post leads with what changed, \
                  “V2: …”, from its author's changelog. ⏎, Space, a double-click or Read more \
                  opens it in a side pane.",
        keys: &[Key::ReadMore],
        key_label: "⏎",
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
        title: "Post while you read",
        caption: "The box at the foot of Reading writes a post without leaving the one you're \
                  reading. c puts the caret in it (⇧⌘K from anywhere), ⌘⏎ posts a fragment, \
                  ⌘D keeps a draft, and esc goes back to the stream with your text kept. With \
                  a post open, ❝ Quote quotes it.",
        keys: &[Key::Compose],
        key_label: "c",
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
        title: "Reading time, and what's behind a post",
        caption: "Two bundled extensions, off until you turn them on: reading-time ends a \
                  post's byline with “· 3 min”, and inspect adds ⋯ to its actions, with the \
                  record Burrow holds for it (ids, versions, references). Click ⋯, then \
                  inspect.",
        keys: &[Key::SlotSheet],
        key_label: "click",
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
        title: "“Generated” is the publisher's word",
        caption: "Lin's blyg marks a sentence of this post as AI-generated. Open it (⏎): \
                  the marked text is tinted, hovering it says so, and the post ends with a \
                  note that the mark is the publisher's own disclosure, not verified.",
        keys: &[Key::ReadMore],
        key_label: "⏎",
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
        title: "Quotes lead to the original",
        caption: "The grey box quotes another post: click its text to open the original beside \
                  it, or the name for the author's profile. Its pane lists who quoted, stubbed \
                  or forked it.",
        keys: &[Key::OpenOriginal, Key::OpenProfile],
        key_label: "click",
        region: Region::StreamPost,
        button: None,
        setup: Setup::StreamQuote,
        pause_ms: 2600,
        settle: false,
        stay: true,
    },
    Step {
        id: "lineage",
        since: "0.8.0",
        title: "Lineage: where a post comes from",
        caption: "The glyph beside a post's name shows the kinds of post it draws on (left) \
                  and that draw on it (right): fork, reply, quote. “1 · 2” beside it counts \
                  them: up · down. ⌘J, or a click on it, opens the map, with the counts by the \
                  post: arrows move, ⏎ makes a neighbour the centre, ⌫ walks back.",
        keys: &[Key::Lineage],
        key_label: "⌘J",
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
        title: "Actions on a ring",
        caption: "With the post in the middle selected, Space opens its actions in fixed \
                  places: F fork, R reply, Q quote, L link post, V versions, O open; “R · 2” \
                  says two replies are known already. A letter previews what it would make; ⏎ \
                  does it, esc backs out. (Sample data: nothing is sent.)",
        keys: &[Key::LineageRing],
        key_label: "Space",
        region: Region::Whole,
        button: None,
        setup: Setup::Lineage,
        pause_ms: 2400,
        settle: false,
        stay: true,
    },
    Step {
        id: "reader",
        since: "0.4.0",
        title: "Reader: sources, posts, post",
        caption: "⌥⌘2 switches to Reader. On the left: smart feeds (All unread, Today, Thumbed, \
                  All, and Background, the only place withdrawn posts are listed), Your blyg \
                  (your own posts, never unread or counted), your hoppers, your folders, which \
                  stay on this Mac, and subscriptions. [ and ] step through a post's versions; ⌥⌘1 is the \
                  Stream again.",
        keys: &[Key::ReaderMode],
        key_label: "⌥⌘2",
        region: Region::Reader,
        button: None,
        setup: Setup::Stream,
        pause_ms: 3000,
        settle: false,
        stay: true,
    },
    // --- hoppers ---
    Step {
        id: "hoppers",
        since: "0.15.0",
        title: "Hoppers: posts you keep together",
        caption: "Under Hoppers on the left are your blyg's hoppers, collections of posts, yours \
                  and others'. Pick one to list its posts. h (or Hopper › on a post) puts the \
                  open post in a hopper or takes it out; a public one, marked with a globe, has its own page.",
        keys: &[Key::HopperPicker],
        key_label: "h",
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
        title: "Several posts into a hopper",
        caption: "Two posts are picked (⌘-click picks one more, ⇧-click a run, ⌘A the whole \
                  list). h now acts on all of them: choosing a hopper puts in the ones it \
                  doesn't hold yet, and a dash marks a hopper holding only some. A hopper lists \
                  its posts as your blyg does, the most recently added first.",
        keys: &[Key::HopperSeveral],
        key_label: "h",
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
        title: "Follow the thicket",
        caption: "Ada's reply quotes Lin's note. Click the quote to open it here; quotes inside \
                  quotes, a stub's “responds to” line and links to blyg posts open the same \
                  way. The breadcrumb above the post shows the path, and ⌘[ or ‹ back steps \
                  back.",
        keys: &[Key::FollowThicket],
        key_label: "click",
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
        title: "Where you've been",
        caption: "⇧⌘H, or history above the post, lists this walk and earlier ones beside the \
                  reader, as the tree they were; each step opens its post. It's private: kept \
                  in your blyg's own browsing log, or on this Mac, and never published.",
        keys: &[Key::BrowseHistory],
        key_label: "⇧⌘H",
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
        title: "Reply to a passage",
        caption: "Reply made this stub, quoting the whole of Lin's post. To answer one passage, \
                  select it inside the quote in the preview on the right, then press ❝ quote \
                  only this (or ⇧⌘D): it goes under that quote. Quote another passage adds one \
                  more to select from.",
        keys: &[Key::StubPassage],
        key_label: "⇧⌘D",
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
        title: "Moving from another reader?",
        caption: "Import OPML… (here on Subscriptions, ⇧⌘S, or Blyg › Import Subscriptions \
                  from OPML…) reads its export: Burrow lists the feeds, ticks the new ones, and \
                  files them in an Imported feeds folder in the Reader, out of your public \
                  blogroll. Export OPML… saves yours. Try the sample file.",
        keys: &[Key::OpmlImport],
        key_label: "click",
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
        title: "Notes while you read",
        caption: "⇧⌘N slides in the Notes drawer. Reading notes are running notes, kept on \
                  this Mac as a scratch note; → Notes, at the bottom of a post, adds the post \
                  to them. Select a passage in a post: ⇧⌘D, or the pill beside it, quotes it \
                  in your draft. The drawer's Notes tab is next.",
        keys: &[Key::Notes],
        key_label: "⇧⌘N",
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
        title: "Your Markdown notes, in the drawer",
        caption: "With markdown-notes on, the Notes tab opens folders of Markdown notes \
                  (Obsidian vaults, say): a chip per folder, Add folder… for more, and Remove \
                  from Burrow, which leaves the folder alone. Open a note to edit it or quote it \
                  into your post. Choose Garden.",
        keys: &[Key::NotesVault],
        key_label: "click",
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
        title: "Links open beside your reading",
        caption: "A link in a post opens in this pane (a blank sample here); ⌘-click opens it \
                  over the whole area. 🛡 turns ad and tracker blocking off for one site; ↗, or \
                  ⌥-click on the link, uses your default browser. esc closes it; ⇧⌘B brings it \
                  back.",
        keys: &[Key::CloseBrowser],
        key_label: "esc",
        region: Region::BrowserChrome,
        button: None,
        setup: Setup::Browser,
        pause_ms: 700,
        settle: false,
        stay: false,
    },
    Step {
        id: "clip",
        since: "0.10.0",
        title: "✂ Clip a page into your draft",
        caption: "⇧⌘C, or ✂ Clip in the pane's bar, quotes the page (or the passage you \
                  selected on it) in your draft, with its title and link. Clip the sample.",
        keys: &[Key::Clip],
        key_label: "⇧⌘C",
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
        title: "cross-post runs beside you",
        caption: "The bundled cross-post (off until you turn it on) puts a published post on \
                  Substack Notes from this pane: you check the text, it fills in the note, and \
                  only you click Post. esc folds the pane mid-run: the status bar says “macro \
                  running · show pane”. (A sample: nothing runs.)",
        keys: &[Key::MacroFolded],
        key_label: "esc",
        region: Region::MacroRun,
        button: None,
        setup: Setup::CrossPost,
        pause_ms: 1600,
        settle: false,
        stay: true,
    },
    Step {
        id: "extensions",
        since: "0.10.0",
        title: "Extensions: ⇧⌘P",
        caption: "⇧⌘P lists what your extensions offer here: their commands, Browse for each \
                  notes folder, and Manage extensions…. Burrow asks before one may do anything, \
                  and none can publish or see your sign-in.",
        keys: &[Key::ExtPalette],
        key_label: "⇧⌘P",
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
        title: "Turn off, or forget",
        caption: "Choose Manage extensions…: what's installed, on and allowed. Turn off stops \
                  one and keeps its permissions for next time; Forget permissions drops them \
                  too, so it asks again. (The tour changes nothing in your config file.)",
        keys: &[Key::ExtManage],
        key_label: "click",
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
        title: "Burrow asks first",
        caption: "The first time an extension starts, Burrow lists what it wants. ⏎ allows \
                  what's ticked (1–9 untick), esc is Not now, and ⌘⌫ turns it off. Try ⌘⌫. \
                  (A sample: nothing is saved.)",
        keys: &[Key::ConsentAnswered],
        key_label: "⌘⌫",
        region: Region::Whole,
        button: None,
        setup: Setup::Consent,
        pause_ms: 1200,
        settle: false,
        stay: false,
    },
    Step {
        id: "done",
        since: "0.1.0",
        title: "That's the tour",
        caption: "Everything you did here was on sample data. Your own posts and reading come \
                  back when you finish. Settings (⌘,) › Help replays this tour.",
        keys: &[],
        key_label: "",
        region: Region::Whole,
        button: None,
        setup: Setup::Posts,
        pause_ms: 0,
        settle: false,
        stay: false,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sound() {
        let mut ids: Vec<_> = STEPS.iter().map(|s| s.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), STEPS.len(), "duplicate step id");
        for (i, s) in STEPS.iter().enumerate() {
            assert!(!s.title.is_empty() && !s.caption.is_empty(), "{}", s.id);
            let last = i + 1 == STEPS.len();
            assert_eq!(
                s.keys.is_empty(),
                last,
                "{}: only the last step waits for Next",
                s.id
            );
            assert_eq!(s.key_label.is_empty(), s.keys.is_empty(), "{}", s.id);
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
            Key::Chat,     // --- chat ---
            Key::Terminal, // --- terminal ---
        ] {
            assert!(STEPS.iter().any(|s| s.accepts(k)), "{k:?} isn't taught");
        }
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
