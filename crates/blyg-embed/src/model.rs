//! A model2vec static-embedding model: a vector per token, read from a
//! safetensors file, and a text's embedding is the mean of its tokens'
//! vectors (unknown tokens left out), normalised. No neural network runs:
//! embedding a paragraph is a tokenizer pass and a few hundred additions,
//! tens of microseconds.
//!
//! The safetensors format is a little-endian u64 header length, a JSON
//! header naming each tensor's dtype, shape and byte range, then the raw
//! data; it's read here directly. Tensors: `embeddings` (F32, F16 or BF16,
//! `[vocab, dim]`), and the optional per-token `weights` (`[vocab]`) and
//! `mapping` (token id → embedding row, I32/I64) of newer model2vec models.

use std::path::Path;

use crate::tokenizer::WordPiece;
use crate::vector::{f16_to_f32, normalize};
use crate::{EmbedError, Embedder, Result};

/// model2vec's default: at most this many tokens of a text count.
pub const MAX_TOKENS: usize = 512;

pub struct StaticModel {
    id: String,
    tokenizer: WordPiece,
    /// `rows × dim`, row-major.
    table: Vec<f32>,
    dim: usize,
    rows: usize,
    weights: Option<Vec<f32>>,
    mapping: Option<Vec<u32>>,
}

fn bad(m: impl Into<String>) -> EmbedError {
    EmbedError::Model(m.into())
}

/// One tensor's dtype, shape and bytes.
struct Tensor<'a> {
    dtype: String,
    shape: Vec<usize>,
    data: &'a [u8],
}

fn tensors(bytes: &[u8]) -> Result<Vec<(String, Tensor<'_>)>> {
    let len = bytes
        .get(..8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap_or_default()) as usize)
        .ok_or_else(|| bad("model.safetensors: too short"))?;
    let header = bytes
        .get(8..8usize.saturating_add(len))
        .ok_or_else(|| bad("model.safetensors: truncated header"))?;
    let data = &bytes[8 + len..];
    let v: serde_json::Value =
        serde_json::from_slice(header).map_err(|e| bad(format!("model.safetensors: {e}")))?;
    let mut out = Vec::new();
    for (name, t) in v.as_object().into_iter().flatten() {
        if name == "__metadata__" {
            continue;
        }
        let dtype = t["dtype"].as_str().unwrap_or_default().to_string();
        let shape = t["shape"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_u64())
                    .map(|x| x as usize)
                    .collect()
            })
            .unwrap_or_default();
        let off = t["data_offsets"].as_array();
        let (a, b) = match off.map(|o| {
            (
                o.first().and_then(|x| x.as_u64()),
                o.get(1).and_then(|x| x.as_u64()),
            )
        }) {
            Some((Some(a), Some(b))) => (a as usize, b as usize),
            _ => return Err(bad(format!("model.safetensors: {name} has no offsets"))),
        };
        let data = data
            .get(a..b)
            .ok_or_else(|| bad(format!("model.safetensors: {name} is truncated")))?;
        out.push((name.clone(), Tensor { dtype, shape, data }));
    }
    Ok(out)
}

fn floats(t: &Tensor) -> Result<Vec<f32>> {
    let v: Vec<f32> = match t.dtype.as_str() {
        "F32" => t
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect(),
        "F16" => t
            .data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| f16_to_f32(u16::from_le_bytes(*c)))
            .collect(),
        "BF16" => t
            .data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| f32::from_bits((u16::from_le_bytes(*c) as u32) << 16))
            .collect(),
        other => return Err(bad(format!("model.safetensors: unsupported dtype {other}"))),
    };
    if v.len() != t.shape.iter().product::<usize>() {
        return Err(bad("model.safetensors: shape and data disagree"));
    }
    Ok(v)
}

fn ints(t: &Tensor) -> Result<Vec<u32>> {
    let v: Vec<u32> = match t.dtype.as_str() {
        "I64" => t
            .data
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| u64::from_le_bytes(*c) as u32)
            .collect(),
        "I32" => t
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect(),
        other => return Err(bad(format!("model.safetensors: mapping dtype {other}"))),
    };
    Ok(v)
}

