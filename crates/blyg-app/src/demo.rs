//! `BLYGGER_DEMO=<scenario>`: scripted, in-process walkthroughs (like the
//! mock's ▶ demos) used for screenshots and smoke-testing without synthetic OS
//! input. They call the same methods a keystroke would.

use std::time::Duration;

use blyg_core::LocalId;
use gpui_kit::*;

use super::MainView;
use crate::scenes::{Area, Scene};

impl MainView {
    pub fn run_demo(&mut self, scenario: Scene, window: &mut Window, cx: &mut Context<Self>) {
        // --- themes --- `BLYGGER_SNAPSHOT` (a `blygger_snap` build): a fixed
        // window size, and a frame saved after the last step whatever the
        // scenario (the theme audit renders offscreen, so a locked screen or
        // a window macOS won't show doesn't matter).
        let snapshot = crate::dev_flags::get().snapshot.path.is_some();
        if snapshot {
            window.resize(size(px(1100.), px(720.)));
        } else if let Some(w) = crate::dev_flags::get().snapshot.width {
            // `BLYGGER_SNAPSHOT_WIDTH` / `_HEIGHT` on a live scene: play it in
            // a window of that size (a narrow-window UX pass).
            let h = crate::dev_flags::get().snapshot.height.unwrap_or(720.);
            window.resize(size(px(w), px(h)));
        }
        cx.spawn_in(window, async move |this, cx| {
            let exec = cx.background_executor().clone();
            let step = |ms: u64| exec.timer(Duration::from_millis(ms));
            step(400).await;
            let _ = this.update_in(cx, |v, window, cx| v.demo_step(scenario, 0, window, cx));
            step(900).await;
            let _ = this.update_in(cx, |v, window, cx| v.demo_step(scenario, 1, window, cx));
            step(1200).await;
            let _ = this.update_in(cx, |v, window, cx| v.demo_step(scenario, 2, window, cx));
            // --- blyg switcher --- a window macOS isn't showing (screenshot
            // runs never activate) may not redraw by itself: draw it a few
            // times so a window capture shows the last step.
            if matches!(scenario.area(), Area::AccessAi | Area::Site)
                || matches!(
                    scenario,
                    Scene::Switcher | Scene::Switched | Scene::SwitcherManage
                )
            {
                for _ in 0..4 {
                    step(300).await;
                    let _ = cx.update(|window, cx| window.draw(cx).clear(cx));
                }
            }
            // About and the capture panels save their own frame.
            let own_frame = matches!(scenario, Scene::About | Scene::Capture | Scene::TbCapture);
            if snapshot && !own_frame {
                step(600).await;
                let _ = this.update_in(cx, |_, window, cx| {
                    super::reading::demo::snapshot_later(window, cx)
                });
            }
            // The readiness signal scripts/verify/burrow-verify.sh waits for:
            // every step has run (a scene's own follow-up animation may
            // still be finishing).
            eprintln!("blygger: demo {scenario} ready");
        })
        .detach();
    }

