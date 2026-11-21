//! The DraftStamp engine (see `spec/0001_core.md`).
//!
//! A [`Bundle`] is one stamp: the exact files, a receipt that turns them into a commitment,
//! and the OpenTimestamps proof of that commitment. A [`Vault`] stores bundles ([`LocalVault`]
//! is a folder on disk), and [`history`] and [`find`] answer questions over any vault.
//!
//! Calls are blocking. Network access goes through the caller's [`Http`].

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bundle;
pub mod commitment;
mod error;
mod id;
mod query;
mod vault;

pub use bundle::{Bundle, Shared, StampOptions, Verified};
pub use error::Error;
pub use id::StampId;
pub use opentimestamps_lite::{Http, Proof, Response, Status, Step};
pub use query::{Found, StampInfo, find, history};
pub use vault::{LocalVault, Vault};
