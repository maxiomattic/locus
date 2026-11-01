//! What clients and home servers say to each other. Everything here is plain
//! data, so any transport that carries requests and responses will do.

use locus_model::{BlobHash, ObjectId, Op};
use locus_store::LogEntry;
use serde::{Deserialize, Serialize};

/// What part of a home a client wants a copy of.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Subscription {
    /// Everything the client may read.
    pub all: bool,
    pub types: Vec<String>,
    pub objects: Vec<ObjectId>,
}

impl Subscription {
    pub fn everything() -> Self {
        Subscription {
            all: true,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// Entries after `since`, plus whatever the client should now start or
    /// stop holding. `wait_ms` lets the server hold the reply until there is
    /// something to say, which is how live updates work.
    Pull {
        client: String,
        since: i64,
        sub: Subscription,
        limit: u32,
        wait_ms: u32,
    },
    Submit {
        ops: Vec<Op>,
    },
    GetBlob {
        hash: BlobHash,
        offset: u64,
        len: u32,
    },
    PutBlob {
        hash: BlobHash,
        offset: u64,
        bytes: Vec<u8>,
        last: bool,
    },
    History {
        object: ObjectId,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SubmitResult {
    Accepted {
        seq: i64,
    },
    /// The actor may only propose this, so the server branched it.
    Proposed {
        proposal: ObjectId,
    },
    Refused {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Pulled {
        entries: Vec<LogEntry>,
        evict: Vec<ObjectId>,
        /// Continue from here next time.
        head: i64,
        more: bool,
    },
    Submitted {
        results: Vec<SubmitResult>,
    },
    Blob {
        bytes: Vec<u8>,
        total: u64,
    },
    BlobStored,
    History {
        entries: Vec<LogEntry>,
    },
    Error(String),
}

/// A transport could not deliver: the server is unreachable. Nothing about the
/// request is known to have happened, and sync is safe to retry.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cannot reach the home server: {0}")]
pub struct Unreachable(pub String);

/// A connection to one home server, already authenticated as someone.
pub trait Transport: Send {
    fn call(&self, req: Request) -> Result<Response, Unreachable>;
}
