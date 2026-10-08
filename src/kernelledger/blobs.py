"""Durable immutable content-addressed payloads."""

import hashlib
import os
import re
import tempfile
from pathlib import Path
from typing import Any, cast


def check_hash(digest: Any) -> Any:
    if not isinstance(digest, str) or re.fullmatch("[0-9a-f]{64}", digest) is None:
        raise ValueError("invalid blob hash")
    return digest


def references(value: Any) -> Any:
    if isinstance(value, dict):
        for key, child in cast(dict[Any, Any], value).items():
            if (
                (
                    key in ("blob", "manifest_sha256", "source_sha256", "blob_sha256")
                    or (
                        key == "source"
                        and cast(dict[str, Any], value).get("k") in ("class", "function")
                    )
                )
                and isinstance(child, str)
                and re.fullmatch("[0-9a-f]{64}", child)
            ):
                yield child
            elif key != "value":
                yield from references(child)
    elif isinstance(value, list):
        for child in cast(list[Any], value):
            yield from references(child)


class BlobStore:
    def __init__(self, state: Any, readonly: bool = False) -> None:
        self.root, self.tmp = Path(state) / "blobs", Path(state) / "tmp"
        if not readonly:
            self.root.mkdir(parents=True, exist_ok=True)
            self.tmp.mkdir(parents=True, exist_ok=True)

    def put(self, data: Any, expected: Any = None) -> Any:
        digest = hashlib.sha256(data).hexdigest()
        if expected is not None and digest != check_hash(expected):
            raise ValueError("blob hash mismatch")
        target = self.root / digest
        if target.exists():
            self.get(digest)
            return digest
        fd, name = tempfile.mkstemp(dir=self.tmp)
        try:
            with os.fdopen(fd, "wb") as stream:
                stream.write(data)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(name, target)
            if os.name != "nt":
                directory = os.open(self.root, os.O_RDONLY)
                try:
                    os.fsync(directory)
                finally:
                    os.close(directory)
        finally:
            Path(name).unlink(missing_ok=True)
        return digest

    def get(self, digest: Any) -> Any:
        data = (self.root / check_hash(digest)).read_bytes()
        if hashlib.sha256(data).hexdigest() != digest:
            raise ValueError(f"blob hash mismatch: {digest}")
        return data

    async def receive(self, reader: Any, digest: str, byte_size: int) -> str:
        """Hash and durably publish a binary frame without keeping it all in RAM."""
        check_hash(digest)
        if type(byte_size) is not int or not 0 <= byte_size <= 2 * 1024**3:
            raise ValueError("invalid binary frame size")
        fd, name = tempfile.mkstemp(dir=self.tmp)
        hasher = hashlib.sha256()
        try:
            with os.fdopen(fd, "wb") as stream:
                remaining = byte_size
                while remaining:
                    data = await reader.readexactly(min(remaining, 1048576))
                    hasher.update(data)
                    stream.write(data)
                    remaining -= len(data)
                stream.flush()
                os.fsync(stream.fileno())
            if hasher.hexdigest() != digest:
                raise ValueError("blob hash mismatch")
            target = self.root / digest
            if not target.exists():
                os.replace(name, target)
                if os.name != "nt":
                    directory = os.open(self.root, os.O_RDONLY)
                    try:
                        os.fsync(directory)
                    finally:
                        os.close(directory)
            else:
                self.get(digest)
            return digest
        finally:
            Path(name).unlink(missing_ok=True)

    def closure(self, seeds: Any) -> Any:
        import json

        pending: list[Any]
        seen: set[Any]
        pending, seen = list(seeds), set()
        while pending:
            digest = pending.pop()
            if digest in seen:
                continue
            data = self.get(digest)
            seen.add(digest)
            try:
                value = json.loads(data)
                if (
                    isinstance(value, dict)
                    and (
                        (cast(dict[str, Any], value).get("schema") == 1
                         and "objects" in value and "bindings" in value)
                        or cast(dict[str, Any], value).get("schema") in (
                            "kernelledger.experimental.artifact/1",
                            "kernelledger.experimental.state/1",
                        )
                    )
                ):
                    pending.extend(references(value))
            except (ValueError, UnicodeDecodeError):
                pass
        return seen
