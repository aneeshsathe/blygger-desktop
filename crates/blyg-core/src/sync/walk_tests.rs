//! The reading list's read decisions (sync/walk.rs), pure.

use super::*;
use crate::model::{SubscriptionKind, SubscriptionPoll};

fn due() -> Due {
    Due {
        gap_open: true,
        has_mark: true,
        ..Due::default()
    }
}

#[test]
fn nothing_moved_reads_nothing() {
    assert_eq!(start(due()), Start::Nothing);
}

#[test]
fn a_moved_counter_reads_the_head() {
    assert_eq!(
        start(Due {
            moved: true,
            ..due()
        }),
        Start::Head
    );
}

#[test]
fn the_head_waits_out_the_gap() {
    let d = Due {
        moved: true,
        gap_open: false,
        ..due()
    };
    assert_eq!(start(d), Start::Nothing);
}

#[test]
fn without_a_mark_the_head_is_a_whole_walk() {
    let d = Due {
        moved: true,
        has_mark: false,
        ..due()
    };
    assert_eq!(start(d), Start::Whole);
}

#[test]
fn the_safety_net_a_real_subscription_change_and_a_deep_move_walk_whole() {
    for d in [
        Due {
            full: true,
            ..due()
        },
        Due {
            subs_changed: true,
            gap_open: false,
            ..due()
        },
        Due {
            deep_due: true,
            gap_open: false,
            ..due()
        },
    ] {
        assert_eq!(start(d), Start::Whole, "{d:?}");
    }
}

#[test]
fn a_walk_under_way_goes_on_whatever_else_is_due() {
    let d = Due {
        in_progress: true,
        full: true,
        moved: true,
        gap_open: false,
        ..due()
    };
    assert_eq!(start(d), Start::Continue);
}

const HOUR: i64 = 60 * 60 * 1000;

#[test]
fn the_end_of_the_list_completes_any_read() {
    for whole in [true, false] {
        for unchanged in [true, false] {
            assert_eq!(
                after_page(whole, false, unchanged, Some(0), Some(10 * HOUR), 1, 20),
                Next::Complete
            );
            // Even on the last page of the budget.
            assert_eq!(
                after_page(whole, false, unchanged, None, None, 20, 20),
                Next::Complete
            );
        }
    }
}

#[test]
fn a_head_read_stops_at_the_first_page_that_changed_nothing() {
    let b = boundary(Some(10 * HOUR));
    // A backfill: every row stamped the same, well inside the slack.
    let at = Some(10 * HOUR);
    assert_eq!(after_page(false, true, false, at, b, 1, 20), Next::Page);
    assert_eq!(after_page(false, true, true, at, b, 2, 20), Next::HeadDone);
}

#[test]
fn a_head_read_stops_once_past_the_boundary() {
    let b = boundary(Some(10 * HOUR));
    assert_eq!(b, Some(9 * HOUR));
    // A page with something new, still within the last read's slack:
    // read on.
    assert_eq!(
        after_page(false, true, false, Some(9 * HOUR + 1), b, 1, 20),
        Next::Page
    );
    // It reaches back past it: done, with one page.
    assert_eq!(
        after_page(false, true, false, Some(9 * HOUR - 1), b, 1, 20),
        Next::HeadDone
    );
}

#[test]
fn a_whole_walk_never_stops_early() {
    let b = boundary(Some(10 * HOUR));
    for unchanged in [true, false] {
        assert_eq!(
            after_page(true, true, unchanged, Some(0), b, 1, 20),
            Next::Page
        );
    }
}

#[test]
fn a_head_read_without_dates_reads_on_while_it_finds_changes() {
    let b = boundary(Some(10 * HOUR));
    assert_eq!(after_page(false, true, false, None, b, 1, 20), Next::Page);
    assert_eq!(
        after_page(false, true, false, Some(0), None, 1, 20),
        Next::Page
    );
}

#[test]
fn the_page_budget_yields_to_the_next_pull() {
    let b = boundary(Some(10 * HOUR));
    // A whole walk of more than 20 pages.
    assert_eq!(
        after_page(true, true, true, Some(0), b, 20, 20),
        Next::Yield
    );
    // A head read that still finds changes after the budget.
    assert_eq!(
        after_page(false, true, false, Some(10 * HOUR), b, 20, 20),
        Next::Yield
    );
    // A zero budget still reads a page.
    assert_eq!(after_page(true, true, false, None, None, 1, 0), Next::Yield);
}

#[test]
fn a_row_written_no_newer_than_the_mark_is_late() {
    let mark = Some(10 * HOUR);
    assert!(!late([10 * HOUR + 1], mark), "newer: an ordinary import");
    assert!(late([10 * HOUR + 1, 10 * HOUR], mark), "as old as the mark");
    assert!(!late(Vec::<i64>::new(), mark), "nothing written");
    assert!(!late([0], None), "no mark: nothing to be late against");
}

#[test]
fn a_restart_drops_what_the_walk_read() {
    let mut w = Walk::new(true, None, true);
    w.cursor = Some("c".into());
    w.keys.insert(("s".into(), "r".into()));
    w.newest_ms = Some(5);
    w.changed = true;
    w.restart(None);
    assert!(w.cursor.is_none() && w.keys.is_empty() && w.newest_ms.is_none());
    // Still whole, still the safety net's, and what it merged did change.
    assert!(w.whole && w.forced && w.changed);
}

fn sub(id: &str, title: &str) -> Subscription {
    Subscription {
        id: id.into(),
        kind: SubscriptionKind::Rss,
        origin: format!("https://{id}.blyg.example.com/"),
        feed_url: format!("https://{id}.blyg.example.com/feed.xml"),
        title: title.into(),
        status: "active".into(),
        in_blogroll: false,
        title_follows_source: true,
        poll: SubscriptionPoll::default(),
    }
}

#[test]
fn poll_bookkeeping_is_not_a_subscription_change() {
    let old = vec![sub("a", "Ada"), sub("b", "Bo")];
    let mut new = old.clone();
    new[0].poll.last_poll_at = Some("2026-11-19T10:00:00Z".into());
    new[0].poll.fail_count = 2;
    new[1].status = "degraded".into();
    new[1].in_blogroll = true;
    new.reverse();
    assert!(!subscriptions_differ(&old, &new));
}

#[test]
fn an_added_removed_or_renamed_subscription_is() {
    let old = vec![sub("a", "Ada"), sub("b", "Bo")];
    assert!(subscriptions_differ(&old, &old[..1]));
    assert!(subscriptions_differ(&old[..1], &old));
    let mut renamed = old.clone();
    renamed[1].title = "Bea".into();
    assert!(subscriptions_differ(&old, &renamed));
}
