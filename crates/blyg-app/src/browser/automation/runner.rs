//! The run itself: a macro's steps over a [`Page`], with the two sheets
//! that gate it. Pure of GPUI and of any one web engine (the app's `Page`
//! is the browser pane; the tests' is a scripted DOM).
//!
//! 1. The Preview sheet, before any step: the text, editable.
//! 2. The steps up to the first `submit`, each with its timeout, the whole
//!    run within [`MAX_RUN`] (the sheets don't count). The main frame
//!    leaving the site's origin, the site's signed-out selector matching,
//!    or the user's Stop ends it.
//! 3. The Post sheet: what the composer holds now (read back), red when it
//!    isn't the previewed text; Post, "I'll click Post myself", or Cancel.
//!    Only Post runs the `submit` and what follows.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use blyg_ext::protocol::{MacroSpec, SiteSpec};
use blyg_ext::recipe::{self, MAX_RUN, MAX_STEP_TIMEOUT, Step};

use super::insert::{self, Via};
use super::js::{Op, Outline, Reply};
use crate::app::browser::surface::PageState;

/// How often a waiting step looks again.
pub const POLL: Duration = Duration::from_millis(150);

/// How long after `load()` an `open` may count a page as loaded without a
/// finished navigation (`document.readyState` is `complete`): a
/// same-document or app-routed navigation, or a load event held back by a
/// request that never ends.
pub const SETTLE: Duration = Duration::from_secs(1);
/// After this, a committed page whose `readyState` is only `interactive`
/// (the DOM is there; a subresource never finished) counts too.
pub const LONG_SETTLE: Duration = Duration::from_secs(8);
/// The most bytes of `macro.log` one DOM outline takes.
pub const OUTLINE_BYTES: usize = 3 * 1024;

/// What to run: a granted macro and the text to post (the template
/// expanded, or `extension/macro.prepare`'s answer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroRun {
    /// The extension's name (its log is `<data dir>/extensions/<ext>/macro.log`).
    pub ext: String,
    pub site: SiteSpec,
    pub spec: MacroSpec,
    pub payload: String,
    /// `macro.prepare`'s line for the Preview sheet ("cut to 280 characters").
    pub note: Option<String>,
}

impl MacroRun {
    pub fn new(ext: impl Into<String>, site: SiteSpec, spec: MacroSpec, payload: String) -> Self {
        MacroRun {
            ext: ext.into(),
            site,
            spec,
            payload,
            note: None,
        }
    }

    pub fn with_note(mut self, note: Option<String>) -> Self {
        self.note = note;
        self
    }

    /// The min-interval key: one per extension and site.
    pub fn pace_key(&self) -> String {
        format!("{}/{}", self.ext, self.site.id)
    }
}

/// What the Preview sheet shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub title: String,
    pub site: String,
    pub origin: String,
    pub tested: String,
    pub note: Option<String>,
    pub payload: String,
}

/// What the Post sheet shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub site: String,
    pub payload: String,
    /// What the composer holds now.
    pub read_back: String,
    /// `read_back` isn't the payload (the sheet shows it in red).
    pub differs: bool,
}

/// The Post sheet's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostChoice {
    Post,
    /// The run ends with the text in the box; the user clicks Post.
    Myself,
    Cancel,
}

/// Why a run stopped short.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// The element didn't appear (or empty, or go away) in time.
    Timeout,
    /// `assert` found nothing, or the element vanished.
    Missing,
    /// The main frame is on another origin (the URL).
    OffOrigin(String),
    /// The site's signed-out selector matched.
    SignedOut,
    /// The text couldn't be put in.
    InsertFailed,
    /// It won't type into that element (why).
    Refused(String),
    /// The whole run took over [`MAX_RUN`].
    RunTooLong,
    /// The selector is invalid.
    BadSelector,
    /// The pane's web view went away.
    Gone,
    /// The user pressed Stop.
    Stopped,
}

