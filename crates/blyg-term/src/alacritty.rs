//! The engine in use: Alacritty's terminal core (`alacritty_terminal`,
//! Apache-2.0), driven by its own VT parser (`vte`). Only the grid and
//! parser are used; Alacritty's pty and event loop are not (the pty is
//! portable-pty's, in `pty.rs`).

use std::sync::{Arc, Mutex};
use std::time::Instant;

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Osc52, TermMode};
use alacritty_terminal::vte::ansi::{
    Color as AColor, CursorShape as AShape, NamedColor, Processor, Rgb as ARgb, StdSyncHandler,
};

use crate::{
    Attrs, Cell, Color, Cursor, CursorShape, Engine, GridPoint, Modes, Rgb, Row, SCROLLBACK,
    Snapshot, xterm_256,
};

/// The grid's size, as Alacritty wants it.
struct TermSize {
    cols: usize,
    rows: usize,
}

impl TermSize {
    fn new(cols: usize, rows: usize) -> Self {
        TermSize { cols, rows }
    }
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// What the terminal's events need: the replies it owes the pty, and the
/// answers to colour and size queries.
struct Shared {
    replies: Vec<u8>,
    fg: Rgb,
    bg: Rgb,
    window: WindowSize,
}

#[derive(Clone)]
struct Listener(Arc<Mutex<Shared>>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let Ok(mut s) = self.0.lock() else { return };
        match event {
            Event::PtyWrite(text) => s.replies.extend_from_slice(text.as_bytes()),
            Event::ColorRequest(index, format) => {
                let c = match index {
                    256 | 258 => s.fg,
                    257 => s.bg,
                    i => xterm_256(i.min(255) as u8),
                };
                let reply = format(ARgb {
                    r: c.r,
                    g: c.g,
                    b: c.b,
                });
                s.replies.extend_from_slice(reply.as_bytes());
            }
            Event::TextAreaSizeRequest(format) => {
                let reply = format(s.window);
                s.replies.extend_from_slice(reply.as_bytes());
            }
            // The clipboard is the user's: a program in the pane can neither
            // read it (OSC 52 paste) nor write it (OSC 52 copy) through the
            // terminal; ⌘C / ⌘V are the only ways across.
            Event::ClipboardLoad(..) | Event::ClipboardStore(..) => {}
            _ => {}
        }
    }
}

pub struct AlacrittyEngine {
    term: Term<Listener>,
    parser: Processor<StdSyncHandler>,
    shared: Arc<Mutex<Shared>>,
}

impl AlacrittyEngine {
    pub fn new(cols: u16, rows: u16) -> Self {
        let (cols, rows) = (cols.max(2), rows.max(1));
        let shared = Arc::new(Mutex::new(Shared {
            replies: Vec::new(),
            fg: Rgb::new(0xdd, 0xdd, 0xdd),
            bg: Rgb::new(0x1e, 0x1e, 0x1e),
            window: WindowSize {
                num_lines: rows,
                num_cols: cols,
                cell_width: 8,
                cell_height: 16,
            },
        }));
        let config = Config {
            scrolling_history: SCROLLBACK,
            osc52: Osc52::Disabled,
            ..Config::default()
        };
        let size = TermSize::new(cols as usize, rows as usize);
        let term = Term::new(config, &size, Listener(shared.clone()));
        AlacrittyEngine {
            term,
            parser: Processor::new(),
            shared,
        }
    }

    /// End a synchronized update (mode 2026) whose time is up.
    fn flush_sync(&mut self) {
        if let Some(t) = self.parser.sync_timeout().sync_timeout()
            && Instant::now() >= t
        {
            self.parser.stop_sync(&mut self.term);
        }
    }
}

fn color(c: AColor, palette: &alacritty_terminal::term::color::Colors) -> Color {
    match c {
        AColor::Spec(rgb) => Color::Rgb(Rgb::new(rgb.r, rgb.g, rgb.b)),
        AColor::Indexed(i) => match palette[i as usize] {
            Some(rgb) => Color::Rgb(Rgb::new(rgb.r, rgb.g, rgb.b)),
            None => Color::Indexed(i),
        },
        AColor::Named(n) => {
            let i = n as usize;
            let base = match n {
                NamedColor::DimBlack => Some(0),
                NamedColor::DimRed => Some(1),
                NamedColor::DimGreen => Some(2),
                NamedColor::DimYellow => Some(3),
                NamedColor::DimBlue => Some(4),
                NamedColor::DimMagenta => Some(5),
                NamedColor::DimCyan => Some(6),
                NamedColor::DimWhite => Some(7),
                _ if i < 16 => Some(i as u8),
                _ => None,
            };
            match base {
                Some(b) => match palette[b as usize] {
                    Some(rgb) => Color::Rgb(Rgb::new(rgb.r, rgb.g, rgb.b)),
                    None => Color::Indexed(b),
                },
                None => Color::Default,
            }
        }
    }
}

