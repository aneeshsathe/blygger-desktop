//! Burrow's terminal pane, without the drawing: a terminal engine behind a
//! small trait ([`Engine`]), the user's shell on a pty ([`pty`]), the
//! scrubbed environment it gets ([`env`]), and key encoding ([`keys`]).
//!
//! **Which engine.** [`new_engine`] returns Alacritty's terminal core
//! (`alacritty_terminal`, Apache-2.0, pure Rust). libghostty-vt 1.0 (MIT) is
//! the engine the design asked for, but no crates.io package ships the 1.0
//! library without building Ghostty with Zig or downloading a prebuilt
//! archive at build time, and Burrow's CI builds with plain cargo. Its
//! bindings (`ghostty.rs`, written against the 1.0 headers) sit behind the
//! `libghostty` cargo feature, off by default; with it on, [`new_engine`]
//! returns that engine instead. [`ENGINE`] names the one in use.
//!
//! **Safety.** The shell gets a scrubbed environment ([`env::scrub`]): no
//! `BLYGGER_*`, nothing Burrow set, no token. Nothing here talks to the app
//! but the pty's bytes: there is no IPC bridge.

pub mod env;
pub mod keys;
pub mod pty;

mod alacritty;
#[cfg(feature = "libghostty")]
mod ghostty;

pub use keys::{Key, encode_paste};

/// The engine [`new_engine`] builds.
#[cfg(not(feature = "libghostty"))]
pub const ENGINE: &str = "alacritty_terminal";
#[cfg(feature = "libghostty")]
pub const ENGINE: &str = "libghostty-vt";

/// Lines of scrollback kept (in memory only; nothing is written to disk).
pub const SCROLLBACK: usize = 5000;

/// A new terminal of `cols` × `rows` on the engine in use ([`ENGINE`]).
pub fn new_engine(cols: u16, rows: u16) -> Box<dyn Engine> {
    #[cfg(feature = "libghostty")]
    {
        if let Some(g) = ghostty::GhosttyEngine::new(cols, rows) {
            return Box::new(g);
        }
    }
    Box::new(alacritty::AlacrittyEngine::new(cols, rows))
}

/// The Alacritty engine, whatever the features (the tests compare against it).
pub fn alacritty_engine(cols: u16, rows: u16) -> Box<dyn Engine> {
    Box::new(alacritty::AlacrittyEngine::new(cols, rows))
}

/// An sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }
}

/// A cell's colour, before the app's theme resolves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Color {
    /// The theme's foreground (for `fg`) or background (for `bg`).
    #[default]
    Default,
    /// One of the 256 palette entries: 0–15 the theme's ANSI colours, 16–255
    /// the xterm cube and grey ramp ([`xterm_256`]).
    Indexed(u8),
    /// A 24-bit colour the program asked for.
    Rgb(Rgb),
}

/// A cell's attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Attrs {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub inverse: bool,
    pub hidden: bool,
}

/// One cell of the grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// The character (a space for an empty cell).
    pub ch: char,
    /// Combining characters after `ch`, if any.
    pub extra: Option<Box<str>>,
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attrs,
    /// The first half of a double-width character: it covers this cell and
    /// the next.
    pub wide: bool,
    /// The second half of a double-width character (draw nothing).
    pub spacer: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            extra: None,
            fg: Color::Default,
            bg: Color::Default,
            attrs: Attrs::default(),
            wide: false,
            spacer: false,
        }
    }
}

/// One row on screen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Row {
    pub cells: Vec<Cell>,
}

impl Row {
    /// The row's text, trailing spaces trimmed (for tests and copying).
    pub fn text(&self) -> String {
        let mut s = String::new();
        for c in &self.cells {
            if c.spacer {
                continue;
            }
            s.push(c.ch);
            if let Some(e) = &c.extra {
                s.push_str(e);
            }
        }
        s.trim_end().to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Block,
    Beam,
    Underline,
}

/// Where the cursor is drawn, in viewport cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub row: u16,
    pub col: u16,
    pub shape: CursorShape,
}

/// The terminal's modes that change what keys and pastes send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modes {
    /// DECCKM: arrows send `ESC O A` rather than `ESC [ A`.
    pub app_cursor: bool,
    /// Mode 2004: pastes are wrapped in `ESC [200~` … `ESC [201~`.
    pub bracketed_paste: bool,
    /// The alternate screen (a full-screen program): the wheel sends arrows.
    pub alt_screen: bool,
}

