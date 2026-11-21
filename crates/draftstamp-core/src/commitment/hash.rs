use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sha2::{Digest as _, Sha256};

use crate::Error;

/// A SHA-256 digest. Written as 64 lowercase hex characters.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Digest(pub [u8; 32]);

/// A per-file random salt. Written as 32 lowercase hex characters.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Salt(pub [u8; 16]);

/// SHA-256 over the concatenation of `parts`.
pub(crate) fn sha256(parts: &[&[u8]]) -> Digest {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    Digest(hasher.finalize().into())
}

fn encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Decodes exactly `N` bytes of lowercase hex. Uppercase is rejected: the format is canonical.
fn decode<const N: usize>(s: &str) -> Option<[u8; N]> {
    fn nibble(c: u8) -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        }
    }
    let s = s.as_bytes();
    if s.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (i, [hi, lo]) in s.as_chunks::<2>().0.iter().enumerate() {
        out[i] = (nibble(*hi)? << 4) | nibble(*lo)?;
    }
    Some(out)
}

macro_rules! hex_type {
    ($name:ident, $len:expr, $what:expr) => {
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&encode(&self.0))
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self)
            }
        }

        impl FromStr for $name {
            type Err = Error;
            fn from_str(s: &str) -> Result<Self, Error> {
                decode::<$len>(s).map($name).ok_or_else(|| {
                    Error::InvalidReceipt(format!(
                        "{} must be {} lowercase hex characters, got {s:?}",
                        $what,
                        $len * 2
                    ))
                })
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&encode(&self.0))
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                s.parse().map_err(de::Error::custom)
            }
        }
    };
}

hex_type!(Digest, 32, "a digest");
hex_type!(Salt, 16, "a salt");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_of_empty_input() {
        assert_eq!(
            sha256(&[]).to_string(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hex_round_trip_and_rejections() {
        let d = sha256(&[b"abc"]);
        assert_eq!(d.to_string().parse::<Digest>().unwrap(), d);
        assert!(
            "ABCD".repeat(16).parse::<Digest>().is_err(),
            "uppercase rejected"
        );
        assert!("00".parse::<Digest>().is_err(), "wrong length rejected");
        assert!(
            "zz".repeat(32).parse::<Digest>().is_err(),
            "non-hex rejected"
        );
    }
}
