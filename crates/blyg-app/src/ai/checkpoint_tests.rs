//! The checkpoint's words (user testing of studio 0.71's screens): the
//! links are this TK's, "the other" agrees with how many, and esc says
//! what to do next.

use blyg_core::generate::{Checkpoint, UnreadLink};

fn unread(n: usize) -> Vec<UnreadLink> {
    (0..n)
        .map(|i| UnreadLink {
            link: format!("https://gone{i}.invalid/a"),
            reason: "HTTP 404".into(),
        })
        .collect()
}

#[test]
fn the_question_is_about_this_tk() {
    let c = Checkpoint {
        read: 1,
        total: 2,
        failed: unread(1),
    };
    assert_eq!(
        super::heading(&c),
        "1 of 2 links in this TK could not be read. Generate anyway?"
    );
    let one = Checkpoint {
        read: 0,
        total: 1,
        failed: unread(1),
    };
    assert_eq!(
        super::heading(&one),
        "1 of 1 link in this TK could not be read. Generate anyway?"
    );
}

/// Finding 3: "The 1 that could be read … the others" for one other.
#[test]
fn the_hint_counts_what_it_reads_and_what_it_doesnt() {
    let hint = |read: u32, failed: usize| {
        super::hint(&Checkpoint {
            read,
            total: read + failed as u32,
            failed: unread(failed),
        })
    };
    assert_eq!(
        hint(1, 1),
        "Burrow will draw on the 1 readable link; the model sees only the address of the \
         other and is told not to describe it."
    );
    assert_eq!(
        hint(2, 3),
        "Burrow will draw on the 2 readable links; the model sees only the addresses of the \
         other 3 and is told not to describe them."
    );
    assert_eq!(
        hint(0, 1),
        "The model sees only the link's address and is told not to describe it."
    );
    assert_eq!(
        hint(0, 2),
        "The model sees only the addresses of the 2 links and is told not to describe them."
    );
}

#[test]
fn esc_says_to_remove_or_replace_the_link() {
    let c = |n| Checkpoint {
        read: 1,
        total: 1 + n as u32,
        failed: unread(n),
    };
    let (text, sub) = super::cancelled(&c(1));
    assert_eq!(text, "Nothing was generated");
    assert!(
        sub.starts_with("Remove or replace the link that couldn't be read"),
        "{sub}"
    );
    let (_, sub) = super::cancelled(&c(2));
    assert!(
        sub.starts_with("Remove or replace the links that couldn't be read"),
        "{sub}"
    );
}
