use std::fmt;

/// Everything that can go wrong in opentimestamps-lite.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// A proof or calendar reply is not valid OpenTimestamps data, with the reason.
    Malformed(&'static str),
    /// Fewer calendars accepted the stamp than required.
    TooFewCalendars {
        /// How many accepted.
        accepted: usize,
    },
    /// A block header both explorers agree on does not contain the value the proof claims.
    HeaderMismatch,
    /// The operating system's random number generator failed.
    Random,
    /// The caller's HTTP implementation reported an error.
    Http(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Malformed(why) => write!(f, "malformed OpenTimestamps data: {why}"),
            Error::TooFewCalendars { accepted } => write!(
                f,
                "only {accepted} calendar(s) accepted the stamp; at least 2 are required"
            ),
            Error::HeaderMismatch => {
                f.write_str("the Bitcoin block header does not match the proof")
            }
            Error::Random => f.write_str("could not get random bytes from the operating system"),
            Error::Http(why) => write!(f, "network error: {why}"),
        }
    }
}

impl std::error::Error for Error {}
