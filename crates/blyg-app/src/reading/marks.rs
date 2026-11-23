//! --- read/unread --- Picking posts in the reading list and marking them
//! read or unread (model: `pick_vm.rs`). ⌘-click / ⇧-click / ⌘A pick,
//! esc lets go; r marks read, u unread (also the row's menu: right-click).
//! With nothing picked, the marks act on the open post. Marks are local and
//! instant (`Backend::set_read`); a server that keeps read state gets them
//! in one batch, and unread reaches only a server that can clear read
//! state (otherwise it stays on this Mac).
//!
//! --- mark all read --- **Mark All Read…** marks everything a source holds
//! to read (`sources_vm::needs_reading`) read, after a question with the
//! count ("Mark 37 posts in Rue read?", ⏎ marks, esc cancels): from a
//! source's menu (subscription, folder, hopper; the pane's own menu for the
//! current source), the palette, or ⇧R on the reading screen (the Stream
//! too). For the current source it's what the list (or the stream) shows,
//! so with a search it's the matches, and the question says so. It goes
//! through the same `Backend::set_read` as r, so it's one batch, and read
//! marks sync where the blyg keeps read state (`reading:state`).

use gpui_kit::*;

use super::pick_vm::{self, Picked};
use super::sources_vm::{self, Source};
use super::stream_vm::ReadMode;
use super::vm::{self, Key};
use super::{RSheet, View};
use crate::app::MainView;

gpui_kit::actions!(blygger, [PickAll]);

/// The open "Mark all read?" question.
pub struct MarkAll {
    /// "Rue", "All unread", "the stream".
    pub what: String,
    /// The posts it marks read (the ones to read, none of yours).
    pub keys: Vec<Key>,
    /// The search the list is filtered by (empty: none).
    pub query: String,
    pub focus: FocusHandle,
}

/// The question's heading and its line under it: "Mark 37 posts in Rue
/// read?" / "Only the ones matching “tide”. u marks a post unread again."
pub fn mark_all_question(n: usize, what: &str, query: &str) -> (String, String) {
    let posts = if n == 1 {
        "1 post".to_string()
    } else {
        format!("{n} posts")
    };
    let heading = format!("Mark {posts} in {what} read?");
    let q = query.trim();
    let body = if q.is_empty() {
        "Everything here you haven't read, or that changed since you read it. \
         u marks a post unread again."
            .to_string()
    } else {
        format!("Only the ones matching “{q}”. u marks a post unread again.")
    };
    (heading, body)
}

/// The palette's row id for Mark All Read….
pub const PALETTE_ID: &str = "reading:mark-all-read";

impl MainView {
    /// The keys the list shows, in order.
    pub(crate) fn shown_keys(&self) -> Vec<Key> {
        self.reading.shown_rows().map(vm::key).collect()
    }

    /// A row clicked with ⌘ or ⇧ (Reader mode): pick instead of opening.
    /// False for a plain click, which lets go of the picks and opens it.
    pub(crate) fn pick_click(&mut self, key: &Key, m: Modifiers, cx: &mut Context<Self>) -> bool {
        if self.reading.mode != ReadMode::Reader {
            return false;
        }
        let current = self.reading.sel.clone();
        if m.platform && !m.shift {
            self.reading.picked.toggle(key, current.as_ref());
        } else if m.shift {
            let shown = self.shown_keys();
            self.reading.picked.extend(key, current.as_ref(), &shown);
        } else {
            if !self.reading.picked.is_empty() {
                self.reading.picked.clear();
                cx.notify();
            }
            return false;
        }
        cx.notify();
        true
    }

    /// `PickAll` (⌘A, Ctrl+A on Windows: the keymap's `Reading` row) on the
    /// Reader. A text field there (search, compose, the trail bar) keeps its
    /// own ⌘A: its `Input` binding is deeper, and the checks below pass the
    /// key on when one has the keyboard anyway, or there's no Reader list.
    pub(crate) fn on_pick_all(&mut self, _: &PickAll, window: &mut Window, cx: &mut Context<Self>) {
        let typing = self.reading_search_focused(window, cx)
            || self.reading.compose.has_keyboard(window, cx)
            || self.trail_field_focused(window, cx);
        if self.reading.sheet.is_some()
            || typing
            || self.reading.view != View::Reading
            || self.reading.mode != ReadMode::Reader
        {
            cx.propagate();
            return;
        }
        self.pick_all(cx);
    }

    /// ⌘A in the list: pick everything it shows.
    pub(crate) fn pick_all(&mut self, cx: &mut Context<Self>) {
        let shown = self.shown_keys();
        self.reading.picked.all(&shown);
        if !shown.is_empty() {
            self.show_toast(
                format!("{} picked", shown.len()),
                Some("r marks them read, u unread · esc lets go".into()),
                cx,
            );
        }
        cx.notify();
    }

