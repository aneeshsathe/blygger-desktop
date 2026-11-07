//! Unit vectors: normalising, cosine (a dot product), brute-force top-k,
//! and the half-precision form vectors are stored in (half the disk, and
//! more than enough precision to rank by).

/// Scale `v` to unit length (a zero vector stays zero).
pub fn normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
}

/// The dot product (cosine for unit vectors). Mismatched lengths compare
/// the common prefix.
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    // Four lanes: the compiler vectorises this; a plain zip sum doesn't
    // (float addition isn't associative).
    let n = a.len().min(b.len());
    let (a, b) = (&a[..n], &b[..n]);
    let mut acc = [0f32; 4];
    let (ca, ra) = a.as_chunks::<4>();
    let (cb, rb) = b.as_chunks::<4>();
    for (x, y) in ca.iter().zip(cb) {
        acc[0] += x[0] * y[0];
        acc[1] += x[1] * y[1];
        acc[2] += x[2] * y[2];
        acc[3] += x[3] * y[3];
    }
    let tail: f32 = ra.iter().zip(rb).map(|(x, y)| x * y).sum();
    acc[0] + acc[1] + acc[2] + acc[3] + tail
}

/// The `k` best `(index, score)` of `scores`, best first (ties: lower index).
pub fn top_k(scores: impl IntoIterator<Item = (usize, f32)>, k: usize) -> Vec<(usize, f32)> {
    let mut all: Vec<(usize, f32)> = scores.into_iter().filter(|(_, s)| s.is_finite()).collect();
    all.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    all.truncate(k);
    all
}

/// `v` as little-endian IEEE half floats.
pub fn to_f16_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 2);
    for x in v {
        out.extend_from_slice(&f32_to_f16(*x).to_le_bytes());
    }
    out
}

/// The inverse of [`to_f16_bytes`] (`None` for an odd length).
pub fn from_f16_bytes(b: &[u8]) -> Option<Vec<f32>> {
    if !b.len().is_multiple_of(2) {
        return None;
    }
    Some(
        b.as_chunks::<2>()
            .0
            .iter()
            .map(|c| f16_to_f32(u16::from_le_bytes(*c)))
            .collect(),
    )
}

/// IEEE 754 binary16 → binary32.
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) as u32) << 31;
    let exp = ((h >> 10) & 0x1f) as u32;
    let frac = (h & 0x3ff) as u32;
    let bits = match (exp, frac) {
        (0, 0) => sign,
        (0, f) => {
            // Subnormal: renormalise.
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

/// IEEE 754 binary32 → binary16, rounding to nearest even.
pub fn f32_to_f16(x: f32) -> u16 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_round_trips_closely() {
        for x in [0.0f32, 1.0, -1.0, 0.5, 0.123_456, -0.000_1, 65504.0, 3.0e-6] {
            let y = f16_to_f32(f32_to_f16(x));
            assert!((x - y).abs() <= x.abs() * 1e-3 + 1e-7, "{x} → {y}");
        }
        assert!(f16_to_f32(f32_to_f16(1e9)).is_infinite());
        let v = vec![0.25, -0.75, 0.0, 1.0];
        assert_eq!(from_f16_bytes(&to_f16_bytes(&v)).unwrap(), v);
        assert!(from_f16_bytes(&[1, 2, 3]).is_none());
    }

    #[test]
    fn dot_and_top_k() {
        let mut a = vec![3.0, 4.0, 0.0, 0.0, 0.0];
        normalize(&mut a);
        assert!((dot(&a, &a) - 1.0).abs() < 1e-6);
        assert_eq!(dot(&[1.0, 2.0, 3.0, 4.0, 5.0], &[1.0; 5]), 15.0);
        let k = top_k([(0, 0.1), (1, 0.9), (2, f32::NAN), (3, 0.9), (4, 0.5)], 3);
        assert_eq!(k, vec![(1, 0.9), (3, 0.9), (4, 0.5)]);
    }
}
