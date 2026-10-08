"""Revision-checked source transactions. Only this module publishes tracked source."""

import base64
import copy
import hashlib
import json
import os
import shutil
import tempfile
import time
import uuid
from contextlib import contextmanager
from pathlib import Path
from typing import Any, cast

from .notebook import EXTENSIONS, Notebook, canonical, filename, source_hash


class SourceError(ValueError):
    def __init__(self, code: str, message: str, details: Any = None):
        super().__init__(message)
        self.code = code
        self.details: Any = details or {}


def manifest_bytes(data: Any) -> bytes:
    header = {k: v for k, v in data.items() if k != "cells"}
    text = json.dumps(header, ensure_ascii=False, indent=2)[:-2] + ',\n  "cells": [\n'
    text += ",\n".join(
        "    " + json.dumps(cell, ensure_ascii=False, sort_keys=True) for cell in data["cells"]
    )
    return (text + "\n  ]\n}\n").encode("utf-8")


def atomic(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix=".kl-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as file:
            file.write(data)
            file.flush()
            os.fsync(file.fileno())
        os.replace(name, path)
        if os.name != "nt":
            directory = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
    finally:
        Path(name).unlink(missing_ok=True)


class SourceService:
    def __init__(self, root: Any):
        self.root = Path(root).resolve()
        self.notebook = Notebook(self.root)
        self.state = self.root / ".kl"
        self.hook: Any = None

    @contextmanager
    def locked(self):
        self.state.mkdir(parents=True, exist_ok=True)
        with (self.state / "source.lock").open("a+b") as file:
            if file.seek(0, 2) == 0:
                file.write(b" ")
                file.flush()
            file.seek(0)
            if os.name == "nt":
                import msvcrt

                while True:
                    try:
                        msvcrt.locking(file.fileno(), msvcrt.LK_NBLCK, 1)
                        break
                    except OSError:
                        time.sleep(0.01)
            else:
                import fcntl

                fcntl.flock(file.fileno(), fcntl.LOCK_EX)
            try:
                self._recover()
                yield
            finally:
                if os.name == "nt":
                    file.seek(0)
                    msvcrt.locking(file.fileno(), msvcrt.LK_UNLCK, 1)
                else:
                    fcntl.flock(file.fileno(), fcntl.LOCK_UN)

    def _path(self, name: str) -> Path:
        path = self.root / name
        if path.is_symlink() or not path.resolve().is_relative_to(self.root):
            raise SourceError("source_invalid", "source path escapes notebook", {"path": name})
        if name != "notebook.json" and not name.startswith(("cells/", "attachments/")):
            raise SourceError("invalid_request", "invalid source path")
        return path

    def _files(self) -> dict[str, bytes]:
        result: dict[str, bytes] = {}
        if self.notebook.path.exists():
            result["notebook.json"] = self.notebook.path.read_bytes()
        for directory in ("cells", "attachments"):
            for path in (self.root / directory).rglob("*"):
                if path.is_file():
                    name = path.relative_to(self.root).as_posix()
                    result[name] = self._path(name).read_bytes()
        return result

    def read(self) -> Any:
        if (self.state / "source.lock").exists() and os.access(self.state, os.W_OK):
            with self.locked():
                return self._read()
        return self._read()

    def _read(self) -> Any:
        # Inspection of a read-only project does not create .kl or take a write lock.
        try:
            raw = self.notebook.path.read_text(encoding="utf-8")
            if self._markers(raw):
                return {"state": "conflict_markers", "files": ["notebook.json"]}
            data: Any = json.loads(raw)
            if isinstance(data, dict):
                data = cast(dict[str, Any], data)
                if isinstance(data.get("schema"), int) and data["schema"] > 1:
                    return {"state": "unsupported_schema", "supported": 1, "actual": data["schema"]}
            data = self.notebook.read()
            cells: list[Any] = []
            markers: list[str] = []
            missing: list[str] = []
            for cell in data["cells"]:
                try:
                    source = self.notebook.source(cell)
                except FileNotFoundError:
                    missing.append(str(self.notebook.cell_path(cell).relative_to(self.root)))
                    continue
                if self._markers(source):
                    markers.append(str(self.notebook.cell_path(cell).relative_to(self.root)))
                cells.append({**cell, "source": source, "revision": source_hash(source)})
            return {
                "state": "missing" if missing else "conflict_markers" if markers else "ok",
                "manifest": data,
                "manifest_revision": hashlib.sha256(manifest_bytes(data)).hexdigest(),
                "cells": cells,
                "files": missing or markers,
            }
        except FileNotFoundError as error:
            return {"state": "missing", "message": str(error)}
        except (ValueError, KeyError, TypeError, AttributeError) as error:
            return {"state": "malformed", "message": str(error)}

    @staticmethod
    def _markers(text: str) -> bool:
        return any(
            line.startswith(("<<<<<<< ", "=======", ">>>>>>> ")) for line in text.splitlines()
        )

    def _check(self, expected: Any) -> Any:
        current = self._read()
        if current["state"] != "ok":
            raise SourceError("source_invalid", "source is not valid", current)
        revisions = {c["id"]: c["revision"] for c in current["cells"]}
        if expected.get("manifest_revision") != current["manifest_revision"] or any(
            revisions.get(cid) != revision
            for cid, revision in expected.get("cell_revisions", {}).items()
        ):
            raise SourceError("source_conflict", "source changed on disk", current)
        return current

    def _publish(self, before: Any, after: Any) -> None:
        writes = {name: data for name, data in after.items() if before.get(name) != data}
        removes = sorted(set(before) - set(after))
        if not writes and not removes:
            return
        if len(writes) == 1 and not removes:
            name, data = next(iter(writes.items()))
            atomic(self._path(name), data)
            return
        tx = self.state / "source-journal" / uuid.uuid4().hex
        tx.mkdir(parents=True)
        for name, data in writes.items():
            atomic(tx / "new" / name, data)
        for name in removes:
            atomic(tx / "old" / name, before[name])
        atomic(
            tx / "intent.json",
            json.dumps(
                {
                    "writes": list(writes),
                    "removes": removes,
                    "prior_revisions": {
                        name: hashlib.sha256(data).hexdigest() for name, data in before.items()
                    },
                }
            ).encode(),
        )
        if self.hook:
            self.hook("prepared")
        atomic(tx / "commit", b"commit\n")
        if self.hook:
            self.hook("committed")
        self._roll_forward(tx)

    def _roll_forward(self, tx: Path) -> None:
        intent = json.loads((tx / "intent.json").read_bytes())
        for name in sorted(intent["writes"], key=lambda n: n == "notebook.json"):
            # Keep journal bytes until every replace is complete, for repeatable recovery.
            atomic(self._path(name), (tx / "new" / name).read_bytes())
            if self.hook:
                self.hook("replaced:" + name)
        for name in intent["removes"]:
            self._path(name).unlink(missing_ok=True)
        shutil.rmtree(tx)

    def _recover(self) -> None:
        for tx in (self.state / "source-journal").glob("*"):
            if tx.is_symlink():
                raise SourceError("source_invalid", "invalid journal symlink")
            if (tx / "commit").exists():
                self._roll_forward(tx)
            else:
                shutil.rmtree(tx)

    def recover(self) -> Any:
        with self.locked():
            return self._read()

    def legacy(self, method: str, *args: Any) -> Any:
        # Stage the existing converters so even a failed import publishes nothing.
        with self.locked():
            if method not in ("write", "doctor"):
                current = self._read()
                if current["state"] != "ok":
                    raise SourceError("source_invalid", "source is not valid", current)
            before = self._files()
            with tempfile.TemporaryDirectory() as directory:
                staged = Notebook(directory)
                staged.direct = True
                for name, data in before.items():
                    path = staged.root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(data)
                (staged.root / "cells").mkdir(exist_ok=True)
                result = getattr(staged, method)(*args)
                after = SourceService(directory)._files()
                self._publish(before, after)
                return result

    def mutate(self, request: Any) -> Any:
        with self.locked():
            current = self._check(request)
            before = self._files()
            after = dict(before)
            data = copy.deepcopy(current["manifest"])
            cells = data["cells"]
            by_id = {c["id"]: c for c in cells}
            action = request["action"]
            cid = request.get("cell_id")
            cell: Any = by_id.get(cid)

            def name(c: Any) -> str:
                return "cells/" + c["id"] + EXTENSIONS[c["type"]]

            def require(c: Any) -> None:
                if c is None:
                    raise SourceError("not_found", "cell not found")
                if c["id"] not in request.get("cell_revisions", {}):
                    raise SourceError("invalid_request", "expected cell revision required")

            if action == "edit":
                require(cell)
                after[name(cell)] = canonical(request["source"]).encode()
            elif action in ("add", "duplicate", "split"):
                if action != "add":
                    require(cell)
                new = copy.deepcopy(cell) if cell else {"type": request.get("type", "code")}
                new["id"] = request.get("new_id", uuid.uuid4().hex[:12])
                index = request.get("index", len(cells))
                source = request.get("source", "")
                if action in ("duplicate", "split"):
                    source = before[name(cell)].decode()
                    index = cells.index(cell) + 1
                    if action == "split":
                        offset = request["offset"]
                        if not isinstance(offset, int) or not 0 <= offset <= len(source):
                            raise SourceError("invalid_request", "invalid split offset")
                        after[name(cell)] = source[:offset].encode()
                        source = source[offset:]
                    for attachment in self.notebook.attachment_names(cell):
                        old = f"attachments/{cid}/{attachment}"
                        after[f"attachments/{new['id']}/{attachment}"] = before[old]
                cells.insert(index, new)
                after[name(new)] = canonical(source).encode()
            elif action == "delete":
                require(cell)
                cells.remove(cell)
                after.pop(name(cell))
                for path in list(after):
                    if path.startswith(f"attachments/{cid}/"):
                        del after[path]
            elif action == "move":
                require(cell)
                cells.remove(cell)
                cells.insert(request["index"], cell)
            elif action == "convert":
                require(cell)
                source = after.pop(name(cell))
                cell["type"] = request["type"]
                after[name(cell)] = source
            elif action == "merge":
                require(cell)
                other: Any = by_id.get(request["other_id"])
                require(other)
                if cell["type"] != other["type"]:
                    raise SourceError("invalid_request", "merge requires matching cell types")
                other_source = after.pop(name(other)).decode()
                for attachment in self.notebook.attachment_names(other):
                    replacement = attachment
                    if replacement.lower() in {
                        n.lower() for n in self.notebook.attachment_names(cell)
                    }:
                        replacement = "merged-" + uuid.uuid4().hex[:12] + Path(attachment).suffix
                        other_source = other_source.replace(
                            "attachment:" + attachment, "attachment:" + replacement
                        )
                    after[f"attachments/{cid}/{replacement}"] = after.pop(
                        f"attachments/{other['id']}/{attachment}"
                    )
                    cell.setdefault("attachments", []).append(replacement)
                    if attachment in other.get("attachment_mimes", {}):
                        cell.setdefault("attachment_mimes", {})[replacement] = other[
                            "attachment_mimes"
                        ][attachment]
                if other.get("attachment_alternatives"):
                    raise SourceError(
                        "invalid_request", "merge MIME alternatives separately before merging"
                    )
                after[name(cell)] += b"\n" + other_source.encode()
                cells.remove(other)
            elif action == "attach":
                require(cell)
                attachment = filename(request.get("filename", Path(request["file_path"]).name))
                if attachment.lower() in {n.lower() for n in self.notebook.attachment_names(cell)}:
                    raise SourceError("invalid_request", "attachment already exists")
                after[f"attachments/{cid}/{attachment}"] = Path(request["file_path"]).read_bytes()
                cell.setdefault("attachments", []).append(attachment)
                cell.setdefault("attachment_mimes", {})[attachment] = request.get(
                    "media_type", "application/octet-stream"
                )
            elif action == "paste":
                index = request.get("index", len(cells))
                for item in request["cells"]:
                    new = copy.deepcopy(item["cell"])
                    new["id"] = uuid.uuid4().hex[:12]
                    after[name(new)] = canonical(item["source"]).encode()
                    for attachment in self.notebook.attachment_names(new):
                        after[f"attachments/{new['id']}/{attachment}"] = base64.b64decode(
                            item["attachments"][attachment], validate=True
                        )
                    cells.insert(index, new)
                    index += 1
            elif action == "metadata":
                if cid:
                    require(cell)
                (cell if cid else data)["metadata"] = request["metadata"]
            elif action == "retarget":
                data["id"] = str(uuid.uuid4())
            else:
                raise SourceError("invalid_request", "unknown mutation")
            after["notebook.json"] = manifest_bytes(data)
            # Reuse schema and filename validation without touching the real notebook.
            with tempfile.TemporaryDirectory() as directory:
                staged = Notebook(directory)
                staged.path.write_bytes(after["notebook.json"])
                staged.read()
            self._publish(before, after)
            return self._read()

    def execution_sources(self, ids: Any, expected: Any = None) -> Any:
        with self.locked():
            current = self._read()
            if current.get("manifest") is None:
                raise SourceError("source_invalid", "invalid manifest", current)
            cells = {c["id"]: c for c in current["cells"]}
            if ids == "all":
                ids = [c["id"] for c in current["cells"] if c["type"] == "code"]
            result: list[Any] = []
            for cid in ids:
                cell = cells.get(cid)
                if cell is None or cell["type"] != "code":
                    raise SourceError(
                        "source_invalid", "missing or non-code cell", {"cell_id": cid}
                    )
                if self._markers(cell["source"]):
                    raise SourceError(
                        "source_invalid", "cell has conflict markers", {"cell_id": cid}
                    )
                if expected and cid in expected and expected[cid] != cell["revision"]:
                    raise SourceError("source_conflict", "cell changed", current)
                result.append(cell)
            return result
