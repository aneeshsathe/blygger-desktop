//! cross-post: Burrow's bundled extension that cross-posts a published
//! post to Substack Notes, Bluesky and Farcaster, through the browser pane
//! where the user signed in by hand. All it contributes is a site and a
//! macro for each (a declarative recipe that Burrow runs, shows and
//! confirms); its only code shapes the text (`extension/macro.prepare`):
//! the post's opening paragraph, cut to fit the site's limit, and the
//! link. It never sees the page, a cookie, a key or a sign-in. Each site is
//! granted on its own, so turning on one never turns on another.
//!
//! - [`text`]: the text logic, pure.
//! - [`run`]: the BXP extension around it (`blygger +ext cross-post`).
//!
//! The Substack recipe was checked by hand against the live Substack Notes
//! on 2026-10-09 (`tested`); update the date when it's checked again. The
//! Bluesky and Farcaster recipes are `tested = "unverified"`: their
//! selectors come from the sites' public intent links and their
//! composers' markup as published, not yet from a run against the live
//! sites.

pub mod run;
pub mod text;

use std::path::PathBuf;

use blyg_ext::manifest::{Installed, Manifest, Origin};

pub use run::{CrossPostExt, run, run_stdio};

/// The extension's name (`extension = cross-post`).
pub const NAME: &str = "cross-post";
/// The Substack Notes site's id.
pub const SITE: &str = "substack-notes";
/// The Substack Notes macro's id.
pub const MACRO: &str = "cross-post-note";
/// The Substack origin it asks to automate.
pub const ORIGIN: &str = "https://substack.com";
/// The Bluesky site's id.
pub const BLUESKY_SITE: &str = "bluesky";
/// The Bluesky macro's id.
pub const BLUESKY_MACRO: &str = "cross-post-bluesky";
/// The Bluesky web app's origin.
pub const BLUESKY_ORIGIN: &str = "https://bsky.app";
/// The Farcaster site's id.
pub const FARCASTER_SITE: &str = "farcaster";
/// The Farcaster macro's id.
pub const FARCASTER_MACRO: &str = "cross-post-cast";
/// The Farcaster web app's origin.
pub const FARCASTER_ORIGIN: &str = "https://farcaster.xyz";

const MANIFEST: &str = r#"
name = "cross-post"
version = "0.1.0"
protocol = 1
description = "Cross-posts a published post to Substack Notes, Bluesky or Farcaster, in the browser pane, after you confirm."
capabilities = [
  "items.read",
  "ui",
  "browser.automate:https://substack.com",
  "browser.automate:https://bsky.app",
  "browser.automate:https://farcaster.xyz",
]

[[settings]]
key = "template"
kind = "text"
docs = "The note: {{excerpt}}, {{title}} and {{permalink}}, \\n for a line break (default {{excerpt}}\\n\\n{{permalink}})."

[[settings]]
key = "max-chars"
kind = "number"
docs = "The longest Substack note, link included, in characters as a reader counts them (default 280)."

[[settings]]
key = "bluesky-max-chars"
kind = "number"
docs = "The longest Bluesky post, link included, in characters as a reader counts them (default 300, Bluesky's limit)."

[[settings]]
key = "farcaster-max-bytes"
kind = "number"
docs = "The longest cast, link included, in bytes of UTF-8 (default 320, Farcaster's limit; 1024 for a long cast)."

[[sites]]
id = "substack-notes"
title = "Substack Notes"
origin = "https://substack.com"
home = "https://substack.com/home"
signed-out = "a[href*='sign-in'], form[action*='sign-in']"
content-blocking = false
min-interval = "60s"

