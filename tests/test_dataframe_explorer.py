"""DataFrame explorer: positional columns, paging, snippet fidelity, stats, passivity."""

from __future__ import annotations

import sys
import time
import types
from typing import Any

import pytest
from _worker_helpers import interactive, new_worker, run, run_snippet

from kernelledger.desktop.contracts import validate
from kernelledger.source import SourceError
from kernelledger.worker import passive_page, passive_preview

pd = pytest.importorskip("pandas")
np = pytest.importorskip("numpy")


def _texts(page: dict[str, Any]) -> list[list[str]]:
    return [[cell["text"] for cell in row] for row in page["table"]["rows"]]


def _live_rows(frame: Any, lib: str, **kwargs: Any) -> tuple[dict[str, Any], list[list[str]]]:
    page = passive_page(frame, 0, 50, col_limit=40, **kwargs)
    total = page["table"]["total_rows"]
    rows = _texts(page)
    offset = 50
    while offset < total:
        rows += _texts(passive_page(frame, offset, 50, col_limit=40, **kwargs))
        offset += 50
    return page, rows


def _snippet_rows(page: dict[str, Any], frame: Any) -> list[list[str]]:
    """Execute the generated snippet, then render it through the same passive formatter."""
    result = run_snippet(page["snippet"], {"df": frame})
    return _live_rows(result, page["library"])[1]


CASES = [
    {"filter_query": "a>1"},
    {"filter_query": "a>=2", "sort_col": "b", "sort_desc": True},
    {"filter_query": "b:X"},  # literal (regex=False) and case-insensitive
    {"filter_query": "b:a.c"},  # `.` must not act as a regex wildcard
    {"filter_query": "a>nan"},
    {"filter_query": "a<inf"},
    {"filter_query": "a!=-inf"},
    {"filter_query": "x"},
    {"sort_col": "a"},
    {"sort_col_index": 2, "sort_desc": True},  # duplicate label -> positional
    {"filter_query": "b=='abc'"},
]


def _frame() -> Any:
    frame = pd.DataFrame(
        [
            [3.0, "Xa", 1],
            [1.0, "abc", 9],
            [np.nan, "a.c", 4],
            [2.0, "axc", 4],
            [np.inf, None, 7],
            [-np.inf, "x", 0],
        ],
        columns=["a", "b", "b"],
    )
    return frame


@pytest.mark.parametrize("case", CASES)
def test_pandas_snippet_reproduces_the_live_view(case: dict[str, Any]) -> None:
    frame = _frame()
    page, live = _live_rows(frame, "pandas", **case)
    assert page["table"]["filter_error"] is None and page["table"]["sort_error"] is None
    assert page["snippet"] is not None
    assert _snippet_rows(page, frame) == live
    if case.get("filter_query", "").startswith("b:"):
        assert "regex=False" in page["snippet"] and "case=False" in page["snippet"]
    if "nan" in case.get("filter_query", "") or "inf" in case.get("filter_query", ""):
        assert "float(" in page["snippet"]


@pytest.mark.parametrize("lib", ["polars", "pyarrow"])
@pytest.mark.parametrize(
    "case",
    [
        {"filter_query": "a>1"},
        {"filter_query": "b:X", "sort_col": "a", "sort_desc": True},
        {"filter_query": "x"},
        {"sort_col": "b"},
        {"filter_query": "a<inf"},
    ],
)
def test_polars_and_pyarrow_snippets_reproduce_the_live_view(
    lib: str, case: dict[str, Any]
) -> None:
    module = pytest.importorskip(lib)
    data = {"a": [3.0, 1.0, None, 2.0, float("inf")], "b": ["Xa", "abc", "a.c", None, "x"]}
    frame = module.DataFrame(data) if lib == "polars" else module.table(data)
    page, live = _live_rows(frame, lib, **case)
    assert page["table"]["filter_error"] is None, page["table"]["filter_error"]
    assert page["table"]["filter_applied"] is bool(case.get("filter_query"))
    assert page["snippet"] is not None
    assert _snippet_rows(page, frame) == live


