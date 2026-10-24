"""Removable assistant records, separate from the runtime ledger."""

import json
import sqlite3
import time
import uuid
from pathlib import Path
from typing import Any

from ..source import SourceError
from ..supervisor.platform import DirectoryLock


class Store:
    def __init__(self, root: Path, retention_days: int = 90):
        directory = root / ".kl/assistant"
        directory.mkdir(parents=True, exist_ok=True)
        self.lock = DirectoryLock(directory)
        try:
            self.lock.acquire()
        except ValueError:
            raise SourceError(
                "busy",
                "another assistant process owns this notebook; close it before using the CLI",
            ) from None
        self.path = directory / "assistant.sqlite"
        self.db = sqlite3.connect(self.path)
        self.db.row_factory = sqlite3.Row
        self.db.execute("PRAGMA foreign_keys=ON")
        self.db.executescript("""
            CREATE TABLE IF NOT EXISTS conversations(id TEXT PRIMARY KEY, title TEXT, created REAL, notebook_id TEXT, pinned INTEGER DEFAULT 0);
            CREATE TABLE IF NOT EXISTS requests(id TEXT PRIMARY KEY, conversation TEXT REFERENCES conversations(id) ON DELETE CASCADE, task TEXT, prompt_version TEXT, provider TEXT, model TEXT, destination TEXT, receipt_hash TEXT, receipt TEXT, usage TEXT, status TEXT, error TEXT, created REAL, finished REAL, reserved INTEGER DEFAULT 0);
            CREATE TABLE IF NOT EXISTS messages(id TEXT PRIMARY KEY, request_id TEXT REFERENCES requests(id) ON DELETE CASCADE, role TEXT, text TEXT, citations TEXT);
            CREATE TABLE IF NOT EXISTS proposals(id TEXT PRIMARY KEY, request_id TEXT REFERENCES requests(id) ON DELETE CASCADE, payload TEXT, state TEXT, manifest_revision TEXT);
            CREATE TABLE IF NOT EXISTS applications(id TEXT PRIMARY KEY, proposal_id TEXT REFERENCES proposals(id) ON DELETE CASCADE, result TEXT, created REAL);
            CREATE TABLE IF NOT EXISTS capabilities(key TEXT PRIMARY KEY, result TEXT, created REAL);
            CREATE VIRTUAL TABLE IF NOT EXISTS history_index USING fts5(kind UNINDEXED, identity UNINDEXED, text);
        """)
        self.db.execute(
            "UPDATE requests SET status='interrupted', error='assistant process ended' WHERE status='running'"
        )
        self.db.execute(
            "DELETE FROM conversations WHERE pinned=0 AND created<?",
            (time.time() - retention_days * 86400,),
        )
        self.db.commit()

    def close(self) -> None:
        self.db.close()
        self.lock.close()

    def rows(self, sql: str, args: Any = ()) -> list[dict[str, Any]]:
        return [dict(row) for row in self.db.execute(sql, args)]

    def used(self, provider: str) -> int:
        # Reservations remain charged on errors/cancellation: partial remote usage is unknown.
        row = self.db.execute(
            "SELECT COALESCE(SUM(reserved),0) FROM requests WHERE provider=? AND created>=?",
            (provider, int(time.time() // 86400) * 86400),
        ).fetchone()
        return int(row[0])

    def begin(self, request: Any, receipt: Any, notebook_id: str, budget: int) -> str:
        rid = uuid.uuid4().hex
        cid = request.get("conversation_id") or uuid.uuid4().hex
        reserved = receipt["token_estimate"] + budget
        with self.db:
            self.db.execute(
                "INSERT OR IGNORE INTO conversations(id,title,created,notebook_id) VALUES(?,?,?,?)",
                (cid, receipt["question"][:100], time.time(), notebook_id),
            )
            self.db.execute(
                "INSERT INTO requests(id,conversation,task,prompt_version,provider,model,destination,receipt_hash,receipt,usage,status,created,reserved) VALUES(?,?,?,?,?,?,?,?,?,'{}','running',?,?)",
                (
                    rid,
                    cid,
                    request.get("task", "ask"),
                    receipt["prompt_version"],
                    receipt["provider"],
                    receipt["model"],
                    receipt["destination"]["host"],
                    receipt["hash"],
                    json.dumps(receipt),
                    time.time(),
                    reserved,
                ),
            )
            self.db.execute(
                "INSERT INTO messages VALUES(?,?,?,?,?)",
                (uuid.uuid4().hex, rid, "user", receipt["question"], "[]"),
            )
        return rid

    def finish(self, rid: str, response: Any, usage: Any) -> list[Any]:
        proposals: list[Any] = []
        manifest = self.db.execute("SELECT receipt FROM requests WHERE id=?", (rid,)).fetchone()
        with self.db:
            self.db.execute(
                "UPDATE requests SET status='done',usage=?,finished=? WHERE id=?",
                (json.dumps(usage), time.time(), rid),
            )
            self.db.execute(
                "INSERT INTO messages VALUES(?,?,?,?,?)",
                (
                    uuid.uuid4().hex,
                    rid,
                    "assistant",
                    response["answer"],
                    json.dumps(response["citations"]),
                ),
            )
            for proposal in response["proposals"]:
                pid = uuid.uuid4().hex
                self.db.execute(
                    "INSERT INTO proposals VALUES(?,?,?,?,?)",
                    (
                        pid,
                        rid,
                        json.dumps(proposal),
                        "proposed",
                        json.loads(manifest[0])["manifest_revision"],
                    ),
                )
                proposals.append({"id": pid, "payload": proposal, "state": "proposed"})
        return proposals

    def fail(self, rid: str, code: str) -> None:
        with self.db:
            self.db.execute(
                "UPDATE requests SET status=?,error=?,finished=? WHERE id=?",
                ("cancelled" if code == "cancelled" else "error", code, time.time(), rid),
            )

    def conversation(self, cid: str) -> Any:
        requests = self.rows(
            "SELECT * FROM requests WHERE conversation=? ORDER BY created LIMIT 200", (cid,)
        )
        return {
            "requests": requests,
            "messages": self.rows(
                "SELECT m.* FROM messages m JOIN requests r ON m.request_id=r.id WHERE r.conversation=? ORDER BY r.created,m.rowid LIMIT 400",
                (cid,),
            ),
        }

    def search(self, query: str) -> Any:
        # Quote user input as FTS text rather than accepting arbitrary MATCH syntax.
        phrase = '"' + query.replace('"', '""')[:1000] + '"'
        return {
            "items": self.rows(
                "SELECT kind,identity,snippet(history_index,2,'[',']','…',24) AS excerpt FROM history_index WHERE history_index MATCH ? ORDER BY rank LIMIT 50",
                (phrase,),
            )
        }
