"""Durable immutable content-addressed payloads."""

import asyncio
import codecs
import hashlib
import os
import re
import tempfile
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Any, BinaryIO, Literal, cast

BUFFER_SIZE = 1024 * 1024
_VERIFIER = object()


async def blob_io(function: Callable[..., Any], *args: Any) -> Any:
    """Finish owned blocking I/O before propagating cancellation and closing files."""
    task = asyncio.create_task(asyncio.to_thread(function, *args))
    canceled = False
    while not task.done():
        try:
            await asyncio.shield(task)
        except asyncio.CancelledError:
            canceled = True
    result = task.result()
    if canceled:
        raise asyncio.CancelledError
    return result


def _sync(stream: BinaryIO) -> None:
    stream.flush()
    os.fsync(stream.fileno())


@dataclass(frozen=True, init=False)
class VerifiedBlob:
    """Operation-local evidence, constructed only after storage verification.

    This is not a hash cache. Callers must not retain it across operations. The
    runtime lock excludes another publisher/GC; file identity detects replacement
    or normal in-place edits between verification and metadata publication.
    """

    sha256: str
    byte_size: int
    publication: Literal["created", "existing"]
    _path: Path
    _identity: tuple[int, int, int, int, int]

    def __init__(
        self,
        sha256: str,
        byte_size: int,
        publication: Literal["created", "existing"],
        path: Path,
        identity: tuple[int, int, int, int, int],
        *,
        _verifier: object,
    ):
        if _verifier is not _VERIFIER:
            raise TypeError("verified blob metadata requires storage verification")
        for key, value in (
            ("sha256", sha256),
            ("byte_size", byte_size),
            ("publication", publication),
            ("_path", path),
            ("_identity", identity),
        ):
            object.__setattr__(self, key, value)

    def check_current(self) -> None:
        if _identity(self._path.stat()) != self._identity:
            raise ValueError(f"blob changed after verification: {self.sha256}")


def _identity(stat: os.stat_result) -> tuple[int, int, int, int, int]:
    # CPython 3.12 Windows stat/fstat disagree about creation/change time after
    # rename. File ID, size and last-write time remain comparable there.
    return (
        stat.st_dev,
        stat.st_ino,
        stat.st_size,
        stat.st_mtime_ns,
        (0 if os.name == "nt" else stat.st_ctime_ns),
    )


