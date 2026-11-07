//! --- nearby --- Nearby thoughts without pixels: the paragraph under the
//! caret, its distinctive phrases, what the rail may offer (the corpus),
//! the in-memory vector index, and the ranking. Everything here runs off
//! the main thread and is pure apart from the `Backend` and `Embedder` it
//! is handed, so the tests drive it on a real store.
//!
//! The ranking: with an embedder, each post's best piece by cosine to the
//! paragraph (its title only, with `nearby = titles`), above a floor; a
//! post that also shares a phrase with the paragraph gains a little, and
//! the phrase is what ⌥⏎ links. With no embedder (or while it's unready),
//! phrase matches alone, through the store's trigram indexes: a phrase in
//! the title, or in your own post, or of three words or more in anyone's.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use blyg_core::nearby::{
    NearbyVector, offers_own, offers_reading, own_key, reading_key, text_hash,
};
use blyg_core::pick;
use blyg_core::{Backend, Item, Kind, ReadingItem, Subscription, SubscriptionKind};
use blyg_embed::{Embedder, chunk, vector};

/// Rows the rail shows.
pub const SHOWN: usize = 5;
/// Candidates kept for a re-rank.
pub const POOL: usize = 10;
/// At most this many phrases of a paragraph are looked up.
pub const MAX_PHRASES: usize = 12;
/// A paragraph shorter than this (characters of words) asks nothing.
pub const MIN_PARAGRAPH: usize = 12;

const STOP: &[&str] = &[
    "a", "about", "above", "after", "again", "all", "also", "am", "an", "and", "any", "are", "as",
    "at", "be", "because", "been", "before", "being", "but", "by", "can", "could", "did", "do",
    "does", "doing", "don't", "down", "during", "each", "even", "every", "few", "for", "from",
    "further", "had", "has", "have", "having", "he", "her", "here", "hers", "him", "his", "how",
    "i", "i'm", "if", "in", "into", "is", "isn't", "it", "it's", "its", "just", "me", "more",
    "most", "much", "my", "no", "nor", "not", "now", "of", "off", "on", "once", "one", "only",
    "or", "other", "our", "ours", "out", "over", "own", "same", "she", "should", "so", "some",
    "still", "such", "than", "that", "that's", "the", "their", "them", "then", "there", "these",
    "they", "this", "those", "through", "to", "too", "under", "until", "up", "us", "very", "was",
    "we", "were", "what", "when", "where", "which", "while", "who", "whom", "why", "will", "with",
    "would", "you", "your", "yours",
];

pub fn is_stop(w: &str) -> bool {
    STOP.contains(&w)
}

// ------------------------------------------------------------ the paragraph

/// The paragraph around byte `cursor` of `text` (lines between blank
/// lines), trimmed; `None` on a blank line.
pub fn paragraph_at(text: &str, cursor: usize) -> Option<Range<usize>> {
    let cursor = cursor.min(text.len());
    let cursor = (0..=cursor).rev().find(|i| text.is_char_boundary(*i))?;
    let line_start = |at: usize| text[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_end = |at: usize| text[at..].find('\n').map_or(text.len(), |i| at + i);
    let blank = |s: usize, e: usize| text[s..e].trim().is_empty();
    let (ls, le) = (line_start(cursor), line_end(cursor));
    if blank(ls, le) {
        return None;
    }
    let mut start = ls;
    while start > 0 {
        let prev_end = start - 1;
        let ps = line_start(prev_end);
        if blank(ps, prev_end) {
            break;
        }
        start = ps;
    }
    let mut end = le;
    while end < text.len() {
        let ne = line_end(end + 1);
        if blank(end + 1, ne) {
            break;
        }
        end = ne;
    }
    // Trim to the words.
    let s = &text[start..end];
    let lead = s.len() - s.trim_start().len();
    let trail = s.len() - s.trim_end().len();
    Some(start + lead..end - trail)
}

/// Byte ranges of `text` that are markup, not words: links and quotes
/// (`[[…]]`, `![[…]]`), code spans, Markdown link targets and bare URLs.
fn masked(text: &str) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = blyg_core::grammar::link_refs(text)
        .into_iter()
        .map(|r| {
            let s = if text[..r.range.start].ends_with('!') {
                r.range.start - 1
            } else {
                r.range.start
            };
            s..r.range.end
        })
        .collect();
    let mut tick: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if c == '`' {
            match tick.take() {
                Some(s) => out.push(s..i + 1),
                None => tick = Some(i),
            }
        }
    }
    let mut from = 0;
    while let Some(i) = text[from..].find("](") {
        let s = from + i;
        let e = text[s..].find(')').map_or(text.len(), |j| s + j + 1);
        out.push(s..e);
        from = e.max(s + 2);
    }
    for pre in ["http://", "https://"] {
        let mut from = 0;
        while let Some(i) = text[from..].find(pre) {
            let s = from + i;
            let e = text[s..]
                .find(char::is_whitespace)
                .map_or(text.len(), |j| s + j);
            out.push(s..e);
            from = e;
        }
    }
    out
}

