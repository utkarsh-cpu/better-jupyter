"""Interactive execution is exercised through a real isolated project worker."""

import concurrent.futures
import time

import pytest
from test_desktop import Client
from test_shell import execute, successful

from kernelledger.cli import ensure_supervisor, rpc
from kernelledger.ledger import Ledger
from kernelledger.source import SourceError
from kernelledger.worker import passive_attributes, passive_preview, suppress_result

pytest_plugins = ["test_shell"]


def test_restore_does_not_expose_reconstructing_namespace(notebook):
    source = "class Slow:\n    __import__('time').sleep(0.6)\nvalue = Slow()\n"
    ensure_supervisor(notebook.root)
    successful(notebook, source)
    checkpoint = rpc(notebook.root, {"command": "checkpoint"})
    client = Client(notebook.root)
    try:
        before = client.call("runtime.status")
        accepted = client.submit("restore", checkpoint_id=checkpoint["checkpoint_id"])
        deadline = time.monotonic() + 10
        while True:
            status = client.call("runtime.status")
            if status.get("worker_start_stage") == "restore":
                break
            assert time.monotonic() < deadline, status
            time.sleep(0.01)
        with pytest.raises(SourceError) as error:
            client.call(
                "namespace.list", worker_generation=status["interactive"]["worker_generation"]
            )
        assert error.value.code == "busy"
        if status.get("implementation") == "rust":
            assert (
                status["interactive"]["worker_generation"]
                == before["interactive"]["worker_generation"]
            )
        assert client.wait(accepted["operation_id"])["state"] == "completed"
        after = client.call("runtime.status")
        assert (
            after["interactive"]["worker_generation"] != before["interactive"]["worker_generation"]
        )
        assert not client.call(
            "namespace.list", worker_generation=after["interactive"]["worker_generation"]
        ).get("error")
    finally:
        client.close()


def test_numpy_preview_is_bounded_and_does_not_call_subclass_hooks():
    numpy = pytest.importorskip("numpy")
    from kernelledger.worker import passive_page

    value = numpy.arange(1000000)
    assert passive_preview(value)["shape"] == [1000000]
    page = passive_page(value, 999995, 5)
    assert len(page["items"]) == 5 and page["items"][-1]["preview"] == "999999"

    class Hostile(numpy.ndarray):
        def __getitem__(self, key):
            raise AssertionError("subclass indexing called")

    assert passive_page(value.view(Hostile))["unsupported"]
    assert passive_page(numpy.array([object()], dtype=object))["unsupported"]


@pytest.mark.parametrize(
    "source,expected",
    [
        ("42;", True),
        ("42; # comment", True),
        ("';'", False),
        ("42 # ;", False),
        ('""";\n;"""', False),
        ("x = 1; x\n", False),
    ],
)
def test_suppression_tokens(source, expected):
    assert suppress_result(source) is expected


def test_execution_conveniences_and_original_source(notebook):
    ensure_supervisor(notebook.root)
    for source, text in [
        ("values = []\nvalues.append(1);", None),
        ("%time values.append(2)", "Wall time:"),
        ("%%time\ndef square(x):\n    return x*x\nsquare(7)", "Wall time:"),
        ("%timeit -n 2 -r 3 values.append(3)", "6 runs"),
        ("%prun square(9)", "Profile"),
        ("square?", "square(x)"),
        ("square??", "return x * x"),
    ]:
        events = successful(notebook, source)
        if text:
            assert any(text in str(event) for event in events), events
        elif text is None:
            assert not any(event["kind"] == "result" for event in events)
    successful(notebook, "assert values == [1, 2, 3, 3, 3, 3, 3, 3]")
    checkpoint = rpc(notebook.root, {"command": "checkpoint"})
    assert checkpoint["status"] == "complete", checkpoint
    rpc(notebook.root, {"command": "restore", "checkpoint_id": checkpoint["checkpoint_id"]})
    successful(notebook, "assert square(8) == 64")


