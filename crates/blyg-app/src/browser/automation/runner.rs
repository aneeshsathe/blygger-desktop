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
use blyg_ext::recipe::{self, MAX_RUN, Step};

use super::insert::{self, Via};
use super::js::{Op, Reply};
use crate::app::browser::surface::PageState;

/// How often a waiting step looks again.
pub const POLL: Duration = Duration::from_millis(150);

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

    /// The step's element, waiting for it.
    async fn element<P: Page>(&self, page: &mut P, step: &Step) -> Result<Reply, Reason> {
        let sel = step.selector().unwrap_or_default();
        let op = self.find(sel, step.match_text());
        self.wait(page, &op, step.timeout(), |r| r.found).await
    }

    /// After `open`: loaded, on the origin, and not signed out.
    async fn loaded<P: Page>(&mut self, page: &mut P, timeout: Duration) -> Result<(), Reason> {
        let start = page.now();
        loop {
            page.sleep(POLL).await;
            self.guard(page)?;
            let st = page.state().ok_or(Reason::Gone)?;
            if !st.loading
                && self.stale.as_deref() != Some(st.url.as_str())
                && recipe::url_on(&st.url, &self.origin)
            {
                self.stale = None;
                if let Some(r) = page.eval(&self.find("html", None)).await
                    && r.signed_out
                {
                    return Err(Reason::SignedOut);
                }
                return Ok(());
            }
            if page.now().saturating_sub(start) >= timeout {
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
    };
    let first_submit = recipe::first_submit(&run.spec.steps);
    let insert_sel = recipe::insert_index(&run.spec.steps)
        .and_then(|i| run.spec.steps[i].selector())
        .unwrap_or_default()
        .to_string();
    let mut submitted = false;
    let mut via = Via::Manual;
    for (i, step) in run.spec.steps.iter().enumerate() {
        page.progress(i, step);
        let fail = |reason: Reason, submitted: bool| match reason {
            Reason::Stopped => Outcome::Cancelled {
                after_submit: submitted,
            },
            reason => Outcome::Failed(MacroError {
                step: i + 1,
                do_: step.name(),
                selector: step.selector().map(String::from),
                reason,
                after_submit: submitted,
            }),
        };
        if let Err(r) = ctx.guard(page) {
            return fail(r, submitted);
        }
        let result: Result<Flow, Reason> = async {
            match step {
                Step::Open { url } => {
                    if !recipe::url_on(url, &ctx.origin) {
                        return Err(Reason::OffOrigin(url.clone()));
                    }
                    ctx.stale = page.state().map(|s| s.url).filter(|u| u != url);
                    page.load(url);
                    ctx.opened = true;
                    ctx.loaded(page, step.timeout()).await.map(|_| Flow::Next)
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
                            !r.found
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
        match result {
            Ok(Flow::Next) => {}
            Ok(Flow::Handed { manual }) => return Outcome::Handed { manual },
            Ok(Flow::Done(message)) => return Outcome::Posted { message, via },
            Err(r) => return fail(r, submitted),
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

/// The most lines `macro.log` keeps.
pub const LOG_LINES: usize = 200;

/// `existing` with `line` appended, keeping the last `cap` lines.
pub fn ring_append(existing: &str, line: &str, cap: usize) -> String {
    let mut lines: Vec<&str> = existing.lines().filter(|l| !l.is_empty()).collect();
    lines.push(line.trim_end());
    let skip = lines.len().saturating_sub(cap);
    let mut out = lines[skip..].join("\n");
    out.push('\n');
    out
}

/// A log line: `<time> <macro> <outcome>`. Never the text itself.
pub fn log_line(time: &str, run: &MacroRun, out: &Outcome) -> String {
    format!("{time} {} {}", run.spec.id, out.log_text())
}
