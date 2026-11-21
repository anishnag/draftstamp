//! Attestations: Bitcoin, Pending, and anything else preserved as opaque bytes.

use std::cmp::Ordering;

use super::encoding::{Reader, write_varbytes, write_varuint};
use crate::Error;

pub(crate) const BITCOIN: [u8; 8] = [0x05, 0x88, 0x96, 0x0d, 0x73, 0xd7, 0x19, 0x01];
pub(crate) const PENDING: [u8; 8] = [0x83, 0xdf, 0xe3, 0x0d, 0x2e, 0xf9, 0x0c, 0x8e];

const MAX_PAYLOAD: usize = 8192;
const MAX_URI: usize = 1000;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Attestation {
    Bitcoin { height: u64 },
    Pending { uri: String },
    Unknown { tag: [u8; 8], payload: Vec<u8> },
}

impl Attestation {
    pub(crate) fn tag(&self) -> [u8; 8] {
        match self {
            Attestation::Bitcoin { .. } => BITCOIN,
            Attestation::Pending { .. } => PENDING,
            Attestation::Unknown { tag, .. } => *tag,
        }
    }

    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Attestation, Error> {
        let tag: [u8; 8] = r.read_exact(8)?.try_into().expect("8 bytes");
        let payload = r.read_varbytes(MAX_PAYLOAD, 0)?;
        let mut p = Reader::new(payload);
        let a = match tag {
            BITCOIN => {
                let height = p.read_varuint()?;
                if height > u64::from(u32::MAX) {
                    return Err(Error::BadPayload);
                }
                Attestation::Bitcoin { height }
            }
            PENDING => {
                let uri = p.read_varbytes(MAX_URI, 0)?;
                if !uri
                    .iter()
                    .all(|c| c.is_ascii_alphanumeric() || b"-._/:".contains(c))
                {
                    return Err(Error::BadUri);
                }
                Attestation::Pending {
                    uri: String::from_utf8(uri.to_vec()).map_err(|_| Error::BadUri)?,
                }
            }
            _ => {
                return Ok(Attestation::Unknown {
                    tag,
                    payload: payload.to_vec(),
                });
            }
        };
        p.finish().map_err(|_| Error::BadPayload)?;
        Ok(a)
    }

    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.tag());
        let mut payload = Vec::new();
        match self {
            Attestation::Bitcoin { height } => write_varuint(&mut payload, *height),
            Attestation::Pending { uri } => write_varbytes(&mut payload, uri.as_bytes()),
            Attestation::Unknown { payload: p, .. } => payload.extend_from_slice(p),
        }
        write_varbytes(out, &payload);
    }
}

/// Upstream order: by tag bytes, then height, URI or raw payload.
impl Ord for Attestation {
    fn cmp(&self, other: &Self) -> Ordering {
        self.tag()
            .cmp(&other.tag())
            .then_with(|| match (self, other) {
                (Attestation::Bitcoin { height: a }, Attestation::Bitcoin { height: b }) => {
                    a.cmp(b)
                }
                (Attestation::Pending { uri: a }, Attestation::Pending { uri: b }) => a.cmp(b),
                (
                    Attestation::Unknown { payload: a, .. },
                    Attestation::Unknown { payload: b, .. },
                ) => a.cmp(b),
                _ => Ordering::Equal,
            })
    }
}

impl PartialOrd for Attestation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(a: Attestation) {
        let mut v = Vec::new();
        a.write(&mut v);
        let mut r = Reader::new(&v);
        assert_eq!(Attestation::read(&mut r).unwrap(), a);
        r.finish().unwrap();
    }

    #[test]
    fn round_trips() {
        round_trip(Attestation::Bitcoin { height: 921_407 });
        round_trip(Attestation::Pending {
            uri: "https://alice.btc.calendar.opentimestamps.org".into(),
        });
        round_trip(Attestation::Unknown {
            tag: [6, 0x86, 0x9a, 0x0d, 0x73, 0xd7, 0x1b, 0x45],
            payload: vec![1, 2, 3],
        });
    }

    #[test]
    fn sorts_bitcoin_before_pending() {
        let mut v = [
            Attestation::Pending { uri: "b".into() },
            Attestation::Bitcoin { height: 9 },
            Attestation::Pending { uri: "a".into() },
            Attestation::Bitcoin { height: 2 },
        ];
        v.sort();
        assert_eq!(v[0], Attestation::Bitcoin { height: 2 });
        assert_eq!(v[3], Attestation::Pending { uri: "b".into() });
    }

    #[test]
    fn rejects_bad_payloads() {
        let mut v = PENDING.to_vec();
        write_varbytes(&mut v, &{
            let mut p = Vec::new();
            write_varbytes(&mut p, b"https://x y");
            p
        });
        assert_eq!(Attestation::read(&mut Reader::new(&v)), Err(Error::BadUri));

        let mut v = BITCOIN.to_vec();
        write_varbytes(&mut v, &[0x01, 0xff]);
        assert_eq!(
            Attestation::read(&mut Reader::new(&v)),
            Err(Error::BadPayload),
            "payload not consumed"
        );
    }
}
