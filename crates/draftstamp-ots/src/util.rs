//! Small encoders kept in-crate to avoid dependencies.

use sha2::{Digest, Sha256};

pub(crate) fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 15) as usize] as char);
    }
    s
}

/// Decodes hex (either case). `None` on odd length or a non-hex character.
pub(crate) fn hex_decode(s: &str) -> Option<Vec<u8>> {
    fn nib(c: u8) -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    }
    let s = s.as_bytes();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    s.chunks(2)
        .map(|p| Some((nib(p[0])? << 4) | nib(p[1])?))
        .collect()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url without padding.
pub(crate) fn b64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        let chars = chunk.len() + 1;
        for i in 0..chars {
            out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

/// Strict base64url without padding: rejects padding, other alphabets and non-canonical trailing bits.
pub(crate) fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let vals: Vec<u32> = s
        .bytes()
        .map(|c| B64.iter().position(|&b| b == c).map(|v| v as u32))
        .collect::<Option<_>>()?;
    if vals.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(vals.len() * 3 / 4);
    for chunk in vals.chunks(4) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, v)| n | v << (18 - 6 * i));
        let bytes = chunk.len() - 1;
        let unused_bits = 8 * (3 - bytes);
        if bytes < 3 && n & ((1 << unused_bits) - 1) != 0 {
            return None;
        }
        for i in 0..bytes {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex() {
        assert_eq!(hex_encode(&[0, 0xab, 0xff]), "00abff");
        assert_eq!(hex_decode("00ABff").unwrap(), vec![0, 0xab, 0xff]);
        assert!(hex_decode("abc").is_none());
        assert!(hex_decode("zz").is_none());
    }

    #[test]
    fn base64url_round_trips_every_length() {
        for len in 0..50u8 {
            let data: Vec<u8> = (0..len)
                .map(|i| i.wrapping_mul(37).wrapping_add(250))
                .collect();
            let enc = b64url_encode(&data);
            assert!(!enc.contains('='));
            assert_eq!(b64url_decode(&enc).unwrap(), data, "len {len}");
        }
        assert_eq!(b64url_encode(&[0u8; 44]).len(), 59);
    }

    #[test]
    fn base64url_is_strict() {
        assert!(b64url_decode("AA==").is_none(), "padding");
        assert!(b64url_decode("A+").is_none(), "standard alphabet");
        assert!(b64url_decode("A").is_none(), "impossible length");
        assert!(b64url_decode("AB").is_none(), "non-canonical trailing bits");
        assert_eq!(b64url_decode("AA").unwrap(), vec![0]);
    }
}
