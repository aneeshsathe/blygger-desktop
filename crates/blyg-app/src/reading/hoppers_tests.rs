//! --- hoppers --- Headless GPUI tests of hoppers in the Reader: the
//! sources pane's section, picking a hopper, Hopper › on a post (keys, the
//! row menu, the chip), New Hopper… with the post in it, and a hopper's
//! menu (rename, description, public, delete), on a zero-latency
//! FakeBackend.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::RSheet;
use super::hoppers::{HopperPicker, NewHopper};
use super::sources::{MenuCmd, MenuTarget};
use super::sources_vm::{Entry, Pane, Smart, Source};
use crate::app::MainView;
use crate::fake::reading_seed::*;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup(cx: &mut TestAppContext) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
    let prefs = Prefs::from_config(ConfigStore::in_memory(CONNECTED).config());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(CONNECTED),
            Arc::new(MemoryTokenStore::default()),
            None,
            cx,
        );
    });
    let fake = Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache());
    let backend: Arc<dyn Backend> = fake.clone();
    let f2 = fake.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(
            backend,
            Some(f2),
            prefs,
            std::time::Instant::now(),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    view.update_in(cx, |v, window, cx| {
        v.set_read_mode(super::stream_vm::ReadMode::Reader, window, cx)
    });
    settle(cx);
    (view, fake, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..20 {
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn shown(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |v, _| v.reading_shown_ids())
}

fn select(view: &Entity<MainView>, s: Source, cx: &mut VisualTestContext) {
    view.update(cx, |v, cx| v.select_source(s, cx));
    settle(cx);
}

fn menu(view: &Entity<MainView>, cmd: MenuCmd, cx: &mut VisualTestContext) {
    view.update_in(cx, |v, window, cx| v.run_menu_cmd(cmd, window, cx));
    settle(cx);
}

fn open(view: &Entity<MainView>, key: (&str, &str), cx: &mut VisualTestContext) {
    view.update_in(cx, |v, window, cx| {
        v.open_reading((key.0.to_string(), key.1.to_string()), window, cx)
    });
    settle(cx);
}

fn members_of(fake: &FakeBackend, sub: &str, rid: &str) -> Vec<String> {
    fake.hopper_members()
        .get(&(sub.to_string(), rid.to_string()))
        .cloned()
        .unwrap_or_default()
}

fn hopper(fake: &FakeBackend, id: &str) -> Option<blyg_core::Hopper> {
    fake.hoppers().into_iter().find(|h| h.id == id)
}

fn sheet_kind(view: &Entity<MainView>, cx: &mut VisualTestContext) -> &'static str {
    view.read_with(cx, |v, _| match &v.reading.sheet {
        Some(RSheet::HopperPick { .. }) => "pick",
        Some(RSheet::HopperName { .. }) => "name",
        Some(RSheet::HopperConfirm { .. }) => "confirm",
        Some(_) => "other",
        None => "none",
    })
}

#[gpui_kit::test]
fn the_sources_pane_lists_hoppers_under_background(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    let lines: Vec<String> = view.read_with(cx, |v, _| {
        v.source_entries()
            .into_iter()
            .map(|e| match e {
                Entry::Heading(h) => format!("# {h}"),
                Entry::Smart { smart, .. } => smart.label().into(),
                Entry::Own => "Your blyg".into(),
                Entry::Mentions { .. } => "Mentions".into(), // --- contacts ---
                Entry::Hopper(h) => format!(
                    "▤ {} {}{}",
                    h.name,
                    h.count,
                    if h.public { " public" } else { "" }
                ),
                Entry::Folder { folder, .. } => format!("[{}]", folder.name),
                Entry::Sub { sub, .. } => sub.title,
            })
            .collect()
    });
    let at = lines
        .iter()
        .position(|l| l == "# Hoppers")
        .expect("a Hoppers section");
    assert_eq!(
        &lines[at - 2..at + 4],
        [
            "Background",
            "Your blyg",
            "# Hoppers",
            "▤ Slow media 3 public",
            "▤ Garden reading 1",
            "# Folders",
        ]
    );
}

#[gpui_kit::test]
fn picking_a_hopper_lists_its_posts_yours_included(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    select(&view, Source::Hopper(HOPPER_SLOW.into()), cx);
    let mut ids = shown(&view, cx);
    ids.sort();
    let mut want = vec![
        ADA_FINISHED.to_string(),
        RUE_TRUST.to_string(),
        OWN_GARDENS_SERVER.to_string(),
    ];
    want.sort();
    assert_eq!(ids, want, "theirs and yours");
    view.read_with(cx, |v, _| {
        assert_eq!(
            super::sources_vm::source_label(
                &v.reading.source,
                &v.reading.folders,
                &v.reading.subs,
                &v.reading.hoppers
            ),
            "Slow media"
        )
    });
    select(&view, Source::Hopper(HOPPER_GARDENS.into()), cx);
    assert_eq!(shown(&view, cx), [LIN_GARDENS]);
}

#[gpui_kit::test]
fn arrows_in_the_sources_pane_step_through_the_hoppers(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    select(&view, Source::Smart(Smart::Background), cx);
    view.update(cx, |v, cx| v.set_pane(Pane::Sources, cx));
    cx.simulate_keystrokes("down");
    settle(cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.reading.source.clone()),
        Source::Own
    );
    cx.simulate_keystrokes("down");
    settle(cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.reading.source.clone()),
        Source::Hopper(HOPPER_SLOW.into())
    );
    assert_eq!(shown(&view, cx).len(), 3, "it picks as it moves");
    cx.simulate_keystrokes("down down");
    settle(cx);
    assert!(
        matches!(
            view.read_with(cx, |v, _| v.reading.source.clone()),
            Source::Folder(_)
        ),
        "on into the folders"
    );
}

