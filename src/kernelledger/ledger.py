"""Single-writer SQLite execution history; readers never create a database."""

import datetime
import json
import os
import sqlite3
import uuid
from contextlib import nullcontext
from pathlib import Path
from typing import Any

TABLES = ("sessions", "executions", "events", "checkpoints", "blobs", "pins", "session_events")
DDL2 = """
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
"""
DDL = """
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
"""


def now() -> Any:
    return datetime.datetime.now(datetime.UTC).isoformat().replace("+00:00", "Z")


def uid() -> Any:
    return str(uuid.uuid4())


class Ledger:
    def __init__(self, path: Any, readonly: Any = False, immutable: bool = False) -> None:
        self.readonly = readonly
        if readonly:
            absolute = str(Path(path).resolve())
            if os.name == "nt" and absolute.startswith("\\\\?\\"):
                absolute = (
                    "\\\\" + absolute[8:] if absolute.startswith("\\\\?\\UNC\\") else absolute[4:]
                )
            self.db = sqlite3.connect(
                Path(absolute).as_uri() + ("?immutable=1" if immutable else "?mode=ro"),
                uri=True,
            )
        else:
            self.db = sqlite3.connect(path)
        self.db.row_factory = sqlite3.Row
        version = self.db.execute("PRAGMA user_version").fetchone()[0]
        if version not in ((1, 2) if readonly else (0, 1, 2)):
            raise ValueError("unsupported ledger schema")
        if not readonly:
            self.db.execute("PRAGMA journal_mode=WAL")
            self.db.execute("PRAGMA synchronous=FULL")
            self.db.execute("PRAGMA foreign_keys=ON")
            self.db.executescript(DDL)
            if version == 0:
                self.db.executescript(DDL2)
                self.db.execute("PRAGMA user_version=2")
            self.db.commit()

    @property
    def schema(self) -> int:
        return self.db.execute("PRAGMA user_version").fetchone()[0]

    def migrate(self, backup: Any) -> Any:
        if self.readonly:
            raise ValueError("read-only ledger")
        if self.schema == 2:
            return {"schema": 2, "already_migrated": True}
        self.db.commit()
        destination = sqlite3.connect(backup)
        try:
            self.db.backup(destination)
            if destination.execute("PRAGMA quick_check").fetchone()[0] != "ok":
                raise ValueError("migration backup failed integrity check")
        finally:
            destination.close()
        try:
            self.db.execute("BEGIN IMMEDIATE")
            statement = ""
            for line in DDL2.splitlines(keepends=True):
                statement += line
                if sqlite3.complete_statement(statement):
                    self.db.execute(statement)
                    statement = ""
            self.db.execute("PRAGMA user_version=2")
            self.db.commit()
        except BaseException:
            self.db.rollback()
            raise
        return {"schema": 2, "backup": str(backup)}

    def change(self, kind: str, ref: Any) -> Any:
        if self.schema == 2:
            return self.db.execute(
                "INSERT INTO changes(kind,ref_json,timestamp) VALUES (?,?,?)",
                (kind, json.dumps(ref), now()),
            ).lastrowid

    def accept_operation(
        self, client: str, key: str, kind: str, request: Any, commit: bool = True
    ) -> Any:
        import hashlib

        from .source import SourceError

        uuid.UUID(key)
        digest = hashlib.sha256(
            json.dumps(request, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        rows = self.rows(
            "SELECT * FROM operations WHERE client_id=? AND client_request_id=?", (client, key)
        )
        if rows:
            if rows[0]["request_sha256"] != digest:
                raise SourceError("key_reused", "request key already used for different content")
            return rows[0], False
        oid = uid()
        if commit:
            self.db.commit()
            self.db.execute("PRAGMA synchronous=FULL")
        with self.db if commit else nullcontext():
            self.db.execute(
                "INSERT INTO operations VALUES (?,?,?,?,?,?,?,?,?,?)",
                (
                    oid,
                    client,
                    key,
                    kind,
                    digest,
                    "accepted",
                    None,
                    now(),
                    None,
                    None,
                ),
            )
            self.change("operation_state", {"operation_id": oid, "state": "accepted"})
        return self.one("SELECT * FROM operations WHERE operation_id=?", (oid,)), True

    def operation_state(self, oid: str, state: str, result: Any = None) -> None:
        with self.db:
            self.db.execute(
                "UPDATE operations SET state=?,result_json=COALESCE(?,result_json),started=CASE WHEN ?='running' THEN ? ELSE started END,"
                "finished=CASE WHEN ? IN ('completed','canceled','failed','unknown') THEN ? ELSE finished END WHERE operation_id=?",
                (
                    state,
                    json.dumps(result) if result is not None else None,
                    state,
                    now(),
                    state,
                    now(),
                    oid,
                ),
            )
            self.change("operation_state", {"operation_id": oid, "state": state})

    def close(self) -> Any:
        self.db.close()

    def rows(self, sql: Any, args: Any = ()) -> Any:
        return [dict(row) for row in self.db.execute(sql, args)]

    def one(self, sql: Any, args: Any = ()) -> Any:
        rows = self.rows(sql, args)
        if not rows:
            raise ValueError("record not found")
        return rows[0]

    def write(self, sql: Any, args: Any = ()) -> Any:
        with self.db:
            self.db.execute(sql, args)

    def takeover(self) -> Any:
        with self.db:
            if self.schema == 2:
                self.db.execute(
                    "UPDATE executions SET status='canceled',end=? WHERE status='queued' AND exec_id IN "
                    "(SELECT value FROM batches,json_each(batches.exec_ids_json))",
                    (now(),),
                )
                for row in self.rows(
                    "SELECT * FROM operations WHERE state IN ('accepted','running')"
                ):
                    state = "canceled" if row["state"] == "accepted" else "unknown"
                    known: Any = json.loads(row["result_json"]) if row["result_json"] else {}
                    known["reason"] = "supervisor takeover"
                    known["executions"] = []
                    for eid in known.get("exec_ids", []):
                        if state == "canceled":
                            self.db.execute(
                                "UPDATE executions SET status='canceled',end=? WHERE exec_id=? AND status='queued'",
                                (now(), eid),
                            )
                        execution = self.rows(
                            "SELECT exec_id,status FROM executions WHERE exec_id=?", (eid,)
                        )
                        known["executions"].extend(execution)
                    self.db.execute(
                        "UPDATE operations SET state=?,finished=?,result_json=? WHERE operation_id=?",
                        (
                            state,
                            now(),
                            json.dumps(known),
                            row["operation_id"],
                        ),
                    )
                    self.change(
                        "operation_state", {"operation_id": row["operation_id"], "state": state}
                    )
                self.db.execute(
                    "UPDATE input_requests SET state='canceled',client_name=NULL,updated=? WHERE state='pending'",
                    (now(),),
                )
            self.db.execute(
                "UPDATE executions SET status='incomplete', end=? WHERE status IN ('queued','running')",
                (now(),),
            )
            self.db.execute(
                "UPDATE sessions SET state='crashed', end=? WHERE state='running'", (now(),)
            )

    def session(
        self,
        notebook_id: Any,
        environment: Any,
        parent: Any = None,
        checkpoint: Any = None,
        origin: Any = "local",
        state: Any = "running",
    ) -> Any:
        import platform

        from . import __version__

        sid = uid()
        self.write(
            "INSERT INTO sessions VALUES (?,?,?,?,?,?,?,?,?,?)",
            (
                sid,
                notebook_id,
                parent,
                checkpoint,
                state,
                now(),
                now() if state != "running" else None,
                json.dumps(environment),
                json.dumps({"kernelledger": __version__, "python": platform.python_version()}),
                origin,
            ),
        )
        return sid

    def execution(self, sid: Any, cid: Any, source: Any) -> Any:
        eid = uid()
        ordinal = self.db.execute(
            "SELECT COALESCE(MAX(ordinal),0)+1 FROM executions WHERE session_id=?", (sid,)
        ).fetchone()[0]
        self.write(
            "INSERT INTO executions VALUES (?,?,?,?,?,?,?,?)",
            (eid, sid, ordinal, cid, source, "queued", now(), None),
        )
        return eid

    def event(self, eid: Any, kind: Any, payload: Any, blob: Any = None, commit: Any = True) -> Any:
        seq = self.db.execute(
            "SELECT COALESCE(MAX(seq),0)+1 FROM events WHERE exec_id=?", (eid,)
        ).fetchone()[0]
        self.db.execute(
            "INSERT INTO events VALUES (?,?,?,?,?,?)",
            (eid, seq, kind, now(), json.dumps(payload), blob),
        )
        if commit:
            self.db.commit()

    def session_event(self, sid: Any, kind: Any, payload: Any) -> Any:
        seq = self.db.execute(
            "SELECT COALESCE(MAX(seq),0)+1 FROM session_events WHERE session_id=?", (sid,)
        ).fetchone()[0]
        self.write(
            "INSERT INTO session_events VALUES (?,?,?,?,?)",
            (sid, seq, kind, now(), json.dumps(payload)),
        )

    def events(self, eid: Any) -> Any:
        return [
            {
                "kind": row["kind"],
                **json.loads(row["payload_json"]),
                "seq": row["seq"],
                "timestamp": row["timestamp"],
            }
            for row in self.rows("SELECT * FROM events WHERE exec_id=? ORDER BY seq", (eid,))
        ]

    def blob(self, digest: Any, size: Any, media_type: Any = None) -> Any:
        self.db.execute("INSERT OR IGNORE INTO blobs VALUES (?,?,?)", (digest, size, media_type))


def fold(events: Any) -> Any:
    visible: list[Any]
    visible, pending_clear = [], False
    for event in events:
        kind = event["kind"]
        if kind == "clear":
            if event.get("wait"):
                pending_clear = True
            else:
                visible: list[Any]
                visible, pending_clear = [], False
        elif kind in (
            "stdout",
            "stderr",
            "display",
            "result",
            "error",
            "truncated",
            "display_update",
        ):
            if pending_clear:
                visible: list[Any]
                visible, pending_clear = [], False
            if kind == "display_update":
                for i in range(len(visible) - 1, -1, -1):
                    if (
                        event.get("display_id") is not None
                        and visible[i].get("display_id") == event["display_id"]
                    ):
                        visible[i] = {**event, "kind": "display"}
                        break
            else:
                visible.append(event)
    return visible
