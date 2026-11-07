//! --- reader folders --- Pure rules for the Reader's sources pane: the
//! smart feeds, folders and subscriptions it lists (in order, with their
//! unread state), which posts a selected source shows, and how focus moves
//! between the three panes. `sources.rs` renders what these say.

use std::collections::{HashMap, HashSet};

use blyg_core::{Folder, Hopper, ReadingItem, Subscription};
use chrono::{DateTime, Local, Utc};

use super::vm::Key;

/// The smart feeds at the top of the sources pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Smart {
    /// Posts you haven't read, or that were edited since you read them.
    Unread,
    /// Posts dated today (the post's own date, local time).
    Today,
    /// Posts you gave a thumb up.
    Thumbed,
    /// Everything held.
    All,
    /// --- background --- withdrawn posts only (studio 0.49.0's Background
    /// lens): the blygs' tombstones and your own withdrawn posts. Nowhere
    /// else lists them.
    Background,
}

impl Smart {
    pub const ALL: [Smart; 5] = [
        Smart::Unread,
        Smart::Today,
        Smart::Thumbed,
        Smart::All,
        Smart::Background,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Smart::Unread => "All unread",
            Smart::Today => "Today",
            Smart::Thumbed => "Thumbed",
            Smart::All => "All",
            Smart::Background => "Background",
        }
    }
}

/// What the middle pane lists.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    Smart(Smart),
    /// A folder id: the posts of every subscription filed in it.
    Folder(String),
    /// A subscription id.
    Sub(String),
    /// --- own posts --- your own published posts ("Your blyg").
    Own,
    /// --- hoppers --- a hopper id: the posts in it, theirs and yours.
    Hopper(String),
    /// --- trails --- a trail id: its stops, in its order (`trails.rs`).
    Trail(String),
    /// --- contacts --- the held posts that mention you
    /// (`mentions_feed`).
    Mentions,
}

impl Default for Source {
    fn default() -> Self {
        Source::Smart(Smart::All)
    }
}

/// Which of the three panes has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    Sources,
    #[default]
    List,
    Post,
}

/// ← / → from `pane`. ← stops at the sources (or at the list when the
/// sources pane is hidden); → reaches the post only when one is open.
pub fn pane_step(pane: Pane, right: bool, sources_open: bool, post_open: bool) -> Pane {
    match (pane, right) {
        (Pane::Sources, true) => Pane::List,
        (Pane::List, true) if post_open => Pane::Post,
        (Pane::List, false) if sources_open => Pane::Sources,
        (Pane::Post, false) => Pane::List,
        (p, _) => p,
    }
}

/// Counts as "to read" for the unread smart feed, the folder counts and
/// the dots: never read, or edited since you read it.
pub fn needs_reading(r: &ReadingItem) -> bool {
    !is_background(r) && (r.is_unread() || r.edited_since_read())
}

/// --- background --- A withdrawn post (an imported tombstone, or your own
/// withdrawn post): listed under Background only, never unread.
pub fn is_background(r: &ReadingItem) -> bool {
    r.state == "tombstone"
}

/// --- background --- The stream's rows (indices from the search's
/// `searched`): everything but the withdrawn posts.
pub fn stream_rows(rows: &[ReadingItem], searched: &[usize]) -> Vec<usize> {
    searched
        .iter()
        .copied()
        .filter(|&i| rows.get(i).is_some_and(|r| !is_background(r)))
        .collect()
}

/// The post's own date (updated, else created) falls on today, local time.
pub fn is_today(r: &ReadingItem, now: DateTime<Utc>) -> bool {
    let t = r.post_time();
    if t.observed {
        // No date of its own: never "today" just because it was imported.
        return false;
    }
    DateTime::parse_from_rfc3339(&t.at)
        .map(|d| d.with_timezone(&Local).date_naive() == now.with_timezone(&Local).date_naive())
        .unwrap_or(false)
}

