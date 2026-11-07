//! --- terminal --- The pane itself: a session (engine + pty), its keys,
//! mouse selection, the clipboard, and the drawing.
//!
//! Keys: the pane takes every key while it has focus, ahead of the main
//! window's bindings (a keystroke interceptor, so ↑, ⏎, esc and ⌃C reach the
//! shell, not the list), except ⌘-keys on macOS and Ctrl+Shift-keys on
//! Windows, which stay the app's, and ⌃` itself. ⌘C / ⌘V (Ctrl+Shift+C / V
//! on Windows) copy the selection and paste.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use blyg_term::pty::{Output, Pty, ShellSpec};
use blyg_term::{Cell, Color, CursorShape, Engine, GridPoint, Key, Rgb, Snapshot};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::theme::{Palette, mix};

/// The pane's monospace face: Menlo on macOS, Consolas on Windows.
pub const MONO: &str = if cfg!(target_os = "windows") {
    "Consolas"
} else {
    "Menlo"
};
pub const FONT_SIZE: f32 = 12.5;
/// The key context while the pane has the keyboard.
pub const CONTEXT: &str = "BurrowTerminal";

/// What the pane tells the main window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermEvent {
    /// The shell ended: close the pane.
    Exited,
    /// The header's "hide".
    Hide,
    /// The header's "end": kill the shell.
    End,
}

/// The pty's output, waiting for the UI thread.
#[derive(Default)]
struct Inbox {
    bytes: Vec<u8>,
    exited: bool,
}

/// The grid as last laid out (prepaint): where it is and how big a cell is.
#[derive(Debug, Clone, Copy)]
struct Layout {
    origin: Point<Pixels>,
    cell_w: Pixels,
    cell_h: Pixels,
}

pub struct TerminalView {
    pub focus: FocusHandle,
    engine: Box<dyn Engine>,
    pty: Option<Pty>,
    shell: String,
    /// Why the shell didn't start.
    error: Option<String>,
    inbox: Arc<Mutex<Inbox>>,
    exited: bool,
    palette: Palette,
    chrome: SharedString,
    layout: Option<Layout>,
    /// The last snapshot drawn.
    snap: Snapshot,
    /// Anchor and head of the mouse selection (buffer points).
    selection: Option<(GridPoint, GridPoint)>,
    selecting: bool,
    scroll_px: Pixels,
    /// Everything written to the pty (tests: keys and pastes go here when
    /// there's no shell).
    pub sent: Vec<u8>,
    _interceptor: Subscription,
}

impl EventEmitter<TermEvent> for TerminalView {}

