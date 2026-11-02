"""Recorded navigation is bounded, read-only, revision checked and owner independent."""

import io
import json
import os

import pytest

from kernelledger.blobs import BlobStore
from kernelledger.desktop.contracts import validate
from kernelledger.desktop.helper import Helper
from kernelledger.desktop.recorded_source import window
from kernelledger.ledger import Ledger
from kernelledger.notebook import Notebook
from kernelledger.source import SourceError


def seeded(tmp_path):
    notebook = Notebook(tmp_path)
    notebook.init()
    source = "\n".join(f"# line {i} λ" for i in range(1, 300)) + "\n" + "x" * 1000000
    blobs = BlobStore(tmp_path / ".kl")
    digest = blobs.put(source.encode())
    ledger = Ledger(tmp_path / ".kl/ledger.sqlite")
    session = ledger.session(notebook.read()["id"], {})
    execution = ledger.execution(session, "source-cell", digest)
    ledger.close()
    return notebook, execution, digest


def test_recorded_window_streams_and_limits_each_line():
    class BoundedReader(io.BytesIO):
        def read(self, size=-1):
            assert 0 < size <= 8192
            return super().read(size)

    rows = window(BoundedReader(b"x" * 1000000 + b"\nlast"), 1)
    assert rows[0] == {"line": 1, "text": "x" * 512, "truncated": True}
    assert rows[1]["text"] == "last"


def test_recorded_source_never_starts_worker_or_writes_history(tmp_path):
    notebook, execution, digest = seeded(tmp_path)
    database = tmp_path / ".kl/ledger.sqlite"
    before = database.read_bytes()
    request = {
        "method": "source.execution",
        "exec_id": execution,
        "source_revision": digest,
        "line": 150,
    }
    reply = Helper(tmp_path).dispatch(request)
    assert [row["line"] for row in reply["lines"]] == list(range(143, 158))
    assert reply["lines"][7]["text"] == "# line 150 λ"
    assert len(json.dumps(reply).encode()) < 65536
    assert database.read_bytes() == before
    assert not (notebook.root / ".kl/supervisor.json").exists()
    with pytest.raises(SourceError) as error:
        Helper(tmp_path).dispatch({**request, "source_revision": "0" * 64})
    assert error.value.code == "source_conflict"
    for invalid in (False, 0, 100001, "1"):
        with pytest.raises(SourceError):
            validate({**request, "line": invalid}, helper=True)
    (tmp_path / ".kl/blobs" / digest).write_bytes(b"corrupt")
    with pytest.raises(ValueError):
        Helper(tmp_path).dispatch(request)


def test_native_recorded_source_matches_python(tmp_path):
    if not os.environ.get("KL_NATIVE_DAEMON"):
        pytest.skip("Requires an explicit native daemon build")
    from test_native_helper import NativeHelper, assert_match

    _, execution, digest = seeded(tmp_path)
    helper = NativeHelper(tmp_path)
    try:
        for line in (1, 150, 300, 999):
            assert_match(
                helper,
                tmp_path,
                "source.execution",
                exec_id=execution,
                source_revision=digest,
                line=line,
            )
        for line in (0, False, "2", 100001):
            assert_match(
                helper,
                tmp_path,
                "source.execution",
                exec_id=execution,
                source_revision=digest,
                line=line,
            )
        assert_match(
            helper,
            tmp_path,
            "source.execution",
            exec_id=execution,
            source_revision="0" * 64,
            line=1,
        )
    finally:
        helper.close()
