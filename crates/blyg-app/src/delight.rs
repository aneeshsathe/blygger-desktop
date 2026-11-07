//! --- delight --- Small motion, optional sound, nothing that counts
//! (Fable's proposal 13).
//!
//! Motion, every one under 300 ms: a published post's row lifts and settles
//! in the list; the sync dot's colour eases from one state to the next
//! (amber → green) instead of switching, and still breathes while syncing;
//! the counter's colour eases as a fragment nears or passes 1000; the list
//! crossfades when you switch blygs; focus mode's hairline slides in. With
//! macOS's Reduce Motion on (System Settings › Accessibility › Display),
//! every one of them is a cut.
//!
//! Sound (`sounds = off | quiet`, off by default): a quiet macOS system
//! sound (NSSound, by name; nothing is bundled or downloaded) when a post
//! is published and when quick capture keeps a note. Never for typing. No
//! sound on Windows. No streaks, no confetti, no counts.
//!
//! A child module of `app`; `app.rs` keeps one-line hooks marked
//! `// --- delight ---`.

use std::cell::RefCell;

use super::*;

/// How long a colour eases.
pub const EASE_MS: u64 = 240;
/// How long a published row takes to settle.
pub const SETTLE_MS: u64 = 280;
/// How long the list's crossfade takes.
pub const FADE_MS: u64 = 220;

/// What a sound is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    /// A post was published.
    Publish,
    /// Quick capture kept a note (scratch or draft).
    Capture,
}

impl Sound {
    /// The macOS system sound (in /System/Library/Sounds) it plays.
    pub fn system_name(self) -> &'static str {
        match self {
            Sound::Publish => "Pop",
            Sound::Capture => "Tink",
        }
    }
}

/// The quiet volume, of the alert volume.
pub const QUIET: f32 = 0.35;

/// `sounds`: whether to play them (off unless `quiet`).
pub fn sounds_on(cx: &App) -> bool {
    cx.try_global::<crate::settings::AppConfig>()
        .is_some_and(|c| c.store.config().get("sounds") == Some("quiet"))
}

/// Play `sound` when `sounds = quiet` (macOS only; tests never play).
pub fn play(sound: Sound, cx: &App) {
    if !sounds_on(cx) || cfg!(test) {
        return;
    }
    play_named(sound.system_name(), QUIET);
}

#[cfg(target_os = "macos")]
fn play_named(name: &str, volume: f32) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;
    let name = NSString::from_str(name);
    // SAFETY: NSSound's class methods and instance methods with their
    // documented types; a missing sound is nil, checked.
    unsafe {
        let s: *mut AnyObject = msg_send![class!(NSSound), soundNamed: &*name];
        if s.is_null() {
            return;
        }
        let _: () = msg_send![s, setVolume: volume];
        let _: bool = msg_send![s, play];
    }
}

#[cfg(not(target_os = "macos"))]
fn play_named(_: &str, _: f32) {}

/// macOS's Reduce Motion (or `BLYGGER_REDUCE_MOTION=1`): every animation is
/// a cut. Read at most every two seconds.
pub fn reduce_motion() -> bool {
    use std::sync::Mutex;
    use std::time::{Duration as D, Instant};
    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    if let Some(v) = std::env::var_os("BLYGGER_REDUCE_MOTION") {
        return v != "0";
    }
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, v)) = *c
        && at.elapsed() < D::from_secs(2)
    {
        return v;
    }
    let v = system_reduce_motion();
    *c = Some((Instant::now(), v));
    v
}

#[cfg(all(target_os = "macos", not(test)))]
fn system_reduce_motion() -> bool {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    // SAFETY: NSWorkspace.sharedWorkspace and its BOOL property (10.12+).
    unsafe {
        let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if ws.is_null() {
            return false;
        }
        msg_send![ws, accessibilityDisplayShouldReduceMotion]
    }
}

#[cfg(any(not(target_os = "macos"), test))]
fn system_reduce_motion() -> bool {
    false
}

/// `from` → `to` at `t` (0…1), channel by channel.
pub fn mix(from: Hsla, to: Hsla, t: f32) -> Hsla {
    let (a, b) = (Rgba::from(from), Rgba::from(to));
    let l = |x: f32, y: f32| x + (y - x) * t.clamp(0., 1.);
    Hsla::from(Rgba {
        r: l(a.r, b.r),
        g: l(a.g, b.g),
        b: l(a.b, b.b),
        a: l(a.a, b.a),
    })
}

/// A colour that eases when it changes: what it was, what it is, and a
/// generation that names the change's animation.
#[derive(Debug, Clone, Copy, Default)]
struct Eased {
    from: Option<Hsla>,
    now: Option<Hsla>,
    seq: usize,
}

impl Eased {
    fn track(&mut self, c: Hsla) -> (Hsla, usize) {
        match self.now {
            Some(n) if n == c => {}
            Some(n) => {
                self.from = Some(n);
                self.now = Some(c);
                self.seq += 1;
            }
            None => {
                self.from = Some(c);
                self.now = Some(c);
            }
        }
        (self.from.unwrap_or(c), self.seq)
    }
}

#[derive(Default)]
struct Track {
    dot: Eased,
    counter: Eased,
    /// The blyg the list was drawn for, and the crossfade's generation.
    blyg: Option<String>,
    list_gen: usize,
}

#[derive(Default)]
pub(crate) struct Delight {
    /// The row that was just published, and its settle's generation.
    settle: Option<(LocalId, usize)>,
    seq: usize,
    track: RefCell<Track>,
}