#[gpui_kit::test]
fn h_opens_the_picker_and_numbers_put_the_post_in_and_out(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open(&view, ("sub-omar", OMAR_YEAR), cx);
    view.update_in(cx, |v, window, cx| window.focus(&v.reading.focus, cx));
    cx.simulate_keystrokes("h");
    settle(cx);
    assert_eq!(sheet_kind(&view, cx), "pick");
    let before = hopper(&fake, HOPPER_GARDENS).unwrap().count;
    cx.simulate_keystrokes("2");
    settle(cx);
    assert_eq!(members_of(&fake, "sub-omar", OMAR_YEAR), [HOPPER_GARDENS]);
    assert_eq!(hopper(&fake, HOPPER_GARDENS).unwrap().count, before + 1);
    assert_eq!(sheet_kind(&view, cx), "pick", "it stays open for more");
    // The list follows: the post is under Garden reading now.
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(sheet_kind(&view, cx), "none");
    select(&view, Source::Hopper(HOPPER_GARDENS.into()), cx);
    assert!(shown(&view, cx).contains(&OMAR_YEAR.to_string()));
    // Out again from the arrows and ⏎.
    view.update_in(cx, |v, window, cx| v.open_hopper_picker(window, cx));
    settle(cx);
    cx.simulate_keystrokes("down enter");
    settle(cx);
    assert!(members_of(&fake, "sub-omar", OMAR_YEAR).is_empty());
    assert_eq!(hopper(&fake, HOPPER_GARDENS).unwrap().count, before);
    assert!(
        fake.reading_sends()
            .iter()
            .any(|s| s == &format!("hopper:{HOPPER_GARDENS}:sub-omar:{OMAR_YEAR}:false"))
    );
}

#[gpui_kit::test]
fn new_hopper_from_the_picker_makes_it_with_the_post_in_it(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open(&view, ("sub-ada", ADA_TIDES), cx);
    cx.dispatch_action(HopperPicker);
    settle(cx);
    assert_eq!(sheet_kind(&view, cx), "pick");
    cx.simulate_keystrokes("n");
    settle(cx);
    assert_eq!(sheet_kind(&view, cx), "name");
    cx.simulate_input("Long reads");
    cx.simulate_keystrokes("enter");
    settle(cx);
    assert_eq!(sheet_kind(&view, cx), "none");
    let made = fake
        .hoppers()
        .into_iter()
        .find(|h| h.name == "Long reads")
        .expect("made");
    assert!(!made.public, "private until made public");
    assert_eq!(made.count, 1);
    assert_eq!(
        members_of(&fake, "sub-ada", ADA_TIDES),
        std::slice::from_ref(&made.id)
    );
    view.read_with(cx, |v, _| {
        assert!(v.reading.hoppers.iter().any(|h| h.id == made.id))
    });
}

