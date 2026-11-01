//! --- background --- A revised post leads with its latest changelog entry
//! (blygger-studio 0.49.0). On a stream card for someone's blyg post past
//! its first version, the first line says what changed: `V3: <note>`, read
//! from the origin's changelog (`Backend::remote_versions`) once the card is
//! drawn, or `V3: No changelog entry available` when that version has no
//! note (or the origin can't be reached). Not for RSS items, your own posts,
//! a first version or a withdrawn post.
//!
//! Nothing new is stored: the notes live in memory, by (post, version), so
//! each is asked once a session. Only cards the virtualized stream draws ask,
//! a few at a time (`MAX_IN_FLIGHT`); a short queue (`MAX_QUEUED`) keeps the
//! newest asks, so scrolling past many posts doesn't line up fetches for
//! cards no longer on screen. A failed ask is tried again after `RETRY`.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use blyg_core::{ReadingItem, RemoteVersion};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::vm::{self, Key};
use crate::app::MainView;

/// Fetches running at once.
pub const MAX_IN_FLIGHT: usize = 3;
/// Asks waiting for a slot; older ones are dropped (and asked again if
/// their card is drawn again).
pub const MAX_QUEUED: usize = 12;
/// A failed ask is tried again after this long.
pub const RETRY: Duration = Duration::from_secs(60);

/// What the line says when the version has no note.
pub const NO_ENTRY: &str = "No changelog entry available";

/// One post version's lead.
#[derive(Debug, Clone, PartialEq)]
pub enum Lead {
    /// Waiting for a slot.
    Queued,
    /// Being asked.
    Loading,
    /// The note for that version (`None`: it has none).
    Ready(Option<String>),
    /// The origin couldn't be asked; the line says there's no entry.
    Failed(Instant),
}

/// The leads of this session (inside `reading::State`).
#[derive(Default)]
pub struct Leads {
    by: RefCell<HashMap<(Key, u32), Lead>>,
    /// Newest ask last.
    queue: RefCell<VecDeque<(Key, u32)>>,
    in_flight: Cell<usize>,
    scheduled: Cell<bool>,
}

impl Leads {
    pub fn get(&self, key: &Key, version: u32) -> Option<Lead> {
        self.by.borrow().get(&(key.clone(), version)).cloned()
    }

    /// Queue an ask for (`key`, `version`) unless it's known, asked or
    /// waiting (a failure is asked again after `RETRY`). True when queued.
    pub fn want(&self, key: &Key, version: u32, now: Instant) -> bool {
        let k = (key.clone(), version);
        let mut by = self.by.borrow_mut();
        match by.get(&k) {
            Some(Lead::Failed(at)) if now.duration_since(*at) >= RETRY => {}
            Some(_) => return false,
            None => {}
        }
        by.insert(k.clone(), Lead::Queued);
        let mut q = self.queue.borrow_mut();
        q.push_back(k);
        while q.len() > MAX_QUEUED {
            if let Some(old) = q.pop_front() {
                by.remove(&old);
            }
        }
        true
    }

    /// The asks to start now (newest first), within `MAX_IN_FLIGHT`.
    pub fn take(&self) -> Vec<(Key, u32)> {
        let mut out = Vec::new();
        let mut q = self.queue.borrow_mut();
        let mut by = self.by.borrow_mut();
        while self.in_flight.get() < MAX_IN_FLIGHT {
            let Some(k) = q.pop_back() else { break };
            by.insert(k.clone(), Lead::Loading);
            self.in_flight.set(self.in_flight.get() + 1);
            out.push(k);
        }
        out
    }

    /// An ask finished.
    pub fn done(&self, key: (Key, u32), lead: Lead) {
        self.in_flight.set(self.in_flight.get().saturating_sub(1));
        self.by.borrow_mut().insert(key, lead);
    }

    #[cfg(test)]
    pub fn in_flight(&self) -> usize {
        self.in_flight.get()
    }
}

/// Does `r` get a lead? Someone's blyg post (`blyg`: not an RSS item) past
/// its first version, not withdrawn.
pub fn eligible(r: &ReadingItem, blyg: bool) -> bool {
    blyg && !super::own::is_own_row(r) && r.state != "tombstone" && r.version > 1
}