impl TerminalView {
    /// A session running `spec` (`None`: no process; tests feed it).
    pub fn new(
        spec: Option<ShellSpec>,
        palette: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let (cols, rows) = (80, 24);
        let mut engine = blyg_term::new_engine(cols, rows);
        engine.set_default_colors(to_rgb(palette.ink), to_rgb(palette.editor));
        let inbox: Arc<Mutex<Inbox>> = Default::default();
        let (mut pty, mut error) = (None, None);
        let shell = spec.as_ref().map(|s| s.name()).unwrap_or_default();
        if let Some(spec) = spec {
            let (tx, rx) = async_channel::unbounded::<()>();
            let sink = inbox.clone();
            match Pty::spawn(&spec, cols, rows, move |o| {
                if let Ok(mut i) = sink.lock() {
                    match o {
                        Output::Bytes(b) => i.bytes.extend(b),
                        Output::Exited => i.exited = true,
                    }
                }
                let _ = tx.try_send(());
            }) {
                Ok(p) => pty = Some(p),
                Err(e) => error = Some(format!("Couldn't start {}: {e}", spec.program)),
            }
            cx.spawn_in(window, async move |this, cx| {
                while rx.recv().await.is_ok() {
                    if this.update(cx, |t, cx| t.drain(cx)).is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
        // Keys reach the pane before the window's bindings (see the module
        // notes); only while it has the keyboard.
        let weak = cx.entity().downgrade();
        let f = focus.clone();
        let interceptor = cx.intercept_keystrokes(move |ev, window, cx| {
            if !f.is_focused(window) {
                return;
            }
            let handled = weak
                .update(cx, |t, cx| t.key_down(&ev.keystroke, cx))
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        });
        TerminalView {
            focus,
            engine,
            pty,
            shell,
            error,
            inbox,
            exited: false,
            palette,
            chrome: "Inter".into(),
            layout: None,
            snap: Snapshot::default(),
            selection: None,
            selecting: false,
            scroll_px: px(0.),
            sent: Vec::new(),
            _interceptor: interceptor,
        }
    }

    /// The theme (and chrome font) for the next frame.
    pub fn set_style(&mut self, p: Palette, chrome: SharedString) {
        if (self.palette.ink, self.palette.editor) != (p.ink, p.editor) {
            self.engine
                .set_default_colors(to_rgb(p.ink), to_rgb(p.editor));
        }
        self.palette = p;
        self.chrome = chrome;
    }

    /// Which engine draws (`alacritty_terminal`).
    #[cfg(test)]
    pub fn engine_name(&self) -> &'static str {
        self.engine.name()
    }

    /// The pane's size in cells.
    #[cfg(test)]
    pub fn size(&self) -> (u16, u16) {
        self.engine.size()
    }

    /// The text on screen (tests).
    #[cfg(test)]
    pub fn screen_text(&mut self) -> Vec<String> {
        self.engine.snapshot().lines()
    }

    /// Bytes as if the shell had written them (tests; the pty's go through
    /// `drain`).
    pub fn feed(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.engine.feed(bytes);
        let replies = self.engine.take_replies();
        self.write(replies);
        cx.notify();
    }

    /// The pty's output into the engine, its replies back out.
    fn drain(&mut self, cx: &mut Context<Self>) {
        let (bytes, exited) = match self.inbox.lock() {
            Ok(mut i) => (std::mem::take(&mut i.bytes), i.exited),
            Err(_) => return,
        };
        if !bytes.is_empty() {
            self.feed(&bytes, cx);
            // A synchronized update still open: look again when it times out.
            if let Some(t) = self.engine.pending_until() {
                let wait = t.saturating_duration_since(Instant::now());
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(wait).await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                })
                .detach();
            }
        }
        if exited && !self.exited {
            self.exited = true;
            cx.emit(TermEvent::Exited);
        }
    }

    fn write(&mut self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        match &self.pty {
            Some(p) => p.write(bytes),
            None => self.sent.extend(bytes),
        }
    }

    /// A paste (⌘V, or a quote sent from the editor): bracketed when the
    /// program asked, escapes removed.
    pub fn paste(&mut self, text: &str, cx: &mut Context<Self>) {
        let bytes = blyg_term::encode_paste(text, self.engine.modes());
        self.selection = None;
        self.engine.scroll_to_bottom();
        self.write(bytes);
        cx.notify();
    }

    /// The selection's text.
    pub fn selected_text(&self) -> Option<String> {
        let (a, b) = self.selection?;
        (a != b).then(|| self.engine.text_between(a, b))
    }

    /// Select from `a` to `b` (tests; the mouse does it in the app).
    #[cfg(test)]
    pub fn select(&mut self, a: GridPoint, b: GridPoint) {
        self.selection = Some((a, b));
    }

    /// A key while the pane has focus. `true`: the pane took it.
    pub fn key_down(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> bool {
        let m = ks.modifiers;
        let key = ks.key.as_str();
        // ⌃` belongs to the window (it hides the pane).
        if m.control && !m.alt && !m.platform && key == "`" {
            return false;
        }
        let (copy, paste) = if cfg!(target_os = "windows") {
            let cs = m.control && m.shift && !m.alt;
            (cs && key == "c", cs && key == "v")
        } else {
            let cmd = m.platform && !m.control && !m.alt && !m.shift;
            (cmd && key == "c", cmd && key == "v")
        };
        if copy {
            if let Some(t) = self.selected_text() {
                cx.write_to_clipboard(ClipboardItem::new_string(t));
            }
            return true;
        }
        if paste {
            if let Some(t) = cx.read_from_clipboard().and_then(|c| c.text()) {
                self.paste(&t, cx);
            }
            return true;
        }
        // The app's own keys: ⌘ on macOS; Ctrl+Shift on Windows, and
        // Ctrl+Alt with an arrow (the app's ⌥⌘-arrows there, the tour's Next
        // and Back among them). Other Ctrl+Alt keys stay here: that's AltGr,
        // which types characters on many layouts.
        let windows = cfg!(target_os = "windows");
        if m.platform
            || (windows && m.control && m.shift)
            || (windows && m.control && m.alt && matches!(key, "left" | "right" | "up" | "down"))
        {
            return false;
        }
        let k = Key {
            key: ks.key.clone(),
            text: ks.key_char.clone(),
            ctrl: m.control,
            alt: m.alt,
            shift: m.shift,
            cmd: m.platform,
        };
        // Shift+PageUp/PageDown scroll the history, as in Terminal.app.
        if m.shift && !m.control && !m.alt && matches!(key, "pageup" | "pagedown") {
            let rows = self.engine.size().1.saturating_sub(1).max(1) as i32;
            self.engine
                .scroll(if key == "pageup" { rows } else { -rows });
            cx.notify();
            return true;
        }
        let Some(bytes) = self.engine.encode_key(&k) else {
            return false;
        };
        self.selection = None;
        self.engine.scroll_to_bottom();
        self.write(bytes);
        cx.notify();
        true
    }

    /// The grid's new size from the layout: resize the engine and the pty.
    fn set_grid(&mut self, layout: Layout, bounds: Bounds<Pixels>) -> Snapshot {
        self.layout = Some(layout);
        let cols = ((bounds.size.width / layout.cell_w).floor() as u16).max(2);
        let rows = ((bounds.size.height / layout.cell_h).floor() as u16).max(1);
        if (cols, rows) != self.engine.size() {
            self.engine.resize(
                cols,
                rows,
                f32::from(layout.cell_w).round() as u32,
                f32::from(layout.cell_h).round() as u32,
            );
            if let Some(p) = &self.pty {
                p.resize(cols, rows);
            }
        }
        self.snap = self.engine.snapshot();
        self.snap.clone()
    }

    /// The buffer point under a window position.
    fn point_at(&self, pos: Point<Pixels>) -> Option<GridPoint> {
        let l = self.layout?;
        let (cols, rows) = self.engine.size();
        let x = ((pos.x - l.origin.x) / l.cell_w).floor().max(0.) as u16;
        let y = ((pos.y - l.origin.y) / l.cell_h).floor().max(0.) as i32;
        Some(GridPoint {
            line: y.min(rows as i32 - 1) - self.snap.display_offset as i32,
            col: x.min(cols.saturating_sub(1)),
        })
    }

    fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        if let Some(p) = self.point_at(e.position) {
            self.selection = Some((p, p));
            self.selecting = true;
        }
        cx.notify();
    }

    fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting || e.pressed_button != Some(MouseButton::Left) {
            return;
        }
        if let (Some(p), Some((a, _))) = (self.point_at(e.position), self.selection) {
            self.selection = Some((a, p));
            cx.notify();
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = false;
        if self.selection.is_some_and(|(a, b)| a == b) {
            self.selection = None;
        }
        cx.notify();
    }

    fn scroll_wheel(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(l) = self.layout else { return };
        self.scroll_px += e.delta.pixel_delta(l.cell_h).y;
        let lines = (self.scroll_px / l.cell_h) as i32;
        if lines == 0 {
            return;
        }
        self.scroll_px -= l.cell_h * lines as f32;
        if self.engine.modes().alt_screen {
            // A full-screen program: the wheel is its arrows.
            let key = Key::named(if lines > 0 { "up" } else { "down" });
            if let Some(b) = self.engine.encode_key(&key) {
                self.write(b.repeat(lines.unsigned_abs() as usize));
            }
        } else {
            self.engine.scroll(lines);
        }
        cx.notify();
    }

    fn render_header(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let name = if self.shell.is_empty() {
            "terminal".to_string()
        } else {
            self.shell.clone()
        };
        let hide = crate::keymap::hint("⌃` hide");
        div()
            .flex_none()
            .h(px(24.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .bg(p.panel())
            .font_family(self.chrome.clone())
            .text_size(px(11.))
            .text_color(p.muted)
            .child(
                div()
                    .text_color(p.ink)
                    .font_weight(FontWeight::MEDIUM)
                    .child(name),
            )
            .when(super::EXPERIMENTAL, |d| {
                d.child(
                    div()
                        .px(px(5.))
                        .rounded(px(3.))
                        .border_1()
                        .border_color(p.line)
                        .text_size(px(9.5))
                        .child("experimental"),
                )
            })
            .child(div().child("scrubbed env · no BLYGGER_* · no token"))
            .child(div().flex_1())
            .child(
                div()
                    .id("terminal-hide")
                    .cursor_pointer()
                    .hover(|s| s.text_color(p.ink))
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(TermEvent::Hide)))
                    .child(hide),
            )
            .child(
                div()
                    .id("terminal-end")
                    .cursor_pointer()
                    .hover(|s| s.text_color(p.ink))
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(TermEvent::End)))
                    .child("end"),
            )
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let colors = TermColors::new(&p);
        let focused = self.focus.is_focused(window);
        let selection = self.selection;
        let weak = cx.entity().downgrade();
        let grid = canvas(
            move |bounds, window, cx| {
                let font = font(MONO);
                let size = px(FONT_SIZE);
                let ts = window.text_system();
                let id = ts.resolve_font(&font);
                let cell_w = ts
                    .advance(id, size, 'm')
                    .map(|s| s.width)
                    .unwrap_or(px(FONT_SIZE * 0.6));
                let layout = Layout {
                    origin: bounds.origin,
                    cell_w,
                    cell_h: (size * 1.3).round(),
                };
                let snap = weak
                    .update(cx, |t, _| t.set_grid(layout, bounds))
                    .unwrap_or_default();
                (layout, snap)
            },
            move |_bounds, (layout, snap), window, cx| {
                paint_grid(&snap, layout, &colors, selection, focused, window, cx);
            },
        )
        .size_full();
        div()
            .id("terminal")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(p.editor)
            .child(self.render_header(&p, cx))
            .child(
                div()
                    .id("terminal-grid")
                    .flex_1()
                    .min_h_0()
                    .px(px(8.))
                    .py(px(4.))
                    .overflow_hidden()
                    .cursor(CursorStyle::IBeam)
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                    .on_mouse_move(cx.listener(Self::mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_scroll_wheel(cx.listener(Self::scroll_wheel))
                    .map(|d| match &self.error {
                        Some(e) => d.text_color(p.over).text_size(px(12.)).child(e.clone()),
                        None => d.child(grid),
                    }),
            )
    }
}

// --- colours -----------------------------------------------------------------

/// The theme's terminal colours: its ink and paper, and sixteen ANSI colours
/// drawn from its palette.
#[derive(Clone)]
pub struct TermColors {
    pub fg: Hsla,
    pub bg: Hsla,
    pub ansi: [Hsla; 16],
    pub cursor: Hsla,
    pub selection: Hsla,
}

impl TermColors {
    pub fn new(p: &Palette) -> Self {
        let (ink, paper) = (p.ink, p.editor);
        let black = if p.dark { mix(paper, ink, 0.25) } else { ink };
        let white = if p.dark { ink } else { mix(paper, ink, 0.3) };
        let blue = p.accent;
        let magenta = mix(p.over, blue, 0.5);
        let cyan = mix(p.green, blue, 0.5);
        let base = [black, p.over, p.green, p.amber, blue, magenta, cyan, white];
        let bright = |c: Hsla| {
            if p.dark {
                mix(c, gpui_kit::white(), 0.2)
            } else {
                mix(c, gpui_kit::black(), 0.15)
            }
        };
        let mut ansi = [ink; 16];
        for i in 0..8 {
            ansi[i] = base[i];
            ansi[i + 8] = bright(base[i]);
        }
        ansi[8] = p.muted;
        TermColors {
            fg: ink,
            bg: paper,
            ansi,
            cursor: p.accent,
            selection: p.text_selection,
        }
    }

