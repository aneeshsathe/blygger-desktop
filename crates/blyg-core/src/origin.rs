//! The one spelling of an origin (protocol 0.4 §12.2, §14, §15.4 step 2;
//! decision #103).
//!
//! An origin written into any document, and every origin Burrow compares,
//! is: ASCII only; scheme and host in lowercase; an internationalized host in
//! its A-label (`xn--`) form; no default port; no user name, query or
//! fragment; a path percent-encoded from its UTF-8 bytes with **uppercase**
//! hex digits; and a final `/`. The path's case is significant and never
//! folded. Origins are then compared byte for byte.
//!
//! A WHATWG parser (the `url` crate) gives all of that except two things it
//! leaves as written: an existing escape's hex case (`%c3` stays `%c3`) and
//! the final `/`. [`canonical`] adds both. A URL taken from the network (a
//! final fetch URL, a mention's source) is put through [`fold_url`] before
//! it is compared with a written origin.
//!
//! Display is separate (§14): [`display_host`] shows an internationalized
//! host as Unicode with its A-label beside it, so two look-alike hosts stay
//! tellable apart. Identity is always the ASCII string.

/// `s` as §12.2 spells an origin: `HTTPS://Blyg.Example:443/Notes` →
/// `https://blyg.example/Notes/`. `None` unless it is an absolute http(s)
/// URL with a host.
pub fn canonical(s: &str) -> Option<String> {
    let mut u = parse(s)?;
    u.set_query(None);
    let mut out = serialize(&u);
    if !out.ends_with('/') {
        out.push('/');
    }
    Some(out)
}

/// A URL from the network folded the way §15.4 step 2 folds it before it is
/// compared with `{origin}…`: the same spelling rules as [`canonical`], with
/// the path's last segment and any query kept, and no `/` added.
pub fn fold_url(s: &str) -> Option<String> {
    let u = parse(s)?;
    let mut out = serialize(&u);
    if let Some(q) = u.query() {
        out.push('?');
        out.push_str(q);
    }
    Some(out)
}