fn modes(m: &TermMode) -> Modes {
    Modes {
        app_cursor: m.contains(TermMode::APP_CURSOR),
        bracketed_paste: m.contains(TermMode::BRACKETED_PASTE),
        alt_screen: m.contains(TermMode::ALT_SCREEN),
    }
}

impl Engine for AlacrittyEngine {
    fn name(&self) -> &'static str {
        "alacritty_terminal"
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    fn resize(&mut self, cols: u16, rows: u16, cell_w_px: u32, cell_h_px: u32) {
        let (cols, rows) = (cols.max(2), rows.max(1));
        if let Ok(mut s) = self.shared.lock() {
            s.window = WindowSize {
                num_lines: rows,
                num_cols: cols,
                cell_width: cell_w_px.min(u16::MAX as u32) as u16,
                cell_height: cell_h_px.min(u16::MAX as u32) as u16,
            };
        }
        if (cols, rows) != self.size() {
            self.term
                .resize(TermSize::new(cols as usize, rows as usize));
        }
    }

    fn size(&self) -> (u16, u16) {
        (self.term.columns() as u16, self.term.screen_lines() as u16)
    }

    fn snapshot(&mut self) -> Snapshot {
        self.flush_sync();
        let (cols, rows) = self.size();
        let content = self.term.renderable_content();
        let offset = content.display_offset;
        let mut out: Vec<Row> = (0..rows)
            .map(|_| Row {
                cells: vec![Cell::default(); cols as usize],
            })
            .collect();
        for indexed in content.display_iter {
            let row = indexed.point.line.0 + offset as i32;
            let col = indexed.point.column.0;
            if row < 0 || row as usize >= out.len() || col >= cols as usize {
                continue;
            }
            let c = &indexed.cell;
            let f = c.flags;
            let extra = c
                .zerowidth()
                .filter(|z| !z.is_empty())
                .map(|z| z.iter().collect::<String>().into_boxed_str());
            out[row as usize].cells[col] = Cell {
                ch: if c.c == '\t' { ' ' } else { c.c },
                extra,
                fg: color(c.fg, content.colors),
                bg: color(c.bg, content.colors),
                attrs: Attrs {
                    bold: f.contains(Flags::BOLD),
                    dim: f.contains(Flags::DIM),
                    italic: f.contains(Flags::ITALIC),
                    underline: f.intersects(Flags::ALL_UNDERLINES),
                    strike: f.contains(Flags::STRIKEOUT),
                    inverse: f.contains(Flags::INVERSE),
                    hidden: f.contains(Flags::HIDDEN),
                },
                wide: f.contains(Flags::WIDE_CHAR),
                spacer: f.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER),
            };
        }
        let cur = content.cursor;
        let crow = cur.point.line.0 + offset as i32;
        let cursor = (content.mode.contains(TermMode::SHOW_CURSOR)
            && cur.shape != AShape::Hidden
            && crow >= 0
            && (crow as usize) < out.len())
        .then(|| Cursor {
            row: crow as u16,
            col: (cur.point.column.0 as u16).min(cols.saturating_sub(1)),
            shape: match cur.shape {
                AShape::Beam => CursorShape::Beam,
                AShape::Underline => CursorShape::Underline,
                _ => CursorShape::Block,
            },
        });
        Snapshot {
            cols,
            rows: out,
            cursor,
            display_offset: offset,
            history: self.term.grid().history_size(),
            modes: modes(&content.mode),
        }
    }

    fn take_replies(&mut self) -> Vec<u8> {
        self.shared
            .lock()
            .map(|mut s| std::mem::take(&mut s.replies))
            .unwrap_or_default()
    }

    fn scroll(&mut self, delta: i32) {
        self.term.scroll_display(Scroll::Delta(delta));
    }

    fn scroll_to_bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }

    fn modes(&self) -> Modes {
        modes(self.term.mode())
    }

    fn text_between(&self, a: GridPoint, b: GridPoint) -> String {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        let top = -(self.term.grid().history_size() as i32);
        let bottom = self.term.screen_lines() as i32 - 1;
        let last = self.term.columns().saturating_sub(1);
        let point = |p: GridPoint| {
            Point::new(
                Line(p.line.clamp(top, bottom)),
                Column((p.col as usize).min(last)),
            )
        };
        let text = self.term.bounds_to_string(point(a), point(b));
        text.lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn set_default_colors(&mut self, fg: Rgb, bg: Rgb) {
        if let Ok(mut s) = self.shared.lock() {
            s.fg = fg;
            s.bg = bg;
        }
    }

    fn pending_until(&self) -> Option<Instant> {
        self.parser.sync_timeout().sync_timeout()
    }
}
