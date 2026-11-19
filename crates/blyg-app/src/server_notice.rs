//! What Burrow tells you about your blyg's server.
//!
//! The "your blyg needs updating" notice: Burrow needs blygger-studio
//! `blyg_core::api::MIN_STUDIO` or later, whose private API
//! contract is 5.x. On an older blyg nothing is pushed or pulled; what's on
//! this Mac stays, and syncs once the studio is updated. A toast when sync
//! first notices (and on connecting to such a blyg), then a status-bar
//! notice for as long as it stays true. The About window says the same.
//!
//! The held-labels sheet (`HELD_*`) dates from the 0.47 floor, when only
//! posts with labelled links waited. It still shows for such a post while
//! the blyg is outdated, though now everything waits.
//!
//! There used to be a second notice here, a sheet listing what a studio
//! older than 0.40 limits. The 0.47 floor made it unreachable: every blyg
//! old enough to see it already gets this one, and nothing is sent to it.
//!
//! A child module of `app` (like the update notice) so it can use
//! `MainView`'s toast.

use gpui_kit::*;

use super::MainView;

/// Where the notice's link goes: the server requirements, which link on to
/// upstream's upgrade guide.
pub const SERVER_HELP_URL: &str = "https://aneeshsathe.github.io/blygger-desktop/server.html";

/// The toast, once per session (and on connecting to an older blyg).
pub const OUTDATED_TOAST: &str = "Your blyg needs updating";
pub const OUTDATED_TOAST_SUB: &str = concat!(
    "Nothing syncs until it's on blygger-studio ",
    blyg_core::min_studio!(),
    " or later. Your posts stay on this Mac."
);

/// The status bar's text while the server is outdated, and its link.
pub const OUTDATED_NOTICE: &str = concat!(
    "Your blyg needs updating · nothing syncs until it's on studio ",
    blyg_core::min_studio!()
);
pub const OUTDATED_LINK: &str = "Update your blyg";

/// The About window's line, under the connection.
pub const OUTDATED_ABOUT: &str = concat!(
    "Nothing syncs until it's on blygger-studio ",
    blyg_core::min_studio!(),
    " or later. Your posts stay on this Mac and sync once it's updated."
);

/// The publish sheet on a post held for its labelled links.
pub const HELD_SHEET: &str = concat!(
    "This post has labelled links, and your blyg needs updating to studio ",
    blyg_core::min_studio!(),
    " before anything syncs."
);
pub const HELD_FIX: &str = "write them as [[id]] and publish";

/// The list row's tooltip on a post that waits for them.
pub const HELD_TIP: &str = concat!("Waiting: your blyg needs studio ", blyg_core::min_studio!());

impl MainView {
    /// Sync just found an outdated server (or a connect did).
    pub(super) fn server_outdated(&mut self, cx: &mut Context<Self>) {
        self.show_toast(
            OUTDATED_TOAST,
            Some(crate::keymap::hint(OUTDATED_TOAST_SUB).into()),
            cx,
        );
    }

