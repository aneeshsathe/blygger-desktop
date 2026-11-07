//! libghostty-vt 1.0 as the engine (the `libghostty` feature, off by
//! default). Hand-written bindings for the part of the C ABI the pane needs:
//! the terminal (`ghostty/vt/terminal.h`) and the render state with its row
//! and cell iterators (`ghostty/vt/render.h`), from the vt-1.0.0 headers.
//!
//! **Status: unverified.** No crates.io package ships libghostty-vt 1.0
//! without Zig or a download at build time, so this module is type-checked
//! (`cargo check --features libghostty`) but has not been linked or run.
//! Types from headers not in hand when it was written (`types.h`, `color.h`,
//! `modes.h`: the result codes, `GhosttyColorRgb`, `GhosttyBuffer`, the
//! packed `GhosttyMode`) were cross-checked against a pre-1.0 binding
//! (`ghostty-vt-rs-sys` 0.2.0); verify them against the 1.0 headers before
//! relying on this. Not yet read from Ghostty: cell styles (bold, underline:
//! `style.h`), wide-cell flags, the cursor's shape, and selection text
//! beyond the visible screen. Keys use the crate's own xterm encoder, not
//! Ghostty's (`key/encoder.h`).

#![allow(non_camel_case_types, non_upper_case_globals)]

use std::ffi::c_void;
use std::ptr;

use crate::{Cell, Color, Cursor, CursorShape, Engine, GridPoint, Modes, Rgb, Row, Snapshot};

// --- the C ABI (vt-1.0.0) ---------------------------------------------------

type GhosttyResult = i32;
const GHOSTTY_SUCCESS: GhosttyResult = 0;

type GhosttyTerminal = *mut c_void;
type GhosttyRenderState = *mut c_void;
type GhosttyRenderStateRowIterator = *mut c_void;
type GhosttyRenderStateRowCells = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GhosttyColorRgb {
    r: u8,
    g: u8,
    b: u8,
}