impl Reason {
    pub fn code(&self) -> &'static str {
        match self {
            Reason::Timeout => "timeout",
            Reason::Missing => "missing",
            Reason::OffOrigin(_) => "off-origin",
            Reason::SignedOut => "signed-out",
            Reason::InsertFailed => "insert-failed",
            Reason::Refused(_) => "refused",
            Reason::RunTooLong => "run-too-long",
            Reason::BadSelector => "bad-selector",
            Reason::Gone => "gone",
            Reason::Stopped => "stopped",
        }
    }
}

/// A run that failed at a step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroError {
    /// 1-based.
    pub step: usize,
    /// The step's `do`.
    pub do_: &'static str,
    pub selector: Option<String>,
    pub reason: Reason,
    /// A `submit` had already run: it may have been posted.
    pub after_submit: bool,
    /// The page's outline when the step's element wasn't found
    /// ([`outline_lines`]: `  dom: …` lines for `macro.log`).
    pub dom: Vec<String>,
}

impl MacroError {
    /// The toast's first line.
    pub fn headline(&self, site: &SiteSpec, wait: Option<&Step>) -> String {
        let sel = self.selector.as_deref().unwrap_or("the page");
        let at = format!("(step {}, {})", self.step, self.do_);
        let what = match &self.reason {
            Reason::Timeout => match wait {
                Some(Step::WaitFor { empty: true, .. }) => format!("{sel} didn't empty {at}"),
                Some(Step::WaitFor { absent: true, .. }) => format!("{sel} didn't go away {at}"),
                Some(Step::Open { .. }) => format!("the page didn't load {at}"),
                _ => format!("{sel} didn't appear {at}"),
            },
            Reason::Missing => format!("{sel} isn't on the page {at}"),
            Reason::OffOrigin(url) => format!(
                "the pane left {} for {} {at}",
                site.origin,
                blyg_core::config::parse::url_origin(url).unwrap_or_else(|| url.clone())
            ),
            Reason::SignedOut => {
                return format!(
                    "Sign in to {} in this pane, then run the macro again",
                    site.title
                );
            }
            Reason::InsertFailed => format!("the text couldn't be put in {at}"),
            Reason::Refused(why) => format!("Burrow won't type into {why} {at}"),
            Reason::RunTooLong => format!("the run took over {} s {at}", MAX_RUN.as_secs()),
            Reason::BadSelector => format!("{sel} isn't a valid selector {at}"),
            Reason::Gone => format!("the browser pane closed {at}"),
            Reason::Stopped => format!("stopped {at}"),
        };
        format!("{}: {what}", site.title)
    }

    /// The toast's second line.
    pub fn detail(&self, tested: &str) -> String {
        let first = if self.after_submit {
            "Post was clicked, so it may have been posted: check the pane."
        } else {
            "Nothing was posted."
        };
        if self.reason == Reason::SignedOut {
            return first.into();
        }
        let checked = if tested.trim().eq_ignore_ascii_case("unverified") {
            "The recipe hasn't been checked by hand (tested = \"unverified\")".to_string()
        } else {
            format!("The recipe was last checked {}", tested.trim())
        };
        format!("{first} {checked}; it lives in extension.toml.")
    }
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Every step ran: `done`'s message.
    Posted {
        message: String,
        via: Via,
    },
    /// Stopped before `submit` with the text in the box (`manual`: it
    /// couldn't be put in; it's on the clipboard to paste): the user posts.
    Handed {
        manual: bool,
    },
    /// Preview or Post sheet cancelled, or Stop.
    Cancelled {
        after_submit: bool,
    },
    Failed(MacroError),
}

impl Outcome {
    /// Counts for the site's `min-interval` (something may have been posted).
    pub fn counts_as_post(&self) -> bool {
        match self {
            Outcome::Posted { .. } | Outcome::Handed { .. } => true,
            Outcome::Cancelled { after_submit } => *after_submit,
            Outcome::Failed(e) => e.after_submit,
        }
    }

    /// What `macro.log` gets: [`Self::log_text`], then a failure's DOM
    /// outline lines.
    pub fn log_lines(&self) -> Vec<String> {
        let mut out = vec![self.log_text()];
        if let Outcome::Failed(e) = self {
            out.extend(e.dom.iter().cloned());
        }
        out
    }

