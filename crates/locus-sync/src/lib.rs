//! Locus sync. Every object has one home server, which orders its operations
//! and checks their permissions. Clients keep a partial copy, apply operations
//! at once, and replay them on top of the server's order. Devices do not sync
//! with each other directly.
//!
//! The transport is a trait: in-process here, and over iroh in `locus-net`.

pub mod local;
pub mod protocol;
pub mod replica;
pub mod server;

pub use local::{LocalTransport, Network};
pub use protocol::{Request, Response, SubmitResult, Subscription, Transport, Unreachable};
pub use replica::{Refusal, Replica, SyncError, SyncReport, Visible};
pub use server::HomeServer;
