//! The browser pane's web view: its own WKWebView on macOS, WebView2 on
//! Windows (both through `wry`), kept apart from the reader and preview
//! web views.
//!
//! Security (it shows arbitrary web pages, unlike the sanitized reader):
//! - no IPC handler and no host script, so the page has no bridge to the
//!   app at all (`window.ipc` doesn't exist), and nothing the app knows (the
//!   owner token, keys) is ever handed to it;
//! - its own website data store: persistent under a fixed identifier on
//!   macOS 14+ (logins survive a relaunch, separately from the reader and
//!   preview, which use the default store), non-persistent before that; on
//!   Windows its own WebView2 user data folder under the app's data folder
//!   (`browser\webview2`), apart from the preview's;
//! - only `http(s)` and `about:blank` navigations (`super::nav_policy`);
//!   `mailto:` goes to the system, everything else (`file:`, `data:`,
//!   `javascript:`, custom schemes) is refused;
//! - downloads are cancelled and handed to the default browser; new-window
//!   requests navigate this view instead.
//!
//! Content blocking (macOS only: the lists compile to WebKit content rules):
//! the compiled rule lists are attached to the view's user content
//! controller, and detached for a host whose shield is off. That's
//! decided when a *main-frame* navigation is about to start, by a small
//! subclass of wry's navigation delegate (`nav_hook`), so the lists are in
//! place before the page's first request.

use async_channel::Sender;
use gpui_kit::{Bounds, Pixels, Window};

/// What the view reports to the pane.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, allow(dead_code))]
pub enum BrowserEvent {
    /// A load started, committed, finished, or the title changed: read
    /// [`PageState`] again.
    Changed,
    /// A link the pane can't show (`mailto:`), or a download: the system
    /// handles it.
    OpenExternally(String),
    /// `target=_blank` / `window.open`: navigate this view.
    NewWindow(String),
    /// The WebContent process died (memory pressure, a crash).
    Crashed,
}

/// What the page is doing, read on demand.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageState {
    pub url: String,
    pub title: String,
    pub loading: bool,
    /// 0.0 ..= 1.0
    pub progress: f64,
    pub can_back: bool,
    pub can_forward: bool,
    /// Main-frame navigations committed (a new document shown) since the
    /// view was made: WebKit's `didCommitNavigation`.
    pub commits: u64,
    /// Main-frame navigations finished (the page's load event) since the
    /// view was made: `didFinishNavigation`. A macro's `open` waits for
    /// this to move past what it was before the load.
    pub finishes: u64,
}

/// Whether blocking applies to a main-frame URL (shared with the view's
/// navigation hook).
pub type BlockingFor = std::rc::Rc<dyn Fn(&str) -> bool>;

/// A place to show web pages. `wry` implements it for real; tests use a stub.
pub trait BrowserSurface {
    fn set_frame(&mut self, bounds: Bounds<Pixels>);
    fn set_visible(&mut self, visible: bool);
    fn load_url(&mut self, url: &str);
    fn back(&mut self);
    fn forward(&mut self);
    fn reload(&mut self);
    fn stop(&mut self);
    fn state(&self) -> PageState;
    /// The compiled lists changed (or blocking was switched): re-attach for
    /// the current page (takes effect from the next load).
    fn refresh_blocking(&mut self);
    /// Hand the keyboard back to GPUI's view.
    fn focus_parent(&mut self);
    /// The page has the keyboard (a text field in it is focused, say).
    fn page_has_keyboard(&self) -> bool {
        false
    }
    /// The keyboard fell to the window itself (nobody gets typing): give it
    /// back to GPUI. Never takes it from the page.
    fn reclaim_lost_keyboard(&mut self) {}
    /// Send an edit command (`copy:`, `paste:`, …) to the page, which the
    /// app's Edit menu would otherwise send to GPUI.
    fn edit_command(&mut self, _selector: &str) {}
    fn set_dark(&mut self, _dark: bool) {}
    /// Debug builds: run `js` in the page and print its result (smoke tests).
    fn probe(&mut self, _js: &str, _label: &str) {}
    /// Debug builds: the page's WebContent process (smoke tests measure it).
    fn web_process_id(&self) -> Option<i32> {
        None
    }
    /// --- notes --- Send the page's selected text on `reply` ("" when
    /// nothing is selected).
    fn selection(&mut self, reply: Sender<String>) {
        let _ = reply.try_send(String::new());
    }
    /// --- browser macros --- Run `js`, an expression that evaluates to a
    /// string (`JSON.stringify(…)`), in the main frame and send that string
    /// on `reply` (`"null"` when it fails or isn't a string). The script
    /// runs apart from the page's own scripts where the engine allows it
    /// (WebKit: `WKContentWorld.defaultClientWorld`, which shares the DOM,
    /// not the page's globals). For host features only (the macro runner,
    /// Clip Page); never offered to an extension.
    fn eval_json(&mut self, _js: &str, reply: Sender<String>) {
        let _ = reply.try_send("null".into());
    }
    /// --- browser macros --- Give the web view the keyboard (make it the
    /// first responder), so an edit command reaches the focused field.
    fn focus_page(&mut self) {}
    /// --- browser macros --- Whether [`Self::edit_command`] reaches the
    /// page (`paste:` then pastes the clipboard as a trusted paste). Where
    /// it doesn't, the macro runner starts its insert chain at `execCommand`.
    fn has_edit_commands(&self) -> bool {
        false
    }
}

