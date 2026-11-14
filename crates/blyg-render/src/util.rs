//! Small helpers that reproduce JavaScript string semantics.

/// The Worker's `escapeHtml` (util.ts): `& < > " '`.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// importer/util.ts `blygItemUrl`: an imported item's own `page` when it has
/// one (an absolute http(s) `page` as-is, since studio 0.36's templated blygs,
/// §16.6e; a relative one origin-relative, a leading `/` dropped), else this
/// client's `{origin}{f|t}/{id}/` convention.
pub fn blyg_item_url(origin: &str, kind: crate::ItemKind, id: &str, page: Option<&str>) -> String {
    match page.filter(|p| !p.is_empty()) {
        Some(p) if is_http_url(p) => p.to_string(),
        Some(p) => format!("{origin}{}", p.strip_prefix('/').unwrap_or(p)),
        None => {
            let seg = if kind == crate::ItemKind::Thread {
                "t"
            } else {
                "f"
            };
            format!("{origin}{seg}/{id}/")
        }
    }
}

/// The address of an item's document when the publisher holds no `page`
/// for it (§10.1, decision #102): `{origin}items/{id}.json`, never a
/// permalink-convention guess. (The origin's `item` template, §6.1, is not
/// held by the local store; the default path is the template's default.)
pub fn item_document_url(origin: &str, id: &str) -> String {
    let origin = if origin.ends_with('/') {
        origin.to_string()
    } else {
        format!("{origin}/")
    };
    format!("{origin}items/{id}.json")
}

/// Whether `url` starts with a scheme (`/^[a-zA-Z][a-zA-Z0-9+.-]*:/`).
fn has_scheme(url: &str) -> bool {
    let Some(colon) = url.find(':') else {
        return false;
    };
    let s = &url[..colon];
    s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
}

/// Undo `escape_html` (and the numeric forms markdown-it writes) in an
/// attribute value.
fn unescape_attr(v: &str) -> String {
    if !v.contains('&') {
        return v.to_string();
    }
    v.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Every `href="…"` and `src="…"` in `html` that is not absolute, resolved
/// by RFC 3986 against `base` (protocol §5.2, decision #102: the item's
/// identity origin, e.g. `https://blyg.example.com/blyg/`). A URL with a
/// scheme is left byte for byte; a fragment-only or network-path reference
/// is resolved too, since §5.2 wants every URL absolute. Nothing changes
/// when `base` is not an absolute http(s) URL.
pub fn absolutize_html(html: &str, base: &str) -> String {
    let Ok(base) = url::Url::parse(base) else {
        return html.to_string();
    };
    if !matches!(base.scheme(), "http" | "https") || base.cannot_be_a_base() {
        return html.to_string();
    }
    if !html.contains("href=\"") && !html.contains("src=\"") {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len() + 64);
    let mut rest = html;
    loop {
        let next = ["href=\"", "src=\""]
            .iter()
            .filter_map(|a| {
                rest.match_indices(a)
                    // An attribute name, not the tail of another
                    // (`data-src`): preceded by whitespace.
                    .find(|(i, _)| *i > 0 && rest.as_bytes()[i - 1].is_ascii_whitespace())
                    .map(|(i, _)| (i, a.len()))
            })
            .min();
        let Some((i, n)) = next else { break };
        let start = i + n;
        let Some(len) = rest[start..].find('"') else {
            break;
        };
        out.push_str(&rest[..start]);
        let value = &rest[start..start + len];
        if has_scheme(value) {
            out.push_str(value);
        } else {
            match base.join(&unescape_attr(value)) {
                Ok(u) => out.push_str(&escape_html(u.as_str())),
                Err(_) => out.push_str(value),
            }
        }
        rest = &rest[start + len..];
    }
    out.push_str(rest);
    out
}

/// `/^https?:\/\//i`.
fn is_http_url(s: &str) -> bool {
    ["http://", "https://"].iter().any(|p| {
        s.get(..p.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(p))
    })
}

/// The Worker's `escapeHref` (util.ts, studio 0.28): an http(s) or mailto
/// URL (relative ones resolve to https), HTML-escaped; anything else is `#`.
pub fn escape_href(value: &str) -> String {
    let followable = url::Url::parse("https://link.invalid/")
        .and_then(|base| base.join(value))
        .is_ok_and(|u| matches!(u.scheme(), "http" | "https" | "mailto"));
    if followable {
        escape_html(value)
    } else {
        "#".to_string()
    }
}

/// JavaScript's WhiteSpace + LineTerminator set (what `String#trim` and `\s` use).
pub fn is_js_ws(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// JavaScript's `String#trim`.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_ws)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_semantics() {
        assert_eq!(js_trim("\u{FEFF} x \u{2028}"), "x");
        assert_eq!(
            escape_html("<a href=\"x\">'&'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;"
        );
    }
}