    /// "Your blyg needs updating · posts with labelled links wait … ·
    /// Update your blyg", while true.
    pub(super) fn render_server_notice(&self) -> Option<AnyElement> {
        if !self.backend.server_outdated() {
            return self.render_studio_update_notice(); // --- site ---
        }
        // --- themes --- on the status bar's own ground, warning-coloured.
        let warn = self.palette.on_status_text(self.palette.warn);
        let link = self.palette.on_status_text(self.palette.accent);
        Some(
            div()
                .id("server-notice")
                .flex()
                .items_center()
                .gap(px(6.))
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        .truncate()
                        .text_color(warn)
                        .child(crate::keymap::hint(OUTDATED_NOTICE)),
                )
                .child("·")
                .child(
                    div()
                        .id("server-notice-link")
                        .cursor_pointer()
                        .text_color(link)
                        .hover(|s| s.underline())
                        .child(OUTDATED_LINK)
                        .on_click(|_, _, cx| cx.open_url(SERVER_HELP_URL)),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// `"0.49.12 or later"` → `([0, 49, 12], 7)`: the version and its
    /// length; `None` unless it starts with `N.N`.
    fn version(s: &str) -> Option<(Vec<u64>, usize)> {
        let end = s
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(s.len());
        let digits = s[..end].trim_end_matches('.');
        let v: Vec<u64> = digits
            .split('.')
            .map(|p| p.parse().ok())
            .collect::<Option<_>>()?;
        (v.len() >= 2).then_some((v, digits.len()))
    }

    /// Every "studio X or later" / "studio X and later" / "studio X+" /
    /// "older than X" in `text` whose X is below `floor`.
    fn stale_floors(text: &str, floor: &[u64]) -> Vec<String> {
        let mut out = Vec::new();
        for (needle, needs_after) in [("studio ", true), ("older than ", false)] {
            let mut from = 0;
            while let Some(i) = text[from..].find(needle) {
                let at = from + i + needle.len();
                from = at;
                let rest = text[at..].trim_start_matches('v');
                let Some((v, len)) = version(rest) else {
                    continue;
                };
                let after = &rest[len..];
                let states_floor = !needs_after
                    || after.starts_with(" or later")
                    || after.starts_with(" and later")
                    || after.starts_with('+');
                if states_floor && v.as_slice() < floor {
                    out.push(format!("{needle}{}", &rest[..len]));
                }
            }
        }
        out
    }

    fn walk(dir: &Path, ext: &str, files: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, ext, files);
            } else if p.extension().is_some_and(|x| x == ext) {
                files.push(p);
            }
        }
    }

    #[test]
    fn the_floor_messages_use_min_studio() {
        let min = blyg_core::api::MIN_STUDIO;
        assert_eq!(min, blyg_core::min_studio!());
        for s in [
            super::OUTDATED_TOAST_SUB,
            super::OUTDATED_NOTICE,
            super::OUTDATED_ABOUT,
            super::HELD_SHEET,
            super::HELD_TIP,
            crate::app::CONNECT_INTRO,
            crate::app::onboarding::SETUP_CONNECT,
        ] {
            assert!(s.contains(min), "{s:?} doesn't say {min}");
        }
    }

    /// No UI string, and no current statement in the site docs, names a
    /// server floor older than `MIN_STUDIO` ("Needs blygger-studio 0.28 or
    /// later" outlived three floor raises). Comments are history and are
    /// skipped, as is the release notes page.
    #[test]
    fn no_text_states_an_older_floor() {
        let (floor, _) = version(blyg_core::api::MIN_STUDIO).unwrap();
        assert_eq!(
            stale_floors("Needs blygger-studio 0.28 or later.", &floor),
            ["studio 0.28"]
        );
        assert_eq!(
            stale_floors("(its studio is older than 0.40)", &floor),
            ["older than 0.40"]
        );
        assert!(stale_floors("studio 0.56 or later; studio 0.39's", &floor).is_empty());

        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut bad = Vec::new();
        let mut rs = Vec::new();
        walk(&root.join("src"), "rs", &mut rs);
        // This file's own strings are `concat!`ed from MIN_STUDIO (checked
        // above), and its tests hold stale examples on purpose.
        for f in rs.into_iter().filter(|f| !f.ends_with("server_notice.rs")) {
            let text = std::fs::read_to_string(&f).unwrap();
            for (n, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for hit in stale_floors(line, &floor) {
                    bad.push(format!("{}:{}: {hit}", f.display(), n + 1));
                }
            }
        }
        let mut md = Vec::new();
        walk(&root.join("../../site/src"), "md", &mut md);
        for f in md {
            if f.file_name().is_some_and(|n| n == "releases.md") {
                continue;
            }
            let text = std::fs::read_to_string(&f).unwrap().replace('\n', " ");
            for hit in stale_floors(&text, &floor) {
                bad.push(format!("{}: {hit}", f.display()));
            }
        }
        assert!(bad.is_empty(), "stale server floors:\n{}", bad.join("\n"));
    }
}