def test_timing_errors_capture_and_exact_writefile(notebook):
    ensure_supervisor(notebook.root)
    result, events = execute(notebook, "%%time\nraise ValueError('timed failure')")
    assert result["status"] == "error"
    assert any("Wall time:" in str(event) for event in events)
    successful(notebook, "%%capture captured\nprint('hello')\n42")
    successful(notebook, "assert 'hello' in captured['text'] and '42' in captured['text']")
    successful(notebook, "%%capture huge\nprint('x' * 100000)")
    successful(notebook, "assert huge['truncated'] and len(huge['text'].encode()) <= 65536")
    successful(notebook, "%%writefile 'example file.txt'\n\nfirst\nsecond\n")
    assert (notebook.root / "example file.txt").read_bytes() == b"\nfirst\nsecond\n"
    successful(notebook, "%commands")


def test_history_collision_alias_and_checkpoint(notebook):
    ensure_supervisor(notebook.root)
    successful(notebook, "_ = 'user'")
    result, _ = execute(notebook, "%history on")
    assert result["status"] == "error"
    successful(notebook, "del _\n%history on")
    successful(notebook, "data = [1, 2]\ndata")
    successful(notebook, "assert _ is data\nsaved = _\n_ = 'explicit user value'")
    successful(notebook, "99")
    successful(notebook, "assert _ == 'explicit user value'\n%history clear")
    checkpoint = rpc(notebook.root, {"command": "checkpoint"})
    assert checkpoint["status"] == "complete", checkpoint
    rpc(notebook.root, {"command": "restore", "checkpoint_id": checkpoint["checkpoint_id"]})
    successful(notebook, "assert saved is data\nassert _ == 'explicit user value'")


def test_async_future_flags_and_cleanup(notebook):
    ensure_supervisor(notebook.root)
    successful(notebook, "from __future__ import annotations\nimport asyncio")
    events = successful(notebook, "await asyncio.sleep(0)\nawait asyncio.sleep(0, result=42)")
    assert any(
        event["kind"] == "result" and event["bundle"]["text/plain"]["value"] == "42"
        for event in events
    )
    successful(
        notebook,
        "def future(x: Missing):\n    return x\nassert future.__annotations__['x'] == 'Missing'",
    )
    successful(
        notebook, "async def launch():\n    asyncio.create_task(asyncio.sleep(100))\nawait launch()"
    )
    checkpoint = rpc(notebook.root, {"command": "checkpoint"})
    assert checkpoint.get("refused") != "background_tasks", checkpoint


def test_cancel_resistant_tasks_block_new_execution_and_snapshots(notebook):
    ensure_supervisor(notebook.root)
    source = "import asyncio\nasync def stubborn():\n    while True:\n        try:\n            await asyncio.sleep(100)\n        except asyncio.CancelledError:\n            pass\nasync def launch():\n    asyncio.create_task(stubborn())\n    await asyncio.sleep(0)\nawait launch()"
    successful(notebook, source)
    client = Client(notebook.root)
    try:
        status = client.call("runtime.status")
        assert status["background_tasks"] and status["checkpoint_eligible"] is False
        assert rpc(notebook.root, {"command": "checkpoint"})["refused"] == "background_tasks"
        result, events = execute(notebook, "should_not_execute = True")
        assert result["status"] == "error" and any(
            "Previous tasks refused" in str(event) for event in events
        )
    finally:
        client.close()


def test_passive_metadata_does_not_call_user_hooks():
    class Hostile:
        def __repr__(self):
            raise AssertionError("repr called")

        def __getattribute__(self, name):
            raise AssertionError("getattribute called")

        def __dir__(self):
            raise AssertionError("dir called")

        @property
        def unsafe(self):
            raise AssertionError("property called")

    value = Hostile()
    assert "Hostile" in passive_preview(value)["type"]
    assert isinstance(passive_attributes(value)["unsafe"], property)