def test_columns_are_identified_by_position() -> None:
    frame = pd.DataFrame({0: [3, 1, 2], "x" * 300: [1, 2, 3]})
    page = passive_page(frame, 0, 5, sort_col="0", filter_query="0>1")
    table = page["table"]
    assert table["sort_col_index"] == 0 and table["sort_applied"] is True
    assert _texts(page) == [["2", "3"], ["3", "1"]]
    assert page["snippet"] == "df[df[0] > 1].sort_values(0, ascending=True)"
    long_col = table["columns"][1]
    assert long_col["index"] == 1 and len(long_col["name"]) <= 128

    dup = pd.DataFrame([[1, 5], [2, 4]], columns=["v", "v"])
    page = passive_page(dup, 0, 5, sort_col_index=1)
    assert _texts(page) == [["2", "4"], ["1", "5"]]
    assert ".iloc[:, 1]" in page["snippet"]
    assert [c["index"] for c in page["table"]["columns"]] == [0, 1]

    multi = pd.DataFrame(
        [[1, 2], [3, 0]], columns=pd.MultiIndex.from_tuples([("g", "a"), ("g", "b")])
    )
    page = passive_page(multi, 0, 5, sort_col_index=1)
    assert [c["name"] for c in page["table"]["columns"]] == ["g / a", "g / b"]
    assert _texts(page) == [["3", "0"], ["1", "2"]]
    assert _snippet_rows(page, multi) == _texts(page)
    # Legacy clients may still send the display label.
    assert passive_page(multi, 0, 5, sort_col="g / b")["table"]["sort_col_index"] == 1


def test_column_paging_reaches_every_column() -> None:
    wide = pd.DataFrame({f"c{i}": [i] for i in range(600)})
    seen: list[int] = []
    offset: int | None = 0
    pages = 0
    while offset is not None:
        table = passive_page(wide, 0, 5, col_offset=offset, col_limit=40)["table"]
        seen += [c["index"] for c in table["columns"]]
        offset = table["next_col_offset"]
        pages += 1
        assert pages < 100
    assert seen == list(range(600))
    last = passive_page(wide, 0, 5, col_offset=590, col_limit=16)["table"]
    assert len(last["columns"]) == 10 and last["next_col_offset"] is None


def test_large_sort_is_applied_and_free_text_is_vectorized() -> None:
    big = pd.DataFrame({"v": np.arange(1_000_000, 0, -1)})
    page = passive_page(big, 0, 3, sort_col="v")
    assert page["table"]["sort_applied"] is True
    assert _texts(page) == [["1"], ["2"], ["3"]]

    mid = pd.DataFrame({f"c{i}": range(50_000) for i in range(32)})
    started = time.perf_counter()
    page = passive_page(mid, 0, 5, filter_query="49999")
    assert time.perf_counter() - started < 3
    assert page["table"]["total_rows"] == 1 and page["table"]["filter_capped"] is False

    tall = pd.DataFrame({"s": ["needle" if i % 2 else "hay" for i in range(3_000_000)]})
    page = passive_page(tall, 0, 3, filter_query="needle")
    table = page["table"]
    assert table["filter_capped"] is True
    assert table["filter_scanned_rows"] < 3_000_000
    assert f".iloc[:{table['filter_scanned_rows']}]" in page["snippet"]


def test_filter_errors_are_reported_without_a_snippet() -> None:
    frame = pd.DataFrame({"a": [1, 2], "s": ["x", 1]})
    page = passive_page(frame, 0, 5, filter_query="a>abc")
    assert page["table"]["filter_error"] and page["table"]["filter_applied"] is False
    assert page["snippet"] is None
    assert page["table"]["total_rows"] == 2  # unfiltered rows are still shown
    page = passive_page(frame, 0, 5, sort_col="s")
    assert page["table"]["sort_applied"] is False and page["table"]["sort_error"]
    assert page["snippet"] is None and page["table"]["sort_col"] is None


