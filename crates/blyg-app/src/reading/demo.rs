//! `BLYGGER_DEMO=rd-…`: scripted walkthroughs of the reading screens for
//! screenshots (fake mode), calling the same methods the keys do.

use blyg_core::LocalId;
use gpui_kit::*;

use super::View;
use crate::app::MainView;
use crate::fake::reading_seed::{ADA_REPLY, LIN_GARDENS, LIN_REREAD, RUE_TRUST};

impl MainView {
    pub(crate) fn reading_demo(
        &mut self,
        scenario: &str,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = |this: &mut Self, id: &str, window: &mut Window, cx: &mut Context<Self>| {
            if let Some(k) = this
                .reading
                .rows
                .iter()
                .find(|r| r.remote_id == id)
                .map(super::vm::key)
            {
                this.open_reading(k, window, cx);
            }
        };
        // The list + post scenarios below predate the stream: show them on
        // the Reader layout. `rd-stream*` use the stream.
        if n == 0 {
            self.reading.mode = if scenario.starts_with("rd-stream") {
                super::stream_vm::ReadMode::Stream
            } else {
                super::stream_vm::ReadMode::Reader
            };
        }
        match (scenario, n) {
            // --- stream --- the default reading view, a thread selected.
            ("rd-stream", 0) => self.show_view(View::Reading, window, cx),
            ("rd-stream", 1) => {
                self.stream_move(1, window, cx);
                self.stream_move(1, window, cx);
            }
            // Scrolled to Ada's stub (lineage line + a quote box).
            ("rd-stream-quote", 0) => self.show_view(View::Reading, window, cx),
            ("rd-stream-quote", 1) => {
                let ix = self
                    .reading
                    .shown_rows()
                    .position(|r| r.remote_id == ADA_REPLY);
                if let Some(ix) = ix {
                    for _ in 0..=ix {
                        self.stream_move(1, window, cx);
                    }
                }
            }
            // Lin's reread: a cited quote not held here, and an `[[id]]` link.
            ("rd-stream-link", 0) => self.show_view(View::Reading, window, cx),
            ("rd-stream-link", 1) => {
                let ix = self
                    .reading
                    .shown_rows()
                    .position(|r| r.remote_id == LIN_REREAD);
                if let Some(ix) = ix {
                    for _ in 0..=ix {
                        self.stream_move(1, window, cx);
                    }
                }
            }
            // "Read more": the side pane beside the stream.
            ("rd-stream-pane", 0) => self.show_view(View::Reading, window, cx),
            ("rd-stream-pane", 1) => open(self, LIN_GARDENS, window, cx),
            // The search filtering the stream.
            ("rd-stream-search", 0) => self.show_view(View::Reading, window, cx),
            ("rd-stream-search", 1) => {
                self.focus_reading_search(window, cx);
                self.set_reading_query("bench", window, cx);
            }
            // --- reading compose --- the compose box under the stream, a
            // thought half typed; `rd-compose-reader`: the Reader with a
            // post open and its quote in the box.
            ("rd-stream-compose", 0) => self.show_view(View::Reading, window, cx),
            ("rd-stream-compose", 1) => {
                self.stream_move(1, window, cx);
                self.focus_compose(window, cx);
                if let Some(e) = self.reading.compose.editor.clone() {
                    e.update(cx, |s, cx| {
                        s.set_value(
                            "Trust as a unit of lending: that's the line I keep coming back to.",
                            window,
                            cx,
                        )
                    });
                }
            }
            ("rd-compose-reader", 0) => self.show_view(View::Reading, window, cx),
            ("rd-compose-reader", 1) => {
                open(self, RUE_TRUST, window, cx);
                self.compose_quote_open(window, cx);
                if let Some(e) = self.reading.compose.editor.clone() {
                    e.update(cx, |s, cx| {
                        let t = format!(
                            "{}\nSpend it carefully, and say where you got it.",
                            s.value()
                        );
                        s.set_value(t, window, cx)
                    });
                }
            }
            // --- quote targets --- a held quote's original opens at once;
            // one nobody here follows is fetched, at its quoted pinned v1.
            ("rd-stream-original", 0) => self.show_view(View::Reading, window, cx),
            ("rd-stream-original", 1) => {
                use crate::fake::reading_seed::{ADA, ADA_TIDES};
                self.open_original(ADA.into(), ADA_TIDES.into(), Some(2), window, cx)
            }
            ("rd-stream-fetch", 0) => self.show_view(View::Reading, window, cx),
            ("rd-stream-fetch", 1) => {
                use crate::fake::reading_seed::{KIT, KIT_TIDES};
                self.open_original(KIT.into(), KIT_TIDES.into(), Some(1), window, cx)
            }
            // --- responses --- Lin's bench note: Ada stubbed and quoted it.
            ("rd-stream-responses", 0) => self.show_view(View::Reading, window, cx),
            ("rd-stream-responses", 1) => {
                open(self, crate::fake::reading_seed::LIN_BENCH, window, cx)
            }
            // --- lineage --- ⌘J on the bench post; `-ring`: Space, then R.
            ("rd-stream-lineage" | "rd-stream-lineage-ring", 0) => {
                self.show_view(View::Reading, window, cx)
            }
            ("rd-stream-lineage" | "rd-stream-lineage-ring", 1) => {
                let bench = crate::fake::reading_seed::LIN_BENCH;
                let origin = self
                    .reading
                    .rows
                    .iter()
                    .find(|r| r.remote_id == bench)
                    .map(|r| r.origin.clone());
                if let Some(origin) = origin {
                    self.open_lineage(origin, bench.into(), window, cx);
                }
            }
            ("rd-stream-lineage-ring", 2) => {
                self.lineage_open_ring(cx);
                self.lineage_preview(super::lineage_vm::Act::Reply, cx);
            }
            // --- reader folders --- the three panes: a folder picked, a post open.
            ("rd-three-pane", 0) => self.show_view(View::Reading, window, cx),
            ("rd-three-pane", 1) => {
                use super::sources_vm::Source;
                use crate::fake::reading_seed::FOLDER_FRIENDS;
                self.select_source(Source::Folder(FOLDER_FRIENDS.into()), cx);
                open(self, RUE_TRUST, window, cx);
            }
            // --- hoppers --- a hopper picked, its first post open; then
            // the Hopper picker on that post.
            ("rd-hoppers" | "rd-hopper-picker", 0) => self.show_view(View::Reading, window, cx),
            ("rd-hoppers" | "rd-hopper-picker", 1) => {
                use super::sources_vm::Source;
                use crate::fake::reading_seed::HOPPER_SLOW;
                self.select_source(Source::Hopper(HOPPER_SLOW.into()), cx);
                let first = self.shown_keys().first().cloned();
                if let Some(k) = first {
                    self.open_reading(k, window, cx);
                }
            }
            ("rd-hopper-picker", 2) => self.open_hopper_picker(window, cx),
            // --- read/unread --- three posts picked, then the row menu.
            ("rd-picks" | "rd-post-menu", 0) => self.show_view(View::Reading, window, cx),
            ("rd-picks" | "rd-post-menu", 1) => {
                let keys = self.shown_keys();
                open(self, &keys[0].1, window, cx);
                let cur = self.reading.sel.clone();
                for k in keys.iter().skip(1).take(2) {
                    self.reading.picked.toggle(k, cur.as_ref());
                }
                cx.notify();
            }
            ("rd-post-menu", 2) => {
                let k = self.shown_keys()[1].clone();
                self.open_post_menu(k, point(px(420.), px(200.)), cx);
            }
            // --- subscription names --- a renamed subscription's menu.
            ("rd-sub-menu", 0) => self.show_view(View::Reading, window, cx),
            ("rd-sub-menu", 1) => {
                use super::sources::MenuTarget;
                let _ = self
                    .backend
                    .rename_subscription("sub-omar", Some("Omar's field notes"));
                self.reading.subs = self.backend.subscriptions();
                self.set_pane(super::sources_vm::Pane::Sources, cx);
                self.open_source_menu(
                    MenuTarget::Sub("sub-omar".into()),
                    point(px(96.), px(392.)),
                    false,
                    cx,
                );
            }
            ("rd-sub-rename", 0) => self.show_view(View::Reading, window, cx),
            ("rd-sub-rename", 1) => {
                let _ = self
                    .backend
                    .rename_subscription("sub-omar", Some("Omar's field notes"));
                self.reading.subs = self.backend.subscriptions();
                self.open_sub_name_sheet("sub-omar".into(), window, cx);
            }
            // --- stub quotes --- Reply: the whole post, the hint line; then
            // the editor preview with a passage selected in the quote box
            // (`rd-stub-select`, "quote only this" offered); then that passage
            // quoted (`rd-stub-passage`).
            ("rd-stub" | "rd-stub-select" | "rd-stub-passage", 0) => {
                self.show_view(View::Reading, window, cx);
                open(self, LIN_GARDENS, window, cx);
            }
            ("rd-stub" | "rd-stub-select" | "rd-stub-passage", 1) => {
                if let Some(item) = self.reading.opened.as_ref().map(|o| o.item.clone()) {
                    self.item_action(item, "Reply", window, cx);
                }
                self.stub_choose_passage(window, cx);
            }
            ("rd-stub-select" | "rd-stub-passage", 2) => {
                self.studio_demo_select_passage();
                if scenario == "rd-stub-passage" {
                    cx.spawn_in(window, async move |this, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(900))
                            .await;
                        let _ = this.update_in(cx, |v, window, cx| {
                            v.quote_stub_passage(window, cx);
                        });
                    })
                    .detach();
                }
            }
            // A subscription's context menu, on its "Move to folder ›" list.
            ("rd-folder-menu", 0) => self.show_view(View::Reading, window, cx),
            ("rd-folder-menu", 1) => {
                use super::sources::MenuTarget;
                self.set_pane(super::sources_vm::Pane::Sources, cx);
                self.open_source_menu(
                    MenuTarget::Sub("sub-omar".into()),
                    point(px(96.), px(392.)),
                    true,
                    cx,
                );
            }
            // The "Today" smart feed, the first post open, keys in the post.
            ("rd-smart-today", 0) => self.show_view(View::Reading, window, cx),
            ("rd-smart-today", 1) => {
                use super::sources_vm::{Pane, Smart, Source};
                self.select_source(Source::Smart(Smart::Today), cx);
                self.move_reading(1, window, cx);
                self.set_pane(Pane::Post, cx);
            }
            // The sources pane hidden (⌥⌘S): list + post as before.
            ("rd-sources-hidden", 0) => self.show_view(View::Reading, window, cx),
            ("rd-sources-hidden", 1) => {
                self.toggle_sources(window, cx);
                open(self, LIN_GARDENS, window, cx);
            }
            // The Subscriptions screen's folder chooser.
            ("rd-subs-folder", 0) => self.show_view(View::Subscriptions, window, cx),
            ("rd-subs-folder", 1) => {
                use super::sources::MenuTarget;
                self.open_source_menu(
                    MenuTarget::Sub("sub-omar".into()),
                    point(px(560.), px(250.)),
                    true,
                    cx,
                );
            }
            // New Folder…
            ("rd-new-folder", 0) => self.show_view(View::Reading, window, cx),
            ("rd-new-folder", 1) => self.open_folder_sheet(None, None, window, cx),
            // The list with an edited post open: notes + pinned diff.
            ("rd-reading", 0) => self.show_view(View::Reading, window, cx),
            ("rd-reading", 1) => open(self, RUE_TRUST, window, cx),
            // --- reader view / changelog --- the same post on its author's
            // site (`BLYGGER_DEMO_SITE_URL`: the page to show, since the
            // sample blygs don't exist), and in the reader with the full
            // history (`rd-changelog`) or a summary (`rd-summary`).
            ("rd-author-site" | "rd-changelog" | "rd-summary", 0) => {
                use blyg_core::changelog::{ChangelogDisplay, ChangelogSettings};
                let mode = match scenario {
                    "rd-changelog" => ChangelogDisplay::History,
                    "rd-summary" => ChangelogDisplay::Summary,
                    _ => ChangelogDisplay::Latest,
                };
                self.reading.changelogs.settings = super::Load::Ready(ChangelogSettings {
                    reader: Some(mode),
                    page: Some(ChangelogDisplay::Latest),
                    summary_prompt: None,
                });
                self.prefs.author_site = scenario == "rd-author-site";
                self.show_view(View::Reading, window, cx)
            }
            ("rd-author-site" | "rd-changelog" | "rd-summary", 1) => {
                open(self, RUE_TRUST, window, cx);
                if let Some(o) = self.reading.opened.as_mut()
                    && scenario == "rd-summary"
                {
                    let at = (o.key.clone(), o.item.version);
                    self.reading.changelogs.summaries.insert(
                        at,
                        super::Load::Ready(blyg_core::changelog::ChangelogSummary {
                            summary: "Started as a short note on trust, rewritten twice as \
                                          the argument sharpened; the author pinned the second \
                                          version, and the latest adds an example."
                                .into(),
                            model: "@cf/example/sample-model".into(),
                            version: o.item.version,
                            cached: true,
                        }),
                    );
                }
            }
            // A window a screenshot run never activates may not redraw by
            // itself (nor load the page into its web view): draw it a few
            // times so a capture of the window shows the last step.
            ("rd-author-site" | "rd-changelog" | "rd-summary", 2) => {
                // Set once the post has loaded (opening it reloads the item).
                if let (Some(o), Ok(u)) = (
                    self.reading.opened.as_mut(),
                    std::env::var("BLYGGER_DEMO_SITE_URL"),
                ) && !u.is_empty()
                {
                    o.item.page = Some(u);
                }
                cx.notify();
                cx.spawn_in(window, async move |_, cx| {
                    for _ in 0..80 {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(300))
                            .await;
                        let _ = cx.update(|window, cx| window.draw(cx).clear(cx));
                    }
                })
                .detach();
            }
            // The reading search, with the first match open (#6).
            ("rd-search", 0) => self.show_view(View::Reading, window, cx),
            ("rd-search", 1) => {
                self.focus_reading_search(window, cx);
                self.set_reading_query("garden", window, cx);
                self.move_reading(1, window, cx);
            }
            // A search that matches nothing.
            ("rd-nomatch", 0) => self.show_view(View::Reading, window, cx),
            ("rd-nomatch", 1) => {
                self.focus_reading_search(window, cx);
                self.set_reading_query("zeppelin", window, cx);
            }
            ("rd-notes", 0) => self.show_view(View::Reading, window, cx),
            ("rd-notes", 1) => open(self, LIN_GARDENS, window, cx),
            // Someone else's post on a pinned version, dropdown open.
            ("rd-pill", 0) => self.show_view(View::Reading, window, cx),
            ("rd-pill", 1) => open(self, RUE_TRUST, window, cx),
            ("rd-pill", 2) => {
                self.step_version(-1, cx);
                if let Some(o) = self.reading.opened.as_mut() {
                    o.dropdown = true;
                }
            }
            // Your own post's history, then the pin sheet.
            ("rd-versions", 0) => {
                self.open(&LocalId("01J9M2A".into()), window, cx);
                self.toggle_versions(window, cx);
            }
            ("rd-pin", 0) => {
                self.open(&LocalId("01J9M2A".into()), window, cx);
                self.toggle_versions(window, cx);
            }
            ("rd-pin", 2) => self.ask_pin(window, cx),
            ("rd-mentions", 0) => self.show_view(View::Mentions, window, cx),
            ("rd-subs", 0) => self.show_view(View::Subscriptions, window, cx),
            // --- themes --- the subscribe sheet with a preview.
            ("rd-subscribe", 0) => self.show_view(View::Subscriptions, window, cx),
            ("rd-subscribe", 1) => {
                self.open_subscribe(window, cx);
                if let Some(super::RSheet::Subscribe { input, .. }) = &self.reading.sheet {
                    let input = input.clone();
                    input.update(cx, |s, cx| {
                        s.set_value("https://tides.example.org/", window, cx)
                    });
                }
            }
            ("rd-subscribe", 2) => self.subscribe_enter(window, cx),
            // --- themes --- the empty states: nothing held, nothing unread,
            // no subscriptions (the fake's data is cleared on screen only).
            ("rd-empty" | "rd-stream-empty", 0) => {
                self.show_view(View::Reading, window, cx);
                self.reading.rows.clear();
                self.reading.subs.clear();
                self.reading.opened = None;
                self.reading.sel = None;
                self.reading.refilter();
            }
            ("rd-all-read", 0) => self.show_view(View::Reading, window, cx),
            ("rd-all-read", 1) => {
                use super::sources_vm::{Smart, Source};
                for r in &mut self.reading.rows {
                    r.read_version = Some(r.version);
                }
                self.select_source(Source::Smart(Smart::Unread), cx);
            }
            ("rd-subs-empty", 0) => {
                self.show_view(View::Subscriptions, window, cx);
                self.reading.subs.clear();
            }
            ("rd-site", 0) => self.open_site_settings(window, cx),
            // --- own posts --- "Your blyg" in the Reader, your "On friction"
            // open with the reading actions; and the same post in Posts with
            // its chips above the editor.
            ("rd-own", 0) => self.show_view(View::Reading, window, cx),
            ("rd-own", 1) => {
                self.select_source(super::sources_vm::Source::Own, cx);
                if let Some(k) = self
                    .reading
                    .rows
                    .iter()
                    .find(|r| r.remote_id.starts_with("01J9M2A"))
                    .map(super::vm::key)
                {
                    self.open_reading(k, window, cx);
                }
            }
            ("rd-own-post" | "rd-own-fork", 0) => self.open(&LocalId("01J9M2A".into()), window, cx),
            // ⌘Y on your post, its 📌 v2 picked: "Fork this pin".
            ("rd-own-fork", 1) => self.toggle_versions(window, cx),
            ("rd-own-fork", 2) => {
                if let Some(own) = self.reading.own.as_mut() {
                    own.sel = Some(2);
                }
                cx.notify();
            }
            // --- background --- the Background feed, Rue's withdrawn post
            // open; and the stream at Ada's tide tables, "V2: one word".
            ("rd-background", 0) => self.show_view(View::Reading, window, cx),
            ("rd-background", 1) => {
                use super::sources_vm::{Smart, Source};
                self.select_source(Source::Smart(Smart::Background), cx);
                self.move_reading(1, window, cx);
            }
            ("rd-stream-lead", 0) => self.show_view(View::Reading, window, cx),
            ("rd-stream-lead", 1) => {
                let ix = self
                    .reading
                    .shown_rows()
                    .position(|r| r.remote_id == crate::fake::reading_seed::ADA_TIDES);
                if let Some(ix) = ix {
                    for _ in 0..=ix {
                        self.stream_move(1, window, cx);
                    }
                }
            }
            ("rd-quote", 0) => self.open(&LocalId("01J9H4C".into()), window, cx),
            ("rd-quote", 1) => self.open_quote_picker(window, cx),
            // --- browse --- the walk, the history pane, a peek, nested quotes.
            (s, n) if s.starts_with("rd-browse") || s == "rd-stream-nested" => {
                self.browse_demo(s, n, window, cx)
            }
            _ => {}
        }
        if n == 2 {
            snapshot_later(window, cx);
        }
    }
}

