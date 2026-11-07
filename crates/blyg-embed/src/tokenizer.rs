//! A BERT WordPiece tokenizer read from a Hugging Face `tokenizer.json`:
//! the `BertNormalizer` (clean the text, space out CJK characters, strip
//! accents, lowercase), the `BertPreTokenizer` (split on whitespace and
//! punctuation) and the `WordPiece` model (greedy longest match, `##`
//! continuations, `[UNK]` for a word it can't spell). No special tokens are
//! added: model2vec encodes without them.
//!
//! What the model2vec models Burrow offers use (potion-base-8M's tokenizer
//! is bge-base-en-v1.5's). Anything else in `tokenizer.json` is refused
//! rather than approximated.

use std::collections::HashMap;

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::{EmbedError, Result};

#[derive(Debug, Clone)]
pub struct WordPiece {
    vocab: HashMap<String, u32>,
    unk: Option<u32>,
    prefix: String,
    max_chars: usize,
    lowercase: bool,
    strip_accents: bool,
    chinese_chars: bool,
    clean_text: bool,
}

fn bad(m: impl Into<String>) -> EmbedError {
    EmbedError::Model(m.into())
}

impl WordPiece {
    /// From the text of a `tokenizer.json`.
    pub fn from_json(json: &str) -> Result<WordPiece> {
        let v: serde_json::Value =
            serde_json::from_str(json).map_err(|e| bad(format!("tokenizer.json: {e}")))?;
        let model = &v["model"];
        if model["type"].as_str() != Some("WordPiece") {
            return Err(bad(
                "tokenizer.json: only WordPiece tokenizers are supported",
            ));
        }
        let vocab: HashMap<String, u32> = model["vocab"]
            .as_object()
            .ok_or_else(|| bad("tokenizer.json: no vocab"))?
            .iter()
            .filter_map(|(k, id)| Some((k.clone(), u32::try_from(id.as_u64()?).ok()?)))
            .collect();
        if vocab.is_empty() {
            return Err(bad("tokenizer.json: empty vocab"));
        }
        let unk = model["unk_token"]
            .as_str()
            .and_then(|t| vocab.get(t).copied());
        let prefix = model["continuing_subword_prefix"]
            .as_str()
            .unwrap_or("##")
            .to_string();
        let max_chars = model["max_input_chars_per_word"].as_u64().unwrap_or(100) as usize;
        let n = &v["normalizer"];
        let (mut lowercase, mut strip, mut chinese, mut clean) = (false, false, false, false);
        match n["type"].as_str() {
            Some("BertNormalizer") => {
                lowercase = n["lowercase"].as_bool().unwrap_or(true);
                // `strip_accents: null` follows `lowercase` (as BERT does).
                strip = n["strip_accents"].as_bool().unwrap_or(lowercase);
                chinese = n["handle_chinese_chars"].as_bool().unwrap_or(true);
                clean = n["clean_text"].as_bool().unwrap_or(true);
            }
            None => {}
            Some(other) => return Err(bad(format!("tokenizer.json: normalizer {other}"))),
        }
        match v["pre_tokenizer"]["type"].as_str() {
            Some("BertPreTokenizer") | None => {}
            Some(other) => return Err(bad(format!("tokenizer.json: pre-tokenizer {other}"))),
        }
        Ok(WordPiece {
            vocab,
            unk,
            prefix,
            max_chars,
            lowercase,
            strip_accents: strip,
            chinese_chars: chinese,
            clean_text: clean,
        })
    }

    pub fn vocab_len(&self) -> usize {
        self.vocab.len()
    }

    pub fn unk_id(&self) -> Option<u32> {
        self.unk
    }

    pub fn id(&self, token: &str) -> Option<u32> {
        self.vocab.get(token).copied()
    }

