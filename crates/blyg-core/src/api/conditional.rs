//! Conditional reads (studio 0.65, contract 5.14.0, studio#152).
//!
//! A studio 0.65+ answers every private `GET` (except the deprecated `GET
//! /api/changes`) with an `ETag`, and a bodyless `304 Not Modified` when the
//! request carries that ETag in `If-None-Match` and nothing changed. There is
//! no capability flag: a studio offers it when its answers carry an `ETag`
//! (an older one sends none, and is asked as before).
//!
//! The rule that keeps this safe: **a 304 is never read as an empty answer.**
//! Each URL's last `200` body is kept here, beside its ETag, and a 304 hands
//! that body back, byte for byte. The caller can't tell a 304 from a 200 with
//! the same body, so nothing it would not have pruned on a 200 is pruned on a
//! 304. A 304 for a URL with no body kept (one Burrow didn't ask
//! conditionally) is asked again without a validator.
//!
//! Kept in memory only, per [`Api`](super::Api) (one credential, one blyg):
//! a new launch asks its first pull unconditionally, which is also when the
//! safety net reads everything whole. ETags are opaque and cover the exact
//! URL, the cursor and query included, so the key is the whole URL.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The most body text kept, across every URL. Past it, the least recently
/// used go first.
pub const KEEP_BYTES: usize = 32 * 1024 * 1024;

/// A body bigger than this isn't kept (it is asked whole every time).
pub const KEEP_BODY_MAX: usize = 8 * 1024 * 1024;

/// One URL's last `200`: its validator and its body.
#[derive(Debug, Clone)]
pub struct Kept {
    pub etag: String,
    pub body: Arc<String>,
}

/// Each URL's last `200`, the least recently used dropped past
/// [`KEEP_BYTES`]: an [`Api`](super::Api)'s, and Burrow in a browser's
/// (`crates/blyg-web/src/live.rs`, whose page does the fetching).
#[derive(Debug, Default)]
pub struct Validators {
    map: HashMap<String, (Kept, u64)>,
    bytes: usize,
    tick: u64,
}

impl Validators {
    /// The body kept for `url`, if any (and mark it used).
    pub fn get(&mut self, url: &str) -> Option<Kept> {
        self.tick += 1;
        let tick = self.tick;
        self.map.get_mut(url).map(|(k, used)| {
            *used = tick;
            k.clone()
        })
    }

    /// Keep `body` for `url` under `etag` (replacing what was kept).
    pub fn put(&mut self, url: &str, etag: &str, body: &str) {
        self.forget(url);
        if body.len() > KEEP_BODY_MAX || etag.is_empty() {
            return;
        }
        self.tick += 1;
        self.bytes += body.len();
        self.map.insert(
            url.to_string(),
            (
                Kept {
                    etag: etag.to_string(),
                    body: Arc::new(body.to_string()),
                },
                self.tick,
            ),
        );
        while self.bytes > KEEP_BYTES {
            let Some(oldest) = self
                .map
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(u, _)| u.clone())
            else {
                break;
            };
            self.forget(&oldest);
        }
    }

    /// Drop what's kept for `url`.
    pub fn forget(&mut self, url: &str) {
        if let Some((k, _)) = self.map.remove(url) {
            self.bytes -= k.body.len();
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.map.len()
    }
}

/// What this `Api` has asked the blyg, for measuring sync's cost.
#[derive(Debug, Default)]
pub(crate) struct Counters {
    requests: AtomicU64,
    not_modified: AtomicU64,
    body_bytes: AtomicU64,
    etags: AtomicU64,
}

/// A snapshot of [`Counters`] (`Api::traffic`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Traffic {
    /// Requests sent.
    pub requests: u64,
    /// Of them, answered `304 Not Modified`.
    pub not_modified: u64,
    /// Response body bytes read (after any content decoding).
    pub body_bytes: u64,
    /// `200`s that carried an `ETag`.
    pub etags: u64,
}

impl Traffic {
    /// What happened between `earlier` and this.
    pub fn since(&self, earlier: &Traffic) -> Traffic {
        Traffic {
            requests: self.requests - earlier.requests,
            not_modified: self.not_modified - earlier.not_modified,
            body_bytes: self.body_bytes - earlier.body_bytes,
            etags: self.etags - earlier.etags,
        }
    }
}

impl Counters {
    pub fn note(&self, status: u16, body: usize, etag: bool) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.body_bytes.fetch_add(body as u64, Ordering::Relaxed);
        if status == 304 {
            self.not_modified.fetch_add(1, Ordering::Relaxed);
        } else if etag {
            self.etags.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Whether any answer so far carried an `ETag` (the studio offers
    /// conditional reads).
    pub fn etags_seen(&self) -> bool {
        self.etags.load(Ordering::Relaxed) > 0 || self.not_modified.load(Ordering::Relaxed) > 0
    }

    pub fn snapshot(&self) -> Traffic {
        Traffic {
            requests: self.requests.load(Ordering::Relaxed),
            not_modified: self.not_modified.load(Ordering::Relaxed),
            body_bytes: self.body_bytes.load(Ordering::Relaxed),
            etags: self.etags.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_body_comes_back_whole_with_its_etag() {
        let mut v = Validators::default();
        assert!(v.get("u").is_none());
        v.put("u", "\"e1\"", "{\"items\":[1,2]}");
        let k = v.get("u").unwrap();
        assert_eq!(k.etag, "\"e1\"");
        assert_eq!(&*k.body, "{\"items\":[1,2]}");
        v.put("u", "\"e2\"", "{}");
        assert_eq!(v.get("u").unwrap().etag, "\"e2\"");
        assert_eq!(v.bytes, 2);
        v.forget("u");
        assert!(v.get("u").is_none());
        assert_eq!(v.bytes, 0);
    }

    #[test]
    fn the_least_recently_used_go_first_past_the_cap() {
        let mut v = Validators::default();
        // Five of these are past the cap; each is under `KEEP_BODY_MAX`.
        let big = "x".repeat(KEEP_BYTES / 5 + 1);
        assert!(big.len() <= KEEP_BODY_MAX);
        for u in ["a", "b", "c", "d"] {
            v.put(u, u, &big);
        }
        // `a` used last: `b` is the oldest when `e` overflows.
        v.get("a");
        v.put("e", "e", &big);
        assert!(v.get("a").is_some());
        assert!(v.get("b").is_none());
        assert!(v.get("e").is_some());
        assert_eq!(v.len(), 4);
        assert!(v.bytes <= KEEP_BYTES);
    }

    #[test]
    fn an_oversized_body_or_an_empty_etag_isnt_kept() {
        let mut v = Validators::default();
        v.put("a", "1", &"x".repeat(KEEP_BODY_MAX + 1));
        v.put("b", "", "{}");
        assert_eq!(v.len(), 0);
        assert_eq!(v.bytes, 0);
    }
}