/// The note the changelog gives `version`, trimmed; `None` when it has
/// none (or only whitespace).
pub fn note_for(log: &[RemoteVersion], version: u32) -> Option<String> {
    log.iter()
        .find(|v| v.version == version)
        .and_then(|v| v.note.as_deref())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
}

/// The line's two halves: `V3:` and what follows it (`None`: still asking).
pub fn lead_parts(version: u32, lead: Option<&Lead>) -> (String, String) {
    let rest = match lead {
        Some(Lead::Ready(Some(n))) => n.clone(),
        Some(Lead::Ready(None)) | Some(Lead::Failed(_)) => NO_ENTRY.into(),
        Some(Lead::Queued) | Some(Lead::Loading) | None => "…".into(),
    };
    (format!("V{version}:"), rest)
}

/// The whole line, as text: `V3: Tightened the claim.`
#[cfg(test)]
pub fn lead_text(version: u32, lead: Option<&Lead>) -> String {
    let (v, rest) = lead_parts(version, lead);
    format!("{v} {rest}")
}

impl MainView {
    /// Whether `r` comes from a blyg rather than an RSS feed (a row whose
    /// subscription isn't listed counts as a blyg, as the actions do).
    fn lead_blyg(&self, r: &ReadingItem) -> bool {
        self.reading
            .subs
            .iter()
            .find(|s| s.id == r.subscription_id)
            .is_none_or(|s| s.kind == blyg_core::SubscriptionKind::Blyg)
    }

