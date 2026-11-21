//! Leaf hashing, the RFC 6962 Merkle tree, inclusion paths, and the commitment.

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

/// Which side of the running hash a sibling sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Left,
    Right,
}

/// RFC 6962 inclusion path (PATH) for leaf `index`: the siblings from the leaf up to the root.
/// Empty for a one-leaf tree; `None` if `index` is out of range.
pub(crate) fn audit_path(leaves: &[Digest], index: usize) -> Option<Vec<(Digest, Side)>> {
    let n = leaves.len();
    if index >= n {
        return None;
    }
    if n == 1 {
        return Some(Vec::new());
    }
    let k = 1usize << (usize::BITS - 1 - (n - 1).leading_zeros());
    Some(if index < k {
        let mut path = audit_path(&leaves[..k], index)?;
        path.push((root(&leaves[k..])?, Side::Right));
        path
    } else {
        let mut path = audit_path(&leaves[k..], index - k)?;
        path.push((root(&leaves[..k])?, Side::Left));
        path
    })
}

/// The bytes that turn a file hash into its leaf hash, and a node's child into the node,
/// as prepend/append arguments for an OpenTimestamps recipe.
pub(crate) fn leaf_prefix(salt: &Salt, path: &str) -> Vec<u8> {
    let len = (path.len() as u32).to_be_bytes();
    [&[LEAF_PREFIX][..], &salt.0, &len, path.as_bytes()].concat()
}

pub(crate) const NODE_PREFIX_BYTES: [u8; 1] = [NODE_PREFIX];

pub(crate) fn commitment_prefix() -> Vec<u8> {
    [&[COMMITMENT_PREFIX][..], COMMITMENT_TAG].concat()
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
    fn audit_paths_rebuild_the_root() {
        for n in 1..=9u8 {
            let leaves: Vec<Digest> = (0..n).map(d).collect();
            let expected = root(&leaves).unwrap();
            for i in 0..n as usize {
                let mut h = leaves[i];
                for (sibling, side) in audit_path(&leaves, i).unwrap() {
                    h = match side {
                        Side::Right => node_hash(&h, &sibling),
                        Side::Left => node_hash(&sibling, &h),
                    };
                }
                assert_eq!(h, expected, "n={n} i={i}");
            }
            assert!(audit_path(&leaves, n as usize).is_none());
        }
    }

    #[test]
    fn domain_separation() {
        let a = d(7);
        assert_ne!(commitment(&a), node_hash(&a, &a));
        assert_ne!(leaf_hash(&Salt([0; 16]), "x", &a), commitment(&a));
    }
}
