"""Standalone worker. Intentionally imports no KernelLedger modules."""

import __future__

import _thread
import ast
import asyncio
import base64
import bdb
import builtins
import codecs
import datetime as dt
import decimal
import enum
import fractions
import hashlib
import importlib
import importlib.util
import inspect
import io
import itertools
import json
import keyword
import linecache
import math
import os
import pathlib
import platform
import queue
import random
import re
import reprlib
import shlex
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import tokenize
import traceback
import types
import unicodedata
import uuid
import zoneinfo
from typing import Any, cast

SCHEMA = 1
INLINE = 65536
ENGINE_IDENTITY = {"name": "custom", "version": 1, "transformer": "kernelledger.notebook/2"}
INTERACTIVE_FEATURES = [
    "timing",
    "profiling",
    "result_suppression",
    "bounded_history",
    "capture",
    "completion",
    "documentation",
    "namespace_metadata",
    "async_cells",
    "shared_console",
    "static_tables",
    "media_attachments",
    "debugger",
    "native_widgets_v1",
]
MAGICS = {
    "pip": "Install with the selected project interpreter (explicit execution).",
    "cd": "Change the worker directory; paths may be quoted.",
    "pwd": "Return the worker directory.",
    "time": "Execute once and report wall and process CPU time.",
    "timeit": "Repeat code: -n COUNT -r REPEATS (defaults: 1 and 3; maximum 10000 runs).",
    "prun": "Profile one execution; report at most 100 functions.",
    "history": "Show recent inputs; on/off/clear controls the optional bounded cache.",
    "commands": "List supported notebook commands and their limits.",
    "inspect": "Passive help for a name; name? and name?? include documentation/recorded source.",
    "run": "Execute exact local UTF-8 file bytes in this namespace; record their hash and working directory.",
}
CELL_MAGICS = {
    "time": MAGICS["time"],
    "timeit": MAGICS["timeit"],
    "prun": MAGICS["prun"],
    "capture": "Capture at most 64 KiB of text output; optional fresh variable name.",
    "writefile": "Write exact cell-body bytes to a file; -a appends. External effect.",
    "sh": "POSIX shell cell (unavailable on Windows). External effects are recorded, not undone.",
    "bash": "Explicit Bash cell on POSIX when Bash is installed.",
    "cmd": "Windows command cell using the selected platform shell.",
}


def cell_magic(source: str) -> tuple[str, str, str] | None:
    """Recognize a cell header only before any executable Python token."""
    lines = source.splitlines(keepends=True)
    for row, line in enumerate(lines):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if not line.startswith("%%"):
            return None
        parts = line[2:].strip().split(maxsplit=1)
        if not parts or parts[0] not in CELL_MAGICS:
            raise ValueError(
                "Unsupported cell magic. Supported: " + ", ".join("%%" + n for n in CELL_MAGICS)
            )
        # Preserve the original line numbers in compiled functions and errors.
        body = "\n" * (row + 1) + "".join(lines[row + 1 :])
        return parts[0], parts[1] if len(parts) > 1 else "", body
    return None


def suppress_result(source: str) -> bool:
    """A final semicolon is syntax, including before trailing comments."""
    last = ""
    try:
        for token in tokenize.generate_tokens(io.StringIO(source).readline):
            if token.type not in (
                tokenize.NEWLINE,
                tokenize.NL,
                tokenize.COMMENT,
                tokenize.ENDMARKER,
                tokenize.INDENT,
                tokenize.DEDENT,
            ):
                last = token.string
    except (tokenize.TokenError, IndentationError):
        return False
    return last == ";"


def json_bytes(value: Any) -> Any:
    return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode()


def recv_exact(sock: Any, count: Any) -> Any:
    chunks: list[Any]
    chunks = []
    while count:
        chunk = sock.recv(min(count, 1048576))
        if not chunk:
            raise EOFError("connection closed")
        chunks.append(chunk)
        count -= len(chunk)
    return b"".join(chunks)


def receive(sock: Any) -> Any:
    size = struct.unpack(">I", recv_exact(sock, 4))[0]
    if size > INLINE:
        raise ValueError("protocol header exceeds inline limit")
    message = json.loads(recv_exact(sock, size))
    if message.get("schema") != SCHEMA:
        raise ValueError("unsupported protocol schema")
    if message["kind"] == "blob":
        if (
            type(message.get("byte_size")) is not int
            or not 0 <= message["byte_size"] <= 2 * 1024**3
        ):
            raise ValueError("invalid binary frame size")
        data = bytearray(message["byte_size"])
        view = memoryview(data)
        offset = 0
        while offset < len(data):
            count = sock.recv_into(view[offset : offset + 1048576])
            if not count:
                raise EOFError("connection closed")
            offset += count
        if hashlib.sha256(data).hexdigest() != message["sha256"]:
            raise ValueError("blob hash mismatch")
        message["data"] = data
    return message


class Channel:
    def __init__(self, sock: Any) -> None:
        self.sock = sock
        self.lock = threading.Lock()

    def send(self, kind: Any, **fields: Any) -> Any:
        message: dict[Any, Any]
        message = {"schema": SCHEMA, "kind": kind, **fields}
        data = json_bytes(message)
        if kind in ("interactive_reply", "interactive_state") and len(data) > INLINE:
            # Live drafts, documentation and debugger locals never use durable
            # blob envelopes, even for a large or malicious object name.
            message = {
                "schema": SCHEMA,
                "kind": kind,
                **(
                    {"debug": {"truncated": True}}
                    if kind == "interactive_state"
                    else {
                        "request_id": fields.get("request_id"),
                        "worker_generation": fields.get("worker_generation"),
                        "error": {"code": "too_large", "message": "Transient reply exceeds 64 KiB"},
                    }
                ),
            }
            data = json_bytes(message)
        if len(data) > INLINE:
            digest = self.blob(data)
            data = json_bytes({"schema": SCHEMA, "kind": "envelope", "blob": digest})
        with self.lock:
            self.sock.sendall(struct.pack(">I", len(data)) + data)

    def blob(self, data: Any) -> Any:
        digest = hashlib.sha256(data).hexdigest()
        header = json_bytes(
            {"schema": SCHEMA, "kind": "blob", "sha256": digest, "byte_size": len(data)}
        )
        with self.lock:
            self.sock.sendall(struct.pack(">I", len(header)) + header)
            self.sock.sendall(memoryview(data))
        return digest


class Excluded(Exception):
    def __init__(self, reason: Any, path: Any, obj: Any) -> None:
        self.reason, self.path = reason, path
        self.type = type(obj).__module__ + "." + type(obj).__qualname__
        super().__init__(reason)


def notebook_tree(source: str, filename: str) -> ast.Module:
    """Translate escapes only where Python tokenization identifies executable code.

    Restart after each escape so shell quotes, comments and brackets are never
    interpreted as Python. Original source stays in the ledger and line cache.
    """
    magic = cell_magic(source)
    if magic:
        if magic[0] in ("writefile", "sh", "bash", "cmd"):
            return ast.parse("pass", filename)
        source = magic[2]
    lines = source.splitlines(keepends=True)
    pattern = re.compile(
        r"^(?P<indent>[ \t]*)(?:(?P<target>[^\W\d]\w*)\s*=\s*)?(?P<escape>!!?|%)(?P<command>.*)$"
    )
    while True:
        changed = False
        depth, logical_row = 0, None
        try:
            for token in tokenize.generate_tokens(io.StringIO("".join(lines)).readline):
                if token.type in (tokenize.NEWLINE, tokenize.ENDMARKER):
                    logical_row = None
                elif token.type not in (
                    tokenize.INDENT,
                    tokenize.DEDENT,
                    tokenize.NL,
                    tokenize.COMMENT,
                    tokenize.ENCODING,
                ):
                    if logical_row is None:
                        logical_row = token.start[0]
                if token.type == tokenize.OP:
                    if token.string in ("(", "[", "{"):
                        depth += 1
                    elif token.string in (")", "]", "}"):
                        depth -= 1
                if token.string not in ("!", "%", "?") or depth or logical_row != token.start[0]:
                    continue
                row, column = token.start
                original = lines[row - 1]
                if token.string == "?":
                    help_match = re.fullmatch(r"(.+?)(\?\??)\s*(?:#.*)?", original.strip())
                    if help_match and all(part.isidentifier() for part in help_match[1].split(".")):
                        command = (
                            "inspect "
                            + help_match[1]
                            + (" source" if help_match[2] == "??" else "")
                        )
                        lines[row - 1] = f"__kl_command__({command!r}, 'magic')\n"
                        changed = True
                        break
                match = pattern.match(original.rstrip("\r\n"))
                if not match or column != match.start("escape"):
                    continue
                escape = match["escape"]
                command = match["command"]
                target = f"{match['target']} = " if match["target"] else ""
                mode = "magic" if escape == "%" else "shell"
                capture = bool(target) or escape == "!!"
                ending = (
                    "\r\n" if original.endswith("\r\n") else "\n" if original.endswith("\n") else ""
                )
                lines[row - 1] = (
                    f"{match['indent']}{target}__kl_command__({command!r}, {mode!r}, {capture!r}, {row!r}, {filename!r}){ending}"
                )
                changed = True
                break
        except (tokenize.TokenError, IndentationError):
            # ast.parse supplies the usual SyntaxError for invalid Python.
            pass
        if not changed:
            return ast.parse("".join(lines), filename)


def windows_shell_path(path: str) -> str:
    """cmd.exe needs DOS/UNC spelling rather than Win32 extended namespaces."""
    if path.upper().startswith("\\\\?\\UNC\\"):
        return "\\\\" + path[8:]
    if path.startswith("\\\\?\\") and len(path) > 6 and path[5] == ":":
        return path[4:]
    return path


def stop_shell(process: Any) -> None:
    """Stop this shell's descendants without stopping the notebook worker."""
    if process.poll() is not None:
        return
    try:
        if os.name == "nt":
            subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                creationflags=0x08000000,
                timeout=5,
            )
        else:
            # Children retain the worker's process group, so forced runtime stop
            # still contains them. For a soft interrupt, target only this tree.
            table = subprocess.run(
                ["ps", "-e", "-o", "pid=,ppid="],
                capture_output=True,
                text=True,
                timeout=5,
                check=True,
            )
            parents = [tuple(map(int, row.split())) for row in table.stdout.splitlines()]
            descendants = [process.pid]
            for parent in descendants:
                descendants.extend(pid for pid, ppid in parents if ppid == parent)
            for pid in reversed(descendants):
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
    except (OSError, subprocess.SubprocessError):
        pass
    if process.poll() is None:
        process.kill()
    process.wait(timeout=5)


class ExecutionIndex:
    def __init__(self) -> None:
        self.revisions: dict[str, str] = {}
        self.entries: dict[Any, Any]
        self.entries = {}
        self.captured: dict[Any, Any]
        self.captured = {}

    def add(self, filename: Any, source: Any, ordinal: Any) -> Any:
        self.revisions[filename] = hashlib.sha256(source.encode()).hexdigest()
        tree = notebook_tree(source, filename)
        linecache.cache[filename] = (len(source), None, source.splitlines(True), filename)
        entries: list[Any]
        entries = []

        def walk(node: Any, prefix: Any = "") -> Any:
            for child in ast.iter_child_nodes(node):
                if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                    qualname = prefix + child.name
                    entries.append((child, qualname))
                    walk(
                        child, qualname + ("." if isinstance(child, ast.ClassDef) else ".<locals>.")
                    )
                elif isinstance(child, ast.Lambda):
                    entries.append((child, prefix + "<lambda>"))
                    walk(child, prefix)
                else:
                    walk(child, prefix)

        walk(tree)
        self.entries[filename] = (source, ordinal, entries)
        return tree

    def capture_classes(
        self, filename: str, namespace: dict[str, Any], previous: dict[str, Any]
    ) -> None:
        _, ordinal, entries = self.entries[filename]
        for node, qualname in entries:
            if isinstance(node, ast.ClassDef) and "." not in qualname:
                obj = namespace.get(node.name)
                if isinstance(obj, type) and obj is not previous.get(node.name):
                    self.captured[id(obj)] = (ast.unparse(node), ordinal, node.lineno)

    def source(self, obj: Any) -> Any:
        if id(obj) in self.captured:
            return self.captured[id(obj)]
        filename = None
        line = None
        if isinstance(obj, types.FunctionType):
            filename, line = obj.__code__.co_filename, obj.__code__.co_firstlineno
        elif isinstance(obj, type):
            line = getattr(obj, "__firstlineno__", None)
            for value in vars(obj).values():
                if isinstance(value, (staticmethod, classmethod)):
                    value = cast(Any, value).__func__
                if (
                    isinstance(value, types.FunctionType)
                    and value.__code__.co_filename in self.entries
                ):
                    filename = value.__code__.co_filename
                    break
        if filename not in self.entries and isinstance(obj, type):
            matches: list[Any]
            matches = [
                file
                for file, (_, _, entries) in self.entries.items()
                if any(
                    isinstance(node, ast.ClassDef) and qualname == obj.__qualname__
                    for node, qualname in entries
                )
            ]
            if len(matches) == 1:
                filename = matches[0]
        if filename not in self.entries:
            raise Excluded("source_unavailable", obj.__qualname__, obj)
        _, ordinal, entries = self.entries[filename]
        candidates: list[Any]
        candidates = []
        for node, qualname in entries:
            start = min([node.lineno] + [d.lineno for d in getattr(node, "decorator_list", [])])
            if isinstance(obj, type):
                match = isinstance(node, ast.ClassDef) and qualname == obj.__qualname__
            elif obj.__name__ == "<lambda>":
                match = isinstance(node, ast.Lambda) and node.lineno == line
            else:
                match = (
                    isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
                    and (start == line or node.lineno == line)
                    and qualname == obj.__qualname__
                )
            if match:
                candidates.append(node)
        if len(candidates) != 1:
            raise Excluded("source_unavailable", obj.__qualname__, obj)
        node = candidates[0]
        node = ast.parse(ast.unparse(node)).body[0]
        target = (
            node.value
            if isinstance(node, ast.Expr) and isinstance(node.value, ast.Lambda)
            else node
        )
        if isinstance(target, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)):
            if not isinstance(target, ast.Lambda):
                target.decorator_list = []
            # Defaults and annotations are restored as graph values; never re-evaluate them.

            target.args.defaults = []

            target.args.kw_defaults = [None for _ in target.args.kwonlyargs]
            if not isinstance(target, ast.Lambda):
                target.returns = None
            for arg in [*target.args.posonlyargs, *target.args.args, *target.args.kwonlyargs]:
                arg.annotation = None
            for arg in [target.args.vararg, target.args.kwarg]:
                if arg:
                    arg.annotation = None
        return ast.unparse(node), ordinal, candidates[0].lineno


