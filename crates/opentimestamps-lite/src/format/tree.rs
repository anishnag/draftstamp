//! The timestamp tree: read, write, merge, graft and walk.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use super::attestation::Attestation;
use super::encoding::Reader;
use super::op::Op;
use crate::Error;

const MAX_DEPTH: usize = 256;
const FORK: u8 = 0xff;
const ATTESTATION: u8 = 0x00;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Node {
    pub(crate) attestations: BTreeSet<Attestation>,
    pub(crate) ops: BTreeMap<Op, Node>,
}

impl Node {
    /// Reads a tree whose first node holds `msg`. Messages are checked against the size
    /// limits wherever they can be evaluated, as upstream does while parsing.
    pub(crate) fn read(r: &mut Reader<'_>, msg: Option<&[u8]>) -> Result<Node, Error> {
        Self::read_depth(r, msg, MAX_DEPTH)
    }

    fn read_depth(r: &mut Reader<'_>, msg: Option<&[u8]>, depth: usize) -> Result<Node, Error> {
        if depth == 0 {
            return Err(Error::Malformed("tree deeper than 256 levels"));
        }
        let mut node = Node::default();
        let mut tag = r.read_u8()?;
        while tag == FORK {
            let item = r.read_u8()?;
            node.read_item(item, r, msg, depth)?;
            tag = r.read_u8()?;
        }
        node.read_item(tag, r, msg, depth)?;
        Ok(node)
    }

    fn read_item(
        &mut self,
        tag: u8,
        r: &mut Reader<'_>,
        msg: Option<&[u8]>,
        depth: usize,
    ) -> Result<(), Error> {
        if tag == ATTESTATION {
            self.attestations.insert(Attestation::read(r)?);
            return Ok(());
        }
        let op = Op::read(tag, r)?;
        if self.ops.contains_key(&op) {
            return Err(Error::Malformed("duplicate operation"));
        }
        let child_msg = match msg {
            Some(m) => op.apply(m)?,
            None => None,
        };
        let child = Self::read_depth(r, child_msg.as_deref(), depth - 1)?;
        self.ops.insert(op, child);
        Ok(())
    }

    /// Canonical form: attestations then ops, each sorted; `0xff` before every item but the last.
    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        debug_assert!(!self.is_empty(), "an empty node cannot be serialized");
        let total = self.attestations.len() + self.ops.len();
        let mut i = 0;
        for a in &self.attestations {
            i += 1;
            if i < total {
                out.push(FORK);
            }
            out.push(ATTESTATION);
            a.write(out);
        }
        for (op, child) in &self.ops {
            i += 1;
            if i < total {
                out.push(FORK);
            }
            op.write(out);
            child.write(out);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.attestations.is_empty() && self.ops.is_empty()
    }

    /// Adds everything in `other`. Returns whether anything new was added. Never removes.
    pub(crate) fn merge(&mut self, other: Node) -> bool {
        let mut changed = false;
        for a in other.attestations {
            changed |= self.attestations.insert(a);
        }
        for (op, child) in other.ops {
            match self.ops.entry(op) {
                Entry::Vacant(e) => {
                    e.insert(child);
                    changed = true;
                }
                Entry::Occupied(mut e) => changed |= e.get_mut().merge(child),
            }
        }
        changed
    }

    /// Merges `fragment` into every node whose message is `target`.
    pub(crate) fn graft(&mut self, msg: Option<&[u8]>, target: &[u8], fragment: &Node) -> bool {
        if msg == Some(target) {
            return self.merge(fragment.clone());
        }
        let mut changed = false;
        for (op, child) in self.ops.iter_mut() {
            let child_msg = msg.and_then(|m| op.apply(m).ok().flatten());
            changed |= child.graft(child_msg.as_deref(), target, fragment);
        }
        changed
    }

    /// Visits every node with its message (`None` below an op we do not evaluate).
    pub(crate) fn walk(&self, msg: Option<&[u8]>, f: &mut dyn FnMut(&Node, Option<&[u8]>)) {
        f(self, msg);
        for (op, child) in &self.ops {
            let child_msg = msg.and_then(|m| op.apply(m).ok().flatten());
            child.walk(child_msg.as_deref(), f);
        }
    }

