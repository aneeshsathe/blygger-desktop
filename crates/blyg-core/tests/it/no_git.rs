//! What a check that reads git does when there's no git (a source tarball,
//! no `git` on the PATH): skip locally, with a note.

/// Whether this run is on CI (`CI` is set by GitHub Actions and most others).
pub fn on_ci() -> bool {
    std::env::var_os("CI").is_some()
}

/// Called when `what` can't run for want of git.
pub fn skip_or_fail(_on_ci: bool, what: &str) {
    eprintln!("skipped {what}: no git checkout");
}

#[test]
#[should_panic(expected = "needs git")]
fn without_git_a_check_fails_on_ci() {
    skip_or_fail(true, "the personal-data scan");
}

#[test]
fn without_git_a_check_is_skipped_locally() {
    skip_or_fail(false, "the personal-data scan");
}