class GraphEncoder:
    def __init__(
        self, index: Any, object_limit: Any = 100000, payload_limit: Any = 2 * 1024**3
    ) -> None:
        self.index = index
        self.object_limit, self.payload_limit = object_limit, payload_limit
        self.objects: dict[Any, Any]
        self.ids: dict[Any, Any]
        self.blobs: dict[Any, Any]
        self.objects, self.ids, self.blobs = {}, {}, {}
        self.active: set[Any]
        self.active = set()
        self.notes: list[Any]
        self.codecs: dict[Any, Any]
        self.notes, self.codecs = [], {}
        self.seen: set[Any]
        self.seen = set()

    def blob(self, data: Any) -> Any:
        digest = hashlib.sha256(data).hexdigest()
        self.blobs[digest] = data
        if sum(map(len, self.blobs.values())) > self.payload_limit:
            raise OverflowError("payload_limit")
        return digest

    def encode(self, obj: Any, path: Any, hashed: Any = False) -> Any:
        identity = id(obj)
        self.seen.add(identity)
        if len(self.seen) > self.object_limit:
            raise Excluded("limit", path, obj)
        if obj is None or type(obj) is bool:
            return obj
        t: Any = cast(Any, type(obj))
        if t is int:
            return {"k": "int", "v": str(obj)}
        if t is float:
            return {"k": "float", "v": obj if math.isfinite(obj) else str(obj)}
        if t is complex:
            return {
                "k": "complex",
                "re": self.encode(obj.real, path),
                "im": self.encode(obj.imag, path),
            }
        if t is str:
            data = obj.encode()
            return {
                "k": "str",
                **({"v": obj} if len(data) <= INLINE else {"blob": self.blob(data)}),
            }
        if t is bytes:
            return {"k": "bytes", **self.binary(obj)}
        if t in (dt.datetime, dt.date, dt.time):
            tz = getattr(obj, "tzinfo", None)
            tzinfo = None
            if tz is not None:
                if isinstance(tz, zoneinfo.ZoneInfo):
                    tzinfo: dict[str, Any] | None
                    tzinfo = {"zone": tz.key}
                elif isinstance(tz, dt.timezone):
                    tzinfo: dict[str, Any] | None
                    tzinfo = {
                        "offset_s": int(obj.utcoffset().total_seconds()),
                        "name": tz.tzname(None),
                    }
                else:
                    raise Excluded("unsupported_type", path, obj)
            return {
                "k": t.__name__,
                "v": obj.isoformat(),
                "tz": tzinfo,
                "fold": getattr(obj, "fold", 0),
            }
        if t is dt.timedelta:
            return {
                "k": "timedelta",
                "days": obj.days,
                "seconds": obj.seconds,
                "microseconds": obj.microseconds,
            }
        if t is decimal.Decimal:
            return {"k": "Decimal", "v": str(obj)}
        if t is fractions.Fraction:
            return {"k": "Fraction", "num": str(obj.numerator), "den": str(obj.denominator)}
        if t is uuid.UUID:
            return {"k": "UUID", "v": str(obj)}
        if t is range:
            return {
                "k": "range",
                "start": str(obj.start),
                "stop": str(obj.stop),
                "step": str(obj.step),
            }
        if isinstance(obj, pathlib.PurePath):
            return {"k": "path", "class": t.__name__, "v": str(obj)}
        if isinstance(obj, enum.Enum):
            return {"k": "enum", "class": self.encode(t, path + ".class"), "name": obj.name}
        if identity in self.ids:
            if identity in self.active and (t in (tuple, frozenset) or hashed):
                raise Excluded("unsupported_cycle", path, obj)
            return {"k": "ref", "id": self.ids[identity]}
        oid = "o" + str(len(self.ids) + 1)
        self.ids[identity] = oid
        self.active.add(identity)
        self.objects[oid] = {}
        try:
            node = self.object(obj, path, hashed)
            self.objects[oid] = node
        finally:
            self.active.remove(identity)
        return {"k": "ref", "id": oid}

    def binary(self, data: Any) -> Any:
        return (
            {"b64": base64.b64encode(data).decode()}
            if len(data) <= 4096
            else {"blob": self.blob(data)}
        )

    def object(self, obj: Any, path: Any, hashed: Any) -> Any:
        t: Any = cast(Any, type(obj))
        if t is bytearray:
            return {"k": "bytearray", **self.binary(bytes(obj))}
        if t in (list, tuple, set, frozenset):
            return {
                "k": t.__name__,
                "items": [
                    self.encode(v, f"{path}[{i}]", hashed or t in (set, frozenset))
                    for i, v in enumerate(obj)
                ],
            }
        if t is dict:
            return {
                "k": "dict",
                "items": [
                    [self.encode(k, path + ".key", True), self.encode(v, f"{path}[{k!r}]", hashed)]
                    for k, v in obj.items()
                ],
            }
        if isinstance(obj, types.ModuleType):
            return {"k": "module", "module": obj.__name__}
        module, qualname = getattr(obj, "__module__", None), getattr(obj, "__qualname__", None)
        if module and module != "__main__" and qualname and "<locals>" not in qualname:
            try:
                imported = importlib.import_module(module)
                for part in qualname.split("."):
                    imported = getattr(imported, part)
                if imported is obj:
                    return {"k": "import", "module": module, "qualname": qualname}
            except (ImportError, AttributeError):
                pass
        if isinstance(obj, types.FunctionType) and module == "__main__":
            source, ordinal, line = self.index.source(obj)
            closure: list[Any]
            closure = []
            for name, cell in zip(obj.__code__.co_freevars, obj.__closure__ or ()):
                try:
                    value = self.encode(cell.cell_contents, path + ".closure." + name)
                    closure.append({"name": name, "value": value})
                except ValueError:
                    closure.append({"name": name, "empty": True})
            if closure:
                self.notes.append({"kind": "closure_cells_split", "path": path})
            return {
                "k": "function",
                "source": self.blob(source.encode()),
                "ordinal": ordinal,
                "line": line,
                "name": obj.__name__,
                "qualname": obj.__qualname__,
                "closure": closure,
                "defaults": self.encode(obj.__defaults__, path + ".defaults"),
                "kwdefaults": self.encode(obj.__kwdefaults__, path + ".kwdefaults"),
                "attrs": self.encode(obj.__dict__, path + ".attrs"),
            }
        if isinstance(obj, type) and module == "__main__":
            source, ordinal, line = self.index.source(obj)
            self.notes.append({"kind": "class_from_source", "path": path})
            return {
                "k": "class",
                "source": self.blob(source.encode()),
                "name": obj.__name__,
                "ordinal": ordinal,
                "line": line,
            }
        if (
            t.__module__ == "__main__"
            and hasattr(obj, "__dict__")
            and not any("__slots__" in vars(c) for c in t.__mro__)
        ):
            return {
                "k": "instance",
                "class": self.encode(t, path + ".class"),
                "attrs": self.encode(vars(obj), path + ".attrs"),
            }
        if t.__module__.startswith("numpy"):
            obj = cast(Any, obj)
            np: Any = importlib.import_module("numpy")

            self.codecs["numpy"] = np.__version__
            if isinstance(obj, np.ndarray):
                if obj.dtype.hasobject:
                    raise Excluded("unsupported_type", path, obj)
                buf = io.BytesIO()
                np.save(buf, obj, allow_pickle=False)
                if obj.base is not None:
                    self.notes.append({"kind": "view_copied", "path": path})
                return {
                    "k": "ndarray",
                    "blob": self.blob(buf.getvalue()),
                    "dtype": str(obj.dtype),
                    "shape": list(obj.shape),
                }
            if isinstance(obj, np.random.Generator):
                return {
                    "k": "numpy_generator",
                    "class": type(obj.bit_generator).__name__,
                    "state": self.encode(obj.bit_generator.state, path + ".state"),
                }
        if t.__module__.startswith("pandas"):
            obj = cast(Any, obj)
            pd: Any = importlib.import_module("pandas")

            if isinstance(obj, (pd.DataFrame, pd.Series)):
                try:
                    pa: Any = importlib.import_module("pyarrow")
                except ImportError:
                    raise Excluded("missing_codec", path, obj) from None
                self.codecs["arrow"] = pa.__version__
                self.codecs["pandas"] = pd.__version__
                frame = obj.to_frame() if isinstance(obj, pd.Series) else obj
                try:
                    table = pa.Table.from_pandas(frame, preserve_index=True)
                    buf = pa.BufferOutputStream()
                    with pa.ipc.new_stream(buf, table.schema) as writer:
                        writer.write_table(table)
                    data = buf.getvalue().to_pybytes()
                    if not pa.ipc.open_stream(data).read_all().to_pandas().equals(frame):
                        raise ValueError("lossy Arrow round trip")
                except Exception:
                    raise Excluded("unsupported_type", path, obj) from None
                return {
                    "k": "table",
                    "codec": "arrow",
                    "blob": self.blob(data),
                    "series": isinstance(obj, pd.Series),
                    "name": self.encode(obj.name, path + ".name")
                    if isinstance(obj, pd.Series)
                    else None,
                }
        resource = isinstance(
            obj,
            (
                io.IOBase,
                socket.socket,
                threading.Thread,
                types.GeneratorType,
                types.CoroutineType,
                types.AsyncGeneratorType,
            ),
        )
        resource = resource or t.__module__ in ("_thread", "sqlite3", "asyncio.locks")
        raise Excluded("resource" if resource else "unsupported_type", path, obj)

    def checkpoint(self, namespace: Any, denylist: Any = (), internal_bindings: Any = None) -> Any:
        bindings: dict[Any, Any]
        excluded: list[Any]
        bindings, excluded = {}, []
        for name, obj in namespace.items():
            if name.startswith("_kl_"):
                continue
            if name == "__name__" and type(obj) is str and obj == "__main__":
                continue
            if internal_bindings and name in internal_bindings and obj is internal_bindings[name]:
                continue
            if name == "__builtins__" and obj is builtins.__dict__:
                continue
            if name == "__builtins__" or name in denylist:
                excluded.append(
                    {"name": name, "type": type(obj).__name__, "path": name, "reason": "denylisted"}
                )
                continue
            # Roll back an excluded root, so later aliases never reference an unfinished node.
            saved = (self.objects.copy(), self.ids.copy(), self.blobs.copy(), list(self.notes))
            self.seen: set[Any]
            self.seen = set()
            try:
                bindings[name] = self.encode(obj, name)
            except (Excluded, RecursionError) as error:
                self.objects, self.ids, self.blobs, self.notes = saved
                if isinstance(error, RecursionError):
                    error = Excluded("limit", name, obj)
                excluded.append(
                    {"name": name, "type": error.type, "path": error.path, "reason": error.reason}
                )
        self.seen: set[Any]
        self.seen = set()
        bindings["_kl_random"] = self.encode(random.getstate(), "_kl_random")
        return {
            "schema": 1,
            "python": platform.python_version(),
            "platform": platform.platform(),
            "codecs": self.codecs,
            "bindings": bindings,
            "objects": self.objects,
            "excluded": excluded,
            "notes": self.notes,
            "imports_reimported": any(
                n["k"] in ("module", "import") for n in self.objects.values()
            ),
        }


