//! The macro runner over a scripted DOM: every step kind, both sheets
//! gating `submit`, read-back, refusals, origins, timeouts, the clipboard,
//! pacing; and the pane wiring (sheets, held close) in a GPUI window.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use async_channel::Sender;
use blyg_ext::protocol::{MacroSpec, SiteSpec, When};
use blyg_ext::recipe::{self, Step};
use gpui_kit::{Bounds, Pixels};

use super::insert::{self, Via};
use super::js::{self, Op, Reply};
use super::runner::{self, Confirm, MacroRun, Outcome, Pacing, Page, PostChoice, Preview, Reason};
use crate::app::browser::surface::{BrowserSurface, PageState};

// ------------------------------------------------------------ the DOM

/// How an element takes text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Static,
    /// A rich editor: which ways in it accepts (a ProseMirror-like one
    /// takes only a trusted paste and insertText).
    Editor {
        paste: bool,
        insert_text: bool,
        synthetic: bool,
    },
    /// An `<input>` (type, autocomplete).
    Field(&'static str, &'static str),
    Button(Action),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    /// Posts the first editor's text and clears it.
    Post,
    /// Shows the editor (a "New note" button).
    Compose,
    /// Leaves the text in the box (a site that ignores the click).
    Ignore,
}

#[derive(Debug, Clone)]
pub struct El {
    pub sels: Vec<String>,
    pub text: String,
    pub kind: Kind,
    /// Visible from this time on.
    pub at: Duration,
    /// Hidden until a Compose button is clicked.
    pub hidden: bool,
    /// The site rewrites the box at this time (read-back mismatch).
    pub later: Option<(Duration, String)>,
}

impl El {
    pub fn new(sels: &[&str], kind: Kind) -> El {
        El {
            sels: sels.iter().map(|s| s.to_string()).collect(),
            text: String::new(),
            kind,
            at: Duration::ZERO,
            hidden: false,
            later: None,
        }
    }
    pub fn at(mut self, d: Duration) -> El {
        self.at = d;
        self
    }
    pub fn text(mut self, t: &str) -> El {
        self.text = t.into();
        self
    }
}

type Pages = Rc<dyn Fn(&str, bool) -> (String, Vec<El>)>;

pub struct Dom {
    pub url: String,
    pub loaded_at: Duration,
    pub load_time: Duration,
    pub cookie: bool,
    pub els: Vec<El>,
    pub focused: Option<usize>,
    pub web_focused: bool,
    pub edit_commands: bool,
    pub posted: Vec<String>,
    pub loads: Vec<String>,
    pub commands: Vec<String>,
    /// What happened, in order (`confirm`, `click:…`, `preview`).
    pub events: Rc<RefCell<Vec<String>>>,
    pub clock: Rc<Cell<Duration>>,
    pub clipboard: Rc<RefCell<String>>,
    pub pages: Pages,
    /// The main frame goes here at this time (a navigation by the page).
    pub nav_at: Option<(Duration, String)>,
    /// What `eval_json` answers a script that isn't an op (Clip Page).
    pub capture: String,
    /// Main-frame navigations committed and finished (`PageState`).
    pub commits: u64,
    pub finishes: u64,
    /// The last load hasn't finished yet.
    pub finish_pending: bool,
    /// Loads never finish (`loading` stays true: a request that never ends).
    pub never_finish: bool,
    /// `document.readyState` instead of the load's (`complete` once loaded).
    pub ready: Option<&'static str>,
}

impl Dom {
    pub fn new(pages: Pages) -> Rc<RefCell<Dom>> {
        Rc::new(RefCell::new(Dom {
            url: "about:blank".into(),
            loaded_at: Duration::ZERO,
            load_time: Duration::from_millis(200),
            cookie: true,
            els: vec![],
            focused: None,
            web_focused: false,
            edit_commands: true,
            posted: vec![],
            loads: vec![],
            commands: vec![],
            events: Rc::default(),
            clock: Rc::default(),
            clipboard: Rc::default(),
            pages,
            nav_at: None,
            capture: "null".into(),
            commits: 0,
            finishes: 0,
            finish_pending: false,
            never_finish: false,
            ready: None,
        }))
    }

    fn now(&self) -> Duration {
        self.clock.get()
    }

    fn tick(&mut self) {
        let now = self.now();
        for el in &mut self.els {
            if let Some((t, text)) = el.later.clone()
                && now >= t
            {
                el.text = text;
                el.later = None;
            }
        }
        if let Some((t, url)) = self.nav_at.clone()
            && now >= t
        {
            self.nav_at = None;
            self.url = url;
        }
        if self.finish_pending && !self.never_finish && now >= self.loaded_at {
            self.finish_pending = false;
            self.finishes += 1;
        }
    }

    fn loading(&self) -> bool {
        self.never_finish || self.now() < self.loaded_at
    }

    /// What `Op::Outline` sees: the text boxes and buttons (never the
    /// static text), as the in-page library lists them.
    fn outline(&self) -> js::Outline {
        let els: Vec<js::OutlineEl> = self
            .els
            .iter()
            .filter(|e| !matches!(e.kind, Kind::Static))
            .map(|e| {
                let sel = e.sels.first().cloned().unwrap_or_default();
                let tag: String = sel
                    .chars()
                    .take_while(|c| c.is_ascii_alphabetic())
                    .collect();
                let rest = &sel[tag.len()..];
                let cls = rest
                    .split('[')
                    .next()
                    .unwrap_or("")
                    .split('.')
                    .filter(|c| !c.is_empty())
                    .map(String::from)
                    .collect();
                let button = matches!(e.kind, Kind::Button(_));
                js::OutlineEl {
                    tag: if tag.is_empty() { "div".into() } else { tag },
                    cls,
                    ce: if matches!(e.kind, Kind::Editor { .. }) {
                        "true".into()
                    } else {
                        String::new()
                    },
                    aria: if button {
                        String::new()
                    } else {
                        "Write a note".into()
                    },
                    vis: self.visible(e),
                    label: if button {
                        e.text.chars().take(30).collect()
                    } else {
                        String::new()
                    },
                    ..js::OutlineEl::default()
                }
            })
            .collect();
        js::Outline {
            path: self
                .url
                .split("://")
                .nth(1)
                .and_then(|r| r.find('/').map(|i| r[i..].to_string()))
                .unwrap_or_else(|| "/".into()),
            title_len: 13,
            total: els.len(),
            els,
        }
    }

    fn visible(&self, el: &El) -> bool {
        !el.hidden && self.now() >= el.at
    }

    fn pick(&self, sel: &str, text: Option<&str>) -> Option<usize> {
        self.els.iter().position(|e| {
            self.visible(e)
                && e.sels.iter().any(|s| s == sel)
                && text.is_none_or(|t| e.text.trim() == t)
        })
    }

    fn credential(el: &El) -> Option<String> {
        match el.kind {
            Kind::Field(ty, _) if ty == "password" || ty == "email" => {
                Some(format!("a {ty} field"))
            }
            Kind::Field(_, ac)
                if [
                    "current-password",
                    "new-password",
                    "one-time-code",
                    "username",
                ]
                .contains(&ac) =>
            {
                Some("a sign-in field".into())
            }
            _ => None,
        }
    }

    fn prep(&mut self, i: usize) -> Option<String> {
        let el = &self.els[i];
        if let Some(why) = Self::credential(el) {
            return Some(why);
        }
        if matches!(el.kind, Kind::Static | Kind::Button(_)) {
            return Some("something that isn't a text box".into());
        }
        self.focused = Some(i);
        None
    }

