//! --- syndication --- Knowing the copy a cross-post made, so Burrow can
//! list it on the post (decision #98: an entry `{url, version}` in the
//! item's `syndication`, written in place, never a new version).
//!
//! Two ways, both keyed by extension and the site's host, never by one
//! network, so another site only adds a [`CopyPattern`]:
//!
//! 1. **Captured** at the end of the run: when the site shows a link to the
//!    new post once the composer closes (Bluesky's "View" toast, a
//!    Farcaster "cast published" link), Burrow reads that link's address
//!    before the macro's `done` ([`capture_selector`]) and, when it's the
//!    copy ([`copy_url`]), lists it at once.
//! 2. **Recorded by hand**: otherwise (Substack's composer closes without a
//!    link, or the toast was missed), the pane's bar offers "＋ Record
//!    copy" after the run, and takes the page on screen when it's the copy.
//!
//! The capture selectors are unverified against the live sites; the
//! one-click fallback stays for every site. A site without a pattern takes
//! any page of its own host other than its home.

/// What a copy's address looks like on one site, and where the site shows
/// it after posting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyPattern {
    /// The extension that owns the site (`extension = …`).
    pub ext: &'static str,
    /// The site's host (its `origin`'s), and the copy's (exact, or a
    /// subdomain of it).
    pub host: &'static str,
    /// Something every copy's path contains (`/note/` on Substack Notes:
    /// `https://substack.com/@name/note/c-123`).
    pub path_has: &'static str,
    /// The link to the new post the site shows once it's posted (a CSS
    /// selector list for an `<a>`), read before `done`. `None`: none known.
    pub link: Option<&'static str>,
    /// What to tell the user when the page isn't one.
    pub hint: &'static str,
}

/// The sites whose copy addresses are known.
pub const COPY_PATTERNS: &[CopyPattern] = &[
    CopyPattern {
        ext: crate::NAME,
        host: "substack.com",
        path_has: "/note/",
        link: None,
        hint: "Open the note you posted (its own page, …/note/…) in the pane first",
    },
    // Bluesky: a toast with "View" → /profile/<handle or did>/post/<rkey>.
    CopyPattern {
        ext: crate::NAME,
        host: "bsky.app",
        path_has: "/post/",
        link: Some(
            "[role='alert'] a[href*='/post/'], [role='status'] a[href*='/post/'], \
             [data-testid*='toast' i] a[href*='/post/']",
        ),
        hint: "Open the post you made (…/profile/…/post/…) in the pane first",
    },
    // Farcaster: a "cast published" toast linking /<username>/0x<hash>.
    CopyPattern {
        ext: crate::NAME,
        host: "farcaster.xyz",
        path_has: "/0x",
        link: Some(
            "[role='alert'] a[href*='/0x'], [role='status'] a[href*='/0x'], \
             [data-testid*='toast' i] a[href*='/0x']",
        ),
        hint: "Open the cast you made (…/<username>/0x…) in the pane first",
    },
    CopyPattern {
        ext: crate::NAME,
        host: "warpcast.com",
        path_has: "/0x",
        link: Some(
            "[role='alert'] a[href*='/0x'], [role='status'] a[href*='/0x'], \
             [data-testid*='toast' i] a[href*='/0x']",
        ),
        hint: "Open the cast you made (…/<username>/0x…) in the pane first",
    },
];

fn pattern(ext: &str, origin: &str) -> Option<&'static CopyPattern> {
    let (host, _) = split_https(origin)?;
    COPY_PATTERNS
        .iter()
        .find(|p| p.ext == ext && host_matches(&host, p.host))
}

/// The selector of the link to the new post that `ext`'s site at `origin`
/// shows once it's posted, if one is known.
pub fn capture_selector(ext: &str, origin: &str) -> Option<&'static str> {
    pattern(ext, origin)?.link
}

