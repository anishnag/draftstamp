//! `Bundle`: one stamp's snapshot, receipt and proof, and what can be done with it.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use opentimestamps_lite::{Http, Proof, Status, Step};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::Error;
use crate::commitment::hash::sha256;
use crate::commitment::paths::{Canonical, canonicalize};
use crate::commitment::stamp::{File, commit};
use crate::commitment::tree::{
    NODE_PREFIX_BYTES, Side, audit_path, commitment_prefix, leaf_hash, leaf_prefix,
};
use crate::commitment::{Advisory, Receipt};
use crate::config::default_project;
use crate::id::{StampId, iso_time};
use crate::ignore::{IGNORE_FILE, Ignore};

/// Folder names never descended into.
const SKIPPED_DIRS: [&str; 3] = [".git", ".draftstamp", "__MACOSX"];

/// Optional tags recorded in the receipt's advisory section.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StampOptions {
    /// A note about this draft.
    pub label: Option<String>,
    /// The project tag. Defaults to [`default_project`](crate::default_project).
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

/// One file from a stamp, with its own standard OpenTimestamps proof.
///
/// Anyone can check it with the official client: `ots verify -f <file> <file>.ots`. It reveals
/// this file's name and salt, and nothing about the stamp's other files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shared {
    /// The file's path inside the stamp.
    pub path: String,
    /// The file's exact bytes.
    pub bytes: Vec<u8>,
    /// A proof whose digest is the file's SHA-256.
    pub proof: Proof,
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
    /// Commits every file in the folder at `path` (or the single file at `path`), timestamps the
    /// commitment, and returns the new bundle.
    pub fn create(http: &impl Http, path: &Path, opts: &StampOptions) -> Result<Bundle, Error> {
        let files = if path.is_dir() {
            read_folder(path)?
        } else {
            read_one_file(path)?
        };
        let stamped = commit(&files)?;
        let commitment = stamped.committed.commitment();
        let proof = opentimestamps_lite::stamp(http, commitment.0)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let id = StampId::new(now, &commitment);
        let project = match &opts.project {
            Some(p) => Some(p.clone()),
            None => default_project(path)?,
        };
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
        let files = self.check()?;
        let mut proof = self.proof.clone();
        let status = opentimestamps_lite::upgrade(http, &mut proof)?;
        Ok(Verified { files, status })
    }

    /// Verification steps that need no network. Returns how many files matched.
    /// Offline: every file in the snapshot matches the receipt, the receipt's tree gives its
    /// commitment, and the proof is for that commitment. Returns the number of files.
    pub fn check(&self) -> Result<usize, Error> {
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

    /// One file and its own proof: the file's path through this stamp's tree, followed by the
    /// stamp's proof. Upgrade first so the file's proof already reaches Bitcoin.
    pub fn share(&self, path: &str) -> Result<Shared, Error> {
        let index = self
            .receipt
            .committed()
            .leaves()
            .iter()
            .position(|l| l.path == path)
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        Ok(Shared {
            path: path.to_owned(),
            bytes: self.snapshot_file(path)?,
            proof: self.file_proof(index)?,
        })
    }

    fn file_proof(&self, index: usize) -> Result<Proof, Error> {
        let leaves = self.receipt.committed().leaves();
        let hashes: Vec<_> = leaves
            .iter()
            .map(|l| leaf_hash(&l.salt, &l.path, &l.sha256))
            .collect();
        let leaf = &leaves[index];
        let mut steps = vec![
            Step::Prepend(leaf_prefix(&leaf.salt, &leaf.path)),
            Step::Sha256,
        ];
        for (sibling, side) in audit_path(&hashes, index).expect("index is in range") {
            match side {
                Side::Right => steps.extend([
                    Step::Prepend(NODE_PREFIX_BYTES.to_vec()),
                    Step::Append(sibling.0.to_vec()),
                    Step::Sha256,
                ]),
                Side::Left => steps.extend([
                    Step::Prepend([&NODE_PREFIX_BYTES[..], &sibling.0].concat()),
                    Step::Sha256,
                ]),
            }
        }
        steps.extend([Step::Prepend(commitment_prefix()), Step::Sha256]);
        Ok(Proof::from_steps(leaf.sha256.0, &steps, &self.proof)?)
    }

    /// One file's bytes from the snapshot, checked against the receipt.
    fn snapshot_file(&self, path: &str) -> Result<Vec<u8>, Error> {
        let leaf = self
            .receipt
            .committed()
            .leaves()
            .iter()
            .find(|l| l.path == path)
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        let mut archive = ZipArchive::new(Cursor::new(&self.snapshot))?;
        let mut bytes = Vec::new();
        archive
            .by_name(path)?
            .take(leaf.size + 1)
            .read_to_end(&mut bytes)?;
        if sha256(&[&bytes]) != leaf.sha256 {
            return Err(Error::Mismatch(format!(
                "{path} does not match the receipt"
            )));
        }
        Ok(bytes)
    }

    /// Writes one zip for a reviewer: the bundle (`snapshot.zip`, `receipt.json`, `proof.ots`)
    /// and, under `files/`, every file with its own proof, checkable with `ots verify` alone.
    pub fn export(&self, out: &Path) -> Result<(), Error> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let stored = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .large_file(true);
        let mut entries = vec![
            ("README.txt".to_owned(), self.readme().into_bytes()),
            ("snapshot.zip".to_owned(), self.snapshot.clone()),
            (
                "receipt.json".to_owned(),
                self.receipt.to_json().into_bytes(),
            ),
            ("proof.ots".to_owned(), self.proof.to_bytes()),
        ];
        for (index, leaf) in self.receipt.committed().leaves().iter().enumerate() {
            entries.push((
                format!("files/{}", leaf.path),
                self.snapshot_file(&leaf.path)?,
            ));
            entries.push((
                format!("files/{}.ots", leaf.path),
                self.file_proof(index)?.to_bytes(),
            ));
        }
        for (name, bytes) in entries {
            zip.start_file(format!("{}/{name}", self.id), stored)?;
            zip.write_all(&bytes)?;
        }
        fs::write(out, zip.finish()?.into_inner())?;
        Ok(())
    }

    /// Plain-language instructions for whoever receives an export.
    fn readme(&self) -> String {
        let advisory = self.receipt.advisory();
        let mut about = format!("Stamp {}", self.id);
        if let Some(created) = &advisory.created_at {
            about.push_str(&format!(", made {created}"));
        }
        if let Some(project) = &advisory.project {
            about.push_str(&format!(", project \"{project}\""));
        }
        if let Some(label) = &advisory.label {
            about.push_str(&format!(", labelled \"{label}\""));
        }
        format!(
            "{about}. {} file(s).\n\
             \n\
             What this is\n\
             These files were timestamped in Bitcoin using OpenTimestamps. Each file in files/ has\n\
             a matching .ots proof showing that this exact file existed no later than the date of a\n\
             specific Bitcoin block.\n\
             \n\
             How to check a file yourself\n\
             1. Install the official OpenTimestamps client: https://github.com/opentimestamps/opentimestamps-client\n\
             2. Run:  ots verify -f files/<name> files/<name>.ots\n\
             It prints the Bitcoin block and date. If the file has been changed in any way, it fails.\n\
             \n\
             No DraftStamp software is needed. The other files here (snapshot.zip, receipt.json,\n\
             proof.ots) are the original stamp, kept for completeness.\n",
            self.receipt.committed().leaves().len()
        )
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
/// A single file named directly: a one-file stamp whose path is the file's name. The ignore
/// file does not apply.
fn read_one_file(file: &Path) -> Result<Vec<File>, Error> {
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::InvalidPath {
            path: file.to_string_lossy().into_owned(),
            reason: "not a valid UTF-8 file name",
        })?;
    Ok(vec![File {
        path: name.to_owned(),
        bytes: fs::read(file)?,
    }])
}

fn read_folder(folder: &Path) -> Result<Vec<File>, Error> {
    let ignore = match fs::read_to_string(folder.join(IGNORE_FILE)) {
        Ok(text) => Ignore::parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ignore::default(),
        Err(e) => return Err(e.into()),
    };
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
                if !SKIPPED_DIRS.contains(&name.as_str()) && !ignore.matches(&rel, true) {
                    stack.push((entry.path(), format!("{rel}/")));
                }
            } else if kind.is_file() && !ignore.matches(&rel, false) {
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
