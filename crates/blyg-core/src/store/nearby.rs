//! --- nearby --- Nearby thoughts in the store (schema v20): the stored
//! embeddings, and the phrase search the rail falls back to with no
//! embedder (the trigram indexes `items_fts` and `reading_fts`).

use std::collections::HashSet;

use rusqlite::params;

use super::Store;
use crate::backend::Result;
use crate::nearby::NearbyVector;

/// `"phrase"` as an FTS5 string (quotes doubled).
fn fts_phrase(p: &str) -> String {
    format!("\"{}\"", p.replace('"', "\"\""))
}

/// f16 little-endian, as `blyg-embed` stores them (decoded here so the
/// core needs no embedding crate).
fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) as u32) << 31;
    let exp = ((h >> 10) & 0x1f) as u32;
    let frac = (h & 0x3ff) as u32;
    let bits = match (exp, frac) {
        (0, 0) => sign,
        (0, f) => {
            let mut e = 127 - 15 + 1;
            let mut f = f;
            while f & 0x400 == 0 {
                f <<= 1;
                e -= 1;
            }
            sign | (e << 23) | ((f & 0x3ff) << 13)
        }
        (0x1f, 0) => sign | 0x7f80_0000,
        (0x1f, f) => sign | 0x7fc0_0000 | (f << 13),
        (e, f) => sign | ((e + 127 - 15) << 23) | (f << 13),
    };
    f32::from_bits(bits)
}

fn f32_to_f16(x: f32) -> u16 {
    let bits = x.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let frac = bits & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if frac != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = frac | 0x80_0000;
        let shift = (14 - e) as u32;
        let half = 1u32 << (shift - 1);
        let mut r = m >> shift;
        let rest = m & ((1 << shift) - 1);
        if rest > half || (rest == half && r & 1 == 1) {
            r += 1;
        }
        return sign | r as u16;
    }
    let mut r = ((e as u32) << 10) | (frac >> 13);
    let rest = frac & 0x1fff;
    if rest > 0x1000 || (rest == 0x1000 && r & 1 == 1) {
        r += 1;
    }
    sign | r as u16
}

pub(crate) fn encode(v: &[f32]) -> Vec<u8> {
    v.iter()
        .flat_map(|x| f32_to_f16(*x).to_le_bytes())
        .collect()
}

pub(crate) fn decode(b: &[u8]) -> Vec<f32> {
    b.as_chunks::<2>()
        .0
        .iter()
        .map(|c| f16_to_f32(u16::from_le_bytes(*c)))
        .collect()
}