    /// One line for `macro.log`.
    pub fn log_text(&self) -> String {
        match self {
            Outcome::Posted { message, via } => {
                format!("posted via={} {message:?}", via.name())
            }
            Outcome::Handed { manual: true } => "handed manual (text on the clipboard)".into(),
            Outcome::Handed { manual: false } => "handed (the user clicks Post)".into(),
            Outcome::Cancelled { after_submit } => {
                format!("cancelled after-submit={after_submit}")
            }
            Outcome::Failed(e) => format!(
                "error step={} do={} reason={} selector={:?} after-submit={}",
                e.step,
                e.do_,
                e.reason.code(),
                e.selector.as_deref().unwrap_or(""),
                e.after_submit
            ),
        }
    }
}

/// What the runner needs from the browser and the UI. The app's is the
/// browser pane plus the two sheets; tests script one.
pub(crate) trait Page {
    /// Run `op` in the page; `None` when it didn't answer (loading, gone).
    async fn eval(&mut self, op: &Op) -> Option<Reply>;
    /// The main frame; `None` when the web view is gone.
    fn state(&mut self) -> Option<PageState>;
    /// Show the pane (full width, visible) on `url`.
    fn load(&mut self, url: &str);
    fn focus_page(&mut self);
    /// The Edit menu's `paste:`, sent to the page.
    fn paste(&mut self);
    fn has_edit_commands(&self) -> bool;
    /// A monotonic clock.
    fn now(&self) -> Duration;
    async fn sleep(&mut self, d: Duration);
    /// Remember the user's clipboard (once per run).
    fn save_clipboard(&mut self);
    fn set_clipboard(&mut self, text: &str);
    /// Put back what [`Self::save_clipboard`] saw (nothing if it wasn't called).
    fn restore_clipboard(&mut self);
    /// The user pressed Stop.
    fn stopped(&self) -> bool;
    /// The Preview sheet: the (edited) text, or `None` for Cancel.
    async fn preview(&mut self, p: Preview) -> Option<String>;
    /// The Post sheet.
    async fn confirm(&mut self, c: Confirm) -> PostChoice;
    /// A step is starting (the demo prints it).
    fn progress(&mut self, _index: usize, _step: &Step) {}
    /// `BLYGGER_MACRO_TRACE=1`: [`Self::trace`] gets the page's outline
    /// after every step.
    fn tracing(&self) -> bool {
        false
    }
    /// A trace entry for `macro.log`: a first line (`trace step=…`), then
    /// `  dom:` lines.
    fn trace(&mut self, _lines: &[String]) {}
}

struct Ctx {
    origin: String,
    signed_out: Option<String>,
    t0: Duration,
    sheets: Duration,
    opened: bool,
    /// The address before the last `open`, while the pane may still show
    /// it (not yet judged against the origin).
    stale: Option<String>,
    /// The text going in (an outline never shows it).
    payload: String,
}

impl Ctx {
    fn guard<P: Page>(&self, page: &mut P) -> Result<(), Reason> {
        if page.stopped() {
            return Err(Reason::Stopped);
        }
        if page.now().saturating_sub(self.t0 + self.sheets) > MAX_RUN {
            return Err(Reason::RunTooLong);
        }
        // No web view before the first `open` is fine; after it, it's gone.
        let Some(st) = page.state() else {
            return if self.opened {
                Err(Reason::Gone)
            } else {
                Ok(())
            };
        };
        if self.opened
            && !st.loading
            && !st.url.is_empty()
            // A new web view's first page, before the load commits.
            && st.url != "about:blank"
            && self.stale.as_deref() != Some(st.url.as_str())
            && !recipe::url_on(&st.url, &self.origin)
        {
            return Err(Reason::OffOrigin(st.url));
        }
        Ok(())
    }

