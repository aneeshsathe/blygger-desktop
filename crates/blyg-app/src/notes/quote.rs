//! Universal quoting (⇧⌘D, Post › Quote Selection in Draft, the browser's
//! "→ Draft"): text highlighted in the browser pane, the reading pane's
//! post or the notes drawer goes into the draft being written, at its
//! caret, with where it came from; a new draft when none is open.
//!
//! The Markdown is [`vm::quote_block`]: a partial transclusion
//! (`![[id]]` + `> passage`) for a blyg post we hold, going into a thread;
//! otherwise a blockquote with a `> — [title](url)` source line.

use blyg_core::{FRAGMENT_LIMIT, Kind, Status, published_len};
use gpui_kit::*;

use crate::app::MainView;
use crate::app::reading::vm::{self, QuoteSource};

gpui_kit::actions!(
    blygger,
    [
        /// ⇧⌘D: put the passage highlighted in a page, a post or your notes
        /// into the draft, with a link to where it came from.
        QuoteToDraft
    ]
);

impl MainView {
    /// ⇧⌘D / Post › Quote Selection in Draft / the browser's "→ Draft".
    pub(crate) fn quote_to_draft(
        &mut self,
        _: &QuoteToDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The notes drawer, when it has the keyboard and a selection.
        if let Some((sel, source)) = self.notes_selection(window, cx) {
            return self.quote_into_draft(sel, source, window, cx);
        }
        let (tx, rx) = async_channel::bounded::<String>(1);
        let source = if self.browser.open {
            let page = &self.browser.page;
            let url = Some(page.url.clone()).filter(|u| crate::app::browser::is_web_url(u));
            let held = url
                .as_deref()
                .and_then(|u| vm::blyg_item_for_url(u, &self.reading.rows, &self.reading.subs));
            let source = match held {
                Some(r) => QuoteSource::Blyg {
                    id: r.remote_id.clone(),
                    html: r.content_html.clone(),
                    title: page.title.clone(),
                    url: url.clone(),
                },
                None => QuoteSource::Page {
                    title: page.title.clone(),
                    url,
                },
            };
            self.browser.selection(tx.clone()).then_some(source)
        } else if self.studio.reader.active()
            && let Some(o) = self.reading.opened.as_ref()
        {
            let r = &o.item;
            let blyg = self
                .reading
                .subs
                .iter()
                .find(|s| s.id == r.subscription_id)
                .is_none_or(|s| s.kind == blyg_core::SubscriptionKind::Blyg);
            let (title, url) = (vm::post_title(r), vm::web_url(r));
            let source = if blyg {
                QuoteSource::Blyg {
                    id: r.remote_id.clone(),
                    html: r.content_html.clone(),
                    title,
                    url,
                }
            } else {
                QuoteSource::Page { title, url }
            };
            self.studio.reader.selection(tx.clone()).then_some(source)
        } else {
            None
        };
        let Some(source) = source else {
            return self.show_toast(NOTHING, Some(WHERE.into()), cx);
        };
        // A page that never answers mustn't make the key look dead.
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
            let _ = tx.try_send(String::new());
        })
        .detach();
        cx.spawn_in(window, async move |this, cx| {
            let text = rx.recv().await.unwrap_or_default();
            let _ = this.update_in(cx, |v, window, cx| {
                v.quote_into_draft(text, source, window, cx)
            });
        })
        .detach();
    }

    /// The notes drawer's selection, when the drawer has the keyboard: the
    /// passage (its `>` markers dropped) and the source line of the quote
    /// it sits in, if any. `None` otherwise.
    fn notes_selection(&self, window: &Window, cx: &App) -> Option<(String, QuoteSource)> {
        if !self.notes.open {
            return None;
        }
        let focus = self.notes.focus.as_ref()?;
        if !focus.contains_focused(window, cx) {
            return None;
        }
        let editor = self.notes.editor.as_ref()?.read(cx);
        let (text, range) = (editor.value().to_string(), editor.selected_range());
        let (sel, link) = super::notes_passage(&text, range)?;
        let source = match link {
            Some((title, url)) => QuoteSource::Page {
                title,
                url: Some(url),
            },
            None => QuoteSource::Page {
                title: String::new(),
                url: None,
            },
        };
        Some((sel, source))
    }

    /// Put `selection` into the draft being written (at its caret), or a
    /// new draft, and show it.
    pub(crate) fn quote_into_draft(
        &mut self,
        selection: String,
        source: QuoteSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self.current.clone().filter(|c| {
            matches!(c.status, Status::Draft | Status::Scratch)
                && self.notes.id.as_ref() != Some(&c.local_id)
        });
        let blyg = matches!(source, QuoteSource::Blyg { .. });
        let into_thread = target.as_ref().map_or(blyg, |t| t.kind == Kind::Thread);
        let Some(block) = vm::quote_block(&selection, &source, into_thread) else {
            return self.show_toast(NOTHING, Some(WHERE.into()), cx);
        };
        let partial = block.starts_with("![[");
        self.close_browser(window, cx);
        match target {
            Some(item) => {
                // The caret before anything moves it (opening puts it at the end).
                let (text, cursor) = {
                    let s = self.editor.read(cx);
                    (s.value().to_string(), s.cursor())
                };
                self.leave_reading();
                self.back_to_search(window, cx);
                self.open(&item.local_id, window, cx);
                let now = self.editor.read(cx).value().to_string();
                let cursor = if now == text { cursor } else { now.len() };
                let (new_text, caret) = crate::app::reading::insert_block(&now, cursor, &block);
                self.splice_editor(&now, &new_text, Some(caret), window, cx);
                self.show_toast(
                    format!("Quoted in “{}”", crate::vm::item_title(&item)),
                    None,
                    cx,
                );
            }
            None => {
                let kind = if partial || published_len(&block) > FRAGMENT_LIMIT {
                    Kind::Thread
                } else {
                    Kind::Fragment
                };
                match self.backend.create_draft(kind, &format!("{block}\n\n")) {
                    Ok(id) => {
                        self.open_new_draft(&id, window, cx);
                        self.show_toast("New draft with the quote", None, cx);
                    }
                    Err(e) => self.show_toast(format!("Couldn't start a draft: {e}"), None, cx),
                }
            }
        }
    }
}

const NOTHING: &str = "Nothing selected to quote";
const WHERE: &str = "Highlight a passage in a page, a post or your notes";
