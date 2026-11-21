//! Stamping a set of files into committed data.

use super::hash::{Salt, sha256};
use super::paths::{Canonical, canonicalize};
use super::receipt::{Committed, Leaf};
use crate::Error;

/// One input file: a path relative to the project root using `/` separators, and the
/// file's exact bytes. Hosts must read bytes as binary, never decode them as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct File {
    /// Path relative to the project root.
    pub(crate) path: String,
    /// The file's exact bytes.
    pub(crate) bytes: Vec<u8>,
}

/// The result of stamping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stamped {
    /// Leaves, root and commitment.
    pub(crate) committed: Committed,
    /// Input paths left out as junk (for example `.DS_Store`), sorted.
    pub(crate) skipped: Vec<String>,
}

/// Stamps `files` with fresh random salts. Input order does not matter.
pub(crate) fn commit(files: &[File]) -> Result<Stamped, Error> {
    let prepared = prepare(files)?;
    let mut random = vec![0u8; 16 * prepared.included.len()];
    getrandom::getrandom(&mut random).map_err(|_| Error::Random)?;
    let mut salts = random.as_chunks::<16>().0.iter();
    prepared.build(|| Salt(*salts.next().expect("one salt per file")))
}

/// Stamps `files` taking salts from `next_salt`, called once per file in canonical path
/// order. For tests and test vectors; real stamps must use [`stamp`].
#[cfg(test)]
pub(crate) fn commit_with_salts(
    files: &[File],
    next_salt: impl FnMut() -> Salt,
) -> Result<Stamped, Error> {
    prepare(files)?.build(next_salt)
}

struct Prepared<'a> {
    included: Vec<(String, &'a [u8])>,
    skipped: Vec<String>,
}

fn prepare(files: &[File]) -> Result<Prepared<'_>, Error> {
    let mut included = Vec::with_capacity(files.len());
    let mut skipped = Vec::new();
    for file in files {
        match canonicalize(&file.path)? {
            Canonical::Keep(path) => included.push((path, file.bytes.as_slice())),
            Canonical::Skip => skipped.push(file.path.clone()),
        }
    }
    // `str` ordering is byte-wise on UTF-8, which is the canonical order.
    included.sort_by(|a, b| a.0.cmp(&b.0));
    if let Some(w) = included.windows(2).find(|w| w[0].0 == w[1].0) {
        return Err(Error::DuplicatePath(w[0].0.clone()));
    }
    if included.is_empty() {
        return Err(Error::EmptyFolder);
    }
    skipped.sort();
    Ok(Prepared { included, skipped })
}

impl Prepared<'_> {
    fn build(self, mut next_salt: impl FnMut() -> Salt) -> Result<Stamped, Error> {
        let leaves = self
            .included
            .into_iter()
            .map(|(path, bytes)| Leaf {
                path,
                size: bytes.len() as u64,
                sha256: sha256(&[bytes]),
                salt: next_salt(),
            })
            .collect();
        Ok(Stamped {
            committed: Committed::from_leaves(leaves)?,
            skipped: self.skipped,
        })
    }
}