    /// Ask `op` until `cond` holds or `timeout` passes.
    async fn wait<P: Page>(
        &self,
        page: &mut P,
        op: &Op,
        timeout: Duration,
        cond: impl Fn(&Reply) -> bool,
    ) -> Result<Reply, Reason> {
        let start = page.now();
        loop {
            self.guard(page)?;
            if let Some(r) = page.eval(op).await {
                if r.bad {
                    return Err(Reason::BadSelector);
                }
                if r.signed_out {
                    return Err(Reason::SignedOut);
                }
                if cond(&r) {
                    return Ok(r);
                }
            }
            if page.now().saturating_sub(start) >= timeout {
                return Err(Reason::Timeout);
            }
            page.sleep(POLL).await;
        }
    }

    fn find(&self, sel: &str, text: Option<&str>) -> Op {
        Op::Find {
            sel: sel.into(),
            text: text.map(String::from),
            signed_out: self.signed_out.clone(),
        }
    }

    /// The step's element, waiting for it (to click: for it to be
    /// enabled, as a Post button is once there's text).
    async fn element<P: Page>(&self, page: &mut P, step: &Step) -> Result<Reply, Reason> {
        let sel = step.selector().unwrap_or_default();
        let op = self.find(sel, step.match_text());
        let clicks = matches!(step, Step::Click { .. } | Step::Submit { .. });
        self.wait(page, &op, step.timeout(), move |r| {
            r.found && !(clicks && r.disabled)
        })
        .await
    }

    /// The page's outline as `  dom:` lines (one line saying so when the
    /// page doesn't answer).
    async fn outline<P: Page>(&self, page: &mut P) -> Vec<String> {
        match page.eval(&Op::Outline).await.and_then(|r| r.outline) {
            Some(o) => outline_lines(&o, &self.payload),
            None => vec!["  dom: (the page didn't answer)".into()],
        }
    }

    /// After `open`: loaded, on the origin, and not signed out. Loaded is
    /// a main-frame navigation finishing after the `load()` (`before`: the
    /// state then), whatever URL it ends on: a redirect back to the page
    /// the pane already showed counts. Without one, after [`SETTLE`]:
    /// `document.readyState` is `complete` and either a new document was
    /// committed or nothing is loading (same-document and app-routed
    /// navigations; a load event held back by a never-ending request);
    /// after [`LONG_SETTLE`], a committed document that's `interactive`.
    async fn loaded<P: Page>(
        &mut self,
        page: &mut P,
        before: (u64, u64),
        timeout: Duration,
    ) -> Result<(), Reason> {
        let start = page.now();
        loop {
            page.sleep(POLL).await;
            self.guard(page)?;
            let st = page.state().ok_or(Reason::Gone)?;
            let since = page.now().saturating_sub(start);
            if recipe::url_on(&st.url, &self.origin) {
                let committed = st.commits > before.0;
                let finished = st.finishes > before.1 && !st.loading;
                let reply = if finished || since >= SETTLE {
                    page.eval(&self.find("html", None)).await
                } else {
                    None
                };
                let ready = reply.as_ref().map(|r| r.ready.as_str()).unwrap_or("");
                let done = finished
                    || (since >= SETTLE && ready == "complete" && (committed || !st.loading))
                    || (since >= LONG_SETTLE && committed && ready == "interactive");
                if done {
                    self.stale = None;
                    if reply.is_some_and(|r| r.signed_out) {
                        return Err(Reason::SignedOut);
                    }
                    return Ok(());
                }
            }
            if since >= timeout {
                return Err(Reason::Timeout);
            }
        }
    }
}

/// Run `run` on `page`: Preview, steps, Post, the rest. The user's
/// clipboard is put back afterwards unless the text was left on it for a
/// paste by hand.
pub(crate) async fn run<P: Page>(page: &mut P, run: &MacroRun) -> Outcome {
    let out = steps(page, run).await;
    if out != (Outcome::Handed { manual: true }) {
        page.restore_clipboard();
    }
    out
}

