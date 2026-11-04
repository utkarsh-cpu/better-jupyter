"""Bounded immutable source windows for debugging and profile navigation."""

from typing import Any, BinaryIO

from ..blobs import BlobStore, check_hash
from ..source import SourceError

SOURCE_LIMIT = 8 * 1024 * 1024


def window(stream: BinaryIO, line: int) -> list[dict[str, Any]]:
    first, last = max(1, line - 7), line + 7
    current, length = 1, 0
    prefix = bytearray()
    rows: list[dict[str, Any]] = []
    while current <= last:
        chunk = stream.read(8192)
        if not chunk:
            if length or current == 1:
                if current >= first:
                    rows.append(
                        {
                            "line": current,
                            "text": bytes(prefix).decode("utf-8", errors="replace"),
                            "truncated": length > 512,
                        }
                    )
            break
        parts = chunk.split(b"\n")
        for index, part in enumerate(parts):
            length += len(part)
            prefix.extend(part[: max(0, 512 - len(prefix))])
            if index < len(parts) - 1:
                if current >= first:
                    rows.append(
                        {
                            "line": current,
                            "text": bytes(prefix).decode("utf-8", errors="replace"),
                            "truncated": length > 512,
                        }
                    )
                current += 1
                prefix.clear()
                length = 0
                if current > last:
                    break
    return rows


def preview(blobs: BlobStore, record: Any, request: Any) -> Any:
    digest = check_hash(record["source_sha256"])
    if request["source_revision"] != digest:
        raise SourceError("source_conflict", "Recorded source revision differs from this location")
    with (blobs.root / digest).open("rb") as stream:
        stream.seek(0, 2)
        if stream.tell() > SOURCE_LIMIT:
            raise SourceError("too_large", "Recorded source preview is limited to 8 MiB files")
        evidence = blobs.verify_stream(stream, digest)
        stream.seek(0)
        rows = window(stream, request["line"])
        evidence.check_current()
    return {
        "exec_id": record["exec_id"],
        "cell_id": record["cell_id"],
        "source_revision": digest,
        "requested_line": request["line"],
        "lines": rows,
        "byte_size": evidence.byte_size,
        "previous_line": max(1, request["line"] - 15) if request["line"] > 8 else None,
        "next_line": request["line"] + 15
        if rows and rows[-1]["line"] == request["line"] + 7 and request["line"] + 15 <= 100000
        else None,
    }