    /// What a mark acts on now: the picked posts, else the open one.
    pub(crate) fn mark_targets(&self) -> Vec<Key> {
        let open = self
            .reading
            .opened
            .as_ref()
            .map(|o| o.key.clone())
            .or_else(|| self.reading.sel.clone());
        self.reading
            .picked
            .targets(&self.shown_keys(), open.as_ref())
    }

    /// Mark these posts read or unread (local, instant).
    pub(crate) fn mark_posts(&mut self, keys: Vec<Key>, read: bool, cx: &mut Context<Self>) {
        // --- own posts --- yours have no read state, and none is sent.
        let keys: Vec<Key> = keys
            .into_iter()
            .filter(|k| !super::own::is_own(k))
            .collect();
        if keys.is_empty() {
            return self.show_toast("Nothing to mark", Some(PICK_HOW.get().into()), cx);
        }
        let n = keys.len();
        match self.backend.set_read(&keys, read) {
            Ok(_) => {
                // The open post keeps "edited since you read it" from when it
                // opened; a mark read clears that, a mark unread shows it.
                if let Some(o) = self.reading.opened.as_mut()
                    && keys.contains(&o.key)
                {
                    o.item.read_version = (read).then_some(o.item.version);
                }
                self.show_toast(
                    format!(
                        "Marked {} {}",
                        if n == 1 {
                            "1 post".to_string()
                        } else {
                            format!("{n} posts")
                        },
                        if read { "read" } else { "unread" }
                    ),
                    None,
                    cx,
                );
                self.reading_changed(cx);
                // --- unread counts --- the status bar's count follows at once.
                self.unread = self.backend.unread_count();
            }
            Err(e) => self.show_toast(format!("Couldn't mark them: {e}"), None, cx),
        }
    }

    /// r / u, and the row menu's Mark Read / Mark Unread.
    pub(crate) fn mark_picked(&mut self, read: bool, cx: &mut Context<Self>) {
        let keys = self.mark_targets();
        self.mark_posts(keys, read, cx);
    }

    /// Right-click on a row: its menu acts on the picks when it's among
    /// them, else on that row alone (the picks let go).
    pub(crate) fn open_post_menu(&mut self, key: Key, at: Point<Pixels>, cx: &mut Context<Self>) {
        if !self.reading.picked.contains(&key) {
            self.reading.picked = Picked::default();
        }
        self.open_source_menu(super::sources::MenuTarget::Posts(key), at, false, cx);
    }

    /// The row menu's items (`menu_items`).
    pub(crate) fn post_menu_items(&self, key: &Key) -> Vec<(String, super::sources::MenuCmd)> {
        use super::sources::MenuCmd;
        let keys = if self.reading.picked.contains(key) {
            self.reading.picked.targets(&self.shown_keys(), None)
        } else {
            vec![key.clone()]
        };
        let n = keys.len();
        let rows: Vec<_> = keys
            .iter()
            .filter_map(|k| self.reading.rows.iter().find(|r| vm::key(r) == *k))
            .collect();
        let any_read = rows.iter().any(|r| r.read_version.is_some());
        let any_unread = rows.iter().any(|r| r.is_unread() || r.edited_since_read());
        let mut v = Vec::new();
        if any_unread || rows.is_empty() {
            v.push((
                pick_vm::mark_label(true, n),
                MenuCmd::Mark(keys.clone(), true),
            ));
        }
        if any_read || rows.is_empty() {
            v.push((pick_vm::mark_label(false, n), MenuCmd::Mark(keys, false)));
        }
        v
    }
}

impl MainView {
    // ------------------------------------------------------- mark all read

    /// The name Mark All Read… gives `source` ("the stream" in the Stream).
    pub(crate) fn mark_all_name(&self, source: Option<&Source>) -> String {
        let r = &self.reading;
        let s = match source {
            Some(s) => s,
            None if r.mode == ReadMode::Stream => return "the stream".into(),
            None => &r.source,
        };
        match s {
            // --- trails --- the trail's name.
            Source::Trail(id) => r
                .trails
                .list
                .iter()
                .find(|t| &t.id == id)
                .map_or_else(|| "the trail".into(), |t| t.name.clone()),
            s => sources_vm::source_label(s, &r.folders, &r.subs, &r.hoppers),
        }
    }

    /// `source` is what's on screen (`None` is the current one).
    fn is_current_source(&self, source: Option<&Source>) -> bool {
        source.is_none_or(|s| self.reading.mode == ReadMode::Reader && *s == self.reading.source)
    }