def validate_checkpoint(manifest: Any) -> None:
    """Validate the graph before importing libraries or executing saved definitions."""
    if not isinstance(manifest, dict) or cast(dict[str, Any], manifest).get("schema") != 1:
        raise ValueError("unsupported checkpoint schema")
    manifest = cast(dict[str, Any], manifest)
    for field in ("bindings", "objects", "codecs"):
        if not isinstance(manifest.get(field), dict):
            raise ValueError("invalid checkpoint field: " + field)
    for field in ("excluded", "notes"):
        if not isinstance(manifest.get(field), list):
            raise ValueError("invalid checkpoint field: " + field)
    if not isinstance(manifest.get("python"), str) or not re.fullmatch(
        r"\d+\.\d+\.\d+", manifest["python"]
    ):
        raise ValueError("invalid checkpoint Python version")
    objects: dict[Any, Any] = manifest["objects"]
    required = {
        "list": ("items",),
        "tuple": ("items",),
        "dict": ("items",),
        "set": ("items",),
        "frozenset": ("items",),
        "bytearray": (),
        "module": ("module",),
        "import": ("module", "qualname"),
        "function": (
            "source",
            "ordinal",
            "line",
            "name",
            "qualname",
            "closure",
            "defaults",
            "kwdefaults",
            "attrs",
        ),
        "class": ("source", "ordinal", "line", "name"),
        "instance": ("class", "attrs"),
        "ndarray": ("blob", "dtype", "shape"),
        "table": ("blob", "codec", "series", "name"),
        "numpy_generator": ("class", "state"),
    }

    def digest(value: Any) -> None:
        if not isinstance(value, str) or re.fullmatch("[0-9a-f]{64}", value) is None:
            raise ValueError("invalid checkpoint blob hash")

    def binary(node: dict[str, Any]) -> None:
        if ("blob" in node) == ("b64" in node):
            raise ValueError("binary value needs exactly one payload")
        if "blob" in node:
            digest(node["blob"])
        else:
            base64.b64decode(node["b64"], validate=True)

    def value(node: Any) -> None:
        if node is None or type(node) is bool:
            return
        if not isinstance(node, dict):
            raise ValueError("untagged checkpoint value")
        node = cast(dict[str, Any], node)
        kind = node.get("k")
        if kind == "ref":
            if node.get("id") not in objects:
                raise ValueError("dangling graph reference")
        elif kind in ("int", "range", "Fraction"):
            fields = (
                ("v",)
                if kind == "int"
                else ("start", "stop", "step")
                if kind == "range"
                else ("num", "den")
            )
            for field in fields:
                if not isinstance(node.get(field), str) or not re.fullmatch(r"-?\d+", node[field]):
                    raise ValueError("invalid integer encoding")
        elif kind == "float":
            number = node.get("v")
            if not (type(number) in (int, float) or number in ("nan", "inf", "-inf")):
                raise ValueError("invalid float encoding")
        elif kind == "complex":
            value(node["re"])
            value(node["im"])
        elif kind == "str":
            if ("v" in node) == ("blob" in node):
                raise ValueError("string needs exactly one payload")
            if "blob" in node:
                digest(node["blob"])
            elif not isinstance(node["v"], str):
                raise ValueError("invalid string encoding")
        elif kind == "bytes":
            binary(node)
        elif kind in ("date", "time", "datetime", "Decimal", "UUID", "path"):
            if not isinstance(node.get("v"), str):
                raise ValueError("invalid value encoding")
            if kind == "path" and node.get("class") not in (
                "PosixPath",
                "WindowsPath",
                "PurePosixPath",
                "PureWindowsPath",
            ):
                raise ValueError("unknown path class")
            if kind in ("time", "datetime") and (
                "tz" not in node or type(node.get("fold")) is not int or node["fold"] not in (0, 1)
            ):
                raise ValueError("invalid datetime encoding")
        elif kind == "timedelta":
            if not all(type(node.get(n)) is int for n in ("days", "seconds", "microseconds")):
                raise ValueError("invalid timedelta encoding")
        elif kind == "enum":
            value(node["class"])
            if not isinstance(node.get("name"), str):
                raise ValueError("invalid enum encoding")
        else:
            raise ValueError("unsupported checkpoint value kind")

    for name, binding in manifest["bindings"].items():
        if (
            not isinstance(name, str)
            or (name.startswith("_kl_") and name != "_kl_random")
            or name == "__builtins__"
        ):
            raise ValueError("invalid checkpoint binding")
        value(binding)
    for oid, node in objects.items():
        if not isinstance(oid, str) or not isinstance(node, dict):
            raise ValueError("invalid checkpoint object")
        node = cast(dict[str, Any], node)
        kind = node.get("k")
        if kind not in required or not all(field in node for field in required[kind]):
            raise ValueError("unsupported or malformed checkpoint object")
        for field in ("blob", "source"):
            if field in node:
                digest(node[field])
        if kind in ("list", "tuple", "set", "frozenset", "dict"):
            if not isinstance(node["items"], list):
                raise ValueError("invalid container items")
            for item in cast(list[Any], node["items"]):
                if kind == "dict":
                    if not isinstance(item, list) or len(cast(list[Any], item)) != 2:
                        raise ValueError("invalid dict pair")
                    value(item[0])
                    value(item[1])
                else:
                    value(item)
        elif kind == "bytearray":
            binary(node)
        elif kind == "function":
            for field in ("defaults", "kwdefaults", "attrs"):
                value(node[field])
            if not isinstance(node["closure"], list):
                raise ValueError("invalid closure")
            for cell in cast(list[Any], node["closure"]):
                if not isinstance(cell.get("name"), str) or not cell["name"].isidentifier():
                    raise ValueError("invalid closure name")
                if not cell.get("empty"):
                    value(cell["value"])
        elif kind == "instance":
            value(node["class"])
            value(node["attrs"])
        elif kind == "numpy_generator":
            value(node["state"])
        elif kind == "table":
            if node["codec"] != "arrow" or type(node["series"]) is not bool:
                raise ValueError("unsupported table codec")
            value(node["name"])


class GraphLoader:
    def __init__(self, manifest: Any, blobs: Any, namespace: Any, index: Any = None) -> None:
        validate_checkpoint(manifest)
        self.manifest, self.blobs, self.namespace = manifest, blobs, namespace
        self.index = index
        self.objects: dict[Any, Any]
        self.building: set[Any]
        self.filled: set[Any]
        self.objects, self.building, self.filled = {}, set(), set()
        self.failures: dict[Any, Any]
        self.failures = {}
        for digest, data in blobs.items():
            if hashlib.sha256(data).hexdigest() != digest:
                raise ValueError("checkpoint blob hash mismatch")

    def binary(self, node: Any) -> Any:
        return (
            self.blobs[node["blob"]]
            if "blob" in node
            else base64.b64decode(node["b64"], validate=True)
        )

    def value(self, node: Any) -> Any:
        if node is None or type(node) is bool:
            return node
        k = node["k"]
        if k == "ref":
            return self.object(node["id"])
        if k == "int":
            return int(node["v"])
        if k == "float":
            return float(node["v"])
        if k == "complex":
            return complex(self.value(node["re"]), self.value(node["im"]))
        if k == "str":
            return self.blobs[node["blob"]].decode() if "blob" in node else node["v"]
        if k == "bytes":
            return self.binary(node)
        if k in ("date", "time", "datetime"):
            result = getattr(dt, k).fromisoformat(node["v"])
            if k != "date":
                tz = node["tz"]
                tz = (
                    (
                        zoneinfo.ZoneInfo(tz["zone"])
                        if "zone" in tz
                        else dt.timezone(dt.timedelta(seconds=tz["offset_s"]), tz.get("name", ""))
                    )
                    if tz
                    else None
                )
                result = result.replace(tzinfo=tz, fold=node["fold"])
            return result
        if k == "timedelta":
            return dt.timedelta(**{n: node[n] for n in ("days", "seconds", "microseconds")})
        if k == "Decimal":
            return decimal.Decimal(node["v"])
        if k == "Fraction":
            return fractions.Fraction(int(node["num"]), int(node["den"]))
        if k == "UUID":
            return uuid.UUID(node["v"])
        if k == "range":
            return range(*(int(node[n]) for n in ("start", "stop", "step")))
        if k == "path":
            return getattr(pathlib, node["class"])(node["v"])
        if k == "enum":
            return self.value(node["class"])[node["name"]]
        raise ValueError("unsupported value kind: " + k)

    def object(self, oid: Any) -> Any:
        if oid in self.failures:
            raise ValueError(self.failures[oid])
        if oid in self.objects:
            return self.objects[oid]
        if oid in self.building:
            raise ValueError("unsupported_cycle")
        self.building.add(oid)
        node = self.manifest["objects"][oid]
        k = node["k"]
        result: Any
        try:
            if k == "module":
                result = importlib.import_module(node["module"])
            elif k == "import":
                result = importlib.import_module(node["module"])
                for part in node["qualname"].split("."):
                    result = getattr(result, part)
            elif k in ("list", "dict", "set"):
                result = cast(Any, {"list": list, "dict": dict, "set": set}[k]())
            elif k == "bytearray":
                result = bytearray(self.binary(node))
            elif k in ("tuple", "frozenset"):
                result = {"tuple": tuple, "frozenset": frozenset}[k](
                    self.value(v) for v in node["items"]
                )
            elif k == "class":
                source = self.blobs[node["source"]].decode()
                filename = "<checkpoint-class>@" + oid
                if self.index:
                    self.index.add(filename, source, node["ordinal"])
                exec(compile(source, filename, "exec"), self.namespace)
                result = self.namespace[node["name"]]
            elif k == "function":
                source = self.blobs[node["source"]].decode()
                closure = node["closure"]
                params = ", ".join(c["name"] for c in closure)
                definition = ast.parse(source).body[0]
                is_lambda = isinstance(definition, ast.Expr)
                body = (
                    "    _kl_result = " + source
                    if is_lambda
                    else "\n".join("    " + ln for ln in source.splitlines())
                )
                name = "_kl_result" if is_lambda else definition.name
                factory_source = f"def _kl_factory({params}):\n{body}\n    return {name}\n"
                filename = "<checkpoint-function>@" + oid
                if self.index:
                    self.index.add(filename, factory_source, node["ordinal"])
                exec(compile(factory_source, filename, "exec"), self.namespace)
                factory = self.namespace.pop("_kl_factory")
                result = factory(
                    *(
                        None
                        if c.get("empty")
                        or (
                            isinstance(c.get("value"), dict)
                            and c["value"].get("k") == "ref"
                            and c["value"]["id"] in self.building
                        )
                        else self.value(c["value"])
                        for c in closure
                    )
                )
                for c, cell in zip(closure, result.__closure__ or ()):
                    if c.get("empty"):
                        del cell.cell_contents
                result.__name__, result.__qualname__ = node["name"], node["qualname"]
            elif k == "instance":
                cls = self.value(node["class"])
                result = object.__new__(cls)
            elif k == "ndarray":
                np: Any = importlib.import_module("numpy")

                result = np.load(io.BytesIO(self.blobs[node["blob"]]), allow_pickle=False).copy(
                    order="K"
                )
                if str(result.dtype) != node["dtype"] or list(result.shape) != node["shape"]:
                    raise ValueError("array metadata mismatch")
            elif k == "table":
                pa: Any = importlib.import_module("pyarrow")

                result = pa.ipc.open_stream(self.blobs[node["blob"]]).read_all().to_pandas()
                if node["series"]:
                    result = result.iloc[:, 0]
                    result.name = self.value(node["name"])
            elif k == "numpy_generator":
                np: Any = importlib.import_module("numpy")

                result = np.random.Generator(getattr(np.random, node["class"])())
            else:
                raise ValueError("unsupported object kind: " + k)
            self.objects[oid] = result
            if self.index and k in ("function", "class"):
                self.index.captured[id(result)] = (
                    self.blobs[node["source"]].decode(),
                    node["ordinal"],
                    node["line"],
                )
            return result
        except Exception as error:
            reason = (
                "missing_codec"
                if isinstance(error, ImportError) and k in ("ndarray", "table", "numpy_generator")
                else "unimportable"
                if k in ("module", "import")
                else "source_exec_failed"
            )
            self.failures[oid] = reason
            raise ValueError(reason) from error
        finally:
            self.building.remove(oid)

    def fill(self, oid: Any) -> Any:
        if oid in self.filled:
            return
        self.filled.add(oid)
        node, obj = self.manifest["objects"][oid], self.object(oid)
        k = node["k"]
        if k == "instance":
            for ref in self.references(node["attrs"]):
                self.fill(ref)
            obj.__dict__.update(self.value(node["attrs"]))
        elif k == "function":
            for c, cell in zip(node["closure"], obj.__closure__ or ()):
                if c.get("empty"):
                    try:
                        del cell.cell_contents
                    except ValueError:
                        pass
                else:
                    cell.cell_contents = self.value(c["value"])
            for ref in self.references(node["attrs"]):
                self.fill(ref)
            obj.__defaults__ = self.value(node["defaults"])
            obj.__kwdefaults__ = self.value(node["kwdefaults"])
            obj.__dict__.update(self.value(node["attrs"]))
        elif k == "list":
            obj.extend(self.value(v) for v in node["items"])
        elif k == "dict":
            for a, b in node["items"]:
                self.complete(a, set())
                obj[self.value(a)] = self.value(b)
        elif k == "set":
            for value in node["items"]:
                self.complete(value, set())
                obj.add(self.value(value))
        elif k == "numpy_generator":
            obj.bit_generator.state = self.value(node["state"])

    def complete(self, value: Any, seen: Any) -> Any:
        for oid in self.references(value):
            if oid in seen:
                continue
            seen.add(oid)
            for child in self.references(self.manifest["objects"][oid]):
                self.complete({"k": "ref", "id": child}, seen)
            self.fill(oid)

    def references(self, value: Any) -> Any:
        if isinstance(value, dict):
            if cast(dict[str, Any], value).get("k") == "ref":
                yield value["id"]
            for child in cast(dict[str, Any], value).values():
                yield from self.references(child)
        elif isinstance(value, list):
            for child in cast(list[Any], value):
                yield from self.references(child)

    def load(self) -> Any:
        objects = self.manifest["objects"]
        # Seed scalar/global bindings and imports before source statements execute.
        for name, value in self.manifest["bindings"].items():
            if not name.startswith("_kl_"):
                try:
                    if (
                        not isinstance(value, dict)
                        or cast(dict[str, Any], value).get("k") != "ref"
                        or objects[value["id"]]["k"]
                        in (
                            "module",
                            "import",
                            "list",
                            "dict",
                            "set",
                            "bytearray",
                            "ndarray",
                            "table",
                        )
                    ):
                        self.namespace[name] = self.value(value)
                except Exception:
                    pass
        ordered = sorted(
            objects, key=lambda oid: (objects[oid].get("ordinal", -1), objects[oid].get("line", -1))
        )
        for oid in ordered:
            try:
                obj = self.object(oid)
                for name, value in self.manifest["bindings"].items():
                    if value == {"k": "ref", "id": oid}:
                        self.namespace[name] = obj
            except Exception:
                pass
        # Dicts containing attrs must be filled before functions/instances copy them.
        for kind in ("list", "dict", "instance", "function", "set", "numpy_generator"):
            for oid in ordered:
                if objects[oid]["k"] == kind:
                    try:
                        self.fill(oid)
                    except Exception:
                        self.failures[oid] = "source_exec_failed"
        installed: list[Any]
        excluded: list[Any]
        installed, excluded = [], list(self.manifest["excluded"])
        for name, value in self.manifest["bindings"].items():
            seen: set[Any]
            pending: list[Any]
            seen, pending = set(), list(self.references(value))
            failure = None
            while pending:
                oid = pending.pop()
                if oid in seen:
                    continue
                seen.add(oid)
                if oid in self.failures:
                    failure = self.failures[oid]
                    break
                pending.extend(self.references(objects[oid]))
            try:
                if failure:
                    raise ValueError(failure)
                obj = self.value(value)
                if name == "_kl_random":
                    random.setstate(obj)
                else:
                    self.namespace[name] = obj
                    installed.append(name)
            except Exception as error:
                self.namespace.pop(name, None)
                excluded.append({"name": name, "reason": str(error), "path": name})
        keep = set(installed) | {
            "__builtins__",
            "__name__",
            "display",
            "update_display",
            "clear_output",
            "__kl_command__",
        }
        for name in list(self.namespace):
            if name not in keep:
                del self.namespace[name]
        return {"installed": installed, "excluded": excluded, "notes": self.manifest["notes"]}


