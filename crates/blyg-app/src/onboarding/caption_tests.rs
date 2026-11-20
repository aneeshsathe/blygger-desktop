//! Tour steps point at things that exist: the keys they quote are bound
//! (to the action the step waits for), the menu items they name are in the
//! menus, and the labels they quote (“…”) are text the app shows. Each was
//! fixed by hand after it drifted: 47db75a moved ⇧⌘P to the command
//! palette and the Extensions step still said "Press ⇧⌘P"; 02832fc renamed
//! the reply chips and lines and edited four captions to match.

use super::steps::{Key, STEPS, Step};
use crate::keymap::{self, Keybind};
use crate::platform_text::{Platform, Text};

/// The keymap actions whose key a step's `Key` is: the key label must be
/// bound to one of them. Exhaustive, so a new `Key` says which it is.
fn key_actions(k: Key) -> &'static [&'static str] {
    match k {
        Key::ToggleKind => &["ToggleKind"],
        Key::MakeDraft => &["MakeDraft"],
        Key::Publish => &["Publish"],
        Key::ViewWrite => &["ViewWrite"],
        Key::ViewSplit => &["ViewSplit"],
        Key::ViewStudio => &["ViewStudio"],
        Key::AiGenerate => &["AiGenerate"],
        Key::AiShorten => &["AiShorten"],
        Key::ShowVersions => &["ShowVersions"],
        Key::QuotePicker | Key::Picker => &["QuotePicker"],
        Key::DiscardChanges => &["DiscardChanges"],
        Key::OwnPostAction => &["ReplyToPost", "QuotePost", "LinkToPost"],
        Key::ReaderMode => &["ReaderMode"],
        Key::Notes => &["ToggleNotes"],
        Key::Lineage | Key::LineageStrip => &["ShowLineage"],
        Key::NewPost | Key::Serendipity => &["NewDraft"],
        Key::Clip => &["ClipPage"],
        Key::ExtPalette | Key::ExtManage => &["ShowExtensions"],
        Key::OpmlImport => &["ImportOpml"],
        Key::Compose => &["FocusCompose"],
        Key::BrowseHistory => &["ToggleBrowseHistory"],
        Key::StubPassage => &["QuoteToDraft"],
        Key::BlygSwitcher => &["ShowBlygSwitcher"],
        Key::HopperPicker | Key::HopperSeveral => &["HopperPicker"],
        Key::Shelf | Key::ShelfReading => &["ToggleShelf"],
        Key::Palette => &["ShowPalette"],
        Key::Focus => &["ToggleFocus"],
        Key::Trail => &["PublishTrail"],
        Key::SiteSheet => &["ShowSite"],
        Key::ContactsSheet => &["ShowContacts"],
        Key::StaleQuotes => &["ShowStaleQuotes"],
        Key::ClientAccess => &["ShowClientAccess"],
        Key::Tray => &["AddToTray"],
        Key::Split => &["OpenBeside"],
        Key::CatchUp => &["CatchUp"],
        Key::Discover => &["ShowDiscover"],
        Key::Nearby => &["ToggleNearby"],
        Key::QuietWriting => &["ToggleQuietWriting"],
        // --- 0.27.0 --- ⌘⏎ (then ⏎) reaches the confirm step.
        Key::Tangle => &["Publish"],
        Key::Formula => &["InsertFormula"],
        // ⇧⌘B shows or hides the pane; the step waits for it to close.
        Key::CloseBrowser => &["ToggleBrowser"],
        Key::LineageMap | Key::LineageRing => &["ShowLineage"],
        // Seen on screen or typed, not an action of the keymap's.
        Key::MoveSelection
        | Key::Enter
        | Key::Typed
        | Key::ReadMore
        | Key::OpenOriginal
        | Key::OpenProfile
        | Key::Mention
        | Key::SpellMenu
        | Key::ConsentAnswered
        | Key::SlotSheet
        | Key::NotesVault
        | Key::MacroFolded
        | Key::FollowThicket
        | Key::HoverCard
        | Key::Background
        | Key::ShelfLens
        | Key::MentionsFeed
        | Key::MoreMenu
        | Key::Parts // `p`: reading_key_down
        | Key::ConflictAnswered => &[],
    }
}

/// Keys captions may quote that aren't rows of the keymap: gpui-base's own,
/// a screen's single keys (`reading_key_down`), a sheet's, and clicks.
const NOT_IN_THE_KEYMAP: &[&str] = &[
    "cmd-a",         // select all (gpui-base); picks all in the Reader
    "cmd-c",         // copy (gpui-base)
    "cmd-backspace", // a sheet's "never" (the consent sheet)
    "shift-c",       // Catch up in Reading (reading_key_down)
    "shift-t",       // another trail in Reading
    "shift-r",       // mark all read in Reading
];