    /// The posts Mark All Read… would mark for `source` (`None`: the
    /// current one): to read, never yours. For the source on screen it's
    /// what the list or the stream shows (a search narrows it); for
    /// another, everything that source holds.
    pub(crate) fn mark_all_keys(&self, source: Option<&Source>) -> Vec<Key> {
        let r = &self.reading;
        let wanted = |row: &&blyg_core::ReadingItem| {
            sources_vm::needs_reading(row) && !super::own::is_own_row(row)
        };
        let Some(source) = source.filter(|_| !self.is_current_source(source)) else {
            return r.shown_rows().filter(wanted).map(vm::key).collect();
        };
        let now = chrono::Utc::now();
        // Rows only a hopper holds count in that hopper alone.
        let in_hopper = matches!(source, Source::Hopper(_));
        r.rows
            .iter()
            .filter(wanted)
            .filter(|row| sources_vm::in_source(row, source, &r.filed, now))
            .filter(|row| in_hopper || !r.hopper_only.contains(&vm::key(row)))
            .map(vm::key)
            .collect()
    }

    /// Mark All Read…: ask first, with the count. Nothing to read says so
    /// instead.
    pub(crate) fn ask_mark_all_read(
        &mut self,
        source: Option<Source>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reading.src_menu = None;
        if self.reading.view != View::Reading {
            return self.show_toast(crate::keymap::hint("Open Reading first (⌘R)"), None, cx);
        }
        let keys = self.mark_all_keys(source.as_ref());
        let what = self.mark_all_name(source.as_ref());
        if keys.is_empty() {
            return self.show_toast(format!("Nothing to read in {what}"), None, cx);
        }
        let query = if self.is_current_source(source.as_ref()) {
            self.reading.query.clone()
        } else {
            String::new()
        };
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.reading.sheet_gen += 1;
        self.reading.sheet = Some(RSheet::MarkAll(Box::new(MarkAll {
            what,
            keys,
            query,
            focus,
        })));
        cx.notify();
    }

    /// ⏎ in the question: mark them read (one `set_read`, one batch).
    pub(crate) fn confirm_mark_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(RSheet::MarkAll(m)) = self.reading.sheet.take() else {
            return;
        };
        self.close_reading_sheet(window, cx);
        self.mark_posts(m.keys, true, cx);
    }

    /// The palette's row: Mark All Read… for the current source.
    pub(crate) fn mark_all_palette_entry(&self) -> crate::app::command_palette::Entry {
        let reading = self.reading.view == View::Reading;
        crate::app::command_palette::Entry {
            id: PALETTE_ID.into(),
            cmd: crate::app::command_palette::Cmd::MarkAllRead,
            group: "Blyg",
            label: "Mark All Read…".into(),
            detail: if reading {
                crate::keymap::hint_owned(format!(
                    "Everything to read in {}, after a question · ⇧R",
                    self.mark_all_name(None)
                ))
            } else {
                crate::keymap::hint(
                    "Everything to read in the current source · ⇧R on the reading screen",
                )
                .into()
            },
            aliases: &["mark all", "all read", "clear unread", "read all"],
            key: None,
            disabled: (!reading).then(|| crate::keymap::hint("Open Reading first (⌘R)").into()),
        }
    }

    pub(super) fn render_mark_all(&self, m: &MarkAll, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let (heading, body) = mark_all_question(m.keys.len(), &m.what, &m.query);
        div()
            .id("mark-all-sheet")
            .debug_selector(|| "mark-all-sheet".into())
            .track_focus(&m.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match ev.keystroke.key.as_str() {
                    "enter" => this.confirm_mark_all(window, cx),
                    "escape" => this.close_reading_sheet(window, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(self.sheet_heading(heading))
            .child(
                div()
                    .line_height(relative(1.45))
                    .text_color(p.muted)
                    .child(body),
            )
            .child(self.keys_row(vec![
                self.key_hint("⏎", "mark read"),
                self.key_hint("esc", "cancel"),
            ]))
            .into_any_element()
    }
}

pub(crate) const PICK_HOW: crate::platform_text::Text =
    crate::platform_text::ui_text("Open a post, or pick some: ⌘-click, ⇧-click, ⌘A");

#[cfg(test)]
mod tests {
    use super::mark_all_question;

    #[test]
    fn the_question_says_the_count_and_the_source() {
        let (h, b) = mark_all_question(37, "Rue", "");
        assert_eq!(h, "Mark 37 posts in Rue read?");
        assert!(b.contains("u marks a post unread again"));
        assert_eq!(
            mark_all_question(1, "the stream", "").0,
            "Mark 1 post in the stream read?"
        );
        let (_, b) = mark_all_question(3, "All unread", " tide ");
        assert!(b.starts_with("Only the ones matching “tide”"), "{b}");
    }
}