    /// Every Bitcoin attestation reachable from here: (height, 32-byte message), lowest height first.
    pub(crate) fn bitcoin_claims(&self, msg: &[u8]) -> Vec<(u64, [u8; 32])> {
        let mut claims = Vec::new();
        self.walk(Some(msg), &mut |node, m| {
            for a in &node.attestations {
                if let (Attestation::Bitcoin { height }, Some(m)) = (a, m)
                    && let Ok(m32) = <[u8; 32]>::try_from(m)
                {
                    claims.push((*height, m32));
                }
            }
        });
        claims.sort();
        claims.dedup();
        claims
    }

    pub(crate) fn has_bitcoin(&self) -> bool {
        let mut found = false;
        self.walk(None, &mut |node, _| {
            found |= node
                .attestations
                .iter()
                .any(|a| matches!(a, Attestation::Bitcoin { .. }));
        });
        found
    }
}

/// Parses a calendar reply: a tree fragment starting from `msg`, with nothing after it.
pub(crate) fn read_fragment(bytes: &[u8], msg: &[u8]) -> Result<Node, Error> {
    let mut r = Reader::new(bytes);
    let node = Node::read(&mut r, Some(msg))?;
    r.finish()?;
    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(uri: &str) -> Node {
        Node {
            attestations: [Attestation::Pending { uri: uri.into() }].into(),
            ops: BTreeMap::new(),
        }
    }

    fn chain(ops: Vec<Op>, end: Node) -> Node {
        ops.into_iter().rev().fold(end, |child, op| Node {
            attestations: BTreeSet::new(),
            ops: [(op, child)].into(),
        })
    }

    #[test]
    fn write_read_round_trip_with_forks() {
        let mut root = chain(
            vec![Op::Append(vec![1; 16]), Op::Sha256],
            pending("https://a.example"),
        );
        root.merge(chain(
            vec![Op::Prepend(vec![7]), Op::Sha256],
            pending("https://b.example"),
        ));
        root.attestations.insert(Attestation::Bitcoin { height: 5 });
        let mut bytes = Vec::new();
        root.write(&mut bytes);
        assert_eq!(
            bytes[0], FORK,
            "first of several items carries a fork marker"
        );
        assert_eq!(read_fragment(&bytes, b"msg").unwrap(), root);
    }

    #[test]
    fn rejects_duplicates_depth_and_trailing_bytes() {
        let mut dup = vec![FORK, 0x08, 0x00];
        Attestation::Bitcoin { height: 1 }.write(&mut dup);
        dup.push(0x08);
        dup.push(0x00);
        Attestation::Bitcoin { height: 1 }.write(&mut dup);
        assert_eq!(
            read_fragment(&dup, b"m"),
            Err(Error::Malformed("duplicate operation"))
        );

        let mut deep = vec![0x08; MAX_DEPTH];
        deep.push(0x00);
        Attestation::Bitcoin { height: 1 }.write(&mut deep);
        assert_eq!(
            read_fragment(&deep, b"m"),
            Err(Error::Malformed("tree deeper than 256 levels"))
        );

        let mut ok = vec![0x00];
        Attestation::Bitcoin { height: 1 }.write(&mut ok);
        ok.push(0);
        assert_eq!(
            read_fragment(&ok, b"m"),
            Err(Error::Malformed("trailing bytes"))
        );
    }

    #[test]
    fn merge_only_adds_and_graft_targets_messages() {
        let mut root = chain(vec![Op::Sha256], pending("https://a.example"));
        let target = crate::util::sha256(&[b"m"]);
        let fragment = chain(
            vec![Op::Sha256],
            Node {
                attestations: [Attestation::Bitcoin { height: 7 }].into(),
                ops: BTreeMap::new(),
            },
        );
        assert!(root.graft(Some(b"m"), &target, &fragment));
        assert!(
            !root.graft(Some(b"m"), &target, &fragment),
            "second graft adds nothing"
        );
        assert!(root.has_bitcoin());
        let mut seen_pending = false;
        root.walk(None, &mut |n, _| {
            seen_pending |= n
                .attestations
                .iter()
                .any(|a| matches!(a, Attestation::Pending { .. }))
        });
        assert!(seen_pending, "pending attestation kept");
        let claims = root.bitcoin_claims(b"m");
        assert_eq!(claims, vec![(7, crate::util::sha256(&[&target]))]);
    }
}