    fn answer(&mut self, op: &Op) -> Reply {
        self.tick();
        let ready = self.ready.unwrap_or(if self.loading() {
            "interactive"
        } else {
            "complete"
        });
        let mut r = Reply {
            url: self.url.clone(),
            ready: ready.into(),
            ..Reply::default()
        };
        let (sel, text, signed_out) = match op {
            Op::Outline => {
                r.outline = Some(self.outline());
                return r;
            }
            Op::Find {
                sel,
                text,
                signed_out,
            } => (sel, text.as_deref(), signed_out.as_deref()),
            Op::Focus { sel, text } | Op::Click { sel, text } => (sel, text.as_deref(), None),
            Op::Prep { sel } | Op::Read { sel } | Op::Exec { sel, .. } | Op::Synth { sel, .. } => {
                (sel, None, None)
            }
        };
        if let Some(so) = signed_out {
            r.signed_out = self.pick(so, None).is_some();
        }
        if sel.contains("!!") {
            r.bad = true;
            return r;
        }
        let Some(i) = self.pick(sel, text) else {
            return r;
        };
        r.found = true;
        r.visible = true;
        match op {
            Op::Outline | Op::Find { .. } | Op::Read { .. } => {}
            Op::Focus { .. } => {
                self.focused = Some(i);
                r.refused = Self::credential(&self.els[i]);
            }
            Op::Click { .. } => {
                self.events.borrow_mut().push(format!("click:{sel}"));
                match self.els[i].kind {
                    Kind::Button(Action::Post) => {
                        if let Some(e) = self
                            .els
                            .iter_mut()
                            .find(|e| matches!(e.kind, Kind::Editor { .. }))
                        {
                            self.posted.push(std::mem::take(&mut e.text));
                        }
                    }
                    Kind::Button(Action::Compose) => {
                        for e in &mut self.els {
                            e.hidden = false;
                        }
                    }
                    _ => {}
                }
            }
            Op::Prep { .. } => r.refused = self.prep(i),
            Op::Exec { payload, .. } => match self.prep(i) {
                Some(why) => r.refused = Some(why),
                None => {
                    let takes = match self.els[i].kind {
                        Kind::Editor { insert_text, .. } => insert_text,
                        Kind::Field(..) => true,
                        _ => false,
                    };
                    if takes {
                        self.els[i].text = payload.clone();
                    }
                }
            },
            Op::Synth { payload, .. } => match self.prep(i) {
                Some(why) => r.refused = Some(why),
                None => {
                    if let Kind::Editor {
                        synthetic: true, ..
                    } = self.els[i].kind
                    {
                        self.els[i].text = payload.clone();
                    }
                }
            },
        }
        r.text = self.els[i].text.trim().to_string();
        r
    }
}

/// The scripted DOM as a browser surface.
pub struct ScriptedDom(pub Rc<RefCell<Dom>>);

impl BrowserSurface for ScriptedDom {
    fn set_frame(&mut self, _: Bounds<Pixels>) {}
    fn set_visible(&mut self, _: bool) {}
    fn load_url(&mut self, url: &str) {
        let mut d = self.0.borrow_mut();
        d.loads.push(url.to_string());
        let (to, els) = (d.pages)(url, d.cookie);
        d.url = to;
        d.els = els;
        d.focused = None;
        d.loaded_at = d.now() + d.load_time;
        d.commits += 1;
        d.finish_pending = true;
    }
    fn back(&mut self) {}
    fn forward(&mut self) {}
    fn reload(&mut self) {}
    fn stop(&mut self) {}
    fn state(&self) -> PageState {
        let mut d = self.0.borrow_mut();
        d.tick();
        PageState {
            url: d.url.clone(),
            title: "Fixture Notes".into(),
            loading: d.loading(),
            commits: d.commits,
            finishes: d.finishes,
            ..PageState::default()
        }
    }
    fn refresh_blocking(&mut self) {}
    fn focus_parent(&mut self) {
        self.0.borrow_mut().web_focused = false;
    }
    fn page_has_keyboard(&self) -> bool {
        self.0.borrow().web_focused
    }
    fn edit_command(&mut self, selector: &str) {
        let mut d = self.0.borrow_mut();
        d.commands.push(selector.to_string());
        if selector != "paste:" || !d.web_focused {
            return;
        }
        let clip = d.clipboard.borrow().clone();
        if let Some(i) = d.focused {
            let takes = match d.els[i].kind {
                Kind::Editor { paste, .. } => paste,
                Kind::Field(..) => true,
                _ => false,
            };
            if takes {
                d.els[i].text = clip;
            }
        }
    }
    fn eval_json(&mut self, js: &str, reply: Sender<String>) {
        let mut d = self.0.borrow_mut();
        let out = match js::decode(js) {
            Some(op) => serde_json::to_string(&d.answer(&op)).unwrap(),
            None => d.capture.clone(),
        };
        let _ = reply.try_send(out);
    }
    fn focus_page(&mut self) {
        self.0.borrow_mut().web_focused = true;
    }
    fn has_edit_commands(&self) -> bool {
        self.0.borrow().edit_commands
    }
}

// ------------------------------------------------------------ the page

#[derive(Clone)]
enum PreviewAns {
    Keep,
    Edit(String),
    Cancel,
}

struct TestPage {
    surface: ScriptedDom,
    dom: Rc<RefCell<Dom>>,
    preview: PreviewAns,
    choice: PostChoice,
    /// Virtual time the Post sheet stays up.
    sheet_time: Duration,
    stop_at: Option<Duration>,
    saved: Option<String>,
    previews: Vec<Preview>,
    confirms: Vec<Confirm>,
    /// `BLYGGER_MACRO_TRACE=1`, and what it wrote.
    trace_on: bool,
    traces: Vec<Vec<String>>,
}

impl TestPage {
    fn new(dom: Rc<RefCell<Dom>>) -> TestPage {
        TestPage {
            surface: ScriptedDom(dom.clone()),
            dom,
            preview: PreviewAns::Keep,
            choice: PostChoice::Post,
            sheet_time: Duration::from_secs(2),
            stop_at: None,
            saved: None,
            previews: vec![],
            confirms: vec![],
            trace_on: false,
            traces: vec![],
        }
    }
    fn clock(&self) -> Rc<Cell<Duration>> {
        self.dom.borrow().clock.clone()
    }
    fn events(&self) -> Vec<String> {
        self.dom.borrow().events.borrow().clone()
    }
    fn clipboard(&self) -> String {
        self.dom.borrow().clipboard.borrow().clone()
    }
}

impl Page for TestPage {
    async fn eval(&mut self, op: &Op) -> Option<Reply> {
        let (tx, rx) = async_channel::bounded(1);
        self.surface.eval_json(&js::call(op), tx);
        Reply::parse(&rx.try_recv().ok()?)
    }
    fn state(&mut self) -> Option<PageState> {
        Some(self.surface.state())
    }
    fn load(&mut self, url: &str) {
        self.surface.load_url(url);
    }
    fn focus_page(&mut self) {
        self.surface.focus_page();
    }
    fn paste(&mut self) {
        self.surface.edit_command("paste:");
    }
    fn has_edit_commands(&self) -> bool {
        self.surface.has_edit_commands()
    }
    fn now(&self) -> Duration {
        self.clock().get()
    }
    async fn sleep(&mut self, d: Duration) {
        let c = self.clock();
        c.set(c.get() + d);
    }
    fn save_clipboard(&mut self) {
        if self.saved.is_none() {
            self.saved = Some(self.clipboard());
        }
    }
    fn set_clipboard(&mut self, text: &str) {
        *self.dom.borrow().clipboard.borrow_mut() = text.to_string();
    }
    fn restore_clipboard(&mut self) {
        if let Some(s) = self.saved.take() {
            *self.dom.borrow().clipboard.borrow_mut() = s;
        }
    }
    fn stopped(&self) -> bool {
        self.stop_at.is_some_and(|t| self.now() >= t)
    }
    async fn preview(&mut self, p: Preview) -> Option<String> {
        self.dom.borrow().events.borrow_mut().push("preview".into());
        let payload = p.payload.clone();
        self.previews.push(p);
        match self.preview.clone() {
            PreviewAns::Keep => Some(payload),
            PreviewAns::Edit(t) => Some(t),
            PreviewAns::Cancel => None,
        }
    }
    async fn confirm(&mut self, c: Confirm) -> PostChoice {
        self.dom.borrow().events.borrow_mut().push("confirm".into());
        self.confirms.push(c);
        let clock = self.clock();
        clock.set(clock.get() + self.sheet_time);
        self.choice
    }
    fn tracing(&self) -> bool {
        self.trace_on
    }
    fn trace(&mut self, lines: &[String]) {
        self.traces.push(lines.to_vec());
    }
}

