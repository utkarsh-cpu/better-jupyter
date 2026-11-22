"""Run SQL-cell queries outside the worker, e.g. from `.py` scripts exported by KernelLedger."""

import inspect
from typing import Any

from .worker import SqlEngine


def run_sql(query: str, namespace: dict[str, Any] | None = None, refs: Any = None) -> Any:
    """Execute `query` against DataFrames/record lists in `namespace` (default: caller globals).

    Uses DuckDB when installed, else stdlib SQLite, exactly like a notebook SQL cell. The
    result is a pandas DataFrame when pandas is importable, else Polars, else PyArrow, else a
    list of dicts. Only tables the query references are copied into the SQLite fallback.
    """
    if namespace is None:
        frame = inspect.currentframe()
        caller = frame.f_back if frame is not None else None
        namespace = caller.f_globals if caller is not None else {}
    return SqlEngine(namespace).run(query, refs)