/// `BLYGGER_SNAPSHOT=<file.png>`: once the demo has played, render the
/// window's last frame to a PNG (Metal, no screen capture, no permissions)
/// and quit. Only in a build made with
/// `RUSTFLAGS="--cfg blygger_snap" cargo build -p blyg-app --features gpui-kit/test-support`
/// (GPUI's `render_to_image` is a test-support API); a no-op otherwise.
pub(crate) fn snapshot_later(window: &mut Window, cx: &mut Context<MainView>) {
    let Ok(path) = std::env::var("BLYGGER_SNAPSHOT") else {
        return;
    };
    cx.spawn_in(window, async move |_, cx| {
        cx.background_executor()
            .timer(std::time::Duration::from_millis(900))
            .await;
        // A window that isn't frontmost may not be redrawing: draw now (which
        // starts any sheet animation), wait it out, and draw again.
        for _ in 0..2 {
            let _ = cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.background_executor()
                .timer(std::time::Duration::from_millis(400))
                .await;
        }
        let _ = cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            snap::save_frame(window, &path);
            cx.quit();
        });
    })
    .detach();
}

// `blygger_snap` is a local --cfg for screenshot builds only.
#[allow(unexpected_cfgs)]
pub(crate) mod snap {
    use gpui_kit::Window;

    #[cfg(blygger_snap)]
    pub fn save_frame(window: &mut Window, path: &str) {
        match window.render_to_image() {
            Ok(img) => match img.save(path) {
                Ok(()) => println!("snapshot {path}"),
                Err(e) => eprintln!("snapshot failed: {e}"),
            },
            Err(e) => eprintln!("snapshot failed: {e}"),
        }
    }

    #[cfg(not(blygger_snap))]
    pub fn save_frame(_: &mut Window, path: &str) {
        eprintln!(
            "BLYGGER_SNAPSHOT={path}: this build can't render snapshots (see reading/demo.rs)"
        );
    }
}
