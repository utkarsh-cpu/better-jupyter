"""Parity between the native helper and the Python helper."""

import hashlib
import json
import os
import shutil
import sqlite3
import struct
import subprocess
import sys
import tempfile
import threading
import uuid
from pathlib import Path

import pytest

from kernelledger.blobs import BlobStore
from kernelledger.desktop.helper import Helper
from kernelledger.desktop.protocol import API
from kernelledger.ledger import Ledger, now
from kernelledger.notebook import Notebook, canonical, source_hash
from kernelledger.source import SourceError, SourceService

ROOT = Path(__file__).resolve().parents[1]


def daemon_path() -> Path:
    override = os.environ.get("KERNELLEDGER_DAEMON") or os.environ.get("KL_NATIVE_DAEMON")
    if override:
        return Path(override)
    name = "kernelledger-daemon.exe" if os.name == "nt" else "kernelledger-daemon"
    return ROOT / "desktop" / "target" / "debug" / name


def child_names(pid: int) -> list[str]:
    if os.name != "nt":
        children = Path(f"/proc/{pid}/task/{pid}/children")
        if not children.is_file():
            return []
        names = []
        for child in children.read_text().split():
            try:
                names.append(Path(f"/proc/{child}/comm").read_text().strip())
            except FileNotFoundError:
                pass
        return names
    script = f'(Get-CimInstance Win32_Process -Filter "ParentProcessId={pid}").Name'
    flags = {"creationflags": 0x08000000} if os.name == "nt" else {}
    completed = subprocess.run(
        ["powershell", "-NoProfile", "-Command", script],
        capture_output=True,
        text=True,
        timeout=20,
        check=False,
        **flags,
    )
    return [
        line.strip()
        for line in completed.stdout.splitlines()
        if line.strip() and line.strip().lower() != "conhost.exe"
    ]


class NativeHelper:
    def __init__(self, root: Path, python: str | None = None, env: dict | None = None):
        binary = daemon_path()
        if not binary.is_file():
            if os.environ.get("KERNELLEDGER_DAEMON") or os.environ.get("KL_NATIVE_DAEMON"):
                pytest.fail(f"configured daemon missing: {binary}")
            pytest.skip(f"missing {binary}; build desktop/crates/daemon first")
        args = [str(binary), "helper", "--project", str(root)]
        if python:
            args.extend(["--python", python])
        self.stderr = tempfile.TemporaryFile()
        flags = {"creationflags": 0x08000000} if os.name == "nt" else {}
        self.proc = subprocess.Popen(
            args,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.stderr,
            env=env,
            **flags,
        )
        self.send(
            {
                "kind": "hello",
                "api": API,
                "versions": [1],
                "role": "control",
                "client_id": str(uuid.uuid4()),
                "client_name": "native-parity",
            }
        )
        self.welcome = self.read_json()
        assert self.welcome["kind"] == "welcome", self.failure(self.welcome)

    def failure(self, payload: object) -> str:
        self.stderr.seek(0)
        err = self.stderr.read().decode(errors="replace")
        return f"{payload}\n{err}"

    def send(self, message: dict) -> None:
        raw = json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode()
        assert self.proc.stdin is not None
        self.proc.stdin.write(struct.pack(">I", len(raw)) + raw)
        self.proc.stdin.flush()

    def read_exact(self, size: int) -> bytes:
        assert self.proc.stdout is not None
        data = b""
        while len(data) < size:
            chunk = self.proc.stdout.read(size - len(data))
            if not chunk:
                raise AssertionError(self.failure("helper closed stdout"))
            data += chunk
        return data

    def read_json(self) -> dict:
        size = struct.unpack(">I", self.read_exact(4))[0]
        message = json.loads(self.read_exact(size))
        if not isinstance(message, dict):
            raise AssertionError(self.failure(message))
        return message

    def call(self, method: str, **fields: object) -> tuple[dict, bytes]:
        self.send({"method": method, "request_id": method, **fields})
        frame = self.read_json()
        data = b""
        if frame.get("kind") == "blob":
            data = self.read_exact(frame["byte_size"])
        return frame, data

    def close(self) -> None:
        if self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait(timeout=5)
        self.stderr.close()


def python_result(root: Path, method: str, **fields: object):
    try:
        return Helper(root).dispatch({"method": method, **fields})
    except SourceError as error:
        return {"error": error.code}


