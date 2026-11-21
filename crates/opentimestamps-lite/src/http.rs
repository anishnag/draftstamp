//! The only thing the library needs from its caller.

use crate::Error;

/// An HTTP response: status code and body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// HTTP status code.
    pub status: u16,
    /// Response body.
    pub body: Vec<u8>,
}

/// Blocking HTTPS requests, supplied by the caller: a native client in the CLI or desktop
/// app, recorded replies in tests.
///
/// `Sync` because the library sends independent requests in parallel on scoped threads.
/// Implementations should time out after about 5 seconds and report failures as
/// [`Error::Http`]. Non-2xx responses are returned as `Ok`, not errors.
pub trait Http: Sync {
    /// `GET url`.
    fn get(&self, url: &str) -> Result<Response, Error>;
    /// `POST url` with a raw binary body.
    fn post(&self, url: &str, body: &[u8]) -> Result<Response, Error>;
}
