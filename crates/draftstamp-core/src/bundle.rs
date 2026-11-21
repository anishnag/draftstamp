//! `Bundle`: one stamp's snapshot, receipt and proof, and what can be done with it.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use opentimestamps_lite::{Http, Proof, Status};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::Error;
use crate::commitment::hash::sha256;
use crate::commitment::paths::{Canonical, canonicalize};
use crate::commitment::stamp::{File, commit};
use crate::commitment::{Advisory, Receipt};
use crate::id::{StampId, iso_time};

/// The stdlib-only verifier included in every export.
const VERIFY_PY: &str = include_str!("../../../tools/verify.py");

/// Folder names never descended into.
const SKIPPED_DIRS: [&str; 3] = [".git", ".draftstamp", "__MACOSX"];

/// Optional tags recorded in the receipt's advisory section.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StampOptions {
    /// A note about this draft.
    pub label: Option<String>,
    /// The project tag. Defaults to the stamped folder's name.
    pub project: Option<String>,
}

/// The outcome of a successful verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verified {
    /// How many files were re-hashed and matched.
    pub files: usize,
    /// The proof's status, checked against Bitcoin where possible.
    pub status: Status,
}

/// One stamp: the exact files (as a zip), the receipt, and the OpenTimestamps proof.
#[derive(Clone, Debug)]
pub struct Bundle {
    id: StampId,
    receipt: Receipt,
    proof: Proof,
    snapshot: Vec<u8>,
}

impl Bundle {
    /// Commits every file in `folder`, timestamps the commitment, and returns the new bundle.
    pub fn create(http: &impl Http, folder: &Path, opts: &StampOptions) -> Result<Bundle, Error> {
        let files = read_folder(folder)?;
        let stamped = commit(&files)?;
        let commitment = stamped.committed.commitment();
        let proof = opentimestamps_lite::stamp(http, commitment.0)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let id = StampId::new(now, &commitment);
        let project = opts.project.clone().or_else(|| {
            folder
                .canonicalize()
                .ok()?
                .file_name()?
                .to_str()
                .map(str::to_owned)
        });
        let advisory = Advisory {
            id: Some(id.to_string()),
            created_at: Some(iso_time(now)),
            project,
            label: opts.label.clone(),
        };
        let snapshot = write_snapshot(&files, &stamped.committed)?;
        Ok(Bundle {
            id,
            receipt: Receipt::new(stamped.committed, advisory),
            proof,
            snapshot,
        })
    }

    /// Reassembles a stored bundle. The receipt must already be parsed (and so validated).
    pub(crate) fn from_parts(
        id: StampId,
        receipt: Receipt,
        proof: Proof,
        snapshot: Vec<u8>,
    ) -> Bundle {
        Bundle {
            id,
            receipt,
            proof,
            snapshot,
        }
    }

    /// Asks the calendars for the proof's path to Bitcoin and checks it. Only adds to the proof.
    pub fn upgrade(&mut self, http: &impl Http) -> Result<Status, Error> {
        Ok(opentimestamps_lite::upgrade(http, &mut self.proof)?)
    }

    /// Re-hashes every file in the snapshot, checks them against the receipt, checks the proof is
    /// for this commitment, then checks the proof against Bitcoin. The bundle is not modified.
    pub fn verify(&self, http: &impl Http) -> Result<Verified, Error> {
        let files = self.check_offline()?;
        let mut proof = self.proof.clone();
        let status = opentimestamps_lite::upgrade(http, &mut proof)?;
        Ok(Verified { files, status })
    }

    /// Verification steps that need no network. Returns how many files matched.
    pub(crate) fn check_offline(&self) -> Result<usize, Error> {
        let leaves = self.receipt.committed().leaves();
        let mut archive = ZipArchive::new(Cursor::new(&self.snapshot))?;
        let mut found: BTreeMap<String, ()> = BTreeMap::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name()?.into_owned();
            let leaf = leaves.iter().find(|l| l.path == name).ok_or_else(|| {
                Error::Mismatch(format!("snapshot has a file not in the receipt: {name}"))
            })?;
            let mut bytes = Vec::new();
            (&mut entry).take(leaf.size + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 != leaf.size || sha256(&[&bytes]) != leaf.sha256 {
                return Err(Error::Mismatch(format!(
                    "{name} does not match the receipt"
                )));
            }
            if found.insert(name.clone(), ()).is_some() {
                return Err(Error::Mismatch(format!("snapshot has {name} twice")));
            }
        }
        if let Some(missing) = leaves.iter().find(|l| !found.contains_key(&l.path)) {
            return Err(Error::Mismatch(format!(
                "snapshot is missing {}",
                missing.path
            )));
        }
        if self.proof.digest() != self.receipt.commitment().0 {
            return Err(Error::Mismatch(
                "the proof is for a different commitment".into(),
            ));
        }
        Ok(leaves.len())
    }

    /// Writes one zip for a reviewer: `<id>/snapshot.zip`, `<id>/receipt.json`, `<id>/proof.ots`,
    /// and `<id>/verify.py`, which checks the bundle without DraftStamp.
    pub fn export(&self, out: &Path) -> Result<(), Error> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in [
            ("snapshot.zip", self.snapshot.clone()),
            ("receipt.json", self.receipt.to_json().into_bytes()),
            ("proof.ots", self.proof.to_bytes()),
            ("verify.py", VERIFY_PY.as_bytes().to_vec()),
        ] {
            zip.start_file(format!("{}/{name}", self.id), stored)?;
            zip.write_all(&bytes)?;
        }
        fs::write(out, zip.finish()?.into_inner())?;
        Ok(())
    }

    /// This stamp's id.
    pub fn id(&self) -> &StampId {
        &self.id
    }

    /// The receipt.
    pub fn receipt(&self) -> &Receipt {
        &self.receipt
    }

    /// The OpenTimestamps proof.
    pub fn proof(&self) -> &Proof {
        &self.proof
    }

    pub(crate) fn snapshot(&self) -> &[u8] {
        &self.snapshot
    }
}

/// Every regular file under `folder`, with `/`-separated relative paths. Symlinks are skipped;
/// skipped folders are never entered. Junk files are dropped later, by canonicalization.
fn read_folder(folder: &Path) -> Result<Vec<File>, Error> {
    let mut files = Vec::new();
    let mut stack = vec![(folder.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|raw| Error::InvalidPath {
                    path: format!("{prefix}{}", raw.to_string_lossy()),
                    reason: "not valid UTF-8",
                })?;
            let kind = entry.file_type()?;
            let rel = format!("{prefix}{name}");
            if kind.is_dir() {
                if !SKIPPED_DIRS.contains(&name.as_str()) {
                    stack.push((entry.path(), format!("{rel}/")));
                }
            } else if kind.is_file() {
                files.push(File {
                    path: rel,
                    bytes: fs::read(entry.path())?,
                });
            }
        }
    }
    Ok(files)
}

/// A zip of exactly the stamped files at their canonical paths.
fn write_snapshot(
    files: &[File],
    committed: &crate::commitment::Committed,
) -> Result<Vec<u8>, Error> {
    let by_path: BTreeMap<String, &[u8]> = files
        .iter()
        .filter_map(|f| match canonicalize(&f.path) {
            Ok(Canonical::Keep(p)) => Some((p, f.bytes.as_slice())),
            _ => None,
        })
        .collect();
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true);
    for leaf in committed.leaves() {
        zip.start_file(leaf.path.as_str(), options)?;
        zip.write_all(by_path[&leaf.path])?;
    }
    Ok(zip.finish()?.into_inner())
}
