//! A list sitting pulls out (`churn`) is still read by a pull the user
//! asked for, and the notice says what the sit-out means for each list.

use super::churn::{Churn, STREAK, message};

#[test]
fn a_pull_the_user_asked_for_reads_it_and_counts_as_a_try() {
    let mut c = Churn::default();
    for _ in 0..STREAK {
        c.expired();
    }
    // Sitting out one pull now; Sync now reads it all the same.
    assert!(c.due(true), "Sync now reads a list that sits out");
    // It expired again: the next sit-out is longer, as for any try.
    c.expired();
    assert!(!c.due(false));
    assert!(!c.due(false));
    assert!(c.due(false));
    // Read to its end: every pull again.
    c.read();
    assert!(c.due(false));
}

#[test]
fn each_list_says_what_it_means_for_that_list() {
    let lists = ["background", "hoppers", "signals", "shelf", "freshness"];
    let said: Vec<String> = lists.iter().map(|l| message(l)).collect();
    for (list, m) in lists.iter().zip(&said) {
        assert!(m.contains("later"), "{list}: {m}");
        assert!(!m.contains("minute"), "counted in pulls, not time: {m}");
        assert_eq!(
            m.contains("Feed"),
            *list == "background",
            "only the lens decides Feed or Background: {m}"
        );
    }
    assert!(
        said[3].contains("Saved") && !said[3].contains("Shelf"),
        "{}",
        said[3]
    );
    let distinct: std::collections::HashSet<&String> = said.iter().collect();
    assert_eq!(distinct.len(), lists.len(), "{said:?}");
}
