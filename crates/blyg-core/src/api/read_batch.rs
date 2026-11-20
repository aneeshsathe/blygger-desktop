//! `POST /api/read-batch` (`batchRead`, studio 0.52+, contract 5.3.0):
//! up to [`READ_BATCH_MAX`] reads of one operation (`getItem`,
//! `listMentions`, `listMentionGroups`, `getMentionSummary`) in one request.
//!
//! Studio 0.65 (contract 5.14.0) makes each child conditional, like a plain
//! `GET`: a child sent with `ifNoneMatch` comes back `{status: 304, body:
//! null}` when nothing changed. The bodies and ETags are the ones a plain
//! `GET` of the same URL gets, so they're kept in the same place
//! (`conditional`), and a child's 304 hands back the kept body, never
//! nothing. A child is sent with `ifNoneMatch` only when an ETag is kept for
//! its URL, which only a 0.65+ studio gives: an older one's strict child
//! schema would refuse the key.

use serde_json::{Map, Value, json};

use super::{Api, enc, is_transient};
use crate::backend::{CoreError, Result};

/// The most children one batch takes.
pub const READ_BATCH_MAX: usize = 50;

/// The capability `operations` name of `POST /api/read-batch`.
pub const READ_BATCH_OPERATION: &str = "batchRead";

/// One child read: the operation's path parameters and query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchRead {
    pub path: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
}

/// One child's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchAnswer {
    /// The child's status; a 304 is reported as the kept body's 200.
    pub status: u16,
    /// The body (the kept one, for a 304).
    pub body: Value,
    /// The blyg said nothing changed (304).
    pub not_modified: bool,
}

impl Api {
    /// Whether this studio's capabilities list `batchRead`. A failed read
    /// is an error; one without capabilities, false.
    pub fn serves_read_batch(&self) -> Result<bool> {
        match self.call("GET", "/api/capabilities", None) {
            Ok(v) => Ok(match v.get("operations") {
                Some(Value::Array(a)) => a.iter().any(|o| o == READ_BATCH_OPERATION),
                Some(Value::Object(m)) => m.contains_key(READ_BATCH_OPERATION),
                _ => false,
            }),
            Err(e) if is_transient(&e) || matches!(e, CoreError::Unauthorized) => Err(e),
            Err(_) => Ok(false),
        }
    }

    /// Read `children` of `operation` (whose route is `template`, e.g.
    /// `/items/{id}`) in batches of [`READ_BATCH_MAX`]; one answer per
    /// child, in order. Each child carries the ETag kept for its URL.
    pub fn read_batch(
        &self,
        operation: &str,
        template: &str,
        children: &[BatchRead],
    ) -> Result<Vec<BatchAnswer>> {
        let mut out = Vec::with_capacity(children.len());
        for chunk in children.chunks(READ_BATCH_MAX) {
            let urls: Vec<String> = chunk.iter().map(|c| self.child_url(template, c)).collect();
            let kept: Vec<_> = urls.iter().map(|u| self.kept_etag(u)).collect();
            let requests: Vec<Value> = chunk
                .iter()
                .zip(&kept)
                .map(|(c, k)| {
                    let pairs = |v: &[(String, String)]| {
                        Value::Object(
                            v.iter()
                                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                                .collect::<Map<_, _>>(),
                        )
                    };
                    let mut r = json!({});
                    if !c.path.is_empty() {
                        r["path"] = pairs(&c.path);
                    }
                    if !c.query.is_empty() {
                        r["query"] = pairs(&c.query);
                    }
                    if let Some(k) = k {
                        r["ifNoneMatch"] = Value::String(k.etag.clone());
                    }
                    r
                })
                .collect();
            let v = self.call(
                "POST",
                "/api/read-batch",
                Some(json!({ "operation": operation, "requests": requests })),
            )?;
            let answers = v
                .as_array()
                .filter(|a| a.len() == chunk.len())
                .ok_or_else(|| {
                    CoreError::Other("unexpected response from /api/read-batch".into())
                })?;
            for ((a, url), k) in answers.iter().zip(&urls).zip(kept) {
                let status = a.get("status").and_then(Value::as_u64).unwrap_or(0) as u16;
                let etag = a
                    .get("headers")
                    .and_then(Value::as_object)
                    .and_then(|h| h.iter().find(|(n, _)| n.eq_ignore_ascii_case("etag")))
                    .and_then(|(_, v)| v.as_str());
                let body = a.get("body").cloned().unwrap_or(Value::Null);
                out.push(match (status, k) {
                    // A 304 with the body kept for it: that body. One with
                    // none kept (or naming another validator) is an error
                    // for that child, never an empty answer.
                    (304, Some(k)) if etag.is_none_or(|e| e == k.etag) => BatchAnswer {
                        status: 200,
                        body: super::json_of(&k.body)?,
                        not_modified: true,
                    },
                    (304, _) => {
                        self.forget_etag(url);
                        BatchAnswer {
                            status: 304,
                            body: Value::Null,
                            not_modified: false,
                        }
                    }
                    (200, _) => {
                        match etag {
                            Some(e) => self.remember_etag(url, e, &body.to_string()),
                            None => self.forget_etag(url),
                        }
                        BatchAnswer {
                            status,
                            body,
                            not_modified: false,
                        }
                    }
                    _ => BatchAnswer {
                        status,
                        body,
                        not_modified: false,
                    },
                });
            }
        }
        Ok(out)
    }

    /// The URL a plain `GET` of this child would use (the key its ETag is
    /// kept under).
    fn child_url(&self, template: &str, c: &BatchRead) -> String {
        let mut path = template.to_string();
        for (k, v) in &c.path {
            path = path.replace(&format!("{{{k}}}"), &enc(v));
        }
        let mut url = format!("{}{path}", self.api_url());
        let query: Vec<String> = c
            .query
            .iter()
            .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
            .collect();
        if !query.is_empty() {
            url.push('?');
            url.push_str(&query.join("&"));
        }
        url
    }
}
