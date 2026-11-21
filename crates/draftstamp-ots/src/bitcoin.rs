//! Checking a proof's value against an 80-byte Bitcoin block header.

use crate::Error;
use crate::util::{hex_encode, sha256};

/// The Merkle root field (raw byte order) must equal `message`. Returns the block's time.
pub(crate) fn check_header(message: &[u8; 32], header: &[u8; 80]) -> Result<u32, Error> {
    if &header[36..68] != message {
        return Err(Error::HeaderMismatch);
    }
    Ok(u32::from_le_bytes(
        header[68..72].try_into().expect("4 bytes"),
    ))
}

/// The block hash as explorers display it: double SHA-256, byte-reversed, hex.
pub(crate) fn block_hash_hex(header: &[u8; 80]) -> String {
    let mut h = sha256(&[&sha256(&[header])]);
    h.reverse();
    hex_encode(&h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::hex_decode;

    // Bitcoin's genesis block header, as both explorers return it.
    const GENESIS: &str = "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c";

    fn genesis() -> [u8; 80] {
        hex_decode(GENESIS).unwrap().try_into().unwrap()
    }

    #[test]
    fn genesis_hash_and_root() {
        let h = genesis();
        assert_eq!(
            block_hash_hex(&h),
            "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f"
        );
        // Merkle root as displayed by explorers is 4a5e1e4b…; the header holds it byte-reversed.
        let mut root =
            hex_decode("4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b").unwrap();
        root.reverse();
        let root: [u8; 32] = root.try_into().unwrap();
        assert_eq!(check_header(&root, &h), Ok(1_231_006_505));
        assert_eq!(check_header(&[0; 32], &h), Err(Error::HeaderMismatch));
    }
}