# The selectors are best guesses, not yet checked against the live site.
# The home feed has a "What's on your mind?" prompt; clicking it opens a
# modal composer (an editor, Cancel, and Post, disabled until there's
# text). The editor is only ever looked for inside the dialog, never the
# feed's prompt. A selector list is tried part by part, in order.
[[macros]]
id = "cross-post-note"
title = "Cross-post to Substack Notes…"
detail = "The opening paragraph and the link, after you check it"
site = "substack-notes"
when = "published"
template = "{{excerpt}}\n\n{{permalink}}"
tested = "2026-10-09"
steps = [
  { do = "open", url = "https://substack.com/home" },
  { do = "waitFor", selector = "[role='button'], button, div", text = "What's on your mind?", timeout = "20s" },
  { do = "click", selector = "[role='button'], button, div", text = "What's on your mind?" },
  { do = "waitFor", selector = "[role='dialog'] div.ProseMirror[contenteditable='true'], [role='dialog'] [contenteditable='true'], [aria-modal='true'] [contenteditable='true']", timeout = "10s" },
  { do = "focus", selector = "[role='dialog'] div.ProseMirror[contenteditable='true'], [role='dialog'] [contenteditable='true'], [aria-modal='true'] [contenteditable='true']" },
  { do = "insert", selector = "[role='dialog'] div.ProseMirror[contenteditable='true'], [role='dialog'] [contenteditable='true'], [aria-modal='true'] [contenteditable='true']" },
  { do = "submit", selector = "[role='dialog'] button, [aria-modal='true'] button", text = "Post" },
  { do = "waitFor", selector = "[role='dialog'] div.ProseMirror[contenteditable='true'], [role='dialog'] [contenteditable='true'], [aria-modal='true'] [contenteditable='true']", absent = true, timeout = "15s" },
  { do = "done", text = "Posted to Substack Notes" },
]

[[sites]]
id = "bluesky"
title = "Bluesky"
origin = "https://bsky.app"
home = "https://bsky.app/"
# The signed-out splash's two buttons (social-app's testIDs).
signed-out = "[data-testid='signInButton'], [data-testid='createAccountButton']"
content-blocking = false
min-interval = "60s"

# Bluesky's documented intent link (https://docs.bsky.app/docs/advanced-guides/intent-links)
# opens the composer at any window width; the text goes in by `insert`, never
# in the URL. The composer is a TipTap (ProseMirror) editor, and its Post
# button is social-app's `composerPublishBtn`.
[[macros]]
id = "cross-post-bluesky"
title = "Cross-post to Bluesky…"
detail = "The opening paragraph and the link, after you check it"
site = "bluesky"
when = "published"
template = "{{excerpt}}\n\n{{permalink}}"
tested = "unverified"
steps = [
  { do = "open", url = "https://bsky.app/intent/compose" },
  { do = "waitFor", selector = "[aria-modal='true'] div.ProseMirror[contenteditable='true'], [role='dialog'] div.ProseMirror[contenteditable='true'], div.ProseMirror[contenteditable='true']", timeout = "20s" },
  { do = "focus", selector = "[aria-modal='true'] div.ProseMirror[contenteditable='true'], [role='dialog'] div.ProseMirror[contenteditable='true'], div.ProseMirror[contenteditable='true']" },
  { do = "insert", selector = "[aria-modal='true'] div.ProseMirror[contenteditable='true'], [role='dialog'] div.ProseMirror[contenteditable='true'], div.ProseMirror[contenteditable='true']" },
  { do = "submit", selector = "[data-testid='composerPublishBtn'], [aria-modal='true'] [role='button'], [role='dialog'] [role='button']", text = "Post" },
  { do = "waitFor", selector = "[aria-modal='true'] div.ProseMirror[contenteditable='true'], [role='dialog'] div.ProseMirror[contenteditable='true'], div.ProseMirror[contenteditable='true']", absent = true, timeout = "15s" },
  { do = "done", text = "Posted to Bluesky" },
]

[[sites]]
id = "farcaster"
title = "Farcaster"
origin = "https://farcaster.xyz"
home = "https://farcaster.xyz/"
content-blocking = false
min-interval = "60s"

# The web client's compose link (`/~/compose`, as on warpcast.com before the
# move) opens the cast composer in a dialog; the text goes in by `insert`,
# never in the URL. No signed-out selector: signed out, the composer never
# opens, so the run stops at step 2 and nothing is cast.
[[macros]]
id = "cross-post-cast"
title = "Cross-post to Farcaster…"
detail = "The opening paragraph and the link as a cast, after you check it"
site = "farcaster"
when = "published"
template = "{{excerpt}}\n\n{{permalink}}"
tested = "unverified"
steps = [
  { do = "open", url = "https://farcaster.xyz/~/compose" },
  { do = "waitFor", selector = "[role='dialog'] [contenteditable='true'], [aria-modal='true'] [contenteditable='true']", timeout = "20s" },
  { do = "focus", selector = "[role='dialog'] [contenteditable='true'], [aria-modal='true'] [contenteditable='true']" },
  { do = "insert", selector = "[role='dialog'] [contenteditable='true'], [aria-modal='true'] [contenteditable='true']" },
  { do = "submit", selector = "[role='dialog'] button, [aria-modal='true'] button", text = "Cast" },
  { do = "waitFor", selector = "[role='dialog'] [contenteditable='true'], [aria-modal='true'] [contenteditable='true']", absent = true, timeout = "15s" },
  { do = "done", text = "Cast on Farcaster" },
]
"#;