def native_result(helper: NativeHelper, method: str, **fields: object):
    frame, data = helper.call(method, **fields)
    if frame["kind"] == "error":
        return {"error": frame["error"]["code"]}
    if frame["kind"] == "blob":
        body = {key: value for key, value in frame.items() if key not in ("byte_size", "kind")}
        body["data"] = data
        return body
    return frame["result"]


def assert_match(helper: NativeHelper, root: Path, method: str, **fields: object) -> None:
    actual = native_result(helper, method, **fields)
    expected = python_result(root, method, **fields)
    if actual != expected:
        raise AssertionError(
            json.dumps(
                {"method": method, "actual": actual, "expected": expected},
                indent=2,
                default=lambda value: value.hex() if isinstance(value, bytes) else str(value),
            )
        )


def digest_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def test_native_empty_history_and_corrupt_blob(tmp_path: Path) -> None:
    root = seeded(tmp_path / "native")
    native = NativeHelper(root)
    try:
        for method in ("sessions.list", "executions.list", "checkpoints.list", "events.list"):
            assert_match(native, root, method, exec_id="missing")
        assert_match(native, root, "executions.watermark")
        for method, fields in (
            ("execution.get", {"exec_id": "missing"}),
            ("checkpoint.get", {"checkpoint_id": "missing"}),
            ("fold.get", {"exec_id": "missing"}),
        ):
            assert_match(native, root, method, **fields)
        assert not (root / ".kl/ledger.sqlite").exists()
        blobs = BlobStore(root / ".kl")
        digest = blobs.put(b"valid")
        (root / ".kl/blobs" / digest).write_bytes(b"wrong")
        assert native_result(native, "blob.read", sha256=digest, length=1) == {
            "error": "invalid_request"
        }
        assert_match(native, root, "ping")
    finally:
        native.close()