    fn resolve(&self, c: Color, default: Hsla) -> Hsla {
        match c {
            Color::Default => default,
            Color::Indexed(i) if i < 16 => self.ansi[i as usize],
            Color::Indexed(i) => from_rgb(blyg_term::xterm_256(i)),
            Color::Rgb(rgb) => from_rgb(rgb),
        }
    }

    /// A cell's (foreground, background), attributes applied.
    pub fn cell(&self, c: &Cell) -> (Hsla, Hsla) {
        let mut fg = self.resolve(c.fg, self.fg);
        let mut bg = self.resolve(c.bg, self.bg);
        if c.attrs.inverse {
            std::mem::swap(&mut fg, &mut bg);
        }
        if c.attrs.dim {
            fg = mix(fg, bg, 0.4);
        }
        if c.attrs.hidden {
            fg = bg;
        }
        (fg, bg)
    }
}

fn from_rgb(c: Rgb) -> Hsla {
    rgb(((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32).into()
}

fn to_rgb(c: Hsla) -> Rgb {
    let r = c.to_rgb();
    let q = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    Rgb::new(q(r.r), q(r.g), q(r.b))
}

// --- drawing -----------------------------------------------------------------

/// Whether viewport cell (`row`, `col`) is in the selection.
fn selected(sel: Option<(GridPoint, GridPoint)>, offset: usize, row: usize, col: usize) -> bool {
    let Some((a, b)) = sel else { return false };
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    if a == b {
        return false;
    }
    let p = GridPoint {
        line: row as i32 - offset as i32,
        col: col as u16,
    };
    a <= p && p <= b
}

fn paint_grid(
    snap: &Snapshot,
    l: Layout,
    colors: &TermColors,
    sel: Option<(GridPoint, GridPoint)>,
    focused: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let base = font(MONO);
    let size = px(FONT_SIZE);
    for (y, row) in snap.rows.iter().enumerate() {
        let top = l.origin.y + l.cell_h * y as f32;
        // Backgrounds: runs of one colour, the selection over them.
        let mut x = 0;
        while x < row.cells.len() {
            let pick = |i: usize| {
                if selected(sel, snap.display_offset, y, i) {
                    Some(colors.selection)
                } else {
                    let bg = colors.cell(&row.cells[i]).1;
                    (bg != colors.bg).then_some(bg)
                }
            };
            let here = pick(x);
            let mut end = x + 1;
            while end < row.cells.len() && pick(end) == here {
                end += 1;
            }
            if let Some(bg) = here {
                window.paint_quad(fill(
                    Bounds::new(
                        point(l.origin.x + l.cell_w * x as f32, top),
                        size2(l.cell_w * (end - x) as f32, l.cell_h),
                    ),
                    bg,
                ));
            }
            x = end;
        }
        // Text: one shaped run per segment (a row, split at wide characters
        // so every glyph lands on its cell).
        let last = row
            .cells
            .iter()
            .rposition(|c| c.ch != ' ' || c.attrs.underline || c.extra.is_some())
            .map(|i| i + 1)
            .unwrap_or(0);
        let mut seg = Segment::default();
        for (i, c) in row.cells.iter().enumerate().take(last) {
            if c.spacer {
                continue;
            }
            if c.wide {
                seg.paint(&l, top, size, window, cx);
                let mut one = Segment::at(i);
                one.push(c, colors, &base);
                one.paint(&l, top, size, window, cx);
                seg = Segment::at(i + 2);
                continue;
            }
            if seg.text.is_empty() {
                seg.col = i;
            }
            seg.push(c, colors, &base);
        }
        seg.paint(&l, top, size, window, cx);
    }
    // The cursor: a solid block (focused) or an outline, a bar, or a line.
    if let Some(c) = snap.cursor {
        let at = point(
            l.origin.x + l.cell_w * c.col as f32,
            l.origin.y + l.cell_h * c.row as f32,
        );
        let cell = Bounds::new(at, size2(l.cell_w, l.cell_h));
        match (c.shape, focused) {
            (CursorShape::Block, true) => {
                window.paint_quad(fill(cell, colors.cursor.opacity(0.55)));
            }
            (CursorShape::Block, false) => {
                window.paint_quad(outline(cell, colors.cursor, BorderStyle::Solid));
            }
            (CursorShape::Beam, _) => {
                window.paint_quad(fill(
                    Bounds::new(at, size2(px(2.), l.cell_h)),
                    colors.cursor,
                ));
            }
            (CursorShape::Underline, _) => window.paint_quad(fill(
                Bounds::new(
                    point(at.x, at.y + l.cell_h - px(2.)),
                    size2(l.cell_w, px(2.)),
                ),
                colors.cursor,
            )),
        }
    }
}

fn size2(w: Pixels, h: Pixels) -> Size<Pixels> {
    Size {
        width: w,
        height: h,
    }
}

/// Cells of a row that shape as one line.
#[derive(Default)]
struct Segment {
    col: usize,
    text: String,
    runs: Vec<TextRun>,
}

impl Segment {
    fn at(col: usize) -> Self {
        Segment {
            col,
            ..Default::default()
        }
    }

    fn push(&mut self, c: &Cell, colors: &TermColors, base: &Font) {
        let start = self.text.len();
        self.text.push(c.ch);
        if let Some(e) = &c.extra {
            self.text.push_str(e);
        }
        let (fg, _) = colors.cell(c);
        let font = Font {
            weight: if c.attrs.bold {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            },
            style: if c.attrs.italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
            ..base.clone()
        };
        let run = TextRun {
            len: self.text.len() - start,
            font,
            color: fg,
            background_color: None,
            underline: c.attrs.underline.then_some(UnderlineStyle {
                thickness: px(1.),
                color: Some(fg),
                wavy: false,
            }),
            strikethrough: c.attrs.strike.then_some(StrikethroughStyle {
                thickness: px(1.),
                color: Some(fg),
            }),
        };
        match self.runs.last_mut() {
            Some(last)
                if last.font == run.font
                    && last.color == run.color
                    && last.underline == run.underline
                    && last.strikethrough == run.strikethrough =>
            {
                last.len += run.len
            }
            _ => self.runs.push(run),
        }
    }

    fn paint(&mut self, l: &Layout, top: Pixels, size: Pixels, window: &mut Window, cx: &mut App) {
        if self.text.trim().is_empty() && self.runs.iter().all(|r| r.underline.is_none()) {
            self.text.clear();
            self.runs.clear();
            return;
        }
        let text: SharedString = std::mem::take(&mut self.text).into();
        let runs = std::mem::take(&mut self.runs);
        let line = window
            .text_system()
            .shape_line(text, size, &runs, Some(l.cell_w));
        let origin = point(l.origin.x + l.cell_w * self.col as f32, top);
        let _ = line.paint(origin, l.cell_h, TextAlign::Left, None, window, cx);
    }
}
