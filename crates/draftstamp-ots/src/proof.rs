//! `Proof`: a `.ots` detached timestamp file. OpenTimestamps concepts only.

use crate::Error;
use crate::format::attestation::Attestation;
use crate::format::encoding::Reader;
use crate::format::tree::Node;

const MAGIC: &[u8; 31] = b"\x00OpenTimestamps\x00\x00Proof\x00\xbf\x89\xe2\xe8\x84\xe8\x92\x94";
const VERSION: u8 = 1;
const SHA256_TAG: u8 = 0x08;

/// Largest `.ots` file accepted.
pub const MAX_PROOF_BYTES: usize = 64 * 1024;

/// A standard OpenTimestamps proof whose digest is a DraftStamp commitment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    commitment: [u8; 32],
    tree: Node,
}

/// What a proof shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// No branch reaches Bitcoin yet: only calendar promises.
    Pending,
    /// The proof names a Bitcoin block, but it has not been checked against that block's header
    /// (offline status, or the explorers were unreachable or disagreed).
    Claimed {
        /// Block height.
        height: u64,
    },
    /// Checked: the block header at `height`, agreed by both explorers, contains the proof's value.
    Anchored {
        /// Block height.
        height: u64,
        /// The block's timestamp, in Unix seconds: the proven time.
        time: u32,
    },
}

/// One calendar the stamp was submitted through, and the 44-byte message it filed the stamp under.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalendarEntry {
    /// Calendar base URL, e.g. `https://alice.btc.calendar.opentimestamps.org`.
    pub url: String,
    /// The message at the pending attestation: the calendar's lookup key.
    pub message: Vec<u8>,
}

impl Proof {
    pub(crate) fn new(commitment: [u8; 32], tree: Node) -> Self {
        Proof { commitment, tree }
    }

    /// Reads a `.ots` file. Strict and bounded; never panics.
    pub fn from_bytes(bytes: &[u8]) -> Result<Proof, Error> {
        if bytes.len() > MAX_PROOF_BYTES {
            return Err(Error::TooLarge);
        }
        let mut r = Reader::new(bytes);
        if r.read_exact(MAGIC.len()).map_err(|_| Error::BadMagic)? != MAGIC {
            return Err(Error::BadMagic);
        }
        let version = r.read_u8()?;
        if version != VERSION {
            return Err(Error::UnsupportedVersion(version));
        }
        let hash_op = r.read_u8()?;
        if hash_op != SHA256_TAG {
            return Err(Error::UnsupportedHashOp(hash_op));
        }
        let commitment: [u8; 32] = r.read_exact(32)?.try_into().expect("32 bytes");
        let tree = Node::read(&mut r, Some(&commitment))?;
        r.finish()?;
        Ok(Proof { commitment, tree })
    }

    /// Writes the `.ots` file in canonical form, byte-identical to the official client.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(512);
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.push(SHA256_TAG);
        out.extend_from_slice(&self.commitment);
        self.tree.write(&mut out);
        out
    }

    /// The commitment this proof is about.
    pub fn commitment(&self) -> [u8; 32] {
        self.commitment
    }

    /// Status from the file alone, without any network check: `Pending` or `Claimed`.
    pub fn status(&self) -> Status {
        match self.bitcoin_claims().first() {
            Some(&(height, _)) => Status::Claimed { height },
            None => Status::Pending,
        }
    }

    /// Every calendar the proof was submitted through, answered or not, sorted.
    /// Stable for the proof's whole life, since upgrades never remove pending attestations.
    pub fn calendars(&self) -> Vec<CalendarEntry> {
        let mut out = Vec::new();
        self.tree.walk(Some(&self.commitment), &mut |node, msg| {
            for a in &node.attestations {
                if let (Attestation::Pending { uri }, Some(m)) = (a, msg) {
                    out.push(CalendarEntry {
                        url: uri.clone(),
                        message: m.to_vec(),
                    });
                }
            }
        });
        out.sort();
        out.dedup();
        out
    }

    /// Calendars whose promise has no Bitcoin attestation below it yet.
    pub(crate) fn unanswered(&self) -> Vec<CalendarEntry> {
        let mut out = Vec::new();
        self.tree.walk(Some(&self.commitment), &mut |node, msg| {
            if let Some(m) = msg
                && !node.has_bitcoin()
            {
                for a in &node.attestations {
                    if let Attestation::Pending { uri } = a {
                        out.push(CalendarEntry {
                            url: uri.clone(),
                            message: m.to_vec(),
                        });
                    }
                }
            }
        });
        out.sort();
        out.dedup();
        out
    }

    pub(crate) fn bitcoin_claims(&self) -> Vec<(u64, [u8; 32])> {
        self.tree.bitcoin_claims(&self.commitment)
    }

    pub(crate) fn graft(&mut self, message: &[u8], fragment: &Node) -> bool {
        self.tree.graft(Some(&self.commitment), message, fragment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_headers() {
        assert_eq!(Proof::from_bytes(b"nope"), Err(Error::BadMagic));
        let mut b = MAGIC.to_vec();
        b.push(2);
        assert_eq!(Proof::from_bytes(&b), Err(Error::UnsupportedVersion(2)));
        let mut b = MAGIC.to_vec();
        b.extend_from_slice(&[1, 0x02]);
        assert_eq!(Proof::from_bytes(&b), Err(Error::UnsupportedHashOp(0x02)));
        let mut b = MAGIC.to_vec();
        b.extend_from_slice(&[1, 0x08, 0, 0]);
        assert_eq!(Proof::from_bytes(&b), Err(Error::Truncated));
        assert_eq!(
            Proof::from_bytes(&vec![0; MAX_PROOF_BYTES + 1]),
            Err(Error::TooLarge)
        );
    }
}
