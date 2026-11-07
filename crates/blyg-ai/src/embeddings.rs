//! --- nearby --- Embeddings from a server the user runs: Ollama
//! (`POST /api/embed`) or anything OpenAI-compatible (`POST …/embeddings`,
//! LM Studio, llama.cpp's server, vLLM). Configured by `nearby-endpoint`;
//! nothing is ever sent unless the user set it. No credential is sent.

use serde_json::{Value, json};

use crate::error::{AiError, Result};
use crate::http;

/// Which protocol an endpoint speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// `POST {base}/api/embed` → `{embeddings: [[…]]}`.
    Ollama,
    /// `POST {base}/embeddings` → `{data: [{index, embedding}]}`.
    OpenAi,
}

/// An embeddings endpoint and the model to ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingsEndpoint {
    pub url: String,
    pub model: String,
}

impl EmbeddingsEndpoint {
    pub fn new(url: &str, model: &str) -> Self {
        EmbeddingsEndpoint {
            url: url.trim().trim_end_matches('/').to_string(),
            model: model.trim().to_string(),
        }
    }

    /// An address with a `/v1` segment, or ending in `/embeddings`, is
    /// OpenAI-compatible; any other is Ollama.
    pub fn flavor(&self) -> Flavor {
        let path = url::Url::parse(&self.url)
            .map(|u| u.path().to_string())
            .unwrap_or_default();
        if path.ends_with("/embeddings") || path.split('/').any(|s| s == "v1") {
            Flavor::OpenAi
        } else {
            Flavor::Ollama
        }
    }

    /// Where the request goes.
    pub fn request_url(&self) -> String {
        match self.flavor() {
            Flavor::Ollama if self.url.ends_with("/api/embed") => self.url.clone(),
            Flavor::Ollama => format!("{}/api/embed", self.url),
            Flavor::OpenAi if self.url.ends_with("/embeddings") => self.url.clone(),
            Flavor::OpenAi => format!("{}/embeddings", self.url),
        }
    }

    /// The request body for `texts`.
    pub fn body(&self, texts: &[&str]) -> Value {
        json!({"model": self.model, "input": texts})
    }

    /// One vector per text from a response body.
    pub fn parse(&self, v: &Value, n: usize) -> Result<Vec<Vec<f32>>> {
        let as_vec = |e: &Value| -> Option<Vec<f32>> {
            e.as_array()?
                .iter()
                .map(|x| x.as_f64().map(|f| f as f32))
                .collect()
        };
        let out: Option<Vec<Vec<f32>>> = match self.flavor() {
            Flavor::Ollama => v["embeddings"]
                .as_array()
                .map(|a| a.iter().map(as_vec).collect::<Option<Vec<_>>>())
                .unwrap_or(None),
            Flavor::OpenAi => v["data"].as_array().and_then(|a| {
                let mut rows: Vec<(u64, Vec<f32>)> = a
                    .iter()
                    .enumerate()
                    .map(|(i, d)| {
                        Some((
                            d["index"].as_u64().unwrap_or(i as u64),
                            as_vec(&d["embedding"])?,
                        ))
                    })
                    .collect::<Option<Vec<_>>>()?;
                rows.sort_by_key(|(i, _)| *i);
                Some(rows.into_iter().map(|(_, e)| e).collect())
            }),
        };
        match out {
            Some(o) if o.len() == n && o.iter().all(|e| !e.is_empty()) => Ok(o),
            _ => Err(AiError::Provider(
                "the embeddings endpoint's answer had no embeddings in it".into(),
            )),
        }
    }

    /// Embed `texts` (blocking).
    pub fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let res = http::agent()
            .post(&self.request_url())
            .set("content-type", "application/json")
            .send_json(self.body(texts));
        let v = http::read_json("The embeddings endpoint", res)?;
        self.parse(&v, texts.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaks_ollama_or_openai_by_address() {
        let o = EmbeddingsEndpoint::new("http://localhost:11434/", "nomic-embed-text");
        assert_eq!(o.flavor(), Flavor::Ollama);
        assert_eq!(o.request_url(), "http://localhost:11434/api/embed");
        let a = EmbeddingsEndpoint::new("http://localhost:1234/v1", "m");
        assert_eq!(a.flavor(), Flavor::OpenAi);
        assert_eq!(a.request_url(), "http://localhost:1234/v1/embeddings");
        let b = EmbeddingsEndpoint::new("http://127.0.0.1:8080/v1/embeddings", "m");
        assert_eq!(b.request_url(), "http://127.0.0.1:8080/v1/embeddings");
        assert_eq!(a.body(&["x"]), json!({"model": "m", "input": ["x"]}));
    }

    #[test]
    fn parses_both_answers() {
        let o = EmbeddingsEndpoint::new("http://localhost:11434", "m");
        let v = json!({"embeddings": [[0.5, 0.5], [1.0, 0.0]]});
        assert_eq!(
            o.parse(&v, 2).unwrap(),
            vec![vec![0.5, 0.5], vec![1.0, 0.0]]
        );
        assert!(o.parse(&v, 3).is_err());
        let a = EmbeddingsEndpoint::new("http://localhost:1234/v1", "m");
        let v =
            json!({"data": [{"index": 1, "embedding": [2.0]}, {"index": 0, "embedding": [1.0]}]});
        assert_eq!(a.parse(&v, 2).unwrap(), vec![vec![1.0], vec![2.0]]);
        assert!(a.parse(&json!({"error": "no"}), 1).is_err());
    }
}