#[gpui_kit::test]
fn the_row_menu_and_the_chip_offer_hopper(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let key = ("sub-rue".to_string(), RUE_TRUST.to_string());
    view.update(cx, |v, cx| {
        v.open_source_menu(
            MenuTarget::Posts(key.clone()),
            gpui_kit::point(gpui_kit::px(10.), gpui_kit::px(10.)),
            false,
            cx,
        )
    });
    let items = view.read_with(cx, |v, _| v.menu_items());
    assert!(
        items
            .iter()
            .any(|(l, c, _)| l == "Hopper ›" && *c == Some(MenuCmd::HopperMenu))
    );
    menu(&view, MenuCmd::HopperMenu, cx);
    let items = view.read_with(cx, |v, _| v.menu_items());
    let labels: Vec<(String, bool)> = items
        .iter()
        .filter(|(_, c, _)| c.is_some())
        .map(|(l, _, on)| (l.clone(), *on))
        .collect();
    assert_eq!(
        labels,
        [
            ("Slow media".to_string(), true),
            ("Garden reading".to_string(), false),
            ("New Hopper…".to_string(), false),
        ]
    );
    menu(
        &view,
        MenuCmd::ToggleHopper(vec![key.clone()], HOPPER_SLOW.into()),
        cx,
    );
    assert!(members_of(&fake, "sub-rue", RUE_TRUST).is_empty());

    // The reading pane's chip.
    open(&view, ("sub-rue", RUE_TRUST), cx);
    let chips: Vec<String> = view.read_with(cx, |v, _| {
        v.reading_action_chips()
            .into_iter()
            .map(|c| c.label)
            .collect()
    });
    assert!(chips.contains(&"Hopper ›".to_string()), "{chips:?}");
    view.update_in(cx, |v, window, cx| {
        v.reading_action_for_test("Hopper", window, cx)
    });
    settle(cx);
    assert_eq!(sheet_kind(&view, cx), "pick");
}

#[gpui_kit::test]
fn your_own_post_goes_in_as_own(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open(&view, ("~own", OWN_GARDENS_SERVER), cx);
    view.update_in(cx, |v, window, cx| v.open_hopper_picker(window, cx));
    settle(cx);
    cx.simulate_keystrokes("2");
    settle(cx);
    assert_eq!(
        members_of(&fake, "~own", OWN_GARDENS_SERVER),
        [HOPPER_GARDENS, HOPPER_SLOW]
    );
    select(&view, Source::Hopper(HOPPER_GARDENS.into()), cx);
    assert!(shown(&view, cx).contains(&OWN_GARDENS_SERVER.to_string()));
    // Still never unread, never counted.
    view.read_with(cx, |v, _| {
        assert!(
            v.source_entries()
                .iter()
                .any(|e| matches!(e, Entry::Hopper(h) if h.id == HOPPER_GARDENS && h.count == 2))
        );
    });
}

