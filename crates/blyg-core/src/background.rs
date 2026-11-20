//! --- background --- Background and Foreground in the reader (blygger-studio
//! 0.61.0, contract 5.10.0; blygger-spec docs/background-plan.md, phase 1;
//! the `burst`, `wordless` and `maintenance` rules from studio 0.64).
//!
//! An entry is in Background when the owner placed it there, or, with no
//! choice on the entry, when its whole blyg is placed there or a reason that
//! counts applies (withdrawn always; each rule of [`RULES`] the blyg offers,
//! unless it's off). A choice of Foreground on the entry beats everything
//! else.
//!
//! A thread published with its parts (spec §13.7's fold) lists them as
//! "+N parts": the parts are the Background entries whose reason is `burst`
//! and whose `detail.thread` names it ([`burst_parts`]).
//!
//! Burrow doesn't work this out itself: own minor changes come from the
//! studio's private change classifier, and placements have no read route of
//! their own. It reads the studio's answer instead: every page of `GET
//! /api/reading?background=true` (the Background lens, worked out by the
//! studio from its own rows, placements and Settings), kept as a
//! [`BackgroundSet`]. Every row not in it is in the foreground. Before the
//! first read, or on a studio without `setReadingPlacement`, nothing is
//! annotated and the old rule holds: withdrawn posts are background
//! (`ReadingItem::is_background`).
//!
//! Reader-only: nothing here is published, nothing marks anything read.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::{OWN_SUBSCRIPTION, ReadingItem, RemoteId, SubId};

/// The capability that says a studio has Background (`PUT
/// /api/reading/placement`); `GET /api/reading?background=true`, the
/// `background` on subscriptions and `backgroundRules` in Settings came with it.
pub const OPERATION: &str = "setReadingPlacement";

/// Store meta: "1" when the blyg serves Background, "0" when it doesn't.
pub const SERVED_META: &str = "background.served";
/// Store meta: the last [`BackgroundSet`] read, as JSON.
pub const SET_META: &str = "background.set";

/// What Burrow says when the blyg has no Background.
pub const NOT_SERVED: &str = "Your blyg needs blygger-studio 0.61 or later for Background";

/// The rules an owner can turn off, in the order Settings lists them
/// (studio `BACKGROUND_RULES`). Withdrawn isn't one: it's always on. A
/// studio offers only some of them (0.61: `minor`); Settings shows the ones
/// its `GET /api/settings` lists ([`BackgroundRules::from_settings`]),
/// because it refuses a rule it doesn't know with a 400.
pub const RULES: [&str; 4] = [BURST, "wordless", "maintenance", "minor"];

/// The reason a thread's part is in Background (studio 0.64); its
/// `detail.thread` names the thread.
pub const BURST: &str = "burst";

/// The owner's choice on one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    Background,
    Foreground,
}

impl Placement {
    pub fn as_str(self) -> &'static str {
        match self {
            Placement::Background => "background",
            Placement::Foreground => "foreground",
        }
    }

    fn parse(s: &str) -> Option<Placement> {
        match s {
            "background" => Some(Placement::Background),
            "foreground" => Some(Placement::Foreground),
            _ => None,
        }
    }
}

/// Where one reading row stands, as the studio says (`ReadingEntry.background`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntryBackground {
    /// In the Background lens (and out of every other).
    pub on: bool,
    /// The reasons that count under Settings, in the studio's order:
    /// `withdrawn`, then each enabled rule that applies (`minor`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
    /// The owner's choice on this entry; `None`: the rules decide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
    /// The owner moved this entry's whole blyg to Background.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blyg: bool,
    /// What a reason's label says, e.g. `{"category": "typo"}` for minor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

impl EntryBackground {
    /// Why it's in Background, in the studio's precedence (its Background
    /// lens byline): your choice, then the blyg, then the first reason.
    /// `None` for withdrawn (the post already says so) or no reason.
    pub fn why(&self) -> Option<String> {
        if self.placement == Some(Placement::Background) {
            return Some("you moved this".into());
        }
        if self.blyg {
            return Some("blyg in Background".into());
        }
        match self.reasons.first().map(String::as_str) {
            Some(BURST) => Some("part of a burst".into()),
            Some("wordless") => Some("no words of its own".into()),
            Some("maintenance") => Some("republished unchanged".into()),
            Some("minor") => Some(
                match self
                    .detail
                    .as_ref()
                    .and_then(|d| d.get("category"))
                    .and_then(Value::as_str)
                {
                    Some(c) => format!("minor change ({c})"),
                    None => "minor change".into(),
                },
            ),
            _ => None,
        }
    }
}