def _json_object(stream: BinaryIO) -> bool:
    """Inspect only the first non-whitespace code point, including JSON BOMs.

    A zero-filled binary blob is not a UTF-32 JSON document. Avoid loading it
    just because its first byte could be part of an encoding signature.
    """
    stream.seek(0)
    prefix = stream.read(4)
    width, offset = 1, 0
    order: Literal["little", "big"] = "little"
    for marker, unit, endian in (
        (codecs.BOM_UTF32_LE, 4, "little"),
        (codecs.BOM_UTF32_BE, 4, "big"),
        (codecs.BOM_UTF16_LE, 2, "little"),
        (codecs.BOM_UTF16_BE, 2, "big"),
        (codecs.BOM_UTF8, 1, "little"),
    ):
        if prefix.startswith(marker):
            width, order, offset = unit, cast(Literal["little", "big"], endian), len(marker)
            break
    else:
        if len(prefix) >= 2 and prefix[0] == 0:
            width, order = (4 if len(prefix) == 4 and prefix[1] == 0 else 2), "big"
        elif len(prefix) >= 2 and prefix[1] == 0:
            width = 4 if len(prefix) == 4 and prefix[2:] == b"\0\0" else 2
    stream.seek(offset)
    while chunk := stream.read(BUFFER_SIZE):
        if width == 1:
            stripped = chunk.lstrip(b" \t\r\n")
            if stripped:
                return stripped.startswith(b"{")
        else:
            for start in range(0, len(chunk), width):
                codepoint = int.from_bytes(chunk[start : start + width], order)
                if codepoint not in (9, 10, 13, 32):
                    return codepoint == 123
    return False


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
        # At most two transfers submit blocking I/O, with one 1 MiB chunk per
        # transfer. Awaiting every write supplies byte-level backpressure.
        self._transfers = asyncio.Semaphore(2)
        if not readonly:
            self.root.mkdir(parents=True, exist_ok=True)
            self.tmp.mkdir(parents=True, exist_ok=True)

    def put(self, data: Any, expected: Any = None) -> Any:
        return self.put_verified(data, expected).sha256

    def put_verified(self, data: bytes, expected: str | None = None) -> VerifiedBlob:
        digest = hashlib.sha256(data).hexdigest()
        if expected is not None and digest != check_hash(expected):
            raise ValueError("blob hash mismatch")
        target = self.root / digest
        if target.exists():
            return self.verify(digest)
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
        return VerifiedBlob(
            digest, len(data), "created", target, _identity(target.stat()), _verifier=_VERIFIER
        )

    def verify_stream(self, stream: BinaryIO, digest: str) -> VerifiedBlob:
        """Verify an open stream from offset zero, leaving it at EOF."""
        check_hash(digest)
        stream.seek(0)
        before = _identity(os.fstat(stream.fileno()))
        hasher, size = hashlib.sha256(), 0
        while data := stream.read(BUFFER_SIZE):
            hasher.update(data)
            size += len(data)
        after = _identity(os.fstat(stream.fileno()))
        if before != after or hasher.hexdigest() != digest:
            raise ValueError(f"blob hash mismatch: {digest}")
        result = VerifiedBlob(
            digest, size, "existing", self.root / digest, after, _verifier=_VERIFIER
        )
        result.check_current()
        return result

    def verify(self, digest: str) -> VerifiedBlob:
        """Rehash on every call, in bounded buffers, including existing blobs."""
        with (self.root / check_hash(digest)).open("rb") as stream:
            return self.verify_stream(stream, digest)

    def get(self, digest: Any) -> Any:
        data = (self.root / check_hash(digest)).read_bytes()
        if hashlib.sha256(data).hexdigest() != digest:
            raise ValueError(f"blob hash mismatch: {digest}")
        return data

    async def receive(self, reader: Any, digest: str, byte_size: int) -> str:
        return (await self.receive_verified(reader, digest, byte_size)).sha256

    async def receive_verified(self, reader: Any, digest: str, byte_size: int) -> VerifiedBlob:
        async with self._transfers:
            return await self._receive_verified(reader, digest, byte_size)

    async def _receive_verified(self, reader: Any, digest: str, byte_size: int) -> VerifiedBlob:
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
                    data = await reader.readexactly(min(remaining, BUFFER_SIZE))
                    hasher.update(data)
                    await blob_io(stream.write, data)
                    remaining -= len(data)
                await blob_io(_sync, stream)
            if hasher.hexdigest() != digest:
                raise ValueError("blob hash mismatch")
            return await blob_io(self._publish_received, name, digest, byte_size)
        finally:
            Path(name).unlink(missing_ok=True)

    def _publish_received(self, name: str, digest: str, byte_size: int) -> VerifiedBlob:
        target = self.root / digest
        if target.exists():
            return self.verify(digest)
        os.replace(name, target)
        if os.name != "nt":
            directory = os.open(self.root, os.O_RDONLY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
        return VerifiedBlob(
            digest, byte_size, "created", target, _identity(target.stat()), _verifier=_VERIFIER
        )

    def closure(self, seeds: Any) -> Any:
        return set(self.verified_closure(seeds))

    def verified_closure(self, seeds: Any) -> dict[str, VerifiedBlob]:
        """Verify each reachable blob once; parse only potential JSON objects.

        Structured manifests still require memory proportional to their JSON.
        Non-JSON binary payloads do not need a payload-sized allocation here.
        """
        import json

        pending: list[str] = list(seeds)
        seen: dict[str, VerifiedBlob] = {}
        while pending:
            digest = pending.pop()
            if digest in seen:
                continue
            with (self.root / check_hash(digest)).open("rb") as stream:
                verified = self.verify_stream(stream, digest)
                seen[digest] = verified
                if not _json_object(stream):
                    continue
                stream.seek(0)
                try:
                    value = json.load(stream)
                except (ValueError, UnicodeDecodeError):
                    continue
                verified.check_current()
                if isinstance(value, dict) and (
                    (
                        cast(dict[str, Any], value).get("schema") == 1
                        and "objects" in value
                        and "bindings" in value
                    )
                    or cast(dict[str, Any], value).get("schema")
                    in (
                        "kernelledger.experimental.artifact/1",
                        "kernelledger.experimental.state/1",
                    )
                ):
                    pending.extend(references(value))
        return seen
