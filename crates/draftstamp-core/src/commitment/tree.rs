//! Leaf hashing, the RFC 6962 Merkle tree, and the commitment (spec section 7.2).

use super::hash::{Digest, Salt, sha256};
use super::paths::MAX_PATH_BYTES;

const LEAF_PREFIX: u8 = 0x00;
const NODE_PREFIX: u8 = 0x01;
const COMMITMENT_PREFIX: u8 = 0x02;
const COMMITMENT_TAG: &[u8] = b"draftstamp:v1";

/// `SHA256(0x00 || salt || u32be(len(path)) || path || file_hash)`
pub(crate) fn leaf_hash(salt: &Salt, path: &str, file_hash: &Digest) -> Digest {
    debug_assert!(path.len() <= MAX_PATH_BYTES);
    let len = (path.len() as u32).to_be_bytes();
    sha256(&[&[LEAF_PREFIX], &salt.0, &len, path.as_bytes(), &file_hash.0])
}

/// `SHA256(0x01 || left || right)`
fn node_hash(left: &Digest, right: &Digest) -> Digest {
    sha256(&[&[NODE_PREFIX], &left.0, &right.0])
}

/// RFC 6962 Merkle Tree Hash over leaf hashes. `None` for an empty list.
pub(crate) fn root(leaves: &[Digest]) -> Option<Digest> {
    match leaves.len() {
        0 => None,
        1 => Some(leaves[0]),
        n => {
            // Split at the largest power of two strictly less than n.
            let k = 1usize << (usize::BITS - 1 - (n - 1).leading_zeros());
            Some(node_hash(&root(&leaves[..k])?, &root(&leaves[k..])?))
        }
    }
}

/// `SHA256(0x02 || "draftstamp:v1" || root)`: the value that is timestamped.
pub(crate) fn commitment(root: &Digest) -> Digest {
    sha256(&[&[COMMITMENT_PREFIX], COMMITMENT_TAG, &root.0])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(i: u8) -> Digest {
        Digest([i; 32])
    }

    #[test]
    fn tree_shapes_follow_rfc_6962() {
        assert_eq!(root(&[]), None);
        assert_eq!(root(&[d(1)]), Some(d(1)));
        assert_eq!(root(&[d(1), d(2)]), Some(node_hash(&d(1), &d(2))));
        // Three leaves split 2 + 1, never duplicating the last leaf.
        let three = node_hash(&node_hash(&d(1), &d(2)), &d(3));
        assert_eq!(root(&[d(1), d(2), d(3)]), Some(three));
        // Five leaves split 4 + 1.
        let four = node_hash(&node_hash(&d(1), &d(2)), &node_hash(&d(3), &d(4)));
        assert_eq!(
            root(&[d(1), d(2), d(3), d(4), d(5)]),
            Some(node_hash(&four, &d(5)))
        );
    }

    #[test]
    fn domain_separation() {
        let a = d(7);
        assert_ne!(commitment(&a), node_hash(&a, &a));
        assert_ne!(leaf_hash(&Salt([0; 16]), "x", &a), commitment(&a));
    }
}
