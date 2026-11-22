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


def test_large_html_and_base64_image_mime_are_recorded_intact(notebook):
    import base64

    from kernelledger.blobs import BlobStore

    html = "<div>" + "x" * 100_000 + "</div>"
    png = b"\x89PNG\r\n\x1a\nimage-bytes-for-mime-contract"
    source = (
        "class Rich:\n"
        "    def _repr_mimebundle_(self):\n"
        f'        return {{"text/html": "<div>" + "x" * 100_000 + "</div>", "image/png": {base64.b64encode(png).decode()!r}}}\n'
        "display(Rich())"
    )
    ensure_supervisor(notebook.root)
    events = successful(notebook, source)
    event = next(event for event in events if event["kind"] == "display")
    blobs = BlobStore(notebook.root / ".kl", readonly=True)
    assert blobs.get(event["bundle"]["text/html"]["blob"]) == html.encode()
    assert event["bundle"]["image/png"]["encoding"] == "binary"
    assert blobs.get(event["bundle"]["image/png"]["blob"]) == png


def test_plotly_default_figure_has_offline_html_without_renderer_setup(notebook):
    pytest.importorskip("plotly")
    from kernelledger.blobs import BlobStore

    ensure_supervisor(notebook.root)
    events = successful(
        notebook,
        "import plotly.graph_objects as go\n"
        'fig = go.Figure(go.Bar(x=["Ada", "Bob"], y=[2, 5]))\n'
        "display(fig)",
    )
    event = next(event for event in events if event["kind"] == "display")
    slot = event["bundle"]["text/html"]
    html = BlobStore(notebook.root / ".kl", readonly=True).get(slot["blob"]).decode()
    assert len(html) > 65_536
    assert "Plotly.newPlot(" in html
    assert "cdn.plot.ly" not in html.split("Plotly.newPlot(")[-1]
    assert "[output truncated" not in html
    assert not any(event["kind"] == "stdout" for event in events)


def test_passive_containers_stdlib_dataclasses_and_generators():
    import collections
    import dataclasses
    import datetime as dt
    import decimal
    import enum
    import pathlib
    import re
    import types
    import uuid

    from kernelledger.worker import _resolve_segments, passive_page

    class Color(enum.Enum):
        RED = 1

    Point = collections.namedtuple("Point", ["x", "y"])

    @dataclasses.dataclass
    class Config:
        lr: float
        layers: list[int]

    class Hostile:
        def __repr__(self):
            raise AssertionError("repr called")

        def __getattribute__(self, name):
            raise AssertionError("getattribute called")

    # Nested list + tuples + hostile item stays strictly passive
    nested = [(1, "a"), (2, {"k": [10, 20]}), Hostile()]
    prev = passive_preview(nested, 256)
    assert prev["category"] == "collection"
    assert prev["size_label"] == "3 items"
    assert prev["expandable"] is True
    assert "(1, 'a')" in prev["preview"]
    assert "(2, {'k': <list, 2 items>})" in prev["preview"]
    assert "<Hostile>" in prev["preview"]

    # Circular reference does not recurse infinitely
    circ: list[object] = [1]
    circ.append(circ)
    assert "<circular list>" in passive_preview(circ)["preview"]

    # NamedTuple, deque, Counter, OrderedDict, set
    pt = Point(3, 4)
    assert "Point(x=3, y=4)" in passive_preview(pt)["preview"]
    pt_page = passive_page(pt, 0, 10)
    assert pt_page["mode"] == "sequence"
    assert pt_page["items"][0]["key"]["preview"] == "x"

    dq = collections.deque([1, 2, 3])
    assert passive_preview(dq)["size_label"] == "3 items"
    assert passive_page(dq, 1, 2)["items"][0]["preview"] == "2"

    cnt = collections.Counter({"a": 3, "b": 1})
    assert passive_preview(cnt)["category"] == "collection"
    assert passive_page(cnt, 0, 10)["mode"] == "mapping"

    # Stdlib scalars
    assert "2026-01-02" in passive_preview(dt.date(2026, 1, 2))["preview"]
    assert "3.14" in passive_preview(decimal.Decimal("3.14"))["preview"]
    assert "Color.RED" in passive_preview(Color.RED)["preview"]
    assert "hello" in passive_preview(pathlib.PurePosixPath("/tmp/hello"))["preview"]
    assert "re.compile" in passive_preview(re.compile(r"\d+"))["preview"]
    u = uuid.UUID("12345678-1234-5678-1234-567812345678")
    assert "12345678" in passive_preview(u)["preview"]
    ns = types.SimpleNamespace(alpha=1, beta="two")
    assert "alpha=1" in passive_preview(ns)["preview"]

    # Dataclass passive instance fields
    cfg = Config(lr=0.001, layers=[64, 128])
    cfg_prev = passive_preview(cfg)
    assert cfg_prev["expandable"] is True
    assert "lr=0.001" in cfg_prev["preview"]
    cfg_page = passive_page(cfg, 0, 10)
    assert cfg_page["mode"] == "attributes"
    assert {item["key"]["preview"] for item in cfg_page["items"]} == {"lr", "layers"}

    # Generator is NEVER consumed
    gen = (x * 10 for x in range(3))
    gen_prev = passive_preview(gen)
    assert "not consumed" in gen_prev["preview"]
    assert next(gen) == 0

    # Long string and bytes paging
    long_text = "line-" * 40
    assert passive_preview(long_text)["expandable"] is True
    tpage = passive_page(long_text, 0, 50)
    assert tpage["mode"] == "text" and tpage["total"] == len(long_text)

    raw = bytes(range(256)) * 2
    bpage = passive_page(raw, 0, 16)
    assert bpage["mode"] == "bytes" and bpage["next_offset"] == 256

    # Structured segment drill-down
    target, crumbs = _resolve_segments(
        nested,
        "nested",
        [
            {"kind": "index", "value": 1},
            {"kind": "index", "value": 1},
            {"kind": "key", "value": "k"},
        ],
    )
    assert target == [10, 20]
    assert [c["label"] for c in crumbs] == ["nested", "[1]", "[1]", "['k']"]


