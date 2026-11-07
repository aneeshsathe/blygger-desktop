//! A post's plain text as pieces to embed: paragraphs, short ones joined
//! to the next until a piece is a few sentences, long ones cut at sentence
//! ends. A paragraph is what the rail compares against (the paragraph
//! under the caret), so a piece is about one.

/// A piece is joined with the next while shorter than this (characters).
pub const MIN_CHARS: usize = 160;
/// A paragraph longer than this is cut at sentence ends.
pub const MAX_CHARS: usize = 900;
/// At most this many pieces of one post are kept (the opening ones).
pub const MAX_CHUNKS: usize = 24;

/// The paragraphs of `text` (split at blank lines), whitespace collapsed.
pub fn paragraphs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        for w in line.split_whitespace() {
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(w);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Cut `p` at sentence ends into pieces of at most about [`MAX_CHARS`].
fn split_long(p: &str) -> Vec<String> {
    if p.chars().count() <= MAX_CHARS {
        return vec![p.to_string()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut n = 0;
    for (i, c) in p.char_indices() {
        cur.push(c);
        n += 1;
        let end = matches!(c, '.' | '!' | '?' | '…')
            && p[i + c.len_utf8()..].starts_with(char::is_whitespace);
        if (end && n >= MAX_CHARS / 2) || n >= MAX_CHARS * 3 / 2 {
            out.push(cur.trim().to_string());
            cur.clear();
            n = 0;
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// The pieces of `text` to embed, in order.
pub fn chunks(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut acc = String::new();
    for p in paragraphs(text).iter().flat_map(|p| split_long(p)) {
        if !acc.is_empty() {
            acc.push(' ');
        }
        acc.push_str(&p);
        if acc.chars().count() >= MIN_CHARS {
            out.push(std::mem::take(&mut acc));
        }
        if out.len() >= MAX_CHUNKS {
            return out;
        }
    }
    if !acc.is_empty() {
        match out.last_mut() {
            // A short tail joins the piece before it.
            Some(last) if acc.chars().count() < MIN_CHARS / 2 => {
                last.push(' ');
                last.push_str(&acc);
            }
            _ => out.push(acc),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_short_paragraphs_and_cuts_long_ones() {
        assert_eq!(paragraphs("a\nb\n\n\n c  d \n"), vec!["a b", "c d"]);
        let text = "Short one.\n\nAnother short one.\n\nTail.";
        assert_eq!(chunks(text), vec!["Short one. Another short one. Tail."]);
        let long = "This sentence is about tide pools and is fairly long. ".repeat(40);
        let c = chunks(&long);
        assert!(c.len() >= 2, "{}", c.len());
        assert!(c.iter().all(|p| p.chars().count() <= MAX_CHARS * 3 / 2));
        assert!(chunks("").is_empty());
        let many = "A paragraph that's long enough to stand on its own as a piece, \
                    with a few more words to pass the minimum length of a chunk easily, \
                    and then some more besides.\n\n"
            .repeat(60);
        assert_eq!(chunks(&many).len(), MAX_CHUNKS);
    }
}