    pub(super) fn demo_type(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |s, cx| s.insert(text.to_string(), window, cx));
        self.after_edit(cx);
    }

    fn demo_open(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.open(&LocalId(id.into()), window, cx);
    }

    /// One step of a scene: its area's handler (`scenes.rs` says which).
    pub(crate) fn demo_step(
        &mut self,
        scene: Scene,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match scene.area() {
            Area::Main => self.main_demo_step(scene, n, window, cx),
            Area::Upkeep => self.upkeep_demo_step(scene, n, window, cx),
            Area::Ai => self.ai_demo_step(scene, n, window, cx),
            Area::Reading => self.reading_demo(scene, n, window, cx),
            Area::Profiles => self.profile_demo(scene, n, window, cx),
            Area::Onboarding => self.onboarding_demo(scene, n, window, cx),
            Area::Composer => self.composer_demo(scene, n, window, cx),
            Area::Toolbar => self.toolbar_demo(scene, n, window, cx),
            Area::Browser => self.browser_demo(scene, n, window, cx),
            Area::Notes => self.notes_demo(scene, n, window, cx),
            Area::Side => self.side_demo(scene, n, window, cx),
            Area::Extensions => self.ext_demo(scene, n, window, cx),
            Area::AccessAi => self.access_ai_demo(scene, n, window, cx),
            Area::Site => self.site_demo(scene, n, window, cx),
            Area::Contacts => self.contacts_demo(scene, n, window, cx),
        }
    }

    /// The scenes `demo.rs` plays itself.
    fn main_demo_step(
        &mut self,
        scenario: Scene,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (scenario, n) {
            (Scene::RdTangle, n) => self.tangle_demo(n, true, window, cx), // --- tangle ---
            (Scene::RdTanglePreview, n) => self.tangle_demo(n, false, window, cx),
            // Live filter with highlighted matches, ↓ previews the next row.
            (Scene::Search, 0) => self.set_query_text("the", window, cx),
            (Scene::Search, 1) => self.move_selection(1, window, cx),
            // ⏎ with no match creates a draft seeded with the query.
            (Scene::Create, 0) => self.set_query_text("tide pools", window, cx),
            (Scene::Create, 1) => {
                let seed = self.list.trimmed_query().to_string();
                self.create_from(&seed, window, cx);
                self.demo_type(
                    " are small oceans that forget, twice a day, that they belong to a larger one.",
                    window,
                    cx,
                );
            }
            // --- new post --- ⌘N over a search: an empty editor, the list
            // as it was; then the first words make it a scratch note.
            (Scene::NewPost | Scene::NewPostTyped, 0) => self.set_query_text("the", window, cx),
            (Scene::NewPost | Scene::NewPostTyped, 1) => self.start_new_post(window, cx),
            (Scene::NewPostTyped, 2) => {
                self.editor.update(cx, |s, cx| {
                    s.insert(
                        "Tide pools are small oceans that forget, twice a day.".to_string(),
                        window,
                        cx,
                    )
                });
                self.new_post_first_keystroke(cx);
                self.after_edit(cx);
            }
            (Scene::Publish, 0) => {
                self.demo_open("01J9QK3", window, cx);
                self.demo_type(" So is a good notebook.", window, cx);
            }
            (Scene::Publish, 1) => {
                self.publish(window, cx);
                if let Some(super::PublishSheet { note, .. }) = self.sheet.get() {
                    note.update(cx, |s, cx| s.set_value("first version", window, cx));
                }
            }
            (Scene::Published, 0) => self.demo_open("01J9QK3", window, cx),
            (Scene::Published, 1) => {
                self.publish(window, cx);
                self.do_publish("first version".into(), window, cx);
            }
            (Scene::Long, 0) => {
                self.demo_open("01J9M2A", window, cx);
                let extra = " And the cost is not only time. Each extra field, each login screen, each preview that takes a second to render asks the same question: is this thought worth the trouble? Most thoughts, asked that way, say no.".repeat(5);
                self.demo_type(&extra, window, cx);
            }
            (Scene::Long, 1) => self.publish(window, cx),
            // --- full editor: ⌘3 on a thread with a quote, TK and a video ---
            (Scene::Studio, 0) => self.studio_demo(window, cx),
            (Scene::StudioEdit, 0) => self.studio_demo(window, cx),
            (Scene::StudioEdit, 1) => self.demo_type(" Tide tables.", window, cx),
            (Scene::Preview, 0) => self.demo_open("01J9K7T", window, cx),
            (Scene::Preview, 1) => self.toggle_preview(&super::TogglePreview, window, cx),
            (Scene::Conflict, 0) => {
                self.demo_open("01J9K7T", window, cx);
                self.editor
                    .update(cx, |s, cx| s.set_selected_range(41..41, cx));
                self.demo_type(" (A. Gardener)", window, cx);
            }
            (Scene::Conflict, 1) => {
                if let (Some(f), Some(cur)) = (&self.fake, &self.current) {
                    f.trigger_conflict(&cur.local_id);
                }
            }
            (Scene::Offline, 0) => {
                self.demo_open("01J9QK3", window, cx);
                if let Some(f) = &self.fake {
                    f.set_offline(true);
                }
            }
            (Scene::Offline, 1) => self.demo_type(" Written on a plane.", window, cx),
            (Scene::Settings, 0) => self.open_settings(&super::OpenSettings, window, cx),
            (Scene::SettingsFull, n) => self.settings_demo(n, window, cx),
            // --- FX1 --- the keys Settings › BACKGROUND takes.
            (Scene::SettingsBackgroundKeys, 2) => {
                self.settings_tab_to(super::pref_preview::Row::Background, window, cx);
                self.settings_preview_key("right", false, window, cx);
            }
            (Scene::SettingsBackgroundKeys, n) => self.settings_demo(n, window, cx),
            // --- ux 0.28 --- a sign-in that can read Background but not
            // place (what `background_placing` says for an owner:read
            // token). Its rules are no tab stops, so no key reaches them
            // (--- ux 0.29 --- ⇥ ×5 and space flipped READ MARKS): the page
            // scrolls to BACKGROUND, greyed with its reason.
            (Scene::SettingsBackgroundLocked, 2) => {
                self.reading.background.placing = false;
                self.settings_show_background(window, cx);
            }
            (Scene::SettingsBackgroundLocked, n) => self.settings_demo(n, window, cx),
            // --- studio 0.71 --- Settings at its end, where READ MARKS'
            // "Count an entry as read" is (the fake blyg offers it).
            (Scene::SettingsCountRead, 2) => self.settings_show_background(window, cx),
            (Scene::SettingsCountRead, n) => self.settings_demo(n, window, cx),
            // --- ux 0.28 --- sync met a blyg on a newer contract.
            (Scene::Newer, 0) => self.demo_open("01J9QK3", window, cx),
            (Scene::Newer, 2) => self.server_newer("7.0.0", cx),
            (Scene::SettingsPreview, n) => self.settings_preview_demo(n, window, cx), // --- preview ---
            // --- themes --- chrome surfaces, for the theme audit's captures.
            // A toast with a second line (shown late, so it's up at capture).
            (Scene::Toast, 0) => self.demo_open("01J9QK3", window, cx),
            (Scene::Toast, 2) => self.show_toast(
                "Published v2",
                Some("blyg.example.com/2026/09/harbour-log".into()),
                cx,
            ),
            // The list's empty state for a query with no match.
            (Scene::Empty, 0) => self.set_query_text("zqxj", window, cx),
            (Scene::Connect, 0) => self.open_connect(window, cx),
            // No blyg and setup skipped: the Connect sheet that opens at
            // launch closed, so the empty list and status bar show.
            (Scene::SkippedSetup, 0) => self.close_sheet(window, cx),
            (Scene::Disconnect, 0) => self.on_disconnect(&super::Disconnect, window, cx),
            // --- blyg switcher --- (a config with two blygs): the switcher
            // open; then switched to the second blyg; then Manage….
            (Scene::Switcher, 0) => self.toggle_blyg_switcher(false, window, cx),
            (Scene::Switched, 0) => self.switch_nth(1, window, cx),
            (Scene::SwitcherManage, 0) => self.toggle_blyg_switcher(true, window, cx),
            // The status bar's update notice (no updater runs in fake mode).
            (Scene::Update, 0) => {
                self.demo_open("01J9QK3", window, cx);
                crate::update::demo_notice(cx);
            }
            // --- about --- (the window, then its snapshot)
            (Scene::About, n) => crate::about::demo(n, cx),
            (Scene::Capture, 0) => crate::capture::toggle(cx),
            (Scene::Capture, 1) => crate::capture::demo_fill(
                "The best interface for writing is the one that is already open.",
                cx,
            ),
            (Scene::Image, 0) => {
                self.demo_open("01J9PX1", window, cx);
                // A small generated PNG stands in for a pasted screenshot.
                let png = demo_png();
                self.start_upload(png, "image/png".into(), window, cx);
            }
            (Scene::Image, 2) => self.toggle_preview(&super::TogglePreview, window, cx),
            (Scene::Edit, 0) => {
                self.set_query_text("harb", window, cx);
            }
            // Against a real (local) blyg: write a new fragment and publish it.
            (Scene::LivePublish, 0) => {
                self.set_query_text("Morning notes", window, cx);
                let seed = self.list.trimmed_query().to_string();
                self.create_from(&seed, window, cx);
                self.demo_type(
                    ": the kettle, the window, the first sentence that arrives before the coffee does.",
                    window,
                    cx,
                );
            }
            (Scene::LivePublish, 1) => {
                self.publish(window, cx);
                self.do_publish("first version".into(), window, cx);
            }
            // The newest post, in the editor (after the initial sync).
            (Scene::LiveOpen, 1) => {
                self.back_to_search(window, cx);
                if let Some(id) = self.list.results().first().map(|i| i.local_id.clone()) {
                    self.open(&id, window, cx);
                }
            }
            (Scene::LiveImage, 0) => {
                if let Some(id) = self.list.results().first().map(|i| i.local_id.clone()) {
                    self.open(&id, window, cx);
                    let png = demo_png();
                    self.start_upload(png, "image/png".into(), window, cx);
                }
            }
            (Scene::LiveImage, 2) => self.toggle_preview(&super::TogglePreview, window, cx),
            // The newest post in the preview: its images come over HTTP
            // through the disk cache (a fresh data dir has none cached).
            (Scene::LivePreview, 1) => {
                if let Some(id) = self.list.results().first().map(|i| i.local_id.clone()) {
                    self.open(&id, window, cx);
                }
            }
            (Scene::LivePreview, 2) => self.toggle_preview(&super::TogglePreview, window, cx),
            // The Connect sheet checking a wrong token against a real blyg
            // (BLYGGER_DEMO_URL; nothing is saved when the check fails).
            (Scene::ConnectCheck, 0) => {
                let url = crate::dev_flags::get().demo.url.clone().unwrap_or_default();
                if let Some(super::ConnectSheet { url: u, token, .. }) = self.sheet.get() {
                    u.update(cx, |s, cx| s.set_value(url, window, cx));
                    token.update(cx, |s, cx| s.set_value("not-the-right-token", window, cx));
                }
                self.submit_connect(window, cx);
            }
            (Scene::Edit, 1) => {
                if let super::EnterAction::Open(id) = self.list.enter() {
                    self.open(&id, window, cx);
                }
                self.demo_type(" Nobody in the crowd noticed.", window, cx);
            }
            _ => {}
        }
    }
}