#[gpui_kit::test]
fn a_hoppers_menu_renames_describes_publishes_and_deletes(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    view.update(cx, |v, cx| {
        v.open_source_menu(
            MenuTarget::Hopper(HOPPER_GARDENS.into()),
            gpui_kit::point(gpui_kit::px(10.), gpui_kit::px(10.)),
            false,
            cx,
        )
    });
    let labels: Vec<String> = view.read_with(cx, |v, _| {
        v.menu_items()
            .into_iter()
            .filter(|(_, c, _)| c.is_some())
            .map(|(l, _, _)| l)
            .collect()
    });
    assert_eq!(
        labels,
        [
            "Rename…",
            "Description…",
            "Make Public…",
            "Delete Hopper…",
            "New Hopper…"
        ]
    );

    // Rename…
    menu(&view, MenuCmd::RenameHopper(HOPPER_GARDENS.into()), cx);
    assert_eq!(sheet_kind(&view, cx), "name");
    cx.simulate_input("Allotment reading");
    cx.simulate_keystrokes("enter");
    settle(cx);
    assert_eq!(
        hopper(&fake, HOPPER_GARDENS).unwrap().name,
        "Allotment reading"
    );

    // Description…
    menu(&view, MenuCmd::DescribeHopper(HOPPER_GARDENS.into()), cx);
    cx.simulate_input("Soil, seeds and patience.");
    cx.simulate_keystrokes("enter");
    settle(cx);
    assert_eq!(
        hopper(&fake, HOPPER_GARDENS)
            .unwrap()
            .description
            .as_deref(),
        Some("Soil, seeds and patience.")
    );

    // Make Public… asks first; esc leaves it private.
    menu(
        &view,
        MenuCmd::SetHopperPublic(HOPPER_GARDENS.into(), true),
        cx,
    );
    assert_eq!(sheet_kind(&view, cx), "confirm");
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(!hopper(&fake, HOPPER_GARDENS).unwrap().public);
    menu(
        &view,
        MenuCmd::SetHopperPublic(HOPPER_GARDENS.into(), true),
        cx,
    );
    cx.simulate_keystrokes("enter");
    settle(cx);
    let h = hopper(&fake, HOPPER_GARDENS).unwrap();
    assert!(h.public && h.slug_frozen);
    // Make Private doesn't ask.
    menu(
        &view,
        MenuCmd::SetHopperPublic(HOPPER_GARDENS.into(), false),
        cx,
    );
    assert_eq!(sheet_kind(&view, cx), "none");
    assert!(!hopper(&fake, HOPPER_GARDENS).unwrap().public);

    // Delete… asks; the posts stay, and a selected hopper lets go.
    select(&view, Source::Hopper(HOPPER_GARDENS.into()), cx);
    let held = fake.reading().len();
    menu(&view, MenuCmd::DeleteHopper(HOPPER_GARDENS.into()), cx);
    assert_eq!(sheet_kind(&view, cx), "confirm");
    cx.simulate_keystrokes("enter");
    settle(cx);
    assert!(hopper(&fake, HOPPER_GARDENS).is_none());
    assert_eq!(fake.reading().len(), held, "the posts stay");
    view.read_with(cx, |v, _| {
        assert_eq!(v.reading.source, Source::Smart(Smart::All));
        assert!(!v.reading.hoppers.iter().any(|h| h.id == HOPPER_GARDENS));
    });
}

#[gpui_kit::test]
fn offline_a_new_hopper_waits_but_membership_does_not(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    fake.set_offline(true);
    cx.dispatch_action(NewHopper);
    settle(cx);
    cx.simulate_input("Later");
    cx.simulate_keystrokes("enter");
    settle(cx);
    view.read_with(cx, |v, _| match &v.reading.sheet {
        Some(RSheet::HopperName { error: Some(e), .. }) => {
            assert!(e.contains("can't be reached"), "{e}")
        }
        _ => panic!("the sheet stays, with why"),
    });
    assert!(!fake.hoppers().iter().any(|h| h.name == "Later"));
    cx.simulate_keystrokes("escape");
    settle(cx);
    // Putting a post in a hopper is local, and queued.
    view.update(cx, |v, cx| {
        v.toggle_hopper(
            &[("sub-ada".to_string(), ADA_TIDES.to_string())],
            HOPPER_SLOW,
            cx,
        )
    });
    settle(cx);
    assert_eq!(members_of(&fake, "sub-ada", ADA_TIDES), [HOPPER_SLOW]);
}

#[gpui_kit::test]
fn an_empty_name_is_refused_in_the_sheet(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let n = fake.hoppers().len();
    cx.dispatch_action(NewHopper);
    settle(cx);
    cx.simulate_keystrokes("enter");
    settle(cx);
    view.read_with(cx, |v, _| {
        assert!(matches!(
            &v.reading.sheet,
            Some(RSheet::HopperName { error: Some(_), .. })
        ))
    });
    assert_eq!(fake.hoppers().len(), n);
}

// --- hoppers --- the hopper's own order; several posts at once.

#[gpui_kit::test]
fn a_hopper_lists_its_posts_newest_added_first(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    select(&view, Source::Hopper(HOPPER_SLOW.into()), cx);
    // The seed put them in as Ada's, Rue's, then yours.
    assert_eq!(
        shown(&view, cx),
        [OWN_GARDENS_SERVER, RUE_TRUST, ADA_FINISHED],
        "the hopper's order, not the posts' dates"
    );
    // One put in now goes on top.
    view.update(cx, |v, cx| {
        v.toggle_hopper(
            &[("sub-omar".to_string(), OMAR_YEAR.to_string())],
            HOPPER_SLOW,
            cx,
        )
    });
    settle(cx);
    assert_eq!(
        shown(&view, cx),
        [OMAR_YEAR, OWN_GARDENS_SERVER, RUE_TRUST, ADA_FINISHED]
    );
}