    /// The stream card's first line for `r`, when it gets one; drawing it
    /// asks the origin (once) for the note.
    pub(super) fn render_change_lead(
        &self,
        r: &ReadingItem,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !eligible(r, self.lead_blyg(r)) {
            return None;
        }
        let key = vm::key(r);
        let leads = &self.reading.leads;
        if leads.want(&key, r.version, Instant::now()) && !leads.scheduled.replace(true) {
            let this = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = this.update(cx, |v, cx| v.pump_leads(cx));
            });
        }
        let lead = leads.get(&key, r.version);
        let none = matches!(lead, Some(Lead::Ready(None)) | Some(Lead::Failed(_)));
        let (v, rest) = lead_parts(r.version, lead.as_ref());
        let p = self.palette.on_page();
        let ix = key.1.clone();
        Some(
            div()
                .id(SharedString::from(format!("change-lead-{ix}")))
                .debug_selector(move || format!("change-lead-{ix}"))
                .flex()
                .flex_wrap()
                .gap(px(4.))
                .font_family(self.chrome())
                .text_size(px(12.5))
                .text_color(p.ink)
                .child(div().font_weight(FontWeight::SEMIBOLD).child(v))
                .child(
                    div()
                        .when(none, |d| d.italic().text_color(p.muted))
                        .child(rest),
                )
                .into_any_element(),
        )
    }

    /// Start the queued asks that fit, off the main thread; each answer is
    /// kept and the stream redrawn.
    pub(crate) fn pump_leads(&mut self, cx: &mut Context<Self>) {
        self.reading.leads.scheduled.set(false);
        for (key, version) in self.reading.leads.take() {
            let backend = self.backend.clone();
            let (sub, rid) = key.clone();
            let task = cx.background_spawn(async move {
                crate::guarded(|| backend.remote_versions(&sub, &rid))
            });
            cx.spawn(async move |this, cx| {
                let log = task.await;
                let _ = this.update(cx, |v, cx| {
                    let lead = match log {
                        Ok(log) => Lead::Ready(note_for(&log, version)),
                        Err(_) => Lead::Failed(Instant::now()),
                    };
                    v.reading.leads.done((key.clone(), version), lead);
                    // The line may wrap now: measure the card again.
                    if let Some(ix) = v.reading.shown_pos(&key) {
                        v.reading.stream.list.remeasure_items(ix..ix + 1);
                    }
                    v.pump_leads(cx);
                    cx.notify();
                });
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        Lead, Leads, MAX_IN_FLIGHT, MAX_QUEUED, NO_ENTRY, RETRY, eligible, lead_text, note_for,
    };
    use crate::fake::reading_seed;
    use blyg_core::RemoteVersion;

    fn rv(version: u32, note: Option<&str>) -> RemoteVersion {
        RemoteVersion {
            version,
            at: "2026-10-01T10:00:00Z".into(),
            note: note.map(str::to_string),
            pinned: false,
            current: false,
            media: vec![],
            lineage: Default::default(),
        }
    }

    #[test]
    fn the_line_names_the_version_and_its_note() {
        let log = [
            rv(1, None),
            rv(2, Some("Older edit.")),
            rv(3, Some("  Tightened the claim. ")),
            rv(4, Some("   ")),
        ];
        assert_eq!(note_for(&log, 3).as_deref(), Some("Tightened the claim."));
        assert_eq!(note_for(&log, 4), None, "whitespace is no note");
        assert_eq!(note_for(&log, 9), None, "not in the changelog");
        let ready = |v| Lead::Ready(note_for(&log, v));
        assert_eq!(lead_text(3, Some(&ready(3))), "V3: Tightened the claim.");
        assert_eq!(lead_text(2, Some(&ready(4))), format!("V2: {NO_ENTRY}"));
        assert_eq!(
            lead_text(4, Some(&Lead::Failed(Instant::now()))),
            "V4: No changelog entry available",
            "an unreachable origin says the same"
        );
        assert_eq!(lead_text(5, None), "V5: …");
        assert_eq!(lead_text(5, Some(&Lead::Loading)), "V5: …");
    }

    #[test]
    fn only_someones_revised_live_blyg_post_gets_one() {
        let sd = reading_seed::seed(chrono::Utc::now());
        let mut r = sd.reading[0].clone();
        r.state = "current".into();
        r.version = 3;
        assert!(eligible(&r, true));
        assert!(!eligible(&r, false), "RSS");
        let mut first = r.clone();
        first.version = 1;
        assert!(!eligible(&first, true));
        let mut tomb = r.clone();
        tomb.state = "tombstone".into();
        assert!(!eligible(&tomb, true));
        let mut own = r.clone();
        own.subscription_id = super::super::own::OWN.into();
        assert!(!eligible(&own, true));
    }

    #[test]
    fn asks_are_once_a_session_a_few_at_a_time_newest_first() {
        let leads = Leads::default();
        let now = Instant::now();
        let key = |i: usize| ("sub".to_string(), format!("id{i}"));
        for i in 0..5 {
            assert!(leads.want(&key(i), 2, now));
        }
        assert!(!leads.want(&key(0), 2, now), "already waiting");
        assert!(leads.want(&key(0), 3, now), "a new version asks again");
        let first = leads.take();
        assert_eq!(first.len(), MAX_IN_FLIGHT);
        assert_eq!(first[0], (key(0), 3), "newest first");
        assert!(leads.take().is_empty(), "no slot free");
        leads.done(first[0].clone(), Lead::Ready(Some("n".into())));
        assert!(!leads.want(&key(0), 3, now), "known");
        assert_eq!(leads.take().len(), 1);
        // A failure is asked again, after a while.
        let (k, v) = first[1].clone();
        leads.done((k.clone(), v), Lead::Failed(now));
        assert!(!leads.want(&k, v, now));
        assert!(leads.want(&k, v, now + RETRY + Duration::from_secs(1)));
    }

    #[test]
    fn a_long_scroll_keeps_only_the_newest_asks() {
        let leads = Leads::default();
        let now = Instant::now();
        let key = |i: usize| ("sub".to_string(), format!("id{i}"));
        for i in 0..MAX_QUEUED + 5 {
            leads.want(&key(i), 2, now);
        }
        assert_eq!(leads.get(&key(0), 2), None, "dropped, asked again if drawn");
        assert_eq!(leads.get(&key(MAX_QUEUED + 4), 2), Some(Lead::Queued));
        assert!(leads.want(&key(0), 2, now));
    }
}
