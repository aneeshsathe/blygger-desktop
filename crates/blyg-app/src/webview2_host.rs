//! Hosting a WebView2 inside GPUI's window, shared by the preview
//! (`studio/webview.rs`) and the browser pane (`browser/surface.rs`).
//! Windows only.
//!
//! Creating a WebView2 waits for it in a nested message loop
//! (webview2-com's `wait_with_pump`). GPUI asks for a surface while it draws
//! a frame, with the app borrowed, and a GPUI task or event that the nested
//! loop dispatches then borrows the app again and panics, which aborts the
//! process. So the WebView is built later, from a Win32 thread timer that
//! GPUI's top-level message loop dispatches while nothing is borrowed
//! ([`defer`]); until then a [`Deferred`] records what it is asked to do and
//! replays it once the view exists.
//!
//! GPUI normally draws through a topmost DirectComposition visual, which
//! would cover any child window; `main` turns that off on Windows
//! (`GPUI_DISABLE_DIRECT_COMPOSITION`) so these views show above the app.

use std::cell::{Cell, RefCell};
use std::num::NonZeroIsize;
use std::rc::{Rc, Weak};

use gpui_kit::Window;
use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};

#[link(name = "user32")]
unsafe extern "system" {
    fn SetTimer(
        hwnd: isize,
        id: usize,
        elapse_ms: u32,
        timer_proc: Option<unsafe extern "system" fn(isize, u32, usize, u32)>,
    ) -> usize;
    fn KillTimer(hwnd: isize, id: usize) -> i32;
}

type Job = Box<dyn FnOnce()>;

thread_local! {
    static JOBS: RefCell<Vec<Job>> = const { RefCell::new(Vec::new()) };
    static TIMER: Cell<usize> = const { Cell::new(0) };
}

unsafe extern "system" fn run_jobs(_: isize, _: u32, id: usize, _: u32) {
    // SAFETY: a thread timer this module set; killing it is always valid.
    unsafe { KillTimer(0, id) };
    TIMER.with(|t| t.set(0));
    let jobs = JOBS.with(|j| std::mem::take(&mut *j.borrow_mut()));
    for job in jobs {
        job();
    }
}

/// Run `job` soon, from the top-level message loop.
pub fn defer(job: Job) {
    JOBS.with(|j| j.borrow_mut().push(job));
    if TIMER.with(Cell::get) == 0 {
        // SAFETY: a plain thread timer with a static callback.
        let id = unsafe { SetTimer(0, 0, 1, Some(run_jobs)) };
        TIMER.with(|t| t.set(id));
    }
}

/// GPUI's window, by its HWND, for building a child WebView later.
pub struct Hwnd(NonZeroIsize);

impl Hwnd {
    /// The HWND of GPUI's window (`None` when it has none, which never
    /// happens on Windows).
    pub fn of(window: &Window) -> Option<Hwnd> {
        match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => Some(Hwnd(h.hwnd)),
            _ => None,
        }
    }
}

impl HasWindowHandle for Hwnd {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let raw = RawWindowHandle::Win32(Win32WindowHandle::new(self.0));
        // SAFETY: GPUI's window HWND. If the window has closed since, the
        // build fails, and that is handled.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

/// A view (`V`) that doesn't exist yet, and what it was asked to do
/// meanwhile (`P`).
pub struct Slot<V, P> {
    pub view: Option<V>,
    pub pending: P,
}

/// A WebView2 surface built outside GPUI's frame (see the module docs):
/// `view` is `None` until the deferred build has run (and stays `None` if
/// it failed); `pending` holds the requests made before.
pub struct Deferred<V, P>(Rc<RefCell<Slot<V, P>>>);

impl<V: 'static, P: Default + 'static> Deferred<V, P> {
    /// Schedule `build` against `window`'s HWND. On success `replay` gets
    /// the new view and the recorded requests, then the view is kept; on
    /// failure `failed` gets the requests and the message. Nothing runs if
    /// the `Deferred` was dropped first. `Err` only when the window has no
    /// HWND (`missing` is the sentence for that).
    pub fn start(
        window: &Window,
        missing: &str,
        build: impl FnOnce(&Hwnd) -> Result<V, String> + 'static,
        replay: impl FnOnce(&mut V, &mut P) + 'static,
        failed: impl FnOnce(&mut P, String) + 'static,
    ) -> Result<Self, String> {
        let hwnd = Hwnd::of(window).ok_or_else(|| missing.to_string())?;
        let slot = Rc::new(RefCell::new(Slot {
            view: None,
            pending: P::default(),
        }));
        let weak: Weak<RefCell<Slot<V, P>>> = Rc::downgrade(&slot);
        defer(Box::new(move || {
            if weak.upgrade().is_none() {
                return; // the pane went away first
            }
            let built = build(&hwnd);
            let Some(slot) = weak.upgrade() else {
                return;
            };
            let mut s = slot.borrow_mut();
            match built {
                Ok(mut view) => {
                    replay(&mut view, &mut s.pending);
                    s.view = Some(view);
                }
                Err(msg) => failed(&mut s.pending, msg),
            }
        }));
        Ok(Deferred(slot))
    }

    /// The slot, borrowed for `f`.
    pub fn with<R>(&self, f: impl FnOnce(&mut Slot<V, P>) -> R) -> R {
        f(&mut self.0.borrow_mut())
    }
}