/// Run a future that never really waits (the test page answers at once).
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    for _ in 0..1_000_000 {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("the run never finished");
}

// ------------------------------------------------------------ fixtures

const ORIGIN: &str = "https://social.example.com";
const PM: &str = "div.ProseMirror[contenteditable='true']";
const PAYLOAD: &str = "Tide pools forget, twice a day.\n\nhttps://blyg.example.com/f/tide/";

fn pm_editor() -> Kind {
    Kind::Editor {
        paste: true,
        insert_text: false,
        synthetic: false,
    }
}

/// `/notes`: a cookie-gated composer (signed out: a sign-in link).
fn notes_site(editor: Kind) -> Pages {
    Rc::new(move |url: &str, cookie: bool| {
        let els = if url.ends_with("/login") {
            vec![
                El::new(
                    &["input[type=password]"],
                    Kind::Field("password", "current-password"),
                ),
                El::new(&["input[type=email]"], Kind::Field("email", "username")),
                El::new(&["input.otp"], Kind::Field("text", "one-time-code")),
            ]
        } else if !cookie {
            vec![El::new(&["a.sign-in"], Kind::Static).text("Sign in")]
        } else {
            vec![
                El::new(&[PM], editor),
                El::new(&["button"], Kind::Button(Action::None)).text("Cancel"),
                El::new(&["button"], Kind::Button(Action::Post)).text("Post"),
                El::new(&["h1"], Kind::Static).text("Notes"),
            ]
        };
        (url.to_string(), els)
    })
}

fn site() -> SiteSpec {
    SiteSpec {
        id: "social".into(),
        title: "Social Notes".into(),
        origin: ORIGIN.into(),
        home: format!("{ORIGIN}/notes"),
        signed_out: Some("a.sign-in".into()),
        content_blocking: false,
        min_interval: Some("60s".into()),
    }
}

fn wait(sel: &str) -> Step {
    Step::WaitFor {
        selector: sel.into(),
        text: None,
        empty: false,
        absent: false,
        timeout: None,
    }
}

fn standard_steps() -> Vec<Step> {
    vec![
        Step::Open {
            url: format!("{ORIGIN}/notes"),
        },
        wait(PM),
        Step::Focus {
            selector: PM.into(),
        },
        Step::Insert {
            selector: PM.into(),
        },
        Step::Submit {
            selector: "button".into(),
            text: Some("Post".into()),
        },
        Step::WaitFor {
            selector: PM.into(),
            text: None,
            empty: true,
            absent: false,
            timeout: Some("15s".into()),
        },
        Step::Done {
            text: "Posted to Social Notes".into(),
        },
    ]
}

fn spec(steps: Vec<Step>) -> MacroSpec {
    MacroSpec {
        id: "cross-post-note".into(),
        title: "Cross-post to Social Notes…".into(),
        detail: String::new(),
        site: "social".into(),
        when: When::Published,
        template: recipe::DEFAULT_TEMPLATE.into(),
        tested: "unverified".into(),
        steps,
    }
}

fn macro_run(steps: Vec<Step>) -> MacroRun {
    MacroRun::new("cross-post", site(), spec(steps), PAYLOAD.into())
}

fn go(page: &mut TestPage, steps: Vec<Step>) -> Outcome {
    let run = macro_run(steps);
    block_on(runner::run(page, &run))
}

fn failed(out: &Outcome) -> &runner::MacroError {
    match out {
        Outcome::Failed(e) => e,
        o => panic!("expected a failure, got {o:?}"),
    }
}

// ------------------------------------------------------------ the runs

#[test]
fn a_run_posts_through_every_step_kind_and_only_after_both_sheets() {
    // A "New note" button opens the composer, which appears a second later.
    let pages: Pages = Rc::new(|url: &str, _| {
        let mut ed = El::new(&[PM], pm_editor()).at(Duration::from_secs(1));
        ed.hidden = true;
        (
            url.to_string(),
            vec![
                El::new(&["button.new"], Kind::Button(Action::Compose)).text("New note"),
                El::new(&["h1"], Kind::Static).text("Notes"),
                ed,
                El::new(&["button"], Kind::Button(Action::Post)).text("Post"),
            ],
        )
    });
    let dom = Dom::new(pages);
    *dom.borrow().clipboard.borrow_mut() = "the user's clipboard".into();
    let mut page = TestPage::new(dom.clone());
    let steps = vec![
        Step::Open {
            url: format!("{ORIGIN}/notes"),
        },
        Step::Assert {
            selector: "h1".into(),
            text: Some("Notes".into()),
        },
        Step::Click {
            selector: "button.new".into(),
            text: Some("New note".into()),
        },
        wait(PM),
        Step::Focus {
            selector: PM.into(),
        },
        Step::Insert {
            selector: PM.into(),
        },
        Step::Submit {
            selector: "button".into(),
            text: Some("Post".into()),
        },
        Step::WaitFor {
            selector: PM.into(),
            text: None,
            empty: true,
            absent: false,
            timeout: None,
        },
        Step::Done {
            text: "Posted to Social Notes".into(),
        },
    ];
    recipe::check_macro(&spec(steps.clone()), &[site()]).expect("a valid recipe");
    let out = go(&mut page, steps);
    assert_eq!(
        out,
        Outcome::Posted {
            message: "Posted to Social Notes".into(),
            via: Via::Paste,
        }
    );
    assert_eq!(dom.borrow().posted, vec![PAYLOAD.trim().to_string()]);
    assert_eq!(dom.borrow().loads, vec![format!("{ORIGIN}/notes")]);
    // Preview first, the composer click, then the Post sheet, then Post.
    assert_eq!(
        page.events(),
        vec!["preview", "click:button.new", "confirm", "click:button"]
    );
    let c = &page.confirms[0];
    assert!(!c.differs);
    assert!(insert::same_text(&c.read_back, PAYLOAD));
    assert_eq!(page.previews[0].tested, "unverified");
    assert_eq!(page.clipboard(), "the user's clipboard", "restored");
    assert_eq!(dom.borrow().commands, vec!["paste:"]);
}

#[test]
fn cancelling_the_preview_runs_nothing() {
    let dom = Dom::new(notes_site(pm_editor()));
    let mut page = TestPage::new(dom.clone());
    page.preview = PreviewAns::Cancel;
    let out = go(&mut page, standard_steps());
    assert_eq!(
        out,
        Outcome::Cancelled {
            after_submit: false
        }
    );
    assert!(dom.borrow().loads.is_empty(), "nothing loaded");
    assert!(!out.counts_as_post());
}

