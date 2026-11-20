//! `app.rs` keeps `MainView`'s construction, `render`'s top-level layout and
//! the event wiring. A feature lives in its own file under `app/` (or its
//! own module beside `app.rs`), as an `impl MainView` block.

/// The most lines `app.rs` may have. A new feature goes in its own file
/// under `app/`, not here. This number only ever goes down: when a move
/// shrinks `app.rs`, lower it to the new size in the same change.
const APP_RS_MAX_LINES: usize = 2557;

#[test]
fn app_rs_stays_small() {
    let lines = include_str!("../app.rs").lines().count();
    assert!(
        lines <= APP_RS_MAX_LINES,
        "app.rs has {lines} lines, over its cap of {APP_RS_MAX_LINES}. Put the new \
         feature in its own file under crates/blyg-app/src/app/ (an `impl MainView` \
         block; see skills/burrow-code-shape) instead of growing app.rs."
    );
}