def test_histograms_handle_extremes_and_stay_consistent() -> None:
    frame = pd.DataFrame(
        {
            "v": [-1e308, 1e308, np.inf, -np.inf, np.nan, 0.0],
            "t": pd.to_datetime(["2020-01-01", None, "2021-01-01", "2020-06-01", None, None]),
            "c": pd.Categorical(["a", "b", "a", None, "a", "b"]),
            "flag": [True, False, True, True, False, True],
        }
    )
    page = passive_page(frame, 0, 5, include_stats=True)
    by_name = {c["name"]: c for c in page["schema"]}
    v = by_name["v"]
    assert sum(v["histogram"]) == 3 and v["inf_count"] == 2
    assert v["histogram_range"] == [-1e308, 1e308]
    assert v["min"] == "-1e+308" and v["max"] == "1e+308"
    assert v["histogram_sampled"] is False
    assert sum(by_name["t"]["histogram"]) == 3 and by_name["t"]["min"].startswith("2020-01-01")
    assert by_name["c"]["top_values"][0] == {"value": "a", "count": 3}
    assert by_name["flag"]["top_values"][0] == {"value": "True", "count": 4}

    # Bins and min/max come from the same rows; sampling is flagged.
    many = pd.DataFrame({"x": np.concatenate([np.zeros(1_000_000), np.full(10, 1e9)])})
    stats = passive_page(many, 0, 1, include_stats=True)["schema"][0]
    assert stats["histogram_sampled"] is True and stats["stats_sample_rows"] == 1_000_000
    assert stats["max"] == "0.0" and sum(stats["histogram"]) == 1_000_000

    empty = passive_page(pd.DataFrame({"x": pd.Series([], dtype=float)}), 0, 5, include_stats=True)
    assert "histogram" not in empty["schema"][0] and empty["table"]["total_rows"] == 0


class Spy:
    """Every dunder a careless explorer might call counts as running user code."""

    calls = 0

    def _hit(self, *_args: Any) -> Any:
        Spy.calls += 1
        return NotImplemented

    def __str__(self) -> str:
        Spy.calls += 1
        return "spy"

    def __repr__(self) -> str:
        Spy.calls += 1
        return "spy"

    def __format__(self, spec: str) -> str:
        Spy.calls += 1
        return "spy"

    def __hash__(self) -> int:
        Spy.calls += 1
        return 1

    def __bool__(self) -> bool:
        Spy.calls += 1
        return True

    __eq__ = __ne__ = __lt__ = __le__ = __gt__ = __ge__ = _hit  # type: ignore[assignment]


def test_explorer_never_calls_user_dunders() -> None:
    worker, channel = new_worker()
    frame = pd.DataFrame({"o": [Spy(), Spy(), None], "n": [1, 2, 3]})
    worker.namespace.update(objs=frame, lone=Spy(), items=[Spy()], series=frame["o"])
    Spy.calls = 0
    listed = interactive(worker, channel, "namespace.list")
    assert "error" not in listed
    handle = next(i["handle"] for i in listed["items"] if i["name"] == "objs")
    for fields in (
        {},
        {"include_stats": True},
        {"filter_query": "spy"},
        {"filter_query": "o:spy"},
        {"filter_query": "o==1"},
        {"sort_col": "o"},
        {"filter_query": "n>1", "sort_col_index": 0},
    ):
        reply = interactive(worker, channel, "namespace.preview", handle=handle, **fields)
        assert "error" not in reply, reply.get("error")
    for name in ("lone", "items", "series"):
        reply = interactive(worker, channel, "namespace.preview", handle=f"{handle[0]}:{name}")
        assert "error" not in reply
    assert Spy.calls == 0

    page = passive_page(frame, 0, 5, filter_query="o:spy")
    assert page["table"]["filter_error"] and "Python objects" in page["table"]["filter_error"]
    page = passive_page(frame, 0, 5, filter_query="spy")
    assert page["table"]["filter_skipped_columns"] == [0]
    stats = passive_page(frame, 0, 5, include_stats=True)["schema"][0]
    assert "unique_count" not in stats
    assert Spy.calls == 0


def test_lazy_schema_is_resolved_only_on_request(monkeypatch: pytest.MonkeyPatch) -> None:
    polars_mod = types.ModuleType("polars")

    class LazyFrame:
        resolved = 0

        def collect_schema(self) -> dict[str, str]:
            LazyFrame.resolved += 1
            return {"a": "Int64"}

    polars_mod.LazyFrame = LazyFrame  # type: ignore[attr-defined]
    monkeypatch.setitem(sys.modules, "polars", polars_mod)
    worker, channel = new_worker()
    worker.namespace["lf"] = LazyFrame()
    listed = interactive(worker, channel, "namespace.list")
    item = next(i for i in listed["items"] if i["name"] == "lf")
    assert item["lazy"] is True and item["schema_collected"] is False
    preview = interactive(worker, channel, "namespace.preview", handle=item["handle"])
    assert preview["page"]["schema"] == [] and LazyFrame.resolved == 0
    preview = interactive(
        worker, channel, "namespace.preview", handle=item["handle"], lazy_schema=True
    )
    assert [c["name"] for c in preview["page"]["schema"]] == ["a"] and LazyFrame.resolved == 1


