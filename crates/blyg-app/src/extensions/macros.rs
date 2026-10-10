//! --- extensions --- Browser macros in the ⇧⌘P palette, and the window's
//! side of `burrow/browser.page` and `burrow/browser.open`
//! (docs/EXTENSIONS.md § Browser: capture and macros).
//!
//! A macro row is "<title> ↗ <site>". Choosing it checks the grant again
//! (`Host::macro_entry`), fills the template from the published post open
//! in the editor (title, its opening paragraph, its permalink), lets the
//! extension shape the text (`extension/macro.prepare`, off the main
//! thread), then hands the run to the browser pane
//! (`browser::automation::start`), whose Preview sheet comes first.

use blyg_core::{Item, Status};
use blyg_ext::protocol::{ItemSummary, MacroPrepareParams, PageCapture, Screen};
use blyg_ext::recipe::{self, Vars};
use blyg_ext::{MacroEntry, UiReply};
use gpui_kit::*;

use super::sheets::{Row, RowAction};
use crate::app::MainView;
use crate::app::browser::automation::{self, MacroRun};

/// How long the opening paragraph may be in `{{excerpt}}`.
pub const EXCERPT_CHARS: usize = 280;

/// The palette row's label: the macro's title and the site it posts to.
pub fn row_label(m: &MacroEntry) -> String {
    format!("{} ↗ {}", m.spec.title, m.site.title)
}

/// The template's values for a published `item` at `permalink`.
pub fn vars(item: &Item, permalink: &str) -> Vars {
    Vars {
        title: item.title(),
        excerpt: recipe::excerpt(&item.content_md, EXCERPT_CHARS),
        permalink: permalink.to_string(),
    }
}

/// Published, with a link to post: `Some(permalink)`.
fn published_link(item: &Item) -> Option<&str> {
    (item.status == Status::Public)
        .then_some(item.permalink.as_deref())
        .flatten()
        .filter(|p| !p.trim().is_empty())
}

impl MainView {
    /// The open item as the store has it now (the editor may be ahead of
    /// `current`, the store is what was published).
    fn macro_item(&self) -> Option<Item> {
        let id = &self.current.as_ref()?.local_id;
        self.backend.item(id).or_else(|| self.current.clone())
    }

    /// The palette's macro rows on `screen`: granted macros of running
    /// extensions that apply to what's open.
    pub(crate) fn ext_macro_rows(&self, screen: Screen) -> Vec<Row> {
        let Some(host) = &self.ext.host else {
            return vec![];
        };
        let item = self.macro_item();
        let has_item = item.is_some();
        let public = item.as_ref().is_some_and(|i| i.status == Status::Public);
        host.macros()
            .into_iter()
            .filter(|m| m.spec.applies(screen, has_item, public))
            .map(|m| Row {
                action: RowAction::Macro {
                    ext: m.ext.clone(),
                    id: m.spec.id.clone(),
                },
                label: row_label(&m),
                detail: if m.spec.detail.is_empty() {
                    format!(
                        "Fill it in on {}; you check it and click Post",
                        m.site.title
                    )
                } else {
                    m.spec.detail.clone()
                },
                from: Some(m.ext),
            })
            .collect()
    }

    /// The palette's "Open <site>" rows: each granted macro site of a
    /// running extension, once, so signing in there is one click (the
    /// browser pane is macOS only).
    pub(crate) fn ext_site_rows(&self) -> Vec<Row> {
        let Some(host) = &self.ext.host else {
            return vec![];
        };
        if cfg!(all(not(target_os = "macos"), not(test))) {
            return vec![];
        }
        let mut rows: Vec<Row> = vec![];
        for m in host.macros() {
            let action = RowAction::OpenSite {
                ext: m.ext.clone(),
                site: m.site.id.clone(),
            };
            if rows.iter().any(|r| r.action == action) {
                continue;
            }
            rows.push(Row {
                action,
                label: format!("Open {}", m.site.title),
                detail: "In the browser pane: sign in there once, by hand".into(),
                from: Some(m.ext),
            });
        }
        rows
    }

