import hashlib
import io
import json

import pytest

from kernelledger.artifacts import ARTIFACT_SCHEMA, STATE_SCHEMA, ArtifactStore, canonical
from kernelledger.blobs import BlobStore


def test_stream_dedup_closure_export_and_clean_import(tmp_path):
    store = ArtifactStore(tmp_path / "store", chunk_size=4)
    digest = store.put_bytes(b"abcdabcdxyz")
    value = store.verify(digest)
    assert value["sha256"] == hashlib.sha256(b"abcdabcdxyz").hexdigest()
    assert len(value["chunks"]) == 3
    assert value["chunks"][0]["blob"] == value["chunks"][1]["blob"]
    assert len(store.blobs.closure([digest])) == 3
    before = store._usage()
    assert store.put_bytes(b"abcdabcdxyz") == digest
    assert store._usage() == before
    store.export(digest, tmp_path / "export")
    fresh = ArtifactStore(tmp_path / "fresh")
    assert fresh.import_directory(tmp_path / "export") == digest
    assert b"".join(fresh.read_chunks(digest)) == b"abcdabcdxyz"
    assert not (tmp_path / "fresh/ledger.sqlite").exists()


def test_short_reads_have_canonical_chunks(tmp_path):
    class ShortReads(io.BytesIO):
        def read(self, size=-1):
            assert size > 0
            return super().read(min(size, 3))

    store = ArtifactStore(tmp_path, chunk_size=8)
    assert store.put_stream(ShortReads(b"12345678901")) == store.put_bytes(b"12345678901")


def test_quota_limits_leave_no_published_root(tmp_path):
    store = ArtifactStore(tmp_path / "size", chunk_size=4, max_bytes=6)
    with pytest.raises(ValueError, match="size limit"):
        store.put_bytes(b"1234567")
    assert list((store.root / "roots").iterdir()) == []
    assert store.gc()["blobs"]
    tiny = ArtifactStore(tmp_path / "quota", chunk_size=4, quota_bytes=2)
    with pytest.raises(ValueError, match="quota"):
        tiny.put_bytes(b"1234")
    assert list((tiny.root / "roots").iterdir()) == []


def test_lease_protects_transitive_chunks_and_gc_refuses_corrupt_roots(tmp_path):
    store = ArtifactStore(tmp_path, chunk_size=4)
    digest = store.put_bytes(b"0123456789")
    with store.lease(digest):
        store.release(digest)
        assert store.gc(dry_run=False)["blobs"] == []
        assert b"".join(store.read_chunks(digest)) == b"0123456789"
    deleted = store.gc(dry_run=False)
    assert len(deleted["blobs"]) == 4
    assert list(store.blobs.root.iterdir()) == []
    digest = store.put_bytes(b"another")
    (store.blobs.root / digest).write_bytes(b"corrupt")
    with pytest.raises(ValueError, match="hash mismatch"):
        store.gc(dry_run=False)
    assert (store.blobs.root / digest).exists()


def test_corrupt_chunk_and_full_digest_refuse(tmp_path):
    store = ArtifactStore(tmp_path)
    digest = store.put_bytes(b"abcdefgh")
    manifest = store.manifest(digest)
    chunk = manifest["chunks"][0]["blob"]
    (store.blobs.root / chunk).write_bytes(b"01234567")
    with pytest.raises(ValueError, match="hash mismatch"):
        store.verify(digest)
    (store.blobs.root / chunk).write_bytes(b"abcdefgh")
    manifest["sha256"] = "0" * 64
    bad = store.blobs.put(canonical(manifest))
    with pytest.raises(ValueError, match="payload hash mismatch"):
        store.verify(bad)


def test_failed_import_never_publishes_snapshot(tmp_path):
    donor = ArtifactStore(tmp_path / "donor")
    digest = donor.put_bytes(b"portable")
    donor.export(digest, tmp_path / "export")
    chunk = donor.manifest(digest)["chunks"][0]["blob"]
    (tmp_path / "export/blobs" / chunk).unlink()
    recipient = ArtifactStore(tmp_path / "recipient")
    with pytest.raises(FileNotFoundError):
        recipient.import_directory(tmp_path / "export")
    assert not list((recipient.root / "roots").iterdir())


def test_empty_artifact_native_export_refused_and_readonly_has_no_side_effect(tmp_path):
    missing = tmp_path / "missing"
    reader = ArtifactStore(missing, readonly=True)
    with pytest.raises(FileNotFoundError):
        reader.metadata("0" * 64)
    assert not missing.exists()
    with pytest.raises(ValueError, match="read-only"):
        reader.put_bytes(b"x")
    store = ArtifactStore(tmp_path / "store")
    digest = store.put_bytes(b"")
    assert store.verify(digest)["chunks"] == []
    assert list(store.read_chunks(digest)) == []
    native = store.put_bytes(b"native process memory", kind="native-process")
    with pytest.raises(ValueError, match="sensitive export"):
        store.export(native, tmp_path / "unsafe-export")
    assert not (tmp_path / "unsafe-export").exists()


def test_state_closure_and_v4_closure_compatibility(tmp_path):
    store = ArtifactStore(tmp_path)
    artifact = store.put_bytes(b"payload")
    state = store.put_state(
        {"schema": STATE_SCHEMA, "kind": "prototype", "payload": {"blob": artifact}}
    )
    assert artifact in store.verify_closure(state)
    raw = store.blobs.put(
        json.dumps(
            {"schema": 1, "objects": {}, "bindings": {}, "extra": {"blob": artifact}}
        ).encode()
    )
    assert artifact in BlobStore(tmp_path).closure([raw])
    data = store.blobs.put(json.dumps({"blob": artifact}).encode())
    assert store.blobs.closure([data]) == {data}


def test_malformed_artifact_refuses(tmp_path):
    store = ArtifactStore(tmp_path)
    digest = store.put_bytes(b"a")
    value = store.manifest(digest)
    value["byte_size"] = 1 << 80
    bad = store.blobs.put(canonical(value))
    with pytest.raises(ValueError, match="size"):
        store.verify(bad)
    assert value["schema"] == ARTIFACT_SCHEMA


def test_owned_staging_does_not_release_independently_pinned_artifacts(tmp_path):
    store = ArtifactStore(tmp_path)
    digest = store.put_bytes(b"input")
    with pytest.raises(RuntimeError):
        with store.staging() as owner:
            assert store.put_stream(io.BytesIO(b"input"), owner=owner) == digest
            store.put_stream(io.BytesIO(b"abandoned"), owner=owner)
            raise RuntimeError("capture interrupted")
    store.gc(dry_run=False)
    assert b"".join(store.read_chunks(digest)) == b"input"
    assert len(list((store.root / "roots").iterdir())) == 1