#[test]
fn the_edited_preview_is_what_goes_in() {
    let dom = Dom::new(notes_site(pm_editor()));
    let mut page = TestPage::new(dom.clone());
    page.preview = PreviewAns::Edit("Shorter.\r\n".into());
    let out = go(&mut page, standard_steps());
    assert!(matches!(out, Outcome::Posted { .. }), "{out:?}");
    assert_eq!(dom.borrow().posted, vec!["Shorter."]);
    // An emptied preview is a cancel.
    let dom = Dom::new(notes_site(pm_editor()));
    let mut page = TestPage::new(dom.clone());
    page.preview = PreviewAns::Edit("  \n ".into());
    assert_eq!(
        go(&mut page, standard_steps()),
        Outcome::Cancelled {
            after_submit: false
        }
    );
}

#[test]
fn the_post_sheet_gates_submit() {
    for (choice, want) in [
        (
            PostChoice::Cancel,
            Outcome::Cancelled {
                after_submit: false,
            },
        ),
        (PostChoice::Myself, Outcome::Handed { manual: false }),
    ] {
        let dom = Dom::new(notes_site(pm_editor()));
        *dom.borrow().clipboard.borrow_mut() = "mine".into();
        let mut page = TestPage::new(dom.clone());
        page.choice = choice;
        assert_eq!(go(&mut page, standard_steps()), want);
        assert!(dom.borrow().posted.is_empty(), "{choice:?}: not posted");
        assert!(
            !page.events().iter().any(|e| e.starts_with("click")),
            "{choice:?}: Post never clicked"
        );
        // The text stays in the box for the user.
        let text = dom.borrow().els[0].text.clone();
        assert!(insert::same_text(&text, PAYLOAD));
        assert_eq!(page.clipboard(), "mine");
    }
}

#[test]
fn read_back_compares_as_a_composer_shows_text() {
    assert!(insert::same_text("a  b\n\nc\u{a0}d ", "a b c d"));
    assert!(insert::same_text(" Tide\n", "Tide"));
    assert!(!insert::same_text("a b", "a b c"));
    assert!(!insert::same_text("", "a"));
}

#[test]
fn a_box_changed_before_the_sheet_shows_red() {
    // Pasted fine, then (before the sheet) the site rewrote it.
    let pages: Pages = Rc::new(|url: &str, _| {
        let mut ed = El::new(&[PM], pm_editor());
        ed.later = Some((Duration::from_millis(600), "Tide pools forget".into()));
        (
            url.to_string(),
            vec![
                ed,
                El::new(&["button"], Kind::Button(Action::Post)).text("Post"),
                El::new(&["#saved"], Kind::Static).at(Duration::from_secs(1)),
            ],
        )
    });
    let dom = Dom::new(pages);
    let mut page = TestPage::new(dom.clone());
    let mut steps = standard_steps();
    // Something to wait for between insert and submit: the sheet comes
    // after the rewrite.
    steps.insert(4, wait("#saved"));
    let out = go(&mut page, steps);
    let c = page.confirms.first().expect("the Post sheet showed");
    assert!(c.differs, "{c:?}");
    assert_eq!(c.read_back, "Tide pools forget");
    // Post still posts what the box holds (the user saw it and chose).
    assert!(matches!(out, Outcome::Posted { .. }), "{out:?}");
    assert_eq!(dom.borrow().posted, vec!["Tide pools forget"]);
}

#[test]
fn credential_fields_are_refused() {
    for sel in ["input[type=password]", "input[type=email]", "input.otp"] {
        let dom = Dom::new(notes_site(pm_editor()));
        let mut page = TestPage::new(dom.clone());
        let steps = vec![
            Step::Open {
                url: format!("{ORIGIN}/login"),
            },
            Step::Insert {
                selector: sel.into(),
            },
            Step::Submit {
                selector: "button".into(),
                text: None,
            },
            Step::Done {
                text: "never".into(),
            },
        ];
        let out = go(&mut page, steps);
        let e = failed(&out);
        assert!(matches!(e.reason, Reason::Refused(_)), "{sel}: {e:?}");
        assert_eq!((e.step, e.do_), (2, "insert"));
        assert!(!e.after_submit);
        assert!(
            dom.borrow().els.iter().all(|el| el.text.is_empty()),
            "{sel}: nothing typed"
        );
        assert!(dom.borrow().commands.is_empty(), "{sel}: no paste");
        assert!(page.confirms.is_empty());
        assert!(e.detail("unverified").starts_with("Nothing was posted."));
    }
    // A non-editable element isn't typed into either.
    let dom = Dom::new(notes_site(pm_editor()));
    let mut page = TestPage::new(dom);
    let mut steps = standard_steps();
    steps[3] = Step::Insert {
        selector: "h1".into(),
    };
    let out = go(&mut page, steps);
    assert_eq!(
        failed(&out).reason,
        Reason::Refused("something that isn't a text box".into())
    );
}

#[test]
fn signed_out_stops_and_asks_to_sign_in() {
    let dom = Dom::new(notes_site(pm_editor()));
    dom.borrow_mut().cookie = false;
    let mut page = TestPage::new(dom.clone());
    let out = go(&mut page, standard_steps());
    let e = failed(&out);
    assert_eq!(e.reason, Reason::SignedOut);
    assert_eq!(e.step, 1);
    assert_eq!(
        e.headline(&site(), None),
        "Sign in to Social Notes in this pane, then run the macro again"
    );
    assert_eq!(e.detail("unverified"), "Nothing was posted.");
    // Signed in (by hand, in the pane): the same run posts.
    dom.borrow_mut().cookie = true;
    let mut page = TestPage::new(dom.clone());
    assert!(matches!(
        go(&mut page, standard_steps()),
        Outcome::Posted { .. }
    ));
}

#[test]
fn leaving_the_origin_aborts() {
    // A redirect to another origin on open.
    let dom = Dom::new(Rc::new(|_: &str, _| {
        ("https://login.other.example/sso".to_string(), vec![])
    }));
    let mut page = TestPage::new(dom.clone());
    let out = go(&mut page, standard_steps());
    let e = failed(&out);
    assert_eq!(
        e.reason,
        Reason::OffOrigin("https://login.other.example/sso".into())
    );
    assert_eq!(e.step, 1);
    assert!(
        e.headline(&site(), None)
            .contains("left https://social.example.com for https://login.other.example")
    );
    // The page navigating away mid-run (before the composer appears).
    let pages: Pages = Rc::new(|url: &str, _| {
        (
            url.to_string(),
            vec![El::new(&[PM], pm_editor()).at(Duration::from_secs(3))],
        )
    });
    let dom = Dom::new(pages);
    dom.borrow_mut().nav_at = Some((Duration::from_secs(1), "https://ads.example.net/".into()));
    let mut page = TestPage::new(dom.clone());
    let out = go(&mut page, standard_steps());
    let e = failed(&out);
    assert!(matches!(e.reason, Reason::OffOrigin(_)), "{e:?}");
    assert_eq!((e.step, e.do_), (2, "waitFor"));
    assert!(dom.borrow().posted.is_empty());
    // An `open` off the site (a recipe the loader would refuse) never loads.
    let dom = Dom::new(notes_site(pm_editor()));
    let mut page = TestPage::new(dom.clone());
    let mut steps = standard_steps();
    steps[0] = Step::Open {
        url: "https://social.example.com.evil.example/notes".into(),
    };
    let out = go(&mut page, steps);
    assert!(matches!(failed(&out).reason, Reason::OffOrigin(_)));
    assert!(dom.borrow().loads.is_empty());
}

