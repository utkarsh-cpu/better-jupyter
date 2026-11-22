"""Content-addressed in-memory and persistent disk caching decorators (`@kl.cache`).

A cache key covers the function's code, its bound arguments (positional and keyword
spellings of the same call share a key), the values of the module globals it reads
when they are cheap to hash, and the code of user helpers it calls (one level deep).
Arguments that cannot be hashed by content disable caching for that call, with a
warning, instead of failing it.
"""

from __future__ import annotations

import functools
import hashlib
import inspect
import json
import os
import pickle
import tempfile
import threading
import types
import warnings
from collections import OrderedDict
from collections.abc import Callable
from dataclasses import fields, is_dataclass
from pathlib import Path
from typing import Any, TypeVar, cast

F = TypeVar("F", bound=Callable[..., Any])

_PRIMITIVE_OBJECT_TYPES = (type(None), bool, int, float, complex, str, bytes)
# Globals larger than this are not hashed per call; the key records their identity only.
_GLOBAL_HASH_LIMIT = 1 << 20


class Uncacheable(TypeError):
    """An argument has no stable content hash; the call runs uncached."""


def _hash_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _numpy_key(arr: Any) -> list[Any]:
    if arr.dtype.hasobject:
        # Object arrays hold pointers; hash element reprs, but only of primitives.
        items: list[Any] = arr.ravel(order="K").tolist()
        if not all(type(item) in _PRIMITIVE_OBJECT_TYPES for item in items):
            raise Uncacheable("object ndarray with non-primitive elements")
        digest = _hash_bytes(repr(items).encode())
    else:
        digest = _hash_bytes(arr.tobytes(order="C"))
    return ["ndarray", str(arr.dtype), list(arr.shape), digest]


def _pandas_key(value: Any, kind: str) -> list[Any]:
    import importlib

    pd: Any = importlib.import_module("pandas")
    try:
        rows: Any = pd.util.hash_pandas_object(value, index=True).to_numpy()
    except TypeError as error:  # unhashable cell values, e.g. lists in an object column
        raise Uncacheable(f"pandas {kind} with unhashable values: {error}") from None
    if kind == "DataFrame":
        layout: Any = [[repr(c) for c in value.columns], [str(d) for d in value.dtypes]]
    else:
        layout = [repr(value.name), str(value.dtype)]
    return [f"pandas.{kind}", layout, _hash_bytes(rows.tobytes())]


def _canonical_key(value: Any, _depth: int = 0) -> Any:
    if _depth > 16:
        raise Uncacheable("cache key nesting limit exceeded")
    cls: Any = cast(Any, type(value))
    obj: Any = value  # attribute access below must not depend on isinstance narrowing
    if value is None or isinstance(value, (bool, int, float, str)):
        return [cls.__name__, value]
    if isinstance(value, (bytes, bytearray)):
        return ["bytes", _hash_bytes(bytes(value))]
    if isinstance(value, Path):
        return ["path", str(value)]
    if isinstance(value, (list, tuple)):
        seq = cast(list[Any] | tuple[Any, ...], value)
        return [cls.__name__, [_canonical_key(v, _depth + 1) for v in seq]]
    if isinstance(value, (set, frozenset)):
        bag = cast(set[Any] | frozenset[Any], value)
        encoded = sorted(json.dumps(_canonical_key(v, _depth + 1), sort_keys=True) for v in bag)
        return [cls.__name__, encoded]
    if isinstance(value, dict):
        mapping = cast(dict[Any, Any], value)
        items = sorted(
            (
                json.dumps(_canonical_key(k, _depth + 1), sort_keys=True),
                _canonical_key(v, _depth + 1),
            )
            for k, v in mapping.items()
        )
        return ["dict", items]
    if is_dataclass(value) and not isinstance(value, type):
        return [
            f"dataclass:{cls.__module__}.{cls.__qualname__}",
            [[f.name, _canonical_key(getattr(value, f.name), _depth + 1)] for f in fields(value)],
        ]
    module = cls.__module__.split(".", 1)[0]
    if module == "numpy" and cls.__name__ == "ndarray":
        return _numpy_key(obj)
    if module == "numpy" and hasattr(value, "dtype") and hasattr(value, "item"):
        return ["numpy", str(obj.dtype), _canonical_key(obj.item(), _depth + 1)]
    if module == "pandas" and cls.__name__ in ("DataFrame", "Series"):
        return _pandas_key(obj, cls.__name__)
    if module == "polars" and cls.__name__ == "DataFrame":
        try:
            rows = obj.hash_rows(seed=0).to_list()
        except Exception as error:
            raise Uncacheable(f"polars DataFrame: {error}") from None
        schema: list[Any] = [[str(k), str(v)] for k, v in obj.schema.items()]
        return ["polars.DataFrame", schema, rows]
    raise Uncacheable(f"unsupported cache key type: {cls.__module__}.{cls.__qualname__}")