/// A word of the paragraph: its byte range and lowercase form.
#[derive(Debug, Clone, PartialEq)]
struct Word {
    range: Range<usize>,
    lc: String,
}

fn words(text: &str, para: &Range<usize>) -> Vec<Option<Word>> {
    // `None` marks a break (punctuation, markup): phrases don't cross it.
    let mask = masked(text);
    let in_mask = |i: usize| mask.iter().any(|r| r.contains(&i));
    let mut out: Vec<Option<Word>> = Vec::new();
    let mut cur: Option<usize> = None;
    let s = &text[para.clone()];
    let flush = |out: &mut Vec<Option<Word>>, a: usize, b: usize| {
        let w = text[a..b].trim_matches(|c| c == '\'' || c == '’' || c == '-');
        if !w.is_empty() {
            let off = text[a..b].find(w).unwrap_or(0);
            out.push(Some(Word {
                range: a + off..a + off + w.len(),
                lc: w.to_lowercase().replace('’', "'"),
            }));
        }
    };
    for (i, c) in s.char_indices() {
        let at = para.start + i;
        let word_char = c.is_alphanumeric() || c == '\'' || c == '’' || c == '-';
        if word_char && !in_mask(at) {
            if cur.is_none() {
                cur = Some(at);
            }
            continue;
        }
        if let Some(a) = cur.take() {
            flush(&mut out, a, at);
        }
        if !c.is_whitespace() && !matches!(out.last(), Some(None)) {
            out.push(None);
        }
    }
    if let Some(a) = cur {
        flush(&mut out, a, para.end);
    }
    out
}

/// A phrase of the paragraph worth looking up.
#[derive(Debug, Clone, PartialEq)]
pub struct Phrase {
    /// Lowercase, single-spaced: what's searched for.
    pub needle: String,
    /// Where it is in the editor's text.
    pub range: Range<usize>,
    pub words: usize,
}