#[test]
fn steps_time_out_and_say_which() {
    let dom = Dom::new(notes_site(pm_editor()));
    let mut page = TestPage::new(dom.clone());
    let mut steps = standard_steps();
    steps[1] = Step::WaitFor {
        selector: "div.missing".into(),
        text: None,
        empty: false,
        absent: false,
        timeout: Some("5s".into()),
    };
    let out = go(&mut page, steps.clone());
    let e = failed(&out);
    assert_eq!(e.reason, Reason::Timeout);
    assert_eq!((e.step, e.do_), (2, "waitFor"));
    let t = page.now();
    assert!(
        t >= Duration::from_secs(5) && t < Duration::from_secs(6),
        "{t:?}"
    );
    assert_eq!(
        e.headline(&site(), steps.get(1)),
        "Social Notes: div.missing didn't appear (step 2, waitFor)"
    );
    assert_eq!(
        e.detail("unverified"),
        "Nothing was posted. The recipe hasn't been checked by hand (tested = \"unverified\"); \
         it lives in extension.toml."
    );
    assert_eq!(
        e.detail("2026-10-01"),
        "Nothing was posted. The recipe was last checked 2026-10-01; it lives in extension.toml."
    );
    // Focus waits the default 10 s.
    let dom = Dom::new(notes_site(pm_editor()));
    let mut page = TestPage::new(dom);
    let mut steps = standard_steps();
    steps.remove(1);
    steps[1] = Step::Focus {
        selector: "div.nope".into(),
    };
    let out = go(&mut page, steps);
    assert_eq!(failed(&out).reason, Reason::Timeout);
    assert!(page.now() >= recipe::DEFAULT_STEP_TIMEOUT);
    // An assert doesn't wait.
    let dom = Dom::new(notes_site(pm_editor()));
    let mut page = TestPage::new(dom);
    let mut steps = standard_steps();
    steps[1] = Step::Assert {
        selector: "h1".into(),
        text: Some("Elsewhere".into()),
    };
    let out = go(&mut page, steps);
    assert_eq!(failed(&out).reason, Reason::Missing);
    assert!(page.now() < Duration::from_secs(1));
}

/// `/notes` redirects to `/home`, and `/home` is the composer.
fn redirecting_site() -> Pages {
    let inner = notes_site(pm_editor());
    Rc::new(move |url: &str, cookie: bool| {
        let to = if url.ends_with("/notes") {
            format!("{ORIGIN}/home")
        } else {
            url.to_string()
        };
        let (_, els) = inner(&to, cookie);
        (to, els)
    })
}

#[test]
fn open_counts_a_redirect_back_to_the_page_already_shown() {
    let dom = Dom::new(redirecting_site());
    let mut page = TestPage::new(dom.clone());
    // The pane is already on /home (a run before this one).
    page.load(&format!("{ORIGIN}/home"));
    block_on(page.sleep(Duration::from_secs(1)));
    assert_eq!(page.surface.state().url, format!("{ORIGIN}/home"));
    let out = go(&mut page, standard_steps());
    assert!(matches!(out, Outcome::Posted { .. }), "{out:?}");
    assert_eq!(dom.borrow().posted, vec![PAYLOAD.to_string()]);
    // Loaded as soon as the navigation finished, not after a settle.
    assert_eq!(
        dom.borrow().loads,
        vec![format!("{ORIGIN}/home"), format!("{ORIGIN}/notes")]
    );
}

#[test]
fn open_counts_a_complete_document_when_loading_never_ends() {
    // `loading` stays true (a request that never ends), but the document
    // is complete: loaded after the settle.
    let dom = Dom::new(notes_site(pm_editor()));
    dom.borrow_mut().never_finish = true;
    dom.borrow_mut().ready = Some("complete");
    let mut page = TestPage::new(dom.clone());
    let out = go(&mut page, standard_steps());
    assert!(matches!(out, Outcome::Posted { .. }), "{out:?}");
    // Only `interactive`: after the long settle.
    let dom = Dom::new(notes_site(pm_editor()));
    dom.borrow_mut().never_finish = true;
    dom.borrow_mut().ready = Some("interactive");
    let mut page = TestPage::new(dom.clone());
    let out = go(&mut page, standard_steps()[..2].to_vec());
    assert!(matches!(out, Outcome::Posted { .. }), "{out:?}");
    assert!(page.now() >= runner::LONG_SETTLE, "{:?}", page.now());
    // Never even interactive: the open times out, after 30 s (not 10).
    let dom = Dom::new(notes_site(pm_editor()));
    dom.borrow_mut().never_finish = true;
    dom.borrow_mut().ready = Some("loading");
    let mut page = TestPage::new(dom.clone());
    let out = go(&mut page, standard_steps());
    let e = failed(&out);
    assert_eq!((e.step, e.do_, &e.reason), (1, "open", &Reason::Timeout));
    assert!(page.now() >= recipe::MAX_STEP_TIMEOUT, "{:?}", page.now());
}

const SECRET: &str = "SECRET-BODY-7f3a";

/// The composer plus page text and a button whose label is the payload's
/// start (a preview card): neither may reach an outline.
fn outline_site() -> Pages {
    Rc::new(|url: &str, _| {
        (
            url.to_string(),
            vec![
                El::new(&["h1"], Kind::Static).text(SECRET),
                El::new(&["p.body"], Kind::Static).text(&format!("{SECRET} more text")),
                El::new(
                    &["div.ProseMirror.tiptap[contenteditable='true']", PM],
                    pm_editor(),
                ),
                El::new(&["button.card", "button"], Kind::Button(Action::None))
                    .text("Tide pools forget, twice a day."),
                El::new(&["button.primary", "button"], Kind::Button(Action::Post)).text("Post"),
            ],
        )
    })
}

#[test]
fn a_missed_selector_logs_an_outline_without_page_text_or_the_payload() {
    let dom = Dom::new(outline_site());
    let mut page = TestPage::new(dom.clone());
    let mut steps = standard_steps();
    steps[0] = Step::Open {
        url: format!("{ORIGIN}/notes?ref=SECRETQUERY"),
    };
    steps[1] = Step::WaitFor {
        selector: "div.missing".into(),
        text: None,
        empty: false,
        absent: false,
        timeout: Some("2s".into()),
    };
    let out = go(&mut page, steps);
    let e = failed(&out);
    assert_eq!(e.reason, Reason::Timeout);
    let dom_text = e.dom.join("\n");
    assert!(e.dom.iter().all(|l| l.starts_with("  dom: ")), "{dom_text}");
    assert!(
        dom_text.contains("  dom: page path=/notes title-len=13 candidates=3 listed=3"),
        "{dom_text}"
    );
    assert!(
        dom_text.contains(
            "  dom: div.ProseMirror.tiptap ce=\"true\" aria=\"Write a note\" visible=yes"
        ),
        "{dom_text}"
    );
    assert!(
        dom_text.contains("button.primary visible=yes label=\"Post\""),
        "{dom_text}"
    );
    assert!(
        dom_text.contains("button.card visible=yes label=\"(the text)\""),
        "{dom_text}"
    );
    for never in [SECRET, "SECRETQUERY", "Tide pools", "twice a day"] {
        assert!(!dom_text.contains(never), "{never} in {dom_text}");
    }
    // The log entry is the error line, then the outline.
    let line = runner::log_line("t", &macro_run(vec![]), &out);
    let lines: Vec<&str> = line.lines().collect();
    assert!(lines[0].starts_with("t cross-post-note error step=2 do=waitFor reason=timeout"));
    assert!(lines[1].starts_with("  dom: page path=/notes"));
    assert!(!line.contains(SECRET) && !line.contains("Tide pools"));
    // A click that finds nothing logs one too.
    let dom = Dom::new(outline_site());
    let mut page = TestPage::new(dom);
    let steps = vec![
        standard_steps()[0].clone(),
        Step::Click {
            selector: "button".into(),
            text: Some("New note".into()),
        },
        Step::Insert {
            selector: PM.into(),
        },
        standard_steps()[4].clone(),
    ];
    let out = go(&mut page, steps);
    let e = failed(&out);
    assert_eq!((e.step, e.do_), (2, "click"));
    assert!(e.dom.len() > 1, "{:?}", e.dom);
    // Off the origin: no outline (the page isn't the site's).
    let dom = Dom::new(Rc::new(|_: &str, _| {
        ("https://login.other.example/sso".to_string(), vec![])
    }));
    let mut page = TestPage::new(dom);
    assert!(failed(&go(&mut page, standard_steps())).dom.is_empty());
}

