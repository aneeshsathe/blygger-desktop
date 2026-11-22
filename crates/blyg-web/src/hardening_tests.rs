//! The page runs with the owner's Studio session cookie, so whatever it
//! links to, fetches or runs is the owner's to lose. These hold the rules
//! that keep it on its own blyg: the mount it derives, the API paths it
//! sends, the Content-Security-Policy it loads under and the escaping its
//! templates use.

use base64::Engine;
use sha2::{Digest, Sha256};

use crate::Burrow;
use crate::api;

// ---------------------------------------------------------------- the mount

/// Mounts that aren't empty or `/segment…` of plain segments: another host
/// (`//evil.example`), empty or dot segments, a backslash, anything
/// percent-encoded.
const BAD_MOUNTS: [&str; 10] = [
    "//evil.example/burrow/",
    "//evil.example/x/burrow/",
    "/a//b/burrow/",
    "/./burrow/",
    "/../burrow/",
    "/a/../burrow/",
    "/a\\b/burrow/",
    "/a%2fb/burrow/",
    "/%2e%2e/burrow/",
    "/a:b/burrow/",
];

#[test]
fn a_mount_is_empty_or_plain_segments() {
    assert_eq!(api::mount_of("/burrow/"), Some(""));
    assert_eq!(api::mount_of("/notes/burrow/"), Some("/notes"));
    assert_eq!(
        api::mount_of("/a-b/c_d.e~f/burrow/index.html"),
        Some("/a-b/c_d.e~f")
    );
    for p in BAD_MOUNTS {
        assert_eq!(api::mount_of(p), None, "{p}");
        assert_eq!(Burrow::mount_of(p), None, "{p}");
        assert_eq!(api::api_url(p, "/api/items"), None, "{p}");
        assert_eq!(api::site_base("https://blyg.example.com", p), None, "{p}");
    }
}

#[test]
fn the_page_refuses_a_mount_that_isnt_plain() {
    let net = include_str!("../www/net.js");
    // net.js's copy of the rule: the same segments, or no mount at all.
    assert!(net.contains(r"const MOUNT_RE = /^(?:\/(?!\.\.?(?:\/|$))[A-Za-z0-9._~-]+)*$/;"));
    assert!(net.contains("return MOUNT_RE.test(m) ? m : null;"));
    assert!(net.contains("export const SITE = location.origin + (MOUNT ?? '');"));
    // Without one, nothing is sent and the page says it can't run here.
    assert!(net.contains("return MOUNT !== null && typeof path === 'string'"));
    let live = include_str!("../www/live.js");
    assert!(live.contains("if (MOUNT !== null) for (const a of document.querySelectorAll('a[href^=\"/studio/login\"]'))"));
    assert!(live.contains("if (MOUNT === null || Burrow.mount_of(location.pathname) !== MOUNT) {"));
}

// ---------------------------------------------------------------- API paths

#[test]
fn an_api_path_refuses_encoded_dots_and_slashes() {
    let net = include_str!("../www/net.js");
    // In the path (not the query, where an encoded origin is ordinary),
    // any case.
    assert!(net.contains(r"!/%2e|%2f|%5c/i.test(path.split('?')[0])"));
}

// ---------------------------------------------------------------- CSP

fn inline_importmap(html: &str) -> &str {
    let open = "<script type=\"importmap\">";
    let at = html.find(open).expect("an import map") + open.len();
    let end = html[at..].find("</script>").unwrap();
    &html[at..at + end]
}

#[test]
fn the_page_loads_under_a_content_security_policy() {
    let html = include_str!("../www/index.html");
    let map = inline_importmap(html);
    let hash = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(map.as_bytes()));
    let csp = format!(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"script-src 'self' 'wasm-unsafe-eval' 'sha256-{hash}'; object-src 'none'; base-uri 'none'\">"
    );
    assert!(html.contains(&csp), "index.html needs {csp}");
    // Before any script, so it covers them all.
    assert!(html.find(&csp) < html.find("<script"));
    // The import map is the one inline script, and no element has an
    // inline handler (the policy would refuse them).
    for (at, _) in html.match_indices("<script") {
        let tag = &html[at..at + html[at..].find('>').unwrap()];
        assert!(
            tag.contains(" src=") || tag == "<script type=\"importmap\"",
            "an inline script: {tag}"
        );
    }
    for h in [" onclick=", " onload=", " onerror=", "javascript:"] {
        assert!(!html.contains(h), "{h}");
    }
}

// ---------------------------------------------------------------- escaping

#[test]
fn esc_escapes_single_quotes_too() {
    let core = include_str!("../www/core.js");
    assert!(core.contains(
        "export const esc = (s) => String(s ?? '').replace(/[&<>\"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '\"': '&quot;', \"'\": '&#39;' }[c]));"
    ));
}
