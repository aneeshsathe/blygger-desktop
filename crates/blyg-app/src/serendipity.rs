//! --- serendipity --- The empty editor hands you an old thought (Fable's
//! proposal 9). Opt-in: `serendipity = off | own | on`, off by default;
//! Settings (⌘,) has the row.
//!
//! When it's on, ⌘N's empty editor shows one post under the placeholder: one
//! of your own published posts, weighted toward orphans (nothing of yours or
//! in your reading links to it) and then toward "this week in another year".
//! `on` also offers posts you've read from your subscriptions (muted, with
//! where they're from); `own` never does. A withdrawn post never shows, and
//! a post shown isn't shown again for a month (`state.json`).
//!
//! While nothing is typed: q quotes it (`![[id | title]]` on its own line,
//! and the note becomes a thread), l links it (`[[id | title]]`), o opens
//! it, r rolls another. Any other key just starts typing, and the card goes.
//! Nothing here is counted or shown as a number.
//!
//! A child module of `app`; `app.rs` and `new_post.rs` keep small hooks
//! marked `// --- serendipity ---`.

use std::collections::{BTreeMap, HashSet};

use blyg_core::{ReadingItem, Status};
use chrono::Datelike;

use super::*;

/// A month: a post shown isn't shown again within it.
pub const QUIET_SECS: u64 = 30 * 24 * 3600;

/// `serendipity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    /// Your own posts only.
    Own,
    /// Yours, and posts you've read from your subscriptions.
    On,
}

impl Mode {
    pub fn value(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Own => "own",
            Mode::On => "on",
        }
    }
}

pub fn mode(cx: &App) -> Mode {
    match cx
        .try_global::<crate::settings::AppConfig>()
        .and_then(|c| c.store.config().get("serendipity"))
    {
        Some("on") => Mode::On,
        Some("own") => Mode::Own,
        _ => Mode::Off,
    }
}

/// A post the card can show.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The `state.json` key (`own:<id>`, `held:<origin> <id>`).
    pub key: String,
    /// The id the brackets name.
    pub id: String,
    pub title: String,
    pub excerpt: String,
    pub kind: Kind,
    pub version: u32,
    pub created: String,
    /// Nothing links to it.
    pub orphan: bool,
    /// Written this week, in another year.
    pub anniversary: bool,
    /// Yours: its local id.
    pub own: Option<LocalId>,
    /// Someone else's: (origin, where it's from).
    pub held: Option<(String, String)>,
}

impl Candidate {
    /// How much more likely it is than a plain one.
    pub fn weight(&self) -> u64 {
        let mut w = 1;
        if self.orphan {
            w += 4;
        }
        if self.anniversary {
            w += 3;
        }
        if self.held.is_some() {
            w = 1; // read posts are the garnish
        }
        w
    }
}

/// The card on screen.
#[derive(Debug, Clone)]
pub(crate) struct Card {
    pub c: Candidate,
    pub seq: usize,
}

#[derive(Default)]
pub(crate) struct State {
    pub card: Option<Card>,
    /// Shown this sitting (r never repeats one).
    shown: Vec<String>,
    /// `state.json`'s record, read once.
    seen: Option<BTreeMap<String, u64>>,
    seq: usize,
    /// Test hook: a fixed roll.
    #[cfg(test)]
    pub seed: Option<u64>,
}

/// The first readable lines, flattened (a card, not a page).
pub fn excerpt(md: &str) -> String {
    let mut out = String::new();
    for line in md.lines() {
        let l = line
            .trim()
            .trim_start_matches(['#', '>', '-', '*'])
            .trim()
            .replace("**", "")
            .replace('*', "");
        if l.is_empty() || l.starts_with("![[") {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&l);
        if out.chars().count() > 220 {
            break;
        }
    }
    if out.chars().count() > 220 {
        let cut = blyg_core::text::prefix_graphemes(&out, 217);
        format!("{}…", cut.trim_end())
    } else {
        out
    }
}