def test_native_helper_matches_python_reads_and_does_not_write(tmp_path: Path) -> None:
    notebook = Notebook(tmp_path)
    notebook.init()
    notebook.add("code", "print('current')\n", "a")
    recorded = "print('recorded')\n"
    blobs = BlobStore(notebook.root / ".kl")
    digest = blobs.put(canonical(recorded).encode())
    assert digest == source_hash(recorded)
    payload = blobs.put(b"not-json-payload")
    manifest = {
        "schema": 1,
        "objects": {"value": {"blob": payload}},
        "bindings": {},
        "excluded": ["secret"],
        "notes": ["résumé"],
    }
    manifest_digest = blobs.put(
        json.dumps(manifest, ensure_ascii=False, separators=(",", ":")).encode()
    )
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite")
    session = ledger.session(notebook.read()["id"], {})
    other = ledger.session(notebook.read()["id"], {})
    exec_id = ledger.execution(session, "a", digest)
    ledger.execution(other, None, digest)
    ledger.write("UPDATE executions SET status='ok', end=? WHERE exec_id=?", (now(), exec_id))
    ledger.event(exec_id, "stdout", {"text": "café\n"})
    ledger.event(
        exec_id,
        "display",
        {"display_id": "plot", "bundle": {"text/plain": {"value": "one"}}},
    )
    ledger.event(
        exec_id,
        "display_update",
        {"display_id": "plot", "bundle": {"text/plain": {"value": "two"}}},
    )
    for index in range(40):
        ledger.event(exec_id, "stdout", {"text": f"line {index}\n"})
    ledger.db.execute(
        "INSERT INTO checkpoints VALUES (?,?,?,?,?,?,?)",
        (str(uuid.uuid4()), session, exec_id, "partial", manifest_digest, now(), "local"),
    )
    ledger.db.commit()
    for index in range(30):
        ledger.execution(session, "a", digest)
    ledger.close()
    roots = notebook.root / ".kl/training-artifacts/roots"
    roots.mkdir(parents=True)
    (roots / "b-snap").write_text("b", encoding="utf-8")
    (roots / "a-snap").write_text("a", encoding="utf-8")
    descriptor = tmp_path / "descriptor.json"
    descriptor.write_text('{"schema":1,"name":"α"}\n', encoding="utf-8")
    database = notebook.root / ".kl/ledger.sqlite"
    before = digest_file(database)
    files_before = sorted(path.name for path in (notebook.root / ".kl").iterdir())

    native = NativeHelper(notebook.root)
    try:
        assert native.welcome["helper_implementation"] == "rust"
        assert native.welcome["capabilities"] == ["blob_ranges"]
        assert native.welcome["delegated_methods"] == []
        assert Path(native.welcome["identity"]["project_path"]) == notebook.root
        assert_match(native, notebook.root, "ping")
        assert_match(native, notebook.root, "sessions.list")
        assert_match(native, notebook.root, "executions.watermark")
        assert_match(native, notebook.root, "executions.list", page_size=2)
        first, _ = native.call("executions.list", page_size=2)
        assert len(first["result"]["items"]) == 2
        assert first["result"]["cursor"]
        assert_match(
            native,
            notebook.root,
            "executions.list",
            page_size=2,
            cursor=first["result"]["cursor"],
        )
        assert_match(native, notebook.root, "executions.list", latest_per_cell=True, page_size=10)
        assert_match(native, notebook.root, "executions.list", session_id=session, page_size=5)
        assert_match(native, notebook.root, "execution.get", exec_id=exec_id)
        assert_match(native, notebook.root, "fold.get", exec_id=exec_id)
        assert_match(native, notebook.root, "events.list", exec_id=exec_id, page_size=2)
        events, _ = native.call("events.list", exec_id=exec_id, page_size=2, after_seq=3)
        assert len(events["result"]["items"]) == 2
        assert_match(
            native,
            notebook.root,
            "events.list",
            exec_id=exec_id,
            page_size=2,
            after_seq=3,
            cursor=events["result"]["cursor"],
        )
        assert_match(native, notebook.root, "checkpoints.list")
        checkpoint_id = python_result(notebook.root, "checkpoints.list")["items"][0][
            "checkpoint_id"
        ]
        assert_match(native, notebook.root, "checkpoint.get", checkpoint_id=checkpoint_id)
        assert_match(native, notebook.root, "training.list")
        assert_match(native, notebook.root, "descriptor.inspect", path=str(descriptor))
        assert_match(native, notebook.root, "blob.read", sha256=payload, offset=2, length=4)
        assert_match(native, notebook.root, "blob.read", sha256=digest)
        missing = "ab" * 32
        assert native_result(native, "blob.read", sha256=missing) == {"error": "not_found"}
        assert python_result(notebook.root, "blob.read", sha256=missing) == {"error": "not_found"}
        assert native_result(native, "source.mutate", action="edit", manifest_revision="x") == {
            "error": "source_conflict"
        }
        assert native_result(native, "nope") == {"error": "invalid_request"}
        assert child_names(native.proc.pid) == []
        assert not (notebook.root / ".kl/supervisor.json").exists()
    finally:
        native.close()

    assert digest_file(database) == before
    files_after = sorted(path.name for path in (notebook.root / ".kl").iterdir())
    allowed = {*files_before, "ledger.sqlite-shm", "ledger.sqlite-wal", "source.lock"}
    unexpected = sorted(set(files_after) - allowed)
    assert unexpected == [], (files_before, files_after)
    unchanged = sqlite3.connect(database)
    assert unchanged.execute("SELECT COUNT(*) FROM executions").fetchone()[0] == 32
    unchanged.close()


def test_native_helper_reads_a_live_wal_writer(tmp_path: Path) -> None:
    notebook = Notebook(tmp_path)
    notebook.init()
    blobs = BlobStore(notebook.root / ".kl")
    digest = blobs.put(canonical("print(1)\n").encode())
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite")
    session = ledger.session(notebook.read()["id"], {})
    exec_id = ledger.execution(session, None, digest)
    ledger.db.execute("BEGIN IMMEDIATE")
    ledger.db.execute("UPDATE executions SET status='running' WHERE exec_id=?", (exec_id,))
    native = NativeHelper(notebook.root)
    try:
        listed = native_result(native, "executions.list")
        assert listed["items"][0]["status"] == "queued"
        assert listed == python_result(notebook.root, "executions.list")
        ledger.db.commit()
        listed = native_result(native, "executions.list")
        assert listed["items"][0]["status"] == "running"
        assert listed == python_result(notebook.root, "executions.list")
    finally:
        native.close()
        ledger.close()


def test_native_helper_serves_source_and_delegates_doctor(tmp_path: Path) -> None:
    notebook = Notebook(tmp_path / "empty")
    notebook.init()
    native = NativeHelper(notebook.root, python=sys.executable)
    try:
        assert "portable_training" in native.welcome["capabilities"]
        assert "source.read" in native.welcome["native_methods"]
        assert "source.read" not in native.welcome["delegated_methods"]
        assert "doctor" in native.welcome["delegated_methods"]
        assert child_names(native.proc.pid) == []
        assert_match(native, notebook.root, "sessions.list")
        assert_match(native, notebook.root, "executions.watermark")
        assert native_result(native, "execution.get", exec_id="missing") == {"error": "not_found"}
        assert_match(native, notebook.root, "source.read")
        assert child_names(native.proc.pid) == []
        assert_match(native, notebook.root, "doctor")
        names = [name.lower() for name in child_names(native.proc.pid)]
        assert any("python" in name for name in names)
    finally:
        native.close()
    assert not (notebook.root / ".kl/ledger.sqlite").exists()