// --- browser on Windows --- Pure pieces of the WebView2 surface, kept
// here (not behind `cfg`) so they're tested on every platform.

/// --- browser macros --- What WebView2's `ExecuteScript` (wry's
/// `evaluate_script_with_callback`) hands back is the script's result
/// JSON-encoded. [`BrowserSurface::eval_json`]'s scripts evaluate to a JSON
/// string, so that string arrives encoded a second time (in quotes, with
/// escapes): decode it once. Anything that isn't a JSON string (`null` from
/// a script that threw, a number) is `"null"`, as on WebKit.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub fn decode_eval_result(json: &str) -> String {
    serde_json::from_str::<String>(json).unwrap_or_else(|_| "null".into())
}

/// Main-frame navigations as WebView2 reports them, counted the way
/// WebKit's delegate counts them ([`PageState::commits`] and
/// [`PageState::finishes`]). WebView2 raises `NavigationCompleted` for a
/// navigation the view's own policy cancelled (WebKit never finishes one),
/// so only navigations that started *allowed* are counted, matched by
/// `NavigationId`: a stray completion can't move `finishes` and let a
/// macro's `open` step go on before its page has loaded. A navigation that
/// fails before it commits still finishes (as WebKit's `didFail` ends a
/// load); a redirect keeps its id.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub struct NavCounter {
    /// Allowed navigations not finished yet, oldest first.
    open: Vec<u64>,
    /// The last navigation that committed (`ContentLoading`).
    committed: Option<u64>,
    pub commits: u64,
    pub finishes: u64,
}

#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
impl NavCounter {
    /// `NavigationStarting`, after the policy decided (`allowed`: not
    /// cancelled).
    pub fn started(&mut self, id: u64, allowed: bool) {
        if allowed && !self.open.contains(&id) {
            self.open.push(id);
        }
    }

    /// `ContentLoading`: the navigation committed. `false` (ignored) for
    /// one this counter didn't see start allowed.
    pub fn committed(&mut self, id: u64) -> bool {
        if !self.open.contains(&id) || self.committed == Some(id) {
            return false;
        }
        self.committed = Some(id);
        self.commits += 1;
        true
    }

    /// `NavigationCompleted`. `false` (ignored) for a cancelled one.
    pub fn completed(&mut self, id: u64) -> bool {
        let Some(i) = self.open.iter().position(|&n| n == id) else {
            return false;
        };
        self.open.remove(i);
        self.finishes += 1;
        true
    }

    /// An allowed navigation is under way.
    pub fn loading(&self) -> bool {
        !self.open.is_empty()
    }

    /// WebView2 reports no progress: a rough one for the pane's bar
    /// (started, committed, done).
    pub fn progress(&self) -> f64 {
        match self.open.last() {
            None => 1.0,
            Some(id) if self.committed == Some(*id) => 0.6,
            Some(_) => 0.15,
        }
    }
}

/// [`PageState`] from what WebView2 answers (the view's URL, the core
/// view's title and history flags) and the navigations counted so far.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub fn page_state(
    url: String,
    title: String,
    can_back: bool,
    can_forward: bool,
    navs: &NavCounter,
) -> PageState {
    PageState {
        url,
        title,
        loading: navs.loading(),
        progress: navs.progress(),
        can_back,
        can_forward,
        commits: navs.commits,
        finishes: navs.finishes,
    }
}

/// Makes the browser's surface. Returns a sentence for the pane when it can't.
pub type Factory = std::rc::Rc<
    dyn Fn(
        &mut Window,
        Sender<BrowserEvent>,
        super::rules_handle::Handle,
        BlockingFor,
    ) -> Result<Box<dyn BrowserSurface>, String>,
>;

/// Overrides the factory (tests install a stub).
pub struct FactoryGlobal(pub Factory);
impl gpui_kit::Global for FactoryGlobal {}

pub fn default_factory() -> Factory {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        std::rc::Rc::new(|window, tx, rules, blocking| {
            wry_browser::WryBrowser::new(window, tx, rules, blocking)
                .map(|s| Box::new(s) as Box<dyn BrowserSurface>)
        })
    }
    // No content blocking on Windows (the lists compile to WebKit rules):
    // the rules handle and the per-host test go unused.
    #[cfg(all(target_os = "windows", not(test)))]
    {
        std::rc::Rc::new(|window, tx, _rules, _blocking| {
            wry_browser_windows::DeferredBrowser::new(window, tx)
                .map(|s| Box::new(s) as Box<dyn BrowserSurface>)
        })
    }
    #[cfg(any(not(any(target_os = "macos", target_os = "windows")), test))]
    {
        std::rc::Rc::new(|_, _, _, _| Err("The browser isn't available here.".to_string()))
    }
}

/// A fixed identifier for the browser's own persistent website data store
/// (cookies, local storage): "blygger-browser" padded, as a UUID.
// macOS only (WebKit's data store); Windows names a folder instead.
#[cfg_attr(not(all(target_os = "macos", not(test))), allow(dead_code))]
pub const DATA_STORE_ID: [u8; 16] = *b"blygger-browser\0";

#[cfg(all(target_os = "macos", not(test)))]
#[expect(
    unsafe_code,
    reason = "WebKit through objc2: the browser pane's WKWebView"
)]
mod wry_browser {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use crate::app::studio::webview::{Keyboard, keyboard_of, rect};
    use objc2_web_kit::WKUserContentController;
    use wry::{
        NewWindowResponse, PageLoadEvent, WebView, WebViewBuilder, WebViewBuilderExtDarwin,
        WebViewExtMacOS,
    };