#[test]
fn an_outline_is_capped_at_3_kb() {
    let el = js::OutlineEl {
        tag: "button".into(),
        id: "x".repeat(40),
        cls: vec!["c".repeat(40); 4],
        aria: "a".repeat(40),
        label: "l".repeat(30),
        vis: true,
        ..js::OutlineEl::default()
    };
    let o = js::Outline {
        path: "/home".into(),
        title_len: 8,
        total: 400,
        els: vec![el; 25],
    };
    let lines = runner::outline_lines(&o, PAYLOAD);
    let bytes: usize = lines.iter().map(|l| l.len() + 1).sum();
    assert!(bytes <= runner::OUTLINE_BYTES, "{bytes}");
    assert_eq!(lines.last().unwrap(), "  dom: (cut at 3 KB)");
}

#[test]
fn a_trace_outlines_the_page_after_every_step() {
    let dom = Dom::new(outline_site());
    let mut page = TestPage::new(dom.clone());
    page.trace_on = true;
    let out = go(&mut page, standard_steps());
    assert!(matches!(out, Outcome::Posted { .. }), "{out:?}");
    let firsts: Vec<&str> = page.traces.iter().map(|t| t[0].as_str()).collect();
    assert_eq!(
        firsts,
        [
            "trace step=1 do=open ok",
            "trace step=2 do=waitFor ok",
            "trace step=3 do=focus ok",
            "trace step=4 do=insert ok",
            "trace step=5 do=submit ok",
            "trace step=6 do=waitFor ok",
        ]
    );
    let all = page.traces.concat().join("\n");
    assert!(all.contains("  dom: div.ProseMirror.tiptap"));
    assert!(
        !all.contains(SECRET) && !all.contains("Tide pools"),
        "{all}"
    );
    let entry = runner::trace_entry("t", "cross-post-note", &page.traces[0]);
    assert!(
        entry.starts_with("t cross-post-note trace step=1 do=open ok\n  dom: page path=/notes")
    );
    // Off unless asked.
    let mut page = TestPage::new(Dom::new(outline_site()));
    go(&mut page, standard_steps());
    assert!(page.traces.is_empty());
}

#[test]
fn the_ring_never_starts_inside_an_outline() {
    let log = runner::ring_append("", "a error\n  dom: one\n  dom: two", 10);
    assert_eq!(log, "a error\n  dom: one\n  dom: two\n");
    let log = runner::ring_append(&log, "b posted", 3);
    assert_eq!(log, "b posted\n", "orphaned dom lines go with their entry");
}

#[test]
fn a_run_is_capped_but_the_sheets_dont_count() {
    // Seven waits of 29 s each: over MAX_RUN.
    let pages: Pages = Rc::new(|url: &str, _| {
        let mut els: Vec<El> = (0..7)
            .map(|i| {
                El::new(&[&format!("#w{i}")], Kind::Static).at(Duration::from_secs(29 * (i + 1)))
            })
            .collect();
        els.push(El::new(&[PM], pm_editor()));
        els.push(El::new(&["button"], Kind::Button(Action::Post)).text("Post"));
        (url.to_string(), els)
    });
    let mut steps = standard_steps();
    for i in (0..7).rev() {
        steps.insert(
            1,
            Step::WaitFor {
                selector: format!("#w{i}"),
                text: None,
                empty: false,
                absent: false,
                timeout: Some("30s".into()),
            },
        );
    }
    let mut page = TestPage::new(Dom::new(pages.clone()));
    let out = go(&mut page, steps);
    assert_eq!(failed(&out).reason, Reason::RunTooLong);
    // A Post sheet left up for 10 minutes doesn't end the run.
    let dom = Dom::new(notes_site(pm_editor()));
    let mut page = TestPage::new(dom.clone());
    page.sheet_time = Duration::from_secs(600);
    assert!(matches!(
        go(&mut page, standard_steps()),
        Outcome::Posted { .. }
    ));
}

#[test]
fn the_insert_chain_falls_back_in_order() {
    // No edit commands (WebView2): straight to insertText.
    let dom = Dom::new(notes_site(Kind::Editor {
        paste: true,
        insert_text: true,
        synthetic: false,
    }));
    dom.borrow_mut().edit_commands = false;
    let mut page = TestPage::new(dom.clone());
    let out = go(&mut page, standard_steps());
    assert!(
        matches!(
            out,
            Outcome::Posted {
                via: Via::InsertText,
                ..
            }
        ),
        "{out:?}"
    );
    assert!(dom.borrow().commands.is_empty());
    // Only a synthetic paste works.
    let dom = Dom::new(notes_site(Kind::Editor {
        paste: false,
        insert_text: false,
        synthetic: true,
    }));
    let mut page = TestPage::new(dom.clone());
    let out = go(&mut page, standard_steps());
    assert!(
        matches!(
            out,
            Outcome::Posted {
                via: Via::SyntheticPaste,
                ..
            }
        ),
        "{out:?}"
    );
    // Nothing works: the text is left on the clipboard, nothing is clicked.
    let dom = Dom::new(notes_site(Kind::Editor {
        paste: false,
        insert_text: false,
        synthetic: false,
    }));
    *dom.borrow().clipboard.borrow_mut() = "mine".into();
    let mut page = TestPage::new(dom.clone());
    let out = go(&mut page, standard_steps());
    assert_eq!(out, Outcome::Handed { manual: true });
    assert_eq!(page.clipboard(), PAYLOAD.trim(), "left for ⌘V");
    assert!(page.confirms.is_empty());
    assert!(dom.borrow().posted.is_empty());
    assert!(out.counts_as_post());
}

#[test]
fn the_clipboard_comes_back_whatever_happens() {
    for choice in [PostChoice::Post, PostChoice::Cancel, PostChoice::Myself] {
        let dom = Dom::new(notes_site(pm_editor()));
        *dom.borrow().clipboard.borrow_mut() = "a note to self".into();
        let mut page = TestPage::new(dom);
        page.choice = choice;
        let _ = go(&mut page, standard_steps());
        assert_eq!(page.clipboard(), "a note to self", "{choice:?}");
    }
    // A failure after the paste too.
    let dom = Dom::new(notes_site(pm_editor()));
    *dom.borrow().clipboard.borrow_mut() = "a note to self".into();
    let mut page = TestPage::new(dom);
    let mut steps = standard_steps();
    steps[4] = Step::Submit {
        selector: "button".into(),
        text: Some("Publish".into()),
    };
    let out = go(&mut page, steps);
    assert_eq!(failed(&out).reason, Reason::Timeout);
    assert_eq!(page.clipboard(), "a note to self");
}