/// Created in the same ISO week as `now`, in an earlier year.
pub fn anniversary(created: &str, now: chrono::DateTime<chrono::Utc>) -> bool {
    chrono::DateTime::parse_from_rfc3339(created).is_ok_and(|t| {
        let (w, n) = (t.iso_week(), now.iso_week());
        w.week() == n.week() && w.year() < n.year()
    })
}

/// Every id something of yours or in your reading points at (links,
/// quotes, stubs, forks), lowercased.
fn linked_ids(items: &[Item], reading: &[ReadingItem]) -> HashSet<String> {
    let mut out = HashSet::new();
    for i in items {
        for r in blyg_core::grammar::link_refs(&i.content_md) {
            out.insert(r.id.to_lowercase());
        }
        for id in blyg_core::profile::transclusion_ids(&i.content_md) {
            out.insert(id.to_lowercase());
        }
        for r in [&i.stub_of, &i.forked_from].into_iter().flatten() {
            out.insert(r.id.to_lowercase());
        }
    }
    for r in reading {
        for p in r.references() {
            out.insert(p.id);
        }
        for l in blyg_core::grammar::link_refs(&r.content_md) {
            out.insert(l.id.to_lowercase());
        }
    }
    out
}

/// The pool for `mode`: your published posts (never withdrawn), and with
/// `On` the current posts you've read from your subscriptions.
pub fn candidates(
    items: &[Item],
    reading: &[ReadingItem],
    mode: Mode,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<Candidate> {
    if mode == Mode::Off {
        return Vec::new();
    }
    let linked = linked_ids(items, reading);
    let mut out = Vec::new();
    for i in items {
        let Some(sid) = i.server_id.as_ref().filter(|_| i.status == Status::Public) else {
            continue;
        };
        out.push(Candidate {
            key: format!("own:{}", sid.0),
            id: sid.0.clone(),
            title: i.title(),
            excerpt: excerpt(&i.content_md),
            kind: i.kind,
            version: i.version,
            created: i.created.clone(),
            orphan: !linked.contains(&sid.0.to_lowercase()),
            anniversary: anniversary(&i.created, now),
            own: Some(i.local_id.clone()),
            held: None,
        });
    }
    if mode == Mode::On {
        for r in reading {
            // Held from a subscription (yours have no subscription), read,
            // and not withdrawn.
            if r.subscription_id.is_empty() || r.read_version.is_none() || r.state != "current" {
                continue;
            }
            let created = r.created.clone().unwrap_or_else(|| r.observed_at.clone());
            out.push(Candidate {
                key: format!("held:{} {}", r.origin, r.remote_id),
                id: r.remote_id.clone(),
                title: blyg_core::model::plain_title(&r.content_md)
                    .unwrap_or_else(|| "Untitled".into()),
                excerpt: excerpt(&r.content_md),
                kind: r.kind,
                version: r.version,
                anniversary: anniversary(&created, now),
                created,
                orphan: false,
                own: None,
                held: Some((r.origin.clone(), r.subscription_title.clone())),
            });
        }
    }
    out
}

/// Pick one: weighted by [`Candidate::weight`], leaving out what was shown
/// in the last month (`seen`, Unix seconds) or this sitting (`skip`).
/// `roll` is any number (the clock, or a test's).
pub fn pick(
    cands: &[Candidate],
    seen: &BTreeMap<String, u64>,
    skip: &[String],
    now_secs: u64,
    roll: u64,
) -> Option<usize> {
    let ok: Vec<(usize, u64)> = cands
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            !skip.contains(&c.key)
                && seen
                    .get(&c.key)
                    .is_none_or(|t| now_secs.saturating_sub(*t) >= QUIET_SECS)
        })
        .map(|(i, c)| (i, c.weight()))
        .collect();
    let total: u64 = ok.iter().map(|(_, w)| w).sum();
    if total == 0 {
        return None;
    }
    // A splitmix step, so nearby rolls land far apart.
    let mut z = roll.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    let mut at = (z ^ (z >> 31)) % total;
    for (i, w) in ok {
        if at < w {
            return Some(i);
        }
        at -= w;
    }
    None
}

