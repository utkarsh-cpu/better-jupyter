"""`@kl.cache` / `@kl.persistent_cache` keys, fallbacks and thread safety."""

from __future__ import annotations

import threading
import warnings
from pathlib import Path
from typing import Any

import pytest

from kernelledger.cache import cache, persistent_cache


def test_dataframe_and_array_arguments_are_hashed_by_content() -> None:
    pd = pytest.importorskip("pandas")
    np = pytest.importorskip("numpy")
    calls: list[int] = []

    @cache
    def rows(frame: Any) -> int:
        calls.append(1)
        return len(frame)

    assert rows(pd.DataFrame({"a": [1, 2]})) == 2
    assert rows(pd.DataFrame({"a": [1, 2]})) == 2
    assert len(calls) == 1
    assert rows(pd.DataFrame({"a": [1, 3]})) == 2  # different content
    assert rows(pd.DataFrame({"b": [1, 2]})) == 2  # different columns
    assert rows(pd.DataFrame({"a": [1.0, 2.0]})) == 2  # different dtype
    assert len(calls) == 4

    @cache
    def total(arr: Any) -> float:
        calls.append(2)
        return float(arr.sum())

    calls.clear()
    assert total(np.arange(6).reshape(2, 3)) == 15
    assert total(np.arange(6).reshape(2, 3)) == 15
    assert total(np.arange(6).reshape(3, 2)) == 15  # shape is part of the key
    assert len(calls) == 2
    # Object arrays hash element reprs of primitives, never memory addresses.
    assert total(np.array([1, 2], dtype=object)) == 3
    assert total(np.array([1, 2], dtype=object)) == 3
    assert len(calls) == 3


def test_unhashable_arguments_run_uncached_with_a_warning() -> None:
    calls: list[int] = []

    class Opaque:
        pass

    @cache
    def identity(value: Any) -> Any:
        calls.append(1)
        return value

    with pytest.warns(RuntimeWarning, match="not caching"):
        identity(Opaque())
    identity(Opaque())
    assert len(calls) == 2
    assert identity.cache_info()["uncached"] == 2

    np = pytest.importorskip("numpy")
    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        identity(np.array([Opaque()], dtype=object))
    assert len(calls) == 3


def test_positional_and_keyword_calls_share_a_key() -> None:
    calls: list[int] = []

    @cache
    def scaled(x: int, factor: int = 2) -> int:
        calls.append(1)
        return x * factor

    assert scaled(3) == scaled(3, 2) == scaled(x=3) == scaled(3, factor=2) == 6
    assert len(calls) == 1
    with pytest.raises(TypeError):
        scaled(1, 2, 3)  # type: ignore[call-arg]


def test_key_tracks_globals_and_called_helpers() -> None:
    namespace: dict[str, Any] = {"__name__": "__main__"}
    exec(
        "import kernelledger as kl\n"
        "class Counter:\n"
        "    n = 0\n"
        "counter = Counter()\n"  # opaque object: keyed by type, so mutation keeps hits
        "SCALE = 2\n"
        "def helper(x):\n"
        "    return x + 1\n"
        "@kl.cache\n"
        "def compute(x):\n"
        "    counter.n += 1\n"
        "    return helper(x) * SCALE\n",
        namespace,
    )
    compute, counter = namespace["compute"], namespace["counter"]
    assert compute(1) == 4 and compute(1) == 4
    assert counter.n == 1
    namespace["SCALE"] = 3  # a referenced global changed
    assert compute(1) == 6
    exec("def helper(x):\n    return x + 10\n", namespace)  # helper redefined
    assert compute(1) == 33
    assert compute(1) == 33
    assert counter.n == 3


def test_functions_without_source_get_distinct_keys() -> None:
    first: dict[str, Any] = {"__name__": "__main__"}
    second: dict[str, Any] = {"__name__": "__main__"}
    for ns, body in ((first, "x + 1"), (second, "x + 2")):
        exec(compile(f"def h(x):\n    return {body}\n", "cells/x.py@1", "exec"), ns)
    from kernelledger.cache import _func_signature

    assert _func_signature(first["h"]) != _func_signature(second["h"])


def test_persistent_cache_returns_unpicklable_results(tmp_path: Path) -> None:
    @persistent_cache(cache_dir=tmp_path)
    def make(n: int) -> Any:
        return lambda: n

    with pytest.warns(RuntimeWarning, match="not cached on disk"):
        value = make(1)
    assert value() == 1
    assert make.cache_info()["entries"] == 0

    @persistent_cache(cache_dir=tmp_path)
    def square(n: int) -> int:
        return n * n

    assert square(4) == 16 and square(n=4) == 16
    assert square.cache_info()["hits"] == 1


def test_in_memory_lru_is_thread_safe() -> None:
    @cache(maxsize=8)
    def work(i: int) -> int:
        return i * 2

    failures: list[BaseException] = []

    def hammer(offset: int) -> None:
        try:
            for i in range(2_000):
                assert work((i + offset) % 32) == ((i + offset) % 32) * 2
        except BaseException as error:  # pragma: no cover - surfaced below
            failures.append(error)

    threads = [threading.Thread(target=hammer, args=(n,)) for n in range(8)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    assert not failures
    info = work.cache_info()
    assert info["currsize"] <= 8 and info["hits"] + info["misses"] == 16_000