    /// An "Open <site>" row: the site's `home` in the pane (the grant is
    /// checked again, as for a macro).
    pub(crate) fn ext_open_site(
        &mut self,
        ext: &str,
        site: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.ext.host.clone() else {
            return;
        };
        let Some(entry) = host
            .macros()
            .into_iter()
            .find(|m| m.ext == ext && m.site.id == site)
        else {
            return self.show_toast(
                format!("{ext} can't open that site any more"),
                Some("Its site isn't allowed: Manage extensions…".into()),
                cx,
            );
        };
        if self.browser.automation.running() {
            return self.show_toast(
                "A macro is running in the browser pane",
                Some("■ Stop in the pane's bar stops it".into()),
                cx,
            );
        }
        let home = entry.site.home.clone();
        self.open_url_in_app(&home, crate::app::browser::OpenMode::Slide, window, cx);
    }

    /// A macro row was chosen: check, fill in, prepare, run.
    pub(crate) fn ext_run_macro(
        &mut self,
        ext: String,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.ext.host.clone() else {
            return;
        };
        // The grant may have gone (Reload Config) since the palette opened.
        let Some(entry) = host.macro_entry(&ext, &id) else {
            return self.show_toast(
                format!("{ext} can't post there any more"),
                Some("Its site isn't allowed: Manage extensions…".into()),
                cx,
            );
        };
        if crate::connection::mode(cx) == Some(crate::connection::Mode::Disconnected) {
            return self.show_toast(
                format!("Connect your blyg to cross-post to {}", entry.site.title),
                Some("A post needs its link on your blyg first".into()),
                cx,
            );
        }
        let Some(item) = self.macro_item() else {
            return self.show_toast(
                format!(
                    "Open a published post to cross-post it to {}",
                    entry.site.title
                ),
                None,
                cx,
            );
        };
        let Some(permalink) = published_link(&item).map(str::to_string) else {
            return self.show_toast(
                format!("Publish it first to cross-post it to {}", entry.site.title),
                Some("Only a published post has a link to share".into()),
                cx,
            );
        };
        if self.browser.automation.running() {
            return self.show_toast(
                "A macro is already running in the browser pane",
                Some("Stop it in the pane's bar first".into()),
                cx,
            );
        }
        let text = recipe::expand(&entry.spec.template, &vars(&item, &permalink));
        let params = MacroPrepareParams {
            macro_id: entry.spec.id.clone(),
            item: ItemSummary::of(&item, true),
            permalink,
            text: text.clone(),
        };
        let e = ext.clone();
        let task = cx.background_spawn(async move { host.macro_prepare(&e, &params) });
        cx.spawn_in(window, async move |this, cx| {
            let prepared = task.await;
            let _ = this.update_in(cx, |v, window, cx| match prepared {
                Err(err) => v.show_toast(
                    err.message(&ext),
                    Some(format!("Nothing was posted to {}", entry.site.title).into()),
                    cx,
                ),
                Ok(p) => {
                    let (text, note) = match p {
                        Some(r) => (r.text, r.note),
                        None => (text, None),
                    };
                    let run = MacroRun::new(ext, entry.site, entry.spec, text).with_note(note);
                    automation::start(v, run, window, cx);
                }
            });
        })
        .detach();
    }

    // ------------------------------------------------------------ browser.*

    /// `burrow/browser.page` (the host checked the grant and that one of
    /// the extension's commands is running): the page in the pane, or
    /// `None` when it's closed, has no page, or a macro is running there
    /// (an extension never reads the signed-in page a macro fills in).
    pub(crate) fn ext_browser_page(
        &mut self,
        reply: UiReply<Option<PageCapture>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.browser.automation.running() {
            return reply.answer(None);
        }
        let Some(rx) = self.browser_capture(cx) else {
            return reply.answer(None);
        };
        cx.spawn_in(window, async move |_, _| {
            let got = rx.recv().await.ok().flatten();
            reply.answer(got.map(|c| c.page));
        })
        .detach();
    }

    /// `burrow/browser.open {url}` (the host checked that the URL is on an
    /// origin the user granted the extension): show the pane at `url`.
    pub(crate) fn ext_browser_open(
        &mut self,
        url: String,
        reply: UiReply<Result<String, String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.browser.automation.running() {
            return reply.answer(Err("a macro is running in the browser pane".into()));
        }
        // The pane is macOS only for now (elsewhere a link would go to the
        // default browser, which isn't what was asked).
        if cfg!(all(not(target_os = "macos"), not(test))) {
            return reply.answer(Err("there's no browser pane on this platform yet".into()));
        }
        self.open_url_in_app(&url, crate::app::browser::OpenMode::Full, window, cx);
        if self.browser.open && self.browser.page.url == url {
            reply.answer(Ok(url));
        } else {
            reply.answer(Err("the browser pane didn't open".into()));
        }
    }

