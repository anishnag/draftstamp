//! Storage for bundles. No network and no logic beyond reading and writing.

use std::fs;
use std::path::{Path, PathBuf};

use opentimestamps_lite::Proof;

use crate::Error;
use crate::bundle::Bundle;
use crate::commitment::Receipt;
use crate::id::StampId;

const SNAPSHOT: &str = "snapshot.zip";
const RECEIPT: &str = "receipt.json";
const PROOF: &str = "proof.ots";

/// Reads and writes stamped bundles. Bundles are never deleted or overwritten; replacing the
/// proof with an upgraded one is the only change allowed.
pub trait Vault {
    /// Stores a new bundle.
    fn put(&self, bundle: &Bundle) -> Result<StampId, Error>;
    /// Replaces a stored proof with an upgraded one for the same commitment.
    fn put_proof(&self, id: &StampId, proof: &Proof) -> Result<(), Error>;
    /// Reads a whole bundle.
    fn get(&self, id: &StampId) -> Result<Bundle, Error>;
    /// Reads only the receipt.
    fn receipt(&self, id: &StampId) -> Result<Receipt, Error>;
    /// Reads only the proof.
    fn proof(&self, id: &StampId) -> Result<Proof, Error>;
    /// Every stored stamp, oldest first.
    fn list(&self) -> Result<Vec<StampId>, Error>;
}

/// A vault in a folder on disk: `<root>/<stamp id>/{snapshot.zip, receipt.json, proof.ots}`.
#[derive(Clone, Debug)]
pub struct LocalVault {
    root: PathBuf,
}

impl LocalVault {
    /// Opens the vault at `root`, creating the folder if needed.
    pub fn open(root: &Path) -> Result<LocalVault, Error> {
        fs::create_dir_all(root)?;
        Ok(LocalVault {
            root: root.to_path_buf(),
        })
    }

    fn dir(&self, id: &StampId) -> PathBuf {
        self.root.join(id.as_str())
    }

    fn read(&self, id: &StampId, file: &str) -> Result<Vec<u8>, Error> {
        fs::read(self.dir(id).join(file)).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound(id.to_string()),
            _ => Error::Io(e),
        })
    }
}

impl Vault for LocalVault {
    /// Writes into a temporary folder and renames it into place, so a bundle is either complete
    /// or absent.
    fn put(&self, bundle: &Bundle) -> Result<StampId, Error> {
        let id = bundle.id().clone();
        let dir = self.dir(&id);
        if dir.exists() {
            return Err(Error::Exists(id.to_string()));
        }
        let tmp = self.root.join(format!(".tmp-{id}"));
        let _ = fs::remove_dir_all(&tmp);
        let written = (|| -> Result<(), Error> {
            fs::create_dir(&tmp)?;
            fs::write(tmp.join(SNAPSHOT), bundle.snapshot())?;
            fs::write(tmp.join(PROOF), bundle.proof().to_bytes())?;
            fs::write(tmp.join(RECEIPT), bundle.receipt().to_json())?;
            fs::rename(&tmp, &dir)?;
            Ok(())
        })();
        if written.is_err() {
            let _ = fs::remove_dir_all(&tmp);
        }
        written.map(|()| id)
    }

    /// Refuses a proof for a different commitment; writes a temporary file and renames it.
    fn put_proof(&self, id: &StampId, proof: &Proof) -> Result<(), Error> {
        if self.receipt(id)?.commitment().0 != proof.digest() {
            return Err(Error::Mismatch(
                "the proof is for a different commitment".into(),
            ));
        }
        let tmp = self.dir(id).join(".proof.ots.tmp");
        fs::write(&tmp, proof.to_bytes())?;
        fs::rename(&tmp, self.dir(id).join(PROOF))?;
        Ok(())
    }

    fn get(&self, id: &StampId) -> Result<Bundle, Error> {
        Ok(Bundle::from_parts(
            id.clone(),
            self.receipt(id)?,
            self.proof(id)?,
            self.read(id, SNAPSHOT)?,
        ))
    }

    fn receipt(&self, id: &StampId) -> Result<Receipt, Error> {
        let json = String::from_utf8(self.read(id, RECEIPT)?)
            .map_err(|_| Error::InvalidReceipt("not UTF-8".into()))?;
        Receipt::parse(&json)
    }

    fn proof(&self, id: &StampId) -> Result<Proof, Error> {
        Ok(Proof::from_bytes(&self.read(id, PROOF)?)?)
    }

    /// Folders named like stamp ids; leftovers from interrupted writes are ignored.
    fn list(&self) -> Result<Vec<StampId>, Error> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Some(id) = entry
                    .file_name()
                    .to_str()
                    .and_then(|n| n.parse::<StampId>().ok())
            {
                ids.push(id);
            }
        }
        ids.sort();
        Ok(ids)
    }
}