/// `a` and `b` are the same origin once both are spelled the one way. Two
/// strings that aren't origins are equal only when they are the same bytes.
pub fn same(a: &str, b: &str) -> bool {
    match (canonical(a), canonical(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// `s` is already in the one spelling (what a conformance check asks).
pub fn is_canonical(s: &str) -> bool {
    canonical(s).as_deref() == Some(s)
}

/// The origin a manifest fetched from `final_url` names (§12.2): the URL
/// minus its last path segment, in the one spelling.
pub fn of_fetch(final_url: &str) -> Option<String> {
    let mut u = parse(final_url)?;
    u.set_query(None);
    let path = u.path().to_string();
    let dir = &path[..path.rfind('/').map_or(0, |i| i + 1)];
    u.set_path(if dir.is_empty() { "/" } else { dir });
    canonical(u.as_str())
}

/// The host of `origin` for people to read (§14): an internationalized host
/// as Unicode, with its A-label in parentheses so it can't pass for a
/// look-alike (`bücher.example (xn--bcher-kva.example)`); any other host as
/// it is. `None` unless `origin` is an http(s) URL with a host.
pub fn display_host(origin: &str) -> Option<String> {
    let u = parse(origin)?;
    let ascii = u.host_str()?.to_string();
    if !ascii.split('.').any(|l| l.starts_with("xn--")) {
        return Some(ascii);
    }
    let (unicode, ok) = idna::domain_to_unicode(&ascii);
    Some(match ok {
        Ok(()) if unicode != ascii => format!("{unicode} ({ascii})"),
        _ => ascii,
    })
}

/// Parse an absolute http(s) URL with a non-empty host, dropping what an
/// origin never carries (user name, password, fragment).
fn parse(s: &str) -> Option<url::Url> {
    let mut u = url::Url::parse(s.trim()).ok()?;
    if !matches!(u.scheme(), "http" | "https") || u.host_str().is_none_or(str::is_empty) {
        return None;
    }
    let _ = u.set_username("");
    let _ = u.set_password(None);
    u.set_fragment(None);
    Some(u)
}

/// `scheme://host[:port]` + the path with its escapes in uppercase hex
/// (no query, no fragment).
fn serialize(u: &url::Url) -> String {
    let head = &u[..url::Position::BeforePath];
    let mut out = String::with_capacity(u.as_str().len());
    out.push_str(head);
    out.push_str(&upper_escapes(u.path()));
    out
}

/// `%c3%a9` → `%C3%A9`; nothing else changes.
fn upper_escapes(path: &str) -> String {
    let b = path.as_bytes();
    let mut out = String::with_capacity(path.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && b[i + 1].is_ascii_hexdigit()
            && b[i + 2].is_ascii_hexdigit()
        {
            out.push('%');
            out.push(b[i + 1].to_ascii_uppercase() as char);
            out.push(b[i + 2].to_ascii_uppercase() as char);
            i += 3;
        } else {
            // The path is ASCII after the parser, so a byte is a char.
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_one_spelling() {
        for (given, want) in [
            ("https://blyg.example.com/", "https://blyg.example.com/"),
            ("https://blyg.example.com", "https://blyg.example.com/"),
            ("HTTPS://Blyg.Example.COM:443/", "https://blyg.example.com/"),
            ("http://blyg.example.com:80", "http://blyg.example.com/"),
            (
                "http://blyg.example.com:8787/",
                "http://blyg.example.com:8787/",
            ),
            (
                "https://blyg.example.com/Notes",
                "https://blyg.example.com/Notes/",
            ),
            (
                "https://blyg.example.com/notes/?q=1#top",
                "https://blyg.example.com/notes/",
            ),
            ("https://u:p@blyg.example.com/", "https://blyg.example.com/"),
            (
                "  https://blyg.example.com/x/  ",
                "https://blyg.example.com/x/",
            ),
            // An internationalized host in its A-label form.
            ("https://Bücher.example/", "https://xn--bcher-kva.example/"),
            // A non-ASCII path from its UTF-8 bytes; existing escapes uppercased.
            (
                "https://blyg.example.com/café/",
                "https://blyg.example.com/caf%C3%A9/",
            ),
            (
                "https://blyg.example.com/caf%c3%a9",
                "https://blyg.example.com/caf%C3%A9/",
            ),
            (
                "https://blyg.example.com/a%2fb/",
                "https://blyg.example.com/a%2Fb/",
            ),
        ] {
            assert_eq!(canonical(given).as_deref(), Some(want), "{given}");
            assert!(is_canonical(want), "{want}");
        }
        for bad in [
            "",
            "blyg.example.com",
            "ftp://blyg.example.com/",
            "mailto:a@b.example",
        ] {
            assert_eq!(canonical(bad), None, "{bad}");
        }
    }

    #[test]
    fn spelling_is_a_fixed_point() {
        for s in [
            "HTTPS://Bücher.Example:443/Caf%c3%a9",
            "http://[::1]:8787/blyg",
            "https://blyg.example.com",
        ] {
            let once = canonical(s).unwrap();
            assert_eq!(canonical(&once).unwrap(), once);
        }
    }

    #[test]
    fn origins_compare_after_folding_and_path_case_counts() {
        assert!(same(
            "https://Blyg.example.com",
            "https://blyg.example.com:443/"
        ));
        assert!(same(
            "https://blyg.example.com/a%c3%a9",
            "https://blyg.example.com/a%C3%A9/"
        ));
        assert!(!same(
            "https://blyg.example.com/Notes/",
            "https://blyg.example.com/notes/"
        ));
        assert!(!same(
            "https://blyg.example.com/a/",
            "https://blyg.example.com/b/"
        ));
        assert!(!same(
            "http://blyg.example.com/",
            "https://blyg.example.com/"
        ));
    }

    #[test]
    fn a_fetched_url_folds_before_it_is_compared() {
        let origin = "https://blyg.example.com/notes/";
        let fetched = "HTTPS://BLYG.example.com:443/notes/items/abc.json#x";
        assert_eq!(
            fold_url(fetched).unwrap(),
            format!("{origin}items/abc.json")
        );
        assert_eq!(
            fold_url("https://blyg.example.com/a%c3%a9?x=%c3").unwrap(),
            "https://blyg.example.com/a%C3%A9?x=%c3"
        );
    }

    #[test]
    fn a_manifest_names_its_directory() {
        assert_eq!(
            of_fetch("https://Blyg.example.com/blyg.json").as_deref(),
            Some("https://blyg.example.com/")
        );
        assert_eq!(
            of_fetch("https://blyg.example.com/wp-json/blyg/v1/manifest?x=1").as_deref(),
            Some("https://blyg.example.com/wp-json/blyg/v1/")
        );
    }

    #[test]
    fn internationalized_hosts_show_both_forms() {
        assert_eq!(
            display_host("https://xn--bcher-kva.example/").as_deref(),
            Some("bücher.example (xn--bcher-kva.example)")
        );
        assert_eq!(
            display_host("https://Bücher.example/notes/").as_deref(),
            Some("bücher.example (xn--bcher-kva.example)")
        );
        assert_eq!(
            display_host("https://blyg.example.com/notes/").as_deref(),
            Some("blyg.example.com")
        );
        assert_eq!(display_host("not a url"), None);
    }
}