def test_tensor_preview_reads_elements_without_flattening(monkeypatch: pytest.MonkeyPatch) -> None:
    torch_mod = types.ModuleType("torch")

    class Tensor:
        flattened = 0

        def __init__(self, data: Any) -> None:
            self.data = data
            self.shape = data.shape
            self.dtype = "torch.float32"
            self.device = "cpu"
            self.requires_grad = False

        def numel(self) -> int:
            return int(self.data.size)

        def element_size(self) -> int:
            return 4

        def detach(self) -> Any:
            return self

        def reshape(self, *_shape: Any) -> Any:
            Tensor.flattened += 1
            return Tensor(np.ascontiguousarray(self.data).reshape(-1))

        flatten = reshape

        def __getitem__(self, index: Any) -> Any:
            return Tensor(np.asarray(self.data[index]))

        def item(self) -> Any:
            return self.data.item()

        def cpu(self) -> Any:
            return self

        def tolist(self) -> Any:
            return self.data.tolist()

    torch_mod.Tensor = Tensor  # type: ignore[attr-defined]
    monkeypatch.setitem(sys.modules, "torch", torch_mod)
    # A transposed (non-contiguous) view: flattening it would copy all storage.
    tensor = Tensor(np.arange(24, dtype=np.float32).reshape(4, 6).T)
    preview = passive_preview(tensor)
    assert "[0.0, 6.0, 12.0, …, 23.0]" in preview["preview"]
    vector = Tensor(np.arange(10, dtype=np.float32)[::2])
    page = passive_page(vector, 1, 2)
    assert [item["preview"] for item in page["items"]] == ["2.0", "4.0"]
    assert Tensor.flattened == 0


def test_preview_request_fields_are_validated() -> None:
    import json
    from pathlib import Path

    jsonschema = pytest.importorskip("jsonschema")
    frame = {
        "method": "namespace.preview",
        "worker_generation": "g",
        "handle": "1:df",
        "sort_col_index": 3,
        "sort_col": "a",
        "sort_desc": True,
        "lazy_schema": True,
        "include_stats": True,
        "col_offset": 40,
        "col_limit": 16,
        "filter_query": "a>1",
        "segments": [{"kind": "key", "value": "k"}],
        "slice_prefix": [0, 1],
    }
    validate(frame)
    schema = json.loads(
        (Path(__file__).resolve().parents[1] / "schemas/desktop-protocol.schema.json").read_text()
    )
    jsonschema.validate(frame, schema)
    with pytest.raises(jsonschema.ValidationError):
        jsonschema.validate({**frame, "sort_col_index": -1}, schema)
    for bad in ({"sort_col_index": -1}, {"sort_col_index": "1"}, {"lazy_schema": "yes"}):
        with pytest.raises(SourceError):
            validate(
                {"method": "namespace.preview", "worker_generation": "g", "handle": "h", **bad}
            )


def test_live_preview_round_trip_through_the_worker() -> None:
    worker, channel = new_worker()
    run(
        worker,
        channel,
        "import pandas as pd\ndf = pd.DataFrame({0: [3, 1, 2], 'b': ['x', 'y', 'z']})\n",
    )
    listed = interactive(worker, channel, "namespace.list")
    handle = next(i["handle"] for i in listed["items"] if i["name"] == "df")
    reply = interactive(
        worker, channel, "namespace.preview", handle=handle, sort_col_index=0, sort_desc=True
    )
    page = reply["page"]
    assert page["table"]["sort_col_index"] == 0 and page["table"]["sort_col"] == "0"
    assert [r[1]["text"] for r in page["table"]["rows"]] == ["x", "z", "y"]
    assert page["snippet"] == "df.sort_values(0, ascending=False)"
