"""Bounded, keyset-paged execution search. Never loads the whole ledger."""

import base64
import hashlib
from typing import Any

from ..source import SourceError
from .protocol import page_window

SOURCE_LIMIT = 1024 * 1024
PREVIEW_LIMIT = 1024


def execution_search(ledger: Any, blobs: Any, request: Any) -> Any:
    query = request.get("query", "")
    if not isinstance(query, str) or len(query) > 512:
        raise SourceError("invalid_request", "History query must be at most 512 characters")
    size, after = page_window(request)
    if after > 2**63 - 1:
        raise SourceError("invalid_request", "Invalid History cursor")
    size = min(size, 100)
    where = ["rowid > ?"]
    args: list[Any] = [after]
    words: list[str] = []
    if request.get("session_id"):
        where.append("session_id=?")
        args.append(request["session_id"])
    for part in query.split():
        field, _, value = part.partition(":")
        field = field.lower()
        column = {
            "cell": "cell_id",
            "session": "session_id",
            "outcome": "status",
            "from": "start",
            "after": "start",
            "to": "start",
            "before": "start",
        }.get(field)
        if column and value:
            if field in ("from", "after"):
                where.append("start >= ?")
            elif field in ("to", "before"):
                where.append("start < ?")
                value += "T99"
            else:
                where.append(column + " = ?")
            args.append(value.lower() if field == "outcome" else value)
        else:
            words.append(part.lower())
    rows = ledger.rows(
        "SELECT rowid AS search_rowid,* FROM executions WHERE "
        + " AND ".join(where)
        + " ORDER BY rowid LIMIT ?",
        (*args, size + 1),
    )
    more = len(rows) > size
    rows = rows[:size]
    items: list[Any] = []
    limited = 0
    for row in rows:
        source = ""
        digest = row["source_sha256"]
        try:
            if (
                not isinstance(digest, str)
                or len(digest) != 64
                or any(c not in "0123456789abcdef" for c in digest)
            ):
                raise ValueError("Invalid source hash")
            with (blobs.root / digest).open("rb") as stream:
                raw = stream.read(SOURCE_LIMIT + 1)
            if len(raw) > SOURCE_LIMIT:
                limited += 1
                row["source_search_limited"] = True
            elif hashlib.sha256(raw).hexdigest() == digest:
                source = raw.decode()
            else:
                limited += 1
                row["source_search_limited"] = True
        except (OSError, ValueError, UnicodeError):
            limited += 1
            row["source_search_limited"] = True
        searchable = (source + " " + str(row.get("cell_id", "")) + " " + row["status"]).lower()
        if all(word in searchable for word in words):
            row["source"] = source.encode()[:PREVIEW_LIMIT].decode(errors="ignore")
            row["source_preview_truncated"] = len(source.encode()) > PREVIEW_LIMIT
            items.append(row)
    return {
        "items": items,
        "cursor": base64.b64encode(str(rows[-1]["search_rowid"]).encode()).decode()
        if more and rows
        else None,
        "search_page": True,
        "scanned": len(rows),
        "source_search_limited": limited,
        "query": query,
        "source_limit_bytes": SOURCE_LIMIT,
    }
