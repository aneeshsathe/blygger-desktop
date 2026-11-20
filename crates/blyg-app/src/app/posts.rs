//! The Posts screen's list and search: the omnibar's query (typed, set,
//! settled), re-reading the list after a change, and moving the selection.
//! The omnibar, list and rows are drawn in `posts_render.rs`.

use super::*;

impl MainView {
    pub(super) fn requery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settle_search(window, cx);
        let results = self.backend.search(self.list.query());
        self.requery_with(results, None, window, cx);
    }

    /// `requery` for a change the sync worker made (`ItemsChanged`): the
    /// search, and the open item's fresh copy, are read on a background
    /// thread. A search typed meanwhile is searched again here, since the
    /// one under way may predate the change.
    pub(super) fn requery_later(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.generation += 1;
        let generation = self.search.generation;
        let typed = self.search.pending.clone();
        let q = typed
            .clone()
            .unwrap_or_else(|| self.list.query().to_string());
        let current = self.current.as_ref().map(|c| c.local_id.clone());
        let edits = self.search.edits;
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let results = backend.search(&q);
            let fresh = current.map(|id| {
                let item = backend.item(&id);
                (id, item)
            });
            (q, results, fresh)
        });
        self.search.task = Some(cx.spawn_in(window, async move |this, cx| {
            let (q, results, mut fresh) = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                if v.search.generation != generation {
                    return;
                }
                v.search.pending = None;
                v.search.task = None;
                // Typed into since it was read: read it again.
                if v.search.edits != edits {
                    fresh = None;
                }
                if typed.is_some() {
                    v.apply_query(&q, results, window, cx);
                } else {
                    v.requery_with(results, fresh, window, cx);
                }
            });
        }));
    }

    /// The rest of `requery`, given the search's results and a fresh copy
    /// of the open item (`None` inside: it's gone). One read for another
    /// item than the one open now is read again.
    pub(super) fn requery_with(
        &mut self,
        results: Vec<Item>,
        fresh: Option<(LocalId, Option<Item>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reload_shelf(); // --- shelf ---
        self.upkeep_reload_stale(); // --- upkeep ---
        self.list.refresh(results);
        self.loose_ends_refresh(cx); // --- loose ends ---
        self.new_post_sync_selection(); // --- new post ---
        // Refresh the current item's metadata without clobbering the editor.
        if let Some(cur) = self.current.as_ref().map(|c| c.local_id.clone()) {
            let fresh = match fresh {
                Some((id, fresh)) if id == cur => fresh,
                _ => self.backend.item(&cur),
            };
            match fresh {
                Some(fresh) => {
                    let editor_text = self.editor.read(cx).value();
                    if fresh.content_md != editor_text.as_ref() && self.mode == Mode::Search {
                        self.current = Some(fresh);
                        self.load_current_into_editor(window, cx);
                    } else {
                        self.current = Some(fresh);
                    }
                }
                None => self.load_selected(window, cx),
            }
        } else if self.mode == Mode::Search {
            self.load_selected(window, cx);
        }
        cx.notify();
    }

    pub(super) fn on_omni_event(
        &mut self,
        _: &Entity<InputState>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Change => self.query_typed(window, cx),
            InputEvent::Focus => {
                self.mode = Mode::Search;
                cx.notify();
            }
            InputEvent::PressEnter {
                secondary: true, ..
            } => {
                self.settle_search(window, cx);
                self.publish(window, cx)
            }
            InputEvent::PressEnter { .. } => match self.settle_search(window, cx).list.enter() {
                EnterAction::Open(id) => self.open(&id, window, cx),
                EnterAction::Create(seed) => self.create_from(&seed, window, cx),
                EnterAction::LinkFragment(url) => self.capture_link(url, window, cx), // --- capture ---
                // Not connected, nothing here, nothing typed: Enter connects
                // (the empty list says so).
                EnterAction::Nothing if self.offers_connect(cx) => self.open_connect(window, cx),
                EnterAction::Nothing => {}
            },
            InputEvent::Blur => {}
        }
    }

    pub(super) fn move_selection(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // ↑/↓ move through what was typed, even before its search is back.
        self.settle_search(window, cx);
        // --- new post --- ↑/↓ go on from the row selected before ⌘N.
        let resumed = self.new_post_resume_selection();
        if self.list.move_selection(delta).is_some() || resumed {
            if let Some(ix) = self.list.selected_index() {
                self.list_scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
            }
            self.load_selected(window, cx);
        }
    }

    pub(super) fn focus_search(
        &mut self,
        _: &FocusSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.leave_reading(); // --- reading & versions ---
        self.back_to_search(window, cx);
    }

    pub(super) fn back_to_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = Mode::Search;
        self.omni.update(cx, |s, cx| {
            s.focus(window, cx);
            s.select_all(window, cx);
        });
        cx.notify();
    }

    /// Set the omnibar text programmatically (set_value emits no event).
    pub(super) fn set_query_text(&mut self, q: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.omni
            .update(cx, |s, cx| s.set_value(q.to_string(), window, cx));
        self.query_changed(window, cx);
    }

    /// The omnibar's text was set: search now, on this thread.
    pub(super) fn query_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let q = self.omni.read(cx).value().to_string();
        let typing = self.search.pending.take().is_some();
        if typing {
            // Whatever was typed before is superseded.
            self.search.generation += 1;
            self.search.task = None;
        }
        if q == self.list.query() && !typing {
            return;
        }
        let results = self.backend.search(&q);
        self.apply_query(&q, results, window, cx);
    }

    /// A keystroke in the omnibar: the search runs on a background thread
    /// (searching thousands of posts costs frames), and only the newest
    /// keystroke's results are listed. Until they're back the list shows
    /// the previous query's; ⏎, ↑/↓ and focusing the editor finish the
    /// search first (`settle_search`), so they act on what was typed.
    pub(super) fn query_typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let q = self.omni.read(cx).value().to_string();
        let waiting = self.search.pending.as_deref();
        if waiting == Some(q.as_str()) || (waiting.is_none() && q == self.list.query()) {
            return;
        }
        self.search.generation += 1;
        let generation = self.search.generation;
        self.search.pending = Some(q.clone());
        self.mode = Mode::Search;
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let results = backend.search(&q);
            (q, results)
        });
        self.search.task = Some(cx.spawn_in(window, async move |this, cx| {
            let (q, results) = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                if v.search.generation == generation {
                    v.search.pending = None;
                    v.search.task = None;
                    v.apply_query(&q, results, window, cx);
                }
            });
        }));
        cx.notify();
    }

    /// A typed search still on its way: finish it now, on this thread.
    /// Returns `self`, for whatever reads the list next.
    pub(super) fn settle_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> &mut Self {
        if let Some(q) = self.search.pending.take() {
            self.search.generation += 1;
            self.search.task = None;
            let results = self.backend.search(&q);
            self.apply_query(&q, results, window, cx);
        }
        self
    }

    /// List `results` for the query `q`: the selection jumps to the first.
    pub(super) fn apply_query(
        &mut self,
        q: &str,
        results: Vec<Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.list.set_query(q, results);
        self.list_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.mode = Mode::Search;
        self.load_selected(window, cx);
    }
}