impl Delight {
    /// A post was published: its row settles.
    pub fn published(&mut self, id: &LocalId) {
        self.seq += 1;
        self.settle = Some((id.clone(), self.seq));
    }
}

impl MainView {
    /// Hook (publish succeeded): the row settles, and a quiet sound.
    pub(super) fn delight_published(&mut self, id: &LocalId, cx: &mut Context<Self>) {
        self.delight.published(id);
        play(Sound::Publish, cx);
    }

    /// Hook (a list row): a just-published post's row lifts and settles.
    pub(super) fn delight_row(&self, el: AnyElement, id: &LocalId) -> AnyElement {
        match &self.delight.settle {
            Some((sid, gen_)) if sid == id && !reduce_motion() => div()
                .relative()
                .child(el)
                .with_animation(
                    ("row-settle", *gen_),
                    Animation::new(Duration::from_millis(SETTLE_MS)).with_easing(ease_out_quint()),
                    |d, t| {
                        // Up 6 px in the first third, then down to rest.
                        let lift = if t < 0.33 {
                            t / 0.33
                        } else {
                            1. - (t - 0.33) / 0.67
                        };
                        d.top(px(-6. * lift))
                    },
                )
                .into_any_element(),
            _ => el,
        }
    }

    /// Hook (the status bar's dot): its colour eases between states.
    pub(super) fn delight_dot(&self, dot: Div, color: Hsla) -> AnyElement {
        let (from, gen_) = self.delight.track.borrow_mut().dot.track(color);
        if gen_ == 0 || reduce_motion() {
            return dot.into_any_element();
        }
        dot.with_animation(
            ("dot-ease", gen_),
            Animation::new(Duration::from_millis(EASE_MS)).with_easing(ease_in_out),
            move |d, t| d.bg(mix(from, color, t)),
        )
        .into_any_element()
    }

    /// Hook (the counter): its colour eases, never snaps.
    pub(super) fn delight_counter(&self, counter: Div, color: Hsla) -> AnyElement {
        let (from, gen_) = self.delight.track.borrow_mut().counter.track(color);
        let counter = counter.text_color(color);
        if gen_ == 0 || reduce_motion() {
            return counter.into_any_element();
        }
        counter
            .with_animation(
                ("counter-ease", gen_),
                Animation::new(Duration::from_millis(EASE_MS)).with_easing(ease_in_out),
                move |d, t| d.text_color(mix(from, color, t)),
            )
            .into_any_element()
    }

    /// Hook (the list): it crossfades when the blyg changes.
    pub(super) fn delight_list(&self, list: AnyElement) -> AnyElement {
        let gen_ = {
            let mut t = self.delight.track.borrow_mut();
            match &t.blyg {
                Some(b) if *b == self.title => {}
                Some(_) => {
                    t.blyg = Some(self.title.clone());
                    t.list_gen += 1;
                }
                None => t.blyg = Some(self.title.clone()),
            }
            t.list_gen
        };
        let wrap = div().size_full().child(list);
        if gen_ == 0 || reduce_motion() {
            return wrap.into_any_element();
        }
        wrap.with_animation(
            ("list-fade", gen_),
            Animation::new(Duration::from_millis(FADE_MS)).with_easing(ease_out_quint()),
            |d, t| d.opacity(t),
        )
        .into_any_element()
    }

    /// The crossfade's generation (tests).
    #[cfg(test)]
    pub(crate) fn delight_list_gen(&self) -> usize {
        self.delight.track.borrow().list_gen
    }

    /// The settling row (tests).
    #[cfg(test)]
    pub(crate) fn delight_settling(&self) -> Option<LocalId> {
        self.delight.settle.as_ref().map(|(id, _)| id.clone())
    }

    /// Settings' row: SOUNDS, `sounds = off | quiet`.
    pub(super) fn set_sounds(&mut self, on: bool, cx: &mut Context<Self>) {
        let v = if on { "quiet" } else { "off" };
        if let Err(e) =
            crate::settings::write(&[("sounds", blyg_core::config::Change::Set(v.into()))], cx)
        {
            self.show_toast(e, None, cx);
            return;
        }
        self.watch_stamp = crate::settings::watch_stamp(&crate::settings::watched(cx));
        if on {
            play(Sound::Publish, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Eased, QUIET, Sound, mix};
    use gpui_kit::{Rgba, hsla};

    #[test]
    fn colours_mix_and_ease_only_on_change() {
        let a = hsla(0., 0., 0., 1.);
        let b = hsla(0., 0., 1., 1.);
        let mid = Rgba::from(mix(a, b, 0.5));
        assert!((mid.r - 0.5).abs() < 0.01, "{mid:?}");
        assert_eq!(Rgba::from(mix(a, b, 1.)), Rgba::from(b));
        let mut e = Eased::default();
        assert_eq!(e.track(a).1, 0, "the first colour is a cut");
        assert_eq!(e.track(a).1, 0);
        let (from, g) = e.track(b);
        assert_eq!((Rgba::from(from), g), (Rgba::from(a), 1));
        assert_eq!(e.track(b).1, 1, "the same colour again: no new ease");
    }

    #[test]
    fn sounds_are_system_sounds_by_name() {
        assert_eq!(Sound::Publish.system_name(), "Pop");
        assert_eq!(Sound::Capture.system_name(), "Tink");
        const { assert!(QUIET < 0.5) };
    }
}
