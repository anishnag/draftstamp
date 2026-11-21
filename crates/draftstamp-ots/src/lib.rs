//! OpenTimestamps proofs for DraftStamp (see `spec/0000_ots.md`).
//!
//! The crate does no I/O. [`stamp`], [`upgrade`] and [`link::status`] reach the network
//! through an [`Http`] implementation supplied by the host: the browser's `fetch` in the
//! WebAssembly bindings, a native client in the CLI, recorded replies in tests.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bitcoin;
mod error;
mod flows;
mod format;
mod http;
pub mod link;
mod proof;
mod servers;
mod util;

pub use error::Error;
pub use flows::{stamp, upgrade};
pub use http::{Http, Response};
pub use proof::{CalendarEntry, Proof, Status};
