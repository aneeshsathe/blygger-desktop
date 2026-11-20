//! Publishing the open post (⌘⏎, the publish and held-labels sheets), and
//! conflicts: the resolve sheet and the dev-only offline and conflict toggles.

use super::*;

impl MainView {
    pub(super) fn on_publish(&mut self, _: &Publish, window: &mut Window, cx: &mut Context<Self>) {
        // Under a sheet, ⌘⏎ is the sheet's own (`SheetView::submit`): it
        // never publishes.
        if !self.sheet.is_open() {
            self.publish(window, cx);
        } else if let Some(submit) = self.sheet.submit() {
            submit(self, window, cx);
        }
    }

    pub(super) fn publish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.current.clone() else {
            return;
        };
        // --- AI --- warn before publishing generated text undisclosed.
        if self.ai_publish_guard(&item, window, cx) {
            return;
        }
        match vm::publish_decision(&item) {
            PublishDecision::Shake => {
                self.shake_gen += 1;
                self.show_toast(
                    crate::keymap::hint(
                        "Too long to publish as a fragment. Press ⌘T to make it a thread.",
                    ),
                    None,
                    cx,
                );
            }
            PublishDecision::Empty => self.show_toast("Nothing to publish yet", None, cx),
            PublishDecision::AlreadyPublished(v) => {
                self.show_toast(format!("Already published as v{v}"), None, cx)
            }
            // Below studio 0.47 a post with labelled links can't go as it is.
            PublishDecision::Sheet { .. } if self.labels_held(&item.content_md) => {
                let focus = cx.focus_handle();
                window.focus(&focus, cx);
                self.sheet.open(HeldLabelsSheet {
                    id: item.local_id,
                    focus,
                });
                cx.notify();
            }
            PublishDecision::Sheet {
                title,
                next_version,
            } => {
                let note = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder("Version note (optional), e.g. “fixed a typo”")
                });
                let sub = cx.subscribe_in(&note, window, |this, note, ev, window, cx| {
                    if let InputEvent::PressEnter { .. } = ev {
                        let text = note.read(cx).value().to_string();
                        this.do_publish(text, window, cx);
                    }
                });
                self._subs.push(sub);
                note.update(cx, |s, cx| s.focus(window, cx));
                self.sheet.open(PublishSheet {
                    id: item.local_id,
                    title,
                    next_version,
                    note,
                    ai: Default::default(), // --- access-ai ---
                });
                self.note_ai_start(next_version, window, cx); // --- access-ai ---
                cx.notify();
            }
        }
    }

    /// The blyg is below studio 0.47 and `text` has labelled links: it
    /// waits (`blyg_core::floor`).
    pub(crate) fn labels_held(&self, text: &str) -> bool {
        self.backend.server_outdated() && blyg_core::floor::has_labels(text)
    }

    /// The held-labels sheet's `⏎`: write the open post's labelled links
    /// bare (`[[id]]`, `![[id]]`), then publish it.
    pub(super) fn publish_without_labels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(HeldLabelsSheet { id, .. }) = self.sheet.take() else {
            return;
        };
        if self.current.as_ref().map(|c| &c.local_id) == Some(&id) {
            let text = self.editor.read(cx).value().to_string();
            let bare = blyg_core::floor::strip_labels(&text);
            if bare != text {
                self.splice_editor(&text, &bare, None, window, cx);
            }
        }
        self.focus_after_sheet(window, cx);
        self.start_publish(id, None, window, cx);
    }

    pub(super) fn do_publish(&mut self, note: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(PublishSheet { id, ai, .. }) = self.sheet.take() else {
            return;
        };
        self.focus_after_sheet(window, cx);
        let extras = ai.extras(&note); // --- access-ai ---
        let note_opt = Some(note.trim().to_string()).filter(|n| !n.is_empty());
        self.start_publish_with(id, note_opt, extras, window, cx);
    }

    pub(super) fn start_publish(
        &mut self,
        id: LocalId,
        note_opt: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_publish_with(id, note_opt, Default::default(), window, cx);
    }

    /// --- access-ai --- `start_publish` with the note's provenance and the
    /// confirmed change category.
    pub(super) fn start_publish_with(
        &mut self,
        id: LocalId,
        note_opt: Option<String>,
        extras: blyg_core::access_ai::PublishExtras,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.publishing = true;
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn({
            let id = id.clone();
            let note = note_opt.clone();
            async move {
                if extras == Default::default() {
                    scratch::publish_via(&*backend, &id, note.as_deref())
                } else {
                    backend.publish_with(&id, note.as_deref(), &extras) // --- access-ai ---
                }
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                v.publishing = false;
                if crate::dev_flags::get().timing {
                    match &result {
                        Ok(out) => println!("publish ok v{} {}", out.version, out.permalink),
                        Err(e) => println!("publish failed: {e}"),
                    }
                }
                match result {
                    Ok(out) => {
                        v.ext_item_published(&id, &out, note_opt.as_deref()); // --- extensions ---
                        v.delight_published(&id, cx); // --- delight ---
                        let head = match &note_opt {
                            Some(n) => format!("Published v{} · “{n}”", out.version),
                            None => format!("Published v{}", out.version),
                        };
                        // --- browser macros --- a granted macro can cross-post it.
                        let cross = if v.macro_cross_post_hint() {
                            crate::keymap::hint(" · ⇧⌘P to cross-post")
                        } else {
                            ""
                        };
                        let sub = crate::keymap::hint_owned(format!(
                            "{} · ⌘O opens it{cross}",
                            vm::short_permalink(&out.permalink)
                        ));
                        let head = match out.warning {
                            Some(w) => format!("{head} · {w}"),
                            None => head,
                        };
                        v.show_toast(head, Some(sub.into()), cx);
                    }
                    Err(CoreError::Offline) => {
                        v.show_toast("Offline. Publish again when you're back online.", None, cx)
                    }
                    // --- conflicts --- refused for a conflict: the sheet
                    // again, not a toast with no way out (issue #52).
                    Err(_) if v.reopen_conflict(&id, window, cx) => {}
                    Err(e) => v.show_toast(format!("Publish failed: {e}"), None, cx),
                }
                // --- follow-ups --- promotion may have rewritten local images.
                v.scratch_sync_editor(&id, window, cx);
                v.requery(window, cx);
            });
        })
        .detach();
    }

    pub(super) fn fake_offline(
        &mut self,
        _: &FakeToggleOffline,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(f) = &self.fake {
            let off = !f.is_offline();
            f.set_offline(off);
            self.show_toast(
                if off {
                    "Wi-Fi off (simulated). Keep writing: nothing is lost."
                } else {
                    "Back online"
                },
                None,
                cx,
            );
        }
    }

    pub(super) fn fake_conflict(
        &mut self,
        _: &FakeConflict,
        _: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if let (Some(f), Some(cur)) = (&self.fake, &self.current) {
            f.trigger_conflict(&cur.local_id);
        }
    }

    pub(super) fn resolve(&mut self, how: Resolution, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ConflictSheet { id, .. }) = self.sheet.take() else {
            return;
        };
        match self.backend.resolve_conflict(&id, how) {
            Ok(()) => {
                if self.current.as_ref().is_some_and(|c| c.local_id == id) {
                    self.current = self.backend.item(&id);
                    self.load_current_into_editor(window, cx);
                }
                self.requery(window, cx);
                self.show_toast(
                    match how {
                        Resolution::KeepMine => {
                            "Kept your version; the server copy is saved in version history"
                        }
                        Resolution::TakeServer => "Took the server's version",
                        Resolution::KeepBoth => "Kept both: your version is now a separate draft",
                    },
                    None,
                    cx,
                );
            }
            Err(e) => self.show_toast(format!("Couldn't resolve: {e}"), None, cx),
        }
        self.focus_after_sheet(window, cx);
        cx.notify();
    }

    /// --- conflicts --- The resolve sheet for an item in conflict.
    pub(crate) fn show_conflict_sheet(
        &mut self,
        id: LocalId,
        mine: &str,
        theirs: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (m, t) = vm::word_diff(mine, theirs);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.sheet.open(ConflictSheet {
            id,
            mine: m,
            theirs: t,
            focus,
        });
        cx.notify();
    }

    /// --- conflicts --- Show the resolve sheet again for an item that is
    /// still in conflict, from the texts the store kept (the event that
    /// first showed it fires only once; issue #52). False when it isn't in
    /// conflict, or another sheet is up.
    pub(crate) fn reopen_conflict(
        &mut self,
        id: &LocalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.sheet.is_open() {
            return false;
        }
        let Some((mine, theirs)) = self.backend.conflict_texts(id) else {
            return false;
        };
        self.show_conflict_sheet(id.clone(), &mine, &theirs, window, cx);
        true
    }
}