/// The paragraph's distinctive 2–4-word phrases: neither end a stop word,
/// no punctuation or markup inside, separated only by spaces, at least one
/// word of four letters. Longest and most distinctive first, at most
/// [`MAX_PHRASES`], no two alike.
pub fn phrases(text: &str, para: &Range<usize>) -> Vec<Phrase> {
    let ws = words(text, para);
    let mut out: Vec<(usize, Phrase)> = Vec::new();
    for i in 0..ws.len() {
        for n in 2..=4 {
            let Some(span) = ws.get(i..i + n) else { break };
            let Some(span): Option<Vec<&Word>> = span.iter().map(|w| w.as_ref()).collect() else {
                break;
            };
            let (first, last) = (span[0], span[n - 1]);
            if is_stop(&first.lc) || is_stop(&last.lc) {
                continue;
            }
            if !span.iter().any(|w| w.lc.chars().count() >= 4) {
                continue;
            }
            // Only whitespace between the words (one line).
            let gaps_ok = span.windows(2).all(|p| {
                let gap = &text[p[0].range.end..p[1].range.start];
                !gap.is_empty() && gap.chars().all(|c| c == ' ' || c == '\t')
            });
            if !gaps_ok {
                continue;
            }
            let needle = span
                .iter()
                .map(|w| w.lc.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let weight = span
                .iter()
                .filter(|w| !is_stop(&w.lc))
                .map(|w| w.lc.chars().count() + 4)
                .sum::<usize>();
            out.push((
                weight,
                Phrase {
                    needle,
                    range: first.range.start..last.range.end,
                    words: n,
                },
            ));
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.range.start.cmp(&b.1.range.start)));
    let mut seen = HashSet::new();
    out.into_iter()
        .map(|(_, p)| p)
        .filter(|p| seen.insert(p.needle.clone()))
        .take(MAX_PHRASES)
        .collect()
}

/// The paragraph as plain words, what's embedded (markup dropped).
pub fn paragraph_plain(text: &str, para: &Range<usize>) -> String {
    let mask = masked(text);
    let mut out = String::new();
    let mut last = para.start;
    let mut cuts: Vec<Range<usize>> = mask
        .into_iter()
        .filter(|r| r.end > para.start && r.start < para.end)
        .collect();
    cuts.sort_by_key(|r| r.start);
    for r in cuts {
        let s = r.start.max(para.start);
        if s > last {
            out.push_str(&text[last..s]);
        }
        last = last.max(r.end.min(para.end));
    }
    if last < para.end {
        out.push_str(&text[last..para.end]);
    }
    blyg_core::plain_text(&out)
}

// ------------------------------------------------------------ the corpus

/// A post the rail may offer.
#[derive(Debug, Clone, PartialEq)]
pub struct Doc {
    /// `own:<local_id>` or `reading:<sub>/<remote_id>`.
    pub key: String,
    /// What goes between the brackets.
    pub id: String,
    pub title: String,
    /// What the picker would label it.
    pub label: String,
    /// "You", or the blyg's name.
    pub who: String,
    pub own: bool,
    pub kind: Kind,
    pub at: String,
    /// Its readable text, paragraph by paragraph.
    pub text: String,
    /// `text_hash` of `title` + `text`.
    pub hash: String,
}

impl Doc {
    /// What's embedded: the title (piece 0), then the text's pieces.
    pub fn pieces(&self) -> Vec<String> {
        let mut v = vec![self.title.clone()];
        v.extend(chunk::chunks(&self.text));
        v
    }
}

/// A post's readable text with its paragraphs kept (`plain_text` line by
/// line, blank lines kept).
pub fn readable(md: &str) -> String {
    blyg_core::strip_tk(md)
        .lines()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                blyg_core::plain_text(l)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A stored piece: (chunk, text hash, vector).
type Row<'a> = (u32, &'a str, &'a [f32]);

/// A post's readable text without its title line (the title is its own
/// piece).
fn body_text(md: &str, title: &str) -> String {
    let text = readable(md);
    let t = text.trim_start();
    match t.split_once('\n') {
        Some((first, rest)) if first.trim() == title.trim() => rest.trim_start().to_string(),
        None if t.trim() == title.trim() => String::new(),
        _ => text,
    }
}

/// What a pick of the post is labelled ([`pick::pick_label`]): its title
/// when that's short enough, which is the common case and costs nothing;
/// else the text a bare link to it shows (rendered, so only then).
fn label(title: &str, shows: impl FnOnce() -> String) -> String {
    if !title.trim().is_empty() && title.chars().count() <= pick::TITLE_LABEL_CHARS {
        title.to_string()
    } else {
        pick::pick_label(Some(title), shows())
    }
}

/// Your posts the rail may offer.
pub fn own_docs(items: &[Item]) -> Vec<Doc> {
    items
        .iter()
        .filter(|i| offers_own(i))
        .filter_map(|i| {
            let id = i.server_id.as_ref()?.0.clone();
            let title = i.title();
            let text = body_text(&i.content_md, &title);
            Some(Doc {
                key: own_key(&i.local_id.0),
                id,
                label: label(&title, || pick::link_label_md(&i.content_md, i.kind)),
                hash: text_hash(&format!("{title}\n{text}")),
                title,
                who: "You".into(),
                own: true,
                kind: i.kind,
                at: i.updated.clone(),
                text,
            })
        })
        .collect()
}

fn host(origin: &str) -> String {
    url::Url::parse(origin)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| origin.trim_end_matches('/').to_string())
}

/// The held posts the rail may offer (one per id).
pub fn held_docs(reading: &[ReadingItem], subs: &[Subscription]) -> Vec<Doc> {
    let blyg: HashSet<&str> = subs
        .iter()
        .filter(|s| s.kind == SubscriptionKind::Blyg)
        .map(|s| s.id.as_str())
        .collect();
    let mut seen = HashSet::new();
    reading
        .iter()
        .filter(|r| offers_reading(r, blyg.contains(r.subscription_id.as_str())))
        .filter(|r| seen.insert(r.remote_id.to_lowercase()))
        .map(|r| {
            let title = blyg_core::plain_title(&r.content_md).unwrap_or_else(|| "…".to_string());
            let text = body_text(&r.content_md, &title);
            let who = r.subscription_title.trim();
            Doc {
                key: reading_key(&r.subscription_id, &r.remote_id),
                id: r.remote_id.clone(),
                label: label(&title, || {
                    if r.content_html.trim().is_empty() {
                        pick::link_label_md(&r.content_md, r.kind)
                    } else {
                        pick::link_label(&r.content_html, r.kind)
                    }
                }),
                hash: text_hash(&format!("{title}\n{text}")),
                title,
                who: if who.is_empty() {
                    host(&r.origin)
                } else {
                    who.to_string()
                },
                own: false,
                kind: r.kind,
                at: r.sort_at(),
                text,
            }
        })
        .collect()
}

/// Everything the rail may offer, yours first.
#[derive(Debug, Clone, Default)]
pub struct Corpus {
    pub own: Arc<Vec<Doc>>,
    pub held: Arc<Vec<Doc>>,
}

impl Corpus {
    pub fn docs(&self) -> impl Iterator<Item = &Doc> {
        self.own.iter().chain(self.held.iter())
    }

    pub fn by_key(&self) -> HashMap<&str, &Doc> {
        self.docs().map(|d| (d.key.as_str(), d)).collect()
    }
}

// ------------------------------------------------------------ the index

/// Every piece of every offered post, embedded, in memory: brute-force
/// cosine over it is a few milliseconds at ten thousand pieces.
#[derive(Debug, Clone, Default)]
pub struct Index {
    pub model: String,
    pub dim: usize,
    /// Per piece: its post's key index, piece number, text hash.
    pub meta: Vec<(u32, u32, String)>,
    pub keys: Vec<String>,
    /// Per post key: the `Doc::hash` it was embedded from.
    pub doc_hash: HashMap<String, String>,
    /// `meta.len() × dim`.
    pub vecs: Vec<f32>,
}

impl Index {
    pub fn pieces(&self) -> usize {
        self.meta.len()
    }

    fn push(&mut self, key_ix: u32, chunk: u32, hash: String, v: &[f32]) {
        if self.dim == 0 {
            self.dim = v.len();
        }
        if v.len() != self.dim {
            return;
        }
        self.meta.push((key_ix, chunk, hash));
        self.vecs.extend_from_slice(v);
    }

    fn rows_of(&self) -> HashMap<&str, Vec<Row<'_>>> {
        let mut m: HashMap<&str, Vec<Row<'_>>> = HashMap::new();
        for (i, (k, c, h)) in self.meta.iter().enumerate() {
            m.entry(self.keys[*k as usize].as_str()).or_default().push((
                *c,
                h.as_str(),
                &self.vecs[i * self.dim..(i + 1) * self.dim],
            ));
        }
        m
    }

    /// The best `(post key, piece, score)` per post, best first.
    pub fn nearest(&self, q: &[f32], titles_only: bool, k: usize) -> Vec<(String, u32, f32)> {
        if self.dim == 0 || q.len() != self.dim {
            return Vec::new();
        }
        let mut best: HashMap<u32, (u32, f32)> = HashMap::new();
        for (i, (key, chunk, _)) in self.meta.iter().enumerate() {
            if titles_only && *chunk != 0 {
                continue;
            }
            let s = vector::dot(q, &self.vecs[i * self.dim..(i + 1) * self.dim]);
            let e = best.entry(*key).or_insert((*chunk, f32::MIN));
            if s > e.1 {
                *e = (*chunk, s);
            }
        }
        vector::top_k(best.iter().map(|(k, (_, s))| (*k as usize, *s)), k)
            .into_iter()
            .map(|(key, s)| (self.keys[key].clone(), best[&(key as u32)].0, s))
            .collect()
    }
}