impl StaticModel {
    /// From the bytes of `model.safetensors` and the text of
    /// `tokenizer.json`. `id` names it for stored vectors.
    pub fn from_bytes(id: &str, safetensors: &[u8], tokenizer_json: &str) -> Result<StaticModel> {
        let tokenizer = WordPiece::from_json(tokenizer_json)?;
        let ts = tensors(safetensors)?;
        let find = |n: &str| ts.iter().find(|(k, _)| k == n).map(|(_, t)| t);
        let emb = find("embeddings")
            .or_else(|| ts.first().map(|(_, t)| t))
            .ok_or_else(|| bad("model.safetensors: no embeddings"))?;
        if emb.shape.len() != 2 || emb.shape[1] == 0 {
            return Err(bad("model.safetensors: embeddings aren't a matrix"));
        }
        let (rows, dim) = (emb.shape[0], emb.shape[1]);
        let table = floats(emb)?;
        let weights = find("weights").map(floats).transpose()?;
        let mapping = find("mapping").map(ints).transpose()?;
        if mapping.is_none() && rows < tokenizer.vocab_len() {
            return Err(bad(
                "model.safetensors: fewer rows than the tokenizer has tokens",
            ));
        }
        Ok(StaticModel {
            id: id.to_string(),
            tokenizer,
            table,
            dim,
            rows,
            weights,
            mapping,
        })
    }

    /// From a directory holding `model.safetensors` and `tokenizer.json`.
    pub fn load(id: &str, dir: &Path) -> Result<StaticModel> {
        let read = |n: &str| std::fs::read(dir.join(n)).map_err(|e| bad(format!("{n}: {e}")));
        let st = read("model.safetensors")?;
        let tok = String::from_utf8(read("tokenizer.json")?)
            .map_err(|_| bad("tokenizer.json isn't UTF-8"))?;
        Self::from_bytes(id, &st, &tok)
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    /// One text's vector (zero when no token of it is known).
    pub fn embed_one(&self, text: &str) -> Vec<f32> {
        let mut acc = vec![0f32; self.dim];
        let mut total = 0f32;
        let unk = self.tokenizer.unk_id();
        for id in self.tokenizer.encode(text, MAX_TOKENS) {
            if Some(id) == unk {
                continue;
            }
            let row = match &self.mapping {
                Some(m) => match m.get(id as usize) {
                    Some(r) => *r as usize,
                    None => continue,
                },
                None => id as usize,
            };
            if row >= self.rows {
                continue;
            }
            let w = self
                .weights
                .as_ref()
                .and_then(|w| w.get(id as usize).copied())
                .unwrap_or(1.0);
            let v = &self.table[row * self.dim..(row + 1) * self.dim];
            for (a, x) in acc.iter_mut().zip(v) {
                *a += w * x;
            }
            total += 1.0;
        }
        if total > 0.0 {
            acc.iter_mut().for_each(|a| *a /= total);
        }
        normalize(&mut acc);
        acc
    }
}

impl Embedder for StaticModel {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.embed_one(t)).collect())
    }

    fn batch(&self) -> usize {
        256
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{safetensors_f32, tokenizer_json};
    use crate::vector::dot;

    /// Four tokens in two dimensions: tide and pool point one way, garden
    /// the other.
    fn model() -> StaticModel {
        let tok = tokenizer_json(&["[PAD]", "[UNK]", "tide", "pool", "##s", "garden"]);
        let table = [
            0.0, 0.0, // [PAD]
            9.0, 9.0, // [UNK]: never counted
            1.0, 0.1, // tide
            0.9, 0.0, // pool
            0.0, 0.0, // ##s
            0.0, 1.0, // garden
        ];
        StaticModel::from_bytes("test", &safetensors_f32("embeddings", 6, 2, &table), &tok).unwrap()
    }

    #[test]
    fn mean_pools_known_tokens_and_normalises() {
        let m = model();
        let v = m.embed_one("Tide pools?! qqq");
        assert!((dot(&v, &v) - 1.0).abs() < 1e-5);
        let g = m.embed_one("garden");
        assert!(dot(&v, &g) < 0.2);
        assert!(dot(&v, &m.embed_one("pool")) > 0.95);
        // Nothing known: a zero vector, similar to nothing.
        assert_eq!(m.embed_one("qqq zzz"), vec![0.0, 0.0]);
    }

    #[test]
    fn reads_f16_and_refuses_short_tables() {
        let tok = tokenizer_json(&["[UNK]", "a", "b"]);
        let mut st = Vec::new();
        let header = br#"{"embeddings":{"dtype":"F16","shape":[3,1],"data_offsets":[0,6]}}"#;
        st.extend_from_slice(&(header.len() as u64).to_le_bytes());
        st.extend_from_slice(header);
        for x in [0.0f32, 1.0, -1.0] {
            st.extend_from_slice(&crate::vector::f32_to_f16(x).to_le_bytes());
        }
        let m = StaticModel::from_bytes("t", &st, &tok).unwrap();
        assert_eq!(m.embed_one("b"), vec![-1.0]);
        let short = safetensors_f32("embeddings", 2, 1, &[0.0, 1.0]);
        assert!(StaticModel::from_bytes("t", &short, &tok).is_err());
        assert!(StaticModel::from_bytes("t", b"xx", &tok).is_err());
    }
}
