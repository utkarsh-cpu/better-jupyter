-- Generated from kernelledger.ledger DDL/DDL2 by scripts/runtime_schema.py.

CREATE TABLE IF NOT EXISTS sessions (
 session_id TEXT PRIMARY KEY, notebook_id TEXT NOT NULL, parent_session_id TEXT,
 restored_checkpoint_id TEXT, state TEXT NOT NULL, start TEXT NOT NULL, end TEXT,
 environment_json TEXT NOT NULL, supervisor_json TEXT NOT NULL, origin TEXT NOT NULL DEFAULT 'local');
CREATE TABLE IF NOT EXISTS executions (
 exec_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id),
 ordinal INTEGER NOT NULL, cell_id TEXT, source_sha256 TEXT NOT NULL, status TEXT NOT NULL,
 start TEXT NOT NULL, end TEXT, UNIQUE(session_id,ordinal));
CREATE TABLE IF NOT EXISTS events (
 exec_id TEXT NOT NULL REFERENCES executions(exec_id), seq INTEGER NOT NULL,
 kind TEXT NOT NULL, timestamp TEXT NOT NULL, payload_json TEXT NOT NULL, blob_sha256 TEXT,
 PRIMARY KEY(exec_id,seq));
CREATE TABLE IF NOT EXISTS checkpoints (
 checkpoint_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id),
 boundary_exec_id TEXT, status TEXT NOT NULL, manifest_sha256 TEXT NOT NULL,
 timestamp TEXT NOT NULL, origin TEXT NOT NULL DEFAULT 'local');
CREATE TABLE IF NOT EXISTS blobs (sha256 TEXT PRIMARY KEY, byte_size INTEGER NOT NULL, media_type TEXT);
CREATE TABLE IF NOT EXISTS pins (name TEXT PRIMARY KEY, checkpoint_id TEXT NOT NULL REFERENCES checkpoints(checkpoint_id));
CREATE TABLE IF NOT EXISTS session_events (
 session_id TEXT NOT NULL REFERENCES sessions(session_id), seq INTEGER NOT NULL,
 kind TEXT NOT NULL, timestamp TEXT NOT NULL, payload_json TEXT NOT NULL, PRIMARY KEY(session_id,seq));

CREATE TABLE operations (
 operation_id TEXT PRIMARY KEY, client_id TEXT NOT NULL, client_request_id TEXT NOT NULL,
 kind TEXT NOT NULL, request_sha256 TEXT NOT NULL, state TEXT NOT NULL,
 result_json TEXT, accepted TEXT NOT NULL, started TEXT, finished TEXT,
 UNIQUE(client_id,client_request_id));
CREATE TABLE batches (
 batch_id TEXT PRIMARY KEY, operation_id TEXT NOT NULL REFERENCES operations(operation_id),
 session_id TEXT NOT NULL, on_error TEXT NOT NULL, exec_ids_json TEXT NOT NULL);
CREATE TABLE changes (
 change_seq INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, ref_json TEXT NOT NULL,
 timestamp TEXT NOT NULL);
CREATE TABLE input_requests (
 input_id TEXT PRIMARY KEY, exec_id TEXT NOT NULL REFERENCES executions(exec_id), seq INTEGER NOT NULL,
 password INTEGER NOT NULL, state TEXT NOT NULL, client_name TEXT,
 created TEXT NOT NULL, updated TEXT NOT NULL);
CREATE TRIGGER sessions_insert AFTER INSERT ON sessions BEGIN
 INSERT INTO changes(kind,ref_json,timestamp) VALUES ('session_state',json_object('session_id',NEW.session_id,'state',NEW.state),NEW.start); END;
CREATE TRIGGER sessions_update AFTER UPDATE OF state ON sessions BEGIN
 INSERT INTO changes(kind,ref_json,timestamp) VALUES ('session_state',json_object('session_id',NEW.session_id,'state',NEW.state),COALESCE(NEW.end,NEW.start)); END;
CREATE TRIGGER executions_insert AFTER INSERT ON executions BEGIN
 INSERT INTO changes(kind,ref_json,timestamp) VALUES ('execution_status',json_object('exec_id',NEW.exec_id,'status',NEW.status),NEW.start); END;
CREATE TRIGGER executions_update AFTER UPDATE OF status ON executions BEGIN
 INSERT INTO changes(kind,ref_json,timestamp) VALUES ('execution_status',json_object('exec_id',NEW.exec_id,'status',NEW.status),COALESCE(NEW.end,NEW.start)); END;
CREATE TRIGGER checkpoints_insert AFTER INSERT ON checkpoints BEGIN
 INSERT INTO changes(kind,ref_json,timestamp) VALUES ('checkpoint_published',json_object('checkpoint_id',NEW.checkpoint_id,'status',NEW.status),NEW.timestamp); END;
CREATE TRIGGER pins_insert AFTER INSERT ON pins BEGIN
 INSERT INTO changes(kind,ref_json,timestamp) VALUES ('pin_changed',json_object('name',NEW.name,'checkpoint_id',NEW.checkpoint_id),strftime('%Y-%m-%dT%H:%M:%fZ','now')); END;
CREATE TRIGGER pins_delete AFTER DELETE ON pins BEGIN
 INSERT INTO changes(kind,ref_json,timestamp) VALUES ('pin_changed',json_object('name',OLD.name),strftime('%Y-%m-%dT%H:%M:%fZ','now')); END;