/// A build's progress: pieces embedded so far (for the rail's footer).
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
}

/// What a build did, for the footer and the speed numbers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Built {
    pub posts: usize,
    pub pieces: usize,
    pub embedded: usize,
    pub millis: u128,
}

/// Bring `prev` (or what the store kept, when there's none for this
/// model) up to date with `corpus`: a post whose text is unchanged keeps
/// its vectors, a changed or new one is embedded again (only its changed
/// pieces), and a post no longer offered is dropped, here and in the
/// store. `cancelled` is asked between batches.
pub fn build_index(
    backend: &dyn Backend,
    embedder: &dyn Embedder,
    corpus: &Corpus,
    prev: Option<&Index>,
    progress: &Progress,
    cancelled: &dyn Fn() -> bool,
) -> blyg_embed::Result<(Index, Built)> {
    let started = std::time::Instant::now();
    let model = embedder.id();
    let prev = prev.filter(|p| p.model == model);
    // What's already embedded: the last index, else the store.
    let stored: Vec<NearbyVector>;
    let mut known: HashMap<&str, Vec<Row<'_>>> = HashMap::new();
    let mut known_hash: HashMap<&str, &str> = HashMap::new();
    match prev {
        Some(p) => {
            known = p.rows_of();
            known_hash = p
                .doc_hash
                .iter()
                .map(|(k, h)| (k.as_str(), h.as_str()))
                .collect();
        }
        None => {
            stored = backend.nearby_vectors(&model);
            for v in &stored {
                known.entry(v.doc.as_str()).or_default().push((
                    v.chunk,
                    v.hash.as_str(),
                    v.vec.as_slice(),
                ));
            }
        }
    }
    let mut idx = Index {
        model: model.clone(),
        ..Default::default()
    };
    #[allow(clippy::type_complexity)]
    let mut todo: Vec<(&Doc, Vec<String>, Vec<Option<Vec<f32>>>)> = Vec::new();
    for d in corpus.docs() {
        let have = known.get(d.key.as_str());
        // Unchanged since the last index: keep its rows as they are.
        if let (Some(rows), Some(h)) = (have, known_hash.get(d.key.as_str()))
            && *h == d.hash
        {
            let k = idx.keys.len() as u32;
            idx.keys.push(d.key.clone());
            idx.doc_hash.insert(d.key.clone(), d.hash.clone());
            for (c, h, v) in rows {
                idx.push(k, *c, h.to_string(), v);
            }
            continue;
        }
        let pieces = d.pieces();
        let hashes: Vec<String> = pieces.iter().map(|p| text_hash(p)).collect();
        let same = have.is_some_and(|rows| {
            rows.len() == pieces.len()
                && rows
                    .iter()
                    .all(|(c, h, _)| hashes.get(*c as usize).is_some_and(|x| x == h))
        });
        if same {
            let k = idx.keys.len() as u32;
            idx.keys.push(d.key.clone());
            idx.doc_hash.insert(d.key.clone(), d.hash.clone());
            for (c, h, v) in have.into_iter().flatten() {
                idx.push(k, *c, h.to_string(), v);
            }
        } else {
            // Changed: a piece whose text it already had keeps its vector.
            let reuse: Vec<Option<Vec<f32>>> = hashes
                .iter()
                .map(|h| {
                    have.and_then(|rows| rows.iter().find(|(_, rh, _)| rh == h))
                        .map(|(_, _, v)| v.to_vec())
                })
                .collect();
            todo.push((d, pieces, reuse));
        }
    }
    let fresh = |r: &[Option<Vec<f32>>]| r.iter().filter(|v| v.is_none()).count();
    let total: usize = todo.iter().map(|(_, _, r)| fresh(r)).sum();
    progress.total.store(total as u64, Ordering::Relaxed);
    progress.done.store(0, Ordering::Relaxed);
    let mut embedded = 0;
    let batch = embedder.batch().max(1);
    let mut i = 0;
    while i < todo.len() {
        if cancelled() {
            return Err(blyg_embed::EmbedError::Model("cancelled".into()));
        }
        // Whole posts per batch, about `batch` new pieces.
        let mut j = i;
        let mut n = 0;
        while j < todo.len() && (n == 0 || n + fresh(&todo[j].2) <= batch) {
            n += fresh(&todo[j].2);
            j += 1;
        }
        let texts: Vec<&str> = todo[i..j]
            .iter()
            .flat_map(|(_, p, r)| {
                p.iter()
                    .zip(r)
                    .filter(|(_, v)| v.is_none())
                    .map(|(t, _)| t.as_str())
            })
            .collect();
        let mut vecs = if texts.is_empty() {
            Vec::new().into_iter()
        } else {
            embedder.embed(&texts)?.into_iter()
        };
        let mut rows = Vec::new();
        let mut docs = Vec::new();
        for (d, pieces, reuse) in &todo[i..j] {
            let k = idx.keys.len() as u32;
            idx.keys.push(d.key.clone());
            idx.doc_hash.insert(d.key.clone(), d.hash.clone());
            docs.push(d.key.clone());
            for (c, (p, old)) in pieces.iter().zip(reuse).enumerate() {
                let v = match old {
                    Some(v) => v.clone(),
                    None => {
                        let mut v = vecs.next().unwrap_or_default();
                        vector::normalize(&mut v);
                        v
                    }
                };
                let h = text_hash(p);
                idx.push(k, c as u32, h.clone(), &v);
                rows.push(NearbyVector {
                    doc: d.key.clone(),
                    chunk: c as u32,
                    hash: h,
                    vec: v,
                });
            }
        }
        embedded += n;
        progress.done.fetch_add(n as u64, Ordering::Relaxed);
        let _ = backend.put_nearby_vectors(&model, &docs, &rows);
        i = j;
    }
    let keep: HashSet<String> = corpus.docs().map(|d| d.key.clone()).collect();
    let _ = backend.retain_nearby_vectors(&model, &keep);
    let built = Built {
        posts: idx.keys.len(),
        pieces: idx.pieces(),
        embedded,
        millis: started.elapsed().as_millis(),
    };
    Ok((idx, built))
}

