"""Content-addressed in-memory and persistent disk caching decorators (`@kl.cache`)."""

from __future__ import annotations

import functools
import hashlib
import inspect
import json
import os
import pickle
import tempfile
from collections import OrderedDict
from collections.abc import Callable
from dataclasses import asdict, is_dataclass
from pathlib import Path
from typing import Any, TypeVar

F = TypeVar("F", bound=Callable[..., Any])


def _canonical_key(value: Any, _depth: int = 0) -> Any:
    if _depth > 16:
        raise TypeError("Cache key nesting limit exceeded")
    if value is None or isinstance(value, (bool, int, float, str)):
        return [type(value).__name__, value]
    if isinstance(value, (bytes, bytearray)):
        return ["bytes", hashlib.sha256(bytes(value)).hexdigest()]
    if isinstance(value, Path):
        return ["path", str(value)]
    if isinstance(value, (list, tuple)):
        return [type(value).__name__, [_canonical_key(v, _depth + 1) for v in value]]
    if isinstance(value, (set, frozenset)):
        encoded = sorted(
            json.dumps(_canonical_key(v, _depth + 1), sort_keys=True) for v in value
        )
        return [type(value).__name__, encoded]
    if isinstance(value, dict):
        items = sorted(
            (
                json.dumps(_canonical_key(k, _depth + 1), sort_keys=True),
                _canonical_key(v, _depth + 1),
            )
            for k, v in value.items()
        )
        return ["dict", items]
    if is_dataclass(value) and not isinstance(value, type):
        return [
            f"dataclass:{type(value).__module__}.{type(value).__qualname__}",
            _canonical_key(asdict(value), _depth + 1),
        ]
    # NumPy arrays / pandas DataFrames if present
    mod = type(value).__module__.split(".", 1)[0]
    if mod == "numpy" and hasattr(value, "dtype") and hasattr(value, "shape") and hasattr(value, "tobytes"):
        return [
            "ndarray",
            str(value.dtype),
            list(value.shape),
            hashlib.sha256(value.tobytes()).hexdigest(),
        ]
    raise TypeError(f"Unsupported cache key type: {type(value).__name__}")


def _digest_call(func_sig: str, args: tuple[Any, ...], kwargs: dict[str, Any]) -> str:
    payload = json.dumps(
        {
            "fn": func_sig,
            "args": _canonical_key(args),
            "kwargs": _canonical_key(kwargs),
        },
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def _func_signature(fn: Callable[..., Any]) -> str:
    try:
        src = inspect.getsource(fn)
    except (OSError, TypeError):
        src = ""
    src_hash = hashlib.sha256(src.encode("utf-8")).hexdigest()[:16]
    return f"{fn.__module__}.{fn.__qualname__}:{src_hash}"


def cache(fn: F | None = None, *, maxsize: int = 128) -> Any:
    """In-memory LRU memoization decorator supporting lists, dicts, paths, dataclasses, and arrays."""

    def decorator(func: F) -> F:
        store: OrderedDict[str, Any] = OrderedDict()
        stats = {"hits": 0, "misses": 0}
        sig = _func_signature(func)

        @functools.wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            key = _digest_call(sig, args, kwargs)
            if key in store:
                stats["hits"] += 1
                store.move_to_end(key)
                return store[key]
            stats["misses"] += 1
            result = func(*args, **kwargs)
            store[key] = result
            if maxsize > 0:
                while len(store) > maxsize:
                    store.popitem(last=False)
            return result

        def cache_info() -> dict[str, int]:
            return {
                "hits": stats["hits"],
                "misses": stats["misses"],
                "currsize": len(store),
                "maxsize": maxsize,
            }

        def cache_clear() -> None:
            store.clear()
            stats["hits"] = 0
            stats["misses"] = 0

        wrapper.cache_info = cache_info  # type: ignore[attr-defined]
        wrapper.cache_clear = cache_clear  # type: ignore[attr-defined]
        return wrapper  # type: ignore[return-value]

    if fn is not None:
        return decorator(fn)
    return decorator


def persistent_cache(
    fn: F | None = None,
    *,
    cache_dir: str | Path | None = None,
    name: str | None = None,
) -> Any:
    """Content-addressed disk cache under `.kl/cache` surviving worker restarts and worktrees."""

    def decorator(func: F) -> F:
        stats = {"hits": 0, "misses": 0}
        sig = _func_signature(func)
        bucket = name or f"{func.__module__}_{func.__qualname__}".replace(".", "_")
        safe_bucket = "".join(c if c.isalnum() or c in "_-" else "_" for c in bucket)[:64]

        def resolve_dir() -> Path:
            if cache_dir is not None:
                base = Path(cache_dir)
            else:
                env_dir = os.environ.get("KL_CACHE_DIR")
                base = Path(env_dir) if env_dir else Path.cwd() / ".kl" / "cache"
            return base / safe_bucket

        @functools.wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            key = _digest_call(sig, args, kwargs)
            folder = resolve_dir()
            entry_path = folder / f"{key}.pkl"
            if entry_path.is_file():
                try:
                    with entry_path.open("rb") as fh:
                        value = pickle.load(fh)
                    stats["hits"] += 1
                    return value
                except Exception:
                    pass
            stats["misses"] += 1
            result = func(*args, **kwargs)
            folder.mkdir(parents=True, exist_ok=True)
            data = pickle.dumps(result, protocol=pickle.HIGHEST_PROTOCOL)
            fd, tmp_name = tempfile.mkstemp(dir=str(folder), suffix=".tmp")
            try:
                with os.fdopen(fd, "wb") as fh:
                    fh.write(data)
                os.replace(tmp_name, entry_path)
            except Exception:
                try:
                    os.unlink(tmp_name)
                except OSError:
                    pass
                raise
            return result

        def cache_info() -> dict[str, Any]:
            folder = resolve_dir()
            entries = list(folder.glob("*.pkl")) if folder.is_dir() else []
            return {
                "hits": stats["hits"],
                "misses": stats["misses"],
                "entries": len(entries),
                "bytes": sum(p.stat().st_size for p in entries),
                "path": str(folder),
            }

        def cache_clear() -> None:
            folder = resolve_dir()
            if folder.is_dir():
                for p in folder.glob("*.pkl"):
                    try:
                        p.unlink()
                    except OSError:
                        pass
            stats["hits"] = 0
            stats["misses"] = 0

        wrapper.cache_info = cache_info  # type: ignore[attr-defined]
        wrapper.cache_clear = cache_clear  # type: ignore[attr-defined]
        return wrapper  # type: ignore[return-value]

    if fn is not None:
        return decorator(fn)
    return decorator