async fn steps<P: Page>(page: &mut P, run: &MacroRun) -> Outcome {
    let lf = run.payload.replace("\r\n", "\n").replace('\r', "\n");
    let edited = page
        .preview(Preview {
            title: run.spec.title.clone(),
            site: run.site.title.clone(),
            origin: run.site.origin.clone(),
            tested: run.spec.tested.clone(),
            note: run.note.clone(),
            payload: lf.trim().to_string(),
        })
        .await;
    let Some(edited) = edited else {
        return Outcome::Cancelled {
            after_submit: false,
        };
    };
    let payload = edited.replace("\r\n", "\n").trim().to_string();
    if insert::normalize(&payload).is_empty() {
        return Outcome::Cancelled {
            after_submit: false,
        };
    }
    let mut ctx = Ctx {
        origin: run.site.origin.clone(),
        signed_out: run.site.signed_out.clone(),
        t0: page.now(),
        sheets: Duration::ZERO,
        opened: false,
        stale: None,
        payload: payload.clone(),
    };
    let tracing = page.tracing();
    let first_submit = recipe::first_submit(&run.spec.steps);
    let insert_sel = recipe::insert_index(&run.spec.steps)
        .and_then(|i| run.spec.steps[i].selector())
        .unwrap_or_default()
        .to_string();
    let mut submitted = false;
    let mut via = Via::Manual;
    for (i, step) in run.spec.steps.iter().enumerate() {
        page.progress(i, step);
        let fail = |reason: Reason, submitted: bool, dom: Vec<String>| match reason {
            Reason::Stopped => Outcome::Cancelled {
                after_submit: submitted,
            },
            reason => Outcome::Failed(MacroError {
                step: i + 1,
                do_: step.name(),
                selector: step.selector().map(String::from),
                reason,
                after_submit: submitted,
                dom,
            }),
        };
        if let Err(r) = ctx.guard(page) {
            return fail(r, submitted, vec![]);
        }
        let result: Result<Flow, Reason> = async {
            match step {
                Step::Open { url } => {
                    if !recipe::url_on(url, &ctx.origin) {
                        return Err(Reason::OffOrigin(url.clone()));
                    }
                    let st = page.state();
                    let before = st.as_ref().map_or((0, 0), |s| (s.commits, s.finishes));
                    ctx.stale = st.map(|s| s.url).filter(|u| u != url);
                    page.load(url);
                    ctx.opened = true;
                    // Heavy sites take more than the default 10 s.
                    ctx.loaded(page, before, MAX_STEP_TIMEOUT)
                        .await
                        .map(|_| Flow::Next)
                }
                Step::WaitFor {
                    selector,
                    text,
                    empty,
                    absent,
                    ..
                } => {
                    let op = ctx.find(selector, text.as_deref());
                    let (empty, absent) = (*empty, *absent);
                    ctx.wait(page, &op, step.timeout(), move |r| {
                        if absent {
                            !r.found || !r.visible
                        } else if empty {
                            r.found && r.text.trim().is_empty()
                        } else {
                            r.found
                        }
                    })
                    .await
                    .map(|_| Flow::Next)
                }
                Step::Assert { selector, text } => {
                    let op = ctx.find(selector, text.as_deref());
                    match page.eval(&op).await {
                        Some(r) if r.bad => Err(Reason::BadSelector),
                        Some(r) if r.signed_out => Err(Reason::SignedOut),
                        Some(r) if r.found => Ok(Flow::Next),
                        _ => Err(Reason::Missing),
                    }
                }
                Step::Focus { selector } => {
                    ctx.element(page, step).await?;
                    let r = page
                        .eval(&Op::Focus {
                            sel: selector.clone(),
                            text: None,
                        })
                        .await
                        .ok_or(Reason::Missing)?;
                    if let Some(why) = r.refused {
                        return Err(Reason::Refused(why));
                    }
                    if !r.found {
                        return Err(Reason::Missing);
                    }
                    page.focus_page();
                    Ok(Flow::Next)
                }
                Step::Click { selector, text } => {
                    ctx.element(page, step).await?;
                    click(page, selector, text.as_deref())
                        .await
                        .map(|_| Flow::Next)
                }
                Step::Insert { selector } => {
                    ctx.element(page, step).await?;
                    page.save_clipboard();
                    via = insert::insert(page, selector, &payload).await?;
                    Ok(if via == Via::Manual {
                        Flow::Handed { manual: true }
                    } else {
                        Flow::Next
                    })
                }
                Step::Submit { selector, text } => {
                    if Some(i) == first_submit && !submitted {
                        let read_back = page
                            .eval(&Op::Read {
                                sel: insert_sel.clone(),
                            })
                            .await
                            .map(|r| r.text)
                            .unwrap_or_default();
                        let differs = !insert::same_text(&read_back, &payload);
                        let t = page.now();
                        let choice = page
                            .confirm(Confirm {
                                site: run.site.title.clone(),
                                payload: payload.clone(),
                                read_back,
                                differs,
                            })
                            .await;
                        ctx.sheets += page.now().saturating_sub(t);
                        match choice {
                            PostChoice::Post => {}
                            PostChoice::Myself => return Ok(Flow::Handed { manual: false }),
                            PostChoice::Cancel => return Err(Reason::Stopped),
                        }
                        ctx.guard(page)?;
                    }
                    ctx.element(page, step).await?;
                    click(page, selector, text.as_deref()).await?;
                    submitted = true;
                    Ok(Flow::Next)
                }
                Step::Done { text } => Ok(Flow::Done(text.clone())),
            }
        }
        .await;
        if tracing && !matches!(step, Step::Done { .. }) {
            let mut lines = vec![format!(
                "trace step={} do={} {}",
                i + 1,
                step.name(),
                if result.is_ok() { "ok" } else { "failed" }
            )];
            if matches!(result, Err(Reason::OffOrigin(_) | Reason::Gone)) {
                // Not the site's page: nothing of it in the log.
                lines.push("  dom: (not the site's page)".into());
            } else {
                lines.extend(ctx.outline(page).await);
            }
            page.trace(&lines);
        }
        match result {
            Ok(Flow::Next) => {}
            Ok(Flow::Handed { manual }) => return Outcome::Handed { manual },
            Ok(Flow::Done(message)) => return Outcome::Posted { message, via },
            Err(r) => {
                // What the page does have, so the selector can be fixed.
                let dom = if matches!(r, Reason::Timeout | Reason::Missing) {
                    ctx.outline(page).await
                } else {
                    vec![]
                };
                return fail(r, submitted, dom);
            }
        }
    }
    Outcome::Posted {
        message: format!("Posted to {}", run.site.title),
        via,
    }
}

