"""Standalone worker. Intentionally imports no KernelLedger modules."""

import _thread
import ast
import base64
import builtins
import codecs
import datetime as dt
import decimal
import enum
import fractions
import hashlib
import importlib
import importlib.util
import io
import json
import linecache
import math
import os
import pathlib
import platform
import queue
import random
import re
import shlex
import signal
import socket
import struct
import subprocess
import sys
import threading
import time
import tokenize
import traceback
import types
import uuid
import zoneinfo
from typing import Any, cast

SCHEMA = 1
INLINE = 65536


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
        data = recv_exact(sock, message["byte_size"])
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
            self.sock.sendall(struct.pack(">I", len(header)) + header + data)
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
                if token.string not in ("!", "%") or depth or logical_row != token.start[0]:
                    continue
                row, column = token.start
                original = lines[row - 1]
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
                    f"{match['indent']}{target}__kl_command__({command!r}, {mode!r}, {capture!r}){ending}"
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
        self.entries: dict[Any, Any]
        self.entries = {}
        self.captured: dict[Any, Any]
        self.captured = {}

    def add(self, filename: Any, source: Any, ordinal: Any) -> Any:
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

    def write(self, value: Any) -> Any:
        if not isinstance(value, str):
            raise TypeError("write requires a string")
        with self.lock:
            self.buffered.append(value)
            self.size += len(value.encode())
            if self.size >= 32768 or time.monotonic() - self.last >= 0.05:
                self._flush()
        return len(value)

    def _flush(self) -> Any:
        if self.buffered:
            self.worker.emit(self.kind, text="".join(self.buffered))
        self.buffered: list[Any]
        self.buffered, self.size, self.last = [], 0, time.monotonic()

    def flush(self) -> Any:
        with self.lock:
            self._flush()

    def isatty(self) -> Any:
        return False


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
        self.original_sleep = time.sleep

    def sleep(self, seconds: Any) -> None:
        if threading.current_thread() is not threading.main_thread():
            self.original_sleep(seconds)
        elif self.interrupted.wait(seconds):
            raise KeyboardInterrupt()

    def emit(self, kind: Any, **fields: Any) -> Any:
        if kind not in ("stdout", "stderr"):
            self.stdout.flush()
            self.stderr.flush()
        self.channel.send(kind, exec_id=self.exec_id, **fields)

    def bundle(self, obj: Any) -> Any:
        bundle: dict[Any, Any]
        bundle = {}
        try:
            if hasattr(obj, "_repr_mimebundle_"):
                value = obj._repr_mimebundle_()
                bundle.update(
                    cast(dict[str, Any], value[0] if isinstance(value, tuple) else value or {})
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
                        except Exception as error:
                            self.emit("stderr", text=f"{suffix} repr failed: {error}\n")
        except Exception as error:
            self.emit("stderr", text=f"mime repr failed: {error}\n")
        try:
            bundle["text/plain"] = repr(obj)
        except Exception as error:
            bundle["text/plain"] = f"<repr failed: {error}>"
        result: dict[Any, Any]
        result = {}
        for mime, value in bundle.items():
            if isinstance(value, bytes):
                result[mime] = {"blob": self.channel.blob(value), "encoding": "binary"}
            else:
                data = value.encode() if isinstance(value, str) else json_bytes(value)
                result[mime] = (
                    {"value": value}
                    if len(data) < INLINE // 2
                    else {
                        "blob": self.channel.blob(data),
                        "encoding": "utf8" if isinstance(value, str) else "json",
                    }
                )
        return result

    def display(self, obj: Any, display_id: Any = None) -> Any:
        self.emit("display", bundle=self.bundle(obj), display_id=display_id)

    def update_display(self, obj: Any, display_id: Any) -> Any:
        self.emit("display_update", bundle=self.bundle(obj), display_id=display_id)

    def clear(self, wait: Any = False) -> Any:
        self.emit("clear", wait=bool(wait))

    def notebook_command(self, command: str, mode: str = "shell", capture: bool = False) -> Any:
        if mode == "magic":
            parts = command.split(maxsplit=1)
            name = parts[0] if parts else ""
            argument = parts[1] if len(parts) > 1 else ""
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
                raise ValueError(f"Unsupported notebook magic %{name}. Supported: %pip, %cd, %pwd.")
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
            stop_shell(process)
            raise
        finally:
            for reader in readers:
                if reader.ident is not None:
                    reader.join(timeout=5)
            if package_command:
                importlib.invalidate_caches()

    def input(self, prompt: Any = "", password: Any = False) -> Any:
        self.emit("stdin_request", prompt=str(prompt), password=password)
        reply = self.replies.get()
        if reply.get("error"):
            raise RuntimeError(reply["error"])
        return reply["text"]

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
        self.exec_id = message["exec_id"]
        self.ordinal += 1
        filename = (
            f"cells/{message['cell_id']}.py@{self.exec_id}"
            if message.get("cell_id")
            else f"<unattributed>@{self.exec_id}"
        )
        self.emit("status", state="running")
        state = "ok"
        try:
            if hashlib.sha256(message["source"].encode()).hexdigest() != message["source_sha256"]:
                raise ValueError("source hash mismatch")
            tree = self.index.add(filename, message["source"], self.ordinal)
            expression = (
                tree.body.pop() if tree.body and isinstance(tree.body[-1], ast.Expr) else None
            )
            previous = self.namespace.copy()
            exec(compile(tree, filename, "exec"), self.namespace)
            self.index.capture_classes(filename, self.namespace, previous)
            if expression:
                result = eval(
                    compile(ast.Expression(expression.value), filename, "eval"), self.namespace
                )
                if result is not None:
                    self.emit("result", bundle=self.bundle(result))
            plt: Any = sys.modules.get("matplotlib.pyplot")
            if plt is not None:
                plt.show = self.show_figures
        except BaseException as error:
            state = "interrupted" if isinstance(error, KeyboardInterrupt) else "error"
            self.emit(
                "error",
                ename=type(error).__name__,
                evalue=str(error),
                traceback=traceback.format_exception(error),
            )
        finally:
            try:
                self.figures()
            except Exception as error:
                self.emit("stderr", text=f"figure capture failed: {error}\n")
            self.emit("status", state="idle", outcome=state)
            self.exec_id = None

    def checkpoint(self, message: Any) -> Any:
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
            walk_seconds = time.perf_counter() - started
            for data in encoder.blobs.values():
                self.channel.blob(data)
            self.emit("checkpoint_offer", manifest=manifest, walk_seconds=walk_seconds)
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
                    message = json.loads(self.blobs[message["blob"]])
                    self.commands.put(message)
                elif kind == "interrupt":
                    if self.exec_id:
                        self.interrupted.set()
                        _thread.interrupt_main()
                elif kind == "stdin_reply":
                    self.replies.put(message)
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
            elif kind in ("training_checkpoint", "training_restore"):
                self.training_command(message)
            elif kind == "restore":
                loader = GraphLoader(message["manifest"], self.blobs, self.namespace, self.index)
                self.emit("restore_result", **loader.load())
                self.blobs.clear()
                self.emit("ready")
            elif kind == "shutdown":
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
    channel.send(
        "hello",
        token=token,
        python=platform.python_version(),
        implementation=sys.implementation.name,
        executable=sys.executable,
        platform=platform.platform(),
        cwd=os.getcwd(),
    )
    Worker(channel).run()


if __name__ == "__main__":
    main()
