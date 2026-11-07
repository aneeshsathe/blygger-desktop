//! --- site --- Blyg › Site…: the parts of Studio that run the blyg's public
//! site, which Site Settings (title, author, bio, masthead links, time
//! zone, writing defaults) doesn't hold. One sheet, in sections as Studio's
//! settings page has them: Appearance (the reading theme with Studio's
//! custom theme editor and a preview, the typeface), Home page (order,
//! front page, posts path), Links (the main menu), Profile (avatar, author
//! and site URLs), Network (collection feeds), Updates (the studio's update
//! alerts) and Extensions (the studio's own, on or off).
//!
//! Nothing reaches the blyg until Save, which sends only what changed
//! (`blyg_core::site::SiteConfig::patch`); a refusal shows the server's
//! words. The status bar's quiet "studio update" notice is here too
//! (`render_studio_update_notice`), read every so often in the background.

use std::sync::Arc;
use std::time::Duration;

use blyg_core::site::{self, SiteConfig, THEME_COLORS};
use blyg_core::{Kind, Status};
use gpui_kit::base::input::{InputEvent, InputState, TextareaState};
use gpui_kit::*;

use super::MainView;

mod view;

#[cfg(test)]
mod tests;

gpui_kit::actions!(blygger, [ShowSite]);

/// How often the app asks the backend to refresh the studio's update state
/// (the backend itself reads the network at most every six hours).
const UPDATE_POLL: Duration = Duration::from_secs(10 * 60);

/// The sheet's sections, in Studio's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    Appearance,
    HomePage,
    Links,
    Profile,
    Network,
    Updates,
    Extensions,
}

