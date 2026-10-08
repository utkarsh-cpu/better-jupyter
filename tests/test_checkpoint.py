import ast
import datetime as dt
import decimal
import fractions
import io
import math
import pathlib
import uuid
import zoneinfo

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from kernelledger.worker import ExecutionIndex, GraphEncoder, GraphLoader, json_bytes


def save(namespace, source=None, **limits):
    index = ExecutionIndex()
    if source:
        namespace["__name__"] = "__main__"
        index.add("cells/test.py@1", source, 1)
        exec(compile(source, "cells/test.py@1", "exec"), namespace)
    encoder = GraphEncoder(index, **limits)
    manifest = encoder.checkpoint(namespace)
    return manifest, encoder.blobs


def restore(manifest, blobs):
    namespace = {"__name__": "__main__"}
    report = GraphLoader(manifest, blobs, namespace).load()
    return namespace, report


def test_alias_cycles_and_hashable_keys():
    a = []
    a.append(a)
    manifest, blobs = save({"a": a, "b": a, "keys": {(1, 2): {("x", 3)}, 4: "v"}})
    restored, report = restore(manifest, blobs)
    assert not report["excluded"]
    assert restored["a"] is restored["b"] is restored["a"][0]
    assert restored["keys"] == {(1, 2): {("x", 3)}, 4: "v"}


def test_immutable_cycle_is_excluded():
    a = []
    t = (a,)
    a.append(t)
    manifest, _ = save({"t": t})
    assert manifest["excluded"][0]["reason"] == "unsupported_cycle"


def test_values():
    values = [
        2**200,
        -0.0,
        float("nan"),
        float("inf"),
        complex(2, 3),
        decimal.Decimal("1.00000000000000001"),
        fractions.Fraction(2, 7),
        dt.datetime(2020, 1, 1),
        dt.time(12, 30),
        dt.date(2000, 2, 29),
        dt.timedelta(days=-2, microseconds=5),
        dt.datetime(2020, 1, 1, tzinfo=dt.timezone(dt.timedelta(hours=5, minutes=30))),
        dt.datetime(2020, 11, 1, 1, 30, tzinfo=zoneinfo.ZoneInfo("America/New_York"), fold=1),
        uuid.uuid4(),
        range(1, 12, 2),
        pathlib.PureWindowsPath("C:/test"),
        b"x" * 5000,
        bytearray(b"hi"),
        "x" * 70000,
    ]
    manifest, blobs = save({"values": values})
    result, report = restore(manifest, blobs)
    assert not report["excluded"]
    for a, b in zip(values, result["values"]):
        if isinstance(a, float) and math.isnan(a):
            assert math.isnan(b)
        else:
            assert a == b and type(a) is type(b)
    assert math.copysign(1, result["values"][1]) == -1
    assert result["values"][7].tzinfo is None
    assert result["values"][12].fold == 1
    json_bytes(manifest)


def test_functions_classes_closure_decorator_lambda():
    source = """
from dataclasses import dataclass
g = 7
def reads_global():
    return g
def outer(x):
    def inner(y=2):
        return x + y + g
    return inner
closure = outer(4)
def decorate(fn):
    def wrapper(x):
        return fn(x) + 1
    return wrapper
@decorate
def decorated(x):
    return x * 2
lam = lambda x: x + g
@dataclass
class Point:
    x: int
p = Point(3)
"""
    manifest, blobs = save({}, source)
    result, report = restore(manifest, blobs)
    assert [e for e in report["excluded"] if e["name"] != "__builtins__"] == []
    result["g"] = 10
    assert result["reads_global"]() == 10
    assert result["closure"]() == 16
    assert result["decorated"](3) == 7
    assert result["lam"](2) == 12
    assert result["p"].x == 3
    assert result["p"].__class__ is result["Point"]


def test_resource_shared_exclusions_and_limits():
    handle = io.StringIO()
    manifest, _ = save({"a": [handle], "b": handle, "large": list(range(200001)), "ok": 5})
    reasons = {item["name"]: item["reason"] for item in manifest["excluded"]}
    assert reasons == {"a": "resource", "b": "resource", "large": "limit"}
    assert "ok" in manifest["bindings"]


def test_payload_limit():
    with pytest.raises(OverflowError):
        save({"data": b"x" * 5000}, payload_limit=100)


def test_numpy():
    np = pytest.importorskip("numpy")
    a = np.arange(40).reshape(5, 8)
    rng = np.random.default_rng(42)
    manifest, blobs = save(
        {"a": a, "alias": a, "view": a[:, ::2], "rng": rng, "bad": np.array([{}], dtype=object)}
    )
    result, report = restore(manifest, blobs)
    assert result["a"] is result["alias"]
    assert np.array_equal(result["a"], a)
    assert np.array_equal(result["view"], a[:, ::2])
    assert result["rng"].random() == rng.random()
    assert any(e["name"] == "bad" for e in report["excluded"])


def test_tables():
    pd = pytest.importorskip("pandas")
    pytest.importorskip("pyarrow")
    frame = pd.DataFrame(
        {
            "text": ["a", "b"],
            "category": pd.Categorical(["a", "b"]),
            "nullable": pd.array([1, None], dtype="Int64"),
            "date": pd.date_range("2020-01-01", periods=2, tz="Asia/Kolkata"),
        }
    )
    frame.index.name = "named"
    manifest, blobs = save({"frame": frame, "series": frame["nullable"]})
    result, report = restore(manifest, blobs)
    assert not report["excluded"]
    assert frame.equals(result["frame"])
    assert frame["nullable"].equals(result["series"])


values = st.recursive(
    st.none() | st.booleans() | st.integers() | st.text() | st.binary(),
    lambda child: (
        st.lists(child, max_size=6)
        | st.tuples(child, child)
        | st.dictionaries(st.text(), child, max_size=6)
    ),
    max_leaves=25,
)


@given(values)
@settings(max_examples=80)
def test_generated_graphs(value):
    root = [value]
    manifest, blobs = save({"a": root, "b": root})
    result, report = restore(manifest, blobs)
    assert not report["excluded"]
    assert result["a"] == [value]
    assert result["a"] is result["b"]


def test_worker_has_no_package_imports():
    path = pathlib.Path(__file__).parents[1] / "src/kernelledger/worker.py"
    for node in ast.walk(ast.parse(path.read_text())):
        if isinstance(node, ast.ImportFrom):
            assert node.level == 0
            assert "kernelledger" not in (node.module or "")
        elif isinstance(node, ast.Import):
            assert all("kernelledger" not in item.name for item in node.names)


def test_recursive_closure_and_lambda_defaults_are_not_reexecuted():
    source = """
counter = []
def default_value():
    counter.append(1)
    return 7
lam = lambda x=default_value(): x
def outer():
    def recurse(n):
        return n if n == 0 else recurse(n-1)
    return recurse
fn = outer()
"""
    manifest, blobs = save({}, source)
    result, report = restore(manifest, blobs)
    assert not report["excluded"]
    assert result["lam"]() == 7
    assert result["counter"] == [1]
    assert result["fn"](3) == 0


@pytest.mark.parametrize(
    "change",
    [
        lambda m: m.update(schema=2),
        lambda m: m["bindings"].update(bad={"k": "ref", "id": "missing"}),
        lambda m: m["bindings"].update(bad=5),
    ],
)
def test_checkpoint_validation_before_loading(change):
    manifest, blobs = save({"x": 1})
    change(manifest)
    with pytest.raises(ValueError):
        restore(manifest, blobs)
