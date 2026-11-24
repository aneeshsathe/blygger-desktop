//! The writing side of the main window: loading a post into the editor,
//! the editor's events and saves, ⌘N, ⌘T and ⌘E, and the editor pane.
//! Publishing and conflicts are in `publish.rs`; pasted links and images in
//! `upload.rs`.

use super::*;

impl MainView {
    pub(super) fn set_editor_text(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.loading_editor = true;
        self.editor
            .update(cx, |s, cx| s.set_value(text.to_string(), window, cx));
        self.loading_editor = false;
        self.assist.update(cx, |a, cx| a.reset(cx)); // --- composer ---
    }

    /// Load the selected row into the editor (NV preview-as-you-move).
    pub(super) fn load_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.leave_new_post(window, cx); // --- new post ---
        let q = self.list.trimmed_query().to_string();
        if self.list.wants_create() {
            self.current = None;
            self.set_editor_text("", window, cx);
            let ph = crate::keymap::hint_owned(format!(
                "⏎ creates a new {}: “{q}”",
                scratch::new_note_noun(self.prefs.new_note)
            ));
            self.editor
                .update(cx, |s, cx| s.set_placeholder(ph, window, cx));
        } else {
            self.current = self.list.selected_item().cloned();
            let text = self
                .current
                .as_ref()
                .map(|i| i.content_md.clone())
                .unwrap_or_default();
            self.set_editor_text(&text, window, cx);
            self.editor
                .update(cx, |s, cx| s.set_placeholder("", window, cx));
        }
        self.refresh_preview(true, cx);
        cx.notify();
    }

    /// Re-render the preview (after a typing pause unless `now`).
    pub(super) fn refresh_preview(&mut self, now: bool, cx: &mut Context<Self>) {
        self.studio_refresh(now, cx);
    }

    pub(super) fn load_current_into_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self
            .current
            .as_ref()
            .map(|i| i.content_md.clone())
            .unwrap_or_default();
        self.set_editor_text(&text, window, cx);
        self.refresh_preview(true, cx);
    }

    pub(super) fn open(&mut self, id: &LocalId, window: &mut Window, cx: &mut Context<Self>) {
        // A search typed just before lands first, not over the open post.
        self.settle_search(window, cx);
        // --- new post --- opening another post leaves the new one.
        if self
            .new_post
            .as_ref()
            .is_some_and(|np| np.id.as_ref() != Some(id))
        {
            self.leave_new_post(window, cx);
        }
        self.list.select(id);
        if self.current.as_ref().map(|c| &c.local_id) != Some(id) {
            self.current = self.backend.item(id);
            self.load_current_into_editor(window, cx);
        }
        self.mode = Mode::Edit;
        self.editor.update(cx, |s, cx| {
            s.focus(window, cx);
            let end = s.text().len();
            s.set_selected_range(end..end, cx);
        });
        cx.notify();
    }

    pub(super) fn create_from(&mut self, seed: &str, window: &mut Window, cx: &mut Context<Self>) {
        match scratch::create_note(&*self.backend, self.prefs.new_note, seed) {
            Ok((id, created)) => {
                self.set_query_text("", window, cx);
                let results = self.backend.search("");
                self.list.set_query("", results);
                self.list.select(&id);
                self.current = None;
                self.open(&id, window, cx);
                self.list_scroll.scroll_to_item(0, ScrollStrategy::Top);
                self.show_toast(created, None, cx);
            }
            Err(e) => self.show_toast(
                format!(
                    "Couldn't create a {}: {e}",
                    scratch::new_note_noun(self.prefs.new_note)
                ),
                None,
                cx,
            ),
        }
    }

    pub(super) fn on_editor_event(
        &mut self,
        _: &Entity<TextareaState>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Focus => {
                if self.mode == Mode::Search && self.settle_search(window, cx).list.wants_create() {
                    let seed = self.list.trimmed_query().to_string();
                    self.create_from(&seed, window, cx);
                } else {
                    self.mode = Mode::Edit;
                    cx.notify();
                }
            }
            InputEvent::Change if !self.loading_editor => {
                // --- new post --- the first keystroke creates the note.
                self.new_post_first_keystroke(cx);
                // Typing `![[` at the start of a line opens the quote picker,
                // and `[[` the link picker.
                let typed = self.typed_picker(cx);
                self.after_edit(cx);
                self.nearby_typed(cx); // --- nearby ---
                if let Some((kind, typed)) = typed {
                    self.open_picker_from_typing(kind, typed, window, cx);
                }
            }
            _ => {}
        }
    }

    /// ⌘N: a fresh, empty editor (`new_post.rs`). The omnibar and the list
    /// are left as they are.
    pub(super) fn new_draft(&mut self, _: &NewDraft, window: &mut Window, cx: &mut Context<Self>) {
        self.start_new_post(window, cx);
    }

    pub(super) fn toggle_kind(
        &mut self,
        _: &ToggleKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_kind_now(window, cx);
    }

    pub(super) fn toggle_kind_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.current.clone() else {
            return;
        };
        let kind = match item.kind {
            Kind::Fragment => Kind::Thread,
            Kind::Thread => Kind::Fragment,
        };
        match self.backend.set_kind(&item.local_id, kind) {
            Ok(()) => {
                self.requery(window, cx);
                self.show_toast(
                    match kind {
                        Kind::Thread => {
                            "Now a thread: no length limit, and it can quote other blygs"
                        }
                        Kind::Fragment => "Now a fragment: 1000 characters max",
                    },
                    None,
                    cx,
                );
            }
            Err(e) => self.show_toast(format!("Couldn't change kind: {e}"), None, cx),
        }
    }

    pub(super) fn toggle_preview(
        &mut self,
        _: &TogglePreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_preview_now(window, cx);
    }

    /// ⌘E: the preview on or off in place; the editor keeps the keyboard.
    pub(super) fn toggle_preview_now(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.studio_set_view(self.studio.view.toggle_preview(), cx);
    }

    pub(super) fn open_permalink(
        &mut self,
        _: &OpenPermalink,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.current.as_ref().and_then(|i| i.permalink.clone()) {
            Some(url) => {
                cx.open_url(&url);
                self.show_toast("Opening in your browser…", None, cx);
            }
            None => self.show_toast("Not published yet", None, cx),
        }
    }

    /// Apply `old -> new` to the editor as a minimal, undoable replacement,
    /// keeping scroll. The caret goes to `caret` if given, else stays put
    /// (shifted if the edit was before it). Then saves.
    pub(super) fn splice_editor(
        &mut self,
        old: &str,
        new: &str,
        caret: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (range, insert) = vm::splice(old, new);
        let prev_cursor = self.editor.read(cx).cursor();
        self.editor.update(cx, |s, cx| {
            s.set_selected_range(range.clone(), cx);
            s.replace(insert.to_string(), window, cx);
            let c = caret.unwrap_or_else(|| vm::shift_cursor(prev_cursor, &range, insert.len()));
            let c = c.min(s.text().len());
            s.set_selected_range(c..c, cx);
        });
        self.after_edit(cx);
    }

    /// Save the editor's text to the backend and refresh derived state.
    pub(super) fn after_edit(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|c| c.local_id.clone()) else {
            return;
        };
        // --- discard changes --- the blyg is replacing this text: saving
        // it now could put the old text back after the restore.
        if self.restoring.as_ref() == Some(&id) {
            self.refresh_preview(false, cx);
            cx.notify();
            return;
        }
        let text = self.editor.read(cx).value();
        self.search.edits += 1;
        let saved = self.ai_save_edit(&id, &text);
        if let Err(e) = &saved {
            self.show_toast(format!("Couldn't save: {e}"), None, cx);
        }
        // The row as the save leaves it, worked out here rather than read
        // back from the store: a keystroke never waits on SQLite.
        let fresh = match (&saved, self.current.as_ref()) {
            (Ok(()), Some(cur)) if cur.local_id == id => {
                Some(blyg_core::store::saved_locally(cur, &text))
            }
            _ => self.backend.item(&id),
        };
        if let Some(fresh) = fresh {
            self.list.update_item(fresh.clone());
            self.current = Some(fresh);
        }
        self.refresh_preview(false, cx);
        cx.notify();
    }

    pub(super) fn render_editor_pane(
        &self,
        body_font: &SharedString,
        size: f32,
        side_pad: Pixels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // --- themes --- a lit harbour (the editor inset in a sea), the
        // margins' ornaments, and the divider band under the text.
        let harbour = crate::ornament::harbour(&self.theme);
        let inset = if harbour.is_some() { 10. } else { 0. };
        let margins = crate::ornament::editor_margins(&self.theme, f32::from(side_pad) - inset);
        let band = crate::ornament::divider(&self.theme);
        let p = self.palette;
        let radius = self.theme.radius;
        let paper = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(p.editor)
            .when_some(harbour, |d, (_, glow)| {
                d.rounded(px(radius)).overflow_hidden().shadow(vec![
                    BoxShadow {
                        color: p.line,
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        spread_radius: px(1.),
                        inset: false,
                    },
                    BoxShadow {
                        color: glow.opacity(0.28),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(30.),
                        spread_radius: px(0.),
                        inset: false,
                    },
                ])
            })
            // --- own posts --- Reply, Quote, Fork, Link on your published post.
            .children(self.render_own_post_bar(cx))
            // --- stub quotes --- a stub's hint line and passage chooser.
            .children(self.render_stub_bar(cx))
            .child(
                // --- nearby --- the rail sits right of the text.
                div().flex_1().min_h_0().flex().child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .relative()
                        .child(gpui_kit::base::input::Textarea::new(&self.editor))
                        .children(margins)
                        .child(self.assist.clone()) // --- composer ---
                        .map(|d| self.hover_card_editor(d, cx)) // --- hover cards ---
                        .children(self.render_focus_fade(cx)) // --- focus ---
                        .children(self.render_serendipity(side_pad, cx)), // --- serendipity ---
                ), // --- nearby --- (the rail is a side drawer now: `side.rs`)
            )
            .children(self.render_lineage_strip(cx)) // --- lineage ---
            .children(band);
        div()
            .id("editor-pane")
            .debug_selector(|| "editor-pane".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .relative()
            .when_some(harbour, |d, (sea, _)| d.bg(sea).p(px(inset)))
            .font_family(body_font.clone())
            .text_size(px(size))
            .line_height(relative(1.6))
            // --- composer --- the mention popup / spelling menu keys come first.
            .map(|d| self.composer_keys(d, cx))
            .map(|d| self.serendipity_keys(d, cx)) // --- serendipity ---
            .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                if !this.sheet.is_open() {
                    cx.stop_propagation();
                    // --- focus --- esc leaves focus mode first.
                    if this.focus_escape(window, cx) {
                        return;
                    }
                    // --- AI --- esc cancels a running generation first.
                    if !this.ai_escape(window, cx) {
                        this.back_to_search(window, cx);
                    }
                }
            }))
            .capture_action(cx.listener(|this, a: &Enter, window, cx| {
                if a.secondary {
                    cx.stop_propagation();
                    this.publish(window, cx);
                }
            }))
            .capture_action(cx.listener(|this, _: &Paste, window, cx| {
                this.assist.update(cx, |a, _| a.note_paste()); // --- composer ---
                if this.paste_link(window, cx) || this.paste_image(window, cx) {
                    cx.stop_propagation();
                }
            }))
            .map(|d| self.editor_quote_drop(d, cx)) // --- tray ---
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                let imgs: Vec<_> = paths
                    .paths()
                    .iter()
                    .filter(|p| vm::mime_for_path(p).is_some())
                    .cloned()
                    .collect();
                if !imgs.is_empty() {
                    this.upload_paths(imgs, window, cx);
                }
            }))
            .child(paper)
    }

    // --- composer ---
    /// While the mention popup or the spelling menu is up, ↑/↓/⏎/⇥/esc are
    /// theirs (checked before the editor's own and the pane's esc).
    pub(super) fn composer_keys(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        use crate::composer::AssistKey;
        use gpui_kit::base::input::IndentInline;
        fn route(
            this: &mut MainView,
            key: AssistKey,
            window: &mut Window,
            cx: &mut Context<MainView>,
        ) {
            if this.assist.read(cx).wants_keys()
                && this
                    .assist
                    .update(cx, |a, cx| a.handle_key(key, window, cx))
            {
                cx.stop_propagation();
            }
        }
        d.capture_action(
            cx.listener(|this, _: &MoveUp, window, cx| route(this, AssistKey::Up, window, cx)),
        )
        .capture_action(
            cx.listener(|this, _: &MoveDown, window, cx| route(this, AssistKey::Down, window, cx)),
        )
        .capture_action(cx.listener(|this, a: &Enter, window, cx| {
            if !a.secondary {
                route(this, AssistKey::Enter, window, cx)
            }
        }))
        .capture_action(
            cx.listener(|this, _: &IndentInline, window, cx| {
                route(this, AssistKey::Tab, window, cx)
            }),
        )
        .capture_action(
            cx.listener(|this, _: &Escape, window, cx| route(this, AssistKey::Escape, window, cx)),
        )
    }
}