class Stream:
    encoding = "utf-8"
    errors = "replace"

    def __init__(self, worker: Any, kind: Any) -> None:
        self.worker, self.kind = worker, kind
        self.buffered: list[Any]
        self.buffered, self.size, self.last = [], 0, time.monotonic()
        self.lock = threading.Lock()
        self.origin: Any = None

    def write(self, value: Any) -> Any:
        if not isinstance(value, str):
            raise TypeError("write requires a string")
        with self.lock:
            origin = (
                self.worker.exec_id
                if threading.current_thread() is threading.main_thread()
                or threading.current_thread().name.startswith("_kl_shell_")
                else None
            )
            if origin != self.origin:
                self._flush()
                self.origin = origin
            self.buffered.append(value)
            self.size += len(value.encode())
            if self.size >= 32768 or time.monotonic() - self.last >= 0.05:
                self._flush()
        return len(value)

    def _flush(self) -> Any:
        if self.buffered:
            self.worker.emit(self.kind, text="".join(self.buffered), _origin=self.origin)
        self.buffered: list[Any]
        self.buffered, self.size, self.last = [], 0, time.monotonic()

    def flush(self) -> Any:
        with self.lock:
            self._flush()

    def isatty(self) -> Any:
        return False


class ResultHistory:
    """Optional, bounded conveniences. The ledger remains the history authority."""

    names = ("_", "__", "___", "In", "Out")

    def __init__(self, worker: Any) -> None:
        self.worker = worker
        self.enabled = False
        self.inputs: dict[int, str] = {}
        self.outputs: dict[int, Any] = {}
        self.owned: dict[str, Any] = {}
        self.relinquished: set[str] = set()
        self.limit = 8 * 1024 * 1024

    def account(self, value: Any, seen: set[int]) -> int:
        if id(value) in seen:
            return 0
        if len(seen) >= 100000 or type(value) not in (
            type(None),
            bool,
            int,
            float,
            complex,
            str,
            bytes,
            bytearray,
            list,
            tuple,
            dict,
            set,
            frozenset,
        ):
            raise ValueError("result size is not bounded by the built-in history codec")
        seen.add(id(value))
        size = sys.getsizeof(value)
        if size > self.limit:
            raise ValueError("history byte limit")
        if type(value) is dict:
            for key, item in cast(dict[Any, Any], value).items():
                size += self.account(key, seen) + self.account(item, seen)
                if size > self.limit:
                    raise ValueError("history byte limit")
        elif type(value) in (list, tuple, set, frozenset):
            for item in value:
                size += self.account(item, seen)
                if size > self.limit:
                    raise ValueError("history byte limit")
        return size

    def assign(self, name: str, value: Any) -> None:
        ns = self.worker.namespace
        if name in self.relinquished:
            return
        if name in self.owned and ns.get(name) is not self.owned[name]:
            self.relinquished.add(name)
            self.owned.pop(name, None)
            self.worker.internal_bindings.pop(name, None)
            return
        ns[name] = self.owned[name] = value
        self.worker.internal_bindings[name] = value

    def release_user_writes(self, tree: ast.AST) -> None:
        # Explicit stores take ownership even if assigning the same object.
        # Stores inside a function do not change globals until it is called;
        # conservatively relinquishing a shortcut is safe and preserves data.
        for node in ast.walk(tree):
            if isinstance(node, ast.Name) and isinstance(node.ctx, (ast.Store, ast.Del)):
                if node.id in self.owned:
                    self.relinquished.add(node.id)
                    self.owned.pop(node.id, None)
                    self.worker.internal_bindings.pop(node.id, None)

    def configure(self, command: str) -> Any:
        command = command.strip()
        if command == "on":
            if not self.enabled:
                occupied = [name for name in self.names if name in self.worker.namespace]
                if occupied:
                    raise ValueError("History shortcuts already bound: " + ", ".join(occupied))
                self.enabled = True
                self.relinquished.clear()
                self.assign("In", self.inputs.copy())
                self.assign("Out", self.outputs.copy())
            return None
        if command in ("off", "clear"):
            for name, value in self.owned.items():
                if self.worker.namespace.get(name) is value:
                    self.worker.namespace.pop(name, None)
                self.worker.internal_bindings.pop(name, None)
            self.owned.clear()
            # Replace, do not clear, dictionaries a user may have aliased.
            self.inputs, self.outputs = {}, {}
            self.enabled = command == "clear" and self.enabled
            if self.enabled:
                self.assign("In", self.inputs.copy())
                self.assign("Out", self.outputs.copy())
            return None
        if command:
            raise ValueError("%history [on|off|clear]; default cache is off")
        return "\n".join(f"{count}: {text}" for count, text in self.inputs.items())

    def record(self, ordinal: int, source: str, result: Any, suppressed: bool) -> None:
        if not self.enabled:
            return
        # A user may mutate an exposed cache. Never traverse that object again.
        self.inputs = dict(list(self.inputs.items())[-99:])
        self.outputs = dict(list(self.outputs.items())[-99:])
        if len(source.encode()) <= 64 * 1024:
            self.inputs[ordinal] = source
        if result is not None and not suppressed:
            try:
                self.account(result, set())
            except (ValueError, RecursionError):
                pass
            else:
                self.outputs[ordinal] = result
        while True:
            try:
                self.account([self.inputs, self.outputs], set())
                break
            except (ValueError, RecursionError):
                if self.outputs:
                    self.outputs.pop(next(iter(self.outputs)))
                elif self.inputs:
                    self.inputs.pop(next(iter(self.inputs)))
                else:
                    break
        self.assign("In", self.inputs.copy())
        self.assign("Out", self.outputs.copy())
        values = list(self.outputs.values())[-3:][::-1]
        for name, value in zip(self.names[:3], values):
            self.assign(name, value)
        for name in self.names[len(values) : 3]:
            if name in self.owned:
                old = self.owned.pop(name)
                if self.worker.namespace.get(name) is old:
                    self.worker.namespace.pop(name, None)
                self.worker.internal_bindings.pop(name, None)


class CustomEngine:
    """Execution-thread boundary; transport, source indexing and storage stay outside."""

    def __init__(self, worker: Any) -> None:
        self.worker = worker
        self.future_flags = 0
        self.epoch = 0
        self.loop = asyncio.new_event_loop()
        asyncio.set_event_loop(self.loop)
        self.history = ResultHistory(worker)

    def compile(self, tree: Any, filename: str, mode: str) -> Any:
        code = compile(
            tree,
            filename,
            mode,
            self.future_flags | ast.PyCF_ALLOW_TOP_LEVEL_AWAIT,
            dont_inherit=True,
        )
        for name in __future__.all_feature_names:
            self.future_flags |= code.co_flags & getattr(__future__, name).compiler_flag
        return code

    def evaluate(self, code: Any) -> Any:
        value = eval(code, self.worker.namespace)
        if code.co_flags & 0x80:  # CO_COROUTINE; do not await an ordinary returned coroutine.
            value = self.loop.run_until_complete(value)
        return value

    def run_tree(self, tree: ast.Module, filename: str) -> Any:
        self.history.release_user_writes(tree)
        body = list(tree.body)
        expression = cast(ast.Expr, body.pop()) if body and isinstance(body[-1], ast.Expr) else None
        module = ast.Module(body=body, type_ignores=tree.type_ignores)
        # Compile both before any effects; also propagate future flags to the expression.
        code = self.compile(module, filename, "exec")
        tail = (
            self.compile(ast.Expression(expression.value), filename, "eval") if expression else None
        )
        previous = self.worker.namespace.copy()
        try:
            self.evaluate(code)
        finally:
            self.worker.index.capture_classes(filename, self.worker.namespace, previous)
        return self.evaluate(tail) if tail else None

    def timing(self, wall: float, cpu: float, **fields: Any) -> None:
        data = {
            "wall_seconds": time.perf_counter() - wall,
            "cpu_seconds": time.process_time() - cpu,
            **fields,
        }
        text = f"Wall time: {data['wall_seconds']:.6f} s; CPU time: {data['cpu_seconds']:.6f} s"
        if "runs" in fields:
            text += f"; {fields['runs']} runs ({fields['count']} × {fields['repeats']})"
        self.worker.emit(
            "display",
            bundle={
                "text/plain": {"value": text},
                "application/vnd.kernelledger.timing+json": {"value": data},
            },
        )

    def execute(self, tree: ast.Module, source: str, filename: str) -> Any:
        asyncio.set_event_loop(self.loop)
        magic = cell_magic(source)
        if not magic:
            return self.run_tree(tree, filename)
        name, argument, _body = magic
        if name in ("sh", "bash", "cmd"):
            if argument:
                raise ValueError("Shell cells do not accept header arguments")
            lines = source.splitlines(keepends=True)
            header = next(row for row, line in enumerate(lines) if line.startswith("%%"))
            script = "".join(lines[header + 1 :])
            if name in ("sh", "bash"):
                if os.name == "nt" or not shutil.which(name):
                    raise ValueError(
                        f"%%{name} requires a qualified POSIX shell; use %%cmd on Windows"
                    )
                return self.worker.notebook_command(shlex.join([shutil.which(name), "-c", script]))
            if os.name != "nt":
                raise ValueError("%%cmd is available only on Windows")
            with tempfile.NamedTemporaryFile(
                suffix=".cmd", delete=False, dir=os.environ.get("KL_TRAINING_TEMP_ROOT")
            ) as stream:
                path = stream.name
                stream.write(script.encode("utf-8"))
            try:
                return self.worker.notebook_command('call "' + windows_shell_path(path) + '"')
            finally:
                os.unlink(path)
        if name == "writefile":
            parts = shlex.split(argument, posix=os.name != "nt")
            append = parts[:1] == ["-a"]
            if append:
                parts.pop(0)
            if len(parts) != 1:
                raise ValueError("%%writefile [-a] PATH")
            path = parts[0].strip("'\"")
            original = self.worker.index.entries[filename][0]
            lines = original.splitlines(keepends=True)
            header = next(row for row, line in enumerate(lines) if line.startswith("%%"))
            content = "".join(lines[header + 1 :])
            with open(path, "ab" if append else "wb") as stream:
                stream.write(content.encode("utf-8"))
            print(f"{'Appended' if append else 'Wrote'} {path}")
            return None
        return self.run_magic(name, argument, tree, filename)

    def run_magic(self, name: str, argument: str, tree: ast.Module, filename: str) -> Any:
        if name == "capture":
            if argument and (not argument.isidentifier() or keyword.iskeyword(argument)):
                raise ValueError("%%capture accepts an optional fresh variable name")
            if argument and argument in self.worker.namespace:
                raise ValueError("capture target is already bound")
            self.worker.stdout.flush()
            self.worker.stderr.flush()
            original = self.worker.capture
            captured: dict[str, Any] = {"text": "", "truncated": False}
            self.worker.capture = captured
            try:
                result = self.run_tree(tree, filename)
                if result is not None and not suppress_result(
                    self.worker.index.entries[filename][0]
                ):
                    self.worker.emit("result", bundle=self.worker.bundle(result))
                self.worker.figures()
            finally:
                self.worker.stdout.flush()
                self.worker.stderr.flush()
                self.worker.capture = original
                if argument:
                    self.worker.namespace[argument] = captured
                else:
                    self.worker.emit(
                        "display",
                        bundle={
                            "text/plain": {
                                "value": captured["text"]
                                + (
                                    "\n[capture truncated at 64 KiB]"
                                    if captured["truncated"]
                                    else ""
                                )
                            }
                        },
                    )
            return None
        count, repeats = 1, 1
        if name == "timeit":
            repeats = 3
            parts = shlex.split(argument)
            while parts:
                flag = parts.pop(0)
                if flag not in ("-n", "-r") or not parts:
                    raise ValueError("%timeit/%%timeit accepts -n COUNT -r REPEATS")
                value = int(parts.pop(0))
                if flag == "-n":
                    count = value
                else:
                    repeats = value
            if not (1 <= count <= 10000 and 1 <= repeats <= 100 and count * repeats <= 10000):
                raise ValueError("timeit is limited to 10000 total runs and 100 repeats")
        elif argument:
            raise ValueError(f"%%{name} does not accept arguments")
        profiler: Any = None
        if name == "prun":
            import cProfile

            profiler = cProfile.Profile()
        wall, cpu = time.perf_counter(), time.process_time()
        samples: list[float] = []
        result = None
        try:
            if profiler:
                profiler.enable()
            for _ in range(repeats):
                start = time.perf_counter()
                for _ in range(count):
                    result = self.run_tree(tree, filename)
                samples.append((time.perf_counter() - start) / count)
            return result if name != "timeit" else None
        finally:
            if profiler:
                profiler.disable()
            self.timing(
                wall,
                cpu,
                **(
                    {
                        "runs": count * repeats,
                        "count": count,
                        "repeats": repeats,
                        "samples_seconds": samples,
                    }
                    if name == "timeit"
                    else {}
                ),
            )
            if profiler:
                import pstats

                stats: Any = pstats.Stats(profiler)
                rows: list[Any] = []
                for (path, line, function), values in sorted(
                    stats.stats.items(), key=lambda item: item[1][3], reverse=True
                )[:100]:
                    rows.append(
                        {
                            "file": path[:1024],
                            "source_revision": self.worker.index.revisions.get(path),
                            "line": line,
                            "function": function[:512],
                            "calls": values[1],
                            "self_seconds": values[2],
                            "total_seconds": values[3],
                        }
                    )
                self.worker.emit(
                    "display",
                    bundle={
                        "text/plain": {
                            "value": "Profile (at most 100 functions)\n"
                            + "\n".join(
                                f"{r['calls']:>6} {r['total_seconds']:.6f} s  {r['file']}:{r['line']} ({r['function']})"
                                for r in rows
                            )
                        },
                        "application/vnd.kernelledger.profile+json": {
                            "value": {"rows": rows, "truncated": len(stats.stats) > 100}
                        },
                    },
                )

    def inline(self, command: str, row: int, filename: str) -> Any:
        parts = command.split(maxsplit=1)
        name, code = parts[0], parts[1] if len(parts) > 1 else ""
        argument = ""
        if name == "timeit":
            match = re.match(r"((?:(?:-n|-r)\s+\d+\s+)*)", code)
            assert match is not None
            argument, code = match[0].strip(), code[match.end() :]
        if not code.strip():
            raise ValueError(f"%{name} requires Python code")
        nested = filename + f"/magic-{row}"
        tree = self.worker.index.add(nested, "\n" * (row - 1) + code, self.worker.ordinal)
        return self.run_magic(name, argument, tree, nested)

    def cancel_tasks(self) -> None:
        tasks = asyncio.all_tasks(self.loop)
        for task in tasks:
            task.cancel()
        if tasks:
            self.loop.run_until_complete(asyncio.wait(tasks, timeout=0.2))
        if asyncio.all_tasks(self.loop):
            raise RuntimeError(
                "Background tasks refused cancellation; restart or force-stop the worker"
            )

    def close(self) -> None:
        try:
            self.cancel_tasks()
        finally:
            self.loop.close()