/// A 600×300 sunset gradient PNG (uncompressed deflate), made at runtime.
fn demo_png() -> Vec<u8> {
    let (w, h) = (600u32, 300u32);
    let mut raw = Vec::with_capacity(((w * 3 + 1) * h) as usize);
    for y in 0..h {
        raw.push(0);
        for x in 0..w {
            let t = y as f32 / h as f32;
            let hill = (x as f32 / 90.0).sin() * 18.0 + 215.0;
            let (r, g, b) = if (y as f32) > hill {
                (0x3d, 0x40, 0x5b)
            } else {
                let dx = x as f32 - 430.0;
                let dy = y as f32 - 120.0;
                if dx * dx + dy * dy < 46.0 * 46.0 {
                    (0xff, 0xf4, 0xd6)
                } else {
                    (
                        (0xf3 as f32 * (1.0 - t) + 0xe0 as f32 * t) as u8,
                        (0xc6 as f32 * (1.0 - t) + 0x7a as f32 * t) as u8,
                        (0x8a as f32 * (1.0 - t) + 0x5f as f32 * t) as u8,
                    )
                }
            };
            raw.extend_from_slice(&[r, g, b]);
        }
    }
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &ihdr);
    // zlib stream with stored (uncompressed) deflate blocks.
    let mut z = vec![0x78, 0x01];
    for (i, block) in raw.chunks(65535).enumerate() {
        let last = (i + 1) * 65535 >= raw.len();
        z.push(last as u8);
        let len = block.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);
    png
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_in = kind.to_vec();
    crc_in.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_in).to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
