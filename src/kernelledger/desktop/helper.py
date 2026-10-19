"""Offline desktop source and inspection server. Never launches notebook code."""

import argparse
import json
import os
import shutil
import struct
import subprocess
import sys
from pathlib import Path
from typing import Any

from .. import __version__
from ..blobs import BlobStore
from ..documents import export_document
from ..ledger import Ledger
from ..notebook import Notebook
from ..sharing import Bundle, descriptor, export_bundle, notebook_outputs, output_view
from ..source import SourceError, SourceService
from .contracts import validate
from .protocol import API, LIMIT, encode, error, hello, page, sql_page


def candidates(root: Any) -> Any:
    paths: set[str] = set()
    for name in ("python3", "python"):
        path = shutil.which(name)
        if path:
            paths.add(path)
    for parent in (Path(root), *Path(root).parents):
        path = parent / ".venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        if path.is_file():
            paths.add(str(path))
    for path in Path(root).glob(
        "*/.venv/Scripts/python.exe" if os.name == "nt" else "*/.venv/bin/python"
    ):
        paths.add(str(path))
    if os.name == "nt" and shutil.which("py"):
        try:
            result = subprocess.run(
                ["py", "-0p"], capture_output=True, text=True, timeout=10, creationflags=0x08000000
            )
            for line in result.stdout.splitlines():
                marker = line.find(":\\")
                if marker > 0:
                    paths.add(line[marker - 1 :].strip())
        except (OSError, subprocess.TimeoutExpired):
            pass
    return [{"path": path} for path in sorted(paths)]


def _cell_source(blobs: Any, item: Any) -> None:
    digest = item.get("source_sha256")
    if not isinstance(digest, str):
        return
    try:
        item["source"] = blobs.get(digest).decode()
    except (OSError, UnicodeError, ValueError):
        return


def _execution_list_sql(request: Any) -> tuple[str, tuple[Any, ...]]:
    session_id = request.get("session_id")
    if request.get("latest_per_cell") is True:
        where = "WHERE cell_id IS NOT NULL"
        args: tuple[Any, ...] = ()
        if session_id:
            where += " AND session_id=?"
            args = (session_id,)
        return (
            "SELECT e.*, "
            "(SELECT COUNT(*) FROM events ev WHERE ev.exec_id=e.exec_id) AS fold_events, "
            "(SELECT COALESCE(MAX(seq), 0) FROM events ev WHERE ev.exec_id=e.exec_id) AS fold_seq "
            "FROM executions e JOIN ("
            f"SELECT cell_id, MAX(rowid) AS latest_rowid FROM executions {where} GROUP BY cell_id"
            ") latest ON e.rowid=latest.latest_rowid ORDER BY e.session_id, e.ordinal",
            args,
        )
    if session_id:
        return (
            "SELECT * FROM executions WHERE session_id=? ORDER BY session_id,ordinal",
            (session_id,),
        )
    return "SELECT * FROM executions ORDER BY session_id,ordinal", ()


def validate_interpreter(path: str) -> Any:
    script = "import sys,json,importlib.util; print(json.dumps({'executable':sys.executable,'version':list(sys.version_info[:3]),'implementation':sys.implementation.name,'codecs':{n:importlib.util.find_spec(n) is not None for n in ['numpy','pandas','pyarrow']}}))"
    kwargs: dict[str, Any] = {"creationflags": 0x08000000} if os.name == "nt" else {}
    result = subprocess.run(
        [path, "-I", "-c", script],
        capture_output=True,
        text=True,
        timeout=10,
        **kwargs,
    )
    if result.returncode:
        raise SourceError(
            "invalid_request", "interpreter validation failed", {"stderr": result.stderr}
        )
    data = json.loads(result.stdout)
    data["supported"] = data["implementation"] == "cpython" and data["version"] >= [3, 11, 0]
    return data