def project_files(root: Path) -> dict[str, bytes]:
    files = {}
    for path in root.rglob("*"):
        if path.is_file() and ".kl" not in path.parts:
            files[path.relative_to(root).as_posix()] = path.read_bytes()
    return files


def seeded(root: Path) -> Path:
    notebook = Notebook(root)
    notebook.init()
    notebook.add("code", "old\n", "a")
    notebook.add("markdown", "note\n", "m")
    return root


def mutate_request(root: Path, **fields: object) -> dict:
    current = SourceService(root).read()
    return {
        "manifest_revision": current["manifest_revision"],
        "cell_revisions": {cell["id"]: cell["revision"] for cell in current["cells"]},
        **fields,
    }


def mutate_native(root: Path, env: dict | None = None, **fields: object) -> dict:
    helper = NativeHelper(root, env=env)
    try:
        return native_result(helper, "source.mutate", **mutate_request(root, **fields))
    finally:
        helper.close()


def test_native_source_mutations_match_python_bytes(tmp_path: Path) -> None:
    origin = seeded(tmp_path / "origin")
    python_root = tmp_path / "python"
    native_root = tmp_path / "native"
    shutil.copytree(origin, python_root)
    shutil.copytree(origin, native_root)
    actions = [
        {"action": "edit", "cell_id": "a", "source": "new\n"},
        {"action": "add", "type": "code", "new_id": "b", "source": "value = 1\n"},
        {"action": "move", "cell_id": "b", "index": 0},
        {"action": "convert", "cell_id": "m", "type": "raw"},
        {"action": "delete", "cell_id": "b"},
    ]
    for fields in actions:
        SourceService(python_root).mutate(mutate_request(python_root, **fields))
        result = mutate_native(native_root, **fields)
        assert result.get("state") == "ok", result
        assert project_files(native_root) == project_files(python_root)


def test_native_source_extended_mutations_match_python_bytes(tmp_path: Path) -> None:
    python_root = seeded(tmp_path / "python")
    native_root = tmp_path / "native"
    shutil.copytree(python_root, native_root)
    attachment = tmp_path / "attachment.bin"
    attachment.write_bytes(b"\x00\xffattachment")
    actions = [
        {"action": "edit", "cell_id": "a", "source": "x = '\U0001f642\u2028'\r\n"},
        {"action": "metadata", "metadata": {"small": 1.5e-7, "big": 1e16, "zero": -0.0}},
        {"action": "metadata", "cell_id": "a", "metadata": {"label": "\u2028\u007f\u0001"}},
        {"action": "duplicate", "cell_id": "a", "new_id": "b"},
        {"action": "split", "cell_id": "b", "offset": 6, "new_id": "c"},
        {"action": "merge", "cell_id": "b", "other_id": "c"},
        {"action": "attach", "cell_id": "m", "file_path": str(attachment), "filename": "image.bin"},
        {"action": "duplicate", "cell_id": "m", "new_id": "n"},
        {"action": "move", "cell_id": "n", "index": -1},
    ]
    for fields in actions:
        SourceService(python_root).mutate(mutate_request(python_root, **fields))
        result = mutate_native(native_root, **fields)
        assert result.get("state") == "ok", result
        assert project_files(native_root) == project_files(python_root), fields


@pytest.mark.parametrize("action", ["insert_cells", "paste"])
def test_native_generated_cells_and_attachments(tmp_path: Path, action: str) -> None:
    root = seeded(tmp_path / "native")
    if action == "insert_cells":
        fields = {
            "action": action,
            "index": 1,
            "cells": [{"type": "code", "source": "\U0001f642\r\n"}],
        }
    else:
        fields = {
            "action": action,
            "index": 1,
            "cells": [
                {
                    "cell": {"type": "markdown", "attachments": ["x.bin"]},
                    "source": "attachment:x.bin\r\n",
                    "attachments": {"x.bin": "Zh=="},
                }
            ],
        }
    result = mutate_native(root, **fields)
    assert result["state"] == "ok", result
    cell = result["cells"][1]
    assert cell["source"] == ("\U0001f642\n" if action == "insert_cells" else "attachment:x.bin\n")
    assert len(cell["id"]) == 12
    if action == "paste":
        assert (root / "attachments" / cell["id"] / "x.bin").read_bytes() == b"f"
    assert SourceService(root).read() == result