def test_recorded_file_bytes_and_mime_metadata(notebook):
    import hashlib

    from kernelledger.blobs import BlobStore

    ensure_supervisor(notebook.root)
    raw = b"def loaded(value):\n    return value + 2\n"
    (notebook.root / "loaded.py").write_bytes(raw)
    events = successful(notebook, "%run loaded.py")
    digest = hashlib.sha256(raw).hexdigest()
    assert any(digest in str(event) for event in events)
    assert BlobStore(notebook.root / ".kl", readonly=True).get(digest) == raw
    events = successful(
        notebook,
        "class Rich:\n    def _repr_mimebundle_(self):\n        return ({'text/plain':'fallback','text/html':'<table></table>'}, {'text/html':{'isolated':True}})\ndisplay(Rich())",
    )
    event = next(event for event in events if event["kind"] == "display")
    assert event["metadata"]["text/html"]["isolated"] is True
    assert event["bundle"]["text/plain"]["value"] == "fallback"


def test_live_read_payloads_never_use_durable_envelopes():
    from kernelledger.worker import Channel

    class Socket:
        def __init__(self):
            self.frames = []

        def sendall(self, data):
            self.frames.append(data)

    socket = Socket()
    channel = Channel(socket)
    channel.send(
        "interactive_reply",
        request_id="test",
        worker_generation="worker",
        candidates=["private" * 10000] * 100,
    )
    assert len(socket.frames) == 1 and len(socket.frames[0]) < 65540
    assert b'"kind":"interactive_reply"' in socket.frames[0]
    assert b"private" not in socket.frames[0]


def test_live_reads_unicode_staleness_and_no_history(notebook):
    ensure_supervisor(notebook.root)
    successful(
        notebook,
        "αριθμός = 42\ncafé = 7\nitems = [1, 2, 3]\nkey_hashes = []\nclass Key:\n    def __hash__(self):\n        key_hashes.append(1)\n        return 1\nglobals()[Key()] = 0\nglobals()['x' * 1000000] = None\nkey_hashes.clear()",
    )
    client = Client(notebook.root)
    try:
        interactive = client.call("runtime.status")["interactive"]
        generation = interactive["worker_generation"]
        ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
        before = ledger.rows("SELECT COUNT(*) AS count FROM events")[0]["count"]
        source = "# 😀\nαρι"
        result = client.call(
            "code.complete",
            worker_generation=generation,
            source=source,
            source_revision="draft-1",
            cursor=len(source),
        )
        assert "αριθμός" in result["candidates"]
        assert source[result["start"] : result["end"]] == "αρι"
        assert result["source_revision"] == "draft-1"
        combining = "cafe\u0301"
        result = client.call(
            "code.complete",
            worker_generation=generation,
            source=combining,
            source_revision="draft-2",
            cursor=len(combining),
        )
        assert "café" in result["candidates"] and (result["start"], result["end"]) == (0, 5)
        names = client.call("namespace.list", worker_generation=generation)
        item = next(item for item in names["items"] if item["name"] == "items")
        preview = client.call(
            "namespace.preview", worker_generation=generation, handle=item["handle"]
        )
        assert preview["length"] == 3
        after = ledger.rows("SELECT COUNT(*) AS count FROM events")[0]["count"]
        ledger.close()
        assert before == after
        successful(notebook, "assert key_hashes == []\nitems.append(4)")
        stale = client.call(
            "namespace.preview", worker_generation=generation, handle=item["handle"]
        )
        assert stale["error"]["code"] == "stale_epoch"
        with pytest.raises(SourceError) as error:
            client.call("namespace.list", worker_generation="old")
        assert error.value.code == "stale_generation"
    finally:
        client.close()


def test_busy_reads_do_not_block_interrupt(notebook):
    ensure_supervisor(notebook.root)
    successful(notebook, "1")
    client = Client(notebook.root)
    try:
        generation = client.call("runtime.status")["interactive"]["worker_generation"]
        with concurrent.futures.ThreadPoolExecutor() as pool:
            future = pool.submit(execute, notebook, "import time\ntime.sleep(30)")
            deadline = time.monotonic() + 10
            while client.call("runtime.status")["worker_state"] != "running":
                assert time.monotonic() < deadline
                time.sleep(0.01)
            with pytest.raises(SourceError) as error:
                client.call("namespace.list", worker_generation=generation)
            assert error.value.code == "busy"
            client.call("interrupt")
            assert future.result(timeout=10)[0]["status"] == "interrupted"
    finally:
        client.close()
