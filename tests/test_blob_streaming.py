import asyncio
import hashlib
import json
import socket
import struct
import threading

import pytest

from kernelledger.blobs import BUFFER_SIZE, BlobStore, VerifiedBlob
from kernelledger.supervisor import send_stored_blob
from kernelledger.worker import Channel, receive


def test_verified_metadata_and_existing_corruption(tmp_path, monkeypatch):
    store = BlobStore(tmp_path)
    data = b"payload" * 200000
    first = store.put_verified(data)
    assert first.byte_size == len(data)
    assert first.publication == "created"
    # Metadata and duplicate verification must not use the whole-blob API.
    monkeypatch.setattr(store, "get", lambda _: pytest.fail("unbounded blob read"))
    duplicate = store.put_verified(data)
    assert duplicate.sha256 == first.sha256
    assert duplicate.publication == "existing"
    (store.root / first.sha256).write_bytes(b"corrupt")
    with pytest.raises(ValueError, match="changed"):
        first.check_current()
    with pytest.raises(ValueError, match="hash mismatch"):
        store.put_verified(data)
    with pytest.raises(TypeError):
        VerifiedBlob(
            first.sha256, 1, "existing", store.root / first.sha256, (0,) * 5, _verifier=object()
        )


def test_closure_verifies_binary_without_loading_it(tmp_path, monkeypatch):
    store = BlobStore(tmp_path)
    data = store.put(b"\x93NUMPY" + bytes(BUFFER_SIZE * 3))
    manifest = store.put(
        json.dumps({"schema": 1, "bindings": {}, "objects": [{"blob": data}]}).encode()
    )
    monkeypatch.setattr(store, "get", lambda _: pytest.fail("unbounded blob read"))
    seen = store.verified_closure([manifest])
    assert set(seen) == {manifest, data}
    assert seen[data].byte_size == BUFFER_SIZE * 3 + 6
    (store.root / data).write_bytes(b"corrupt")
    with pytest.raises(ValueError, match="hash mismatch"):
        store.verified_closure([manifest])


@pytest.mark.parametrize("prefix", [b"\0", b"\xff\xfe\0\0", b"\xef\xbb\xbf", b"\0\xff", b"[", b" "])
def test_binary_encoding_prefix_does_not_load_whole_blob(tmp_path, monkeypatch, prefix):
    store = BlobStore(tmp_path)
    digest = store.put(prefix + bytes(BUFFER_SIZE * 2))
    monkeypatch.setattr(json, "load", lambda _: pytest.fail("binary loaded as JSON"))
    assert store.closure([digest]) == {digest}


@pytest.mark.parametrize("boundary", ["fsync", "replace"])
def test_failed_durable_publication_leaves_no_target(tmp_path, monkeypatch, boundary):
    import os

    store = BlobStore(tmp_path)

    def fail(*_):
        raise OSError("injected storage failure")

    monkeypatch.setattr(os, boundary, fail)
    with pytest.raises(OSError, match="storage failure"):
        store.put_verified(b"unpublished")
    assert list(store.root.iterdir()) == []
    assert list(store.tmp.iterdir()) == []


@pytest.mark.parametrize(
    "encoding", ["utf-8", "utf-8-sig", "utf-16", "utf-32", "utf-16-be", "utf-32-be"]
)
def test_closure_retains_json_encoding_and_whitespace_support(tmp_path, encoding):
    store = BlobStore(tmp_path)
    child = store.put(b"child")
    raw = " \t\r\n" * 300000 + json.dumps(
        {"schema": 1, "bindings": {}, "objects": [{"blob": child}]}
    )
    root = store.put(raw.encode(encoding))
    assert store.closure([root]) == {root, child}


@pytest.mark.parametrize("failure", ["eof", "hash", "existing"])
def test_receive_failure_never_publishes_partial_blob(tmp_path, failure):
    store = BlobStore(tmp_path)
    data = b"binary" * 200000
    digest = hashlib.sha256(data).hexdigest()
    if failure == "existing":
        (store.root / digest).write_bytes(b"corrupted existing file")
    expected = "0" * 64 if failure == "hash" else digest

    async def transfer():
        reader = asyncio.StreamReader()
        reader.feed_data(data[:100] if failure == "eof" else data)
        reader.feed_eof()
        return await store.receive_verified(reader, expected, len(data))

    with pytest.raises((ValueError, asyncio.IncompleteReadError)):
        asyncio.run(transfer())
    assert not list(store.tmp.iterdir())
    assert not (store.root / expected).exists() or failure == "existing"


def test_sync_keeps_control_responsive_and_cancellation_waits_for_io(tmp_path, monkeypatch):
    import kernelledger.blobs as storage

    store = BlobStore(tmp_path)
    entered, release = threading.Event(), threading.Event()
    original = storage._sync

    def slow_sync(stream):
        entered.set()
        assert release.wait(5)
        original(stream)  # cancellation must not have closed this stream

    monkeypatch.setattr(storage, "_sync", slow_sync)

    async def exercise():
        reader = asyncio.StreamReader()
        reader.feed_data(b"payload")
        reader.feed_eof()
        pending = asyncio.create_task(
            store.receive_verified(reader, hashlib.sha256(b"payload").hexdigest(), 7)
        )
        try:
            deadline = asyncio.get_running_loop().time() + 5
            while not entered.is_set():
                assert asyncio.get_running_loop().time() < deadline
                await asyncio.sleep(0.01)
            # The event loop remains serviceable while fsync is held in a thread.
            pending.cancel()
            await asyncio.sleep(0.02)
            assert not pending.done()
            pending.cancel()  # repeated cancel cannot close the owned file early
            await asyncio.sleep(0.02)
            assert not pending.done()
        finally:
            release.set()
        with pytest.raises(asyncio.CancelledError):
            await pending

    asyncio.run(exercise())
    assert not list(store.tmp.iterdir())
    assert not list(store.root.iterdir())


def test_receive_and_stored_send_wire_roundtrip(tmp_path):
    store = BlobStore(tmp_path)
    data = bytes(range(256)) * 13000
    digest = store.put(data)

    class Writer:
        def __init__(self):
            self.parts = []

        def write(self, chunk):
            assert len(chunk) <= BUFFER_SIZE
            self.parts.append(bytes(chunk))

        async def drain(self):
            pass

    writer = Writer()
    asyncio.run(send_stored_blob(writer, store, digest))
    wire = b"".join(writer.parts)
    size = struct.unpack(">I", wire[:4])[0]
    header = json.loads(wire[4 : 4 + size])
    assert header == {"schema": 1, "kind": "blob", "sha256": digest, "byte_size": len(data)}
    assert wire[4 + size :] == data
    left, right = socket.socketpair()
    try:
        sender = threading.Thread(target=Channel(left).blob, args=(data,))
        sender.start()
        message = receive(right)
        sender.join(timeout=5)
        assert not sender.is_alive()
        assert message["data"] == data
        assert message["sha256"] == digest
    finally:
        left.close()
        right.close()


@pytest.mark.parametrize("size", [-1, 2**31 + 1, True, "5", None])
def test_worker_rejects_invalid_binary_size_before_reading_body(size):
    left, right = socket.socketpair()
    try:
        header = json.dumps(
            {"schema": 1, "kind": "blob", "sha256": "0" * 64, "byte_size": size}
        ).encode()
        left.sendall(struct.pack(">I", len(header)) + header)
        with pytest.raises(ValueError, match="invalid binary frame size"):
            receive(right)
    finally:
        left.close()
        right.close()
