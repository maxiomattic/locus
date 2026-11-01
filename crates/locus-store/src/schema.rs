//! The database. One file per machine. The server and the client use the same
//! schema; a client simply has rows in the replica tables.
//!
//! `log` and `audit` cannot be changed or shortened: triggers refuse, unless a
//! purge has raised `purge_guard` inside its own transaction.

pub const SCHEMA_VERSION: i64 = 1;

pub const SCHEMA: &str = r#"
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- The operation log, the source of truth.
CREATE TABLE log (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    op_id TEXT NOT NULL UNIQUE,
    object TEXT NOT NULL,
    home TEXT NOT NULL,
    home_seq INTEGER NOT NULL,   -- order at the home server
    rev INTEGER NOT NULL,
    batch TEXT,
    op TEXT NOT NULL,
    effects TEXT NOT NULL
);
CREATE INDEX log_object ON log(object, seq);
CREATE INDEX log_home ON log(home, home_seq);
CREATE INDEX log_batch ON log(batch) WHERE batch IS NOT NULL;

-- Every attempt, including refusals. It holds no field values, so a purge
-- only has to clear intents.
CREATE TABLE audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at INTEGER NOT NULL,
    actor TEXT NOT NULL,
    invoker TEXT,
    action TEXT NOT NULL,
    object TEXT,
    op_id TEXT,
    decision TEXT NOT NULL,      -- applied, proposed, refused
    detail TEXT NOT NULL,
    intent TEXT NOT NULL
);
CREATE INDEX audit_object ON audit(object);

CREATE TABLE purge_guard (active INTEGER NOT NULL);
INSERT INTO purge_guard VALUES (0);

CREATE TRIGGER log_no_update BEFORE UPDATE ON log
    WHEN (SELECT active FROM purge_guard) = 0
    BEGIN SELECT RAISE(ABORT, 'the log is append-only'); END;
CREATE TRIGGER log_no_delete BEFORE DELETE ON log
    BEGIN SELECT RAISE(ABORT, 'the log is append-only'); END;
CREATE TRIGGER audit_no_update BEFORE UPDATE ON audit
    WHEN (SELECT active FROM purge_guard) = 0
    BEGIN SELECT RAISE(ABORT, 'the audit log is append-only'); END;
CREATE TRIGGER audit_no_delete BEFORE DELETE ON audit
    BEGIN SELECT RAISE(ABORT, 'the audit log is append-only'); END;

-- Current state, derived from the log.
CREATE TABLE objects (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    home TEXT NOT NULL,
    rev INTEGER NOT NULL,
    deleted INTEGER NOT NULL,
    doc TEXT NOT NULL
) WITHOUT ROWID;
CREATE INDEX objects_type ON objects(type);

-- Indexes built from objects.
CREATE TABLE refs (src TEXT NOT NULL, dst TEXT NOT NULL, pin INTEGER);
CREATE INDEX refs_dst ON refs(dst);
CREATE INDEX refs_src ON refs(src);
CREATE TABLE blob_refs (hash TEXT NOT NULL, object TEXT NOT NULL);
CREATE INDEX blob_refs_hash ON blob_refs(hash);
CREATE INDEX blob_refs_object ON blob_refs(object);
CREATE TABLE reaction_state (reaction TEXT PRIMARY KEY, ids TEXT NOT NULL);

CREATE TABLE blobs (hash TEXT PRIMARY KEY, size INTEGER NOT NULL, at INTEGER NOT NULL) WITHOUT ROWID;
CREATE TABLE purged_blobs (hash TEXT PRIMARY KEY, at INTEGER NOT NULL) WITHOUT ROWID;

-- Server side: what each replica holds.
CREATE TABLE replica_view (client TEXT NOT NULL, object TEXT NOT NULL, PRIMARY KEY (client, object)) WITHOUT ROWID;
CREATE TABLE replica_epoch (client TEXT PRIMARY KEY, epoch INTEGER NOT NULL) WITHOUT ROWID;

-- Client side.
CREATE TABLE pending (
    n INTEGER PRIMARY KEY AUTOINCREMENT,
    home TEXT NOT NULL,
    op_id TEXT NOT NULL UNIQUE,
    op TEXT NOT NULL
);
CREATE TABLE overlay (id TEXT PRIMARY KEY, doc TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE refused (op_id TEXT PRIMARY KEY, op TEXT NOT NULL, reason TEXT NOT NULL, at INTEGER NOT NULL) WITHOUT ROWID;
CREATE TABLE cursors (home TEXT PRIMARY KEY, seq INTEGER NOT NULL, synced_at INTEGER NOT NULL) WITHOUT ROWID;
CREATE TABLE pending_blobs (hash TEXT PRIMARY KEY, home TEXT NOT NULL) WITHOUT ROWID;
"#;