// ------------------------------------------------------------ the ranking

/// What the rail asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    /// The editor's text.
    pub text: String,
    pub para: Range<usize>,
    /// The post being written (it can't offer itself).
    pub exclude: Option<String>,
    /// Ids the text already links or quotes (lowercase).
    pub linked: HashSet<String>,
    pub titles_only: bool,
}

/// One row of the rail.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub key: String,
    pub id: String,
    pub title: String,
    pub label: String,
    pub who: String,
    pub own: bool,
    pub kind: Kind,
    /// The paragraph's phrase it shares (⌥⏎ links it in place).
    pub phrase: Option<Phrase>,
    /// Why it's here, in a few words: the shared phrase, else the closest
    /// passage.
    pub why: String,
    pub score: f32,
    /// Found by meaning, not by a shared phrase.
    pub semantic: bool,
}

fn snippet(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        format!("{}…", t.trim_end())
    } else {
        t
    }
}

/// The rail's candidates for `ask`, best first, at most [`POOL`].
/// `embedded` is the paragraph's vector when there's an index to use.
pub fn rank(
    ask: &Ask,
    corpus: &Corpus,
    index: Option<(&Index, &[f32])>,
    hits: &[(String, usize)],
    phrases: &[Phrase],
    min_sim: f32,
) -> Vec<Suggestion> {
    let by_key = corpus.by_key();
    let usable = |d: &Doc| {
        Some(d.key.as_str()) != ask.exclude.as_deref() && !ask.linked.contains(&d.id.to_lowercase())
    };
    // The best phrase each post shares: most words, then longest.
    let mut shared: HashMap<&str, &Phrase> = HashMap::new();
    for (key, i) in hits {
        let Some(p) = phrases.get(*i) else { continue };
        let e = shared.entry(key.as_str()).or_insert(p);
        if (p.words, p.needle.len()) > (e.words, e.needle.len()) {
            *e = p;
        }
    }
    let mut out: HashMap<String, Suggestion> = HashMap::new();
    let make =
        |d: &Doc, phrase: Option<&Phrase>, why: String, score: f32, semantic: bool| Suggestion {
            key: d.key.clone(),
            id: d.id.clone(),
            title: d.title.clone(),
            label: d.label.clone(),
            who: d.who.clone(),
            own: d.own,
            kind: d.kind,
            phrase: phrase.cloned(),
            why,
            score,
            semantic,
        };
    if let Some((idx, q)) = index {
        for (key, piece, sim) in idx.nearest(q, ask.titles_only, POOL * 4) {
            if sim < min_sim {
                continue;
            }
            let Some(d) = by_key.get(key.as_str()).filter(|d| usable(d)) else {
                continue;
            };
            let phrase = shared.get(key.as_str()).copied();
            let why = match phrase {
                Some(p) => format!("“{}”", p.needle),
                None => {
                    let pieces = d.pieces();
                    snippet(
                        pieces.get(piece as usize).map_or(d.title.as_str(), |s| s),
                        90,
                    )
                }
            };
            let score =
                sim + if phrase.is_some() { 0.15 } else { 0.0 } + if d.own { 0.03 } else { 0.0 };
            out.insert(key.clone(), make(d, phrase, why, score, phrase.is_none()));
        }
    }
    // Phrase matches (all of them with no index; the strong ones with one).
    for (key, p) in &shared {
        if out.contains_key(*key) {
            continue;
        }
        let Some(d) = by_key.get(key).filter(|d| usable(d)) else {
            continue;
        };
        let in_title = d.title.to_lowercase().contains(&p.needle);
        if ask.titles_only && !in_title {
            continue;
        }
        if !(in_title || d.own || p.words >= 3) {
            continue;
        }
        let score = 0.5
            + 0.1 * p.words as f32
            + if in_title { 0.2 } else { 0.0 }
            + if d.own { 0.05 } else { 0.0 };
        out.insert(
            key.to_string(),
            make(d, Some(p), format!("“{}”", p.needle), score, false),
        );
    }
    let mut v: Vec<Suggestion> = out.into_values().collect();
    v.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(b.own.cmp(&a.own))
            .then(a.id.cmp(&b.id))
    });
    // One row per id (a post held through two subscriptions, or yours).
    let mut seen = HashSet::new();
    v.retain(|s| seen.insert(s.id.to_lowercase()));
    v.truncate(POOL);
    v
}

