//! varuint, varbytes and a bounds-checked reader.

use crate::Error;

const MAX_VARUINT_BYTES: usize = 9;

pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8, Error> {
        let b = *self
            .buf
            .get(self.pos)
            .ok_or(Error::Malformed("truncated"))?;
        self.pos += 1;
        Ok(b)
    }

    pub(crate) fn read_exact(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(Error::Malformed("truncated"))?;
        let s = self
            .buf
            .get(self.pos..end)
            .ok_or(Error::Malformed("truncated"))?;
        self.pos = end;
        Ok(s)
    }

    /// Unsigned LEB128, at most 9 bytes. Non-minimal encodings are accepted, as upstream.
    pub(crate) fn read_varuint(&mut self) -> Result<u64, Error> {
        let mut value = 0u64;
        for i in 0..MAX_VARUINT_BYTES {
            let b = self.read_u8()?;
            value |= u64::from(b & 0x7f) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(Error::Malformed("varuint longer than 9 bytes"))
    }

    pub(crate) fn read_varbytes(&mut self, max: usize, min: usize) -> Result<&'a [u8], Error> {
        let len = self.read_varuint()?;
        if len > max as u64 || len < min as u64 {
            return Err(Error::Malformed("length out of range"));
        }
        self.read_exact(len as usize)
    }

    pub(crate) fn finish(&self) -> Result<(), Error> {
        if self.pos == self.buf.len() {
            Ok(())
        } else {
            Err(Error::Malformed("trailing bytes"))
        }
    }
}

/// Minimal unsigned LEB128.
pub(crate) fn write_varuint(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

pub(crate) fn write_varbytes(out: &mut Vec<u8>, bytes: &[u8]) {
    write_varuint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(n: u64) -> Vec<u8> {
        let mut v = Vec::new();
        write_varuint(&mut v, n);
        v
    }

    #[test]
    fn varuint() {
        assert_eq!(enc(0), [0]);
        assert_eq!(enc(127), [0x7f]);
        assert_eq!(enc(128), [0x80, 0x01]);
        assert_eq!(enc(300), [0xac, 0x02]);
        for n in [0, 1, 127, 128, 16384, u32::MAX as u64, (1 << 63) - 1] {
            assert_eq!(Reader::new(&enc(n)).read_varuint().unwrap(), n);
        }
        assert_eq!(
            Reader::new(&[0x80, 0x00]).read_varuint().unwrap(),
            0,
            "non-minimal accepted"
        );
        assert_eq!(
            Reader::new(&[0xff; 9]).read_varuint(),
            Err(Error::Malformed("varuint longer than 9 bytes"))
        );
        assert_eq!(
            Reader::new(&[0x80]).read_varuint(),
            Err(Error::Malformed("truncated"))
        );
    }

    #[test]
    fn varbytes_bounds() {
        let mut v = Vec::new();
        write_varbytes(&mut v, b"abc");
        assert_eq!(Reader::new(&v).read_varbytes(3, 1).unwrap(), b"abc");
        assert_eq!(
            Reader::new(&v).read_varbytes(2, 0),
            Err(Error::Malformed("length out of range"))
        );
        assert_eq!(
            Reader::new(&[0]).read_varbytes(5, 1),
            Err(Error::Malformed("length out of range"))
        );
        assert_eq!(
            Reader::new(&[5, 1]).read_varbytes(5, 0),
            Err(Error::Malformed("truncated"))
        );
    }
}