    pub struct WryBrowser {
        view: WebView,
        rules: super::super::rules_handle::Handle,
        blocking: BlockingFor,
        /// Lists are attached right now (and which generation of them).
        attached: Rc<Cell<Option<u64>>>,
        /// (commits, finishes) of main-frame navigations, from wry's page
        /// load handler.
        navs: Rc<(Cell<u64>, Cell<u64>)>,
    }

    fn macos_major() -> isize {
        objc2_foundation::NSProcessInfo::processInfo()
            .operatingSystemVersion()
            .majorVersion
    }

    /// Attach the rule lists (or none) to `manager`, unless that's what's
    /// attached already.
    fn apply(
        manager: &WKUserContentController,
        rules: &super::super::rules_handle::Handle,
        attached: &Cell<Option<u64>>,
        on: bool,
    ) {
        let r = rules.borrow();
        let want = on.then_some(r.generation);
        if attached.get() == want {
            return;
        }
        // SAFETY: main thread; the lists and the controller are live.
        unsafe {
            manager.removeAllContentRuleLists();
            if on {
                for l in &r.lists {
                    manager.addContentRuleList(l);
                }
            }
        }
        attached.set(want);
    }

    impl WryBrowser {
        pub fn new(
            window: &mut Window,
            tx: Sender<BrowserEvent>,
            rules: super::super::rules_handle::Handle,
            blocking: BlockingFor,
        ) -> Result<Self, String> {
            let (nav_tx, new_tx, load_tx, title_tx, dl_tx, dead_tx) = (
                tx.clone(),
                tx.clone(),
                tx.clone(),
                tx.clone(),
                tx.clone(),
                tx,
            );
            let navs: Rc<(Cell<u64>, Cell<u64>)> = Rc::default();
            let load_navs = navs.clone();
            let mut b = WebViewBuilder::new()
                .with_bounds(rect(Bounds::default()))
                .with_visible(false)
                .with_focused(false)
                .with_accept_first_mouse(true)
                .with_devtools(cfg!(debug_assertions))
                .with_back_forward_navigation_gestures(true)
                .with_navigation_handler(move |url| match super::super::nav_policy(&url) {
                    super::super::Nav::Allow => true,
                    super::super::Nav::System(u) => {
                        let _ = nav_tx.try_send(BrowserEvent::OpenExternally(u));
                        false
                    }
                    super::super::Nav::Deny => false,
                })
                .with_new_window_req_handler(move |url, _| {
                    match super::super::nav_policy(&url) {
                        super::super::Nav::Allow if url != "about:blank" => {
                            let _ = new_tx.try_send(BrowserEvent::NewWindow(url));
                        }
                        super::super::Nav::System(u) => {
                            let _ = new_tx.try_send(BrowserEvent::OpenExternally(u));
                        }
                        _ => {}
                    }
                    NewWindowResponse::Deny
                })
                .with_download_started_handler(move |url, _path| {
                    if url.starts_with("https://") || url.starts_with("http://") {
                        let _ = dl_tx.try_send(BrowserEvent::OpenExternally(url));
                    }
                    false
                })
                .with_on_page_load_handler(move |ev: PageLoadEvent, _url| {
                    // wry: Started is didCommitNavigation, Finished is
                    // didFinishNavigation (both main-frame only).
                    let (c, f) = &*load_navs;
                    match ev {
                        PageLoadEvent::Started => c.set(c.get() + 1),
                        PageLoadEvent::Finished => f.set(f.get() + 1),
                    }
                    let _ = load_tx.try_send(BrowserEvent::Changed);
                })
                .with_document_title_changed_handler(move |_| {
                    let _ = title_tx.try_send(BrowserEvent::Changed);
                })
                .with_on_web_content_process_terminate_handler(move || {
                    let _ = dead_tx.try_send(BrowserEvent::Crashed);
                })
                .with_url("about:blank");
            // Its own cookie jar: persistent on macOS 14+, else in memory.
            // Debug builds: `BLYGGER_BROWSER_EPHEMERAL` keeps a smoke test's
            // fixture cookies out of the persistent store.
            let ephemeral = cfg!(debug_assertions) && crate::dev_flags::get().browser.ephemeral;
            b = if macos_major() >= 14 && !ephemeral {
                b.with_data_store_identifier(DATA_STORE_ID)
            } else {
                b.with_incognito(true)
            };
            let view = b
                .build_as_child(&*window)
                .map_err(|e| format!("The browser couldn't start ({e})."))?;
            let attached = Rc::new(Cell::new(None));
            let this = WryBrowser {
                view,
                rules,
                blocking,
                attached,
                navs,
            };
            this.install_hook();
            Ok(this)
        }

        fn install_hook(&self) {
            let wk = self.view.webview();
            // The view's navigation delegate is wry's; give that one object a
            // subclass that asks us first about main-frame navigations.
            // SAFETY: main thread; `wk` is live, and its delegate is wry's
            // WryNavigationDelegate (retained by wry for the view's lifetime).
            let delegate: *mut objc2::runtime::AnyObject =
                unsafe { objc2::msg_send![&*wk, navigationDelegate] };
            let Some(delegate) = (unsafe { delegate.as_ref() }) else {
                return;
            };
            let view_ptr = objc2::rc::Retained::as_ptr(&wk) as usize;
            // The hook needs the WebView to swap lists; it's only ever called
            // while the view is alive (WebKit calls its own delegate).
            let rules = self.rules.clone();
            let blocking = self.blocking.clone();
            let attached = self.attached.clone();
            let manager = self.view.manager();
            nav_hook::install(
                delegate,
                view_ptr,
                Box::new(move |url: &str| apply(&manager, &rules, &attached, blocking(url))),
            );
        }