/// What a step leads to.
enum Flow {
    Next,
    Handed { manual: bool },
    Done(String),
}

async fn click<P: Page>(page: &mut P, sel: &str, text: Option<&str>) -> Result<(), Reason> {
    let r = page
        .eval(&Op::Click {
            sel: sel.into(),
            text: text.map(String::from),
        })
        .await
        .ok_or(Reason::Missing)?;
    if r.found {
        Ok(())
    } else {
        Err(Reason::Missing)
    }
}

// ------------------------------------------------------------ pacing

/// The sites' `min-interval`: when each was last posted to (this session).
#[derive(Debug, Default)]
pub struct Pacing {
    last: HashMap<String, Instant>,
}

impl Pacing {
    /// How long until `key` may run again (`None`: now).
    pub fn wait_left(&self, key: &str, min: Duration, now: Instant) -> Option<Duration> {
        let last = *self.last.get(key)?;
        let since = now.saturating_duration_since(last);
        (since < min).then(|| min - since)
    }

    pub fn record(&mut self, key: &str, now: Instant) {
        self.last.insert(key.to_string(), now);
    }
}

// ------------------------------------------------------------ the log

/// The most lines `macro.log` keeps. A failure's DOM outline is up to 27
/// lines (3 KB), and a traced run has one per step, so this keeps dozens
/// of runs with outlines (hundreds without) in about 200 KB at most.
pub const LOG_LINES: usize = 2000;