/// The card's line under the title: "a year ago this week", or where a
/// read post is from. Words, never a number of anything.
pub fn why(c: &Candidate, now: chrono::DateTime<chrono::Utc>) -> String {
    let kind = vm::kind_label(c.kind);
    let when = if c.anniversary {
        let years = chrono::DateTime::parse_from_rfc3339(&c.created)
            .map(|t| now.year() - t.year())
            .unwrap_or(1);
        if years == 1 {
            "a year ago this week".to_string()
        } else {
            format!("{years} years ago this week")
        }
    } else {
        vm::relative_time(&c.created, now)
    };
    match &c.held {
        Some((_, from)) => format!("{} · {kind} · from {from} · {when}", c.title),
        None => format!("{} · {kind} · v{} · {when}", c.title, c.version),
    }
}

/// The card's heading.
pub fn heading(c: &Candidate) -> &'static str {
    if c.held.is_some() {
        "a thought you read"
    } else if c.orphan {
        "a thought from the burrow · nothing links to it yet"
    } else if c.anniversary {
        "a thought from the burrow · this week, another year"
    } else {
        "a thought from the burrow"
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl MainView {
    /// Hook (⌘N): offer a post, when `serendipity` is on.
    pub(super) fn serendipity_offer(&mut self, cx: &mut Context<Self>) {
        self.serendipity.card = None;
        // The tour's step shows it on the samples, and remembers nothing.
        let tour = self.tutorial_on_step("serendipity");
        let m = if tour { Mode::Own } else { mode(cx) };
        if m == Mode::Off || !self.serendipity_empty_editor(cx) {
            return;
        }
        let reading = self.backend.reading();
        let cands = candidates(&self.backend.items(), &reading, m, self.now);
        let dir = crate::app::command_palette::data_dir(cx).filter(|_| !tour);
        let seen = self
            .serendipity
            .seen
            .get_or_insert_with(|| {
                dir.as_deref()
                    .map(blyg_core::state::AppState::load)
                    .map(|s| s.serendipity_seen)
                    .unwrap_or_default()
            })
            .clone();
        let now = now_secs();
        #[cfg(test)]
        let roll = self.serendipity.seed.unwrap_or(now);
        #[cfg(not(test))]
        let roll = now
            ^ (self.serendipity.seq as u64).wrapping_mul(0x2545_F491_4F6C_DD1D)
            ^ std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as u64)
                .unwrap_or(0);
        let Some(i) = pick(&cands, &seen, &self.serendipity.shown, now, roll) else {
            return;
        };
        let c = cands[i].clone();
        self.serendipity.shown.push(c.key.clone());
        if let Some(s) = self.serendipity.seen.as_mut() {
            s.insert(c.key.clone(), now);
            // Forget what's past its month, so the record stays small.
            s.retain(|_, t| now.saturating_sub(*t) < QUIET_SECS);
            let keep = s.clone();
            if let Some(d) = dir.as_deref() {
                let _ = blyg_core::state::AppState::update(d, |st| st.serendipity_seen = keep);
            }
        }
        self.serendipity.seq += 1;
        self.serendipity.card = Some(Card {
            c,
            seq: self.serendipity.seq,
        });
        cx.notify();
    }

    /// ⌘N's editor, still empty and untouched.
    fn serendipity_empty_editor(&self, cx: &App) -> bool {
        self.new_post.as_ref().is_some_and(|np| np.id.is_none())
            && self.current.is_none()
            && self.reading.view == reading::View::Posts
            && self.editor.read(cx).value().is_empty()
    }

    /// Whether the card is up.
    pub(crate) fn serendipity_shown(&self, cx: &App) -> bool {
        self.serendipity.card.is_some() && self.serendipity_empty_editor(cx)
    }

    /// Hook (every frame): the first keystroke (or leaving) dismisses it.
    pub(super) fn serendipity_frame(&mut self, cx: &App) {
        if self.serendipity.card.is_some() && !self.serendipity_empty_editor(cx) {
            self.serendipity.card = None;
        }
    }

    /// Hook (the editor pane): q / l / o / r while the card is up.
    pub(super) fn serendipity_keys(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
            let m = &ev.keystroke.modifiers;
            if m.platform || m.control || m.alt || m.shift || m.function {
                return;
            }
            let k = ev.keystroke.key.as_str();
            if matches!(k, "q" | "l" | "o" | "r")
                && this.serendipity_shown(cx)
                && this.serendipity_act(k, window, cx)
            {
                cx.stop_propagation();
            }
        }))
    }

    /// Do what `key` says to the card's post. `true` when it acted.
    pub(crate) fn serendipity_act(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(c) = self.serendipity.card.as_ref().map(|c| c.c.clone()) else {
            return false;
        };
        match key {
            "q" | "l" => {
                let text = if key == "q" {
                    format!(
                        "{}\n",
                        blyg_core::grammar::directive_source(&c.id, Some(&c.title))
                    )
                } else {
                    format!(
                        "{} ",
                        blyg_core::grammar::link_source(&c.id, Some(&c.title))
                    )
                };
                self.serendipity.card = None;
                self.editor.update(cx, |s, cx| s.insert(text, window, cx));
                self.new_post_first_keystroke(cx);
                self.after_edit(cx);
                // A quote lives in a thread.
                if key == "q"
                    && self
                        .current
                        .as_ref()
                        .is_some_and(|i| i.kind == Kind::Fragment)
                {
                    self.toggle_kind_now(window, cx);
                }
            }
            "o" => {
                self.serendipity.card = None;
                match (&c.own, &c.held) {
                    (Some(id), _) => self.open(id, window, cx),
                    (None, Some((origin, _))) => {
                        self.open_original(origin.clone(), c.id.clone(), None, window, cx)
                    }
                    _ => {}
                }
            }
            "r" => {
                #[cfg(test)]
                if let Some(s) = self.serendipity.seed.as_mut() {
                    *s = s.wrapping_add(1);
                }
                let had = self.serendipity.card.take();
                self.serendipity_offer(cx);
                if self.serendipity.card.is_none() {
                    // Nothing else to show: keep this one.
                    self.serendipity.card = had;
                    self.show_toast("That's every post for now", None, cx);
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    /// Hook (the editor's text area): the card, under the placeholder.
    pub(super) fn render_serendipity(
        &self,
        side_pad: Pixels,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.serendipity_shown(cx) {
            return None;
        }
        let card = self.serendipity.card.as_ref()?;
        let c = &card.c;
        let p = self.palette;
        let theme = &*self.theme;
        let muted = c.held.is_some();
        let key = |k: &'static str, label: &'static str| {
            div()
                .id(SharedString::from(format!("seren-{k}")))
                .debug_selector(move || format!("seren-{k}"))
                .flex()
                .items_center()
                .gap(px(4.))
                .cursor_pointer()
                .hover(|s| s.text_color(p.ink))
                .child(
                    crate::theme_ext::kbd(div(), theme)
                        .px(px(5.))
                        .text_size(px(10.5))
                        .child(k),
                )
                .child(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.serendipity_act(k, window, cx);
                }))
        };
        let el = div()
            .id("serendipity")
            .debug_selector(|| "serendipity".into())
            .absolute()
            .top(px(84.))
            .left(side_pad)
            .right(side_pad)
            .flex()
            .flex_col()
            .gap(px(6.))
            .pl(px(12.))
            .border_l_2()
            .border_color(p.line)
            .text_color(p.muted)
            .child(
                div()
                    .font_family(self.theme.chrome_font(&self.prefs))
                    .text_size(px(10.5))
                    .child(heading(c)),
            )
            .child(
                div()
                    .italic()
                    .line_height(relative(1.5))
                    .when(!muted, |d| d.text_color(p.ink.opacity(0.62)))
                    .child(c.excerpt.clone()),
            )
            .child(
                div()
                    .font_family(self.theme.chrome_font(&self.prefs))
                    .text_size(px(11.))
                    .child(why(c, self.now)),
            )
            .child(
                div()
                    .mt(px(2.))
                    .flex()
                    .flex_wrap()
                    .gap(px(12.))
                    .font_family(self.theme.chrome_font(&self.prefs))
                    .text_size(px(11.))
                    .child(key("q", "quote"))
                    .child(key("l", "link"))
                    .child(key("o", "open"))
                    .child(key("r", "another")),
            );
        Some(if crate::app::delight::reduce_motion() {
            el.into_any_element()
        } else {
            el.with_animation(
                ("serendipity-in", card.seq),
                Animation::new(Duration::from_millis(200)).with_easing(ease_out_quint()),
                |d, t| d.opacity(t),
            )
            .into_any_element()
        })
    }

    /// Forget what was shown (tests: a fresh sitting, a new month).
    #[cfg(test)]
    pub(crate) fn serendipity_forget(&mut self) {
        self.serendipity.shown.clear();
        self.serendipity.seen = Some(BTreeMap::new());
    }

    /// Settings' row: SERENDIPITY.
    pub(super) fn set_serendipity(&mut self, m: Mode, cx: &mut Context<Self>) {
        if let Err(e) = crate::settings::write(
            &[(
                "serendipity",
                blyg_core::config::Change::Set(m.value().into()),
            )],
            cx,
        ) {
            self.show_toast(e, None, cx);
            return;
        }
        self.watch_stamp = crate::settings::watch_stamp(&crate::settings::watched(cx));
        cx.notify();
    }
}

impl MainView {
    /// Hook (Settings): SERENDIPITY (`serendipity`) and SOUNDS (`sounds`).
    pub(super) fn render_flow_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let theme = &*self.theme;
        let chip = |id: &'static str, label: &'static str, on: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(10.))
                .py(px(3.))
                .map(|d| crate::theme_ext::chip(d, theme, on))
                .cursor_pointer()
                .hover(|s| s.border_color(p.accent).bg(p.hover()))
                .child(label)
        };
        let row = |label: &'static str, content: Div| {
            div()
                .flex()
                .gap(px(12.))
                .py(px(7.))
                .child(
                    div()
                        .w(px(92.))
                        .flex_none()
                        .pt(px(4.))
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.muted)
                        .child(label),
                )
                .child(content.flex_1())
        };
        let note = |t: &'static str| div().text_size(px(11.)).text_color(p.muted).child(t);
        let m = mode(cx);
        let sounds = crate::app::delight::sounds_on(cx);
        let seren = [
            (Mode::Off, "seren-off", "Off"),
            (Mode::Own, "seren-own", "My posts"),
            (Mode::On, "seren-on", "Mine and what I've read"),
        ]
        .into_iter()
        .map(|(v, id, label)| {
            chip(id, label, m == v)
                .on_click(cx.listener(move |this, _, _, cx| this.set_serendipity(v, cx)))
        });
        div()
            .flex()
            .flex_col()
            .child(row(
                "SERENDIPITY",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(div().flex().flex_wrap().gap(px(6.)).children(seren))
                    .child(note(crate::keymap::hint(
                        "⌘N's empty editor shows an older post: q quote, l link, o open, r another",
                    ))),
            ))
            .child(row(
                "SOUNDS",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.))
                            .child(
                                chip("sounds-off", "Off", !sounds).on_click(
                                    cx.listener(|this, _, _, cx| this.set_sounds(false, cx)),
                                ),
                            )
                            .child(
                                chip("sounds-quiet", "Quiet", sounds).on_click(
                                    cx.listener(|this, _, _, cx| this.set_sounds(true, cx)),
                                ),
                            ),
                    )
                    .child(note(if cfg!(target_os = "windows") {
                        "A soft sound on publish and capture (macOS only)"
                    } else {
                        "A soft system sound on publish and capture; never while typing"
                    })),
            ))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        Candidate, Kind, LocalId, Mode, QUIET_SECS, Status, anniversary, candidates, excerpt, pick,
        why,
    };

    fn cand(key: &str, orphan: bool, anniversary: bool) -> Candidate {
        Candidate {
            key: key.into(),
            id: key.into(),
            title: key.into(),
            excerpt: String::new(),
            kind: Kind::Fragment,
            version: 1,
            created: String::new(),
            orphan,
            anniversary,
            own: Some(LocalId(key.into())),
            held: None,
        }
    }

    #[test]
    fn orphans_and_anniversaries_weigh_more() {
        let cs = vec![
            cand("plain", false, false),
            cand("orphan", true, false),
            cand("anniv", false, true),
        ];
        let mut hits = [0usize; 3];
        for roll in 0..3000 {
            hits[pick(&cs, &BTreeMap::new(), &[], 0, roll).unwrap()] += 1;
        }
        assert!(hits[1] > hits[2] && hits[2] > hits[0], "{hits:?}");
        assert!(hits[0] > 0, "a plain post still comes up");
    }

    #[test]
    fn a_post_shown_waits_a_month() {
        let cs = vec![cand("a", true, false), cand("b", false, false)];
        let now = 100 * 24 * 3600;
        let mut seen = BTreeMap::new();
        seen.insert("a".to_string(), now - 3600);
        for roll in 0..50 {
            assert_eq!(pick(&cs, &seen, &[], now, roll), Some(1));
        }
        seen.insert("a".to_string(), now - QUIET_SECS);
        assert!((0..200).any(|r| pick(&cs, &seen, &[], now, r) == Some(0)));
        // Shown this sitting: r never repeats it.
        assert_eq!(
            pick(&cs, &BTreeMap::new(), &["a".into(), "b".into()], now, 1),
            None
        );
    }

    #[test]
    fn the_pool_is_published_own_posts_and_read_ones_with_on() {
        let now = chrono::Utc::now();
        let fake = crate::fake::FakeBackend::with_timing(crate::fake::Timing::instant());
        let items = blyg_core::Backend::items(&fake);
        let reading = blyg_core::Backend::reading(&fake);
        let own = candidates(&items, &reading, Mode::Own, now);
        assert!(!own.is_empty());
        for c in &own {
            let i = items
                .iter()
                .find(|i| Some(&i.local_id) == c.own.as_ref())
                .unwrap();
            assert_eq!(i.status, Status::Public, "{} isn't published", c.title);
        }
        assert!(own.iter().all(|c| c.held.is_none()));
        assert!(candidates(&items, &reading, Mode::Off, now).is_empty());
        let on = candidates(&items, &reading, Mode::On, now);
        assert!(on.len() >= own.len());
        for c in on.iter().filter(|c| c.held.is_some()) {
            let r = reading.iter().find(|r| r.remote_id == c.id).expect("held");
            assert!(r.read_version.is_some() && r.state == "current");
        }
    }

    #[test]
    fn this_week_in_another_year() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-06T10:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert!(anniversary("2025-11-05T09:00:00Z", now));
        assert!(!anniversary("2026-11-05T09:00:00Z", now), "this year");
        assert!(!anniversary("2025-10-01T09:00:00Z", now));
        let mut c = cand("a", false, true);
        c.created = "2024-11-04T09:00:00Z".into();
        assert!(why(&c, now).ends_with("2 years ago this week"));
    }

    #[test]
    fn excerpts_are_plain_and_short() {
        assert_eq!(
            excerpt("# Lanterns\n\n> the point of a **lantern**"),
            "Lanterns the point of a lantern"
        );
        assert!(excerpt(&"word ".repeat(200)).ends_with('…'));
    }
}