def _code_digest(code: types.CodeType) -> str:
    """Hash bytecode, names and constants (recursing into nested code objects)."""
    parts: list[Any] = [code.co_code.hex(), code.co_names, code.co_varnames]
    for const in code.co_consts:
        parts.append(_code_digest(const) if isinstance(const, types.CodeType) else repr(const))
    return _hash_bytes(repr(parts).encode())


def _function_source_digest(fn: Callable[..., Any]) -> str:
    """Source text plus bytecode: exec'd notebook functions may have no (or the wrong) file."""
    target = inspect.unwrap(fn)
    try:
        source = inspect.getsource(target)
    except (OSError, TypeError):
        source = ""
    code = getattr(target, "__code__", None)
    code_hash = _code_digest(code) if isinstance(code, types.CodeType) else ""
    return _hash_bytes((source + "\0" + code_hash).encode("utf-8"))


def _func_signature(fn: Callable[..., Any]) -> str:
    return f"{fn.__module__}.{fn.__qualname__}:{_function_source_digest(fn)[:16]}"


def _global_names(code: types.CodeType) -> set[str]:
    names = set(code.co_names)
    for const in code.co_consts:
        if isinstance(const, types.CodeType):
            names |= _global_names(const)
    return names


def _global_value_key(value: Any, user_modules: tuple[str, ...]) -> Any:
    """Cheap content key for a global; identity only when hashing would be costly."""
    kind: Any = cast(Any, type(value))
    if isinstance(value, types.ModuleType):
        return ["module", value.__name__]
    if isinstance(value, type):
        return ["class", value.__module__, value.__qualname__]
    if callable(value):
        target = inspect.unwrap(value)
        module = getattr(target, "__module__", None)
        qualname = getattr(target, "__qualname__", type(target).__qualname__)
        if isinstance(target, types.FunctionType) and module in user_modules:
            # One level deep: a called user helper's code, not what *it* calls.
            return ["function", qualname, _function_source_digest(target)]
        return ["callable", str(module), qualname]
    if isinstance(value, (str, bytes, bytearray)) and len(value) > _GLOBAL_HASH_LIMIT:
        data = value.encode("utf-8", "surrogatepass") if isinstance(value, str) else bytes(value)
        return [type(value).__name__, _hash_bytes(data)]
    shape = getattr(value, "shape", None)
    if isinstance(shape, tuple) and all(isinstance(d, int) for d in cast(tuple[Any, ...], shape)):
        elements = 1
        for dim in cast(tuple[int, ...], shape):
            elements *= dim
        if elements > _GLOBAL_HASH_LIMIT:
            return ["large", type(value).__qualname__, id(value)]
    if kind in (list, tuple, dict, set, frozenset) and len(cast(Any, value)) > 10_000:
        return ["large", kind.__qualname__, id(cast(object, value))]
    try:
        return _canonical_key(value)
    except (Uncacheable, RecursionError):
        return ["unhashed", kind.__module__, kind.__qualname__]


def _globals_key(fn: Callable[..., Any]) -> Any:
    """Values of the globals the function body reads (including nested comprehensions)."""
    target = inspect.unwrap(fn)
    code = getattr(target, "__code__", None)
    namespace = getattr(target, "__globals__", None)
    if not isinstance(code, types.CodeType) or not isinstance(namespace, dict):
        return []
    global_ns = cast(dict[str, Any], namespace)
    user_modules = ("__main__", str(getattr(target, "__module__", "__main__")))
    keyed: list[Any] = []
    for name in sorted(_global_names(code)):
        if name not in global_ns:
            continue
        value = global_ns[name]
        if value is fn or inspect.unwrap(value) is target:
            continue
        keyed.append([name, _global_value_key(value, user_modules)])
    return keyed


class _KeyBuilder:
    def __init__(self, func: Callable[..., Any]) -> None:
        self.func = func
        self.sig = _func_signature(func)
        try:
            self.signature: inspect.Signature | None = inspect.signature(func)
        except (TypeError, ValueError):
            self.signature = None

    def __call__(self, args: tuple[Any, ...], kwargs: dict[str, Any]) -> str:
        if self.signature is not None:
            bound = self.signature.bind(*args, **kwargs)
            bound.apply_defaults()
            call: Any = [[name, _canonical_key(value)] for name, value in bound.arguments.items()]
        else:
            call = [_canonical_key(args), _canonical_key(kwargs)]
        payload = json.dumps(
            {"fn": self.sig, "call": call, "globals": _globals_key(self.func)},
            sort_keys=True,
            separators=(",", ":"),
            default=repr,
        ).encode("utf-8")
        return _hash_bytes(payload)


def _cache_key(
    builder: _KeyBuilder, args: tuple[Any, ...], kwargs: dict[str, Any], warned: list[bool]
) -> str | None:
    try:
        return builder(args, kwargs)
    except TypeError as error:
        if not isinstance(error, Uncacheable):
            raise  # Invalid call arguments: let the function report them.
        if not warned[0]:
            warned[0] = True
            warnings.warn(
                f"{builder.func.__qualname__}: not caching this call ({error})",
                RuntimeWarning,
                stacklevel=3,
            )
        return None


