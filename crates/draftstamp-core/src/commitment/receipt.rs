//! Receipts: committed data plus advisory metadata.

use serde::{Deserialize, Serialize};

use super::hash::{Digest, Salt};
use super::paths::is_canonical;
use super::tree;
use crate::Error;

/// The tree algorithm identifier for v1.
pub const TREE_ALGORITHM: &str = "draftstamp-merkle-v1";

const FORMAT: &str = "draftstamp.receipt";
const VERSION: u32 = 1;

/// One stamped file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Leaf {
    /// Canonical path.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// SHA-256 of the file's bytes.
    pub sha256: Digest,
    /// Random salt mixed into this leaf's hash.
    pub salt: Salt,
}

/// The part of a receipt that the commitment covers. Always internally consistent:
/// it can only be built by stamping or by parsing a receipt that checks out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Committed {
    tree: String,
    leaves: Vec<Leaf>,
    root: Digest,
    commitment: Digest,
}

impl Committed {
    /// Builds the tree over leaves that are already canonical, sorted and unique.
    pub(crate) fn from_leaves(leaves: Vec<Leaf>) -> Result<Self, Error> {
        let hashes: Vec<Digest> = leaves
            .iter()
            .map(|l| tree::leaf_hash(&l.salt, &l.path, &l.sha256))
            .collect();
        let root = tree::root(&hashes).ok_or(Error::EmptyFolder)?;
        Ok(Committed {
            tree: TREE_ALGORITHM.to_owned(),
            leaves,
            commitment: tree::commitment(&root),
            root,
        })
    }

    /// The stamped files, in canonical order.
    pub fn leaves(&self) -> &[Leaf] {
        &self.leaves
    }

    /// The Merkle root over all leaves.
    pub fn root(&self) -> Digest {
        self.root
    }

    /// The value that is timestamped.
    pub fn commitment(&self) -> Digest {
        self.commitment
    }
}

/// A receipt: committed data plus advisory metadata.
///
/// Advisory fields are for people and tools; they never decide validity, and unknown ones
/// are ignored when reading.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Receipt {
    format: String,
    version: u32,
    committed: Committed,
    advisory: Advisory,
}

/// Advisory metadata: never part of the commitment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Advisory {
    /// The stamp id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// When the stamp was made, as an ISO 8601 UTC time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// The project tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// A note about this draft.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Receipt {
    /// The file paths selected by `paths`: a file's path, a folder's path (with or without a
    /// trailing `/`, every file under it), or `.` (every file). In tree order, without repeats;
    /// a path that selects nothing is an error.
    pub fn select(&self, paths: &[&str]) -> Result<Vec<String>, Error> {
        let leaves = self.committed().leaves();
        let mut chosen = vec![false; leaves.len()];
        for raw in paths {
            let path = raw.trim_start_matches("./").trim_end_matches('/');
            let mut any = false;
            for (i, leaf) in leaves.iter().enumerate() {
                let under = leaf
                    .path
                    .strip_prefix(path)
                    .is_some_and(|rest| rest.starts_with('/'));
                if path.is_empty() || path == "." || leaf.path == path || under {
                    chosen[i] = true;
                    any = true;
                }
            }
            if !any {
                return Err(Error::NotFound(format!("{raw} in this stamp")));
            }
        }
        Ok(leaves
            .iter()
            .zip(chosen)
            .filter(|(_, c)| *c)
            .map(|(l, _)| l.path.clone())
            .collect())
    }

    /// Wraps committed data and advisory metadata into a receipt.
    pub fn new(committed: Committed, advisory: Advisory) -> Self {
        Receipt {
            format: FORMAT.to_owned(),
            version: VERSION,
            committed,
            advisory,
        }
    }

    /// Parses a receipt and checks every rule in the spec, including that the
    /// stated root and commitment match the leaves. Fails closed on anything unknown.
    pub fn parse(json: &str) -> Result<Self, Error> {
        let raw: RawReceipt =
            serde_json::from_str(json).map_err(|e| Error::InvalidReceipt(e.to_string()))?;
        let bad = |why: String| Err(Error::InvalidReceipt(why));

        if raw.format != FORMAT {
            return bad(format!("unknown format {:?}", raw.format));
        }
        if raw.version != VERSION {
            return bad(format!("unsupported version {}", raw.version));
        }
        let c = raw.committed;
        if c.tree != TREE_ALGORITHM {
            return bad(format!("unknown tree algorithm {:?}", c.tree));
        }
        if c.leaves.is_empty() {
            return bad("no leaves".into());
        }
        for leaf in &c.leaves {
            if !is_canonical(&leaf.path) {
                return bad(format!("path {:?} is not canonical", leaf.path));
            }
        }
        if let Some(w) = c.leaves.windows(2).find(|w| w[0].path >= w[1].path) {
            return bad(format!("leaves not in canonical order at {:?}", w[1].path));
        }

        let committed = Committed::from_leaves(c.leaves)?;
        if committed.root != c.root {
            return bad("root does not match the leaves".into());
        }
        if committed.commitment != c.commitment {
            return bad("commitment does not match the root".into());
        }
        Ok(Receipt::new(committed, raw.advisory))
    }

    /// Serializes the receipt as pretty-printed JSON with a trailing newline.
    pub fn to_json(&self) -> String {
        let mut out = serde_json::to_string_pretty(self).expect("receipt always serializes");
        out.push('\n');
        out
    }

    /// The committed data.
    pub fn committed(&self) -> &Committed {
        &self.committed
    }

    /// The value the OpenTimestamps proof must attest.
    pub fn commitment(&self) -> Digest {
        self.committed.commitment
    }

    /// Advisory metadata. Never used to decide validity.
    pub fn advisory(&self) -> &Advisory {
        &self.advisory
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReceipt {
    format: String,
    version: u32,
    committed: RawCommitted,
    #[serde(default)]
    advisory: Advisory,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommitted {
    tree: String,
    leaves: Vec<Leaf>,
    root: Digest,
    commitment: Digest,
}