#[repr(C)]
struct GhosttyBuffer {
    ptr: *mut u8,
    cap: usize,
    len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
union GhosttyTerminalScrollViewportValue {
    delta: isize,
    row: usize,
    delta_prompt: isize,
    _padding: [u64; 2],
}

#[repr(C)]
struct GhosttyTerminalScrollViewport {
    tag: i32,
    value: GhosttyTerminalScrollViewportValue,
}

#[repr(C)]
#[derive(Default)]
struct GhosttyTerminalScrollbar {
    total: u64,
    offset: u64,
    len: u64,
}

#[repr(C)]
struct GhosttyTerminalModeConfig {
    /// Packed: the mode number in bits 0–14, bit 15 set for ANSI modes.
    mode: u16,
    value: bool,
}

type GhosttyTerminalWritePtyFn =
    unsafe extern "C" fn(GhosttyTerminal, *mut c_void, *const u8, usize);

// GhosttyTerminalOption
const OPT_USERDATA: i32 = 1;
const OPT_WRITE_PTY: i32 = 2;
const OPT_COLOR_FOREGROUND: i32 = 12;
const OPT_COLOR_BACKGROUND: i32 = 13;
// GhosttyTerminalData
const DATA_SCROLLBAR: i32 = 9;
const DATA_MODE: i32 = 37;
// GhosttyTerminalScrollViewportTag
const SCROLL_BOTTOM: i32 = 1;
const SCROLL_DELTA: i32 = 2;
// GhosttyRenderStateData
const RS_COLS: i32 = 1;
const RS_ROWS: i32 = 2;
const RS_ROW_ITERATOR: i32 = 4;
const RS_CURSOR_VISIBLE: i32 = 11;
const RS_CURSOR_VIEWPORT_HAS_VALUE: i32 = 14;
const RS_CURSOR_VIEWPORT_X: i32 = 15;
const RS_CURSOR_VIEWPORT_Y: i32 = 16;
// GhosttyRenderStateRowData
const ROW_CELLS: i32 = 3;
// GhosttyRenderStateRowCellsData
const CELL_BG_COLOR: i32 = 5;
const CELL_FG_COLOR: i32 = 6;
const CELL_GRAPHEMES_UTF8: i32 = 9;
// DEC private modes (GhosttyMode, ANSI bit clear).
const MODE_DECCKM: u16 = 1;
const MODE_ALT_SCREEN: u16 = 1049;
const MODE_BRACKETED_PASTE: u16 = 2004;

unsafe extern "C" {
    fn ghostty_terminal_new(
        allocator: *const c_void,
        terminal: *mut GhosttyTerminal,
        cols: u16,
        rows: u16,
    ) -> GhosttyResult;
    fn ghostty_terminal_free(terminal: GhosttyTerminal);
    fn ghostty_terminal_resize(
        terminal: GhosttyTerminal,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> GhosttyResult;
    fn ghostty_terminal_set(
        terminal: GhosttyTerminal,
        option: i32,
        value: *const c_void,
    ) -> GhosttyResult;
    fn ghostty_terminal_vt_write(terminal: GhosttyTerminal, data: *const u8, len: usize);
    fn ghostty_terminal_scroll_viewport(
        terminal: GhosttyTerminal,
        behavior: GhosttyTerminalScrollViewport,
    ) -> GhosttyResult;
    fn ghostty_terminal_get(
        terminal: GhosttyTerminal,
        data: i32,
        out: *mut c_void,
    ) -> GhosttyResult;

    fn ghostty_render_state_new(
        allocator: *const c_void,
        state: *mut GhosttyRenderState,
    ) -> GhosttyResult;
    fn ghostty_render_state_free(state: GhosttyRenderState);
    fn ghostty_render_state_update(
        state: GhosttyRenderState,
        terminal: GhosttyTerminal,
    ) -> GhosttyResult;
    fn ghostty_render_state_get(
        state: GhosttyRenderState,
        data: i32,
        out: *mut c_void,
    ) -> GhosttyResult;
    fn ghostty_render_state_row_iterator_new(
        allocator: *const c_void,
        out: *mut GhosttyRenderStateRowIterator,
    ) -> GhosttyResult;
    fn ghostty_render_state_row_iterator_free(it: GhosttyRenderStateRowIterator);
    fn ghostty_render_state_row_iterator_next(it: GhosttyRenderStateRowIterator) -> bool;
    fn ghostty_render_state_row_get(
        it: GhosttyRenderStateRowIterator,
        data: i32,
        out: *mut c_void,
    ) -> GhosttyResult;
    fn ghostty_render_state_row_cells_new(
        allocator: *const c_void,
        out: *mut GhosttyRenderStateRowCells,
    ) -> GhosttyResult;
    fn ghostty_render_state_row_cells_next(cells: GhosttyRenderStateRowCells) -> bool;
    fn ghostty_render_state_row_cells_get(
        cells: GhosttyRenderStateRowCells,
        data: i32,
        out: *mut c_void,
    ) -> GhosttyResult;
    fn ghostty_render_state_row_cells_free(cells: GhosttyRenderStateRowCells);
}

// --- the engine -------------------------------------------------------------

/// What the write-pty callback appends to (boxed: its address is the
/// terminal's userdata).
struct Replies(Vec<u8>);

unsafe extern "C" fn write_pty(
    _t: GhosttyTerminal,
    userdata: *mut c_void,
    data: *const u8,
    len: usize,
) {
    if userdata.is_null() || data.is_null() {
        return;
    }
    // SAFETY: userdata is the `Replies` box this engine owns for the
    // terminal's lifetime; the callback runs synchronously inside vt_write,
    // on the thread that called it.
    let replies = unsafe { &mut *(userdata as *mut Replies) };
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    replies.0.extend_from_slice(bytes);
}

pub struct GhosttyEngine {
    term: GhosttyTerminal,
    render: GhosttyRenderState,
    rows_it: GhosttyRenderStateRowIterator,
    cells: GhosttyRenderStateRowCells,
    replies: Box<Replies>,
    size: (u16, u16),
    /// The last snapshot (selection text reads the visible screen from it).
    last: Snapshot,
}

// SAFETY: the handles are used only through `&mut self` (or `&self` for
// reads that don't touch the library), never shared between threads.
unsafe impl Send for GhosttyEngine {}

impl GhosttyEngine {
    pub fn new(cols: u16, rows: u16) -> Option<Self> {
        let (cols, rows) = (cols.max(2), rows.max(1));
        let mut term: GhosttyTerminal = ptr::null_mut();
        let mut render: GhosttyRenderState = ptr::null_mut();
        let mut rows_it: GhosttyRenderStateRowIterator = ptr::null_mut();
        let mut cells: GhosttyRenderStateRowCells = ptr::null_mut();
        // SAFETY: out-pointers to locals; a NULL allocator is the default.
        unsafe {
            if ghostty_terminal_new(ptr::null(), &mut term, cols, rows) != GHOSTTY_SUCCESS {
                return None;
            }
            if ghostty_render_state_new(ptr::null(), &mut render) != GHOSTTY_SUCCESS
                || ghostty_render_state_row_iterator_new(ptr::null(), &mut rows_it)
                    != GHOSTTY_SUCCESS
                || ghostty_render_state_row_cells_new(ptr::null(), &mut cells) != GHOSTTY_SUCCESS
            {
                ghostty_render_state_row_cells_free(cells);
                ghostty_render_state_row_iterator_free(rows_it);
                ghostty_render_state_free(render);
                ghostty_terminal_free(term);
                return None;
            }
        }
        let mut replies = Box::new(Replies(Vec::new()));
        let userdata = &mut *replies as *mut Replies as *const c_void;
        let callback: GhosttyTerminalWritePtyFn = write_pty;
        // SAFETY: pointer options are passed directly (terminal.h); the box
        // outlives the terminal (dropped after it in `Drop`).
        unsafe {
            ghostty_terminal_set(term, OPT_USERDATA, userdata);
            ghostty_terminal_set(term, OPT_WRITE_PTY, callback as *const c_void);
        }
        Some(GhosttyEngine {
            term,
            render,
            rows_it,
            cells,
            replies,
            size: (cols, rows),
            last: Snapshot::default(),
        })
    }

    fn rs_get<T: Default>(&self, data: i32) -> Option<T> {
        let mut out = T::default();
        // SAFETY: `T` is the output type render.h documents for `data`.
        let r = unsafe {
            ghostty_render_state_get(self.render, data, &mut out as *mut T as *mut c_void)
        };
        (r == GHOSTTY_SUCCESS).then_some(out)
    }

    fn mode(&self, mode: u16) -> bool {
        let mut cfg = GhosttyTerminalModeConfig { mode, value: false };
        // SAFETY: DATA_MODE reads and writes a GhosttyTerminalModeConfig.
        let r = unsafe {
            ghostty_terminal_get(self.term, DATA_MODE, &mut cfg as *mut _ as *mut c_void)
        };
        r == GHOSTTY_SUCCESS && cfg.value
    }

    fn cell_color(&self, data: i32) -> Color {
        let mut c = GhosttyColorRgb::default();
        // SAFETY: FG/BG_COLOR write a GhosttyColorRgb (NO_VALUE: the default).
        let r = unsafe {
            ghostty_render_state_row_cells_get(self.cells, data, &mut c as *mut _ as *mut c_void)
        };
        if r == GHOSTTY_SUCCESS {
            Color::Rgb(Rgb::new(c.r, c.g, c.b))
        } else {
            Color::Default
        }
    }

    fn cell_text(&self, buf: &mut Vec<u8>) -> String {
        let mut b = GhosttyBuffer {
            ptr: buf.as_mut_ptr(),
            cap: buf.len(),
            len: 0,
        };
        // SAFETY: the buffer is ours; on OUT_OF_SPACE `len` is the size needed.
        let mut r = unsafe {
            ghostty_render_state_row_cells_get(
                self.cells,
                CELL_GRAPHEMES_UTF8,
                &mut b as *mut _ as *mut c_void,
            )
        };
        if r != GHOSTTY_SUCCESS && b.len > buf.len() {
            buf.resize(b.len, 0);
            b = GhosttyBuffer {
                ptr: buf.as_mut_ptr(),
                cap: buf.len(),
                len: 0,
            };
            r = unsafe {
                ghostty_render_state_row_cells_get(
                    self.cells,
                    CELL_GRAPHEMES_UTF8,
                    &mut b as *mut _ as *mut c_void,
                )
            };
        }
        if r != GHOSTTY_SUCCESS {
            return String::new();
        }
        String::from_utf8_lossy(&buf[..b.len.min(buf.len())]).into_owned()
    }

    fn scrollbar(&self) -> GhosttyTerminalScrollbar {
        let mut s = GhosttyTerminalScrollbar::default();
        // SAFETY: DATA_SCROLLBAR writes a GhosttyTerminalScrollbar.
        unsafe { ghostty_terminal_get(self.term, DATA_SCROLLBAR, &mut s as *mut _ as *mut c_void) };
        s
    }
}

impl Drop for GhosttyEngine {
    fn drop(&mut self) {
        // SAFETY: handles this engine created; each freed once.
        unsafe {
            ghostty_render_state_row_cells_free(self.cells);
            ghostty_render_state_row_iterator_free(self.rows_it);
            ghostty_render_state_free(self.render);
            ghostty_terminal_free(self.term);
        }
    }
}

impl Engine for GhosttyEngine {
    fn name(&self) -> &'static str {
        "libghostty-vt"
    }

    fn feed(&mut self, bytes: &[u8]) {
        // SAFETY: a valid terminal and a live slice.
        unsafe { ghostty_terminal_vt_write(self.term, bytes.as_ptr(), bytes.len()) };
    }

    fn resize(&mut self, cols: u16, rows: u16, cell_w_px: u32, cell_h_px: u32) {
        let (cols, rows) = (cols.max(2), rows.max(1));
        // SAFETY: a valid terminal.
        let r = unsafe { ghostty_terminal_resize(self.term, cols, rows, cell_w_px, cell_h_px) };
        if r == GHOSTTY_SUCCESS {
            self.size = (cols, rows);
        }
    }

    fn size(&self) -> (u16, u16) {
        self.size
    }

    fn snapshot(&mut self) -> Snapshot {
        // SAFETY: valid handles; row data is read before the next update.
        unsafe { ghostty_render_state_update(self.render, self.term) };
        let cols: u16 = self.rs_get(RS_COLS).unwrap_or(self.size.0);
        let n_rows: u16 = self.rs_get(RS_ROWS).unwrap_or(self.size.1);
        let mut rows = Vec::with_capacity(n_rows as usize);
        let mut it = self.rows_it;
        let mut buf = vec![0u8; 32];
        // SAFETY: ROW_ITERATOR populates our pre-allocated iterator.
        let ok = unsafe {
            ghostty_render_state_get(
                self.render,
                RS_ROW_ITERATOR,
                &mut it as *mut _ as *mut c_void,
            )
        } == GHOSTTY_SUCCESS;
        while ok && unsafe { ghostty_render_state_row_iterator_next(it) } {
            let mut cells = self.cells;
            // SAFETY: ROW_CELLS populates our pre-allocated cells handle.
            let got = unsafe {
                ghostty_render_state_row_get(it, ROW_CELLS, &mut cells as *mut _ as *mut c_void)
            } == GHOSTTY_SUCCESS;
            let mut row = Row::default();
            while got && unsafe { ghostty_render_state_row_cells_next(cells) } {
                let text = self.cell_text(&mut buf);
                let mut chars = text.chars();
                let ch = chars.next().unwrap_or(' ');
                let rest: String = chars.collect();
                row.cells.push(Cell {
                    ch,
                    extra: (!rest.is_empty()).then(|| rest.into_boxed_str()),
                    fg: self.cell_color(CELL_FG_COLOR),
                    bg: self.cell_color(CELL_BG_COLOR),
                    ..Cell::default()
                });
            }
            row.cells.resize(cols as usize, Cell::default());
            rows.push(row);
        }
        rows.resize(
            n_rows as usize,
            Row {
                cells: vec![Cell::default(); cols as usize],
            },
        );
        let visible: bool = self.rs_get(RS_CURSOR_VISIBLE).unwrap_or(false);
        let in_view: bool = self.rs_get(RS_CURSOR_VIEWPORT_HAS_VALUE).unwrap_or(false);
        let cursor = (visible && in_view)
            .then(|| {
                Some(Cursor {
                    col: self.rs_get::<u16>(RS_CURSOR_VIEWPORT_X)?,
                    row: self.rs_get::<u16>(RS_CURSOR_VIEWPORT_Y)?,
                    shape: CursorShape::Block,
                })
            })
            .flatten();
        let bar = self.scrollbar();
        let history = bar.total.saturating_sub(bar.len) as usize;
        let snap = Snapshot {
            cols,
            rows,
            cursor,
            display_offset: history.saturating_sub(bar.offset as usize),
            history,
            modes: self.modes(),
        };
        self.last = snap.clone();
        snap
    }

    fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies.0)
    }

    fn scroll(&mut self, delta: i32) {
        let behavior = GhosttyTerminalScrollViewport {
            tag: SCROLL_DELTA,
            // Ghostty's up is negative; ours is positive.
            value: GhosttyTerminalScrollViewportValue {
                delta: -(delta as isize),
            },
        };
        // SAFETY: a valid terminal and a tagged union by value.
        unsafe { ghostty_terminal_scroll_viewport(self.term, behavior) };
    }

    fn scroll_to_bottom(&mut self) {
        let behavior = GhosttyTerminalScrollViewport {
            tag: SCROLL_BOTTOM,
            value: GhosttyTerminalScrollViewportValue { _padding: [0; 2] },
        };
        // SAFETY: as above.
        unsafe { ghostty_terminal_scroll_viewport(self.term, behavior) };
    }

    fn modes(&self) -> Modes {
        Modes {
            app_cursor: self.mode(MODE_DECCKM),
            bracketed_paste: self.mode(MODE_BRACKETED_PASTE),
            alt_screen: self.mode(MODE_ALT_SCREEN),
        }
    }

    fn text_between(&self, a: GridPoint, b: GridPoint) -> String {
        // The visible screen only (see the module notes).
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        let off = self.last.display_offset as i32;
        let mut out = Vec::new();
        for line in a.line..=b.line {
            let Some(row) = usize::try_from(line + off)
                .ok()
                .and_then(|r| self.last.rows.get(r))
            else {
                continue;
            };
            let from = if line == a.line { a.col as usize } else { 0 };
            let to = if line == b.line {
                b.col as usize + 1
            } else {
                row.cells.len()
            };
            let text: String = row
                .cells
                .iter()
                .skip(from)
                .take(to.saturating_sub(from))
                .map(|c| c.ch)
                .collect();
            out.push(text.trim_end().to_string());
        }
        out.join("\n")
    }

    fn set_default_colors(&mut self, fg: Rgb, bg: Rgb) {
        let rgb = |c: Rgb| GhosttyColorRgb {
            r: c.r,
            g: c.g,
            b: c.b,
        };
        let (fg, bg) = (rgb(fg), rgb(bg));
        // SAFETY: COLOR_FOREGROUND/BACKGROUND take a GhosttyColorRgb*.
        unsafe {
            ghostty_terminal_set(
                self.term,
                OPT_COLOR_FOREGROUND,
                &fg as *const _ as *const c_void,
            );
            ghostty_terminal_set(
                self.term,
                OPT_COLOR_BACKGROUND,
                &bg as *const _ as *const c_void,
            );
        }
    }
}