    // ------------------------------------------------------------ demo

    /// `BLYGGER_DEMO=br-macro-ext` (debug builds;
    /// `scripts/browser-macro-ext-check.sh`): a real extension's macro,
    /// end to end. The config enables an installed fixture extension whose
    /// site is the fixture at `BLYGGER_DEMO_URL` on 127.0.0.1 (never a real
    /// site). Signs in there with the fixture's own button, opens a
    /// published post, chooses the macro's ⇧⌘P row (so `macro.prepare`
    /// runs), and lets the sheets answer themselves; then prints what the
    /// fixture's `#posted` list holds and quits.
    pub(crate) fn macro_ext_demo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !automation::demo_auto() {
            return;
        }
        let Ok(base) = std::env::var("BLYGGER_DEMO_URL") else {
            eprintln!("br-macro-ext: set BLYGGER_DEMO_URL to the fixture site");
            return;
        };
        let base = base.trim_end_matches('/').to_string();
        cx.spawn_in(window, async move |this, cx| {
            let exec = cx.background_executor().clone();
            let pause = |ms| exec.timer(std::time::Duration::from_millis(ms));
            let quit = |this: &WeakEntity<MainView>, cx: &mut AsyncWindowContext| {
                let _ = this.update(cx, |_, cx| cx.quit());
            };
            // The extension starts about a second after the window opens.
            let mut ready = false;
            for _ in 0..150 {
                pause(100).await;
                ready = this
                    .update(cx, |v, _| {
                        v.ext.host.as_ref().is_some_and(|h| !h.macros().is_empty())
                    })
                    .unwrap_or(false);
                if ready {
                    break;
                }
            }
            if !ready {
                println!("macro-ext-no-macro");
                return quit(&this, cx);
            }
            // Sign in to the fixture by hand, as a user would in the pane.
            let login = format!("{base}/login.html");
            let _ = this.update_in(cx, |v, window, cx| {
                v.open_url_in_app(&login, crate::app::browser::OpenMode::Full, window, cx)
            });
            pause(1500).await;
            let _ = this.update(cx, |v, _| {
                v.browser.with(|s| {
                    s.probe(
                        "(document.getElementById('sign-in').click(), 'signed-in')",
                        "macro-demo",
                    )
                })
            });
            pause(1500).await;
            // A published post in the editor, then ⇧⌘P and the macro's row.
            let row = this
                .update_in(cx, |v, window, cx| {
                    let item = v
                        .backend
                        .items()
                        .into_iter()
                        .find(|i| published_link(i).is_some())?;
                    v.open(&item.local_id, window, cx);
                    v.ext_toggle_palette(window, cx);
                    let Some(super::Overlay::Palette { rows, .. }) = &v.ext.overlay else {
                        return None;
                    };
                    let i = rows
                        .iter()
                        .position(|r| matches!(r.action, RowAction::Macro { .. }))?;
                    println!("macro-ext-row {:?}", rows[i].label);
                    v.ext_run_row(i, window, cx);
                    Some(())
                })
                .ok()
                .flatten();
            if row.is_none() {
                println!("macro-ext-no-row");
                return quit(&this, cx);
            }
            // macro.prepare, then the run (its sheets answer themselves).
            let running = |this: &WeakEntity<MainView>, cx: &mut AsyncWindowContext| {
                this.update(cx, |v, _| v.browser.automation.running())
                    .unwrap_or(false)
            };
            for _ in 0..150 {
                pause(100).await;
                if running(&this, cx) {
                    break;
                }
            }
            for _ in 0..600 {
                pause(100).await;
                if !running(&this, cx) {
                    break;
                }
            }
            pause(500).await;
            let _ = this.update(cx, |v, _| {
                v.browser.with(|s| {
                    s.probe(
                        "JSON.stringify([...document.querySelectorAll('#posted li')]\
                         .map(li => li.innerText))",
                        "macro-dom-posted",
                    )
                })
            });
            pause(800).await;
            let _ = this.update(cx, |v, _| {
                v.browser.with(|s| {
                    s.probe(
                        "(document.cookie = 'fixture_session=; max-age=0; path=/', 'signed-out')",
                        "macro-demo",
                    )
                })
            });
            pause(500).await;
            println!("macro-demo-done");
            quit(&this, cx);
        })
        .detach();
    }
}