#[test]
fn after_submit_a_failure_says_it_may_have_posted() {
    // The site ignores the click: the box never empties.
    let pages: Pages = Rc::new(|url: &str, _| {
        (
            url.to_string(),
            vec![
                El::new(&[PM], pm_editor()),
                El::new(&["button"], Kind::Button(Action::Ignore)).text("Post"),
            ],
        )
    });
    let mut page = TestPage::new(Dom::new(pages));
    let out = go(&mut page, standard_steps());
    let e = failed(&out);
    assert_eq!((e.step, e.reason.clone()), (6, Reason::Timeout));
    assert!(e.after_submit);
    assert!(out.counts_as_post());
    assert!(
        e.detail("unverified")
            .starts_with("Post was clicked, so it may have been posted")
    );
    assert_eq!(
        e.headline(&site(), standard_steps().get(5)),
        format!("Social Notes: {PM} didn't empty (step 6, waitFor)")
    );
}

#[test]
fn stop_ends_the_run() {
    let pages: Pages = Rc::new(|url: &str, _| {
        (
            url.to_string(),
            vec![El::new(&[PM], pm_editor()).at(Duration::from_secs(8))],
        )
    });
    let dom = Dom::new(pages);
    let mut page = TestPage::new(dom.clone());
    page.stop_at = Some(Duration::from_secs(2));
    assert_eq!(
        go(&mut page, standard_steps()),
        Outcome::Cancelled {
            after_submit: false
        }
    );
    assert!(page.now() < Duration::from_secs(3));
}

#[test]
fn an_invalid_selector_is_reported() {
    let dom = Dom::new(notes_site(pm_editor()));
    let mut page = TestPage::new(dom);
    let mut steps = standard_steps();
    steps[1] = wait("div[!!");
    assert_eq!(failed(&go(&mut page, steps)).reason, Reason::BadSelector);
}

#[test]
fn min_interval_paces_posts_per_site() {
    let mut p = Pacing::default();
    let t = Instant::now();
    let min = Duration::from_secs(60);
    assert_eq!(p.wait_left("cross-post/social", min, t), None);
    p.record("cross-post/social", t);
    assert_eq!(
        p.wait_left("cross-post/social", min, t + Duration::from_secs(20)),
        Some(Duration::from_secs(40))
    );
    assert_eq!(
        p.wait_left("cross-post/social", min, t + Duration::from_secs(61)),
        None
    );
    assert_eq!(p.wait_left("cross-post/other", min, t), None);
    assert_eq!(p.wait_left("cross-post/social", Duration::ZERO, t), None);
    assert_eq!(macro_run(vec![]).pace_key(), "cross-post/social");
    assert_eq!(site().min_interval(), min);
}

#[test]
fn the_log_is_a_ring_without_the_text() {
    let mut log = String::new();
    for i in 0..5 {
        log = runner::ring_append(&log, &format!("line {i}"), 3);
    }
    assert_eq!(log, "line 2\nline 3\nline 4\n");
    let run = macro_run(standard_steps());
    let line = runner::log_line(
        "2026-10-08T12:00:00Z",
        &run,
        &Outcome::Failed(runner::MacroError {
            step: 2,
            do_: "waitFor",
            selector: Some(PM.into()),
            reason: Reason::Timeout,
            after_submit: false,
            dom: vec![],
        }),
    );
    assert!(line.starts_with(
        "2026-10-08T12:00:00Z cross-post-note error step=2 do=waitFor reason=timeout"
    ));
    let posted = runner::log_line(
        "t",
        &run,
        &Outcome::Posted {
            message: "Posted".into(),
            via: Via::Paste,
        },
    );
    assert!(!posted.contains("Tide pools"), "never the payload");
    let dir = tempfile::tempdir().unwrap();
    super::append_log(dir.path(), "cross-post", "one");
    super::append_log(dir.path(), "cross-post", "two");
    let path = super::log_path(dir.path(), "cross-post");
    assert!(path.ends_with("extensions/cross-post/macro.log"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "one\ntwo\n");
}

/// The pane never reads cookies: wry's cookie getters (`cookies`,
/// `cookies_for_url`) aren't called anywhere in the browser's sources.
#[test]
fn the_browser_never_reads_its_cookie_jar() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/browser");
    let needles = [["cookies", "("].concat(), ["cookies_for_url", "("].concat()];
    let mut stack = vec![root];
    let mut seen = 0;
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                seen += 1;
                let src = std::fs::read_to_string(&p).unwrap();
                for n in &needles {
                    assert!(!src.contains(n.as_str()), "{} calls {n}", p.display());
                }
            }
        }
    }
    assert!(seen >= 10, "found the sources ({seen})");
}

// ------------------------------------------------------------ in the app

mod ui {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    use gpui_kit::{Entity, TestAppContext, VisualTestContext};

    use super::*;
    use crate::app::browser::surface::FactoryGlobal;
    use crate::app::{MainView, Sheet};