class Helper:
    def __init__(self, root: Any):
        self.root = Path(root).resolve()
        self.source = SourceService(self.root)
        self.notebook = Notebook(self.root)
        self.blobs = BlobStore(self.root / ".kl", readonly=True)

    def dispatch(self, request: Any) -> Any:
        validate(request, helper=True)
        method = request["method"]
        if method in ("source.read", "source.watch"):
            return self.source.read()
        if method == "source.recover":
            return self.source.recover()
        if method == "source.mutate":
            return self.source.mutate(request)
        if method == "ping":
            return {"kind": "pong"}
        if method == "native.status":
            from ..gpu.backends.capabilities import inventory

            return {
                "refused": "native_backend_unavailable",
                "native_qualified": False,
                "capabilities": inventory(),
            }
        if method == "training.list":
            directory = self.root / ".kl/training-artifacts/roots"
            return page(
                [
                    {"snapshot": path.name, "mode": "portable-training", "native_qualified": False}
                    for path in sorted(directory.iterdir())
                    if path.is_file()
                ]
                if directory.exists()
                else [],
                request,
            )
        if method == "training.inspect":
            from ..artifacts import ArtifactStore
            from ..experiments import inspect_experiment

            snapshot = request["snapshot"]
            manifest = inspect_experiment(
                ArtifactStore(self.root / ".kl/training-artifacts", readonly=True), snapshot
            )
            return {
                **manifest,
                "snapshot": snapshot,
                "mode": "portable-training",
                "native_qualified": False,
            }
        if method == "interpreter.candidates":
            return {"items": candidates(self.root)}
        if method == "interpreter.validate":
            return validate_interpreter(request["path"])
        if method == "bundle.inspect":
            bundle = Bundle(request["path"])
            try:
                return bundle.inspect()
            finally:
                bundle.close()
        if method == "descriptor.inspect":
            return json.loads(Path(request["path"]).read_bytes())
        if method == "doctor":
            source = self.source.read()
            report: dict[str, Any] = {"source": source, "issues": []}
            if source["state"] == "ok":
                report.update(self.notebook.doctor())
            return report
        if method == "blob.read":
            digest = request["sha256"]
            try:
                data = self.blobs.get(digest)
            except FileNotFoundError:
                raise SourceError("not_found", "blob missing", {"sha256": digest}) from None
            offset, length = request.get("offset", 0), request.get("length", LIMIT)
            if (
                not isinstance(offset, int)
                or not isinstance(length, int)
                or offset < 0
                or not 0 < length <= LIMIT
            ):
                raise SourceError("invalid_request", "invalid blob range")
            return {
                "sha256": digest,
                "offset": offset,
                "total_size": len(data),
                "data": data[offset : offset + length],
                "verified": offset == 0 and length >= len(data),
            }
        path = self.root / ".kl/ledger.sqlite"
        if not path.exists():
            if method == "executions.watermark":
                return {
                    "executions": 0,
                    "execution_rowid": 0,
                    "events": 0,
                    "event_rowid": 0,
                    "max_end": "",
                }
            if method.endswith(".list"):
                return {"items": [], "cursor": None}
            if method == "export_ipynb":
                self.notebook.export_ipynb(request["path"])
                return {"path": request["path"]}
            if method == "export_document":
                return export_document(self.notebook, request["path"], request["format"])
            raise SourceError("not_found", "history is empty")
        immutable = (
            not os.access(path.parent, os.W_OK) and not (path.parent / "supervisor.json").exists()
        )
        ledger = Ledger(path, readonly=True, immutable=immutable)
        try:
            if method == "executions.watermark":
                # Executions alone miss event appends and status updates that keep the same rowid.
                row = ledger.rows(
                    "SELECT (SELECT COUNT(*) FROM executions) AS executions, "
                    "(SELECT COALESCE(MAX(rowid), 0) FROM executions) AS execution_rowid, "
                    "(SELECT COUNT(*) FROM events) AS events, "
                    "(SELECT COALESCE(MAX(rowid), 0) FROM events) AS event_rowid, "
                    "(SELECT COALESCE(MAX(end), '') FROM executions) AS max_end"
                )[0]
                return dict(row)
            if method in ("sessions.list", "executions.list", "checkpoints.list"):
                if method == "executions.list":
                    sql, args = _execution_list_sql(request)
                else:
                    table, order = {
                        "sessions.list": ("sessions", "start,session_id"),
                        "checkpoints.list": ("checkpoints", "timestamp,checkpoint_id"),
                    }[method]
                    sql, args = f"SELECT * FROM {table} ORDER BY {order}", ()
                listed = sql_page(ledger, sql, args, request)
                if method == "executions.list" and request.get("latest_per_cell") is not True:
                    for item in listed["items"]:
                        _cell_source(self.blobs, item)
                return {**listed, "read_only_snapshot": immutable}
            if method in ("execution.get", "fold.get"):
                return output_view(self.notebook, ledger, self.blobs, request["exec_id"])
            if method == "events.list":
                return page(
                    [
                        e
                        for e in ledger.events(request["exec_id"])
                        if e["seq"] > request.get("after_seq", 0)
                    ],
                    request,
                )
            if method == "checkpoint.get":
                return descriptor(ledger, self.blobs, request["checkpoint_id"])
            if method == "export_ipynb":
                outputs: Any = (
                    notebook_outputs(ledger, self.blobs, request["session_id"])
                    if request.get("session_id")
                    else {}
                )
                self.notebook.export_ipynb(request["path"], outputs)
                return {"path": request["path"]}
            if method == "export_document":
                return export_document(
                    self.notebook, request["path"], request["format"], ledger, self.blobs
                )
            if method == "bundle_export":
                return export_bundle(
                    self.notebook,
                    ledger,
                    self.blobs,
                    request["path"],
                    request.get("sessions"),
                    request.get("checkpoints"),
                    request.get("external"),
                )
            raise SourceError("invalid_request", "unknown helper request")
        finally:
            ledger.close()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project", required=True)
    args = parser.parse_args()
    if os.name == "nt":
        import msvcrt

        msvcrt.setmode(sys.stdin.fileno(), os.O_BINARY)
        msvcrt.setmode(sys.stdout.fileno(), os.O_BINARY)
    helper = Helper(args.project)
    authenticated = False
    while True:
        header = sys.stdin.buffer.read(4)
        if not header:
            break
        request: Any = {}
        data: bytes = b""
        fatal = False
        try:
            if len(header) != 4:
                break
            size = struct.unpack(">I", header)[0]
            if size > LIMIT:
                fatal = True
                raise SourceError("too_large", "JSON frame exceeds 1 MiB")
            raw = sys.stdin.buffer.read(size)
            if len(raw) != size:
                break
            request = json.loads(raw)
            if not authenticated:
                hello(request)
                authenticated = True
                response = {
                    "kind": "welcome",
                    "api": API,
                    "version": 1,
                    "capabilities": ["blob_ranges", "portable_training", "native_snapshot_status"],
                    "identity": {
                        "kernelledger_version": __version__,
                        "project_path": str(helper.root),
                    },
                }
            else:
                result = helper.dispatch(request)
                if request["method"] == "blob.read":
                    data = result.pop("data")
                    response = {"kind": "blob", "byte_size": len(data), **result}
                else:
                    data = b""
                    response = {
                        "kind": "response",
                        "request_id": request.get("request_id"),
                        "result": result,
                    }
            sys.stdout.buffer.write(encode(response))
            if authenticated and request.get("method") == "blob.read":
                sys.stdout.buffer.write(data)
        except Exception as exception:
            sys.stdout.buffer.write(
                encode(
                    {
                        "kind": "error",
                        "request_id": request.get("request_id"),
                        "error": error(exception),
                    }
                )
            )
        sys.stdout.buffer.flush()
        if fatal:
            break


if __name__ == "__main__":
    main()