/// Does `r` belong to `source`? `filed` = subscription id → folder id.
pub fn in_source(
    r: &ReadingItem,
    source: &Source,
    filed: &HashMap<String, String>,
    now: DateTime<Utc>,
) -> bool {
    // --- background --- withdrawn posts are in Background and nowhere else.
    if is_background(r) != (*source == Source::Smart(Smart::Background)) {
        return false;
    }
    match source {
        Source::Smart(Smart::All) | Source::Smart(Smart::Background) => true,
        Source::Smart(Smart::Unread) => needs_reading(r),
        Source::Smart(Smart::Today) => is_today(r, now),
        Source::Smart(Smart::Thumbed) => r.thumb == Some(1),
        Source::Folder(f) => filed.get(&r.subscription_id) == Some(f),
        Source::Sub(s) => &r.subscription_id == s,
        Source::Own => super::own::is_own_row(r),
        Source::Hopper(h) => r.hoppers.iter().any(|x| x == h),
        // --- trails --- listed in the trail's order by `State::refilter`.
        Source::Trail(_) => false,
        // --- contacts --- needs the feed's keys: `State::mentions_shown`.
        Source::Mentions => false,
    }
}

/// The rows (indices into `rows`, from the search's `searched`) the list
/// shows for `source`. Rows in `sticky` stay even when they no longer match
/// (a post read in "All unread" doesn't vanish under the cursor).
pub fn filter_source(
    rows: &[ReadingItem],
    searched: &[usize],
    source: &Source,
    filed: &HashMap<String, String>,
    sticky: &HashSet<Key>,
    now: DateTime<Utc>,
) -> Vec<usize> {
    searched
        .iter()
        .copied()
        .filter(|&i| {
            rows.get(i).is_some_and(|r| {
                in_source(r, source, filed, now)
                    || sticky.contains(&(r.subscription_id.clone(), r.remote_id.clone()))
            })
        })
        .collect()
}

/// One line of the sources pane.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// "Smart feeds", "Folders", "Subscriptions".
    Heading(&'static str),
    Smart {
        smart: Smart,
        /// Posts to read in it (a private, reader-local number).
        unread: usize,
    },
    Folder {
        folder: Folder,
        unread: usize,
        expanded: bool,
        /// Subscriptions filed in it.
        subs: usize,
    },
    Sub {
        sub: Subscription,
        /// Inside a folder (indented).
        filed: bool,
        /// Anything to read: a dot, never a count.
        unread: bool,
    },
    /// --- own posts --- "Your blyg": your published posts (never unread,
    /// so never a count or a dot).
    Own,
    /// --- hoppers --- one of your hoppers, with how many posts it holds
    /// (the blyg's count, not unread: a hopper is a collection).
    Hopper(Hopper),
    /// --- trails --- one of your trails, with its number of stops.
    Trail(blyg_core::Trail),
    /// --- contacts --- "Mentions", under All unread: a dot, never a count.
    Mentions { unread: bool },
}

impl Entry {
    /// The source this line selects (headings select nothing).
    pub fn source(&self) -> Option<Source> {
        match self {
            Entry::Heading(_) => None,
            Entry::Smart { smart, .. } => Some(Source::Smart(*smart)),
            Entry::Folder { folder, .. } => Some(Source::Folder(folder.id.clone())),
            Entry::Sub { sub, .. } => Some(Source::Sub(sub.id.clone())),
            Entry::Own => Some(Source::Own),
            Entry::Hopper(h) => Some(Source::Hopper(h.id.clone())),
            Entry::Trail(t) => Some(Source::Trail(t.id.clone())), // --- trails ---
            Entry::Mentions { .. } => Some(Source::Mentions),     // --- contacts ---
        }
    }
}

