//! --- capture --- Capture from anywhere (proposal 12): the quick-capture
//! panel arrives already holding what you were looking at.
//!
//! - macOS: a Services entry, "Capture in Burrow" (`NSServices` in the
//!   bundle's Info.plist, `packaging/Info.plist.in`). macOS hands the
//!   provider registered here the selected text, and from a browser
//!   sometimes the page's address; the panel opens with them as a quote
//!   block. No Accessibility permission is involved: Services pass only
//!   what the user selected and chose to send.
//! - Windows: the clipboard's text, when it changed in the last ten
//!   seconds, is offered in the panel as a chip ("quote clipboard"),
//!   never inserted silently.

use std::time::{Duration, Instant};

use gpui_kit::*;

use crate::app::reading::vm::{QuoteSource, quote_block};

/// How long a clipboard change stays on offer (Windows).
pub const CLIP_FRESH: Duration = Duration::from_secs(10);

/// The Markdown the panel opens with for `text` selected at `url`: a
/// blockquote, the address under it (`> — [host](url)`). `None` for an
/// empty selection.
pub fn service_quote(text: &str, url: Option<&str>) -> Option<String> {
    let url = url
        .map(str::trim)
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"));
    let title = url
        .and_then(crate::vm::url_host)
        .map(|h| h.strip_prefix("www.").unwrap_or(&h).to_string())
        .unwrap_or_default();
    quote_block(
        text,
        &QuoteSource::Page {
            title,
            url: url.map(str::to_string),
        },
        false,
    )
}

/// What the panel holds after a capture arrives: the quote alone, or
/// after what was already typed.
pub fn with_capture(existing: &str, quote: &str) -> String {
    let e = existing.trim_end();
    if e.is_empty() {
        format!("{quote}\n\n")
    } else {
        format!("{e}\n\n{quote}\n\n")
    }
}

/// The clipboard's text as last seen, and when it last changed (Windows).
#[derive(Debug, Default)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub struct ClipWatch {
    seen: bool,
    last: Option<String>,
    changed: Option<Instant>,
}

impl Global for ClipWatch {}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
impl ClipWatch {
    /// The clipboard reads `text` at `now`. The first reading is only the
    /// baseline: what was on the clipboard before is nothing new.
    pub fn observe(&mut self, text: Option<String>, now: Instant) {
        let text = text.filter(|t| !t.trim().is_empty());
        if !self.seen {
            self.seen = true;
            self.last = text;
        } else if text != self.last {
            self.changed = text.as_ref().map(|_| now);
            self.last = text;
        }
    }

    /// The text to offer at `now`: changed within [`CLIP_FRESH`].
    pub fn offer(&self, now: Instant) -> Option<String> {
        let at = self.changed?;
        (now.saturating_duration_since(at) <= CLIP_FRESH)
            .then(|| self.last.clone())
            .flatten()
    }
}

/// The clipboard text on offer now, if any (always `None` off Windows).
pub fn clipboard_offer(cx: &App) -> Option<String> {
    cx.try_global::<ClipWatch>()?.offer(Instant::now())
}

/// Start whatever this platform captures with: the Services provider on
/// macOS, the clipboard watch on Windows.
pub fn init(cx: &mut App) {
    let (tx, rx) = async_channel::unbounded::<(String, Option<String>)>();
    #[cfg(target_os = "macos")]
    mac::register(tx);
    #[cfg(not(target_os = "macos"))]
    drop(tx);
    cx.spawn(async move |cx| {
        while let Ok((text, url)) = rx.recv().await {
            let Some(q) = service_quote(&text, url.as_deref()) else {
                continue;
            };
            cx.update(|cx| crate::capture::open_with(&q, cx));
        }
    })
    .detach();
    #[cfg(target_os = "windows")]
    {
        cx.set_global(ClipWatch::default());
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(1000))
                    .await;
                cx.update(|cx| {
                    let text = cx.read_from_clipboard().and_then(|i| i.text());
                    cx.global_mut::<ClipWatch>().observe(text, Instant::now());
                });
            }
        })
        .detach();
    }
}

