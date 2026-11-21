//! A small OpenTimestamps library (see `spec/0000_ots.md`).
//!
//! Timestamps a 32-byte digest in Bitcoin through the public OpenTimestamps calendars and
//! checks the resulting `.ots` proof against Bitcoin. Proofs are standard: the official
//! OpenTimestamps tools read and verify them.
//!
//! The library does no I/O. [`stamp`] and [`upgrade`] reach the network through an [`Http`]
//! implementation supplied by the caller. Calls are blocking; requests run in parallel on
//! scoped threads.
//!
//! This is a local crate, not the upstream `opentimestamps` crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bitcoin;
mod error;
mod flows;
mod format;
mod http;
mod proof;
mod servers;
mod util;

pub use error::Error;
pub use flows::{stamp, upgrade};
pub use http::{Http, Response};
pub use proof::{Proof, Status};