def passive_type(value: Any) -> str:
    cls = cast(Any, type(value))
    # Invoke the built-in descriptors directly, bypassing hostile metaclasses.
    name = type.__dict__["__qualname__"].__get__(cls)
    module = type.__dict__["__module__"].__get__(cls)
    return (module[:256] + "." if type(module) is str else "") + name[:256]


def passive_attributes(value: Any) -> dict[str, Any]:
    """Never call dir(), a property, module __getattr__, or object __getattribute__."""
    if type(value) is types.ModuleType:
        return types.ModuleType.__getattribute__(value, "__dict__")
    if type(value) is type:
        return type.__dict__["__dict__"].__get__(value)
    if type(value) in (types.FunctionType, types.BuiltinFunctionType):
        return {}
    result: dict[str, Any] = {}
    for cls in type.__dict__["__mro__"].__get__(type(value)):
        for name, item in type.__dict__["__dict__"].__get__(cls).items():
            if len(result) >= 1000:
                return result
            result.setdefault(name, item)
    return result


def passive_array(value: Any) -> Any:
    """Use only NumPy's C descriptors on an exact ndarray, never its subclasses."""
    module = sys.modules.get("numpy")
    if type(module) is not types.ModuleType:
        return None
    namespace = types.ModuleType.__getattribute__(module, "__dict__")
    cls = namespace.get("ndarray")
    dtype_cls = namespace.get("dtype")
    generic = namespace.get("generic")
    if type(value) is not cls or type(cls) is not type:
        return None
    attributes = type.__dict__["__dict__"].__get__(cls)
    dtype_attributes = type.__dict__["__dict__"].__get__(dtype_cls)
    needed = ("shape", "size", "nbytes", "dtype", "flat")
    if any(type(attributes.get(name)) is not types.GetSetDescriptorType for name in needed):
        return None
    if any(
        type(dtype_attributes.get(name))
        not in (types.GetSetDescriptorType, types.MemberDescriptorType)
        for name in ("kind", "itemsize", "hasobject")
    ):
        return None
    dtype = attributes["dtype"].__get__(value)
    kind = dtype_attributes["kind"].__get__(dtype)
    itemsize = dtype_attributes["itemsize"].__get__(dtype)
    return {
        "shape": list(attributes["shape"].__get__(value)),
        "length": attributes["size"].__get__(value),
        "nbytes": attributes["nbytes"].__get__(value),
        "dtype": f"{kind}{itemsize}",
        "hasobject": dtype_attributes["hasobject"].__get__(dtype),
        "flat_descriptor": attributes["flat"],
        "item_descriptor": type.__dict__["__dict__"].__get__(generic).get("item"),
    }


def passive_preview(value: Any, limit: int = 4096) -> dict[str, Any]:
    result: dict[str, Any] = {"type": passive_type(value), "preview": "", "truncated": False}
    cls = cast(Any, type(value))
    if cls in (type(None), bool, float, complex):
        result["preview"] = repr(value)
    elif cls is int:
        result["preview"] = str(value) if value.bit_length() < 4096 else "<large integer>"
    elif cls in (str, bytes, bytearray):
        result.update(
            length=len(value), preview=repr(value[:limit])[:limit], truncated=len(value) > limit
        )
    elif cls in (list, tuple, dict, set, frozenset):
        result.update(length=len(value), preview=f"<{cls.__name__}, {len(value)} items>")
    elif cls is types.FunctionType:
        result["preview"] = "function " + value.__name__
        result["file"], result["line"] = value.__code__.co_filename, value.__code__.co_firstlineno
        # Signature construction uses no annotation/default repr hooks.
        code = value.__code__
        result["signature"] = (
            value.__name__ + "(" + ", ".join(code.co_varnames[: code.co_argcount]) + ")"
        )
        doc = value.__doc__
        result["documentation"] = doc[: min(limit, 16384)] if type(doc) is str else ""
    elif cls is types.BuiltinFunctionType:
        result["preview"] = "builtin " + value.__name__
        doc = value.__doc__
        signature = value.__text_signature__
        result["documentation"] = doc[: min(limit, 16384)] if type(doc) is str else ""
        if type(signature) is str:
            result["signature"] = value.__name__ + signature[:1024]
    elif cls is type:
        name = type.__dict__["__name__"].__get__(value)
        doc = type.__dict__["__dict__"].__get__(value).get("__doc__")
        result["preview"] = "class " + name[:1024]
        result["documentation"] = doc[: min(limit, 16384)] if type(doc) is str else ""
    else:
        array = passive_array(value)
        if array is not None:
            result.update({key: array[key] for key in ("shape", "length", "nbytes", "dtype")})
            result["preview"] = f"<ndarray, {array['length']} elements>"
        else:
            result["preview"] = "<preview unavailable for this type>"
    return result


def passive_page(value: Any, offset: int = 0, size: int = 100) -> dict[str, Any]:
    if type(offset) is not int or offset < 0 or type(size) is not int or not 1 <= size <= 100:
        raise ValueError("invalid_page")
    if type(value) in (list, tuple):
        total = len(value)
        items = [passive_preview(item, 256) for item in value[offset : offset + size]]
    elif type(value) is dict:
        value = cast(dict[Any, Any], value)
        total = len(value)
        items = [
            {"key": passive_preview(key, 128), "value": passive_preview(item, 128)}
            for key, item in itertools.islice(value.items(), offset, offset + size)
        ]
    else:
        array = passive_array(value)
        if (
            array is None
            or array["hasobject"]
            or type(array["item_descriptor"]) is not types.MethodDescriptorType
        ):
            return {"items": [], "total": None, "next_offset": None, "unsupported": True}
        total = array["length"]
        flat = array["flat_descriptor"].__get__(value)
        items = [
            passive_preview(array["item_descriptor"](flat[index]), 256)
            for index in range(offset, min(offset + size, total))
        ]
    if offset > total:
        raise ValueError("invalid_page")
    return {
        "items": items,
        "total": total,
        "offset": offset,
        "next_offset": offset + size if offset + size < total else None,
    }


class NativeWidget:
    """Declared native subset, unrelated to the ipywidgets comm/model protocol."""

    def __init__(
        self,
        worker: Any,
        kind: str,
        value: Any = None,
        *,
        label: str = "",
        options: Any = (),
        lower: int = 0,
        upper: int = 100,
        callback: Any = None,
    ) -> None:
        if len(worker.widgets) >= 128:
            raise ValueError("Close an existing widget before creating more than 128 models")
        if kind not in ("button", "checkbox", "dropdown", "integer_slider", "progress"):
            raise ValueError(
                "Supported widgets: button, checkbox, dropdown, integer_slider, progress"
            )
        if type(label) is not str or len(label.encode()) > 1024:
            raise ValueError("Widget label exceeds 1024 bytes")
        if (
            type(options) not in (tuple, list)
            or len(options) > 100
            or any(type(option) is not str or len(option.encode()) > 1024 for option in options)
        ):
            raise ValueError("Dropdown options must be at most 100 bounded strings")
        if (
            type(lower) is not int
            or type(upper) is not int
            or not -(2**31) <= lower <= upper < 2**31
        ):
            raise ValueError("Widget bounds must be ordered signed 32-bit integers")
        if callback is not None and not callable(callback):
            raise TypeError("Widget callback must be callable")
        if callback is not None and inspect.iscoroutinefunction(callback):
            raise ValueError(
                "Native widget callbacks are synchronous; run asynchronous work in an accepted cell"
            )
        self.worker, self.kind, self.label = worker, kind, label
        self.options, self.lower, self.upper, self.callback = list(options), lower, upper, callback
        self.handle = str(uuid.uuid4())
        self.value = self.validate(value)
        if len(json_bytes(self.state())) > 65536:
            raise ValueError("Widget state exceeds 64 KiB")
        worker.widgets[self.handle] = self
        worker.widget_snapshots[self.handle] = json.loads(json_bytes(self.state()))

    def validate(self, value: Any) -> Any:
        if self.kind == "button":
            if value is not None:
                raise ValueError("Button events carry null, never replayed click counts")
        elif self.kind == "checkbox":
            if type(value) is not bool:
                raise ValueError("Checkbox value must be a boolean")
        elif self.kind == "dropdown":
            if type(value) is not str or value not in self.options:
                raise ValueError("Dropdown value is not a declared option")
        elif type(value) is not int or not self.lower <= value <= self.upper:
            raise ValueError(
                "Native numeric widgets accept bounded integers only; floats are unqualified"
            )
        return value

    def state(self) -> dict[str, Any]:
        return {
            "schema": "kernelledger.widget/1",
            "handle": self.handle,
            "worker_generation": self.worker.generation,
            "kind": self.kind,
            "label": self.label,
            "value": self.value,
            "options": self.options,
            "lower": self.lower,
            "upper": self.upper,
        }

    def _repr_mimebundle_(self) -> dict[str, Any]:
        return {"application/vnd.kernelledger.widget+json": self.state()}

    def __repr__(self) -> str:
        return f"<{self.kind}: {self.label}; recorded snapshot, recreate after restart>"

    def close(self) -> None:
        self.worker.widgets.pop(self.handle, None)
        self.worker.widget_snapshots.pop(self.handle, None)
        self.callback = None