        fn wk(&self) -> objc2::rc::Retained<wry::WryWebView> {
            self.view.webview()
        }
    }

    impl Drop for WryBrowser {
        fn drop(&mut self) {
            let wk = self.view.webview();
            nav_hook::uninstall(objc2::rc::Retained::as_ptr(&wk) as usize);
            if keyboard_of(&wk) != Keyboard::Elsewhere {
                let _ = self.view.focus_parent();
            }
        }
    }

    impl BrowserSurface for WryBrowser {
        fn set_frame(&mut self, bounds: Bounds<Pixels>) {
            let _ = self.view.set_bounds(rect(bounds));
        }

        fn set_visible(&mut self, visible: bool) {
            // As for the preview: never hide the first responder (typing
            // would then reach nobody).
            if !visible && keyboard_of(&self.wk()) != Keyboard::Elsewhere {
                let _ = self.view.focus_parent();
            }
            let _ = self.view.set_visible(visible);
        }

        fn load_url(&mut self, url: &str) {
            // The first page: attach before it loads (the hook covers every
            // navigation after this one too).
            let on = (self.blocking)(url);
            apply(&self.view.manager(), &self.rules, &self.attached, on);
            let _ = self.view.load_url(url);
        }

        fn back(&mut self) {
            let _ = self.view.go_back();
        }

        fn forward(&mut self) {
            let _ = self.view.go_forward();
        }

        fn reload(&mut self) {
            let url = self.state().url;
            let on = (self.blocking)(&url);
            apply(&self.view.manager(), &self.rules, &self.attached, on);
            let _ = self.view.reload();
        }

        fn stop(&mut self) {
            // SAFETY: main thread; live view.
            unsafe { self.wk().stopLoading() };
        }

        fn state(&self) -> PageState {
            let wk = self.wk();
            // SAFETY: main thread; plain WKWebView getters.
            unsafe {
                PageState {
                    url: wk
                        .URL()
                        .and_then(|u| u.absoluteString())
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    title: wk.title().map(|s| s.to_string()).unwrap_or_default(),
                    loading: wk.isLoading(),
                    progress: wk.estimatedProgress(),
                    can_back: wk.canGoBack(),
                    can_forward: wk.canGoForward(),
                    commits: self.navs.0.get(),
                    finishes: self.navs.1.get(),
                }
            }
        }

        fn refresh_blocking(&mut self) {
            let url = self.state().url;
            let on = (self.blocking)(&url);
            apply(&self.view.manager(), &self.rules, &self.attached, on);
        }

        fn focus_parent(&mut self) {
            let _ = self.view.focus_parent();
        }

        fn page_has_keyboard(&self) -> bool {
            keyboard_of(&self.wk()) == Keyboard::InPage
        }

        fn reclaim_lost_keyboard(&mut self) {
            if keyboard_of(&self.wk()) == Keyboard::Nowhere {
                let _ = self.view.focus_parent();
            }
        }

        fn edit_command(&mut self, selector: &str) {
            let sel = objc2::runtime::Sel::register(
                &std::ffi::CString::new(selector).unwrap_or_default(),
            );
            let wk = self.wk();
            // SAFETY: main thread; `tryToPerform:with:` walks the responder
            // chain from the web view's first responder like the Edit menu.
            unsafe {
                let win: *mut objc2::runtime::AnyObject = objc2::msg_send![&*wk, window];
                if let Some(win) = win.as_ref() {
                    let responder: *mut objc2::runtime::AnyObject =
                        objc2::msg_send![win, firstResponder];
                    if let Some(r) = responder.as_ref() {
                        let nil: *mut objc2::runtime::AnyObject = std::ptr::null_mut();
                        let _: bool = objc2::msg_send![r, tryToPerform: sel, with: nil];
                    }
                }
            }
        }

        fn set_dark(&mut self, dark: bool) {
            crate::app::studio::webview::set_appearance(&self.wk(), dark);
        }

        fn web_process_id(&self) -> Option<i32> {
            if !cfg!(debug_assertions) {
                return None;
            }
            let wk = self.wk();
            // SAFETY: main thread; `_webProcessIdentifier` is WebKit SPI
            // (a pid_t getter), used only for debug measurements.
            unsafe {
                let ok: bool = objc2::msg_send![
                    &*wk,
                    respondsToSelector: objc2::sel!(_webProcessIdentifier)
                ];
                if !ok {
                    return None;
                }
                let pid: i32 = objc2::msg_send![&*wk, _webProcessIdentifier];
                (pid > 0).then_some(pid)
            }
        }

        fn probe(&mut self, js: &str, label: &str) {
            let label = label.to_string();
            let _ = self
                .view
                .evaluate_script_with_callback(js, move |json| println!("{label} {json}"));
        }

