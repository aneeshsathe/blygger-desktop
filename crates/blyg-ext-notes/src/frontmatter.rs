//! YAML frontmatter, just enough of it: find the block, read a top-level
//! `title:` scalar, and replace a note's body while keeping the block byte
//! for byte. Notes' frontmatter is never added to or rewritten. No YAML
//! library: anything beyond `key: scalar` is opaque text that's carried
//! through untouched.

/// Split `text` into `(frontmatter block, body)`. The block is everything
/// from the opening `---` line to the closing `---` (or `...`) line,
/// inclusive of its line ending; `None` when the note has none. LF and
/// CRLF both work.
pub fn split(text: &str) -> (Option<&str>, &str) {
    let first_end = match text.find('\n') {
        Some(i) => i + 1,
        None => return (None, text),
    };
    if text[..first_end].trim_end_matches(['\r', '\n']) != "---" {
        return (None, text);
    }
    let mut pos = first_end;
    while pos < text.len() {
        let end = text[pos..]
            .find('\n')
            .map(|i| pos + i + 1)
            .unwrap_or(text.len());
        let line = text[pos..end].trim_end_matches(['\r', '\n']);
        if line == "---" || line == "..." {
            return (Some(&text[..end]), &text[end..]);
        }
        pos = end;
    }
    (None, text) // unterminated: not frontmatter
}

/// The body of a note (frontmatter removed).
pub fn body(text: &str) -> &str {
    split(text).1
}

/// A top-level `key: value` scalar from the frontmatter, unquoted.
pub fn scalar(text: &str, key: &str) -> Option<String> {
    let block = split(text).0?;
    for line in block.lines().skip(1) {
        let line = line.trim_end_matches('\r');
        if line.starts_with([' ', '\t']) {
            continue; // nested
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        let v = v.trim();
        let v = v
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(v);
        return (!v.is_empty()).then(|| v.to_string());
    }
    None
}

/// `original` with its body replaced by `new_body`, the frontmatter block
/// kept exactly. For an editor that shows only the body.
pub fn with_body(original: &str, new_body: &str) -> String {
    match split(original).0 {
        Some(block) => format!("{block}{new_body}"),
        None => new_body.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lf_and_crlf() {
        let t = "---\ntitle: Tide tables\ntags: [sea, moon]\n---\n# Body\ntext\n";
        let (fm, body) = split(t);
        assert_eq!(
            fm,
            Some("---\ntitle: Tide tables\ntags: [sea, moon]\n---\n")
        );
        assert_eq!(body, "# Body\ntext\n");
        let c = t.replace('\n', "\r\n");
        let (fm, body) = split(&c);
        assert!(fm.unwrap().ends_with("---\r\n"));
        assert_eq!(body, "# Body\r\ntext\r\n");
        assert_eq!(scalar(&c, "title").as_deref(), Some("Tide tables"));
    }

    #[test]
    fn no_or_unterminated_frontmatter_is_all_body() {
        assert_eq!(split("just text"), (None, "just text"));
        assert_eq!(split("---\nnot closed\n"), (None, "---\nnot closed\n"));
        assert_eq!(split("text\n---\nmore\n---\n").0, None);
        assert_eq!(split("---\n---\nbody").0, Some("---\n---\n"));
    }

    #[test]
    fn reads_scalars_but_not_nested_keys() {
        let t = "---\ntitle: \"Quoted: yes\"\nmeta:\n  title: nested\nempty:\n...\nbody";
        assert_eq!(scalar(t, "title").as_deref(), Some("Quoted: yes"));
        assert_eq!(scalar(t, "empty"), None);
        assert_eq!(scalar(t, "missing"), None);
        assert_eq!(scalar("no fm", "title"), None);
    }

    #[test]
    fn replacing_the_body_keeps_the_block_byte_for_byte() {
        let t = "---\r\nweird:   [a,  b]   # comment\r\n---\r\nold body\r\n";
        let n = with_body(t, "new body\r\n");
        assert_eq!(
            n,
            "---\r\nweird:   [a,  b]   # comment\r\n---\r\nnew body\r\n"
        );
        assert_eq!(with_body("plain", "x"), "x");
    }
}