    fn setup(
        cx: &mut TestAppContext,
        dom: Rc<RefCell<Dom>>,
    ) -> (Entity<MainView>, &mut VisualTestContext) {
        use std::sync::Arc;

        use blyg_core::config::MemoryTokenStore;
        use blyg_core::{Backend, ConfigStore};

        use crate::fake::{FakeBackend, Timing};
        use crate::prefs::Prefs;

        let config = "blyg-url = https://blyg.example.com\n".to_string();
        let prefs = Prefs::from_config(ConfigStore::in_memory(&config).config());
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::app::bind_keys(cx);
            crate::settings::init(
                ConfigStore::in_memory(&config),
                Arc::new(MemoryTokenStore::default()),
                None,
                cx,
            );
            cx.set_global(FactoryGlobal(Rc::new(move |_, _, _, _| {
                Ok(Box::new(ScriptedDom(dom.clone())) as Box<dyn BrowserSurface>)
            })));
        });
        let fake = Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache());
        let backend: Arc<dyn Backend> = fake.clone();
        let (view, cx) = cx.add_window_view(move |window, cx| {
            MainView::new(
                backend,
                Some(fake),
                prefs,
                std::time::Instant::now(),
                window,
                cx,
            )
        });
        cx.run_until_parked();
        (view, cx)
    }

    /// Let the run's timers fire until `done` holds.
    fn until(
        view: &Entity<MainView>,
        cx: &mut VisualTestContext,
        done: impl Fn(&MainView) -> bool,
    ) {
        for _ in 0..400 {
            cx.run_until_parked();
            if view.read_with(cx, |v, _| done(v)) {
                return;
            }
            cx.executor().advance_clock(Duration::from_millis(200));
        }
        panic!("never got there");
    }

    fn quick_dom() -> Rc<RefCell<Dom>> {
        // The test platform's clipboard isn't the scripted DOM's: insertText.
        let dom = Dom::new(notes_site(Kind::Editor {
            paste: true,
            insert_text: true,
            synthetic: false,
        }));
        dom.borrow_mut().load_time = Duration::ZERO;
        dom
    }

    fn start(view: &Entity<MainView>, cx: &mut VisualTestContext) {
        view.update_in(cx, |v, window, cx| {
            super::super::start(v, macro_run(standard_steps()), window, cx)
        });
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn both_sheets_gate_the_run_in_the_app(cx: &mut TestAppContext) {
        let dom = quick_dom();
        let (view, cx) = setup(cx, dom.clone());
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("mine".into()));
        start(&view, cx);
        // The Preview sheet, before anything loads.
        view.read_with(cx, |v, cx| {
            let Some(Sheet::MacroPreview(s)) = &v.sheet else {
                panic!("no preview sheet");
            };
            assert_eq!(s.info.tested, "unverified");
            assert_eq!(s.editor.read(cx).value().to_string(), PAYLOAD);
            assert!(v.browser.automation.running());
        });
        assert!(dom.borrow().loads.is_empty());
        view.update_in(cx, |v, window, cx| v.macro_preview_answer(true, window, cx));
        until(&view, cx, |v| matches!(v.sheet, Some(Sheet::MacroPost(_))));
        // The text is in; nothing is posted until Post.
        assert_eq!(dom.borrow().loads, vec![format!("{ORIGIN}/notes")]);
        assert!(dom.borrow().posted.is_empty());
        view.read_with(cx, |v, _| {
            let Some(Sheet::MacroPost(s)) = &v.sheet else {
                unreachable!()
            };
            assert!(!s.info.differs);
            assert!(v.browser.open, "the pane is open");
            assert_eq!(v.browser.mode, crate::app::browser::OpenMode::Full);
        });
        // esc while it runs: the pane stays.
        view.update_in(cx, |v, window, cx| v.close_browser(window, cx));
        view.read_with(cx, |v, _| assert!(v.browser.open));
        view.update_in(cx, |v, window, cx| {
            v.macro_post_answer(PostChoice::Post, window, cx)
        });
        until(&view, cx, |v| !v.browser.automation.running());
        assert_eq!(dom.borrow().posted, vec![PAYLOAD.trim().to_string()]);
        // The paste went through the clipboard, and it's the user's again.
        assert_eq!(dom.borrow().commands, vec!["paste:"]);
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
            Some("mine")
        );
        view.read_with(cx, |v, _| {
            assert_eq!(
                v.toast.as_ref().map(|t| t.text.to_string()).as_deref(),
                Some("Posted to Social Notes")
            );
            assert!(v.browser.automation.run_unblocked.borrow().is_empty());
        });
        // Again at once: min-interval, no sheet.
        start(&view, cx);
        view.read_with(cx, |v, _| {
            assert!(v.sheet.is_none());
            assert!(!v.browser.automation.running());
            let t = v.toast.as_ref().unwrap().text.to_string();
            assert!(t.starts_with("Social Notes: wait "), "{t}");
        });
        // Now the pane closes.
        view.update_in(cx, |v, window, cx| v.close_browser(window, cx));
        view.read_with(cx, |v, _| assert!(!v.browser.open));
    }

    #[gpui_kit::test]
    fn cancel_on_the_post_sheet_posts_nothing(cx: &mut TestAppContext) {
        let dom = quick_dom();
        let (view, cx) = setup(cx, dom.clone());
        start(&view, cx);
        // The shield is off for the site during the run only.
        view.read_with(cx, |v, _| {
            assert!(
                v.browser
                    .automation
                    .run_unblocked
                    .borrow()
                    .contains("social.example.com")
            );
        });
        view.update_in(cx, |v, window, cx| v.macro_preview_answer(true, window, cx));
        until(&view, cx, |v| matches!(v.sheet, Some(Sheet::MacroPost(_))));
        view.update_in(cx, |v, window, cx| {
            v.macro_post_answer(PostChoice::Cancel, window, cx)
        });
        until(&view, cx, |v| !v.browser.automation.running());
        assert!(dom.borrow().posted.is_empty());
        view.read_with(cx, |v, _| {
            assert_eq!(
                v.toast.as_ref().map(|t| t.text.to_string()).as_deref(),
                Some("Cancelled. Nothing was posted.")
            );
            assert!(v.browser.automation.run_unblocked.borrow().is_empty());
        });
        // A cancel doesn't count for min-interval.
        start(&view, cx);
        view.read_with(cx, |v, _| {
            assert!(matches!(v.sheet, Some(Sheet::MacroPreview(_))))
        });
        // Closing the sheet another way cancels too.
        view.update_in(cx, |v, window, cx| v.close_sheet(window, cx));
        until(&view, cx, |v| !v.browser.automation.running());
        assert!(dom.borrow().loads.len() == 1);
    }

    /// The sheets' own keys, as typed: ⌘⏎ with the Preview's text box
    /// focused continues (it isn't Publish there), ⏎ posts, esc cancels.
    #[gpui_kit::test]
    fn the_sheets_answer_their_keys(cx: &mut TestAppContext) {
        let dom = quick_dom();
        let (view, cx) = setup(cx, dom.clone());
        start(&view, cx);
        let focused = view.update_in(cx, |v, window, cx| {
            use gpui_kit::Focusable as _;
            let Some(Sheet::MacroPreview(s)) = &v.sheet else {
                panic!("no preview sheet");
            };
            s.editor.read(cx).focus_handle(cx).is_focused(window)
        });
        assert!(focused, "the text box has the keyboard");
        cx.simulate_keystrokes("cmd-enter");
        until(&view, cx, |v| matches!(v.sheet, Some(Sheet::MacroPost(_))));
        cx.simulate_keystrokes("enter");
        until(&view, cx, |v| !v.browser.automation.running());
        assert_eq!(dom.borrow().posted, vec![PAYLOAD.trim().to_string()]);
    }

    /// esc in the Preview's text box cancels; nothing loads.
    #[gpui_kit::test]
    fn esc_in_the_preview_text_cancels(cx: &mut TestAppContext) {
        let dom = quick_dom();
        let (view, cx) = setup(cx, dom.clone());
        start(&view, cx);
        assert!(view.read_with(cx, |v, _| matches!(v.sheet, Some(Sheet::MacroPreview(_)))));
        cx.simulate_keystrokes("escape");
        until(&view, cx, |v| !v.browser.automation.running());
        assert!(view.read_with(cx, |v, _| v.sheet.is_none()));
        assert!(dom.borrow().loads.is_empty());
    }

    /// esc on the Post sheet cancels; nothing is posted.
    #[gpui_kit::test]
    fn esc_on_the_post_sheet_cancels(cx: &mut TestAppContext) {
        let dom = quick_dom();
        let (view, cx) = setup(cx, dom.clone());
        start(&view, cx);
        cx.simulate_keystrokes("cmd-enter");
        until(&view, cx, |v| matches!(v.sheet, Some(Sheet::MacroPost(_))));
        cx.simulate_keystrokes("escape");
        until(&view, cx, |v| !v.browser.automation.running());
        assert!(dom.borrow().posted.is_empty());
        view.read_with(cx, |v, _| {
            assert_eq!(
                v.toast.as_ref().map(|t| t.text.to_string()).as_deref(),
                Some("Cancelled. Nothing was posted.")
            );
        });
    }

    #[gpui_kit::test]
    fn clip_page_quotes_the_page_into_a_new_draft(cx: &mut TestAppContext) {
        let dom = quick_dom();
        dom.borrow_mut().capture = r#"{"url": "https://news.example.com/story",
            "title": "Harbour log", "canonical": "https://news.example.com/story",
            "selection": "",
            "html": "<article><h2>Low water</h2><p>The <em>tide</em> went out.</p></article>"}"#
            .into();
        let (view, cx) = setup(cx, dom);
        view.update_in(cx, |v, window, cx| {
            v.open_url_in_app(
                "https://news.example.com/story",
                crate::app::browser::OpenMode::Slide,
                window,
                cx,
            );
        });
        cx.run_until_parked();
        cx.dispatch_action(crate::app::browser::ClipPage);
        cx.run_until_parked();
        // The fake data opens a draft: the clip goes in at its caret.
        view.read_with(cx, |v, cx| {
            let text = v.editor.read(cx).value().to_string();
            assert!(
                text.starts_with(
                    "> ## Low water\n>\n> The *tide* went out.\n>\n> — [Harbour log](https://news.example.com/story)\n\n"
                ),
                "{text}"
            );
            assert!(v.toast.as_ref().unwrap().text.starts_with("Clipped into"));
            assert!(!v.browser.open, "the pane closes, like → Draft");
        });
    }
}