@pytest.mark.parametrize(
    "fields",
    [
        {"action": "move", "cell_id": "a"},
        {"action": "merge", "cell_id": "a"},
        {"action": "metadata"},
        {"action": "insert_cells", "index": 0, "cells": []},
        {"action": "split", "cell_id": "a", "new_id": "b", "offset": 1000},
    ],
)
def test_native_invalid_mutations_leave_source_unchanged(tmp_path: Path, fields: dict) -> None:
    root = seeded(tmp_path / "native")
    before = project_files(root)
    assert mutate_native(root, **fields) == {"error": "invalid_request"}
    assert project_files(root) == before


def test_native_source_journal_recovers_from_either_side(tmp_path: Path) -> None:
    crashed = seeded(tmp_path / "rust-committed")
    env = os.environ.copy()
    env["KERNELLEDGER_SOURCE_CRASH"] = "committed"
    failed = mutate_native(
        crashed, env=env, action="add", type="code", new_id="b", source="value = 1\n"
    )
    assert failed["error"] == "internal"
    recovered = SourceService(crashed).recover()
    assert {cell["id"] for cell in recovered["cells"]} == {"a", "b", "m"}
    assert not list((crashed / ".kl/source-journal").glob("*"))

    dropped = seeded(tmp_path / "rust-prepared")
    env["KERNELLEDGER_SOURCE_CRASH"] = "prepared"
    mutate_native(dropped, env=env, action="add", type="code", new_id="b", source="value = 1\n")
    recovered = SourceService(dropped).recover()
    assert [cell["id"] for cell in recovered["cells"]] == ["a", "m"]
    assert not list((dropped / ".kl/source-journal").glob("*"))

    python_crash = seeded(tmp_path / "python-committed")
    service = SourceService(python_crash)
    request = mutate_request(
        python_crash, action="add", type="code", new_id="b", source="value = 1\n"
    )

    def crash(point: str) -> None:
        if point == "committed":
            raise RuntimeError("crash")

    service.hook = crash
    with pytest.raises(RuntimeError):
        service.mutate(request)
    expected = seeded(tmp_path / "expected")
    SourceService(expected).mutate(
        mutate_request(expected, action="add", type="code", new_id="b", source="value = 1\n")
    )
    helper = NativeHelper(python_crash)
    try:
        recovered = native_result(helper, "source.recover")
    finally:
        helper.close()
    assert recovered["state"] == "ok"
    rust_cells = [(cell["id"], cell["type"], cell["source"]) for cell in recovered["cells"]]
    python_cells = [
        (cell["id"], cell["type"], cell["source"])
        for cell in SourceService(expected).read()["cells"]
    ]
    assert rust_cells == python_cells


def test_native_source_rejects_stale_revision_and_shares_the_lock(tmp_path: Path) -> None:
    root = seeded(tmp_path / "conflict")
    current = SourceService(root).read()
    (root / "cells/a.py").write_text("external", encoding="utf-8")
    helper = NativeHelper(root)
    try:
        result = native_result(
            helper,
            "source.mutate",
            action="edit",
            cell_id="a",
            source="mine\n",
            manifest_revision=current["manifest_revision"],
            cell_revisions={cell["id"]: cell["revision"] for cell in current["cells"]},
        )
    finally:
        helper.close()
    assert result == {"error": "source_conflict"}
    assert (root / "cells/a.py").read_text(encoding="utf-8") == "external"

    shared = seeded(tmp_path / "shared")
    request = mutate_request(shared, action="edit", cell_id="a", source="from-race\n")
    results: list[str] = []

    def native_call() -> None:
        helper = NativeHelper(shared)
        try:
            result = native_result(helper, "source.mutate", **request)
            results.append(result.get("state") or result["error"])
        finally:
            helper.close()

    def python_call() -> None:
        try:
            results.append(SourceService(shared).mutate(request)["state"])
        except SourceError as error:
            results.append(error.code)

    workers = [threading.Thread(target=native_call), threading.Thread(target=python_call)]
    for worker in workers:
        worker.start()
    for worker in workers:
        worker.join()
    assert sorted(results) == ["ok", "source_conflict"]
    assert (shared / "cells/a.py").read_text(encoding="utf-8") == "from-race\n"