def cache(fn: Callable[..., Any] | None = None, *, maxsize: int = 128) -> Any:
    """Thread-safe in-memory LRU memoization keyed by argument content."""

    def decorator(func: F) -> F:
        store: OrderedDict[str, Any] = OrderedDict()
        stats = {"hits": 0, "misses": 0, "uncached": 0}
        lock = threading.RLock()
        builder = _KeyBuilder(func)
        warned = [False]

        @functools.wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            try:
                key = _cache_key(builder, args, kwargs, warned)
            except TypeError:
                return func(*args, **kwargs)
            if key is None:
                with lock:
                    stats["uncached"] += 1
                return func(*args, **kwargs)
            with lock:
                if key in store:
                    stats["hits"] += 1
                    store.move_to_end(key)
                    return store[key]
                stats["misses"] += 1
            # Compute outside the lock so recursive and concurrent calls never deadlock.
            result = func(*args, **kwargs)
            with lock:
                store[key] = result
                store.move_to_end(key)
                if maxsize > 0:
                    while len(store) > maxsize:
                        store.popitem(last=False)
            return result

        def cache_info() -> dict[str, int]:
            with lock:
                return {
                    "hits": stats["hits"],
                    "misses": stats["misses"],
                    "uncached": stats["uncached"],
                    "currsize": len(store),
                    "maxsize": maxsize,
                }

        def cache_clear() -> None:
            with lock:
                store.clear()
                stats.update(hits=0, misses=0, uncached=0)

        wrapper.cache_info = cache_info  # type: ignore[attr-defined]
        wrapper.cache_clear = cache_clear  # type: ignore[attr-defined]
        return wrapper  # type: ignore[return-value]

    if fn is not None:
        return decorator(fn)
    return decorator


def persistent_cache(
    fn: Callable[..., Any] | None = None,
    *,
    cache_dir: str | Path | None = None,
    name: str | None = None,
) -> Any:
    """Content-addressed disk cache under `.kl/cache` surviving worker restarts and worktrees."""

    def decorator(func: F) -> F:
        stats = {"hits": 0, "misses": 0, "uncached": 0}
        lock = threading.Lock()
        builder = _KeyBuilder(func)
        warned = [False]
        bucket = name or f"{func.__module__}_{func.__qualname__}".replace(".", "_")
        safe_bucket = "".join(c if c.isalnum() or c in "_-" else "_" for c in bucket)[:64]

        def resolve_dir() -> Path:
            if cache_dir is not None:
                base = Path(cache_dir)
            else:
                env_dir = os.environ.get("KL_CACHE_DIR")
                base = Path(env_dir) if env_dir else Path.cwd() / ".kl" / "cache"
            return base / safe_bucket

        def count(stat: str) -> None:
            with lock:
                stats[stat] += 1

        @functools.wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            try:
                key = _cache_key(builder, args, kwargs, warned)
            except TypeError:
                return func(*args, **kwargs)
            if key is None:
                count("uncached")
                return func(*args, **kwargs)
            folder = resolve_dir()
            entry_path = folder / f"{key}.pkl"
            if entry_path.is_file():
                try:
                    with entry_path.open("rb") as fh:
                        value = pickle.load(fh)
                    count("hits")
                    return value
                except Exception:
                    pass
            count("misses")
            result = func(*args, **kwargs)
            try:
                data = pickle.dumps(result, protocol=pickle.HIGHEST_PROTOCOL)
            except Exception as error:
                # The computed result is still the answer; it just cannot be persisted.
                warnings.warn(
                    f"{func.__qualname__}: result not cached on disk ({type(error).__name__}: "
                    f"{error})",
                    RuntimeWarning,
                    stacklevel=2,
                )
                return result
            try:
                folder.mkdir(parents=True, exist_ok=True)
                fd, tmp_name = tempfile.mkstemp(dir=str(folder), suffix=".tmp")
            except OSError as error:
                warnings.warn(
                    f"{func.__qualname__}: cache directory unavailable ({error})",
                    RuntimeWarning,
                    stacklevel=2,
                )
                return result
            try:
                with os.fdopen(fd, "wb") as fh:
                    fh.write(data)
                os.replace(tmp_name, entry_path)
            except OSError as error:
                try:
                    os.unlink(tmp_name)
                except OSError:
                    pass
                warnings.warn(
                    f"{func.__qualname__}: result not cached on disk ({error})",
                    RuntimeWarning,
                    stacklevel=2,
                )
            return result

        def cache_info() -> dict[str, Any]:
            folder = resolve_dir()
            entries = list(folder.glob("*.pkl")) if folder.is_dir() else []
            with lock:
                snapshot = dict(stats)
            return {
                **snapshot,
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
            with lock:
                stats.update(hits=0, misses=0, uncached=0)

        wrapper.cache_info = cache_info  # type: ignore[attr-defined]
        wrapper.cache_clear = cache_clear  # type: ignore[attr-defined]
        return wrapper  # type: ignore[return-value]

    if fn is not None:
        return decorator(fn)
    return decorator
