//! --- conditional reads --- The owner API's ETags (studio 0.65, contract
//! 5.14.0): every private resource GET answers with an `ETag`, and a later
//! GET that sends it back as `If-None-Match` gets a bodyless `304` while
//! the representation is unchanged. [`Validators`] keeps, per URL, the
//! validator and the body it came with, so a 304 is answered from here.
//! Validators are opaque and sent back unchanged.
//!
//! Studio 0.65 deprecates the change counters (`GET /api/changes`) that the
//! sync engine uses to skip reads, and 0.67 removes them. Without them every
//! pull reads every list whole; with validators, each unchanged page is one
//! small 304 instead of its whole body. An older studio sends no `ETag`, so
//! nothing is kept and every read is as before.
//!
//! The bodies are bounded ([`MAX_BYTES`] in all, [`MAX_ENTRY`] each), and
//! the oldest go first. A dropped entry costs one whole read, never a wrong
//! answer: a 304 is only ever asked for with a body to reuse, and one that
//! comes back anyway (the entry went meanwhile) is read again without a
//! validator. The cache lives as long as its `Api` and is never written to
//! disk, since it holds private drafts.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// The most body text kept across all URLs.
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
/// A body larger than this isn't kept (it's read whole each time).
pub const MAX_ENTRY: usize = 4 * 1024 * 1024;

struct Entry {
    etag: String,
    body: Arc<str>,
}

/// Validators and bodies by absolute URL, oldest first.
#[derive(Default)]
pub struct Validators {
    entries: HashMap<String, Entry>,
    order: VecDeque<String>,
    bytes: usize,
    /// Reads answered with a 304 (diagnostics and tests).
    revalidated: u64,
}

impl Validators {
    /// The validator to send for `url`, when its body is kept.
    pub fn etag(&self, url: &str) -> Option<String> {
        self.entries.get(url).map(|e| e.etag.clone())
    }

    /// A 304 for `url`: the kept body, if it's still here.
    pub fn not_modified(&mut self, url: &str) -> Option<Arc<str>> {
        let body = self.entries.get(url)?.body.clone();
        self.revalidated += 1;
        // Recently used: last to go.
        if let Some(i) = self.order.iter().position(|u| u == url) {
            let u = self.order.remove(i).expect("position is in range");
            self.order.push_back(u);
        }
        Some(body)
    }

    /// A 200 for `url`: keep its validator and body, or, without a
    /// validator (an older studio) or when it's too large, forget any
    /// earlier one.
    pub fn store(&mut self, url: &str, etag: Option<&str>, body: &str) {
        self.forget(url);
        let Some(etag) = etag.map(str::trim).filter(|e| !e.is_empty()) else {
            return;
        };
        if body.len() > MAX_ENTRY {
            return;
        }
        while self.bytes + body.len() > MAX_BYTES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(e) = self.entries.remove(&oldest) {
                self.bytes -= e.body.len();
            }
        }
        self.bytes += body.len();
        self.order.push_back(url.to_string());
        self.entries.insert(
            url.to_string(),
            Entry {
                etag: etag.to_string(),
                body: body.into(),
            },
        );
    }

    /// Drop what's kept for `url`.
    pub fn forget(&mut self, url: &str) {
        if let Some(e) = self.entries.remove(url) {
            self.bytes -= e.body.len();
            self.order.retain(|u| u != url);
        }
    }

    /// How many reads were answered with a 304 so far.
    pub fn revalidated(&self) -> u64 {
        self.revalidated
    }

    #[cfg(test)]
    fn bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "https://blyg.example.com/api/items?limit=100";
    const B: &str = "https://blyg.example.com/api/subscriptions?limit=100";

    #[test]
    fn a_validated_body_answers_a_304() {
        let mut v = Validators::default();
        assert_eq!(v.etag(A), None, "nothing to send before a read");
        v.store(A, Some("W/\"abc\""), "{\"items\":[]}");
        assert_eq!(
            v.etag(A).as_deref(),
            Some("W/\"abc\""),
            "sent back unchanged"
        );
        assert_eq!(v.not_modified(A).as_deref(), Some("{\"items\":[]}"));
        assert_eq!(v.revalidated(), 1);
    }

    #[test]
    fn an_answer_without_a_validator_forgets_the_old_one() {
        let mut v = Validators::default();
        v.store(A, Some("W/\"abc\""), "old");
        // An older studio (or a proxy) sends no ETag: never reuse "old".
        v.store(A, None, "new");
        assert_eq!(v.etag(A), None);
        assert_eq!(v.not_modified(A), None);
        assert_eq!(v.bytes(), 0);
    }

    #[test]
    fn a_304_with_nothing_kept_is_not_an_answer() {
        let mut v = Validators::default();
        assert_eq!(v.not_modified(A), None);
        assert_eq!(v.revalidated(), 0);
    }

    #[test]
    fn bodies_stay_within_the_bound_and_the_oldest_go_first() {
        let mut v = Validators::default();
        let big = "x".repeat(MAX_ENTRY);
        let urls: Vec<String> = (0..MAX_BYTES / MAX_ENTRY + 2)
            .map(|i| format!("{A}&cursor={i}"))
            .collect();
        for u in &urls {
            v.store(u, Some("W/\"1\""), &big);
            assert!(v.bytes() <= MAX_BYTES);
        }
        assert_eq!(v.etag(&urls[0]), None, "the oldest went first");
        assert!(v.etag(urls.last().unwrap()).is_some());
        // Too large to keep: read whole each time.
        v.store(B, Some("W/\"2\""), &"y".repeat(MAX_ENTRY + 1));
        assert_eq!(v.etag(B), None);
    }

    #[test]
    fn a_reused_body_is_kept_longer_than_one_not_reused() {
        let mut v = Validators::default();
        let big = "x".repeat(MAX_ENTRY);
        let n = MAX_BYTES / MAX_ENTRY;
        let urls: Vec<String> = (0..n).map(|i| format!("{A}&cursor={i}")).collect();
        for u in &urls {
            v.store(u, Some("W/\"1\""), &big);
        }
        // The first is used again, so the second is now the oldest.
        assert!(v.not_modified(&urls[0]).is_some());
        v.store(B, Some("W/\"2\""), &big);
        assert!(v.etag(&urls[0]).is_some());
        assert_eq!(v.etag(&urls[1]), None);
    }
}
