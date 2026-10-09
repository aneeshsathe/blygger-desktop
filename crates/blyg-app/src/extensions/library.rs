//! --- extensions --- A library (the bundled markdown-notes' folder of
//! Markdown notes) in the notes drawer, beside the reading notes: browse
//! its folders, search it, open a note, edit and save it, make a new one,
//! and copy a note (or the selected part) into the post being written, as
//! a quote or verbatim. And the quote picker's read-only Notes chip.
//!
//! Every call goes to the extension off the main thread and comes back
//! when it answers; typing never waits. A search that takes longer than
//! the host's 2 s says "Notes didn't answer". A save names the hash the
//! note was read at, so a note changed on disk meanwhile is never
//! overwritten: the panel says so and offers to reload it or keep the edit
//! as a new note. Frontmatter is kept as it was (the editor shows the body).

use blyg_ext::LibraryRef;
use blyg_ext::protocol::{
    LibraryDocument, LibraryEntry, LibraryListParams, LibraryReadParams, LibrarySearchParams,
    LibraryWriteParams, SourceEntry,
};
use blyg_ext::{ExtError, Host};
use gpui_kit::base::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::MainView;
use crate::theme::Rule;

/// How many search hits the panel and the picker ask for.
pub const SEARCH_LIMIT: usize = 50;

/// A note open in the panel.
pub(crate) struct OpenNote {
    /// `None`: a new note, not saved yet.
    pub id: Option<String>,
    pub title: String,
    /// The whole file as read (frontmatter included).
    pub markdown: String,
    /// What a save is based on.
    pub hash: String,
    /// The body as last read or saved (dirty = the editor differs).
    pub saved_body: String,
    /// The last save was refused: it changed on disk.
    pub stale: bool,
    /// Where a new note goes.
    pub folder: Option<String>,
}

/// The panel's state (part of `Extensions`).
#[derive(Default)]
pub(crate) struct Library {
    /// The drawer shows the library instead of the reading notes.
    pub tab: bool,
    /// The folder on screen (`None`: the top).
    pub path: Option<String>,
    pub entries: Vec<LibraryEntry>,
    pub search: Option<Entity<InputState>>,
    pub query: String,
    /// Search results while there's a query.
    pub hits: Option<Vec<SourceEntry>>,
    search_gen: u64,
    /// "Notes didn't answer", a refusal, "Saved".
    pub status: Option<String>,
    pub note: Option<OpenNote>,
    pub editor: Option<Entity<TextareaState>>,
    /// Setting the editor's text (not an edit).
    loading: bool,
    // --- the quote picker's Notes chip ---
    pub picker: bool,
    pub picker_hits: Vec<SourceEntry>,
    picker_gen: u64,
    pub picker_status: Option<String>,
}

impl Library {
    /// The open note has edits not saved yet.
    pub fn dirty(&self, cx: &App) -> bool {
        match (&self.note, &self.editor) {
            (Some(n), Some(e)) => e.read(cx).value().as_ref() != n.saved_body,
            _ => false,
        }
    }
}

/// What a library call failed with, in words ("Notes didn't answer").
fn failure(lib: &LibraryRef, e: &ExtError) -> String {
    match e {
        ExtError::Timeout => format!("{} didn't answer", lib.library.title),
        ExtError::NotRunning => format!("{} isn't running", lib.ext),
        ExtError::Stale { .. } => "Changed on disk since you opened it".into(),
        ExtError::Rpc(r) => format!("{}: {}", lib.library.title, r.message),
    }
}

impl MainView {
    /// The library the panel shows: the first running extension's first.
    pub(crate) fn ext_library(&self) -> Option<LibraryRef> {
        self.ext.host.as_ref()?.libraries().into_iter().next()
    }

    fn ext_lib_host(&self) -> Option<(Host, LibraryRef)> {
        Some((self.ext.host.clone()?, self.ext_library()?))
    }