class NotebookDebugger(bdb.Bdb):
    """Local stepping; only the paused execution thread reads frame objects."""

    def __init__(self, worker: Any) -> None:
        super().__init__()
        self.worker = worker
        self.requests: queue.Queue[Any] = queue.Queue(maxsize=32)
        self.paused = False
        self.owner: str | None = None
        self.snapshot: dict[str, Any] = {}
        self.configured: list[dict[str, Any]] = []
        self.post_mortem = False

    def user_line(self, frame: Any) -> None:
        if frame.f_code.co_filename not in self.worker.index.entries:
            return
        stack: list[Any] = []
        current = frame
        while current is not None and len(stack) < 32:
            if current.f_code.co_filename in self.worker.index.entries:
                stack.append(
                    {
                        "file": current.f_code.co_filename,
                        "source_revision": self.worker.index.revisions.get(
                            current.f_code.co_filename
                        ),
                        "line": current.f_lineno,
                        "function": current.f_code.co_name,
                    }
                )
            current = current.f_back
        self.snapshot = {
            "post_mortem": self.post_mortem,
            "source_context": [
                {"line": number, "current": number == frame.f_lineno, "text": text[:1024]}
                for number, text in enumerate(
                    linecache.getlines(frame.f_code.co_filename)[
                        max(0, frame.f_lineno - 4) : frame.f_lineno + 3
                    ],
                    max(1, frame.f_lineno - 3),
                )
            ],
            "stack": stack,
            "locals": [
                {"name": name[:1024], **passive_preview(value, 128)}
                for name, value in itertools.islice(frame.f_locals.items(), 100)
                if type(name) is str and name != "__builtins__"
            ],
            "source_revision": self.worker.index.revisions.get(
                frame.f_code.co_filename, self.worker.executed_hash
            ),
        }
        self.paused = True
        self.worker.channel.send("interactive_state", debug=self.snapshot)
        try:
            while True:
                try:
                    request = self.requests.get(timeout=0.05)
                except queue.Empty:
                    if self.worker.interrupted.is_set():
                        raise KeyboardInterrupt()
                    continue
                command = request["request"].get("command", "snapshot")
                owner = request["request"].get("controller_id")
                error = None
                if command != "snapshot" and self.owner not in (None, owner):
                    error = {
                        "code": "debug_claimed",
                        "message": "Another client owns debugger controls",
                    }
                elif command != "snapshot":
                    self.owner = owner
                    if self.post_mortem and command not in ("continue", "stop"):
                        error = {
                            "code": "post_mortem",
                            "message": "This frame already raised; continue or stop to finish the failed execution",
                        }
                    elif command == "continue":
                        self.set_continue()
                    elif command == "step":
                        self.set_step()
                    elif command == "next":
                        self.set_next(frame)
                    elif command == "out":
                        self.set_return(frame)
                    elif command == "stop":
                        self.set_quit()
                    else:
                        error = {"code": "invalid_request", "message": "Unknown debug control"}
                self.worker.channel.send(
                    "interactive_reply",
                    request_id=request.get("request_id"),
                    worker_generation=self.worker.generation,
                    **({"error": error} if error else {"debug": self.snapshot, "command": command}),
                )
                if error is None and command != "snapshot":
                    break
        finally:
            self.paused = False
            self.snapshot = {}
            self.worker.channel.send("interactive_state", debug=None)
            del frame, current