        // --- browser macros ---
        fn eval_json(&mut self, js: &str, reply: Sender<String>) {
            use objc2_foundation::{NSError, NSString};
            use objc2_web_kit::WKContentWorld;
            let Some(mtm) = objc2::MainThreadMarker::new() else {
                let _ = reply.try_send("null".into());
                return;
            };
            let wk = self.wk();
            let script = NSString::from_str(js);
            let block = block2::RcBlock::new(
                move |result: *mut objc2::runtime::AnyObject, _err: *mut NSError| {
                    // SAFETY: WebKit hands a live (or nil) result object.
                    let text = unsafe { result.as_ref() }
                        .and_then(|o| o.downcast_ref::<NSString>())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "null".into());
                    let _ = reply.try_send(text);
                },
            );
            // SAFETY: main thread; live view; WebKit copies the block.
            unsafe {
                let world = WKContentWorld::defaultClientWorld(mtm);
                wk.evaluateJavaScript_inFrame_inContentWorld_completionHandler(
                    &script,
                    None,
                    &world,
                    Some(&block),
                );
            }
        }

        fn focus_page(&mut self) {
            let wk = self.wk();
            // SAFETY: main thread; `makeFirstResponder:` on the view's own
            // window (there's none while it isn't in one).
            unsafe {
                let win: *mut objc2::runtime::AnyObject = objc2::msg_send![&*wk, window];
                if let Some(win) = win.as_ref() {
                    let _: bool = objc2::msg_send![win, makeFirstResponder: &*wk];
                }
            }
        }

        fn has_edit_commands(&self) -> bool {
            true
        }

        // --- notes ---
        fn selection(&mut self, reply: Sender<String>) {
            let _ = self.view.evaluate_script_with_callback(
                crate::app::notes::SELECTION_JS,
                move |json| {
                    let _ = reply.try_send(crate::app::notes::parse_selection(&json));
                },
            );
        }
    }

    /// A per-object subclass of wry's navigation delegate (isa-swizzled, as
    /// KVO does) that calls a hook for main-frame navigations before wry's
    /// own policy runs. The subclass adds no ivars, so wry's state is
    /// untouched; the reader's and preview's delegates keep wry's class.
    mod nav_hook {
        use std::cell::RefCell;
        use std::collections::HashMap;
        use std::sync::OnceLock;

        use block2::DynBlock;
        use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
        use objc2::{msg_send, sel};
        use objc2_web_kit::{WKNavigationAction, WKNavigationActionPolicy, WKWebView};

        type Hook = Box<dyn Fn(&str)>;

        thread_local! {
            static HOOKS: RefCell<HashMap<usize, Hook>> = RefCell::new(HashMap::new());
        }

        /// (subclass, wry's class), made once.
        static CLASSES: OnceLock<(usize, usize)> = OnceLock::new();

        extern "C-unwind" fn decide(
            this: &AnyObject,
            _sel: Sel,
            webview: &WKWebView,
            action: &WKNavigationAction,
            handler: &DynBlock<dyn Fn(WKNavigationActionPolicy)>,
        ) {
            // SAFETY: main thread (WebKit calls its delegate there); plain
            // getters on live objects.
            unsafe {
                let main = action.targetFrame().is_some_and(|f| f.isMainFrame());
                if main {
                    let url = action
                        .request()
                        .URL()
                        .and_then(|u| u.absoluteString())
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    let key = webview as *const WKWebView as usize;
                    HOOKS.with(|h| {
                        if let Some(f) = h.borrow().get(&key) {
                            f(&url);
                        }
                    });
                }
                let Some(&(_, base)) = CLASSES.get() else {
                    return;
                };
                let base = &*(base as *const AnyClass);
                let _: () = msg_send![
                    super(this, base),
                    webView: webview,
                    decidePolicyForNavigationAction: action,
                    decisionHandler: handler
                ];
            }
        }

        pub fn install(delegate: &AnyObject, view: usize, hook: Hook) {
            let cls = delegate.class();
            let &(sub, base) = CLASSES.get_or_init(|| {
                let mut b = ClassBuilder::new(c"BlyggerBrowserNavigationDelegate", cls)
                    .expect("the browser's delegate class is registered once");
                // SAFETY: the signature matches WKNavigationDelegate's
                // webView:decidePolicyForNavigationAction:decisionHandler:.
                unsafe {
                    b.add_method(
                        sel!(webView:decidePolicyForNavigationAction:decisionHandler:),
                        decide as extern "C-unwind" fn(_, _, _, _, _),
                    );
                }
                let sub = b.register();
                (
                    sub as *const AnyClass as usize,
                    cls as *const AnyClass as usize,
                )
            });
            if cls as *const AnyClass as usize != base {
                // Not the class the subclass was made from: leave it alone.
                return;
            }
            // SAFETY: the subclass derives from the object's own class and
            // adds no ivars, so the object's layout is unchanged.
            unsafe {
                AnyObject::set_class(delegate, &*(sub as *const AnyClass));
            }
            HOOKS.with(|h| h.borrow_mut().insert(view, hook));
        }

        pub fn uninstall(view: usize) {
            HOOKS.with(|h| h.borrow_mut().remove(&view));
        }
    }
}