    /// Make the search box and the note editor (once).
    fn ext_lib_ensure(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ext.lib.search.is_none() {
            let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search notes…"));
            self._subs.push(cx.subscribe_in(
                &search,
                window,
                |this, input, ev: &InputEvent, window, cx| match ev {
                    InputEvent::Change => {
                        let q = input.read(cx).value().to_string();
                        this.ext_lib_search(q, window, cx);
                    }
                    InputEvent::PressEnter { .. } => {
                        let first = this
                            .ext
                            .lib
                            .hits
                            .as_ref()
                            .and_then(|h| h.first())
                            .map(|h| h.id.clone());
                        if let Some(id) = first {
                            this.ext_lib_open(id, window, cx);
                        }
                    }
                    _ => {}
                },
            ));
            self.ext.lib.search = Some(search);
        }
        if self.ext.lib.editor.is_none() {
            let editor = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .soft_wrap(true)
                    .placeholder("Write the note…")
            });
            self._subs.push(
                cx.subscribe_in(&editor, window, |this, _, ev: &InputEvent, _, cx| {
                    if matches!(ev, InputEvent::Change) && !this.ext.lib.loading {
                        cx.notify(); // the Save chip follows
                    }
                }),
            );
            self.ext.lib.editor = Some(editor);
        }
    }

    /// Open the drawer on the library (the palette's "Browse Notes").
    pub(crate) fn ext_lib_show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ext_library().is_none() {
            return self.show_toast("No notes library is running", None, cx);
        }
        self.ext_lib_ensure(window, cx);
        self.ext.lib.tab = true;
        self.open_notes(window, cx);
        self.ext_lib_focus(window, cx);
        self.ext_lib_refresh(window, cx);
    }

    /// The drawer's tabs: the reading notes, or the library.
    pub(crate) fn ext_lib_tab(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if on {
            self.ext_lib_ensure(window, cx);
        }
        self.ext.lib.tab = on;
        if on {
            self.ext_lib_focus(window, cx);
            self.ext_lib_refresh(window, cx);
        } else {
            self.open_notes(window, cx);
        }
        cx.notify();
    }

    /// The keyboard to the open note's editor, else the search box.
    fn ext_lib_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lib = &self.ext.lib;
        if lib.note.is_some()
            && let Some(e) = lib.editor.clone()
        {
            e.update(cx, |s, cx| s.focus(window, cx));
        } else if let Some(s) = lib.search.clone() {
            s.update(cx, |s, cx| s.focus(window, cx));
        }
    }

    /// The drawer shows the library now.
    pub(crate) fn ext_lib_showing(&self) -> bool {
        self.ext.lib.tab && self.ext.host.is_some() && self.ext_library().is_some()
    }

    /// List the folder on screen again (and search again), when shown.
    pub(crate) fn ext_lib_refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ext.lib.tab {
            return;
        }
        let path = self.ext.lib.path.clone();
        self.ext_lib_list(path, window, cx);
        if !self.ext.lib.query.trim().is_empty() {
            let q = self.ext.lib.query.clone();
            self.ext_lib_search(q, window, cx);
        }
    }

    /// Show folder `path`.
    pub(crate) fn ext_lib_list(
        &mut self,
        path: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((host, lib)) = self.ext_lib_host() else {
            return;
        };
        let p = LibraryListParams {
            library: Some(lib.library.id.clone()),
            path: path.clone(),
        };
        let e = lib.ext.clone();
        let task = cx.background_spawn(async move { host.library_list(&e, &p) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, _, cx| {
                match r {
                    Ok(entries) => {
                        v.ext.lib.path = path;
                        v.ext.lib.entries = entries;
                        if v.ext
                            .lib
                            .status
                            .as_deref()
                            .is_some_and(|s| s.contains("answer"))
                        {
                            v.ext.lib.status = None;
                        }
                    }
                    Err(e) => v.ext.lib.status = Some(failure(&lib, &e)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Search as the user types: each keystroke asks again, and only the
    /// newest answer is shown.
    fn ext_lib_search(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        self.ext.lib.query = query.clone();
        self.ext.lib.search_gen += 1;
        let gen_ = self.ext.lib.search_gen;
        if query.trim().is_empty() {
            self.ext.lib.hits = None;
            cx.notify();
            return;
        }
        let Some((host, lib)) = self.ext_lib_host() else {
            return;
        };
        let p = LibrarySearchParams {
            library: Some(lib.library.id.clone()),
            query,
            limit: SEARCH_LIMIT,
        };
        let e = lib.ext.clone();
        let task = cx.background_spawn(async move { host.library_search(&e, &p) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, _, cx| {
                if v.ext.lib.search_gen != gen_ {
                    return; // a newer search is on its way
                }
                match r {
                    Ok(hits) => {
                        v.ext.lib.hits = Some(hits);
                        v.ext.lib.status = None;
                    }
                    Err(e) => v.ext.lib.status = Some(failure(&lib, &e)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Open note `id` in the panel's editor.
    pub(crate) fn ext_lib_open(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some((host, lib)) = self.ext_lib_host() else {
            return;
        };
        self.ext_lib_ensure(window, cx);
        let p = LibraryReadParams {
            library: Some(lib.library.id.clone()),
            id,
        };
        let e = lib.ext.clone();
        let task = cx.background_spawn(async move { host.library_read(&e, &p) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                match r {
                    Ok(doc) => v.ext_lib_load(doc, window, cx),
                    Err(e) => v.ext.lib.status = Some(failure(&lib, &e)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Show `doc` in the editor (its body; the frontmatter is kept aside).
    fn ext_lib_load(&mut self, doc: LibraryDocument, window: &mut Window, cx: &mut Context<Self>) {
        let folder = doc.id.rsplit_once('/').map(|(f, _)| f.to_string());
        self.ext.lib.note = Some(OpenNote {
            id: Some(doc.id),
            title: doc.title,
            markdown: doc.markdown,
            hash: doc.hash,
            saved_body: doc.body.clone(),
            stale: false,
            folder,
        });
        self.ext.lib.status = None;
        self.ext_lib_set_text(doc.body, window, cx);
        self.ext_lib_focus(window, cx);
    }

    fn ext_lib_set_text(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.ext.lib.editor.clone() {
            self.ext.lib.loading = true;
            e.update(cx, |s, cx| {
                s.set_value(text, window, cx);
                s.set_selected_range(0..0, cx);
            });
            self.ext.lib.loading = false;
        }
    }

    /// "New note": an empty note in the folder on screen, saved when the
    /// user saves it (named from its first line).
    pub(crate) fn ext_lib_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ext_lib_ensure(window, cx);
        self.ext.lib.note = Some(OpenNote {
            id: None,
            title: "New note".into(),
            markdown: String::new(),
            hash: String::new(),
            saved_body: String::new(),
            stale: false,
            folder: self.ext.lib.path.clone(),
        });
        self.ext.lib.status = None;
        self.ext_lib_set_text(String::new(), window, cx);
        self.ext_lib_focus(window, cx);
        cx.notify();
    }

    /// Back to the folder (an edit is saved first).
    pub(crate) fn ext_lib_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keep = self.ext.lib.note.as_ref().is_some_and(|n| n.stale);
        if self.ext.lib.dirty(cx) && !keep {
            self.ext_lib_save(window, cx);
        }
        if !keep || !self.ext.lib.dirty(cx) {
            self.ext.lib.note = None;
        }
        self.ext_lib_refresh(window, cx);
        self.ext_lib_focus(window, cx);
        cx.notify();
    }

    /// Save the open note: its body, the frontmatter as it was, refused if
    /// the file changed since it was read.
    pub(crate) fn ext_lib_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((host, lib)) = self.ext_lib_host() else {
            return;
        };
        let Some(note) = self.ext.lib.note.as_ref() else {
            return;
        };
        let body = self
            .ext
            .lib
            .editor
            .as_ref()
            .map(|e| e.read(cx).value().to_string())
            .unwrap_or_default();
        if note.id.is_none() && body.trim().is_empty() {
            return;
        }
        let p = LibraryWriteParams {
            library: Some(lib.library.id.clone()),
            id: note.id.clone(),
            title: None,
            folder: note.id.is_none().then(|| note.folder.clone()).flatten(),
            markdown: blyg_ext_notes::frontmatter::with_body(&note.markdown, &body),
            base_hash: note.id.as_ref().map(|_| note.hash.clone()),
        };
        self.ext_lib_write(host, lib, p, body, window, cx);
    }

    /// Write, then read back what's on disk now.
    fn ext_lib_write(
        &mut self,
        host: Host,
        lib: LibraryRef,
        p: LibraryWriteParams,
        body: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let e = lib.ext.clone();
        let lib_id = lib.library.id.clone();
        let task = cx.background_spawn(async move {
            let w = host.library_write(&e, &p)?;
            host.library_read(
                &e,
                &LibraryReadParams {
                    library: Some(lib_id),
                    id: w.id,
                },
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                match r {
                    Ok(doc) => {
                        let id = doc.id.clone();
                        let still = v
                            .ext
                            .lib
                            .editor
                            .as_ref()
                            .map(|e| e.read(cx).value().to_string())
                            == Some(body.clone());
                        let folder = id.rsplit_once('/').map(|(f, _)| f.to_string());
                        let title = doc.title.clone();
                        v.ext.lib.note = Some(OpenNote {
                            id: Some(id),
                            title: doc.title,
                            markdown: doc.markdown,
                            hash: doc.hash,
                            saved_body: if still { doc.body } else { body },
                            stale: false,
                            folder,
                        });
                        v.ext.lib.status = Some(format!("Saved “{title}”"));
                        v.ext_lib_refresh(window, cx);
                    }
                    Err(ExtError::Stale { .. }) => {
                        if let Some(n) = v.ext.lib.note.as_mut() {
                            n.stale = true;
                        }
                        v.ext.lib.status = Some("Changed on disk since you opened it".into());
                    }
                    Err(e) => v.ext.lib.status = Some(failure(&lib, &e)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// After "changed on disk": read it again (the edit is dropped).
    pub(crate) fn ext_lib_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.ext.lib.note.as_ref().and_then(|n| n.id.clone()) {
            self.ext_lib_open(id, window, cx);
        }
    }

    /// After "changed on disk": keep the edit as a new note beside it.
    pub(crate) fn ext_lib_save_as_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((host, lib)) = self.ext_lib_host() else {
            return;
        };
        let Some(note) = self.ext.lib.note.as_ref() else {
            return;
        };
        let body = self
            .ext
            .lib
            .editor
            .as_ref()
            .map(|e| e.read(cx).value().to_string())
            .unwrap_or_default();
        let p = LibraryWriteParams {
            library: Some(lib.library.id.clone()),
            id: None,
            title: Some(format!("{} (my edit)", note.title)),
            folder: note.folder.clone(),
            markdown: body.clone(),
            base_hash: None,
        };
        self.ext_lib_write(host, lib, p, body, window, cx);
    }

    /// The text selected in the open note, when the panel is showing.
    pub(crate) fn ext_lib_selection(&self, cx: &App) -> Option<String> {
        if !(self.notes.open && self.ext.lib.tab && self.ext.lib.note.is_some()) {
            return None;
        }
        let s = self.ext.lib.editor.as_ref()?.read(cx);
        let r = s.selected_range();
        let t = s.value().get(r)?.to_string();
        (!t.trim().is_empty()).then_some(t)
    }

    /// "Copy into post": the selection, else the whole note, into the
    /// draft at its caret, as a quote naming the note or verbatim.
    pub(crate) fn ext_lib_copy(
        &mut self,
        quote: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(note) = self.ext.lib.note.as_ref() else {
            return;
        };
        let title = note.title.clone();
        let text = self.ext_lib_selection(cx).unwrap_or_else(|| {
            self.ext
                .lib
                .editor
                .as_ref()
                .map(|e| e.read(cx).value().to_string())
                .unwrap_or_default()
        });
        let block = copy_block(&text, &title, quote);
        let from = format!("from {title}");
        self.ext_insert_into_draft(block, &from, window, cx);
    }

    // ------------------------------------------------------------ quote picker

    /// The quote picker's "notes" chip on or off.
    pub(crate) fn ext_picker_toggle(&mut self, query: String, cx: &mut Context<Self>) {
        self.ext.lib.picker = !self.ext.lib.picker;
        self.ext_picker_select(0);
        if self.ext.lib.picker {
            self.ext_picker_search(query, cx);
        }
        cx.notify();
    }

    /// The picker's notes are showing (and a library is running).
    pub(crate) fn ext_picker_on(&self) -> bool {
        self.ext.lib.picker && self.ext_library().is_some()
    }

    /// Search the notes for the picker's box (only the newest answer shows).
    pub(crate) fn ext_picker_search(&mut self, query: String, cx: &mut Context<Self>) {
        if !self.ext.lib.picker {
            return;
        }
        let Some((host, lib)) = self.ext_lib_host() else {
            return;
        };
        self.ext.lib.picker_gen += 1;
        let gen_ = self.ext.lib.picker_gen;
        let p = LibrarySearchParams {
            library: Some(lib.library.id.clone()),
            query,
            limit: SEARCH_LIMIT,
        };
        let e = lib.ext.clone();
        let task = cx.background_spawn(async move { host.library_search(&e, &p) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                if v.ext.lib.picker_gen != gen_ {
                    return;
                }
                match r {
                    Ok(h) => {
                        v.ext.lib.picker_hits = h;
                        v.ext.lib.picker_status = None;
                    }
                    Err(e) => {
                        v.ext.lib.picker_hits.clear();
                        v.ext.lib.picker_status = Some(failure(&lib, &e));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Quote picked note `i` into `target` (a blockquote naming the note,
    /// never a `![[…]]`).
    pub(crate) fn ext_picker_pick(
        &mut self,
        i: usize,
        target: blyg_core::LocalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((host, lib)) = self.ext_lib_host() else {
            return;
        };
        let Some(hit) = self.ext.lib.picker_hits.get(i).cloned() else {
            return;
        };
        let p = LibraryReadParams {
            library: Some(lib.library.id.clone()),
            id: hit.id,
        };
        let e = lib.ext.clone();
        let task = cx.background_spawn(async move { host.library_read(&e, &p) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                let doc = match r {
                    Ok(d) => d,
                    Err(e) => return v.show_toast(failure(&lib, &e), None, cx),
                };
                if v.current.as_ref().map(|c| &c.local_id) != Some(&target) {
                    v.open(&target, window, cx);
                }
                v.mode = crate::app::Mode::Edit;
                v.editor.update(cx, |s, cx| s.focus(window, cx));
                let (text, cursor) = {
                    let s = v.editor.read(cx);
                    (s.value().to_string(), s.cursor())
                };
                let block = copy_block(&doc.body, &doc.title, true);
                let (new_text, caret) = crate::app::reading::insert_block(&text, cursor, &block);
                v.splice_editor(&text, &new_text, Some(caret), window, cx);
                v.show_toast(
                    format!("Quoted “{}”", doc.title),
                    Some(lib.library.title.clone().into()),
                    cx,
                );
            });
        })
        .detach();
    }

    /// The picker's "notes" chip (quotes only), when a library is running.
    pub(crate) fn ext_picker_chip(
        &self,
        query: String,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let lib = self.ext_library()?;
        let p = self.palette.on_page();
        let on = self.ext.lib.picker;
        Some(
            div()
                .id("pick-src-notes")
                .debug_selector(|| "pick-src-notes".into())
                .px(px(7.))
                .py(px(1.))
                .rounded(px(5.))
                .border_1()
                .border_color(if on { p.accent } else { p.edge() })
                .when(on, |d| d.text_color(p.accent_text()))
                .text_size(px(11.))
                .cursor_pointer()
                .hover(|s| s.bg(p.hover()))
                .child(lib.library.title.to_lowercase())
                .on_click(
                    cx.listener(move |this, _, _, cx| this.ext_picker_toggle(query.clone(), cx)),
                )
                .into_any_element(),
        )
    }

    /// The picker's rows while its notes are showing.
    pub(crate) fn ext_picker_rows(&self, sel: usize, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.palette.on_page();
        let now = chrono::Utc::now();
        if let Some(s) = &self.ext.lib.picker_status {
            return vec![
                div()
                    .p(px(8.))
                    .italic()
                    .text_color(p.muted)
                    .child(s.clone())
                    .into_any_element(),
            ];
        }
        if self.ext.lib.picker_hits.is_empty() {
            return vec![
                div()
                    .p(px(8.))
                    .italic()
                    .text_color(p.muted)
                    .child("No note matches.")
                    .into_any_element(),
            ];
        }
        self.ext
            .lib
            .picker_hits
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let when =
                    h.at.as_deref()
                        .map(|t| crate::vm::relative_time(t, now))
                        .unwrap_or_default();
                div()
                    .id(("pick-note", i))
                    .debug_selector(move || format!("pick-note-{i}"))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .when(i == sel, |d| d.bg(p.pick()))
                    .when(i != sel, |d| d.hover(|s| s.bg(p.hover())))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.ext_picker_pick_row(i, window, cx);
                    }))
                    .child(div().truncate().child(h.title.clone()))
                    .when(!h.excerpt.trim().is_empty(), |d| {
                        d.child(
                            div()
                                .text_size(px(11.5))
                                .text_color(p.muted)
                                .truncate()
                                .child(h.excerpt.clone()),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(p.muted)
                            .truncate()
                            .child(format!("note · quoted as text · {when}")),
                    )
                    .into_any_element()
            })
            .collect()
    }

    // ------------------------------------------------------------ the drawer

    /// Hook (the drawer's top): "Reading notes | Notes" when a library runs.
    pub(crate) fn render_ext_tabs(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let lib = self.ext_library()?;
        let p = self.palette;
        let theme = &*self.theme;
        let on = self.ext.lib.tab;
        let tab = |id: &'static str, label: SharedString, active: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(9.))
                .h(px(22.))
                .flex()
                .items_center()
                .map(|d| crate::theme_ext::chip(d, theme, active))
                .when(active, |d| d.bg(p.pick()))
                .text_size(px(11.5))
                .cursor_pointer()
                .hover(|s| s.bg(p.hover()))
                .child(label)
        };
        Some(
            div()
                .flex_none()
                .px(px(14.))
                .pt(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    tab("notes-tab-reading", "Reading notes".into(), !on).on_click(
                        cx.listener(|this, _, window, cx| this.ext_lib_tab(false, window, cx)),
                    ),
                )
                .child(
                    tab("notes-tab-library", lib.library.title.clone().into(), on).on_click(
                        cx.listener(|this, _, window, cx| this.ext_lib_tab(true, window, cx)),
                    ),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(p.muted)
                        .child(lib.ext.clone()),
                )
                .into_any_element(),
        )
    }

    /// Hook: the drawer's header, body and footer while it shows the
    /// library (`None`: the reading notes).
    pub(crate) fn render_ext_library(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<(AnyElement, AnyElement, AnyElement)> {
        if !self.ext_lib_showing() {
            return None;
        }
        let lib = self.ext_library()?;
        let search = self.ext.lib.search.clone()?;
        let editor = self.ext.lib.editor.clone()?;
        let p = self.palette;
        let theme = &*self.theme;
        let chip = |id: &'static str, label: SharedString| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex_none()
                .px(px(7.))
                .h(px(22.))
                .flex()
                .items_center()
                .map(|d| crate::theme_ext::chip(d, theme, false))
                .text_size(px(11.5))
                .text_color(p.ink)
                .cursor_pointer()
                .hover(|s| s.bg(p.hover()))
                .child(label)
        };
        let status = self.ext.lib.status.clone();
        let lib_title = lib.library.title.clone();
        let footer_hint = |hint: &'static str| {
            div()
                .flex_none()
                .px(px(14.))
                .py(px(6.))
                .rule_t(&p)
                .text_size(px(11.))
                .text_color(p.muted)
                .child(hint)
                .into_any_element()
        };

        // ---------------------------------------------------- an open note
        if let Some(note) = &self.ext.lib.note {
            let dirty = self.ext.lib.dirty(cx);
            let stale = note.stale;
            let header = div()
                .flex_none()
                .px(px(14.))
                .pt(px(8.))
                .pb(px(8.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .rule_b(&p)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            chip("lib-back", "←".into())
                                .border_color(transparent_black())
                                .on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.ext_lib_back(window, cx)
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(note.title.clone()),
                        )
                        .when(dirty, |d| {
                            d.child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(p.muted)
                                    .child("edited"),
                            )
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            chip("lib-save", "Save".into())
                                .when(!dirty || stale, |d| d.opacity(0.5))
                                .on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.ext_lib_save(window, cx)
                                    }),
                                ),
                        )
                        .child(chip("lib-copy-quote", "Quote into post".into()).on_click(
                            cx.listener(|this, _, window, cx| this.ext_lib_copy(true, window, cx)),
                        ))
                        .child(chip("lib-copy-verbatim", "Copy verbatim".into()).on_click(
                            cx.listener(|this, _, window, cx| this.ext_lib_copy(false, window, cx)),
                        )),
                )
                .when(stale, |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(11.5))
                            .text_color(p.warn_text())
                            .child("Changed on disk since you opened it.")
                            .child(chip("lib-reload", "Reload".into()).on_click(
                                cx.listener(|this, _, window, cx| this.ext_lib_reload(window, cx)),
                            ))
                            .child(
                                chip("lib-save-new", "Keep mine as a new note".into()).on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.ext_lib_save_as_new(window, cx)
                                    }),
                                ),
                            ),
                    )
                })
                .when(!stale && status.is_some(), |d| {
                    d.child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(status.clone().unwrap_or_default()),
                    )
                })
                .into_any_element();
            let body = div()
                .id("lib-note-body")
                .flex_1()
                .min_h_0()
                .relative()
                .font_family(SharedString::from(self.prefs.writing().family))
                .text_size(px((self.prefs.font_size - 2.).max(12.)))
                .line_height(relative(1.5))
                .child(Textarea::new(&editor))
                .into_any_element();
            let footer =
                footer_hint("Copies the selection, or the whole note · frontmatter stays as it is");
            editor.update(cx, |s, _| {
                s.set_editor_style(gpui_kit::base::input::InputEditorStyle {
                    foreground: p.ink,
                    muted_foreground: p.placeholder(),
                    background: p.bg,
                    border: p.edge(),
                    selection: p.text_selection,
                    caret: p.caret(),
                    ..Default::default()
                });
                s.set_editor_paddings(Edges {
                    top: px(12.),
                    bottom: px(24.),
                    left: px(18.),
                    right: px(18.),
                });
            });
            return Some((header, body, footer));
        }

        // ---------------------------------------------------- browsing
        search.update(cx, |s, _| s.set_editor_style(p.field()));
        let mut crumbs: Vec<AnyElement> = vec![
            div()
                .id("lib-crumb-top")
                .cursor_pointer()
                .hover(|s| s.underline())
                .font_weight(FontWeight::SEMIBOLD)
                .child(lib_title.clone())
                .on_click(cx.listener(|this, _, window, cx| this.ext_lib_list(None, window, cx)))
                .into_any_element(),
        ];
        if let Some(path) = &self.ext.lib.path {
            let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            for (i, part) in parts.iter().enumerate() {
                let to = parts[..=i].join("/");
                crumbs.push(div().text_color(p.muted).child("›").into_any_element());
                crumbs.push(
                    div()
                        .id(("lib-crumb", i))
                        .cursor_pointer()
                        .hover(|s| s.underline())
                        .child(part.to_string())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.ext_lib_list(Some(to.clone()), window, cx)
                        }))
                        .into_any_element(),
                );
            }
        }
        let header =
            div()
                .flex_none()
                .px(px(14.))
                .pt(px(8.))
                .pb(px(8.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .rule_b(&p)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(13.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(px(5.))
                                .children(crumbs),
                        )
                        .child(chip("lib-new", "New note".into()).on_click(
                            cx.listener(|this, _, window, cx| this.ext_lib_new(window, cx)),
                        )),
                )
                .child(
                    crate::theme_ext::field_box(div(), theme)
                        .id("lib-search")
                        .px(px(9.))
                        .py(px(4.))
                        .text_size(px(12.5))
                        .child(Input::new(&search)),
                )
                .into_any_element();
        let now = chrono::Utc::now();
        let row = |id: ElementId, title: String, sub: Option<String>, dir: bool| {
            div()
                .id(id)
                .px(px(14.))
                .py(px(5.))
                .cursor_pointer()
                .hover(|s| s.bg(p.hover()))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(div().flex_1().min_w_0().truncate().child(title))
                        .when(dir, |d| d.child(div().text_color(p.muted).child("›"))),
                )
                .children(sub.filter(|s| !s.is_empty()).map(|s| {
                    div()
                        .text_size(px(11.))
                        .text_color(p.muted)
                        .truncate()
                        .child(s)
                }))
        };
        let rows: Vec<AnyElement> = match &self.ext.lib.hits {
            Some(hits) => hits
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    let id = h.id.clone();
                    row(
                        ElementId::NamedInteger("lib-hit".into(), i as u64),
                        h.title.clone(),
                        Some(h.excerpt.clone()),
                        false,
                    )
                    .debug_selector(move || format!("lib-hit-{i}"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.ext_lib_open(id.clone(), window, cx)
                    }))
                    .into_any_element()
                })
                .collect(),
            None => self
                .ext
                .lib
                .entries
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    let id = e.id.clone();
                    let dir = e.is_dir;
                    let when = e
                        .modified
                        .as_deref()
                        .filter(|_| !dir)
                        .map(|t| crate::vm::relative_time(t, now));
                    row(
                        ElementId::NamedInteger("lib-entry".into(), i as u64),
                        e.title.clone(),
                        when,
                        dir,
                    )
                    .debug_selector(move || format!("lib-entry-{i}"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if dir {
                            this.ext_lib_list(Some(id.clone()), window, cx)
                        } else {
                            this.ext_lib_open(id.clone(), window, cx)
                        }
                    }))
                    .into_any_element()
                })
                .collect(),
        };
        let empty = rows.is_empty();
        let searching = self.ext.lib.hits.is_some();
        let body = div()
            .id("lib-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .py(px(4.))
            .text_size(px(12.5))
            .children(rows)
            .when(empty, |d| {
                d.child(
                    div()
                        .px(px(14.))
                        .py(px(10.))
                        .italic()
                        .text_color(p.muted)
                        .child(if searching {
                            "No note matches."
                        } else {
                            "No notes here yet. New note makes one."
                        }),
                )
            })
            .into_any_element();
        let footer = match status {
            Some(s) => div()
                .flex_none()
                .px(px(14.))
                .py(px(6.))
                .rule_t(&p)
                .text_size(px(11.))
                .text_color(p.warn_text())
                .child(s)
                .into_any_element(),
            None => footer_hint("Type to search titles and text · a note opens to edit or copy"),
        };
        Some((header, body, footer))
    }

    /// A click on the picker's note row `i`.
    fn ext_picker_pick_row(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.ext_picker_select(i);
        self.pick_quote(window, cx);
    }

    /// The picker's highlighted note.
    pub(crate) fn ext_picker_select(&mut self, i: usize) {
        if let Some(crate::app::reading::RSheet::Quote { sel, .. }) = self.reading.sheet.as_mut() {
            *sel = i;
        }
    }
}

/// What "Copy into post" puts in the draft: the text quoted with the
/// note's name (`> …` then `> — Title`), or the text as it is.
pub fn copy_block(text: &str, title: &str, quote: bool) -> String {
    if quote {
        crate::app::notes::quote_with_source(text, title, None)
    } else {
        text.trim().to_string()
    }
}
