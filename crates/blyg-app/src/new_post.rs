//! ⌘N, New Post (docs/SPEC.md § Interaction spec, "New post").
//!
//! A child module of `app` (like `scratch.rs`). ⌘N opens a fresh, empty
//! editor with the caret in it; the omnibar's text and the list's filter and
//! selection are left alone. Nothing exists until the first real keystroke,
//! which creates a local scratch note (`Backend::create_scratch`), or a draft
//! with `new-note = draft`; after that it autosaves like any edit. ⌘D makes
//! the scratch note a draft and ⌘⏎ publishes it, through the usual paths.
//! Leaving it while it's empty (another post, the omnibar's search, ⌘N
//! again) deletes it, so there are no stray blank items.

use blyg_core::Status;
use blyg_core::config::NewNote;

use super::*;

/// The new post being written: what it starts as, and its id once the
/// first keystroke has created it.
#[derive(Debug, Clone)]
pub(crate) struct NewPost {
    pub flavor: NewNote,
    pub id: Option<LocalId>,
}

/// The status a new post of `flavor` is created with.
fn status_of(flavor: NewNote) -> Status {
    match flavor {
        NewNote::Scratch => Status::Scratch,
        NewNote::Draft => Status::Draft,
    }
}

/// The status bar's label while writing a new post.
pub(crate) fn label(flavor: NewNote) -> &'static str {
    match flavor {
        NewNote::Scratch => crate::keymap::hint("New note · saved locally · ⌘D draft · ⌘⏎ publish"),
        NewNote::Draft => crate::keymap::hint("New draft · syncs to your blyg · ⌘⏎ publish"),
    }
}

/// The empty editor's placeholder.
fn placeholder(flavor: NewNote) -> &'static str {
    match flavor {
        NewNote::Scratch => {
            "New note. Start typing; it stays on this Mac until you make it a draft."
        }
        NewNote::Draft => "New draft. Start typing.",
    }
}

impl MainView {
    /// ⌘N: leave whatever is open and give the editor a fresh, empty note.
    pub(super) fn start_new_post(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.leave_reading(); // --- reading & versions --- back to Posts
        let flavor = self.prefs.new_post;
        let fresh = self
            .new_post
            .as_ref()
            .is_some_and(|np| np.id.is_none() && self.current.is_none());
        if !fresh {
            self.leave_new_post(window, cx);
            self.current = None;
            self.set_editor_text("", window, cx);
            self.refresh_preview(true, cx);
        }
        self.new_post = Some(NewPost { flavor, id: None });
        // Edit first, so focusing the editor never runs the omnibar's create.
        self.mode = Mode::Edit;
        self.editor.update(cx, |s, cx| {
            s.set_placeholder(placeholder(flavor), window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    /// The editor changed: in a new post that doesn't exist yet, the first
    /// real keystroke creates it (with the text so far).
    pub(super) fn new_post_first_keystroke(&mut self, cx: &mut Context<Self>) {
        let Some(flavor) = self
            .new_post
            .as_ref()
            .filter(|np| np.id.is_none() && self.current.is_none())
            .map(|np| np.flavor)
        else {
            return;
        };
        let text = self.editor.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        let made = match flavor {
            NewNote::Scratch => self.backend.create_scratch(Kind::Fragment, &text),
            NewNote::Draft => self.backend.create_draft(Kind::Fragment, &text),
        };
        match made {
            Ok(id) => {
                if let Some(np) = self.new_post.as_mut() {
                    np.id = Some(id.clone());
                }
                self.current = self.backend.item(&id);
                // Same query: it shows (and is selected) if it matches.
                let results = self.backend.search(self.list.query());
                self.list.refresh(results);
                self.list.select(&id);
            }
            Err(e) => self.show_toast(
                format!("Couldn't create a {}: {e}", scratch::new_note_noun(flavor)),
                None,
                cx,
            ),
        }
    }

    /// Leave the new post: kept if it has any text, deleted if it's empty
    /// (and still what ⌘N made it: a draft made with ⌘D stays).
    pub(super) fn leave_new_post(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(np) = self.new_post.take() else {
            return;
        };
        self.editor
            .update(cx, |s, cx| s.set_placeholder("", window, cx));
        let Some(id) = np.id else {
            return;
        };
        let Some(item) = self.backend.item(&id) else {
            return;
        };
        if item.status == status_of(np.flavor)
            && item.content_md.trim().is_empty()
            && self.backend.delete_draft(&id).is_ok()
        {
            self.list.remove(&id);
            if self.current.as_ref().is_some_and(|c| c.local_id == id) {
                self.current = None;
            }
        }
    }

    /// The status bar's label while the editor holds the new post (until
    /// ⌘D or ⌘⏎ makes it something else).
    pub(super) fn new_post_label(&self) -> Option<&'static str> {
        let np = self.new_post.as_ref()?;
        let open = match (&np.id, &self.current) {
            (None, None) => true,
            (Some(id), Some(cur)) => &cur.local_id == id && cur.status == status_of(np.flavor),
            _ => false,
        };
        (open && reading::vm::screen_status(self.reading.view, self.unread).post)
            .then(|| label(np.flavor))
    }
}

#[cfg(test)]
#[path = "new_post_tests.rs"]
mod tests;