impl EntryBackground {
    /// The thread this entry is a part of: its `detail.thread`, when
    /// `burst` is among the reasons that count (so not while the rule is
    /// off). A remote id of the same blyg, or your own item's id.
    pub fn burst_thread(&self) -> Option<&str> {
        if !self.on || !self.reasons.iter().any(|r| r == BURST) {
            return None;
        }
        self.detail.as_ref()?.get("thread")?.as_str()
    }
}

/// Each thread's parts ("+N parts", studio 0.64's `ReadingEntry.parts`),
/// from annotated rows ([`BackgroundSet::annotate`]): thread key → part
/// keys, oldest first (the studio orders them by creation), and where two
/// were made in the same second, in the order the thread quotes them. A
/// part is a current row of the thread's own blyg (`~own` for yours) that
/// is in Background as `burst` and names the thread. Withdrawn parts
/// aren't listed.
pub fn burst_parts(rows: &[ReadingItem]) -> HashMap<Key, Vec<Key>> {
    let at = |r: &ReadingItem| r.created.clone().unwrap_or_else(|| r.observed_at.clone());
    let mut out: HashMap<Key, Vec<(String, Key)>> = HashMap::new();
    for r in rows.iter().filter(|r| r.state != "tombstone") {
        let Some(thread) = r.background.as_ref().and_then(|b| b.burst_thread()) else {
            continue;
        };
        let part = (r.subscription_id.clone(), r.remote_id.clone());
        let list = out
            .entry((r.subscription_id.clone(), RemoteId::from(thread)))
            .or_default();
        if !list.iter().any(|(_, k)| *k == part) {
            list.push((at(r), part));
        }
    }
    out.into_iter()
        .map(|(thread, mut parts)| {
            let quoted: Vec<&str> = rows
                .iter()
                .find(|r| r.subscription_id == thread.0 && r.remote_id == thread.1)
                .map(|t| t.transclusions.iter().map(|q| q.id.as_str()).collect())
                .unwrap_or_default();
            let place = |k: &Key| {
                quoted
                    .iter()
                    .position(|q| *q == &*k.1)
                    .unwrap_or(usize::MAX)
            };
            parts.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| place(&a.1).cmp(&place(&b.1))));
            (thread, parts.into_iter().map(|(_, k)| k).collect())
        })
        .collect()
}

/// What a thread says of its parts: "+2 parts" folded, "−2 parts" listed
/// (the studio's toggle).
pub fn parts_label(n: usize, listed: bool) -> String {
    format!(
        "{}{n} {}",
        if listed { "−" } else { "+" },
        if n == 1 { "part" } else { "parts" }
    )
}

/// A reading row's key: `(subscription id, remote id)`, your own posts as
/// `("~own", id)`, as every Burrow list keys them.
pub type Key = (SubId, RemoteId);

/// The studio's Background lens as last read, plus the Foreground choices
/// made here (the studio only shows a Foreground choice on an entry it
/// lists in the foreground, which Burrow never reads whole).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BackgroundSet {
    /// Every entry in Background, with why.
    #[serde(default)]
    pub entries: Vec<(Key, EntryBackground)>,
    /// Foreground choices this client made (cleared when it clears them,
    /// or when the studio lists the entry in Background again).
    #[serde(default)]
    pub foreground: Vec<Key>,
    #[serde(skip)]
    index: Option<Index>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Index {
    on: HashMap<Key, usize>,
    fg: HashSet<Key>,
    blygs: HashSet<SubId>,
}

/// One `GET /api/reading` entry's key and `background`. `None` for a shape
/// that isn't one (no key, or no `background`: a studio before 0.61).
pub fn parse_entry(v: &Value) -> Option<(Key, EntryBackground)> {
    let key = if let Some(own) = v.get("own").filter(|o| o.is_object()) {
        (
            SubId::from(OWN_SUBSCRIPTION),
            RemoteId::from(own.get("id")?.as_str()?),
        )
    } else {
        let imp = v.get("imported").filter(|o| o.is_object())?;
        (
            SubId::from(imp.get("subscriptionId")?.as_str()?),
            RemoteId::from(imp.get("remoteId")?.as_str()?),
        )
    };
    let b = v.get("background")?;
    let reasons = b
        .get("reasons")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Some((
        key,
        EntryBackground {
            on: true,
            reasons,
            placement: b
                .get("placement")
                .and_then(Value::as_str)
                .and_then(Placement::parse),
            blyg: b.get("blyg").and_then(Value::as_bool).unwrap_or(false),
            detail: b.get("detail").filter(|d| d.is_object()).cloned(),
        },
    ))
}