/// The pane on Windows: a WebView2, configured as the macOS view is (no IPC
/// handler and no initialization script, so the page has no bridge to the
/// app; `nav_policy` for navigations; new windows and downloads handed on).
/// It's built outside GPUI's frame like the preview
/// ([`crate::webview2_host`]), so [`DeferredBrowser`] records what it's
/// asked meanwhile and replays it.
///
/// What wry doesn't pass on comes from the core WebView2 directly: which
/// navigation started, committed and completed ([`NavCounter`]), the
/// process failing, and whether the view has the keyboard (the controller's
/// focus events: WebView2 has no cheap "who has focus" query).
///
/// There's no `paste:` to send (no responder chain), so
/// `has_edit_commands` is false and a macro's insert starts at
/// `execCommand("insertText")`. There's no isolated script world either:
/// the scripts `eval_json` runs are each one function expression that
/// leaves no globals behind (`automation/js.rs`, `capture.rs`).
#[cfg(all(target_os = "windows", not(test)))]
#[expect(
    unsafe_code,
    reason = "WebView2 COM calls: the browser pane's web view"
)]
mod wry_browser_windows {
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;
    use std::rc::Rc;

    use super::*;
    use crate::app::studio::webview::rect;
    use crate::webview2_host::{Deferred, Hwnd};
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
    };
    use webview2_com::{
        ContentLoadingEventHandler, FocusChangedEventHandler, NavigationCompletedEventHandler,
        NavigationStartingEventHandler, ProcessFailedEventHandler, take_pwstr,
    };
    use windows_core::{BOOL, PWSTR};
    use wry::{NewWindowResponse, Theme, WebContext, WebView, WebViewBuilder, WebViewExtWindows};

    /// The pane's own WebView2 user data folder: cookies and sign-ins
    /// survive a relaunch, apart from the preview's (wry's default folder).
    /// Two WebView2 environments with different folders can share a process.
    fn profile_dir() -> PathBuf {
        blyg_core::config::paths::data_dir()
            .join("browser")
            .join("webview2")
    }

    /// What the pane asked for before its WebView existed.
    #[derive(Default)]
    struct Pending {
        frame: Option<Bounds<Pixels>>,
        visible: bool,
        dark: Option<bool>,
        url: Option<String>,
        /// `eval_json` requests waiting for the view, with their replies.
        evals: Vec<(String, Sender<String>)>,
        /// The build failed: there will never be a view.
        dead: bool,
    }

    impl Pending {
        /// Answer every waiting `eval_json` with `"null"`, so a macro run
        /// never waits for a view that won't come.
        fn answer_null(&mut self) {
            for (_, tx) in self.evals.drain(..) {
                let _ = tx.try_send("null".into());
            }
        }
    }

    impl Drop for Pending {
        fn drop(&mut self) {
            self.answer_null();
        }
    }

    /// The pane's surface: the WebView2 once it's built (see above).
    pub struct DeferredBrowser(Deferred<WryBrowser, Pending>);

    impl DeferredBrowser {
        pub fn new(window: &mut Window, tx: Sender<BrowserEvent>) -> Result<Self, String> {
            Deferred::start(
                window,
                "The browser couldn't find its window.",
                move |hwnd| WryBrowser::new(hwnd, tx),
                |view: &mut WryBrowser, p: &mut Pending| {
                    if let Some(b) = p.frame {
                        view.set_frame(b);
                    }
                    if let Some(d) = p.dark {
                        view.set_dark(d);
                    }
                    if let Some(u) = p.url.take() {
                        view.load_url(&u);
                    }
                    for (js, reply) in std::mem::take(&mut p.evals) {
                        view.eval_json(&js, reply);
                    }
                    view.set_visible(p.visible);
                },
                |p: &mut Pending, msg: String| {
                    eprintln!("blygger: {msg}");
                    p.dead = true;
                    p.answer_null();
                },
            )
            .map(DeferredBrowser)
        }

        /// Run `f` on the view if it exists (nothing to record otherwise).
        fn on_view(&mut self, f: impl FnOnce(&mut WryBrowser)) {
            self.0.with(|s| {
                if let Some(v) = s.view.as_mut() {
                    f(v);
                }
            })
        }
    }

    impl BrowserSurface for DeferredBrowser {
        fn set_frame(&mut self, bounds: Bounds<Pixels>) {
            self.0.with(|s| match s.view.as_mut() {
                Some(v) => v.set_frame(bounds),
                None => s.pending.frame = Some(bounds),
            })
        }

        fn set_visible(&mut self, visible: bool) {
            self.0.with(|s| match s.view.as_mut() {
                Some(v) => v.set_visible(visible),
                None => s.pending.visible = visible,
            })
        }

        fn load_url(&mut self, url: &str) {
            self.0.with(|s| match s.view.as_mut() {
                Some(v) => v.load_url(url),
                None => s.pending.url = Some(url.to_string()),
            })
        }

        fn back(&mut self) {
            self.on_view(|v| v.back());
        }

        fn forward(&mut self) {
            self.on_view(|v| v.forward());
        }

        fn reload(&mut self) {
            self.on_view(|v| v.reload());
        }

        fn stop(&mut self) {
            self.on_view(|v| v.stop());
        }

        fn state(&self) -> PageState {
            self.0.with(|s| match s.view.as_ref() {
                Some(v) => v.state(),
                // Not built yet: the page asked for, loading.
                None => PageState {
                    loading: s.pending.url.is_some() && !s.pending.dead,
                    url: s.pending.url.clone().unwrap_or_default(),
                    ..PageState::default()
                },
            })
        }

        fn refresh_blocking(&mut self) {}

        fn focus_parent(&mut self) {
            self.on_view(|v| v.focus_parent());
        }

        fn page_has_keyboard(&self) -> bool {
            self.0
                .with(|s| s.view.as_ref().is_some_and(|v| v.page_has_keyboard()))
        }

        fn reclaim_lost_keyboard(&mut self) {
            self.on_view(|v| v.reclaim_lost_keyboard());
        }

        fn set_dark(&mut self, dark: bool) {
            self.0.with(|s| match s.view.as_mut() {
                Some(v) => v.set_dark(dark),
                None => s.pending.dark = Some(dark),
            })
        }

        fn probe(&mut self, js: &str, label: &str) {
            self.on_view(|v| v.probe(js, label));
        }

        // --- notes --- (nothing is selected before the page exists)
        fn selection(&mut self, reply: Sender<String>) {
            self.0.with(|s| match s.view.as_mut() {
                Some(v) => v.selection(reply),
                None => {
                    let _ = reply.try_send(String::new());
                }
            })
        }

        // --- browser macros ---
        fn eval_json(&mut self, js: &str, reply: Sender<String>) {
            self.0.with(|s| match s.view.as_mut() {
                Some(v) => v.eval_json(js, reply),
                None if s.pending.dead => {
                    let _ = reply.try_send("null".into());
                }
                None => s.pending.evals.push((js.to_string(), reply)),
            })
        }

        fn focus_page(&mut self) {
            self.on_view(|v| v.focus_page());
        }

        fn has_edit_commands(&self) -> bool {
            false
        }
    }

    pub struct WryBrowser {
        view: WebView,
        /// Keeps the user data folder's context alive with the view.
        _context: Option<WebContext>,
        navs: Rc<RefCell<NavCounter>>,
        /// The view has the keyboard (the controller's GotFocus/LostFocus).
        focused: Rc<Cell<bool>>,
    }

    impl WryBrowser {
        fn new(parent: &Hwnd, tx: Sender<BrowserEvent>) -> Result<Self, String> {
            let (nav_tx, new_tx, title_tx, dl_tx) =
                (tx.clone(), tx.clone(), tx.clone(), tx.clone());
            // Its own profile, as on macOS. Debug builds:
            // `BLYGGER_BROWSER_EPHEMERAL` keeps a smoke test's fixture
            // cookies out of it (in memory only).
            let ephemeral = cfg!(debug_assertions) && crate::dev_flags::get().browser.ephemeral;
            let mut context = (!ephemeral).then(|| WebContext::new(Some(profile_dir())));
            let builder = match context.as_mut() {
                Some(c) => WebViewBuilder::new_with_web_context(c),
                None => WebViewBuilder::new().with_incognito(true),
            };
            let view = builder
                .with_bounds(rect(Bounds::default()))
                .with_visible(false)
                .with_focused(false)
                .with_devtools(cfg!(debug_assertions))
                .with_navigation_handler(move |url| match super::super::nav_policy(&url) {
                    super::super::Nav::Allow => true,
                    super::super::Nav::System(u) => {
                        let _ = nav_tx.try_send(BrowserEvent::OpenExternally(u));
                        false
                    }
                    super::super::Nav::Deny => false,
                })
                .with_new_window_req_handler(move |url, _| {
                    match super::super::nav_policy(&url) {
                        super::super::Nav::Allow if url != "about:blank" => {
                            let _ = new_tx.try_send(BrowserEvent::NewWindow(url));
                        }
                        super::super::Nav::System(u) => {
                            let _ = new_tx.try_send(BrowserEvent::OpenExternally(u));
                        }
                        _ => {}
                    }
                    NewWindowResponse::Deny
                })
                .with_download_started_handler(move |url, _path| {
                    if url.starts_with("https://") || url.starts_with("http://") {
                        let _ = dl_tx.try_send(BrowserEvent::OpenExternally(url));
                    }
                    false
                })
                .with_document_title_changed_handler(move |_| {
                    let _ = title_tx.try_send(BrowserEvent::Changed);
                })
                .with_url("about:blank")
                .build_as_child(parent)
                .map_err(|e| {
                    format!(
                        "The browser couldn't start ({e}). It needs the Microsoft Edge \
                         WebView2 Runtime, which Windows 11 includes."
                    )
                })?;
            let navs: Rc<RefCell<NavCounter>> = Rc::default();
            let focused = Rc::new(Cell::new(false));
            hook_events(&view, &navs, &focused, tx)
                .map_err(|e| format!("The browser couldn't start ({e})."))?;
            Ok(WryBrowser {
                view,
                _context: context,
                navs,
                focused,
            })
        }
    }

    /// The core WebView2's navigation, process and focus events. wry's own
    /// navigation handler was added when the view was built, so by the
    /// time ours sees `NavigationStarting` it has already decided `Cancel`.
    fn hook_events(
        view: &WebView,
        navs: &Rc<RefCell<NavCounter>>,
        focused: &Rc<Cell<bool>>,
        tx: Sender<BrowserEvent>,
    ) -> windows_core::Result<()> {
        let core = view.webview();
        let controller = view.controller();
        let mut token = 0i64;
        let (n1, n2, n3) = (navs.clone(), navs.clone(), navs.clone());
        let (load_tx, done_tx, dead_tx) = (tx.clone(), tx.clone(), tx);
        let (f1, f2) = (focused.clone(), focused.clone());
        // SAFETY: main thread; live core view and controller; the handlers
        // only touch the `Rc`s they own and send on channels.
        unsafe {
            core.add_NavigationStarting(
                &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else {
                        return Ok(());
                    };
                    let (mut id, mut cancel) = (0u64, BOOL::default());
                    args.NavigationId(&mut id)?;
                    args.Cancel(&mut cancel)?;
                    n1.borrow_mut().started(id, !cancel.as_bool());
                    Ok(())
                })),
                &mut token,
            )?;
            core.add_ContentLoading(
                &ContentLoadingEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else {
                        return Ok(());
                    };
                    let mut id = 0u64;
                    args.NavigationId(&mut id)?;
                    if n2.borrow_mut().committed(id) {
                        let _ = load_tx.try_send(BrowserEvent::Changed);
                    }
                    Ok(())
                })),
                &mut token,
            )?;
            core.add_NavigationCompleted(
                &NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else {
                        return Ok(());
                    };
                    let mut id = 0u64;
                    args.NavigationId(&mut id)?;
                    if n3.borrow_mut().completed(id) {
                        let _ = done_tx.try_send(BrowserEvent::Changed);
                    }
                    Ok(())
                })),
                &mut token,
            )?;
            core.add_ProcessFailed(
                &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else {
                        return Ok(());
                    };
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                    args.ProcessFailedKind(&mut kind)?;
                    // The page's own process (or the whole browser) died or
                    // hung: the pane loads the page again. A GPU or utility
                    // process WebView2 restarts by itself.
                    if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                        || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE
                        || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
                    {
                        let _ = dead_tx.try_send(BrowserEvent::Crashed);
                    }
                    Ok(())
                })),
                &mut token,
            )?;
            controller.add_GotFocus(
                &FocusChangedEventHandler::create(Box::new(move |_, _| {
                    f1.set(true);
                    Ok(())
                })),
                &mut token,
            )?;
            controller.add_LostFocus(
                &FocusChangedEventHandler::create(Box::new(move |_, _| {
                    f2.set(false);
                    Ok(())
                })),
                &mut token,
            )?;
        }
        Ok(())
    }

    impl Drop for WryBrowser {
        fn drop(&mut self) {
            if self.focused.get() {
                let _ = self.view.focus_parent();
            }
        }
    }

    impl BrowserSurface for WryBrowser {
        fn set_frame(&mut self, bounds: Bounds<Pixels>) {
            let _ = self.view.set_bounds(rect(bounds));
        }

        fn set_visible(&mut self, visible: bool) {
            // As on macOS: never hide the view while it has the keyboard
            // (typing would then reach nobody); hand it back first.
            if !visible && self.focused.get() {
                let _ = self.view.focus_parent();
            }
            let _ = self.view.set_visible(visible);
        }

        fn load_url(&mut self, url: &str) {
            let _ = self.view.load_url(url);
        }

        fn back(&mut self) {
            let _ = self.view.go_back();
        }

        fn forward(&mut self) {
            let _ = self.view.go_forward();
        }

        fn reload(&mut self) {
            let _ = self.view.reload();
        }

        fn stop(&mut self) {
            // SAFETY: main thread; live core view.
            let _ = unsafe { self.view.webview().Stop() };
        }

        fn state(&self) -> PageState {
            let core = self.view.webview();
            let mut title = PWSTR::null();
            let (mut back, mut forward) = (BOOL::default(), BOOL::default());
            // SAFETY: main thread; plain getters on the live core view.
            // `DocumentTitle` allocates the string; `take_pwstr` frees it.
            let title = unsafe {
                match core.DocumentTitle(&mut title) {
                    Ok(()) => take_pwstr(title),
                    Err(_) => String::new(),
                }
            };
            unsafe {
                let _ = core.CanGoBack(&mut back);
                let _ = core.CanGoForward(&mut forward);
            }
            page_state(
                self.view.url().unwrap_or_default(),
                title,
                back.as_bool(),
                forward.as_bool(),
                &self.navs.borrow(),
            )
        }

        // No content blocking on Windows yet (see the module docs).
        fn refresh_blocking(&mut self) {}

        fn focus_parent(&mut self) {
            let _ = self.view.focus_parent();
        }

        fn page_has_keyboard(&self) -> bool {
            self.focused.get()
        }

        fn reclaim_lost_keyboard(&mut self) {
            // Never take the keyboard from the page; giving it to GPUI's
            // window when nobody has it (or GPUI already has it) is harmless.
            if !self.focused.get() {
                let _ = self.view.focus_parent();
            }
        }

        fn set_dark(&mut self, dark: bool) {
            let _ = self
                .view
                .set_theme(if dark { Theme::Dark } else { Theme::Light });
        }

        fn probe(&mut self, js: &str, label: &str) {
            let label = label.to_string();
            let _ = self
                .view
                .evaluate_script_with_callback(js, move |json| println!("{label} {json}"));
        }

        // --- notes ---
        fn selection(&mut self, reply: Sender<String>) {
            let fallback = reply.clone();
            let sent = self.view.evaluate_script_with_callback(
                crate::app::notes::SELECTION_JS,
                move |json| {
                    let _ = reply.try_send(crate::app::notes::parse_selection(&json));
                },
            );
            if sent.is_err() {
                let _ = fallback.try_send(String::new());
            }
        }

        // --- browser macros ---
        fn eval_json(&mut self, js: &str, reply: Sender<String>) {
            let fallback = reply.clone();
            let sent = self.view.evaluate_script_with_callback(js, move |json| {
                let _ = reply.try_send(decode_eval_result(&json));
            });
            if sent.is_err() {
                let _ = fallback.try_send("null".into());
            }
        }

        fn focus_page(&mut self) {
            let _ = self.view.focus();
        }

        fn has_edit_commands(&self) -> bool {
            false
        }
    }
}