impl Store {
    /// Every stored vector made by `model`.
    pub fn nearby_vectors(&self, model: &str) -> Result<Vec<NearbyVector>> {
        let c = self.rconn();
        let mut st = c.prepare_cached(
            "SELECT doc, chunk, hash, vec FROM nearby_vectors WHERE model = ?1 \
             ORDER BY doc, chunk",
        )?;
        let rows = st
            .query_map([model], |r| {
                Ok(NearbyVector {
                    doc: r.get(0)?,
                    chunk: r.get::<_, i64>(1)? as u32,
                    hash: r.get(2)?,
                    vec: decode(&r.get::<_, Vec<u8>>(3)?),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Replace the vectors of each post in `docs` with `rows` (all of
    /// whose `doc`s are among them), in one transaction. A post that's
    /// gone from the store meanwhile keeps none: it's checked here, so a
    /// slow indexer can't bring back a withdrawn post's vectors.
    pub fn put_nearby_vectors(
        &self,
        model: &str,
        docs: &[String],
        rows: &[NearbyVector],
    ) -> Result<()> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        {
            let mut del = tx.prepare_cached("DELETE FROM nearby_vectors WHERE doc = ?1")?;
            for d in docs {
                del.execute([d])?;
            }
            let mut live_own = tx.prepare_cached(
                "SELECT EXISTS(SELECT 1 FROM items WHERE local_id = ?1 AND status = 'public')",
            )?;
            let mut live_reading = tx.prepare_cached(
                "SELECT EXISTS(SELECT 1 FROM reading WHERE subscription_id = ?1 \
                 AND remote_id = ?2 AND state = 'current' AND sub_kind = 'blyg')",
            )?;
            let mut ins = tx.prepare_cached(
                "INSERT OR REPLACE INTO nearby_vectors (model, doc, chunk, hash, vec) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            let mut alive: std::collections::HashMap<&str, bool> = Default::default();
            for r in rows {
                let ok = match alive.get(r.doc.as_str()) {
                    Some(ok) => *ok,
                    None => {
                        let ok = if let Some(id) = r.doc.strip_prefix("own:") {
                            live_own.query_row([id], |x| x.get::<_, bool>(0))?
                        } else if let Some((s, id)) = r
                            .doc
                            .strip_prefix("reading:")
                            .and_then(|k| k.split_once('/'))
                        {
                            live_reading.query_row([s, id], |x| x.get::<_, bool>(0))?
                        } else {
                            false
                        };
                        alive.insert(r.doc.as_str(), ok);
                        ok
                    }
                };
                if ok {
                    ins.execute(params![
                        model,
                        r.doc,
                        r.chunk as i64,
                        r.hash,
                        encode(&r.vec)
                    ])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Keep only `model`'s vectors of the posts in `keep`: another model's
    /// are dropped, and so are posts no longer offered.
    pub fn retain_nearby_vectors(&self, model: &str, keep: &HashSet<String>) -> Result<usize> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let mut n = tx.execute("DELETE FROM nearby_vectors WHERE model <> ?1", [model])?;
        let docs: Vec<String> = {
            let mut st = tx.prepare("SELECT DISTINCT doc FROM nearby_vectors")?;
            st.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        {
            let mut del = tx.prepare_cached("DELETE FROM nearby_vectors WHERE doc = ?1")?;
            for d in docs.iter().filter(|d| !keep.contains(*d)) {
                n += del.execute([d])?;
            }
        }
        tx.commit()?;
        Ok(n)
    }

    /// The posts whose text contains each phrase (case-insensitively,
    /// through the trigram indexes), as `(doc key, phrase index)`: your own
    /// published posts and current posts from blyg subscriptions. Phrases
    /// under three characters aren't looked up.
    pub fn nearby_phrase_hits(&self, phrases: &[String]) -> Result<Vec<(String, usize)>> {
        let c = self.rconn();
        let mut out = Vec::new();
        let mut own = c.prepare_cached(
            "SELECT local_id, content_md, rid IN (SELECT rid FROM items_fts_stale) FROM items \
             WHERE status = 'public' AND server_id IS NOT NULL AND \
             (rid IN (SELECT rowid FROM items_fts WHERE items_fts MATCH ?1) \
              OR rid IN (SELECT rid FROM items_fts_stale))",
        )?;
        let mut held = c.prepare_cached(
            "SELECT subscription_id, remote_id FROM reading WHERE sub_kind = 'blyg' \
             AND state = 'current' \
             AND rowid IN (SELECT rowid FROM reading_fts WHERE reading_fts MATCH ?1)",
        )?;
        for (i, p) in phrases.iter().enumerate() {
            let p = p.trim();
            if p.chars().count() < 3 {
                continue;
            }
            let needle = p.to_lowercase();
            let q = fts_phrase(p);
            let rows = own.query_map([&q], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                ))
            })?;
            for row in rows {
                let (id, text, stale) = row?;
                // Indexed under older text: what it says now decides.
                if stale && !crate::util::lower_contains(&text, &needle) {
                    continue;
                }
                out.push((crate::nearby::own_key(&id), i));
            }
            let rows = held.query_map([&q], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (s, id) = row?;
                out.push((crate::nearby::reading_key(&s, &id), i));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats_round_trip() {
        let v = vec![0.0, 0.5, -0.25, 1.0, 0.123];
        let back = decode(&encode(&v));
        for (a, b) in v.iter().zip(&back) {
            assert!((a - b).abs() < 1e-3);
        }
    }
}