def test_passive_numpy_matrix_and_pandas_dataframe():
    numpy = pytest.importorskip("numpy")
    pandas = pytest.importorskip("pandas")
    from kernelledger.worker import passive_page

    # NumPy 2D & 3D slicing
    mat = numpy.arange(12, dtype=numpy.int64).reshape(3, 4)
    mprev = passive_preview(mat)
    assert mprev["category"] == "array"
    assert mprev["shape"] == [3, 4]
    assert "int64 (3, 4)" in mprev["preview"]
    mpage = passive_page(mat, 1, 2, col_offset=1, col_limit=2)
    assert mpage["mode"] == "matrix"
    assert mpage["table"]["rows"][0][0]["text"] == "5"
    assert mpage["table"]["next_col_offset"] == 3

    cube = numpy.arange(24).reshape(2, 3, 4)
    cpage = passive_page(cube, 0, 3, slice_prefix=[1])
    assert cpage["mode"] == "matrix"
    assert cpage["lead_dims"] == [2]
    assert cpage["table"]["rows"][0][0]["text"] == "12"

    # pandas DataFrame + Schema + on-demand stats
    df = pandas.DataFrame(
        {
            "city": ["Paris", "Tokyo", None, "Lima"],
            "temp": [18.5, 22.0, float("nan"), 19.5],
            "rank": [1, 2, 3, 4],
        }
    )
    dprev = passive_preview(df)
    assert dprev["category"] == "tabular"
    assert dprev["library"] == "pandas"
    assert dprev["shape"] == [4, 3]
    assert dprev["columns_preview"] == ["city", "temp", "rank"]

    dpage = passive_page(df, 0, 2, col_offset=0, col_limit=2, include_stats=False)
    assert dpage["mode"] == "table"
    assert len(dpage["table"]["rows"]) == 2
    assert len(dpage["table"]["columns"]) == 2
    assert dpage["table"]["next_row_offset"] == 2
    assert dpage["table"]["next_col_offset"] == 2
    assert dpage["schema"][0]["null_count"] == 1
    assert "mean" not in dpage["schema"][1]

    dpage_stats = passive_page(df, 0, 4, col_offset=0, col_limit=3, include_stats=True)
    assert dpage_stats["table"]["rows"][2][0]["null"] is True
    assert dpage_stats["table"]["rows"][2][1]["null"] is True
    assert dpage_stats["schema"][1]["mean"] == "20"

    # pandas Series
    sprev = passive_preview(df["temp"])
    assert sprev["category"] == "tabular" and sprev["length"] == 4
    spage = passive_page(df["temp"], 0, 2)
    assert spage["mode"] == "series" and len(spage["items"]) == 2


