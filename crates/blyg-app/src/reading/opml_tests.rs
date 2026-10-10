//! The OPML import sheet: ticks, already-following, the run's summary and
//! filing, retry and cancel, through real keystrokes on a zero-latency
//! FakeBackend. (What the server answers is the e2e's job, against a real
//! local studio.)

use std::sync::Arc;
use std::time::Duration;

use blyg_core::config::MemoryTokenStore;
use blyg_core::opml::{self, FailKind, IMPORT_FOLDER, Outcome, Pace};
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::RSheet;
use super::opml::{ImportPace, Phase, Sheet};
use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

/// One already followed (Rue's blyg, by its origin), one that can't be
/// resolved, two new feeds in a folder, and a duplicate.
const FILE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0"><head><title>Export</title></head><body>
  <outline text="Rue" xmlUrl="https://rue.blyg.example.com/feed.xml" htmlUrl="https://rue.blyg.example.com/"/>
  <outline text="Dead" xmlUrl="https://dead.invalid/rss"/>
  <outline text="Tech">
    <outline type="rss" text="Kit" xmlUrl="https://kit.example.org/feed.xml"/>
    <outline type="rss" text="Moss" xmlUrl="https://moss.example.net/rss.xml"/>
    <outline type="rss" text="Kit again" xmlUrl="https://kit.example.org/feed.xml"/>
  </outline>
</body></opml>"#;

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
        cx.set_global(ImportPace(Pace {
            // One at a time runs on the task's own thread: the test
            // scheduler refuses wakes from threads it doesn't know.
            concurrency: 1,
            interval: Duration::ZERO,
            max_waits: 1,
        }));
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
    (view, fake, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..30 {
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(3));
    }
}

fn open(view: &Entity<MainView>, cx: &mut VisualTestContext) {
    let parsed = opml::parse(FILE).unwrap();
    view.update_in(cx, |v, window, cx| {
        v.opml_open_preview("export.opml".into(), parsed, window, cx)
    });
    cx.run_until_parked();
}

fn sheet<R>(view: &Entity<MainView>, cx: &mut VisualTestContext, f: impl FnOnce(&Sheet) -> R) -> R {
    view.read_with(cx, |v, _| match &v.reading.sheet {
        Some(RSheet::Opml(s)) => f(s),
        _ => panic!("the OPML sheet isn't open"),
    })
}

fn ticks(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Vec<bool> {
    sheet(view, cx, |s| s.rows.iter().map(|r| r.ticked).collect())
}

#[gpui_kit::test]
fn preview_marks_already_followed_and_ticks_with_keys(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    open(&view, cx);
    let (titles, already, folders, dups) = sheet(&view, cx, |s| {
        (
            s.rows
                .iter()
                .map(|r| r.feed.title.clone())
                .collect::<Vec<_>>(),
            s.rows.iter().map(|r| r.already.clone()).collect::<Vec<_>>(),
            s.rows
                .iter()
                .map(|r| r.feed.folder.clone())
                .collect::<Vec<_>>(),
            s.duplicates,
        )
    });
    assert_eq!(titles, ["Rue", "Dead", "Kit", "Moss"]);
    assert_eq!(
        already[0].as_deref(),
        Some("Rue"),
        "followed by its blyg's origin"
    );
    assert!(already[1..].iter().all(Option::is_none));
    assert_eq!(folders[2].as_deref(), Some("Tech"));
    assert_eq!(dups, 1);
    assert_eq!(
        ticks(&view, cx),
        [false, true, true, true],
        "already followed starts unticked"
    );

    cx.simulate_keystrokes("3");
    assert_eq!(ticks(&view, cx), [false, true, false, true]);
    cx.simulate_keystrokes("3");
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-a"));
    assert_eq!(ticks(&view, cx), [false, false, false, false], "⌘A: none");
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-a"));
    assert_eq!(
        ticks(&view, cx),
        [false, true, true, true],
        "⌘A: all not already followed"
    );
    cx.simulate_keystrokes("up up up space");
    assert!(ticks(&view, cx)[0], "space ticks the selected row");

    // esc cancels: nothing subscribed.
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |v, _| assert!(v.reading.sheet.is_none()));
}

#[gpui_kit::test]
fn import_files_new_ones_and_leaves_followed_ones_where_they_are(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let mine = fake.create_folder("Mine").unwrap();
    fake.set_subscription_folder("sub-rue", Some(&mine.id))
        .unwrap();
    let before = fake.subscriptions().len();
    open(&view, cx);
    // Tick Rue too: the server's 409 makes it "already following".
    cx.simulate_keystrokes("1");
    cx.simulate_keystrokes("enter");
    settle(cx);

    let (phase, outcomes) = sheet(&view, cx, |s| {
        (
            s.phase.clone(),
            s.rows.iter().map(|r| r.outcome.clone()).collect::<Vec<_>>(),
        )
    });
    let Phase::Done(sum) = phase else {
        panic!("finished: {phase:?}")
    };
    assert_eq!(sum.line(), "2 added, 1 already followed, 1 failed");
    assert_eq!(outcomes[0], Some(Outcome::AlreadyFollowing));
    assert!(matches!(
        &outcomes[1],
        Some(Outcome::Failed {
            kind: FailKind::NotAFeed,
            ..
        })
    ));
    assert!(matches!(outcomes[2], Some(Outcome::Added { .. })));
    assert_eq!(fake.subscriptions().len(), before + 2);

    // The new ones are in "Imported feeds"; Rue stays in Mine.
    let folder = fake
        .folders()
        .into_iter()
        .find(|f| f.name == IMPORT_FOLDER)
        .expect("the import folder");
    let filed = fake.subscription_folders();
    for o in &outcomes[2..] {
        let Some(Outcome::Added { id, .. }) = o else {
            panic!("added")
        };
        assert_eq!(filed.get(id), Some(&folder.id));
    }
    assert_eq!(filed.get("sub-rue"), Some(&mine.id));
    assert!(super::opml::WHERE_THEY_GO.contains(IMPORT_FOLDER));
    view.read_with(cx, |v, _| {
        assert!(
            v.reading.folders.iter().any(|f| f.name == IMPORT_FOLDER),
            "the Reader sees it"
        );
    });

    // Retry failed runs only the failed one, and reuses the folder.
    cx.simulate_keystrokes("r");
    settle(cx);
    let phase = sheet(&view, cx, |s| s.phase.clone());
    let Phase::Done(sum) = phase else {
        panic!("finished again")
    };
    assert_eq!(sum.line(), "0 added, 0 already followed, 1 failed");
    assert_eq!(
        fake.folders()
            .iter()
            .filter(|f| f.name == IMPORT_FOLDER)
            .count(),
        1
    );
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |v, _| assert!(v.reading.sheet.is_none()));
}

#[gpui_kit::test]
fn cancel_before_anything_starts_tries_nothing(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let before = fake.subscriptions().len();
    open(&view, cx);
    view.update(cx, |v, cx| {
        v.opml_start(None, cx);
        v.opml_cancel(cx);
    });
    settle(cx);
    let phase = sheet(&view, cx, |s| s.phase.clone());
    let Phase::Done(sum) = phase else {
        panic!("finished: {phase:?}")
    };
    assert_eq!(sum.skipped, 3);
    assert_eq!(fake.subscriptions().len(), before);
}