/// The ids `text` already links (`[[id]]`) or quotes (`![[id]]`),
/// lowercase.
pub fn linked_ids(text: &str) -> HashSet<String> {
    blyg_core::grammar::link_refs(text)
        .into_iter()
        .map(|r| r.id.to_lowercase())
        .chain(
            blyg_core::profile::transclusion_ids(text)
                .into_iter()
                .map(|i| i.to_lowercase()),
        )
        .collect()
}

// ------------------------------------------------------------ re-rank

/// The re-rank prompt: the paragraph and the numbered candidates.
pub fn rerank_prompt(paragraph: &str, cands: &[Suggestion], corpus: &Corpus) -> (String, String) {
    let by_key = corpus.by_key();
    let system = "You compare a paragraph someone is writing with short posts. Reply with only \
                  the numbers of the posts that say the same thing as the paragraph, or argue \
                  with it, most closely related first, comma-separated (e.g. 3, 1). Leave out \
                  posts that are unrelated. No other words."
        .to_string();
    let mut user = format!("Paragraph:\n{}\n\nPosts:\n", snippet(paragraph, 1200));
    for (i, s) in cands.iter().enumerate() {
        let body = by_key
            .get(s.key.as_str())
            .map(|d| snippet(&d.text.replace('\n', " "), 300))
            .unwrap_or_default();
        user.push_str(&format!("{}. {} — {}\n", i + 1, s.title, body));
    }
    (system, user)
}