/// The Bluesky composer's editor, preferring the one in the modal.
pub const BLUESKY_EDITOR: &str = "[aria-modal='true'] div.ProseMirror[contenteditable='true'], \
                                  [role='dialog'] div.ProseMirror[contenteditable='true'], \
                                  div.ProseMirror[contenteditable='true']";

/// The Farcaster composer's editor: inside the dialog only.
pub const FARCASTER_EDITOR: &str = "[role='dialog'] [contenteditable='true'], \
                                    [aria-modal='true'] [contenteditable='true']";

/// The Substack composer's editor: inside the modal only (never the feed's prompt).
pub const EDITOR: &str = "[role='dialog'] div.ProseMirror[contenteditable='true'], \
                          [role='dialog'] [contenteditable='true'], \
                          [aria-modal='true'] [contenteditable='true']";

/// The manifest (checked by the same rules as any extension's).
pub fn manifest() -> Manifest {
    Manifest::parse(MANIFEST, true).expect("the bundled manifest is valid")
}

/// The bundled extension for the host's `HostConfig::bundled`: run as
/// `program args…` (the app passes its own executable and
/// `["+ext", "cross-post"]`). Off until `extension = cross-post`.
pub fn bundled(program: PathBuf, args: Vec<String>) -> Installed {
    Installed {
        manifest: manifest(),
        origin: Origin::Bundled { program, args },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blyg_ext::Capability;
    use blyg_ext::protocol::When;
    use blyg_ext::recipe::{Step, check_macro, check_site, first_submit, insert_index};

    #[test]
    fn the_manifest_asks_for_its_three_sites_only() {
        let m = manifest();
        assert_eq!(m.name, NAME);
        assert_eq!(
            m.capabilities,
            vec![
                Capability::ItemsRead,
                Capability::Ui,
                Capability::BrowserAutomate(ORIGIN.into()),
                Capability::BrowserAutomate(BLUESKY_ORIGIN.into()),
                Capability::BrowserAutomate(FARCASTER_ORIGIN.into()),
            ]
        );
        assert!(m.commands.is_empty() && m.libraries.is_empty() && m.sources.is_empty());
        let keys: Vec<&str> = m.settings.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "template",
                "max-chars",
                "bluesky-max-chars",
                "farcaster-max-bytes"
            ]
        );
    }

    /// Each macro on its own site: a grant for one never offers another.
    #[test]
    fn each_macro_is_on_its_own_site() {
        let m = manifest();
        let pairs: Vec<(&str, &str, &str)> = m
            .macros_with_sites()
            .map(|(mac, site)| (mac.id.as_str(), site.id.as_str(), site.origin.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                (MACRO, SITE, ORIGIN),
                (BLUESKY_MACRO, BLUESKY_SITE, BLUESKY_ORIGIN),
                (FARCASTER_MACRO, FARCASTER_SITE, FARCASTER_ORIGIN),
            ]
        );
    }

    /// Steps by name, and the editor every editor step names.
    fn check_composer_macro(id: &str, open: &str, submit: &str, editor: &str) {
        let m = manifest();
        let (mac, site) = m.macros_with_sites().find(|(m, _)| m.id == id).unwrap();
        check_site(site).unwrap();
        check_macro(mac, &m.sites).unwrap();
        assert!(!site.content_blocking);
        assert_eq!(site.min_interval(), std::time::Duration::from_secs(60));
        assert_eq!(mac.when, When::Published);
        assert_eq!(
            mac.tested, "unverified",
            "not yet run against the live site"
        );
        assert_eq!(mac.template, "{{excerpt}}\n\n{{permalink}}");
        let names: Vec<&str> = mac.steps.iter().map(Step::name).collect();
        assert_eq!(
            names,
            [
                "open", "waitFor", "focus", "insert", "submit", "waitFor", "done"
            ]
        );
        assert!(matches!(&mac.steps[0], Step::Open { url } if url == open));
        assert_eq!(insert_index(&mac.steps), Some(3));
        assert_eq!(first_submit(&mac.steps), Some(4));
        assert_eq!(mac.steps[4].match_text(), Some(submit));
        let editor = editor.split_whitespace().collect::<Vec<_>>().join(" ");
        for i in [1, 2, 3, 5] {
            assert_eq!(
                mac.steps[i].selector(),
                Some(editor.as_str()),
                "step {}",
                i + 1
            );
        }
        assert!(matches!(&mac.steps[5], Step::WaitFor { absent: true, .. }));
    }

    #[test]
    fn the_bluesky_macro_uses_the_intent_link_and_the_post_button() {
        check_composer_macro(
            BLUESKY_MACRO,
            "https://bsky.app/intent/compose",
            "Post",
            BLUESKY_EDITOR,
        );
        let m = manifest();
        let (mac, site) = m
            .macros_with_sites()
            .find(|(m, _)| m.id == BLUESKY_MACRO)
            .unwrap();
        assert_eq!(site.title, "Bluesky");
        assert_eq!(mac.title, "Cross-post to Bluesky…");
        assert!(site.signed_out.as_deref().unwrap().contains("signInButton"));
        assert!(
            mac.steps[4]
                .selector()
                .unwrap()
                .starts_with("[data-testid='composerPublishBtn']")
        );
    }

    #[test]
    fn the_farcaster_macro_casts_from_the_compose_dialog() {
        check_composer_macro(
            FARCASTER_MACRO,
            "https://farcaster.xyz/~/compose",
            "Cast",
            FARCASTER_EDITOR,
        );
        let m = manifest();
        let (mac, site) = m
            .macros_with_sites()
            .find(|(m, _)| m.id == FARCASTER_MACRO)
            .unwrap();
        assert_eq!(site.title, "Farcaster");
        assert_eq!(mac.title, "Cross-post to Farcaster…");
        assert_eq!(site.signed_out, None, "a wrong guess would block every run");
        // Only ever inside the dialog: never a contenteditable elsewhere.
        assert!(
            FARCASTER_EDITOR
                .split(',')
                .all(|part| part.trim().starts_with("[role='dialog']")
                    || part.trim().starts_with("[aria-modal='true']"))
        );
    }

    #[test]
    fn the_site_and_macro_pass_the_recipe_rules() {
        let m = manifest();
        assert_eq!(m.sites.len(), 3);
        let site = &m.sites[0];
        assert_eq!(site.id, SITE);
        assert_eq!(check_site(site).unwrap(), ORIGIN);
        assert!(!site.content_blocking);
        assert_eq!(site.min_interval(), std::time::Duration::from_secs(60));
        assert!(site.signed_out.is_some());
        let (mac, s) = m.macros_with_sites().next().unwrap();
        assert_eq!(s.id, SITE);
        assert_eq!(mac.id, MACRO);
        assert_eq!(mac.title, "Cross-post to Substack Notes…");
        assert_eq!(mac.when, When::Published);
        assert_eq!(
            mac.tested, "2026-10-09",
            "the date it was last checked by hand"
        );
        assert_eq!(mac.template, "{{excerpt}}\n\n{{permalink}}");
        check_macro(mac, &m.sites).unwrap();
        let names: Vec<&str> = mac.steps.iter().map(Step::name).collect();
        assert_eq!(
            names,
            [
                "open", "waitFor", "click", "waitFor", "focus", "insert", "submit", "waitFor",
                "done"
            ]
        );
        // The home feed, not /notes (which redirects).
        assert!(matches!(&mac.steps[0], Step::Open { url } if url == "https://substack.com/home"));
        assert_eq!(mac.steps[2].match_text(), Some("What's on your mind?"));
        assert_eq!(insert_index(&mac.steps), Some(5));
        assert_eq!(first_submit(&mac.steps), Some(6));
        assert_eq!(mac.steps[6].match_text(), Some("Post"));
        assert!(
            mac.steps[6]
                .selector()
                .unwrap()
                .starts_with("[role='dialog'] button")
        );
        // The editor only inside the modal; gone once it closes.
        let editor = EDITOR.split_whitespace().collect::<Vec<_>>().join(" ");
        for i in [3, 4, 5, 7] {
            assert_eq!(
                mac.steps[i].selector(),
                Some(editor.as_str()),
                "step {}",
                i + 1
            );
        }
        assert!(matches!(&mac.steps[7], Step::WaitFor { absent: true, .. }));
    }
}
