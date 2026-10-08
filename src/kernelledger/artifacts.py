"""Experimental v5 chunk store. Independent of the v4 runtime ledger.

Every published object is a root until explicitly released. Writers, transfers,
and GC serialize on an OS lock; leases protect transitive closure between calls.
The format is deliberately namespaced until the runtime successor is adopted.
"""

import hashlib
import io
import json
import os
import re
import shutil
import tempfile
import uuid
from collections.abc import Generator, Iterator
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Protocol, cast

from .blobs import BlobStore, check_hash
from .supervisor.platform import DirectoryLock

ARTIFACT_SCHEMA = "kernelledger.experimental.artifact/1"
STATE_SCHEMA = "kernelledger.experimental.state/1"
MAX_METADATA = 8 * 1024**2
MAX_CHUNK = 16 * 1024**2
KINDS = {"tensor-storage", "adapter-state", "input", "native-process", "environment", "cache"}


class BinaryReader(Protocol):
    def read(self, size: int = -1, /) -> bytes: ...


def canonical(value: Any) -> bytes:
    return json.dumps(
        value, sort_keys=True, ensure_ascii=False, allow_nan=False, separators=(",", ":")
    ).encode("utf-8")


def integer(value: Any, maximum: int, field: str) -> int:
    if type(value) is not int or not 0 <= value <= maximum:
        raise ValueError("invalid " + field)
    return value


def validate_artifact(value: Any, max_bytes: int = 64 * 1024**3) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError("unsupported artifact schema")
    value = cast(dict[str, Any], value)
    if value.get("schema") != ARTIFACT_SCHEMA:
        raise ValueError("unsupported artifact schema")
    if set(value) != {"schema", "kind", "byte_size", "sha256", "chunks"}:
        raise ValueError("invalid artifact fields")
    if not isinstance(value["kind"], str) or value["kind"] not in KINDS:
        raise ValueError("unsupported artifact kind")
    size = integer(value["byte_size"], max_bytes, "artifact size")
    check_hash(value["sha256"])
    chunks = value["chunks"]
    if not isinstance(chunks, list) or len(cast(list[Any], chunks)) > MAX_METADATA // 80:
        raise ValueError("invalid artifact chunks")
    total = 0
    for chunk in cast(list[Any], chunks):
        if not isinstance(chunk, dict):
            raise ValueError("invalid artifact chunk")
        chunk = cast(dict[str, Any], chunk)
        if set(chunk) != {"blob", "byte_size"}:
            raise ValueError("invalid artifact chunk")
        check_hash(chunk["blob"])
        length = integer(chunk["byte_size"], MAX_CHUNK, "chunk size")
        if not length:
            raise ValueError("empty artifact chunk")
        total += length
        if total > max_bytes:
            raise ValueError("artifact size limit")
    if total != size:
        raise ValueError("artifact size mismatch")
    return value