/// Everything the sources pane lists, top to bottom: the smart feeds (and
/// Your blyg), your hoppers, each folder with its subscriptions (unless
/// collapsed), then the subscriptions in no folder. Subscriptions keep the
/// server's order within a group, hoppers the blyg's.
pub fn entries(
    hoppers: &[Hopper],
    folders: &[Folder],
    filed: &HashMap<String, String>,
    subs: &[Subscription],
    rows: &[ReadingItem],
    collapsed: &HashSet<String>,
    now: DateTime<Utc>,
) -> Vec<Entry> {
    let mut unread_by_sub: HashMap<&str, usize> = HashMap::new();
    for r in rows.iter().filter(|r| needs_reading(r)) {
        *unread_by_sub.entry(r.subscription_id.as_str()).or_default() += 1;
    }
    let unread_of = |pred: &dyn Fn(&ReadingItem) -> bool| {
        rows.iter().filter(|r| needs_reading(r) && pred(r)).count()
    };
    let mut out = vec![Entry::Heading("Smart feeds")];
    for smart in Smart::ALL {
        let src = Source::Smart(smart);
        out.push(Entry::Smart {
            smart,
            unread: unread_of(&|r| in_source(r, &src, filed, now)),
        });
    }
    // --- own posts --- your blyg, under the smart feeds, once it has posts.
    if rows
        .iter()
        .any(|r| super::own::is_own_row(r) && !is_background(r))
    {
        out.push(Entry::Own);
    }
    // --- hoppers --- under Background (and Your blyg), before the folders.
    if !hoppers.is_empty() {
        out.push(Entry::Heading("Hoppers"));
        out.extend(hoppers.iter().cloned().map(Entry::Hopper));
    }
    // A folder id that no longer exists counts as unfiled.
    let known: HashSet<&str> = folders.iter().map(|f| f.id.as_str()).collect();
    let folder_of = |s: &Subscription| {
        filed
            .get(&s.id)
            .map(String::as_str)
            .filter(|f| known.contains(f))
    };
    let sub_entry = |s: &Subscription, in_folder: bool| Entry::Sub {
        sub: s.clone(),
        filed: in_folder,
        unread: unread_by_sub.get(s.id.as_str()).copied().unwrap_or(0) > 0,
    };
    if !folders.is_empty() {
        out.push(Entry::Heading("Folders"));
    }
    for f in folders {
        let members: Vec<&Subscription> = subs
            .iter()
            .filter(|s| folder_of(s) == Some(f.id.as_str()))
            .collect();
        let unread = members
            .iter()
            .map(|s| unread_by_sub.get(s.id.as_str()).copied().unwrap_or(0))
            .sum();
        let expanded = !collapsed.contains(&f.id);
        out.push(Entry::Folder {
            folder: f.clone(),
            unread,
            expanded,
            subs: members.len(),
        });
        if expanded {
            out.extend(members.into_iter().map(|s| sub_entry(s, true)));
        }
    }
    let unfiled: Vec<&Subscription> = subs.iter().filter(|s| folder_of(s).is_none()).collect();
    if !unfiled.is_empty() {
        out.push(Entry::Heading("Subscriptions"));
        out.extend(unfiled.into_iter().map(|s| sub_entry(s, false)));
    }
    out
}

/// The selectable sources in pane order (↑/↓ move through these).
pub fn selectable(entries: &[Entry]) -> Vec<Source> {
    entries.iter().filter_map(Entry::source).collect()
}

/// ↑/↓ in the sources pane: the source `delta` steps from `current`
/// (clamped); the first one when `current` isn't listed.
pub fn step_source(entries: &[Entry], current: &Source, delta: isize) -> Option<Source> {
    let list = selectable(entries);
    if list.is_empty() {
        return None;
    }
    let next = match list.iter().position(|s| s == current) {
        None => 0,
        Some(i) => (i as isize + delta).clamp(0, list.len() as isize - 1) as usize,
    };
    list.get(next).cloned()
}

/// The letter badge for a subscription without a cached avatar.
pub fn letter(title: &str, origin: &str) -> String {
    let src = if title.trim().is_empty() {
        super::vm::host(origin)
    } else {
        title.to_string()
    };
    src.chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "·".into())
}

/// The name a subscription row shows: its title, else its host.
pub fn sub_name(s: &Subscription) -> String {
    if s.title.trim().is_empty() {
        super::vm::host(&s.origin)
    } else {
        s.title.clone()
    }
}

