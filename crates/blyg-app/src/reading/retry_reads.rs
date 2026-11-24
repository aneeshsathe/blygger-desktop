//! --- studio 0.71 --- Retry reads: Blyg › Retry Reads, and the link beside
//! a read that failed (the Responses screen, a post's history). It asks the
//! blyg again (a pull, `Backend::sync_now`), reads the failed reads on
//! screen again, and re-reads every list on screen: the posts list, the
//! reading rows and their counts. Studio 0.71's "retry reads" also reloads
//! the page's lists; before, a list that had failed stayed as it was until
//! the next sync.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Load;
use crate::app::MainView;

gpui_kit::actions!(blygger, [RetryReads]);

/// The link's words.
pub const RETRY_READS: &str = "Retry reads";

impl MainView {
    /// Hook: the action on the root element.
    pub(super) fn retry_reads_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &RetryReads, window, cx| this.retry_reads(window, cx)))
    }

    /// Ask the blyg again, then read again what failed and every list on
    /// screen. One at a time: a second ask while one runs does nothing.
    pub(crate) fn retry_reads(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reading.retrying {
            return;
        }
        self.reading.retrying = true;
        cx.notify();
        let backend = self.backend.clone();
        let pulled = cx.background_spawn(async move { backend.sync_now() });
        cx.spawn_in(window, async move |this, cx| {
            let r = pulled.await;
            let _ = this.update_in(cx, |v, window, cx| {
                v.reading.retrying = false;
                // The reads that failed on screen, again.
                if matches!(v.reading.mentions, Load::Failed(_)) {
                    v.load_mentions(cx);
                }
                v.refresh_own_post(cx);
                // The lists, from what the store holds now.
                v.unread = v.backend.unread_count();
                v.requery(window, cx);
                v.reading_changed(cx);
                if let Err(e) = r {
                    v.show_toast(format!("Couldn't read from your blyg: {e}"), None, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A failed read's note, with Retry reads beside it.
    pub(super) fn retry_note(
        &self,
        s: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let busy = self.reading.retrying;
        div()
            .flex()
            .items_baseline()
            .gap(px(10.))
            .child(self.muted_note(s))
            .child(
                div()
                    .id("retry-reads")
                    .debug_selector(|| "retry-reads".into())
                    .flex_none()
                    .pr(px(16.))
                    .text_size(px(12.5))
                    .text_color(if busy { p.muted } else { p.accent })
                    .when(!busy, |d| d.cursor_pointer())
                    .child(if busy { "Retrying…" } else { RETRY_READS })
                    .on_click(cx.listener(|this, _, window, cx| this.retry_reads(window, cx))),
            )
            .into_any_element()
    }
}
