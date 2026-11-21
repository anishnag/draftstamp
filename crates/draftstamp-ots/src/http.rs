//! The only thing draftstamp-ots needs from a host.

use crate::Error;

/// An HTTP response: status code and body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// HTTP status code.
    pub status: u16,
    /// Response body.
    pub body: Vec<u8>,
}

/// HTTPS requests, supplied by the host: the browser's `fetch` in WebAssembly, a native
/// client in the CLI, recorded replies in tests.
///
/// Implementations should time out after about 5 seconds and report failures as
/// [`Error::Http`]. Non-2xx responses are returned as `Ok`, not errors.
#[allow(async_fn_in_trait)]
pub trait Http {
    /// `GET url`.
    async fn get(&self, url: &str) -> Result<Response, Error>;
    /// `POST url` with a raw binary body.
    async fn post(&self, url: &str, body: &[u8]) -> Result<Response, Error>;
}