/// The source's name, for the list header and the empty state.
pub fn source_label(
    source: &Source,
    folders: &[Folder],
    subs: &[Subscription],
    hoppers: &[Hopper],
) -> String {
    match source {
        Source::Hopper(id) => hoppers
            .iter()
            .find(|h| &h.id == id)
            .map(|h| h.name.clone())
            .unwrap_or_else(|| "Hopper".into()),
        Source::Smart(s) => s.label().to_string(),
        Source::Folder(id) => folders
            .iter()
            .find(|f| &f.id == id)
            .map(|f| f.name.clone())
            .unwrap_or_else(|| "Folder".into()),
        Source::Sub(id) => subs
            .iter()
            .find(|s| &s.id == id)
            .map(sub_name)
            .unwrap_or_else(|| "Subscription".into()),
        Source::Own => super::own::OWN_LABEL.into(),
        Source::Trail(_) => "Trail".into(), // --- trails --- (named by the caller)
        Source::Mentions => MENTIONS_LABEL.into(), // --- contacts ---
    }
}

/// --- contacts --- The Mentions feed's name.
pub const MENTIONS_LABEL: &str = "Mentions";

/// What the list says when the selected source has nothing to show.
pub fn empty_label(source: &Source) -> &'static str {
    match source {
        Source::Smart(Smart::Unread) => "All read. Nothing new to read.",
        Source::Smart(Smart::Today) => "Nothing dated today.",
        Source::Smart(Smart::Thumbed) => "No thumbed posts yet. 👍 a post to keep it here.",
        Source::Smart(Smart::All) => "Nothing to read yet.",
        Source::Smart(Smart::Background) => {
            "Nothing withdrawn. Posts their authors withdraw are kept here, out of the way."
        }
        Source::Folder(_) => "Nothing here. Drag a subscription onto the folder to file it.",
        Source::Sub(_) => "No posts from this subscription yet.",
        Source::Own => "Nothing published yet. Your published posts show here.",
        Source::Hopper(_) => "Nothing in this hopper yet. Press h on a post to put it here.",
        Source::Trail(_) => "No stops held here yet. Press t on a post to add it to the trail.",
        Source::Mentions => "No post you hold mentions you yet.", // --- contacts ---
    }
}

