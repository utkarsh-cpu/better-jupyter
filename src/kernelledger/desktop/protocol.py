"""Bounded JSON framing and shared validation for socket and stdio clients."""

import json
import struct
from typing import Any, cast

from ..source import SourceError

LIMIT = 1048576
API = "kernelledger.desktop"
CAPABILITIES = [
    "portable_training",
    "native_snapshot_status",
    "batches",
    "input_claims",
    "secret_input_redaction",
    "operations",
    "change_feed",
    "blob_ranges",
    "gc_preview",
    "migrate",
]
ROLES = ("control", "subscription", "blob")


def encode(message: Any) -> bytes:
    data = json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode()
    if len(data) > LIMIT:
        raise SourceError("too_large", "JSON frame exceeds 1 MiB")
    return struct.pack(">I", len(data)) + data


async def read(reader: Any) -> Any:
    size = struct.unpack(">I", await reader.readexactly(4))[0]
    if size > LIMIT:
        raise SourceError("too_large", "JSON frame exceeds 1 MiB")
    message = json.loads(await reader.readexactly(size))
    if not isinstance(message, dict):
        raise SourceError("invalid_request", "frame must be an object")
    return cast(dict[str, Any], message)


async def send(writer: Any, message: Any) -> None:
    writer.write(encode(message))
    await writer.drain()


def error(exception: Exception) -> Any:
    code = getattr(exception, "code", "internal")
    if code == "internal" and isinstance(exception, (ValueError, KeyError, TypeError)):
        code = "invalid_request"
        if "--trust" in str(exception):
            code = "trust_required"
        elif "Python major.minor mismatch" in str(exception):
            code = "python_mismatch"
        elif "record not found" in str(exception):
            code = "not_found"
    if isinstance(exception, FileNotFoundError):
        code = "not_found"
    return {
        "code": code,
        "message": str(exception),
        "retryable": False,
        "details": getattr(exception, "details", {}),
    }


def hello(message: Any) -> None:
    import uuid

    if message.get("kind") != "hello" or message.get("api") != API:
        raise SourceError("invalid_request", "expected desktop hello")
    if 1 not in message.get("versions", []):
        raise SourceError("unsupported_version", "supported desktop versions: 1", {"versions": [1]})
    if message.get("role") not in ROLES:
        raise SourceError("invalid_request", "invalid connection role")
    uuid.UUID(message["client_id"])
    if not isinstance(message.get("client_name"), str) or len(message["client_name"]) > 256:
        raise SourceError("invalid_request", "invalid client name")


def page_window(request: Any) -> tuple[int, int]:
    import base64

    size = request.get("page_size", 100)
    if not isinstance(size, int) or not 1 <= size <= 500:
        raise SourceError("invalid_request", "page_size must be between 1 and 500")
    offset = 0
    if request.get("cursor"):
        try:
            offset = int(base64.b64decode(request["cursor"], validate=True))
        except (ValueError, TypeError):
            raise SourceError("invalid_request", "invalid cursor") from None
        if offset < 0:
            raise SourceError("invalid_request", "invalid cursor")
    return size, offset


def page(rows: Any, request: Any) -> Any:
    import base64

    size, offset = page_window(request)
    cursor = (
        base64.b64encode(str(offset + size).encode()).decode()
        if offset + size < len(rows)
        else None
    )
    return {"items": rows[offset : offset + size], "cursor": cursor}


def sql_page(ledger: Any, sql: str, args: Any, request: Any) -> Any:
    """Page with SQL LIMIT/OFFSET. The cursor remains a base64 offset."""
    import base64

    size, offset = page_window(request)
    total = ledger.db.execute(f"SELECT COUNT(*) FROM ({sql}) AS listed", args).fetchone()[0]
    items = ledger.rows(f"{sql} LIMIT ? OFFSET ?", tuple(args) + (size, offset))
    cursor = (
        base64.b64encode(str(offset + size).encode()).decode() if offset + size < total else None
    )
    return {"items": items, "cursor": cursor}
