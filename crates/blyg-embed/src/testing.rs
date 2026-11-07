//! Test doubles and fixtures (used by this crate's tests and the app's):
//! a deterministic embedder that knows a few families of related words,
//! and builders for a tiny `tokenizer.json` and safetensors file, so no
//! test ever downloads a model.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::vector::normalize;
use crate::{Embedder, Result};

/// Words that mean the same thing to [`ConceptEmbedder`]: each family is
/// one dimension, so "tide" and "shore" match without sharing a letter.
pub const FAMILIES: &[&[&str]] = &[
    &[
        "tide", "tides", "sea", "shore", "pool", "pools", "rockpool", "ocean", "coast",
    ],
    &[
        "garden", "gardens", "tend", "tended", "weed", "weeds", "weeding", "grow", "plants",
    ],
    &[
        "notebook",
        "notebooks",
        "journal",
        "diary",
        "habit",
        "page",
        "pages",
    ],
    &["stream", "streams", "feed", "feeds", "timeline", "newest"],
    &[
        "bread",
        "dough",
        "loaf",
        "sourdough",
        "bake",
        "baking",
        "oven",
    ],
];

/// A deterministic embedder for tests: one dimension per word family
/// ([`FAMILIES`]) plus hashed buckets for every other word. Counts its
/// calls and the texts it was given.
pub struct ConceptEmbedder {
    pub id: String,
    pub calls: AtomicUsize,
    pub texts: Mutex<Vec<String>>,
}

impl Default for ConceptEmbedder {
    fn default() -> Self {
        ConceptEmbedder {
            id: "test-concepts".into(),
            calls: AtomicUsize::new(0),
            texts: Mutex::new(Vec::new()),
        }
    }
}

/// Dimensions: the families, then 64 hashed buckets.
pub const CONCEPT_DIM: usize = 5 + 64;

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

impl ConceptEmbedder {
    pub fn vector(text: &str) -> Vec<f32> {
        let mut v = vec![0f32; CONCEPT_DIM];
        for w in text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2)
            .map(str::to_lowercase)
        {
            match FAMILIES.iter().position(|f| f.contains(&w.as_str())) {
                Some(i) => v[i] += 3.0,
                None => v[5 + (fnv(&w) % 64) as usize] += 0.5,
            }
        }
        normalize(&mut v);
        v
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Embedder for ConceptEmbedder {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut t) = self.texts.lock() {
            t.extend(texts.iter().map(|s| s.to_string()));
        }
        Ok(texts.iter().map(|t| Self::vector(t)).collect())
    }
}

/// A `tokenizer.json` like bge's (BertNormalizer, BertPreTokenizer,
/// WordPiece) with `tokens` as its vocabulary, ids in order. `[UNK]`, if
/// listed, is the unknown token.
pub fn tokenizer_json(tokens: &[&str]) -> String {
    let vocab: serde_json::Map<String, serde_json::Value> = tokens
        .iter()
        .enumerate()
        .map(|(i, t)| (t.to_string(), serde_json::Value::from(i as u64)))
        .collect();
    serde_json::json!({
        "version": "1.0",
        "normalizer": {"type": "BertNormalizer", "clean_text": true,
                       "handle_chinese_chars": true, "strip_accents": null,
                       "lowercase": true},
        "pre_tokenizer": {"type": "BertPreTokenizer"},
        "model": {"type": "WordPiece", "unk_token": "[UNK]",
                  "continuing_subword_prefix": "##",
                  "max_input_chars_per_word": 100, "vocab": vocab}
    })
    .to_string()
}

/// A safetensors file holding one F32 matrix `name` of `rows × dim`.
pub fn safetensors_f32(name: &str, rows: usize, dim: usize, data: &[f32]) -> Vec<u8> {
    let header = serde_json::json!({
        name: {"dtype": "F32", "shape": [rows, dim], "data_offsets": [0, data.len() * 4]}
    })
    .to_string();
    let mut out = Vec::new();
    out.extend_from_slice(&(header.len() as u64).to_le_bytes());
    out.extend_from_slice(header.as_bytes());
    for x in data {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::dot;

    #[test]
    fn families_match_without_shared_words() {
        let a = ConceptEmbedder::vector("What the tide leaves behind");
        let b = ConceptEmbedder::vector("Rock pools along the shore");
        let c = ConceptEmbedder::vector("Weeding the garden");
        assert!(dot(&a, &b) > 0.7, "{}", dot(&a, &b));
        assert!(dot(&a, &c) < 0.3);
    }
}