/// After the current post (position `from` in `shown`, or before the top),
/// the next one that needs reading: Space's "next unread".
pub fn next_unread(rows: &[ReadingItem], shown: &[usize], from: Option<usize>) -> Option<usize> {
    let start = from.map(|i| i + 1).unwrap_or(0);
    (start..shown.len()).find(|&p| rows.get(shown[p]).is_some_and(needs_reading))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::reading_seed::{self, *};

    fn seed() -> (reading_seed::Seed, DateTime<Utc>) {
        let now = Utc::now();
        (reading_seed::seed(now), now)
    }

    #[test]
    fn panes_step_left_and_right() {
        use Pane::*;
        assert_eq!(pane_step(List, false, true, false), Sources);
        assert_eq!(pane_step(List, false, false, false), List, "sources hidden");
        assert_eq!(pane_step(Sources, false, true, true), Sources);
        assert_eq!(pane_step(Sources, true, true, false), List);
        assert_eq!(pane_step(List, true, true, false), List, "nothing open");
        assert_eq!(pane_step(List, true, true, true), Post);
        assert_eq!(pane_step(Post, true, true, true), Post);
        assert_eq!(pane_step(Post, false, true, true), List);
    }

    #[test]
    fn the_pane_lists_smart_feeds_then_folders_then_unfiled() {
        let (sd, now) = seed();
        let rows = super::super::vm::order(sd.reading.clone());
        let e = entries(
            &[],
            &sd.folders,
            &sd.filed,
            &sd.subs,
            &rows,
            &HashSet::new(),
            now,
        );
        let lines: Vec<String> = e
            .iter()
            .map(|e| match e {
                Entry::Heading(h) => format!("# {h}"),
                Entry::Smart { smart, .. } => smart.label().into(),
                Entry::Folder { folder, .. } => format!("[{}]", folder.name),
                Entry::Sub { sub, filed, .. } => {
                    format!("{}{}", if *filed { "  " } else { "" }, sub.title)
                }
                Entry::Own => "Your blyg".into(),
                Entry::Hopper(h) => format!("▤ {}", h.name),
                Entry::Trail(t) => format!("⤳ {}", t.name), // --- trails ---
                Entry::Mentions { .. } => "Mentions".into(),
            })
            .collect();
        assert_eq!(
            lines,
            [
                "# Smart feeds",
                "All unread",
                "Today",
                "Thumbed",
                "All",
                "Background",
                "# Folders",
                "[Friends]",
                "  Rue",
                "  Ada",
                "[Gardens]",
                "  Lin",
                "# Subscriptions",
                "Omar's notes",
            ]
        );
        // Collapsing a folder hides its subscriptions, not the folder.
        let e2 = entries(
            &[],
            &sd.folders,
            &sd.filed,
            &sd.subs,
            &rows,
            &HashSet::from([FOLDER_FRIENDS.to_string()]),
            now,
        );
        assert!(
            !e2.iter()
                .any(|e| matches!(e, Entry::Sub { sub, .. } if sub.id == "sub-rue"))
        );
        assert!(e2.iter().any(
            |e| matches!(e, Entry::Folder { folder, expanded: false, subs: 2, .. } if folder.id == FOLDER_FRIENDS)
        ));
    }

    #[test]
    fn counts_are_unread_posts_and_subscriptions_get_a_dot() {
        let (sd, now) = seed();
        let rows = super::super::vm::order(sd.reading.clone());
        let e = entries(
            &[],
            &sd.folders,
            &sd.filed,
            &sd.subs,
            &rows,
            &HashSet::new(),
            now,
        );
        let unread_total = rows.iter().filter(|r| needs_reading(r)).count();
        assert!(unread_total > 0);
        assert!(e.iter().any(
            |e| matches!(e, Entry::Smart { smart: Smart::Unread, unread } if *unread == unread_total)
        ));
        // Friends = Rue + Ada.
        let friends = rows
            .iter()
            .filter(|r| {
                needs_reading(r) && ["sub-rue", "sub-ada"].contains(&r.subscription_id.as_str())
            })
            .count();
        assert!(e.iter().any(
            |e| matches!(e, Entry::Folder { folder, unread, .. } if folder.id == FOLDER_FRIENDS && *unread == friends)
        ));
        // Omar's one post was read: no dot.
        assert!(
            e.iter().any(
                |e| matches!(e, Entry::Sub { sub, unread: false, .. } if sub.id == "sub-omar")
            )
        );
    }

    #[test]
    fn sources_filter_the_list() {
        let (sd, now) = seed();
        let rows = super::super::vm::order(sd.reading.clone());
        let all: Vec<usize> = (0..rows.len()).collect();
        let ids = |src: Source, sticky: &HashSet<Key>| -> Vec<String> {
            filter_source(&rows, &all, &src, &sd.filed, sticky, now)
                .into_iter()
                .map(|i| rows[i].remote_id.clone())
                .collect()
        };
        let none = HashSet::new();
        let gardens = ids(Source::Folder(FOLDER_GARDENS.into()), &none);
        assert!(!gardens.is_empty());
        assert!(gardens.iter().all(|id| {
            rows.iter()
                .any(|r| &r.remote_id == id && r.subscription_id == "sub-lin")
        }));
        assert_eq!(ids(Source::Sub("sub-omar".into()), &none), [OMAR_YEAR]);
        // --- background --- Rue's thumbed post is withdrawn: Background only.
        assert!(ids(Source::Smart(Smart::Thumbed), &none).is_empty());
        assert_eq!(ids(Source::Smart(Smart::Background), &none), [RUE_KEPT]);
        let unread = ids(Source::Smart(Smart::Unread), &none);
        assert!(unread.contains(&RUE_TRUST.to_string()), "edited since read");
        assert!(!unread.contains(&ADA_TIDES.to_string()), "read");
        // A read post stays while it's sticky.
        let sticky = HashSet::from([("sub-ada".to_string(), ADA_TIDES.to_string())]);
        assert!(ids(Source::Smart(Smart::Unread), &sticky).contains(&ADA_TIDES.to_string()));
        assert_eq!(ids(Source::Smart(Smart::All), &none).len(), rows.len() - 1);
        assert!(!ids(Source::Smart(Smart::All), &none).contains(&RUE_KEPT.to_string()));
    }

    #[test]
    fn today_is_the_posts_own_date_in_local_time() {
        let (sd, now) = seed();
        let mut r = sd.reading[0].clone();
        r.created = Some(now.to_rfc3339());
        r.updated = None;
        assert!(is_today(&r, now));
        r.created = Some((now - chrono::Duration::days(3)).to_rfc3339());
        assert!(!is_today(&r, now));
        // Imported today, dated nothing: not "today".
        r.created = None;
        r.observed_at = now.to_rfc3339();
        assert!(!is_today(&r, now));
    }

    #[test]
    fn stepping_and_letters() {
        let (sd, now) = seed();
        let e = entries(
            &[],
            &sd.folders,
            &sd.filed,
            &sd.subs,
            &[],
            &HashSet::new(),
            now,
        );
        let all = Source::Smart(Smart::All);
        assert_eq!(
            step_source(&e, &all, 2),
            Some(Source::Folder(FOLDER_FRIENDS.into())),
            "headings are skipped (Background comes first)"
        );
        assert_eq!(
            step_source(&e, &Source::Smart(Smart::Unread), -1),
            Some(Source::Smart(Smart::Unread)),
            "clamped"
        );
        assert_eq!(letter("Omar's notes", ""), "O");
        assert_eq!(letter("", "https://zed.example.com/"), "Z");
        assert_eq!(letter("  ", ""), "·");
    }

    #[test]
    fn space_finds_the_next_post_to_read() {
        let (sd, _) = seed();
        let rows = super::super::vm::order(sd.reading.clone());
        let all: Vec<usize> = (0..rows.len()).collect();
        let first = next_unread(&rows, &all, None).unwrap();
        assert!(needs_reading(&rows[first]));
        let second = next_unread(&rows, &all, Some(first)).unwrap();
        assert!(second > first && needs_reading(&rows[second]));
        assert_eq!(next_unread(&rows, &all, Some(rows.len() - 1)), None);
    }

    // --- own posts ---
    #[test]
    fn your_blyg_is_a_source_but_never_unread_or_thumbed() {
        let (sd, now) = seed();
        let mut mine = sd.reading[0].clone();
        mine.subscription_id = super::super::own::OWN.into();
        mine.remote_id = "7c9wk2n4h6q1x8v0z3m5rjy2ke".into();
        mine.read_version = Some(mine.version);
        mine.thumb = None;
        mine.updated = Some(now.to_rfc3339());
        let mut rows = super::super::vm::order(sd.reading.clone());
        let before = entries(
            &[],
            &sd.folders,
            &sd.filed,
            &sd.subs,
            &rows,
            &HashSet::new(),
            now,
        );
        assert!(!before.contains(&Entry::Own), "no posts of yours, no entry");
        rows.push(mine.clone());
        let e = entries(
            &[],
            &sd.folders,
            &sd.filed,
            &sd.subs,
            &rows,
            &HashSet::new(),
            now,
        );
        let at = e
            .iter()
            .position(|x| *x == Entry::Own)
            .expect("Your blyg is listed");
        assert_eq!(
            e[at - 1],
            Entry::Smart {
                smart: Smart::Background,
                unread: 0,
            },
            "right under the smart feeds"
        );
        // The unread counts are the same with and without your posts.
        let counts = |e: &[Entry]| -> Vec<usize> {
            e.iter()
                .filter_map(|x| match x {
                    Entry::Smart { unread, .. } | Entry::Folder { unread, .. } => Some(*unread),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(counts(&e), counts(&before));
        let filed = HashMap::new();
        assert!(in_source(&mine, &Source::Own, &filed, now));
        assert!(in_source(&mine, &Source::Smart(Smart::All), &filed, now));
        assert!(in_source(&mine, &Source::Smart(Smart::Today), &filed, now));
        assert!(!in_source(
            &mine,
            &Source::Smart(Smart::Unread),
            &filed,
            now
        ));
        assert!(!in_source(
            &mine,
            &Source::Smart(Smart::Thumbed),
            &filed,
            now
        ));
        assert!(!in_source(&sd.reading[0], &Source::Own, &filed, now));
        assert_eq!(source_label(&Source::Own, &[], &[], &[]), "Your blyg");
        assert_eq!(
            selectable(&e).iter().filter(|s| **s == Source::Own).count(),
            1
        );
    }

    // --- background ---
    #[test]
    fn withdrawn_posts_are_background_and_nowhere_else() {
        let (sd, now) = seed();
        let mut rows = super::super::vm::order(sd.reading.clone());
        // A withdrawn post that was never read, and one of yours.
        let mut gone = sd.reading[0].clone();
        gone.remote_id = "01k2bg0gone000000000000001".into();
        gone.state = "tombstone".into();
        gone.read_version = None;
        gone.thumb = Some(1);
        gone.created = Some(now.to_rfc3339());
        let mut mine = gone.clone();
        mine.subscription_id = super::super::own::OWN.into();
        mine.remote_id = "7c9wk2n4h6q1x8v0z3m5rjy2bg".into();
        mine.thumb = None;
        rows.push(gone.clone());
        rows.push(mine.clone());
        let filed = HashMap::new();
        let bg = Source::Smart(Smart::Background);
        for r in [&gone, &mine] {
            assert!(is_background(r));
            assert!(!needs_reading(r), "never unread");
            assert!(in_source(r, &bg, &filed, now));
            for other in [
                Source::Smart(Smart::Unread),
                Source::Smart(Smart::Today),
                Source::Smart(Smart::Thumbed),
                Source::Smart(Smart::All),
                Source::Own,
                Source::Sub(r.subscription_id.clone()),
            ] {
                assert!(!in_source(r, &other, &filed, now), "{other:?}");
            }
        }
        // Background lists nothing live.
        assert!(
            rows.iter()
                .filter(|r| !is_background(r))
                .all(|r| !in_source(r, &bg, &filed, now))
        );
        // The stream leaves them out.
        let all: Vec<usize> = (0..rows.len()).collect();
        let stream = stream_rows(&rows, &all);
        assert!(stream.iter().all(|&i| !is_background(&rows[i])));
        assert_eq!(
            stream.len(),
            rows.iter().filter(|r| !is_background(r)).count()
        );
        // No count, and your withdrawn post alone doesn't make "Your blyg".
        let e = entries(&[], &[], &filed, &[], &rows, &HashSet::new(), now);
        assert!(e.contains(&Entry::Smart {
            smart: Smart::Background,
            unread: 0
        }));
        assert!(!e.contains(&Entry::Own));
        assert_eq!(source_label(&bg, &[], &[], &[]), "Background");
        assert!(empty_label(&bg).starts_with("Nothing withdrawn"));
    }

    // --- hoppers ---
    #[test]
    fn hoppers_are_a_section_and_a_source_but_never_withdrawn_posts() {
        let (sd, now) = seed();
        let mut rows = super::super::vm::order(sd.reading.clone());
        rows[0].hoppers = vec![HOPPER_SLOW.into()];
        let kept = rows[0].clone();
        let mut gone = rows[1].clone();
        gone.state = "tombstone".into();
        gone.hoppers = vec![HOPPER_SLOW.into()];
        rows.push(gone.clone());
        let e = entries(
            &sd.hoppers,
            &sd.folders,
            &sd.filed,
            &sd.subs,
            &rows,
            &HashSet::new(),
            now,
        );
        let at = e
            .iter()
            .position(|x| *x == Entry::Heading("Hoppers"))
            .unwrap();
        assert!(matches!(&e[at + 1], Entry::Hopper(h) if h.id == HOPPER_SLOW));
        assert!(matches!(&e[at + 2], Entry::Hopper(h) if h.id == HOPPER_GARDENS));
        assert_eq!(e[at + 3], Entry::Heading("Folders"));
        let filed = HashMap::new();
        let src = Source::Hopper(HOPPER_SLOW.into());
        assert!(in_source(&kept, &src, &filed, now));
        assert!(!in_source(
            &kept,
            &Source::Hopper(HOPPER_GARDENS.into()),
            &filed,
            now
        ));
        assert!(
            !in_source(&gone, &src, &filed, now),
            "withdrawn: Background only"
        );
        assert_eq!(source_label(&src, &[], &[], &sd.hoppers), "Slow media");
        assert!(empty_label(&src).contains("hopper"));
        // No hoppers, no section.
        let e = entries(&[], &[], &filed, &[], &rows, &HashSet::new(), now);
        assert!(!e.contains(&Entry::Heading("Hoppers")));
    }
}