/// What to draw: the viewport, top row first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    pub cols: u16,
    pub rows: Vec<Row>,
    /// `None` when the cursor is hidden or scrolled out of view.
    pub cursor: Option<Cursor>,
    /// Rows scrolled back into history (0: the live screen).
    pub display_offset: usize,
    /// Rows of history above the screen.
    pub history: usize,
    pub modes: Modes,
}

impl Snapshot {
    /// Every row's text (tests).
    pub fn lines(&self) -> Vec<String> {
        self.rows.iter().map(Row::text).collect()
    }
}

/// A position in the whole buffer: `line` 0 is the top of the live screen,
/// negative lines are history. A viewport row `r` is line
/// `r - display_offset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GridPoint {
    pub line: i32,
    pub col: u16,
}

/// A terminal engine: bytes in from the pty, cells out to draw.
///
/// The engine owns no process and draws nothing: the app feeds it what the
/// shell wrote, draws its [`Snapshot`], and writes [`Engine::take_replies`]
/// (answers to the program's queries) and encoded keys back to the pty.
pub trait Engine: Send {
    /// What this engine is ([`ENGINE`]).
    fn name(&self) -> &'static str;
    /// Bytes the shell wrote.
    fn feed(&mut self, bytes: &[u8]);
    /// The pane's size in cells (and the cell's size in pixels, for programs
    /// that ask).
    fn resize(&mut self, cols: u16, rows: u16, cell_w_px: u32, cell_h_px: u32);
    /// The size in cells.
    fn size(&self) -> (u16, u16);
    /// What to draw now.
    fn snapshot(&mut self) -> Snapshot;
    /// Bytes the terminal owes the pty (answers to device status, colour
    /// and size queries), oldest first.
    fn take_replies(&mut self) -> Vec<u8>;
    /// Scroll the view `delta` rows into history (negative: back down).
    fn scroll(&mut self, delta: i32);
    /// Back to the live screen.
    fn scroll_to_bottom(&mut self);
    fn modes(&self) -> Modes;
    /// The text between two points, inclusive, in reading order. Wrapped
    /// lines join without a newline; trailing spaces go.
    fn text_between(&self, a: GridPoint, b: GridPoint) -> String;
    /// The theme's default colours, for programs that ask (OSC 10/11): a
    /// CLI picks light or dark output from the background.
    fn set_default_colors(&mut self, fg: Rgb, bg: Rgb);
    /// When the program is mid synchronized update (mode 2026), the time the
    /// engine gives up waiting for its end: take a [`Engine::snapshot`] then.
    fn pending_until(&self) -> Option<std::time::Instant> {
        None
    }
    /// The bytes a key sends, or `None` for a key the terminal ignores
    /// (anything with ⌘, which the app keeps).
    fn encode_key(&self, key: &Key) -> Option<Vec<u8>> {
        keys::encode(key, self.modes())
    }
}

/// The xterm 256-colour palette entry `i` for 16–255 (the 6×6×6 cube and
/// the grey ramp). 0–15 are the theme's; this returns xterm's for them.
pub fn xterm_256(i: u8) -> Rgb {
    const BASE: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 49, 49),
        (13, 188, 121),
        (229, 229, 16),
        (36, 114, 200),
        (188, 63, 188),
        (17, 168, 205),
        (229, 229, 229),
        (102, 102, 102),
        (241, 76, 76),
        (35, 209, 139),
        (245, 245, 67),
        (59, 142, 234),
        (214, 112, 214),
        (41, 184, 219),
        (255, 255, 255),
    ];
    match i {
        0..=15 => {
            let (r, g, b) = BASE[i as usize];
            Rgb::new(r, g, b)
        }
        16..=231 => {
            let n = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Rgb::new(level(n / 36), level((n / 6) % 6), level(n % 6))
        }
        _ => {
            let v = 8 + (i - 232) * 10;
            Rgb::new(v, v, v)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_256_palette() {
        assert_eq!(xterm_256(16), Rgb::new(0, 0, 0));
        assert_eq!(xterm_256(196), Rgb::new(255, 0, 0));
        assert_eq!(xterm_256(231), Rgb::new(255, 255, 255));
        assert_eq!(xterm_256(232), Rgb::new(8, 8, 8));
        assert_eq!(xterm_256(255), Rgb::new(238, 238, 238));
    }
}
