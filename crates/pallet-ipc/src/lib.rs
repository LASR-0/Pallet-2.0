//! The protocol between Pallet's processes.
//!
//! The picker is a resident helper holding a warm GPU context, because
//! initialising `wgpu` costs about 220 ms — measured, and the dominant part of
//! a cold pick. The CLI and the app reach it over a local socket rather than
//! spawning a fresh process per pick.
//!
//! Messages are length-prefixed JSON. JSON because the volume is a handful of
//! small messages per pick, where being able to read the wire in a debugger is
//! worth more than compactness; length-prefixed because a stream socket has no
//! message boundaries of its own.

#![warn(missing_docs)]

pub mod codec;
pub mod protocol;

#[cfg(unix)]
pub mod transport;

pub use codec::{Error, Result, read_message, write_message};
pub use protocol::{PickOptions, Request, Response};
