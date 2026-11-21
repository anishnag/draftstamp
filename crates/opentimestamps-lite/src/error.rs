use std::fmt;

/// Everything that can go wrong in opentimestamps-lite.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Input is larger than the limit for its kind.
    TooLarge,
    /// The file does not start with the OpenTimestamps magic bytes.
    BadMagic,
    /// The file's major version is not 1.
    UnsupportedVersion(u8),
    /// The file's digest is not SHA-256.
    UnsupportedHashOp(u8),
    /// Input ended early.
    Truncated,
    /// Bytes remain after a complete structure.
    TrailingBytes,
    /// A varuint is longer than 9 bytes.
    VaruintTooLong,
    /// A length-prefixed value is outside its allowed range.
    LengthOutOfRange,
    /// An unknown operation tag.
    BadOpTag(u8),
    /// The same operation appears twice under one node.
    DuplicateOp,
    /// The timestamp tree is deeper than 256 levels.
    DepthExceeded,
    /// A message would exceed 4096 bytes.
    MessageTooLong,
    /// A pending attestation's calendar URI is invalid.
    BadUri,
    /// An attestation payload is malformed.
    BadPayload,
    /// Fewer calendars accepted the stamp than required.
    TooFewCalendars {
        /// How many accepted.
        accepted: usize,
    },
    /// A block header does not contain the value the proof claims.
    HeaderMismatch,
    /// The operating system's random number generator failed.
    Random,
    /// The caller's HTTP implementation reported an error.
    Http(String),
}

impl Error {
    /// A stable code for this kind of error.
    pub fn code(&self) -> &'static str {
        match self {
            Error::TooLarge => "too_large",
            Error::BadMagic => "bad_magic",
            Error::UnsupportedVersion(_) => "unsupported_version",
            Error::UnsupportedHashOp(_) => "unsupported_hash_op",
            Error::Truncated => "truncated",
            Error::TrailingBytes => "trailing_bytes",
            Error::VaruintTooLong => "varuint_too_long",
            Error::LengthOutOfRange => "length_out_of_range",
            Error::BadOpTag(_) => "bad_op_tag",
            Error::DuplicateOp => "duplicate_op",
            Error::DepthExceeded => "depth_exceeded",
            Error::MessageTooLong => "message_too_long",
            Error::BadUri => "bad_uri",
            Error::BadPayload => "bad_payload",
            Error::TooFewCalendars { .. } => "too_few_calendars",
            Error::HeaderMismatch => "header_mismatch",
            Error::Random => "random",
            Error::Http(_) => "http",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnsupportedVersion(v) => write!(f, "unsupported OpenTimestamps version {v}"),
            Error::UnsupportedHashOp(t) => write!(f, "unsupported digest type 0x{t:02x}"),
            Error::BadOpTag(t) => write!(f, "unknown operation tag 0x{t:02x}"),
            Error::TooFewCalendars { accepted } => {
                write!(
                    f,
                    "only {accepted} calendar(s) accepted the stamp; at least 2 are required"
                )
            }
            Error::Http(why) => write!(f, "network error: {why}"),
            other => f.write_str(other.code()),
        }
    }
}

impl std::error::Error for Error {}