#[gpui_kit::test]
fn the_picker_acts_on_the_picked_posts(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let omar = ("sub-omar".to_string(), OMAR_YEAR.to_string());
    let ada = ("sub-ada".to_string(), ADA_TIDES.to_string());
    open(&view, ("sub-omar", OMAR_YEAR), cx);
    // ⌘-click: the open post and Ada's are picked.
    view.update(cx, |v, cx| {
        let m = gpui_kit::Modifiers {
            platform: true,
            ..Default::default()
        };
        assert!(v.pick_click(&ada, m, cx));
    });
    settle(cx);
    view.update_in(cx, |v, window, cx| v.open_hopper_picker(window, cx));
    settle(cx);
    view.read_with(cx, |v, _| match &v.reading.sheet {
        Some(RSheet::HopperPick { keys, title, .. }) => {
            assert_eq!(keys.len(), 2, "{keys:?}");
            assert!(keys.contains(&omar) && keys.contains(&ada));
            assert_eq!(title, "2 posts");
        }
        _ => panic!("the picker"),
    });
    // In together…
    cx.simulate_keystrokes("2");
    settle(cx);
    assert_eq!(members_of(&fake, "sub-omar", OMAR_YEAR), [HOPPER_GARDENS]);
    assert_eq!(members_of(&fake, "sub-ada", ADA_TIDES), [HOPPER_GARDENS]);
    // …and out together.
    cx.simulate_keystrokes("2");
    settle(cx);
    assert!(members_of(&fake, "sub-omar", OMAR_YEAR).is_empty());
    assert!(members_of(&fake, "sub-ada", ADA_TIDES).is_empty());
    // Some of them in: a dash, and choosing it puts the rest in.
    view.update(cx, |v, cx| {
        v.toggle_hopper(std::slice::from_ref(&omar), HOPPER_GARDENS, cx)
    });
    let both = [omar.clone(), ada.clone()];
    assert_eq!(
        super::hoppers::held_by(&fake.hopper_members(), &both, HOPPER_GARDENS),
        super::hoppers::Held::Some
    );
    cx.simulate_keystrokes("2");
    settle(cx);
    assert_eq!(
        super::hoppers::held_by(&fake.hopper_members(), &both, HOPPER_GARDENS),
        super::hoppers::Held::All
    );
    cx.simulate_keystrokes("escape");
    settle(cx);

    // A picked row's menu acts on the picks too.
    view.update(cx, |v, cx| {
        v.open_post_menu(
            ada.clone(),
            gpui_kit::point(gpui_kit::px(10.), gpui_kit::px(10.)),
            cx,
        )
    });
    menu(&view, MenuCmd::HopperMenu, cx);
    let items = view.read_with(cx, |v, _| v.menu_items());
    let garden = items
        .iter()
        .find(|(l, _, _)| l == "Garden reading")
        .cloned()
        .expect("listed");
    assert!(garden.2, "checked: both are in it");
    match &garden.1 {
        Some(MenuCmd::ToggleHopper(keys, id)) => {
            assert_eq!(id, HOPPER_GARDENS);
            assert_eq!(keys.len(), 2);
        }
        c => panic!("{c:?}"),
    }
    menu(&view, garden.1.clone().unwrap(), cx);
    assert!(members_of(&fake, "sub-omar", OMAR_YEAR).is_empty());
    assert!(members_of(&fake, "sub-ada", ADA_TIDES).is_empty());

    // New Hopper… from the picks makes it with both in it.
    let new = items
        .iter()
        .find(|(l, _, _)| l == "New Hopper…")
        .and_then(|(_, c, _)| c.clone())
        .expect("New Hopper…");
    menu(&view, new, cx);
    cx.simulate_input("Pairs");
    cx.simulate_keystrokes("enter");
    settle(cx);
    let made = fake
        .hoppers()
        .into_iter()
        .find(|h| h.name == "Pairs")
        .expect("made");
    assert_eq!(made.count, 2);
}