/// `existing` with `entry` (one line or several) appended, keeping the
/// last `cap` lines, and never starting on an indented (`  dom:`) line
/// whose entry's first line was cut.
pub fn ring_append(existing: &str, entry: &str, cap: usize) -> String {
    let mut lines: Vec<&str> = existing.lines().filter(|l| !l.is_empty()).collect();
    lines.extend(entry.lines().map(str::trim_end).filter(|l| !l.is_empty()));
    let mut skip = lines.len().saturating_sub(cap);
    while skip < lines.len() && lines[skip].starts_with("  ") {
        skip += 1;
    }
    let mut out = lines[skip..].join("\n");
    out.push('\n');
    out
}

/// A log entry: `<time> <macro> <outcome>`, then a failure's `  dom:`
/// lines. Never the text itself.
pub fn log_line(time: &str, run: &MacroRun, out: &Outcome) -> String {
    let mut lines = out.log_lines().into_iter();
    let mut s = format!(
        "{time} {} {}",
        run.spec.id,
        lines.next().unwrap_or_default()
    );
    for l in lines {
        s.push('\n');
        s.push_str(&l);
    }
    s
}

/// A trace entry: `<time> <macro> trace step=…`, then its `  dom:` lines.
pub fn trace_entry(time: &str, macro_id: &str, lines: &[String]) -> String {
    let mut s = format!("{time} {macro_id}");
    for (i, l) in lines.iter().enumerate() {
        s.push(if i == 0 { ' ' } else { '\n' });
        s.push_str(l);
    }
    s
}

// ------------------------------------------------------------ the outline

/// `s`, unless it's (part of) the payload: an outline never shows the text.
fn scrub(s: &str, payload: &str) -> String {
    let bare = s.trim_end_matches('\u{2026}').trim();
    let p = payload.split_whitespace().collect::<Vec<_>>().join(" ");
    let head: String = p.chars().take(12).collect();
    let in_payload = bare.chars().count() >= 6 && p.contains(bare);
    let holds_payload = head.chars().count() >= 4 && bare.contains(head.as_str());
    if in_payload || holds_payload {
        "(the text)".into()
    } else {
        s.to_string()
    }
}

/// `macro.log`'s lines for an outline: the page (path without the query,
/// the title's length, how many candidates), then one line per candidate:
/// `tag#id.class…` and its role, contenteditable, aria-label,
/// placeholder, data-testid, name, type, visible, disabled, and a
/// button's label. Cut at [`OUTLINE_BYTES`]. Nothing that is (part of)
/// the payload.
pub fn outline_lines(o: &Outline, payload: &str) -> Vec<String> {
    let sc = |s: &str| scrub(s, payload);
    let path = o.path.split(['?', '#']).next().unwrap_or("");
    let listed = o.els.len().min(super::js::OUTLINE_MAX);
    let mut out = vec![format!(
        "  dom: page path={path} title-len={} candidates={} listed={listed}",
        o.title_len, o.total
    )];
    let mut bytes = out[0].len() + 1;
    for e in o.els.iter().take(super::js::OUTLINE_MAX) {
        let mut l = format!("  dom: {}", e.tag);
        if !e.id.is_empty() {
            l += &format!("#{}", sc(&e.id));
        }
        for c in e.cls.iter().take(4) {
            l += &format!(".{}", sc(c));
        }
        let attrs = [
            ("role", &e.role),
            ("ce", &e.ce),
            ("aria", &e.aria),
            ("ph", &e.ph),
            ("testid", &e.testid),
            ("name", &e.name),
            ("type", &e.kind),
        ];
        for (k, v) in attrs {
            if !v.is_empty() {
                l += &format!(" {k}={:?}", sc(v));
            }
        }
        l += if e.vis { " visible=yes" } else { " visible=no" };
        if e.disabled {
            l += " disabled";
        }
        if !e.label.is_empty() {
            let label: String = e.label.chars().take(31).collect();
            l += &format!(" label={:?}", sc(&label));
        }
        if bytes + l.len() + 1 > OUTLINE_BYTES {
            out.push("  dom: (cut at 3 KB)".into());
            break;
        }
        bytes += l.len() + 1;
        out.push(l);
    }
    out
}