impl Section {
    pub(crate) const ALL: [Section; 7] = [
        Section::Appearance,
        Section::HomePage,
        Section::Links,
        Section::Profile,
        Section::Network,
        Section::Updates,
        Section::Extensions,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Section::Appearance => "Appearance",
            Section::HomePage => "Home page",
            Section::Links => "Links",
            Section::Profile => "Profile",
            Section::Network => "Network",
            Section::Updates => "Updates",
            Section::Extensions => "Extensions",
        }
    }

    /// Whether this blyg's settings have anything for the section (an older
    /// studio doesn't report every key).
    pub(crate) fn shown(self, c: &SiteConfig) -> bool {
        match self {
            Section::Appearance => c.theme.is_some() || c.font.is_some(),
            Section::HomePage => {
                c.feed_order.is_some() || c.front_page.is_some() || c.posts_path.is_some()
            }
            Section::Links => c.links_position.is_some() || c.links_external.is_some(),
            Section::Profile => {
                c.author_url.is_some() || c.site_url.is_some() || c.avatar_media_id.is_some()
            }
            Section::Network => c.hopper_feeds.is_some(),
            Section::Updates => c.update_check.is_some(),
            Section::Extensions => !c.server_extensions.is_empty(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Load {
    Loading,
    Ready,
    /// The server has no settings route.
    Unavailable,
    Failed(String),
}

/// What the thread picker is choosing for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pick {
    FrontPage,
    MenuThread,
}

pub(crate) struct Sheet {
    pub(crate) section: Section,
    pub(crate) load: Load,
    /// As last read from (or saved to) the blyg.
    pub(crate) base: SiteConfig,
    /// As edited; what Save sends the difference of.
    pub(crate) form: SiteConfig,
    /// The site title, for the theme preview.
    pub(crate) title: String,
    pub(crate) author_url: Entity<InputState>,
    pub(crate) site_url: Entity<InputState>,
    pub(crate) posts_path: Entity<InputState>,
    pub(crate) theme_name: Entity<InputState>,
    pub(crate) feed_url: Entity<InputState>,
    pub(crate) thread_q: Entity<InputState>,
    pub(crate) external: Entity<TextareaState>,
    /// The custom palette's colour fields (`THEME_COLORS` order), light
    /// and dark companion.
    pub(crate) light: Vec<Entity<InputState>>,
    pub(crate) dark: Vec<Entity<InputState>>,
    /// The colour fields show the dark companion.
    pub(crate) editing_dark: bool,
    pub(crate) picking: Option<Pick>,
    pub(crate) error: Option<String>,
    pub(crate) busy: bool,
    pub(crate) saved: bool,
    pub(crate) uploading: bool,
    /// The avatar just uploaded, as picked (Studio shows it only then too).
    pub(crate) avatar: Option<Arc<Image>>,
    pub(crate) focus: FocusHandle,
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) sheet: Option<Box<Sheet>>,
    pub(crate) sheet_gen: u64,
}

/// The posts that can be the front page or sit in the main menu: your
/// published threads with a server id, as (id, title).
pub(crate) fn published_threads(items: &[blyg_core::Item]) -> Vec<(String, String)> {
    items
        .iter()
        .filter(|i| i.kind == Kind::Thread && i.status == Status::Public)
        .filter_map(|i| Some((i.server_id.as_ref()?.0.clone(), crate::vm::item_title(i))))
        .collect()
}

impl MainView {
    pub(crate) fn site_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &ShowSite, window, cx| this.open_site(window, cx)))
    }

    /// Start the quiet studio-update poll (the status bar shows it).
    pub(crate) fn site_init(&mut self, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        cx.spawn(async move |this, cx| {
            loop {
                let b = backend.clone();
                let before = b.studio_update();
                let after = cx
                    .background_spawn(async move { b.refresh_studio_update(false) })
                    .await
                    .ok()
                    .flatten();
                if before != after && this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
                cx.background_executor().timer(UPDATE_POLL).await;
                if this.update(cx, |_, _| ()).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Blyg › Site…: open (or close) the sheet and read the settings.
    pub(crate) fn open_site(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.site.sheet.is_some() {
            self.close_site(window, cx);
            return;
        }
        let input = |ph: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(ph))
        };
        let author_url = input("blank = this blyg's own address", window, cx);
        let site_url = input("blank = derived from the request", window, cx);
        let posts_path = input("posts", window, cx);
        let theme_name = input("Custom", window, cx);
        let feed_url = input(
            "https://github.com/blygger/blygger-studio/releases.atom",
            window,
            cx,
        );
        let thread_q = input("Search your published threads", window, cx);
        let external = cx.new(|cx| TextareaState::new(window, cx).soft_wrap(true));
        let mut colours = |dark: bool, window: &mut Window, cx: &mut Context<Self>| {
            THEME_COLORS
                .iter()
                .map(|slot| {
                    let e = cx.new(|cx| InputState::new(window, cx).placeholder("#rrggbb"));
                    let slot = *slot;
                    let sub = cx.subscribe_in(&e, window, move |this, e, ev, _, cx| {
                        if let InputEvent::Change = ev {
                            let v = e.read(cx).value().trim().to_ascii_lowercase();
                            this.site_set_colour(dark, slot, v, cx);
                        }
                    });
                    self._subs.push(sub);
                    e
                })
                .collect::<Vec<_>>()
        };
        let light = colours(false, window, cx);
        let dark = colours(true, window, cx);
        // ⌘⏎ in any field saves.
        for e in [&author_url, &site_url, &posts_path, &theme_name, &feed_url]
            .into_iter()
            .chain(light.iter())
            .chain(dark.iter())
        {
            let sub = cx.subscribe_in(e, window, |this, _, ev, window, cx| match ev {
                InputEvent::PressEnter {
                    secondary: true, ..
                } => this.save_site(window, cx),
                InputEvent::Change => {
                    if let Some(s) = this.site.sheet.as_mut() {
                        s.saved = false;
                    }
                    cx.notify();
                }
                _ => {}
            });
            self._subs.push(sub);
        }
        let sub = cx.subscribe_in(&thread_q, window, |_, _, _: &InputEvent, _, cx| cx.notify());
        self._subs.push(sub);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.site.sheet_gen += 1;
        self.site.sheet = Some(Box::new(Sheet {
            section: Section::Appearance,
            load: Load::Loading,
            base: SiteConfig::default(),
            form: SiteConfig::default(),
            title: String::new(),
            author_url,
            site_url,
            posts_path,
            theme_name,
            feed_url,
            thread_q,
            external,
            light,
            dark,
            editing_dark: false,
            picking: None,
            error: None,
            busy: false,
            saved: false,
            uploading: false,
            avatar: None,
            focus,
        }));
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let title = backend.settings().ok().and_then(|s| s.site_title);
            (backend.site_config(), title)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (r, title) = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                v.site_loaded(r, title, window, cx);
            });
        })
        .detach();
    }

    fn site_loaded(
        &mut self,
        r: blyg_core::Result<SiteConfig>,
        title: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.site.sheet.as_mut() else {
            return;
        };
        match r {
            Ok(c) => {
                s.title = title.unwrap_or_default();
                s.load = Load::Ready;
                s.section = Section::ALL
                    .into_iter()
                    .find(|x| x.shown(&c))
                    .unwrap_or(Section::Appearance);
                Self::site_fill(s, &c, window, cx);
                s.base = c.clone();
                s.form = c;
            }
            Err(blyg_core::CoreError::NotFound) => s.load = Load::Unavailable,
            Err(e) => s.load = Load::Failed(e.to_string()),
        }
        cx.notify();
    }

    /// Put the settings' text into the fields.
    fn site_fill(s: &mut Sheet, c: &SiteConfig, window: &mut Window, cx: &mut Context<Self>) {
        let set = |e: &Entity<InputState>, v: &str, window: &mut Window, cx: &mut Context<Self>| {
            e.update(cx, |st, cx| st.set_value(v.to_string(), window, cx));
        };
        let text = |v: &Option<String>| v.clone().unwrap_or_default();
        set(&s.author_url, &text(&c.author_url), window, cx);
        set(&s.site_url, &text(&c.site_url), window, cx);
        set(&s.posts_path, &text(&c.posts_path), window, cx);
        set(&s.theme_name, &text(&c.custom_theme_name), window, cx);
        set(&s.feed_url, &text(&c.update_feed_url), window, cx);
        let links = c.links_external.clone().unwrap_or_default();
        s.external.update(cx, |st, cx| {
            st.set_value(crate::app::reading::vm::format_links(&links), window, cx)
        });
        let light = c.custom_theme.clone().flatten();
        let dark = c.custom_theme_dark.clone().flatten();
        for (i, slot) in THEME_COLORS.iter().enumerate() {
            let l = light
                .as_ref()
                .map(|t| t.get(slot).to_string())
                .unwrap_or_default();
            let d = dark
                .as_ref()
                .map(|t| t.get(slot).to_string())
                .unwrap_or_default();
            set(&s.light[i], &l, window, cx);
            set(&s.dark[i], &d, window, cx);
        }
    }

    pub(crate) fn close_site(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.site.sheet = None;
        self.focus_after_sheet(window, cx);
        cx.notify();
    }

    /// Change the form (not the blyg) and redraw.
    pub(crate) fn site_edit(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Sheet)) {
        if let Some(s) = self.site.sheet.as_mut()
            && s.load == Load::Ready
            && !s.busy
        {
            f(s);
            s.saved = false;
            s.error = None;
            cx.notify();
        }
    }

    /// A colour typed into the custom theme editor: kept when it's a
    /// colour, so the preview follows.
    fn site_set_colour(
        &mut self,
        dark: bool,
        slot: &'static str,
        v: String,
        cx: &mut Context<Self>,
    ) {
        if !site::is_hex(&v) {
            return;
        }
        self.site_edit(cx, |s| {
            let t = if dark {
                &mut s.form.custom_theme_dark
            } else {
                &mut s.form.custom_theme
            };
            if let Some(Some(t)) = t.as_mut()
                && t.get(slot) != v
            {
                t.set(slot, v);
            }
        });
    }

    /// Choose a theme. Custom starts from the preset showing, as in Studio.
    pub(crate) fn site_choose_theme(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = id.to_string();
        let mut fill = None;
        self.site_edit(cx, |s| {
            if id == site::CUSTOM_THEME_ID
                && s.form.custom_theme.as_ref().is_none_or(|t| t.is_none())
            {
                let from = s.form.theme.clone().unwrap_or_default();
                let t = site::custom_from_preset(&from);
                fill = Some(t.clone());
                s.form.custom_theme = Some(Some(t));
                s.editing_dark = false;
            }
            s.form.theme = Some(id);
        });
        if let Some(t) = fill
            && let Some(s) = self.site.sheet.as_ref()
        {
            for (i, slot) in THEME_COLORS.iter().enumerate() {
                let v = t.get(slot).to_string();
                s.light[i].update(cx, |st, cx| st.set_value(v, window, cx));
            }
        }
    }

    /// Add or remove the dark companion (it starts from Nord).
    pub(crate) fn site_toggle_dark_companion(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut fill = None;
        self.site_edit(cx, |s| {
            let has = s
                .form
                .custom_theme_dark
                .as_ref()
                .is_some_and(|t| t.is_some());
            if has {
                s.form.custom_theme_dark = Some(None);
                s.editing_dark = false;
            } else {
                let t = site::custom_from_preset("nord");
                fill = Some(t.clone());
                s.form.custom_theme_dark = Some(Some(t));
                // A companion pairs with a light palette.
                if let Some(Some(l)) = s.form.custom_theme.as_mut() {
                    l.dark = false;
                }
                s.editing_dark = true;
            }
        });
        if let Some(t) = fill
            && let Some(s) = self.site.sheet.as_ref()
        {
            for (i, slot) in THEME_COLORS.iter().enumerate() {
                let v = t.get(slot).to_string();
                s.dark[i].update(cx, |st, cx| st.set_value(v, window, cx));
            }
        }
    }

    /// Fill the palette being edited from a preset.
    pub(crate) fn site_start_from(
        &mut self,
        preset: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let t = site::custom_from_preset(preset);
        let mut which = None;
        self.site_edit(cx, |s| {
            let dark = s.editing_dark;
            which = Some(dark);
            let mut t = t.clone();
            // The pair keeps its kinds: light, and its dark companion.
            if s.form
                .custom_theme_dark
                .as_ref()
                .is_some_and(|d| d.is_some())
                || dark
            {
                t.dark = dark;
            }
            let slot = if dark {
                &mut s.form.custom_theme_dark
            } else {
                &mut s.form.custom_theme
            };
            *slot = Some(Some(t));
        });
        if let Some(dark) = which
            && let Some(s) = self.site.sheet.as_ref()
        {
            let fields = if dark { &s.dark } else { &s.light };
            for (i, slot) in THEME_COLORS.iter().enumerate() {
                let v = t.get(slot).to_string();
                fields[i].update(cx, |st, cx| st.set_value(v, window, cx));
            }
        }
    }

    /// Pick a thread for the front page or the main menu.
    pub(crate) fn site_pick_thread(&mut self, id: String, cx: &mut Context<Self>) {
        self.site_edit(cx, |s| match s.picking.take() {
            Some(Pick::FrontPage) => s.form.front_page = Some(id),
            Some(Pick::MenuThread) => {
                if let Some(t) = s.form.links_threads.as_mut()
                    && !t.contains(&id)
                    && t.len() < site::MAX_LINKS
                {
                    t.push(id);
                }
            }
            None => {}
        });
    }

    /// Move a main-menu thread up (`-1`) or down (`1`).
    pub(crate) fn site_move_thread(&mut self, i: usize, by: isize, cx: &mut Context<Self>) {
        self.site_edit(cx, |s| {
            if let Some(t) = s.form.links_threads.as_mut() {
                let j = i as isize + by;
                if j >= 0 && (j as usize) < t.len() && i < t.len() {
                    t.swap(i, j as usize);
                }
            }
        });
    }

    /// Upload an image as the avatar (it's set on Save).
    pub(crate) fn site_upload_avatar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .site
            .sheet
            .as_ref()
            .is_none_or(|s| s.uploading || s.busy)
        {
            return;
        }
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Upload".into()),
        });
        let backend = self.backend.clone();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let Some((mime, format)) = image_kind(&path) else {
                let _ = this.update(cx, |v, cx| {
                    v.site_edit(cx, |s| {
                        s.error = Some("An avatar is a PNG, JPEG, GIF, WebP or SVG image".into())
                    });
                });
                return;
            };
            let _ = this.update(cx, |v, cx| v.site_edit(cx, |s| s.uploading = true));
            let up = cx
                .background_spawn(async move {
                    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                    let m = backend
                        .upload_media(bytes.clone(), mime, None, None)
                        .map_err(|e| e.to_string())?;
                    let id = blyg_core::api::media::media_id(&m.url)
                        .ok_or_else(|| "the blyg didn't say the image's id".to_string())?;
                    Ok::<_, String>((id, bytes))
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                if let Some(s) = v.site.sheet.as_mut() {
                    s.uploading = false;
                    match up {
                        Ok((id, bytes)) => {
                            s.form.avatar_media_id = Some(id);
                            s.avatar = Some(Arc::new(Image::from_bytes(format, bytes)));
                            s.saved = false;
                        }
                        Err(e) => s.error = Some(format!("Couldn't upload it: {e}")),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The form with the fields' text in, or why it can't be sent.
    fn site_gather(&self, cx: &App) -> Result<SiteConfig, String> {
        let s = self.site.sheet.as_ref().ok_or("no sheet")?;
        let mut f = s.form.clone();
        let text = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let put = |slot: &mut Option<String>, v: String| {
            if slot.is_some() {
                *slot = Some(v);
            }
        };
        put(&mut f.author_url, text(&s.author_url));
        put(&mut f.site_url, text(&s.site_url));
        put(&mut f.posts_path, text(&s.posts_path).to_ascii_lowercase());
        put(&mut f.custom_theme_name, text(&s.theme_name));
        put(&mut f.update_feed_url, text(&s.feed_url));
        if f.links_external.is_some() {
            f.links_external = Some(crate::app::reading::vm::parse_links(
                &s.external.read(cx).value(),
            )?);
        }
        // The colours as typed, so a bad one is caught here.
        for (fields, slot) in [
            (&s.light, &mut f.custom_theme),
            (&s.dark, &mut f.custom_theme_dark),
        ] {
            if let Some(Some(t)) = slot.as_mut() {
                for (i, c) in THEME_COLORS.iter().enumerate() {
                    t.set(c, text(&fields[i]).to_ascii_lowercase());
                }
            }
        }
        if let Some(e) = f.error() {
            return Err(e);
        }
        Ok(f)
    }

    /// Save: send what changed. The sheet stays open, saying so.
    pub(crate) fn save_site(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.site.sheet.as_ref() else {
            return;
        };
        if s.load != Load::Ready || s.busy || s.uploading {
            return;
        }
        let next = match self.site_gather(cx) {
            Ok(n) => n,
            Err(e) => {
                if let Some(s) = self.site.sheet.as_mut() {
                    s.error = Some(e);
                }
                cx.notify();
                return;
            }
        };
        let base = s.base.clone();
        if next.patch(&base).is_empty() {
            if let Some(s) = self.site.sheet.as_mut() {
                s.saved = true;
                s.error = None;
            }
            cx.notify();
            return;
        }
        if let Some(s) = self.site.sheet.as_mut() {
            s.busy = true;
            s.error = None;
        }
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.save_site_config(&base, &next) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                let Some(s) = v.site.sheet.as_mut() else {
                    return;
                };
                s.busy = false;
                match r {
                    Ok(saved) => {
                        Self::site_fill(s, &saved, window, cx);
                        s.base = saved.clone();
                        s.form = saved;
                        s.saved = true;
                        v.show_toast(
                            "Site saved",
                            Some("Your blyg's public pages show it now".into()),
                            cx,
                        );
                    }
                    Err(e) => s.error = Some(refusal(&e)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `BLYGGER_DEMO=site-<section>` (`site-custom`: a custom theme with a
    /// dark companion; `site-notice`: just the status bar's update notice,
    /// with `BLYGGER_FAKE_STUDIO_LATEST` set).
    pub(crate) fn site_demo(
        &mut self,
        s: &str,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (s, n) {
            ("site-notice", _) => {}
            (_, 0) => self.open_site(window, cx),
            (_, 1) => {
                let section = match s {
                    "site-home" => Section::HomePage,
                    "site-links" => Section::Links,
                    "site-profile" => Section::Profile,
                    "site-network" => Section::Network,
                    "site-updates" => Section::Updates,
                    "site-extensions" => Section::Extensions,
                    _ => Section::Appearance,
                };
                if let Some(sh) = self.site.sheet.as_mut() {
                    sh.section = section;
                }
                if s == "site-custom" {
                    self.site_choose_theme(site::CUSTOM_THEME_ID, window, cx);
                    self.site_toggle_dark_companion(window, cx);
                    self.site_edit(cx, |sh| sh.editing_dark = false);
                }
                if s == "site-links" {
                    let first = published_threads(&self.backend.items()).into_iter().next();
                    if let Some((id, _)) = first {
                        self.site_edit(cx, |sh| sh.form.links_threads = Some(vec![id]));
                    }
                }
                cx.notify();
            }
            _ => {}
        }
    }

    /// The status bar's quiet notice while the blyg's studio has a newer
    /// release (and its update alerts are on).
    pub(crate) fn render_studio_update_notice(&self) -> Option<AnyElement> {
        let u = self.backend.studio_update().filter(|u| u.behind())?;
        let muted = self.palette.on_status_text(self.palette.muted);
        let link = self.palette.on_status_text(self.palette.accent);
        Some(
            div()
                .id("studio-update-notice")
                .flex()
                .items_center()
                .gap(px(6.))
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        .truncate()
                        .text_color(muted)
                        .child(format!("Your blyg's studio has an update: {}", u.latest)),
                )
                .child("·")
                .child(
                    div()
                        .id("studio-update-link")
                        .cursor_pointer()
                        .text_color(link)
                        .hover(|s| s.underline())
                        .child("Release notes")
                        .on_click(|_, _, cx| cx.open_url(site::STUDIO_RELEASES_URL)),
                )
                .into_any_element(),
        )
    }
}

/// A refused save, in words: the server's own message when it gave one.
pub(crate) fn refusal(e: &blyg_core::CoreError) -> String {
    match e {
        blyg_core::CoreError::Rejected { message, .. } if !message.is_empty() => {
            format!("Your blyg refused it: {message}")
        }
        e => format!("Couldn't save: {e}"),
    }
}

/// An image file's MIME type and GPUI format, by its extension.
fn image_kind(path: &std::path::Path) -> Option<(&'static str, ImageFormat)> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => ("image/png", ImageFormat::Png),
        "jpg" | "jpeg" => ("image/jpeg", ImageFormat::Jpeg),
        "gif" => ("image/gif", ImageFormat::Gif),
        "webp" => ("image/webp", ImageFormat::Webp),
        "svg" => ("image/svg+xml", ImageFormat::Svg),
        _ => return None,
    })
}
