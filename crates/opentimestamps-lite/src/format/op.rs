//! Operations. Variants are declared in tag order so the derived ordering matches upstream:
//! by tag, then by argument bytes.

use super::encoding::{Reader, write_varbytes};
use crate::Error;
use crate::util::sha256;

/// Upper bound on any message and on op arguments.
pub(crate) const MAX_MSG: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Op {
    Sha1,
    Ripemd160,
    Sha256,
    Keccak256,
    Append(Vec<u8>),
    Prepend(Vec<u8>),
    Reverse,
    Hexlify,
}

impl Op {
    pub(crate) fn tag(&self) -> u8 {
        match self {
            Op::Sha1 => 0x02,
            Op::Ripemd160 => 0x03,
            Op::Sha256 => 0x08,
            Op::Keccak256 => 0x67,
            Op::Append(_) => 0xf0,
            Op::Prepend(_) => 0xf1,
            Op::Reverse => 0xf2,
            Op::Hexlify => 0xf3,
        }
    }

    pub(crate) fn read(tag: u8, r: &mut Reader<'_>) -> Result<Op, Error> {
        Ok(match tag {
            0x02 => Op::Sha1,
            0x03 => Op::Ripemd160,
            0x08 => Op::Sha256,
            0x67 => Op::Keccak256,
            0xf0 => Op::Append(r.read_varbytes(MAX_MSG, 1)?.to_vec()),
            0xf1 => Op::Prepend(r.read_varbytes(MAX_MSG, 1)?.to_vec()),
            0xf2 => Op::Reverse,
            0xf3 => Op::Hexlify,
            other => return Err(Error::BadOpTag(other)),
        })
    }

    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        out.push(self.tag());
        if let Op::Append(arg) | Op::Prepend(arg) = self {
            write_varbytes(out, arg);
        }
    }

    /// Applies the op. `Ok(None)` for ops DraftStamp does not evaluate (preserved only).
    pub(crate) fn apply(&self, msg: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        if msg.len() > MAX_MSG {
            return Err(Error::MessageTooLong);
        }
        let out = match self {
            Op::Sha256 => sha256(&[msg]).to_vec(),
            Op::Append(arg) => [msg, arg].concat(),
            Op::Prepend(arg) => [arg, msg].concat(),
            _ => return Ok(None),
        };
        if out.len() > MAX_MSG {
            return Err(Error::MessageTooLong);
        }
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_matches_tags() {
        let mut ops = vec![
            Op::Hexlify,
            Op::Prepend(vec![1]),
            Op::Append(vec![2]),
            Op::Append(vec![1, 5]),
            Op::Sha256,
            Op::Keccak256,
            Op::Sha1,
            Op::Reverse,
            Op::Ripemd160,
        ];
        ops.sort();
        let tags: Vec<u8> = ops.iter().map(Op::tag).collect();
        assert_eq!(tags, [0x02, 0x03, 0x08, 0x67, 0xf0, 0xf0, 0xf1, 0xf2, 0xf3]);
        assert_eq!(
            ops[4],
            Op::Append(vec![1, 5]),
            "same tag: compare argument bytes"
        );
    }

    #[test]
    fn apply() {
        assert_eq!(
            Op::Append(b"cd".to_vec()).apply(b"ab").unwrap().unwrap(),
            b"abcd"
        );
        assert_eq!(
            Op::Prepend(b"cd".to_vec()).apply(b"ab").unwrap().unwrap(),
            b"cdab"
        );
        assert_eq!(
            Op::Sha256.apply(b"").unwrap().unwrap()[..4],
            [0xe3, 0xb0, 0xc4, 0x42]
        );
        assert_eq!(Op::Sha1.apply(b"x").unwrap(), None);
        assert_eq!(
            Op::Append(vec![0; 10]).apply(&[0; MAX_MSG]),
            Err(Error::MessageTooLong)
        );
    }

    #[test]
    fn read_rejects_empty_and_unknown() {
        assert_eq!(
            Op::read(0xf0, &mut Reader::new(&[0])),
            Err(Error::LengthOutOfRange)
        );
        assert_eq!(
            Op::read(0x42, &mut Reader::new(&[])),
            Err(Error::BadOpTag(0x42))
        );
    }
}
