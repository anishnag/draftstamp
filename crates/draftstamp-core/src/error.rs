use std::fmt;

/// Everything that can go wrong in draftstamp-core.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A file's path breaks the canonical path rules.
    InvalidPath {
        /// The path as found.
        path: String,
        /// Which rule it broke.
        reason: &'static str,
    },
    /// Two files have the same canonical path.
    DuplicatePath(String),
    /// Nothing is left to stamp after skipping junk files.
    EmptyFolder,
    /// A receipt is malformed or internally inconsistent.
    InvalidReceipt(String),
    /// A bundle does not verify: a file, the commitment or the proof does not match.
    Mismatch(String),
    /// No stamp with this id.
    NotFound(String),
    /// A stamp with this id already exists.
    Exists(String),
    /// The operating system's random number generator failed.
    Random,
    /// Reading or writing files failed.
    Io(std::io::Error),
    /// Timestamping or checking the proof failed.
    Ots(opentimestamps_lite::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidPath { path, reason } => write!(f, "invalid path {path:?}: {reason}"),
            Error::DuplicatePath(path) => write!(f, "two files share the path {path:?}"),
            Error::EmptyFolder => f.write_str("no files to stamp"),
            Error::InvalidReceipt(why) => write!(f, "invalid receipt: {why}"),
            Error::Mismatch(why) => write!(f, "does not verify: {why}"),
            Error::NotFound(id) => write!(f, "no stamp {id}"),
            Error::Exists(id) => write!(f, "stamp {id} already exists"),
            Error::Random => f.write_str("could not get random bytes from the operating system"),
            Error::Io(e) => write!(f, "file error: {e}"),
            Error::Ots(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<opentimestamps_lite::Error> for Error {
    fn from(e: opentimestamps_lite::Error) -> Self {
        Error::Ots(e)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Error::Mismatch(format!("snapshot is not a valid zip: {e}"))
    }
}
