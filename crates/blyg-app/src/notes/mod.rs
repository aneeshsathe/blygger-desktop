//! --- notes --- The notes drawer (docs/SPEC.md § Notes drawer).
//!
//! A pane docked at the right edge of the window (⇧⌘N, View › Notes):
//! running notes, links to posts and pages, quoted selections, and the
//! markdown-notes library, to read while you write. It's a layout column
//! beside the editor (and the reading screens), not an overlay: clicking or
//! typing elsewhere never closes it. Closing is explicit (its ×, or ⇧⌘N
//! while it has the keyboard); esc only hands the keyboard back. Its width
//! drags, and the width and whether it's open are kept in `state.json`.
//!
//! It's backed by an ordinary scratch note (local only, in the Posts list
//! with its `scratch` pill; ⌘D makes it a draft). One rolling note titled
//! "Reading notes" by default, made a thread so `![[id]]` quotes are valid
//! in it; "New notes page" starts another and leaves the old one in Posts.
//!
//! - this file: the text the drawer appends (pure, unit-tested) and its state;
//! - `drawer`: the `MainView` hooks, the drawer itself and its demos.

mod drawer;
mod quote; // universal quoting (⇧⌘D)

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use gpui_kit::base::input::TextareaState;
use gpui_kit::*;

use blyg_core::LocalId;

pub(crate) use drawer::NotesPark;
pub use drawer::{SELECTION_JS, ToggleNotes}; // --- onboarding ---
pub(crate) use quote::QuoteFrom;
pub use quote::QuoteToDraft;

use crate::vm;

/// The pane's width until the user drags its edge.
pub const WIDTH: f32 = 380.;

/// The narrowest the pane gets.
pub const MIN_WIDTH: f32 = 280.;

/// The widest the pane gets (a note reads badly wider than this anyway).
pub const MAX_WIDTH: f32 = 760.;

/// The pane never takes more than this share of the window.
pub const MAX_SHARE: f32 = 0.6;

/// How long the slide in (and out) takes.
pub const SLIDE_MS: u64 = 180;

/// The rolling note's title (its first line, a heading).
pub const TITLE: &str = "Reading notes";

/// The key context inside the drawer.
pub const CONTEXT: &str = "Notes";

/// A quoted selection is cut off here (a page can't flood the note).
pub const MAX_QUOTE: usize = 4000;

// ------------------------------------------------------------ formatting

/// `[title](url)`: brackets in the title escaped, newlines folded, a
/// destination with parentheses wrapped in `<…>` (`vm::escape_link_text`,
/// `vm::link_destination`). An empty title shows the URL.
pub fn link(title: &str, url: &str) -> String {
    let t: String = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('\\', "\\\\");
    let t = if t.is_empty() { url.to_string() } else { t };
    format!(
        "[{}]({})",
        vm::escape_link_text(&t),
        vm::link_destination(url.trim())
    )
}