class ArtifactStore:
    """Explicit prototype store; never creates or migrates a runtime database."""

    def __init__(
        self,
        root: Path,
        *,
        readonly: bool = False,
        chunk_size: int = 4 * 1024**2,
        max_bytes: int = 64 * 1024**3,
        quota_bytes: int = 128 * 1024**3,
    ) -> None:
        self.root = Path(root)
        self.readonly = readonly
        self.chunk_size = integer(chunk_size, MAX_CHUNK, "chunk size")
        if not self.chunk_size:
            raise ValueError("empty chunk size")
        self.max_bytes = integer(max_bytes, 2**63 - 1, "artifact limit")
        self.quota_bytes = integer(quota_bytes, 2**63 - 1, "store quota")
        self.blobs = BlobStore(root, readonly=readonly)
        if not readonly:
            for name in ("roots", "leases"):
                (self.root / name).mkdir(parents=True, exist_ok=True)

    @contextmanager
    def _write(self) -> Generator[None]:
        if self.readonly:
            raise ValueError("read-only artifact store")
        lock = DirectoryLock(self.root / "coordination").acquire()
        try:
            yield
        finally:
            lock.close()

    def _record(self, path: Path, value: Any) -> None:
        fd, name = tempfile.mkstemp(dir=self.blobs.tmp)
        try:
            with os.fdopen(fd, "wb") as stream:
                stream.write(canonical(value))
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(name, path)
            if os.name != "nt":
                directory = os.open(path.parent, os.O_RDONLY)
                try:
                    os.fsync(directory)
                finally:
                    os.close(directory)
        finally:
            Path(name).unlink(missing_ok=True)

    def _put(self, data: bytes, usage: list[int]) -> str:
        digest = hashlib.sha256(data).hexdigest()
        if not (self.blobs.root / digest).exists():
            if usage[0] + len(data) > self.quota_bytes:
                raise ValueError("artifact store quota exceeded")
            usage[0] += len(data)
        return self.blobs.put(data)

    def _usage(self) -> int:
        return sum(p.stat().st_size for p in self.blobs.root.iterdir() if p.is_file())

    def _root_path(self, digest: str, owner: str | None) -> Path:
        check_hash(digest)
        if owner is not None and re.fullmatch("[0-9a-f]{32}", owner) is None:
            raise ValueError("invalid artifact owner")
        return self.root / "roots" / (digest + ("-" + owner if owner else ""))

    def _retain(self, digest: str, owner: str | None = None) -> None:
        self._record(self._root_path(digest, owner), {"blob": digest})

    def retain(self, digest: str) -> None:
        with self._write():
            self.verify_closure(digest)
            self._retain(digest)

    @contextmanager
    def staging(self) -> Generator[str]:
        """Temporary roots hand off to a published parent, or become GC-able on failure."""
        owner = uuid.uuid4().hex
        try:
            yield owner
        finally:
            with self._write():
                for path in (self.root / "roots").iterdir():
                    if path.name.endswith("-" + owner):
                        check_hash(path.name[:-33])
                        path.unlink()

    def put_stream(
        self, stream: BinaryReader, *, kind: str = "input", owner: str | None = None
    ) -> str:
        if not isinstance(cast(Any, kind), str) or kind not in KINDS:
            raise ValueError("unsupported artifact kind")
        with self._write():
            usage = [self._usage()]
            chunks: list[dict[str, Any]] = []
            size, hasher = 0, hashlib.sha256()
            # Short reads do not change content addressing or chunk boundaries.
            while True:
                block = bytearray()
                while len(block) < self.chunk_size:
                    data: Any = stream.read(self.chunk_size - len(block))
                    if not isinstance(data, bytes) or len(data) > self.chunk_size - len(block):
                        raise ValueError("invalid binary stream read")
                    if not data:
                        break
                    block.extend(data)
                    if size + len(block) > self.max_bytes:
                        raise ValueError("artifact size limit")
                if not block:
                    break
                data = bytes(block)
                size += len(data)
                hasher.update(data)
                chunks.append({"blob": self._put(data, usage), "byte_size": len(data)})
                if len(chunks) > MAX_METADATA // 80:
                    raise ValueError("artifact metadata limit")
            manifest = {
                "schema": ARTIFACT_SCHEMA,
                "kind": kind,
                "byte_size": size,
                "sha256": hasher.hexdigest(),
                "chunks": chunks,
            }
            encoded = canonical(manifest)
            if len(encoded) > MAX_METADATA:
                raise ValueError("artifact metadata limit")
            digest = self._put(encoded, usage)
            # Verify before advertising a root. Failed captures leave only GC-able blobs.
            self.verify(digest)
            self._retain(digest, owner)
            return digest

    def put_bytes(self, data: bytes, *, kind: str = "input") -> str:
        return self.put_stream(io.BytesIO(data), kind=kind)

    def put_state(self, value: dict[str, Any], *, owner: str | None = None) -> str:
        if value.get("schema") != STATE_SCHEMA:
            raise ValueError("unsupported state schema")
        data = canonical(value)
        if len(data) > MAX_METADATA:
            raise ValueError("state metadata limit")
        with self._write():
            digest = self._put(data, [self._usage()])
            self.verify_closure(digest)
            self._retain(digest, owner)
            return digest

    def metadata(self, digest: str) -> dict[str, Any]:
        path = self.blobs.root / check_hash(digest)
        if path.stat().st_size > MAX_METADATA:
            raise ValueError("metadata size limit")
        value = json.loads(self.blobs.get(digest))
        if not isinstance(value, dict):
            raise ValueError("invalid metadata")
        return cast(dict[str, Any], value)

    def manifest(self, digest: str) -> dict[str, Any]:
        return validate_artifact(self.metadata(digest), self.max_bytes)

    def verify(self, digest: str) -> dict[str, Any]:
        manifest = self.manifest(digest)
        hasher = hashlib.sha256()
        for chunk in manifest["chunks"]:
            path = self.blobs.root / chunk["blob"]
            if path.stat().st_size != chunk["byte_size"]:
                raise ValueError("artifact chunk size mismatch")
            hasher.update(self.blobs.get(chunk["blob"]))
        if hasher.hexdigest() != manifest["sha256"]:
            raise ValueError("artifact payload hash mismatch")
        return manifest

    def read_chunks(self, digest: str) -> Iterator[bytes]:
        manifest = self.verify(digest)
        for chunk in manifest["chunks"]:
            yield self.blobs.get(chunk["blob"])

    def verify_closure(self, digest: str) -> set[str]:
        # BlobStore closure recognizes only versioned manifests, never arbitrary data.
        closure: set[str] = self.blobs.closure([check_hash(digest)])
        for item in closure:
            path = self.blobs.root / item
            if path.stat().st_size <= MAX_METADATA:
                try:
                    value = json.loads(self.blobs.get(item))
                except (ValueError, UnicodeDecodeError):
                    continue
                if (
                    isinstance(value, dict)
                    and cast(dict[str, Any], value).get("schema") == ARTIFACT_SCHEMA
                ):
                    self.verify(item)
        return closure

    @contextmanager
    def lease(self, digest: str) -> Generator[str]:
        """No timeout-based expiry: interrupted leases require explicit release."""
        identity = uuid.uuid4().hex
        path = self.root / "leases" / identity
        with self._write():
            self.verify_closure(digest)
            self._record(path, {"blob": digest})
        try:
            yield identity
        finally:
            with self._write():
                path.unlink(missing_ok=True)

    def release(self, digest: str, *, owner: str | None = None) -> None:
        with self._write():
            self._root_path(digest, owner).unlink(missing_ok=True)

    def gc(self, *, dry_run: bool = True) -> dict[str, Any]:
        with self._write():
            roots: list[str] = []
            for directory in ("roots", "leases"):
                for path in (self.root / directory).iterdir():
                    roots.append(check_hash(json.loads(path.read_bytes())["blob"]))
            # A corrupt/missing root aborts GC, rather than dropping its dependencies.
            keep: set[str] = set()
            for digest in roots:
                keep.update(self.verify_closure(digest))
            discarded = sorted(
                p.name for p in self.blobs.root.iterdir() if p.is_file() and p.name not in keep
            )
            for digest in discarded:
                check_hash(digest)
            byte_size = sum((self.blobs.root / digest).stat().st_size for digest in discarded)
            if not dry_run:
                for digest in discarded:
                    (self.blobs.root / digest).unlink()
            return {"dry_run": dry_run, "blobs": discarded, "byte_size": byte_size}

    def export(self, digest: str, destination: Path) -> None:
        """Materialize a self-contained directory without donor database or paths."""
        destination = Path(destination)
        if destination.exists():
            raise ValueError("export destination exists")
        # Lease is a write; read-only callers can inspect/verify but cannot export.
        with self.lease(digest):
            closure = self.verify_closure(digest)
            for item in closure:
                try:
                    value = self.metadata(item)
                except (ValueError, UnicodeDecodeError):
                    continue
                if value.get("schema") == ARTIFACT_SCHEMA and value.get("kind") == "native-process":
                    raise ValueError("native export requires a sensitive export policy")
            destination.parent.mkdir(parents=True, exist_ok=True)
            stage = Path(tempfile.mkdtemp(dir=destination.parent))
            try:
                (stage / "blobs").mkdir()
                for item in closure:
                    with (self.blobs.root / item).open("rb") as source:
                        with (stage / "blobs" / item).open("wb") as output:
                            shutil.copyfileobj(source, output, MAX_CHUNK)
                            output.flush()
                            os.fsync(output.fileno())
                with (stage / "selection.json").open("wb") as selection:
                    selection.write(canonical({"schema": STATE_SCHEMA, "blob": digest}))
                    selection.flush()
                    os.fsync(selection.fileno())
                ArtifactStore(stage, readonly=True, max_bytes=self.max_bytes).verify_closure(digest)
                os.rename(stage, destination)
            finally:
                if stage.exists():
                    shutil.rmtree(stage)

    def import_directory(self, source: Path) -> str:
        source = Path(source)
        selection_path = source / "selection.json"
        if selection_path.is_symlink() or selection_path.stat().st_size > MAX_METADATA:
            raise ValueError("invalid artifact selection")
        selection = json.loads(selection_path.read_bytes())
        if (
            not isinstance(selection, dict)
            or cast(dict[str, Any], selection).get("schema") != STATE_SCHEMA
        ):
            raise ValueError("unsupported artifact selection")
        digest = check_hash(selection["blob"])
        # Verify using a read-only store before writing any imported bytes.
        donor = ArtifactStore(source, readonly=True, max_bytes=self.max_bytes)
        if (source / "blobs").is_symlink():
            raise ValueError("symlink artifact directory")
        for path in (source / "blobs").iterdir():
            check_hash(path.name)
            if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_CHUNK:
                raise ValueError("invalid artifact file")
        closure = donor.verify_closure(digest)
        total = sum((source / "blobs" / item).stat().st_size for item in closure)
        if total > self.quota_bytes:
            raise ValueError("artifact import quota exceeded")
        with self._write():
            usage = [self._usage()]
            for item in closure:
                self._put(donor.blobs.get(item), usage)
            self.verify_closure(digest)
            self._retain(digest)
        return digest