class Worker:
    def __init__(self, channel: Any) -> None:
        self.channel = channel
        self.index = ExecutionIndex()
        self.namespace: dict[Any, Any]
        self.namespace = {
            "__name__": "__main__",
            "display": self.display,
            "update_display": self.update_display,
            "clear_output": self.clear,
            "__kl_command__": self.notebook_command,
            "widget": self.widget,
            "__kl_widget_event__": self.widget_event,
            "display_table": self.display_table,
            "display_media": self.display_media,
        }
        self.internal_bindings = self.namespace.copy()
        self.exec_id, self.ordinal = None, 0
        self.commands: queue.Queue[Any] = queue.Queue()
        self.replies: queue.Queue[Any] = queue.Queue()
        self.blobs: dict[Any, Any]
        self.blobs = {}
        self.stdout, self.stderr = Stream(self, "stdout"), Stream(self, "stderr")
        self.protocol_thread = None
        self.interrupted = threading.Event()
        self.correlated_inputs = False
        self.input_replies: dict[str, queue.Queue[Any]] = {}
        self.input_lock = threading.Lock()
        self.cleaning_shell = False
        self.interruptible = False
        self.original_sleep = time.sleep
        self.capture: dict[str, Any] | None = None
        self.engine = CustomEngine(self)
        self.generation = str(uuid.uuid4())
        self.advanced = os.environ.pop("KL_EXPERIMENTAL_INTERACTIVITY", "") == "1"
        self.debugger = NotebookDebugger(self)
        self.executed_hash = ""
        self.widgets: dict[str, NativeWidget] = {}
        self.widget_snapshots: dict[str, Any] = {}

    def widget(self, kind: str, value: Any = None, **options: Any) -> NativeWidget:
        if not self.advanced:
            raise ValueError(
                "Native widgets are experimental; start the owner with KL_EXPERIMENTAL_INTERACTIVITY=1"
            )
        return NativeWidget(self, kind, value, **options)

    def display_table(self, rows: Any, columns: Any = ()) -> None:
        if type(rows) not in (list, tuple) or type(columns) not in (list, tuple):
            raise ValueError("display_table accepts built-in rows and column names")
        if len(columns) > 20:
            raise ValueError("Tables are limited to 20 columns per recorded page")
        page: list[Any] = []
        for row in rows[:100]:
            if type(row) not in (list, tuple) or len(row) > 20:
                raise ValueError("Each table row must be a built-in sequence of at most 20 items")
            page.append([passive_preview(item, 128)["preview"] for item in row])
        headers = [passive_preview(column, 128)["preview"] for column in columns]
        self.emit(
            "display",
            bundle={
                "application/vnd.kernelledger.table+json": {
                    "value": {
                        "columns": headers,
                        "rows": page,
                        "total_rows": len(rows),
                        "truncated": len(rows) > 100,
                    }
                },
                "text/plain": {
                    "value": "\n".join(" | ".join(row) for row in [headers, *page])
                    + (
                        "\n[first 100 rows; inspect the source variable for more]"
                        if len(rows) > 100
                        else ""
                    )
                },
            },
        )

    def display_media(self, data: bytes, mime: str) -> None:
        if type(data) is not bytes or len(data) > 32 * 1024 * 1024:
            raise ValueError("Media attachments must be bytes of at most 32 MiB")
        if mime not in ("audio/wav", "audio/mpeg", "audio/ogg", "video/mp4", "video/webm"):
            raise ValueError("Unsupported media attachment type")
        self.emit(
            "display",
            bundle={
                mime: {"blob": self.channel.blob(data), "encoding": "binary"},
                "text/plain": {
                    "value": f"<{mime} attachment, {len(data)} bytes; playback requires a qualified media renderer. Export to play externally.>"
                },
            },
        )

    def widget_event(self, generation: str, handle: str, value: Any) -> None:
        if generation != self.generation or handle not in self.widgets:
            raise ValueError("Stale widget; recreate it in the current session")
        widget = self.widgets[handle]
        if widget.kind == "progress":
            raise ValueError("Progress is read-only")
        widget.value = widget.validate(value)
        try:
            if widget.callback is not None:
                result = widget.callback(widget.value)
                if type(result) is types.CoroutineType:
                    result.close()
                    raise ValueError(
                        "Native widget callbacks must not return an unawaited coroutine"
                    )
        finally:
            self.widget_snapshots[handle] = json.loads(json_bytes(widget.state()))
            self.update_display(widget, widget.handle)

    def transient(self, message: Any, busy: bool = False) -> None:
        """Read responses are ephemeral and must never enter the execution ledger."""
        request = message.get("request", {})
        response: dict[str, Any] = {
            "request_id": message.get("request_id"),
            "worker_generation": self.generation,
            "source_revision": request.get("source_revision"),
            "namespace_epoch": self.engine.epoch,
            "truncated": False,
        }
        try:
            if request.get("worker_generation") != self.generation:
                raise ValueError("stale_generation")
            method = request.get("method")
            if busy:
                raise ValueError("busy")
            if any(
                thread not in (threading.main_thread(), self.protocol_thread)
                for thread in threading.enumerate()
            ):
                raise ValueError("background_work")
            if request.get("namespace_epoch", self.engine.epoch) != self.engine.epoch:
                raise ValueError("stale_epoch")
            if method == "widget.state":
                handle = request.get("handle")
                if not self.advanced or handle not in self.widget_snapshots:
                    raise ValueError("stale_widget")
                response["model"] = self.widget_snapshots[handle]
            elif method == "debug.breakpoints":
                if not self.advanced:
                    raise ValueError("unsupported_capability")
                entries = request.get("breakpoints", [])
                if type(entries) is not list:
                    raise ValueError("invalid_breakpoints")
                entries = cast(list[Any], entries)
                if len(entries) > 100:
                    raise ValueError("invalid_breakpoints")
                for raw_entry in entries:
                    if type(raw_entry) is not dict:
                        raise ValueError("invalid_breakpoints")
                    entry = cast(dict[str, Any], raw_entry)
                    if (
                        type(entry) is not dict
                        or type(entry.get("line")) is not int
                        or not 1 <= entry["line"] <= 100000
                    ):
                        raise ValueError("invalid_breakpoints")
                    if type(entry.get("cell_id")) is not str or not re.fullmatch(
                        r"[A-Za-z0-9][A-Za-z0-9_-]{0,63}", entry["cell_id"]
                    ):
                        raise ValueError("invalid_breakpoints")
                    if type(entry.get("source_revision")) is not str or not re.fullmatch(
                        r"[0-9a-f]{64}", entry["source_revision"]
                    ):
                        raise ValueError("invalid_breakpoints")
                self.debugger.configured = entries
                response["breakpoints"] = entries
            elif method == "code.complete":
                source, cursor = request["source"], request["cursor"]
                if type(source) is not str or len(source.encode()) > 65536:
                    raise ValueError("source_too_large")
                if type(cursor) is not int or not 0 <= cursor <= len(source):
                    raise ValueError("invalid_cursor")
                before = source[:cursor]
                token_start = len(before)
                while token_start and (
                    before[token_start - 1] == "." or ("a" + before[token_start - 1]).isidentifier()
                ):
                    token_start -= 1
                token = before[token_start:]
                prefix = token.rsplit(".", 1)[-1]
                start = cursor - len(prefix)
                prefix = unicodedata.normalize("NFKC", prefix)
                names: Any = itertools.chain(keyword.kwlist, vars(builtins), self.namespace)
                if "." in token:
                    path = unicodedata.normalize("NFKC", token.rsplit(".", 1)[0]).split(".")
                    if any(not name.isidentifier() for name in path):
                        raise ValueError("unsupported_expression")
                    value = self.namespace.get(path[0], vars(builtins).get(path[0]))
                    for name in path[1:]:
                        value = passive_attributes(value).get(name)
                    names = passive_attributes(value)
                if before.lstrip().startswith("%") and " " not in before.strip():
                    prefix = before.strip().lstrip("%")
                    start = cursor - len(prefix)
                    names = CELL_MAGICS if before.lstrip().startswith("%%") else MAGICS
                candidates: list[str] = []
                seen: set[str] = set()
                candidate_bytes = 0
                for name in itertools.islice(names, 10000):
                    if (
                        type(name) is str
                        and len(name) <= 1024
                        and name.isidentifier()
                        and name.startswith(prefix)
                        and name not in seen
                    ):
                        if len(candidates) == 100:
                            response["truncated"] = True
                            break
                        if len(name.encode()) <= 1024:
                            if candidate_bytes + len(name.encode()) > 48000:
                                response["truncated"] = True
                                break
                            candidates.append(name)
                            candidate_bytes += len(name.encode())
                            seen.add(name)
                response.update(
                    start=start, end=cursor, candidates=sorted(candidates), origin="idle_namespace"
                )
            elif method in ("namespace.list", "namespace.preview", "code.inspect"):
                if method == "namespace.list":
                    cursor = request.get("cursor", "")
                    offset = 0
                    if cursor:
                        epoch, position = cursor.split(":", 1)
                        if epoch != str(self.engine.epoch):
                            raise ValueError("stale_epoch")
                        offset = int(position)
                    if not 0 <= offset <= len(self.namespace):
                        raise ValueError("invalid_cursor")
                    size = request.get("page_size", 100)
                    if type(size) is not int or not 1 <= size <= 100:
                        raise ValueError("invalid_page_size")
                    items: list[Any] = []
                    scanned = offset
                    for name, value in itertools.islice(
                        self.namespace.items(), offset, offset + size
                    ):
                        scanned += 1
                        if type(name) is not str or len(name) > 1024:
                            response["truncated"] = True
                            continue
                        if name in self.internal_bindings and self.internal_bindings[name] is value:
                            continue
                        if name == "__builtins__":
                            continue
                        metadata = passive_preview(value, 256)
                        metadata.pop("documentation", None)
                        if len(name.encode()) > 1024:
                            response["truncated"] = True
                            continue
                        if (
                            len(json_bytes(items))
                            + len(json_bytes(metadata))
                            + 2 * len(name.encode())
                            > 48000
                        ):
                            scanned -= 1
                            response["truncated"] = True
                            break
                        items.append(
                            {
                                "name": name[:1024],
                                "handle": f"{self.engine.epoch}:{name}",
                                **metadata,
                            }
                        )
                    response.update(
                        items=items,
                        cursor=f"{self.engine.epoch}:{scanned}"
                        if scanned < len(self.namespace)
                        else None,
                    )
                else:
                    name = request.get("name", "")
                    handle = request.get("handle")
                    if handle:
                        epoch, name = handle.split(":", 1)
                        if epoch != str(self.engine.epoch):
                            raise ValueError("stale_epoch")
                    path = unicodedata.normalize("NFKC", name).split(".")
                    if not all(part.isidentifier() for part in path):
                        raise ValueError("unsupported_expression")
                    if path[0] not in self.namespace and path[0] not in vars(builtins):
                        raise ValueError("name_not_found")
                    value = self.namespace.get(path[0], vars(builtins).get(path[0]))
                    for part in path[1:]:
                        attrs = passive_attributes(value)
                        if part not in attrs:
                            raise ValueError("name_not_found")
                        value = attrs[part]
                    response.update(name=name, **passive_preview(value))
                    if method == "namespace.preview":
                        response["page"] = passive_page(
                            value, request.get("offset", 0), request.get("page_size", 100)
                        )
            else:
                raise ValueError("unsupported_capability")
        except (ValueError, KeyError, TypeError) as error:
            response["error"] = {"code": str(error), "message": str(error), "retryable": False}
        if len(json_bytes(response)) > 256 * 1024:
            response = {
                "request_id": message.get("request_id"),
                "worker_generation": self.generation,
                "error": {"code": "too_large", "message": "Read reply exceeds 256 KiB"},
            }
        self.channel.send("interactive_reply", **response)

    def sleep(self, seconds: Any) -> None:
        if self.cleaning_shell or threading.current_thread() is not threading.main_thread():
            self.original_sleep(seconds)
        elif self.interrupted.wait(seconds):
            raise KeyboardInterrupt()

    def emit(self, kind: Any, **fields: Any) -> Any:
        if kind not in ("stdout", "stderr"):
            self.stdout.flush()
            self.stderr.flush()
        foreground = (
            threading.current_thread() is threading.main_thread()
            or threading.current_thread().name.startswith("_kl_shell_")
        )
        origin = fields.pop("_origin", self.exec_id if foreground else None)
        if (
            self.capture is not None
            and origin == self.exec_id
            and kind
            in (
                "stdout",
                "stderr",
                "result",
                "display",
                "display_update",
            )
        ):
            text = fields.get(
                "text", fields.get("bundle", {}).get("text/plain", {}).get("value", "")
            )
            remaining = max(0, 65536 - len(self.capture["text"].encode()))
            encoded = str(text).encode()
            self.capture["text"] += encoded[:remaining].decode("utf-8", errors="ignore")
            self.capture["truncated"] |= len(encoded) > remaining
            return
        self.channel.send(kind, exec_id=origin, **fields)

    def bundle(self, obj: Any) -> Any:
        return self.mimebundle(obj)[0]

    def mimebundle(self, obj: Any) -> tuple[Any, Any]:
        bundle: dict[Any, Any]
        bundle = {}
        metadata: dict[str, Any] = {}
        try:
            if hasattr(obj, "_repr_mimebundle_"):
                value = obj._repr_mimebundle_()
                if (
                    isinstance(value, tuple)
                    and len(cast(tuple[Any, ...], value)) > 1
                    and type(cast(Any, value[1])) is dict
                ):
                    metadata = cast(dict[str, Any], value[1])
                bundle.update(
                    cast(
                        dict[str, Any],
                        (cast(tuple[Any, ...], value)[0] if value else {})
                        if isinstance(value, tuple)
                        else value or {},
                    )
                )
            else:
                for suffix, mime in [
                    ("html", "text/html"),
                    ("markdown", "text/markdown"),
                    ("svg", "image/svg+xml"),
                    ("png", "image/png"),
                    ("jpeg", "image/jpeg"),
                    ("latex", "text/latex"),
                    ("json", "application/json"),
                ]:
                    method = getattr(obj, "_repr_" + suffix + "_", None)
                    if method:
                        try:
                            value = method()
                            if value is not None:
                                bundle[mime] = value[0] if isinstance(value, tuple) else value
                                if (
                                    isinstance(value, tuple)
                                    and len(cast(tuple[Any, ...], value)) > 1
                                    and type(cast(Any, value[1])) is dict
                                ):
                                    metadata[mime] = cast(Any, value[1])
                        except Exception as error:
                            self.emit("stderr", text=f"{suffix} repr failed: {error}\n")
        except Exception as error:
            self.emit("stderr", text=f"mime repr failed: {error}\n")
        try:
            if "text/plain" not in bundle:
                formatter = reprlib.Repr()
                formatter.maxstring, formatter.maxother = 65536, 65536
                formatter.maxlist = formatter.maxtuple = formatter.maxdict = 100
                bundle["text/plain"] = (
                    formatter.repr(obj)
                    if type(obj) in (list, tuple, dict, set, frozenset, str, bytes)
                    else repr(obj)
                )
        except Exception as error:
            bundle["text/plain"] = f"<repr failed: {error}>"
        result: dict[Any, Any]
        result = {}
        for mime, value in bundle.items():
            if type(mime) is not str or len(mime) > 256:
                continue
            if isinstance(value, bytes):
                if len(value) > 32 * 1024 * 1024:
                    self.emit("stderr", text=f"{mime} output exceeds the 32 MiB attachment limit\n")
                    continue
                result[mime] = {"blob": self.channel.blob(value), "encoding": "binary"}
            else:
                if isinstance(value, str) and len(value) > 65536:
                    value = value[:65536] + "\n[output truncated at 65536 characters]"
                data = value.encode() if isinstance(value, str) else json_bytes(value)
                if len(data) > 256 * 1024:
                    self.emit(
                        "stderr",
                        text=f"{mime} output exceeds the 256 KiB structured output limit\n",
                    )
                    continue
                result[mime] = (
                    {"value": value}
                    if len(data) < INLINE // 2
                    else {
                        "blob": self.channel.blob(data),
                        "encoding": "utf8" if isinstance(value, str) else "json",
                    }
                )
        if len(json_bytes(metadata)) > 65536:
            metadata = {"kernelledger": {"metadata_truncated": True}}
        return result, metadata

    def display(self, obj: Any, display_id: Any = None) -> Any:
        if type(obj) is NativeWidget and display_id is None:
            display_id = obj.handle
        bundle, metadata = self.mimebundle(obj)
        self.emit("display", bundle=bundle, metadata=metadata, display_id=display_id)

    def update_display(self, obj: Any, display_id: Any) -> Any:
        bundle, metadata = self.mimebundle(obj)
        self.emit("display_update", bundle=bundle, metadata=metadata, display_id=display_id)

    def clear(self, wait: Any = False) -> Any:
        self.emit("clear", wait=bool(wait))

    def notebook_command(
        self,
        command: str,
        mode: str = "shell",
        capture: bool = False,
        row: int = 1,
        filename: str = "<command>",
    ) -> Any:
        if mode == "magic":
            parts = command.split(maxsplit=1)
            name = parts[0] if parts else ""
            argument = parts[1] if len(parts) > 1 else ""
            if name in ("time", "timeit", "prun"):
                return self.engine.inline(command, row, filename)
            if name == "history":
                return self.engine.history.configure(argument)
            if name == "commands":
                return "\n".join(
                    [
                        *(f"%{n}: {help}" for n, help in MAGICS.items()),
                        *(f"%%{n}: {help}" for n, help in CELL_MAGICS.items()),
                    ]
                )
            if name == "inspect":
                path, *options = argument.split()
                names = unicodedata.normalize("NFKC", path).split(".")
                if not all(part.isidentifier() for part in names):
                    raise ValueError("Passive help accepts names, not expressions")
                if names[0] not in self.namespace and names[0] not in vars(builtins):
                    raise NameError(path)
                value = self.namespace.get(names[0], vars(builtins).get(names[0]))
                for part in names[1:]:
                    value = passive_attributes(value)[part]
                metadata = passive_preview(value)
                text = (
                    metadata.get("signature", metadata["type"])
                    + "\n"
                    + metadata.get("documentation", metadata["preview"])
                )
                if options == ["source"] and type(value) in (types.FunctionType, type):
                    try:
                        text += "\n" + self.index.source(value)[0][:65536]
                    except Excluded:
                        text += "\nRecorded source unavailable"
                return text
            if name == "run":
                parts = shlex.split(argument, posix=os.name != "nt")
                if len(parts) != 1:
                    raise ValueError(
                        "%run accepts one explicit file; arguments and isolated namespaces are unsupported"
                    )
                path = pathlib.Path(parts[0].strip("'\"")).resolve()
                if path.stat().st_size > 8 * 1024 * 1024:
                    raise ValueError("%run source is limited to 8 MiB")
                with path.open("rb") as stream:
                    raw = stream.read(8 * 1024 * 1024 + 1)
                if len(raw) > 8 * 1024 * 1024:
                    raise ValueError("%run source is limited to 8 MiB")
                source = raw.decode("utf-8-sig")
                digest = self.channel.blob(raw)
                self.emit(
                    "display",
                    bundle={
                        "text/plain": {
                            "value": f"Executed file: {path} (sha256 {digest}); external effects are not undone by restore"
                        },
                        "application/vnd.kernelledger.executed-source+json": {
                            "value": {
                                "path": str(path),
                                "cwd": os.getcwd(),
                                "sha256": digest,
                                "source": {"blob": digest, "encoding": "binary"},
                            }
                        },
                    },
                )
                nested = str(path) + "@" + digest
                tree = self.index.add(nested, source, self.ordinal)
                return self.engine.run_tree(tree, nested)
            if name == "pwd":
                return os.getcwd()
            if name == "cd":
                path = argument.strip()
                if len(path) >= 2 and path[0] == path[-1] and path[0] in ("'", '"'):
                    path = path[1:-1]
                os.chdir(os.path.expanduser(os.path.expandvars(path or "~")))
                print(os.getcwd())
                return None
            if name != "pip":
                raise ValueError(
                    f"Unsupported notebook magic %{name}. Supported: "
                    + ", ".join("%" + n for n in MAGICS)
                )
            command = "pip " + argument
        command = command.strip()
        if not command:
            raise ValueError("A command is required after !")
        if command.endswith("&") and not command.endswith("&&"):
            raise ValueError("Background shell commands are not supported")
        executable = re.match(r"^(pip3?|python3?|\{sys\.executable\})(?=\s|$)", command)
        package_command = executable is not None and executable[1].startswith("pip")
        if package_command and importlib.util.find_spec("pip") is None:
            print("Setting up pip in the notebook environment...", flush=True)
            self.notebook_command("python -m ensurepip --upgrade")
            importlib.invalidate_caches()
        python = windows_shell_path(sys.executable) if os.name == "nt" else sys.executable
        if executable:
            args = [python, "-m", "pip"] if package_command else [python]
            prefix = subprocess.list2cmdline(args) if os.name == "nt" else shlex.join(args)
            command = prefix + command[executable.end() :]
        env = {**os.environ, "PYTHONIOENCODING": "utf-8", "PYTHONUNBUFFERED": "1"}
        path = env.get("PATH", "")
        cwd = os.getcwd()
        if os.name == "nt":
            path = os.pathsep.join(windows_shell_path(entry) for entry in path.split(os.pathsep))
            cwd = windows_shell_path(cwd)
        env["PATH"] = str(pathlib.Path(python).parent) + os.pathsep + path
        if sys.prefix != sys.base_prefix:
            env["VIRTUAL_ENV"] = windows_shell_path(sys.prefix) if os.name == "nt" else sys.prefix
        else:
            env.pop("VIRTUAL_ENV", None)
        process = subprocess.Popen(
            command,
            shell=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT if capture else subprocess.PIPE,
            env=env,
            cwd=cwd,
            creationflags=0x08000000 if os.name == "nt" else 0,
        )
        captured: list[str] = []

        def forward(pipe: Any, stream: Stream) -> None:
            decoder = codecs.getincrementaldecoder("utf-8")(errors="replace")
            try:
                while chunk := pipe.read1(4096):
                    text = decoder.decode(chunk)
                    if capture:
                        captured.append(text)
                    else:
                        stream.write(text)
                        stream.flush()
                remaining = decoder.decode(b"", final=True)
                if capture:
                    captured.append(remaining)
                elif remaining:
                    stream.write(remaining)
            finally:
                pipe.close()

        readers = [
            threading.Thread(
                target=forward,
                args=(process.stdout, self.stdout),
                name="_kl_shell_stdout",
                daemon=True,
            )
        ]
        if not capture:
            readers.append(
                threading.Thread(
                    target=forward,
                    args=(process.stderr, self.stderr),
                    name="_kl_shell_stderr",
                    daemon=True,
                )
            )
        try:
            for reader in readers:
                reader.start()
            while process.poll() is None or any(reader.is_alive() for reader in readers):
                if self.interrupted.wait(0.05):
                    raise KeyboardInterrupt()
            status = process.wait()
            self.namespace["_exit_code"] = status
            if status:
                raise subprocess.CalledProcessError(status, command)
            return "".join(captured).splitlines() if capture else None
        except BaseException:
            # subprocess.wait uses time.sleep on POSIX. The notebook's patched
            # sleep must not re-raise the same interrupt while we kill/reap its
            # shell children. Further soft interrupts coalesce during cleanup;
            # supervisor hard kill remains available.
            self.cleaning_shell = True
            try:
                stop_shell(process)
            finally:
                self.cleaning_shell = False
            raise
        finally:
            for reader in readers:
                if reader.ident is not None:
                    reader.join(timeout=5)
            if package_command:
                importlib.invalidate_caches()

    def input(self, prompt: Any = "", password: Any = False) -> Any:
        if threading.current_thread() is not threading.main_thread():
            raise RuntimeError("Notebook input requires the foreground execution thread")
        input_id = str(uuid.uuid4()) if self.correlated_inputs else None
        replies: queue.Queue[Any] = self.replies if input_id is None else queue.Queue()
        try:
            if input_id is not None:
                with self.input_lock:
                    self.input_replies[input_id] = replies
            self.emit(
                "stdin_request",
                prompt=str(prompt),
                password=password,
                **({"input_id": input_id} if input_id is not None else {}),
            )
            reply = replies.get()
        finally:
            if input_id is not None:
                with self.input_lock:
                    self.input_replies.pop(input_id, None)
        if reply.get("error"):
            raise RuntimeError(reply["error"])
        return reply["text"]

    def input_reply(self, message: Any) -> None:
        input_id = message.get("input_id")
        if input_id is None:
            self.replies.put(message)
        else:
            with self.input_lock:
                replies = self.input_replies.get(input_id)
                if replies is not None:
                    replies.put(message)

    def show_figures(self, *args: Any, **kwargs: Any) -> Any:
        self.figures()

    def figures(self) -> Any:
        plt: Any = sys.modules.get("matplotlib.pyplot")
        if plt is None:
            return
        for number in plt.get_fignums():
            buf = io.BytesIO()
            plt.figure(number).savefig(buf, format="png")
            self.emit(
                "display",
                bundle={
                    "image/png": {"blob": self.channel.blob(buf.getvalue()), "encoding": "binary"},
                    "text/plain": {"value": "<matplotlib figure>"},
                },
            )
            plt.close(number)

    def execute(self, message: Any) -> Any:
        self.interrupted.clear()
        self.correlated_inputs = message.get("correlated_inputs", False)
        self.exec_id = message["exec_id"]
        self.interruptible = True
        self.ordinal += 1
        filename = (
            f"cells/{message['cell_id']}.py@{self.exec_id}"
            if message.get("cell_id")
            else f"<unattributed>@{self.exec_id}"
        )
        state = "ok"
        ran_foreground = False
        try:
            self.emit(
                "status",
                state="running",
                origin=message.get(
                    "origin", "notebook" if message.get("cell_id") else "unattributed"
                ),
            )
            if hashlib.sha256(message["source"].encode()).hexdigest() != message["source_sha256"]:
                raise ValueError("source hash mismatch")
            if asyncio.all_tasks(self.engine.loop):
                raise RuntimeError(
                    "Previous tasks refused cancellation; restart or terminate this worker before another execution"
                )
            tree = self.index.add(filename, message["source"], self.ordinal)
            self.executed_hash = message["source_sha256"]
            if message.get("debug", False):
                if not self.advanced:
                    raise ValueError(
                        "Debugger is experimental; start the owner with KL_EXPERIMENTAL_INTERACTIVITY=1"
                    )
                self.debugger.reset()
                self.debugger.owner = None
                for breakpoint in self.debugger.configured:
                    prefix = f"cells/{breakpoint['cell_id']}.py@"
                    for indexed, revision in self.index.revisions.items():
                        if indexed.startswith(prefix) and revision == breakpoint["source_revision"]:
                            linecache.cache[self.debugger.canonic(indexed)] = linecache.cache[
                                indexed
                            ]
                            error = self.debugger.set_break(indexed, breakpoint["line"])
                            if error:
                                raise ValueError(error)
                sys.settrace(self.debugger.trace_dispatch)
            ran_foreground = True
            result = self.engine.execute(tree, message["source"], filename)
            suppressed = suppress_result(message["source"])
            self.engine.history.record(self.ordinal, message["source"], result, suppressed)
            if result is not None and not suppressed:
                bundle, metadata = self.mimebundle(result)
                self.emit(
                    "result",
                    bundle=bundle,
                    metadata=metadata,
                    **({"display_id": result.handle} if type(result) is NativeWidget else {}),
                )
            plt: Any = sys.modules.get("matplotlib.pyplot")
            if plt is not None:
                plt.show = self.show_figures
        except BaseException as error:
            self.interruptible = False
            sys.settrace(None)
            if (
                message.get("debug")
                and self.advanced
                and not isinstance(error, (KeyboardInterrupt, bdb.BdbQuit))
            ):
                tb = error.__traceback__
                frame = None
                while tb:
                    if tb.tb_frame.f_code.co_filename in self.index.entries:
                        frame = tb.tb_frame
                    tb = tb.tb_next
                if frame is not None:
                    self.debugger.post_mortem = True
                    try:
                        self.debugger.user_line(frame)
                    except KeyboardInterrupt:
                        pass
                    finally:
                        self.debugger.post_mortem = False
                        del frame, tb
            state = (
                "interrupted" if isinstance(error, (KeyboardInterrupt, bdb.BdbQuit)) else "error"
            )
            self.emit(
                "error",
                ename=type(error).__name__,
                evalue=str(error),
                traceback=traceback.format_exception(error),
            )
        finally:
            self.interruptible = False
            sys.settrace(None)
            # Bdb.reset performs filesystem I/O via linecache.checkcache. It
            # must not run in terminal publication, where a late cancellation
            # previously escaped execute() and disconnected a healthy worker.
            self.debugger.botframe = None
            self.debugger.stopframe = None
            self.debugger.returnframe = None
            self.debugger.clear_all_breaks()
            self.debugger.snapshot = {}
            self.debugger.owner = None
            self.engine.epoch += 1
            try:
                self.interruptible = True
                if ran_foreground:
                    self.engine.cancel_tasks()
            except (RuntimeError, KeyboardInterrupt) as error:
                self.interruptible = False
                self.emit("stderr", text=str(error) + "\n")
                if isinstance(error, KeyboardInterrupt):
                    state = "interrupted"
            finally:
                self.interruptible = False
            try:
                self.figures()
            except Exception as error:
                self.emit("stderr", text=f"figure capture failed: {error}\n")
            self.emit(
                "status",
                state="idle",
                outcome=state,
                namespace_epoch=self.engine.epoch,
                history_cache=self.engine.history.enabled,
                background_tasks=bool(asyncio.all_tasks(self.engine.loop)),
            )
            self.exec_id = None

    def checkpoint(self, message: Any) -> Any:
        if asyncio.all_tasks(self.engine.loop):
            self.emit("checkpoint_offer", refused="background_tasks")
            return
        bad: list[Any]
        bad = []
        for thread in threading.enumerate():
            typename = type(thread).__module__ + "." + type(thread).__qualname__
            if (
                thread not in (threading.main_thread(), self.protocol_thread)
                and thread.name not in message.get("thread_allowlist", [])
                and typename not in message.get("thread_allowlist", [])
            ):
                bad.append(thread.name)
        if bad:
            self.emit("checkpoint_offer", refused="background_threads", threads=bad)
            return
        try:
            started = time.perf_counter()
            encoder = GraphEncoder(self.index, **message.get("limits", {}))
            manifest = encoder.checkpoint(
                self.namespace, message.get("denylist", []), self.internal_bindings
            )
            if self.engine.future_flags:
                manifest["notes"].append(
                    "Compiler future flags belong to the session and reset after namespace restore; repeat relevant __future__ imports in the new session."
                )
            walk_seconds = time.perf_counter() - started
            transfer_started = time.perf_counter()
            for data in encoder.blobs.values():
                self.channel.blob(data)
            self.emit(
                "checkpoint_offer",
                manifest=manifest,
                walk_seconds=walk_seconds,
                transfer_seconds=time.perf_counter() - transfer_started,
            )
        except OverflowError:
            self.emit("checkpoint_offer", refused="payload_limit")
        except Exception as error:
            self.emit("checkpoint_offer", failed=str(error))

    def training_command(self, message: Any) -> None:
        # Ordinary worker startup stays package/framework independent. Capture
        # discovers only a plugin explicitly imported by notebook source.
        try:
            if message["kind"] == "training_restore":
                import importlib

                plugin = importlib.import_module("kernelledger.gpu.notebook")
                training = plugin.reconstruct(
                    message["source"],
                    message["snapshot"],
                    trust=message.get("trust", False),
                    device=message.get("device", "cpu"),
                )
                self.namespace["training"] = training
                report = {
                    key: value for key, value in training.restore_report.items() if key != "objects"
                }
                result = {**plugin.status(), "restore_report": report}
            else:
                plugin = sys.modules.get("kernelledger.gpu.notebook")
                if plugin is None:
                    raise ValueError(
                        "no training plugin registered; explicitly import and register it first"
                    )
                result = plugin.capture(message["destination"])
            self.emit("training_result", result=result)
        except Exception as error:
            self.emit("training_result", failed=str(error))

    def reader(self) -> Any:
        try:
            while True:
                message = receive(self.channel.sock)
                kind = message["kind"]
                if kind == "blob":
                    self.blobs[message["sha256"]] = message["data"]
                elif kind == "envelope":
                    envelope_id = message["blob"]
                    message = json.loads(self.blobs[envelope_id])
                    if message["kind"] == "stdin_reply":
                        self.blobs.pop(envelope_id, None)
                        self.input_reply(message)
                    else:
                        self.commands.put(message)
                elif kind == "interrupt":
                    if self.exec_id and not self.cleaning_shell:
                        # Queue the Python interrupt before waking cooperative
                        # waits. Waking first can raise KeyboardInterrupt in the
                        # main thread, then interrupt its child-process cleanup
                        # a second time when interrupt_main finally runs.
                        if self.interruptible:
                            _thread.interrupt_main()
                        self.interrupted.set()
                elif kind == "stdin_reply":
                    self.input_reply(message)
                elif kind == "interactive_read" and self.exec_id is not None:
                    if (
                        message.get("request", {}).get("method") == "debug.control"
                        and self.debugger.paused
                    ):
                        if message["request"].get("worker_generation") != self.generation:
                            self.transient(message, busy=True)
                        else:
                            try:
                                self.debugger.requests.put_nowait(message)
                            except queue.Full:
                                self.transient(message, busy=True)
                    else:
                        self.transient(message, busy=True)
                else:
                    self.commands.put(message)
        except (EOFError, OSError, ValueError):
            if os.name != "nt":
                os.killpg(os.getpgrp(), signal.SIGKILL)
            os._exit(1)

    def run(self) -> Any:
        import getpass

        builtins.input = self.input
        getpass.getpass = lambda prompt="Password: ", stream=None: self.input(prompt, True)
        time.sleep = self.sleep
        sys.stdout, sys.stderr = self.stdout, self.stderr
        self.protocol_thread = threading.Thread(
            target=self.reader, name="_kl_protocol", daemon=True
        )
        self.protocol_thread.start()
        self.channel.send("ready")
        while True:
            message = self.commands.get()
            kind = message["kind"]
            if kind == "execute":
                self.execute(message)
            elif kind == "checkpoint":
                self.checkpoint(message)
            elif kind == "interactive_read":
                self.transient(message)
            elif kind in ("training_checkpoint", "training_restore"):
                self.training_command(message)
            elif kind == "restore":
                loader = GraphLoader(message["manifest"], self.blobs, self.namespace, self.index)
                self.emit("restore_result", **loader.load())
                self.blobs.clear()
                self.engine.epoch += 1
                self.emit("ready", namespace_epoch=self.engine.epoch)
            elif kind == "shutdown":
                self.engine.close()
                return


def main() -> Any:
    if sys.implementation.name != "cpython" or sys.version_info < (3, 11):
        raise SystemExit("worker requires CPython 3.11 or newer")
    port, token = int(os.environ.pop("KL_WORKER_PORT")), os.environ.pop("KL_WORKER_TOKEN")
    project = sys.argv[1]
    os.chdir(project)
    sys.path.insert(0, project)
    os.environ.setdefault("MPLBACKEND", "agg")
    channel = Channel(socket.create_connection(("127.0.0.1", port)))
    worker = Worker(channel)
    channel.send(
        "hello",
        token=token,
        python=platform.python_version(),
        implementation=sys.implementation.name,
        executable=sys.executable,
        platform=platform.platform(),
        cwd=os.getcwd(),
        engine=ENGINE_IDENTITY,
        interactive_features=[
            feature
            for feature in INTERACTIVE_FEATURES
            if worker.advanced or feature not in ("debugger", "native_widgets_v1")
        ],
        worker_generation=worker.generation,
    )
    worker.run()


if __name__ == "__main__":
    main()