    /// The normalised text (BertNormalizer).
    pub fn normalize(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            if self.clean_text {
                if c == '\0' || c == '\u{fffd}' || (c.is_control() && !is_ws(c)) {
                    continue;
                }
                if is_ws(c) {
                    out.push(' ');
                    continue;
                }
            }
            if self.chinese_chars && is_cjk(c) {
                out.push(' ');
                out.push(c);
                out.push(' ');
            } else {
                out.push(c);
            }
        }
        if self.strip_accents {
            out = out.nfd().filter(|c| !is_combining_mark(*c)).collect();
        }
        if self.lowercase {
            out = out.to_lowercase();
        }
        out
    }

    /// The token ids of `text`, at most `max` of them (no special tokens).
    pub fn encode(&self, text: &str, max: usize) -> Vec<u32> {
        let norm = self.normalize(text);
        let mut ids = Vec::new();
        for word in pre_tokenize(&norm) {
            if ids.len() >= max {
                break;
            }
            self.word_piece(word, &mut ids);
        }
        ids.truncate(max);
        ids
    }

    fn word_piece(&self, word: &str, out: &mut Vec<u32>) {
        if word.chars().count() > self.max_chars {
            out.extend(self.unk);
            return;
        }
        let mut pieces = Vec::new();
        let mut start = 0;
        let mut piece = String::new();
        while start < word.len() {
            let mut end = word.len();
            let mut found = None;
            while end > start {
                piece.clear();
                if start > 0 {
                    piece.push_str(&self.prefix);
                }
                piece.push_str(&word[start..end]);
                if let Some(id) = self.vocab.get(piece.as_str()) {
                    found = Some(*id);
                    break;
                }
                // Back off one character.
                end = word[..end]
                    .char_indices()
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(start);
            }
            match found {
                Some(id) => {
                    pieces.push(id);
                    start = end;
                }
                None => {
                    out.extend(self.unk);
                    return;
                }
            }
        }
        out.extend(pieces);
    }
}

fn is_ws(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n' || c == '\r' || c.is_whitespace()
}

/// BERT's "Chinese character" ranges (CJK ideographs).
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF | 0x2A700..=0x2B73F
        | 0x2B740..=0x2B81F | 0x2B820..=0x2CEAF | 0xF900..=0xFAFF | 0x2F800..=0x2FA1F)
}

/// BERT's punctuation: every non-alphanumeric ASCII symbol, and Unicode's
/// punctuation blocks (general punctuation, CJK and full-width forms, the
/// Latin-1 marks).
pub fn is_punct(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_punctuation();
    }
    matches!(c as u32,
        0x00A1 | 0x00A7 | 0x00AB | 0x00B6 | 0x00B7 | 0x00BB | 0x00BF
        | 0x2010..=0x2027 | 0x2030..=0x205E | 0x3001..=0x3003 | 0x3008..=0x3011
        | 0x3014..=0x301F | 0xFF01..=0xFF0F | 0xFF1A..=0xFF20 | 0xFF3B..=0xFF40
        | 0xFF5B..=0xFF65)
}

/// Split on whitespace (dropped) and punctuation (each mark its own word).
pub fn pre_tokenize(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() || is_punct(c) {
            if let Some(s) = start.take() {
                out.push(&text[s..i]);
            }
            if is_punct(c) {
                out.push(&text[i..i + c.len_utf8()]);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push(&text[s..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok() -> WordPiece {
        let json = crate::testing::tokenizer_json(&[
            "[PAD]", "[UNK]", "tide", "pool", "##s", "cafe", "un", "##aff", "##able", ",", "!",
            "的",
        ]);
        WordPiece::from_json(&json).unwrap()
    }

    #[test]
    fn splits_words_and_punctuation() {
        assert_eq!(
            pre_tokenize("Tide pools, again!  “really”"),
            vec!["Tide", "pools", ",", "again", "!", "“", "really", "”"]
        );
    }

    #[test]
    fn normalizes_like_bert() {
        let t = tok();
        assert_eq!(t.normalize("Café\tNOIR\u{0}"), "cafe noir");
        assert_eq!(t.normalize("a的b"), "a 的 b");
    }

    #[test]
    fn greedy_longest_match_with_continuations() {
        let t = tok();
        let id = |s: &str| t.id(s).unwrap();
        assert_eq!(
            t.encode("Tide pools, unaffable!", 100),
            vec![
                id("tide"),
                id("pool"),
                id("##s"),
                id(","),
                id("un"),
                id("##aff"),
                id("##able"),
                id("!")
            ]
        );
        // A word it can't spell is one [UNK]; Café is café is cafe.
        assert_eq!(t.encode("Café zzz", 100), vec![id("cafe"), id("[UNK]")]);
        assert_eq!(t.encode("tide tide tide", 2).len(), 2);
    }

    #[test]
    fn refuses_other_tokenizers() {
        let json = r#"{"model":{"type":"BPE","vocab":{"a":0}}}"#;
        assert!(WordPiece::from_json(json).is_err());
    }
}
