//! Questions about stored stamps, answered through any `Vault`.

use std::fs;
use std::path::Path;

use opentimestamps_lite::Status;

use crate::Error;
use crate::commitment::hash::sha256;
use crate::id::StampId;
use crate::vault::Vault;

/// A summary of one stored stamp.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StampInfo {
    /// The stamp's id.
    pub id: StampId,
    /// The project tag, if any.
    pub project: Option<String>,
    /// The label, if any.
    pub label: Option<String>,
    /// How many files the stamp covers.
    pub files: usize,
    /// Status from the stored proof alone: `Pending` or `Claimed`.
    pub status: Status,
}

/// A stamp containing a given file's exact bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// The stamp.
    pub stamp: StampInfo,
    /// The file's path inside that stamp.
    pub path: String,
}

fn info(vault: &impl Vault, id: &StampId) -> Result<StampInfo, Error> {
    let receipt = vault.receipt(id)?;
    Ok(StampInfo {
        id: id.clone(),
        project: receipt.advisory().project.clone(),
        label: receipt.advisory().label.clone(),
        files: receipt.committed().leaves().len(),
        status: vault.proof(id)?.status(),
    })
}

/// Stored stamps, newest first, optionally only those tagged with `project`.
pub fn history(vault: &impl Vault, project: Option<&str>) -> Result<Vec<StampInfo>, Error> {
    let mut out = Vec::new();
    for id in vault.list()?.iter().rev() {
        let stamp = info(vault, id)?;
        if project.is_none() || stamp.project.as_deref() == project {
            out.push(stamp);
        }
    }
    Ok(out)
}

/// Every stored stamp containing exactly the bytes of `file`, newest first.
pub fn find(vault: &impl Vault, file: &Path) -> Result<Vec<Found>, Error> {
    let target = sha256(&[&fs::read(file)?]);
    let mut out = Vec::new();
    for id in vault.list()?.iter().rev() {
        let receipt = vault.receipt(id)?;
        for leaf in receipt
            .committed()
            .leaves()
            .iter()
            .filter(|l| l.sha256 == target)
        {
            out.push(Found {
                stamp: info(vault, id)?,
                path: leaf.path.clone(),
            });
        }
    }
    Ok(out)
}
