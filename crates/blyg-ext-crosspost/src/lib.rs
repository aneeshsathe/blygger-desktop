//! cross-post: Burrow's bundled extension that cross-posts a published
//! post to Substack Notes, through the browser pane where the user signed
//! in by hand. All it contributes is a site and a macro (a declarative
//! recipe that Burrow runs, shows and confirms); its only code shapes the
//! text (`extension/macro.prepare`): the post's opening paragraph, cut to
//! fit, and the link. It never sees the Substack page, a cookie or a
//! sign-in.
//!
//! - [`text`]: the text logic, pure.
//! - [`run`]: the BXP extension around it (`blygger +ext cross-post`).
//!
//! The selectors in the recipe are best guesses, marked
//! `tested = "unverified"` until someone checks them by hand.

pub mod run;
pub mod text;

use std::path::PathBuf;

use blyg_ext::manifest::{Installed, Manifest, Origin};

pub use run::{CrossPostExt, run, run_stdio};

/// The extension's name (`extension = cross-post`).
pub const NAME: &str = "cross-post";
/// The site's id.
pub const SITE: &str = "substack-notes";
/// The macro's id.
pub const MACRO: &str = "cross-post-note";
/// The one origin it asks to automate.
pub const ORIGIN: &str = "https://substack.com";

const MANIFEST: &str = r#"
name = "cross-post"
version = "0.1.0"
protocol = 1
description = "Cross-posts a published post to Substack Notes, in the browser pane, after you confirm."
capabilities = ["items.read", "ui", "browser.automate:https://substack.com"]

[[settings]]
key = "template"
kind = "text"
docs = "The note: {{excerpt}}, {{title}} and {{permalink}}, \\n for a line break (default {{excerpt}}\\n\\n{{permalink}})."

[[settings]]
key = "max-chars"
kind = "number"
docs = "The longest note, link included, in characters as a reader counts them (default 280)."

[[sites]]
id = "substack-notes"
title = "Substack Notes"
origin = "https://substack.com"
home = "https://substack.com/notes"
signed-out = "a[href*='sign-in'], form[action*='sign-in']"
content-blocking = false
min-interval = "60s"

# The selectors are best guesses, not yet checked against the live site.
[[macros]]
id = "cross-post-note"
title = "Cross-post to Substack Notes…"
detail = "The opening paragraph and the link, after you check it"
site = "substack-notes"
when = "published"
template = "{{excerpt}}\n\n{{permalink}}"
tested = "unverified"
steps = [
  { do = "open", url = "https://substack.com/notes" },
  { do = "waitFor", selector = "div.ProseMirror[contenteditable='true']", timeout = "20s" },
  { do = "focus", selector = "div.ProseMirror[contenteditable='true']" },
  { do = "insert", selector = "div.ProseMirror[contenteditable='true']" },
  { do = "submit", selector = "button", text = "Post" },
  { do = "waitFor", selector = "div.ProseMirror[contenteditable='true']", empty = true, timeout = "15s" },
  { do = "done", text = "Posted to Substack Notes" },
]
"#;

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
    fn the_manifest_asks_for_substack_only() {
        let m = manifest();
        assert_eq!(m.name, NAME);
        assert_eq!(
            m.capabilities,
            vec![
                Capability::ItemsRead,
                Capability::Ui,
                Capability::BrowserAutomate(ORIGIN.into())
            ]
        );
        assert!(m.commands.is_empty() && m.libraries.is_empty() && m.sources.is_empty());
        let keys: Vec<&str> = m.settings.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, ["template", "max-chars"]);
    }

    #[test]
    fn the_site_and_macro_pass_the_recipe_rules() {
        let m = manifest();
        assert_eq!(m.sites.len(), 1);
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
        assert_eq!(mac.tested, "unverified", "unverified until checked by hand");
        assert_eq!(mac.template, "{{excerpt}}\n\n{{permalink}}");
        check_macro(mac, &m.sites).unwrap();
        let names: Vec<&str> = mac.steps.iter().map(Step::name).collect();
        assert_eq!(
            names,
            [
                "open", "waitFor", "focus", "insert", "submit", "waitFor", "done"
            ]
        );
        assert_eq!(insert_index(&mac.steps), Some(3));
        assert_eq!(first_submit(&mac.steps), Some(4));
        assert_eq!(mac.steps[4].match_text(), Some("Post"));
        assert!(matches!(&mac.steps[5], Step::WaitFor { empty: true, .. }));
    }
}