def test_passive_lazyframe_and_gpu_tensor_never_collect_or_copy(monkeypatch):
    import sys
    import types

    from kernelledger.worker import passive_page

    # Mock polars.LazyFrame
    polars_mod = types.ModuleType("polars")

    class FakeLazyFrame:
        def collect_schema(self):
            return {"a": "Int64", "b": "Utf8"}

        def collect(self):
            raise AssertionError("LazyFrame.collect() must never be called passively")

    polars_mod.LazyFrame = FakeLazyFrame  # type: ignore[attr-defined]
    monkeypatch.setitem(sys.modules, "polars", polars_mod)

    lf = FakeLazyFrame()
    lprev = passive_preview(lf)
    assert lprev["category"] == "tabular"
    assert lprev["lazy"] is True
    assert lprev["columns_preview"] == ["a", "b"]
    lpage = passive_page(lf, 0, 10)
    assert lpage["mode"] == "table" and lpage["lazy"] is True
    assert len(lpage["schema"]) == 2

    # Mock torch.Tensor on cuda:0
    torch_mod = types.ModuleType("torch")

    class FakeDevice:
        def __init__(self, dev_type: str, text: str):
            self.type = dev_type
            self._text = text

        def __str__(self):
            return self._text

    class FakeTensor:
        def __init__(self, on_gpu: bool = True):
            self.shape = (2, 3)
            self.dtype = "torch.float32"
            self.device = FakeDevice("cuda" if on_gpu else "cpu", "cuda:0" if on_gpu else "cpu")
            self.requires_grad = True
            self.copied = False

        def element_size(self):
            return 4

        def numel(self):
            return 6

        def detach(self):
            return self

        def __getitem__(self, idx):
            return self

        def cpu(self):
            self.copied = True
            return self

        def tolist(self):
            return [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]

    torch_mod.Tensor = FakeTensor  # type: ignore[attr-defined]
    monkeypatch.setitem(sys.modules, "torch", torch_mod)

    t = FakeTensor(on_gpu=True)
    tprev = passive_preview(t)
    assert tprev["category"] == "array"
    assert tprev["on_device"] is True
    assert t.copied is False

    # Default page refuses GPU copy
    tpage = passive_page(t, 0, 10)
    assert tpage["mode"] == "device_tensor"
    assert t.copied is False

    # Explicit opt-in copies bounded slice
    tpage_optin = passive_page(t, 0, 10, allow_device_copy=True)
    assert tpage_optin["mode"] == "matrix"
    assert t.copied is True
    assert tpage_optin["table"]["rows"][0][0]["text"] == "1.0"


def test_live_namespace_preview_drilldown_and_contracts(notebook):
    from kernelledger.desktop import contracts

    contracts.validate(
        {
            "method": "namespace.preview",
            "worker_generation": "gen",
            "handle": "1:rows",
            "segments": [{"kind": "index", "value": 0}, {"kind": "key", "value": "score"}],
            "col_offset": 0,
            "col_limit": 12,
            "include_stats": True,
            "allow_device_copy": False,
            "slice_prefix": [0, 1],
        }
    )
    with pytest.raises(SourceError):
        contracts.validate(
            {
                "method": "namespace.preview",
                "worker_generation": "gen",
                "handle": "1:rows",
                "segments": [{"kind": "eval", "value": "1+1"}],
            }
        )

    ensure_supervisor(notebook.root)
    successful(notebook, "rows = [('alpha', {'score': 9.5}), ('beta', {'score': 7.2})]")
    client = Client(notebook.root)
    try:
        gen = client.call("runtime.status")["interactive"]["worker_generation"]
        listing = client.call("namespace.list", worker_generation=gen)
        row_item = next(item for item in listing["items"] if item["name"] == "rows")
        assert "('alpha', {'score': 9.5})" in row_item["preview"]
        assert row_item["size_label"] == "2 items"

        drilled = client.call(
            "namespace.preview",
            worker_generation=gen,
            handle=row_item["handle"],
            segments=[
                {"kind": "index", "value": 0},
                {"kind": "index", "value": 1},
                {"kind": "key", "value": "score"},
            ],
        )
        assert drilled["preview"] == "9.5"
        assert [c["label"] for c in drilled["breadcrumbs"]] == [
            "rows",
            "[0]",
            "[1]",
            "['score']",
        ]
    finally:
        client.close()