impl BackgroundSet {
    /// The set from the Background lens's entries (every page, collected),
    /// keeping this client's Foreground choices the studio doesn't contradict.
    pub fn from_entries(items: &[Value], previous: Option<&BackgroundSet>) -> BackgroundSet {
        let mut seen = HashSet::new();
        let entries: Vec<(Key, EntryBackground)> = items
            .iter()
            .filter_map(parse_entry)
            .filter(|(k, _)| seen.insert(k.clone()))
            .collect();
        let foreground = previous
            .map(|p| {
                p.foreground
                    .iter()
                    .filter(|k| !seen.contains(*k))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        BackgroundSet {
            entries,
            foreground,
            index: None,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(s: &str) -> Option<BackgroundSet> {
        serde_json::from_str(s).ok()
    }

    fn index(&mut self) -> &Index {
        if self.index.is_none() {
            self.index = Some(Index {
                on: self
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(i, (k, _))| (k.clone(), i))
                    .collect(),
                fg: self.foreground.iter().cloned().collect(),
                blygs: self
                    .entries
                    .iter()
                    .filter(|(_, e)| e.blyg)
                    .map(|(k, _)| k.0.clone())
                    .collect(),
            });
        }
        self.index.as_ref().expect("built above")
    }

    /// Ready for many [`Self::of`] lookups.
    pub fn indexed(mut self) -> BackgroundSet {
        self.index();
        self
    }

    /// How many entries are in Background.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether a whole blyg (a subscription id) is in Background, as far as
    /// its entries in the set say.
    pub fn blyg_on(&self, sub: &SubId) -> bool {
        match &self.index {
            Some(ix) => ix.blygs.contains(sub),
            None => self.entries.iter().any(|(k, e)| e.blyg && k.0 == *sub),
        }
    }

    /// Where the row `key` stands.
    pub fn of(&self, key: &Key) -> EntryBackground {
        let found = match &self.index {
            Some(ix) => ix.on.get(key).map(|&i| &self.entries[i].1),
            None => self.entries.iter().find(|(k, _)| k == key).map(|(_, e)| e),
        };
        if let Some(e) = found {
            return e.clone();
        }
        let fg = match &self.index {
            Some(ix) => ix.fg.contains(key),
            None => self.foreground.contains(key),
        };
        EntryBackground {
            on: false,
            placement: fg.then_some(Placement::Foreground),
            blyg: key.0 != OWN_SUBSCRIPTION && self.blyg_on(&key.0),
            ..Default::default()
        }
    }

    /// Put each row's place on it (`ReadingItem::background`).
    pub fn annotate(&self, rows: &mut [ReadingItem]) {
        for r in rows {
            r.background = Some(self.of(&(r.subscription_id.clone(), r.remote_id.clone())));
        }
    }

    /// The owner chose `placement` on the row `key` (`withdrawn`: it's a
    /// withdrawn post): its place now, before the studio is read again. The
    /// rules' reasons stay as last read; clearing a choice works out the
    /// place from them, the blyg and withdrawal.
    pub fn place(&mut self, key: &Key, withdrawn: bool, placement: Option<Placement>) {
        let mut e = self.of(key);
        self.entries.retain(|(k, _)| k != key);
        self.foreground.retain(|k| k != key);
        e.placement = placement;
        if withdrawn && !e.reasons.iter().any(|r| r == "withdrawn") {
            e.reasons.insert(0, "withdrawn".into());
        }
        e.on = match placement {
            Some(Placement::Background) => true,
            Some(Placement::Foreground) => false,
            None => e.blyg || !e.reasons.is_empty(),
        };
        if e.on {
            self.entries.push((key.clone(), e));
        } else if placement == Some(Placement::Foreground) {
            self.foreground.push(key.clone());
        }
        self.index = None;
    }

    /// The owner moved the blyg `sub` to Background (`on`) or back: each
    /// entry of it without a choice of its own follows, as far as the set
    /// knows (the studio is read again after).
    pub fn place_blyg(&mut self, sub: &SubId, on: bool, rows: &[ReadingItem]) {
        for (_, e) in self.entries.iter_mut().filter(|(k, _)| k.0 == *sub) {
            e.blyg = on;
        }
        if on {
            let have: HashSet<Key> = self.entries.iter().map(|(k, _)| k.clone()).collect();
            let fg: HashSet<&Key> = self.foreground.iter().collect();
            for r in rows.iter().filter(|r| r.subscription_id == *sub) {
                let k = (r.subscription_id.clone(), r.remote_id.clone());
                if have.contains(&k) || fg.contains(&k) {
                    continue;
                }
                self.entries.push((
                    k,
                    EntryBackground {
                        on: true,
                        blyg: true,
                        reasons: if r.state == "tombstone" {
                            vec!["withdrawn".into()]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    },
                ));
            }
        } else {
            self.entries.retain(|(k, e)| {
                k.0 != *sub || e.placement == Some(Placement::Background) || !e.reasons.is_empty()
            });
        }
        self.index = None;
    }
}

/// The owner's Background rules (`Settings.backgroundRules`): rule → on,
/// for the rules the blyg offers. [`Self::on`] reads a rule it didn't send
/// as on, as the studio's defaults do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackgroundRules {
    pub rules: Vec<(String, bool)>,
}

impl BackgroundRules {
    /// From `GET /api/settings`, which sends every rule the studio has:
    /// the rules of [`RULES`] it lists, in that order (a 0.61 studio:
    /// `minor` only). `None` when it has no `backgroundRules` (before 0.61).
    pub fn from_settings(v: &Value) -> Option<BackgroundRules> {
        let o = v.get("backgroundRules")?.as_object()?;
        Some(BackgroundRules {
            rules: RULES
                .iter()
                .filter(|r| o.contains_key(**r))
                .map(|r| {
                    (
                        r.to_string(),
                        o.get(*r).and_then(Value::as_bool) != Some(false),
                    )
                })
                .collect(),
        })
    }

    /// Whether the blyg offers `rule` (its settings list it).
    pub fn offers(&self, rule: &str) -> bool {
        self.rules.iter().any(|(r, _)| r == rule)
    }

    /// Whether `rule` counts.
    pub fn on(&self, rule: &str) -> bool {
        self.rules
            .iter()
            .find(|(r, _)| r == rule)
            .is_none_or(|(_, on)| *on)
    }

    /// The partial `PATCH /api/settings` body that turns one rule on or
    /// off and leaves every other setting (and rule) as it is: the studio
    /// merges `backgroundRules` into what it stores.
    pub fn patch(rule: &str, on: bool) -> Value {
        json!({ "backgroundRules": { rule: on } })
    }
}

/// What Settings says of each rule (the studio's Settings › Background).
pub fn rule_label(rule: &str) -> (&'static str, &'static str) {
    match rule {
        BURST => (
            "Parts published with a thread",
            "Items a thread names that were published with it, within 15 minutes; \
             the thread lists them as “+N parts”.",
        ),
        "wordless" => (
            "Posts with no words of their own",
            "Only quotes of other posts, nothing the author wrote.",
        ),
        "maintenance" => (
            "Republished unchanged",
            "A new version with the same text and the same quotes.",
        ),
        "minor" => (
            "Your own minor changes",
            "A version of yours the change classifier calls a typo, minor or refresh.",
        ),
        _ => ("", ""),
    }
}

/// Whether a `GET /api/capabilities` answer lists [`OPERATION`].
pub fn serves(capabilities: &Value) -> bool {
    match capabilities.get("operations") {
        Some(Value::Array(a)) => a.iter().any(|o| o.as_str() == Some(OPERATION)),
        Some(Value::Object(m)) => m.contains_key(OPERATION),
        _ => false,
    }
}

/// `PUT /api/reading/placement`'s body for the row `key` (your own posts
/// are `source: own`, no `sub`).
pub fn placement_body(key: &Key, placement: Option<Placement>) -> Value {
    let p = placement.map_or(Value::Null, |p| json!(p.as_str()));
    if key.0 == OWN_SUBSCRIPTION {
        json!({ "source": "own", "id": key.1, "placement": p })
    } else {
        json!({ "source": "imported", "sub": key.0, "id": key.1, "placement": p })
    }
}

/// The path of one page of the Background lens (the contract's `limit` is
/// at most 50 on `/api/reading`).
pub const LENS_QUERY: &str = "background=true";
pub const LENS_LIMIT: u32 = 50;

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