fn texts(s: &Step) -> [(&'static str, Text); 3] {
    [
        ("title", s.title),
        ("caption", s.caption),
        ("key_label", s.key_label),
    ]
}

fn rows(p: Platform) -> Vec<Keybind> {
    keymap::table_for(p)
        .into_iter()
        .filter(|k| !k.key.is_empty())
        .collect()
}

/// Every key a step quotes is a key the app binds, on each platform.
#[test]
fn every_key_a_step_quotes_is_bound() {
    let mut bad = Vec::new();
    for p in [Platform::Mac, Platform::Windows] {
        let rows = rows(p);
        for s in STEPS {
            for (field, t) in texts(s) {
                for key in keymap::hint_keys(&t.on(p), p) {
                    let n = keymap::normalize(&key);
                    let bound = rows.iter().any(|k| keymap::normalize(k.key) == n);
                    let known = NOT_IN_THE_KEYMAP
                        .iter()
                        .any(|k| keymap::normalize(&keymap::respell_for(k, p)) == n);
                    if !bound && !known {
                        bad.push(format!("  {p:?} {}.{field}: {key}", s.id));
                    }
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "tour steps quote keys nothing is bound to:\n{}\nFix the caption, or the binding \
         (keymap::mac_table, WINDOWS_OVERRIDES); a key that isn't a keymap row goes in \
         NOT_IN_THE_KEYMAP with where it's handled.",
        bad.join("\n")
    );
}

/// "Press ⌘X" runs what the step waits for: the key label is bound to the
/// action of one of the step's keys.
#[test]
fn a_steps_key_label_is_bound_to_what_it_waits_for() {
    let actions: Vec<&str> = keymap::mac_table().iter().map(|k| k.action).collect();
    for p in [Platform::Mac, Platform::Windows] {
        let rows = rows(p);
        for s in STEPS {
            let wanted: Vec<&str> = s
                .keys
                .iter()
                .flat_map(|k| key_actions(*k))
                .copied()
                .collect();
            for a in &wanted {
                assert!(actions.contains(a), "{}: no keymap action {a}", s.id);
            }
            for key in keymap::hint_keys(&s.key_label.on(p), p) {
                let n = keymap::normalize(&key);
                let bound: Vec<&str> = rows
                    .iter()
                    .filter(|k| keymap::normalize(k.key) == n)
                    .map(|k| k.action)
                    .collect();
                if bound.is_empty() {
                    continue; // every_key_a_step_quotes_is_bound says so
                }
                assert!(
                    bound.iter().any(|a| wanted.contains(a)),
                    "{p:?} {}: the card says \"Press {}\", which runs {bound:?}, but the step \
                     waits for {:?} ({wanted:?}). Fix the key label, or key_actions.",
                    s.id,
                    s.key_label.on(p),
                    s.keys
                );
            }
        }
    }
}

/// The menu items steps name (`Post › Discard Changes…`) are in the menus:
/// the keymap's `menu` paths, which `main::menu_tests` holds to the menus.
#[test]
fn every_menu_item_a_step_names_is_in_the_menus() {
    let paths: Vec<&str> = keymap::mac_table().iter().filter_map(|k| k.menu).collect();
    let mut tops: Vec<&str> = paths.iter().filter_map(|m| m.split(" › ").next()).collect();
    tops.sort();
    tops.dedup();
    let mut bad = Vec::new();
    for s in STEPS {
        for (field, t) in texts(s) {
            for p in [Platform::Mac, Platform::Windows] {
                let text = t.on(p);
                for top in &tops {
                    let head = format!("{top} › ");
                    for (at, _) in text.match_indices(&head) {
                        let rest = &text[at..];
                        if !paths.iter().any(|m| rest.starts_with(m)) {
                            let shown: String = rest.chars().take(40).collect();
                            bad.push(format!("  {}.{field}: {shown}…", s.id));
                        }
                    }
                }
            }
        }
    }
    bad.dedup();
    assert!(
        bad.is_empty(),
        "tour steps name menu items the menus don't have:\n{}\nName the item as the \
         menu spells it (keymap::mac_table's `menu`).",
        bad.join("\n")
    );
}

/// Quoted labels (“Reply”) are text the app shows: a literal outside the
/// tour, so a renamed label fails here instead of in a user's hands.
#[test]
fn every_label_a_step_quotes_is_shown_by_the_app() {
    use crate::platform_text::scan;
    let shown: Vec<String> = scan::crate_sources()
        .into_iter()
        .filter(|(path, _)| !path.starts_with("onboarding/"))
        .flat_map(|(_, src)| scan::literals(&src))
        .map(|l| {
            l.text
                .replace("\\\n", "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    // What the user types or the samples say, not the app's own words.
    let not_labels: Vec<&str> = STEPS
        .iter()
        .filter_map(|s| match s.setup {
            super::steps::Setup::Query(q) => Some(q),
            _ => None,
        })
        .chain(QUOTED_NOT_LABELS.iter().copied())
        .collect();
    let mut bad = Vec::new();
    for s in STEPS {
        for (field, t) in texts(s) {
            for p in [Platform::Mac, Platform::Windows] {
                let text = t.on(p);
                for q in text.split('“').skip(1).filter_map(|r| r.split('”').next()) {
                    // "Also posted at …": the label's fixed parts, around
                    // what the app fills in.
                    let parts: Vec<&str> = q
                        .split('…')
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                        .collect();
                    let found = |part: &&str| shown.iter().any(|l| l.contains(*part));
                    if not_labels.contains(&q) || (!parts.is_empty() && parts.iter().all(found)) {
                        continue;
                    }
                    bad.push(format!("  {}.{field}: “{q}”", s.id));
                }
            }
        }
    }
    bad.dedup();
    assert!(
        bad.is_empty(),
        "tour steps quote labels the app doesn't show:\n{}\nQuote the label as the UI \
         spells it, or (for sample text or what a user types) list it in QUOTED_NOT_LABELS.",
        bad.join("\n")
    );
}

/// Quoted text in captions that is sample data or typing, not a label.
const QUOTED_NOT_LABELS: &[&str] = &[
    "V2: …",    // a sample post's own first line
    "· 3 min",  // the sample extension's reading-time slot
    "1 · 2",    // a lineage glyph's counts, drawn from numbers
    "R · 2",    // the ring's reply count, likewise
    "+2 parts", // a thread's parts, counted (`parts_label`)
];
