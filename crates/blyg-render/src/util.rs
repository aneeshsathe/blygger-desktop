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