#[cfg(target_os = "macos")]
mod mac {
    //! The `NSServices` provider: AppKit calls `captureInBurrow:userData:
    //! error:` on the main thread with the sender's pasteboard.

    use std::sync::OnceLock;

    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{ClassType, class, define_class, msg_send};
    use objc2_foundation::NSString;

    static TX: OnceLock<async_channel::Sender<(String, Option<String>)>> = OnceLock::new();

    fn string_for(pboard: &AnyObject, ty: &str) -> Option<String> {
        let ty = NSString::from_str(ty);
        let s: Option<Retained<NSString>> = unsafe { msg_send![pboard, stringForType: &*ty] };
        s.map(|s| s.to_string())
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements, and this class
        // has no ivars and doesn't implement Drop.
        #[unsafe(super(NSObject))]
        #[name = "BurrowCaptureServiceProvider"]
        struct Provider;

        impl Provider {
            /// The Info.plist's `NSMessage` (`captureInBurrow`).
            #[unsafe(method(captureInBurrow:userData:error:))]
            fn capture(
                &self,
                pboard: &AnyObject,
                _user_data: *mut AnyObject,
                _error: *mut *mut AnyObject,
            ) {
                let text = string_for(pboard, "public.utf8-plain-text")
                    .or_else(|| string_for(pboard, "NSStringPboardType"));
                let url = string_for(pboard, "public.url");
                if let (Some(text), Some(tx)) = (text, TX.get()) {
                    let _ = tx.try_send((text, url));
                }
            }
        }
    );

    pub fn register(tx: async_channel::Sender<(String, Option<String>)>) {
        if TX.set(tx).is_err() {
            return;
        }
        unsafe {
            let provider: Retained<Provider> = msg_send![Provider::class(), new];
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            if app.is_null() {
                return;
            }
            let _: () = msg_send![app, setServicesProvider: &*provider];
            // AppKit doesn't promise to keep it: it lives as long as the app.
            std::mem::forget(provider);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{CLIP_FRESH, ClipWatch, service_quote, with_capture};

    #[test]
    fn a_selection_becomes_a_quote_with_its_address() {
        assert_eq!(
            service_quote(
                "Scarcity with a schedule.",
                Some("https://www.lin.example.org/t/rock-pools")
            )
            .unwrap(),
            "> Scarcity with a schedule.\n>\n> — [lin.example.org](https://www.lin.example.org/t/rock-pools)"
        );
        // No address (an email, a note): the quote alone.
        assert_eq!(
            service_quote("Low at noon.\nHigh at six.", None).unwrap(),
            "> Low at noon.\n>\n> High at six."
        );
        // Not a web address: left out.
        assert_eq!(service_quote("x", Some("file:///Users/x")).unwrap(), "> x");
        assert_eq!(service_quote("   ", None), None);
    }

    #[test]
    fn a_capture_goes_after_what_was_typed() {
        assert_eq!(with_capture("", "> q"), "> q\n\n");
        assert_eq!(with_capture("Thought\n", "> q"), "Thought\n\n> q\n\n");
    }

    #[test]
    fn only_a_fresh_clipboard_change_is_offered() {
        let t0 = Instant::now();
        let mut w = ClipWatch::default();
        // What was there before is the baseline, not news.
        w.observe(Some("old".into()), t0);
        assert_eq!(w.offer(t0), None);
        w.observe(Some("old".into()), t0 + Duration::from_secs(1));
        assert_eq!(w.offer(t0 + Duration::from_secs(1)), None);
        // A copy: offered for ten seconds.
        let t1 = t0 + Duration::from_secs(2);
        w.observe(Some("Scarcity with a schedule.".into()), t1);
        assert_eq!(w.offer(t1).as_deref(), Some("Scarcity with a schedule."));
        assert!(w.offer(t1 + CLIP_FRESH).is_some());
        assert_eq!(w.offer(t1 + CLIP_FRESH + Duration::from_secs(1)), None);
        // Emptied: nothing on offer.
        w.observe(None, t1 + Duration::from_secs(3));
        assert_eq!(w.offer(t1 + Duration::from_secs(3)), None);
    }
}