/// The candidates in the order the reply gives (numbers it names, in
/// order); unchanged when the reply names none of them.
pub fn apply_rerank(cands: &[Suggestion], reply: &str) -> Vec<Suggestion> {
    let mut order: Vec<usize> = Vec::new();
    for n in reply
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|t| t.parse::<usize>().ok())
    {
        if (1..=cands.len()).contains(&n) && !order.contains(&(n - 1)) {
            order.push(n - 1);
        }
    }
    if order.is_empty() {
        return cands.to_vec();
    }
    order.into_iter().map(|i| cands[i].clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_paragraph_under_the_caret() {
        let t = "# Title\n\nFirst line\nsecond line.\n\n  Third.  \n";
        let at = |i| paragraph_at(t, i).map(|r| &t[r]);
        assert_eq!(at(0), Some("# Title"));
        assert_eq!(at(12), Some("First line\nsecond line."));
        assert_eq!(
            at(t.find("second").unwrap()),
            Some("First line\nsecond line.")
        );
        assert_eq!(at(8), None);
        assert_eq!(at(t.find("Third").unwrap() + 2), Some("Third."));
        assert_eq!(at(t.len()), None);
    }

    #[test]
    fn distinctive_phrases_skip_stop_words_markup_and_punctuation() {
        let t = "The tide pools again, and [[01j9rk0000000000000000000c | rock pool]] economics of the shore.";
        let r = 0..t.len();
        let ps = phrases(t, &r);
        let needles: Vec<&str> = ps.iter().map(|p| p.needle.as_str()).collect();
        assert!(needles.contains(&"tide pools"), "{needles:?}");
        // "again" is a stop word: no phrase ends on it.
        assert!(!needles.contains(&"tide pools again"));
        // Not across the comma, not into the link, not ending on "the".
        assert!(!needles.iter().any(|n| n.contains("again and")));
        assert!(!needles.iter().any(|n| n.contains("rock")));
        assert!(!needles.iter().any(|n| n.ends_with(" the")));
        let p = ps.iter().find(|p| p.needle == "tide pools").unwrap();
        assert_eq!(&t[p.range.clone()], "tide pools");
        assert_eq!(
            paragraph_plain(t, &r),
            "The tide pools again, and economics of the shore."
        );
    }

    #[test]
    fn rerank_reply_reorders_and_falls_back() {
        let s = |id: &str| Suggestion {
            key: id.into(),
            id: id.into(),
            title: id.into(),
            label: id.into(),
            who: "You".into(),
            own: true,
            kind: Kind::Thread,
            phrase: None,
            why: String::new(),
            score: 1.0,
            semantic: true,
        };
        let c = vec![s("a"), s("b"), s("c")];
        let ids = |v: Vec<Suggestion>| v.into_iter().map(|s| s.id).collect::<Vec<_>>();
        assert_eq!(ids(apply_rerank(&c, "3, 1")), vec!["c", "a"]);
        assert_eq!(ids(apply_rerank(&c, "none of them")), vec!["a", "b", "c"]);
        assert_eq!(ids(apply_rerank(&c, "9, 2, 2")), vec!["b"]);
    }
}