/// Whether `url` (the page on screen after a run of `ext`'s macro on the
/// site at `origin`, home page `home`, or the link captured at its end) is
/// the copy to list: `Ok` with the address to record, or `Err` with a hint.
/// Only `https:` pages count, and only on the site's own host.
pub fn copy_url(
    ext: &str,
    origin: &str,
    home: Option<&str>,
    url: &str,
) -> Result<String, &'static str> {
    let url = url.trim();
    let Some((host, path)) = split_https(url) else {
        return Err("Only an https: page can be listed as a copy");
    };
    if let Some(p) = pattern(ext, origin) {
        return if host_matches(&host, p.host) && path.contains(p.path_has) {
            Ok(strip_fragment(url))
        } else {
            Err(p.hint)
        };
    }
    // No pattern: a page of the site's own host that isn't its home.
    let Some((site_host, _)) = split_https(origin) else {
        return Err("This site has no https: address");
    };
    let at_home = home.is_some_and(|h| same_page(h, url));
    if !host_matches(&host, &site_host) || at_home || path.trim_matches('/').is_empty() {
        return Err("Open the copy you posted (its own page) in the pane first");
    }
    Ok(strip_fragment(url))
}

/// `(host, path)` of an `https:` URL with a host, no credentials.
fn split_https(url: &str) -> Option<(String, String)> {
    let rest = url.trim().strip_prefix("https://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let host = authority
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let path = rest[end..]
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_string();
    Some((host, path))
}

fn host_matches(host: &str, want: &str) -> bool {
    let want = want.to_ascii_lowercase();
    host == want || host.ends_with(&format!(".{want}"))
}

fn same_page(a: &str, b: &str) -> bool {
    let norm = |s: &str| strip_fragment(s).trim_end_matches('/').to_string();
    norm(a) == norm(b)
}

fn strip_fragment(url: &str) -> String {
    url.split('#').next().unwrap_or(url).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: Option<&str> = Some("https://substack.com/home");

    #[test]
    fn a_substack_note_page_is_the_copy() {
        let ok = copy_url(
            crate::NAME,
            crate::ORIGIN,
            HOME,
            "https://substack.com/@writer/note/c-123456#comments",
        );
        assert_eq!(ok.unwrap(), "https://substack.com/@writer/note/c-123456");
        for not in [
            "https://substack.com/home",
            "https://substack.com/@writer",
            "http://substack.com/@writer/note/c-1",
            "https://evil.example/substack.com/note/c-1",
            "https://user@substack.com/@w/note/c-1",
        ] {
            assert!(
                copy_url(crate::NAME, crate::ORIGIN, HOME, not).is_err(),
                "{not}"
            );
        }
        assert_eq!(capture_selector(crate::NAME, crate::ORIGIN), None);
    }

    #[test]
    fn bluesky_and_farcaster_copies_are_captured_from_their_toasts() {
        let bsky = "https://bsky.app";
        assert!(
            capture_selector(crate::NAME, bsky)
                .unwrap()
                .contains("/post/")
        );
        assert_eq!(
            copy_url(
                crate::NAME,
                bsky,
                None,
                "https://bsky.app/profile/did:plc:abcdexample234/post/3lzexample22"
            )
            .unwrap(),
            "https://bsky.app/profile/did:plc:abcdexample234/post/3lzexample22"
        );
        assert!(copy_url(crate::NAME, bsky, None, "https://bsky.app/profile/someone").is_err());
        let fc = "https://farcaster.xyz";
        assert!(capture_selector(crate::NAME, fc).unwrap().contains("/0x"));
        assert!(
            copy_url(
                crate::NAME,
                fc,
                None,
                "https://farcaster.xyz/someone/0xabc123"
            )
            .is_ok()
        );
        assert!(
            copy_url(
                crate::NAME,
                fc,
                None,
                "https://farcaster.xyz/~/notifications"
            )
            .is_err()
        );
        // Another extension's site on the same host isn't these patterns.
        assert_eq!(capture_selector("other", bsky), None);
    }

    #[test]
    fn a_site_without_a_pattern_takes_any_page_of_its_host_but_home() {
        let check = |url| {
            copy_url(
                "another-ext",
                "https://social.example.net",
                Some("https://social.example.net/home"),
                url,
            )
        };
        assert_eq!(
            check("https://social.example.net/@me/111").unwrap(),
            "https://social.example.net/@me/111"
        );
        assert!(check("https://social.example.net/home").is_err());
        assert!(check("https://social.example.net/").is_err());
        assert!(check("https://elsewhere.example.org/@me/1").is_err());
        assert!(check("javascript:alert(1)").is_err());
    }
}