/// Every line of `text` as a `>` quote (blank lines stay in the quote).
pub fn blockquote(text: &str) -> String {
    text.trim()
        .lines()
        .map(|l| {
            let l = l.trim_end();
            if l.is_empty() {
                ">".to_string()
            } else {
                format!("> {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A selection, quoted, with where it came from:
///
/// ```text
/// > the selected text
/// >
/// > — [Source title](https://…)
/// ```
pub fn quote_with_source(selection: &str, title: &str, url: Option<&str>) -> String {
    let sel: String = selection.chars().take(MAX_QUOTE).collect();
    let q = blockquote(&sel);
    match url {
        Some(u) => format!("{q}\n>\n> — {}", link(title, u)),
        None if !title.trim().is_empty() => format!("{q}\n>\n> — {}", title.trim()),
        None => q,
    }
}

/// A selection in the notes (byte `range` of `text`) as a passage to quote
/// elsewhere, and the page it came from when the note says so: `>` markers
/// dropped, and, when the selection sits in a quote ending in a
/// `> — [title](url)` source line (what [`quote_with_source`] writes), that
/// title and URL. `None` for an empty selection. A note has no source URL of
/// its own, so anything else is quoted without a link.
pub fn notes_passage(
    text: &str,
    range: std::ops::Range<usize>,
) -> Option<(String, Option<(String, String)>)> {
    let (start, end) = (range.start.min(text.len()), range.end.min(text.len()));
    let selected = text.get(start..end)?;
    let passage: Vec<&str> = selected
        .lines()
        .map(|l| {
            let t = l.trim_start();
            match t.strip_prefix('>') {
                Some(rest) => rest.strip_prefix(' ').unwrap_or(rest),
                None => l,
            }
        })
        .filter(|l| source_line(l).is_none())
        .collect();
    let passage = passage.join("\n");
    if passage.trim().is_empty() {
        return None;
    }
    // The quote block around the selection's first line.
    let lines: Vec<&str> = text.split('\n').collect();
    let mut at = 0;
    let mut first = 0;
    for (i, l) in lines.iter().enumerate() {
        if at + l.len() >= start {
            first = i;
            break;
        }
        at += l.len() + 1;
    }
    let quoted = |i: usize| lines[i].trim_start().starts_with('>');
    let link = quoted(first)
        .then(|| {
            (first..lines.len())
                .take_while(|&i| quoted(i))
                .find_map(|i| source_line(lines[i].trim_start().trim_start_matches('>').trim()))
        })
        .flatten();
    Some((passage, link))
}

/// `— [title](url)` (a source line) as its title and URL.
fn source_line(line: &str) -> Option<(String, String)> {
    let rest = line.trim().strip_prefix("— [")?;
    let close = rest.rfind("](")?;
    let url = rest[close + 2..].strip_suffix(')')?;
    let url = url
        .strip_prefix('<')
        .and_then(|u| u.strip_suffix('>'))
        .unwrap_or(url);
    let mut title = String::new();
    let mut chars = rest[..close].chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            title.extend(chars.next());
        } else {
            title.push(c);
        }
    }
    Some((title, url.to_string()))
}

/// What "→ Notes" adds for a post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostRef {
    /// The post's id, when it's a blyg post (transcludable).
    pub blyg_id: Option<String>,
    /// Its title (or first line).
    pub title: String,
    /// Its first line of text (quoted under the link when there's no `![[…]]`).
    pub first_line: String,
    pub url: Option<String>,
    /// --- quote labels --- the label its `![[id | label]]` carries.
    pub label: Option<String>,
}

/// The block for a post: `![[id]]` for a blyg post when the note may hold
/// quotes (a thread); otherwise a link, plus, for a blyg post, its first
/// line as a `>` quote. RSS posts get the link.
pub fn post_block(post: &PostRef, note_takes_quotes: bool) -> String {
    if let (Some(id), true) = (&post.blyg_id, note_takes_quotes) {
        return blyg_core::grammar::directive_source(id, post.label.as_deref());
    }
    let head = match &post.url {
        Some(u) => link(&post.title, u),
        None => post.title.trim().to_string(),
    };
    let first = post.first_line.trim();
    if post.blyg_id.is_some() && !first.is_empty() && first != post.title.trim() {
        format!("{head}\n{}", blockquote(first))
    } else {
        head
    }
}

/// Append `block` as its own paragraph at the end of `text`. Returns the new
/// text and the caret: on a new line below the block, ready to type.
pub fn append(text: &str, block: &str) -> (String, usize) {
    let mut out = text.trim_end_matches([' ', '\t', '\n']).to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(block.trim_end());
    out.push_str("\n\n");
    let caret = out.len();
    (out, caret)
}

/// A new page's text: the title as a heading and an empty paragraph.
pub fn page_seed(title: &str) -> String {
    format!("# {title}\n\n")
}

/// The title for a fresh notes page: "Reading notes · Sep 27".
pub fn fresh_title(today: chrono::NaiveDate) -> String {
    format!("{TITLE} · {}", today.format("%b %-d"))
}

/// The web view's answer to [`SELECTION_JS`] (a JSON string) as text.
pub fn parse_selection(json: &str) -> String {
    serde_json::from_str::<String>(json).unwrap_or_default()
}

/// Is this the drawer's kind of note (by its title)?
pub fn is_notes_title(title: &str) -> bool {
    title.trim().starts_with(TITLE)
}

// ------------------------------------------------------------ layout

/// The pane's widths (`side::Span`): its default, and the range a drag of
/// its left edge keeps it in.
pub const SPAN: crate::app::side::Span = crate::app::side::Span {
    default: WIDTH,
    min: MIN_WIDTH,
    max: MAX_WIDTH,
};

/// The side padding that keeps a note's lines a readable length in a pane
/// `pane_w` wide at `font_size` (about 66 characters at most).
pub fn side_pad(pane_w: f32, font_size: f32) -> f32 {
    let measure = font_size * 33.;
    ((pane_w - measure) / 2.).max(18.)
}

// ------------------------------------------------------------ state

/// The drawer's state (a field of `MainView`).
pub struct Notes {
    /// On screen (or sliding in).
    pub open: bool,
    /// Sliding out: still drawn (and the web views still cut off).
    pub closing: bool,
    /// Times shown (a new slide-in animation each time).
    pub(crate) shown: u64,
    close_gen: u64,
    /// The scratch note it writes to, once there is one.
    pub id: Option<LocalId>,
    /// `id` has been looked up (or deliberately cleared) this session.
    resolved: bool,
    editor: Option<Entity<TextareaState>>,
    assist: Option<Entity<crate::composer::Assist>>,
    focus: Option<FocusHandle>,
    /// Where the keyboard was before the drawer took it.
    return_focus: Option<FocusHandle>,
    /// The header's ⋯ menu.
    menu: bool,
    /// The width the user chose (dragged), before [`dock_width`] fits it to
    /// the window.
    pub width: f32,
    /// The width it's drawn at (set each frame, from the window).
    pub dock_w: f32,
    /// It was open when the app last closed: open it at launch.
    pub reopen: bool,
    /// The latest drag of its edge (the width is saved once dragging stops).
    width_gen: u64,
    /// When it last started to slide in or out (the browser pane's room
    /// follows it).
    moved: Option<std::time::Instant>,
    /// Setting the editor's text programmatically (not an edit to save).
    loading: bool,
    data_dir: Option<std::path::PathBuf>,
}

impl Notes {
    pub fn new(data_dir: Option<std::path::PathBuf>) -> Notes {
        let state = data_dir
            .as_deref()
            .map(blyg_core::state::AppState::load)
            .unwrap_or_default();
        let width = SPAN.restore(state.notes_width);
        Notes {
            open: false,
            closing: false,
            shown: 0,
            close_gen: 0,
            id: None,
            resolved: false,
            editor: None,
            assist: None,
            focus: None,
            return_focus: None,
            menu: false,
            width,
            dock_w: width,
            reopen: state.notes_open,
            width_gen: 0,
            moved: None,
            loading: false,
            data_dir,
        }
    }

    /// Drawn this frame (open, or sliding out).
    pub fn drawn(&self) -> bool {
        self.open || self.closing
    }

    /// How much of the window's width the pane takes (its final place;
    /// [`Self::room_at`] follows the slide): the rest of the layout, and the
    /// browser pane, fit to its left.
    pub fn room(&self) -> Pixels {
        if self.open { px(self.dock_w) } else { px(0.) }
    }

    /// Fit the chosen width to a window `window_w` wide (each frame).
    pub fn fit(&mut self, window_w: f32) {
        self.dock_w = SPAN.fit(self.width, window_w, MAX_SHARE);
    }

    /// Persist whether it's open and its width (`state.json`).
    fn save_layout(&self) {
        if let Some(d) = &self.data_dir {
            let (open, w) = (self.open, SPAN.saved(self.width));
            let _ = blyg_core::state::AppState::update(d, |s| {
                s.notes_open = open;
                s.notes_width = w;
            });
        }
    }

    /// [`Self::room`] at `now`, part way through a slide.
    pub fn room_at(&self, now: std::time::Instant) -> Pixels {
        let target = f32::from(self.room());
        let Some(at) = self.moved else {
            return px(target);
        };
        let t = now.saturating_duration_since(at).as_secs_f32() * 1000. / SLIDE_MS as f32;
        if t >= 1. {
            return px(target);
        }
        let eased = 1.0 - (1.0 - t).powi(5); // ease_out_quint, as the drawer
        let w = self.dock_w;
        px(if self.open {
            eased * w
        } else {
            (1. - eased) * w
        })
    }

    /// --- extensions --- The drawer has the keyboard.
    pub fn has_keyboard(&self, window: &Window, cx: &App) -> bool {
        self.focus
            .as_ref()
            .is_some_and(|f| f.contains_focused(window, cx))
    }

    /// The editor's text ("" before the drawer was first opened).
    pub fn text(&self, cx: &App) -> String {
        self.editor
            .as_ref()
            .map(|e| e.read(cx).value().to_string())
            .unwrap_or_default()
    }
}
