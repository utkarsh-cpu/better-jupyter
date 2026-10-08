"""Read-only output views and verified sharing artifacts."""

import base64
import hashlib
import json
import shutil
import tempfile
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any, cast

from .blobs import references
from .ledger import TABLES, fold, now
from .notebook import source_hash, write_json
from .worker import validate_checkpoint


def payload(value: Any, blobs: Any) -> Any:
    if "value" in value:
        return value["value"]
    data = blobs.get(value["blob"])
    if value.get("encoding") == "binary":
        return data
    if value.get("encoding") == "json":
        return json.loads(data)
    return data.decode()


def output_view(notebook: Any, ledger: Any, blobs: Any, eid: Any, raw: Any = False) -> Any:
    row = ledger.one("SELECT * FROM executions WHERE exec_id=?", (eid,))
    current = None
    if row["cell_id"]:
        try:
            current = source_hash(notebook.source(notebook.cell(row["cell_id"])))
        except (ValueError, OSError):
            pass
    events = ledger.events(eid)
    return {
        **row,
        "source": blobs.get(row["source_sha256"]).decode(),
        "current_source_sha256": current,
        "output_mismatch": current != row["source_sha256"],
        "outputs": events if raw else fold(events),
    }


def notebook_outputs(ledger: Any, blobs: Any, sid: Any) -> Any:
    result: dict[Any, Any]
    result = {}
    for row in ledger.rows("SELECT * FROM executions WHERE session_id=? ORDER BY ordinal", (sid,)):
        outputs: list[Any]
        outputs = []
        for event in fold(ledger.events(row["exec_id"])):
            kind = event["kind"]
            if kind in ("stdout", "stderr"):
                text = event.get("text")
                if text is None:
                    text = blobs.get(event["blob"]).decode()
                outputs.append({"output_type": "stream", "name": kind, "text": text})
            elif kind in ("display", "result"):
                data: dict[Any, Any]
                data = {
                    mime: base64.b64encode(value).decode()
                    if isinstance(value := payload(item, blobs), bytes)
                    else value
                    for mime, item in event["bundle"].items()
                }
                output: dict[Any, Any]
                output = {
                    "output_type": "execute_result" if kind == "result" else "display_data",
                    "data": data,
                    "metadata": event.get("metadata", {}),
                }
                if kind == "result":
                    output["execution_count"] = row["ordinal"]
                outputs.append(output)
            elif kind == "error":
                outputs.append(
                    {
                        "output_type": "error",
                        **{key: event[key] for key in ("ename", "evalue", "traceback")},
                    }
                )
        if row["cell_id"]:
            result[row["cell_id"]] = {**row, "outputs": outputs}
    return result


def descriptor(ledger: Any, blobs: Any, checkpoint_id: Any) -> Any:
    row = ledger.one("SELECT * FROM checkpoints WHERE checkpoint_id=?", (checkpoint_id,))
    manifest = json.loads(blobs.get(row["manifest_sha256"]))
    return {
        "schema": 1,
        "checkpoint_id": checkpoint_id,
        "manifest_sha256": row["manifest_sha256"],
        "blob_hashes": sorted(blobs.closure([row["manifest_sha256"]])),
        "status": row["status"],
        "excluded": manifest["excluded"],
        "notes": manifest["notes"],
    }


def gc(ledger: Any, blobs: Any, dry_run: bool = False) -> Any:
    seeds: list[Any]
    seeds = []
    seeds.extend(
        row["source_sha256"] for row in ledger.rows("SELECT source_sha256 FROM executions")
    )
    seeds.extend(
        row["manifest_sha256"] for row in ledger.rows("SELECT manifest_sha256 FROM checkpoints")
    )
    for row in ledger.rows("SELECT payload_json,blob_sha256 FROM events"):
        seeds.extend(references(json.loads(row["payload_json"])))
        if row["blob_sha256"]:
            seeds.append(row["blob_sha256"])
    for row in ledger.rows("SELECT payload_json FROM session_events"):
        seeds.extend(references(json.loads(row["payload_json"])))
    protected = blobs.closure(seeds)
    removed: list[Any]
    removed = []
    total_bytes = 0
    for path in blobs.root.iterdir():
        if path.name not in protected:
            total_bytes += path.stat().st_size
            if not dry_run:
                path.unlink()
            removed.append(path.name)
    if not dry_run:
        with ledger.db:
            ledger.db.executemany("DELETE FROM blobs WHERE sha256=?", [(h,) for h in removed])
    return {
        "removed": [] if dry_run else removed,
        "candidates": removed,
        "total_bytes": total_bytes,
        "dry_run": dry_run,
        "protected": len(protected),
    }


def selected_rows(ledger: Any, sessions: Any = None, checkpoints: Any = None) -> Any:
    sessions = set(sessions or [])
    checkpoints = set(checkpoints or [])
    if not sessions and not checkpoints:
        sessions = {r["session_id"] for r in ledger.rows("SELECT session_id FROM sessions")}
        checkpoints = {
            r["checkpoint_id"] for r in ledger.rows("SELECT checkpoint_id FROM checkpoints")
        }
    for cid in checkpoints:
        row = ledger.one("SELECT * FROM checkpoints WHERE checkpoint_id=?", (cid,))
        sessions.add(row["session_id"])
    rows: list[Any]
    rows = []
    eids: set[Any]
    eids = {
        row["exec_id"]
        for row in ledger.rows("SELECT * FROM executions")
        if row["session_id"] in sessions
    }
    for table in TABLES:
        for row in ledger.rows("SELECT * FROM " + table):
            include = (
                (
                    table in ("sessions", "executions", "session_events")
                    and row["session_id"] in sessions
                )
                or (table == "events" and row["exec_id"] in eids)
                or (table == "checkpoints" and row["checkpoint_id"] in checkpoints)
                or (table == "pins" and row["checkpoint_id"] in checkpoints)
            )
            if include:
                rows.append({"table": table, "row": row})
    return rows


def export_bundle(
    notebook: Any,
    ledger: Any,
    blobs: Any,
    target: Any,
    sessions: Any = None,
    checkpoints: Any = None,
    external: Any = None,
) -> Any:
    target = Path(target).resolve()
    if target.exists():
        raise ValueError("bundle destination already exists")
    rows = selected_rows(ledger, sessions, checkpoints)
    password_replies = sum(
        1
        for item in rows
        if item["table"] == "events"
        and item["row"]["kind"] == "stdin_reply"
        and json.loads(item["row"]["payload_json"]).get("password")
        and "text" in json.loads(item["row"]["payload_json"])
    )
    seeds: list[Any]
    seeds = []
    for item in rows:
        seeds.extend(references(item["row"]))
        if "payload_json" in item["row"]:
            seeds.extend(references(json.loads(item["row"]["payload_json"])))
    hashes = blobs.closure(seeds)
    rows.extend(
        {
            "table": "blobs",
            "row": {"sha256": digest, "byte_size": len(blobs.get(digest)), "media_type": None},
        }
        for digest in sorted(hashes)
    )
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        source = root / "notebook"
        source.mkdir()
        document = notebook.read()
        files: list[Any]
        files = [notebook.path]
        files.extend(notebook.cell_path(cell) for cell in document["cells"])
        files.extend(
            notebook.root / "attachments" / cell["id"] / name
            for cell in document["cells"]
            for name in notebook.attachment_names(cell)
        )
        files.extend((notebook.root / "checkpoints").glob("*.json"))
        for path in files:
            dest = source / path.relative_to(notebook.root)
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(path.read_bytes())
        (root / "ledger.jsonl").write_text(
            "".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8", newline="\n"
        )
        (root / "blobs").mkdir()
        for digest in hashes:
            (root / "blobs" / digest).write_bytes(blobs.get(digest))
        manifest: dict[Any, Any]
        manifest = {
            "schema": 1,
            "notebook_id": document["id"],
            "session_ids": [
                item["row"]["session_id"] for item in rows if item["table"] == "sessions"
            ],
            "checkpoint_ids": [
                item["row"]["checkpoint_id"] for item in rows if item["table"] == "checkpoints"
            ],
            "blob_hashes": sorted(hashes),
            "external": external or [],
            "files": {
                p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in root.rglob("*")
                if p.is_file()
            },
        }
        write_json(root / "MANIFEST.json", manifest)
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.suffix == ".zip":
            with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as archive:
                for path in root.rglob("*"):
                    if path.is_file():
                        archive.write(path, path.relative_to(root).as_posix())
        else:
            shutil.copytree(root, target)
    return {
        "path": str(target),
        "recorded_password_replies": password_replies,
        "blob_count": len(hashes),
        "warning": "Checkpoint values and recorded input may contain secrets.",
    }


class Bundle:
    def __init__(self, path: Any) -> None:
        self.path = Path(path)
        self.archive = zipfile.ZipFile(self.path) if self.path.is_file() else None
        if self.archive and len(self.archive.namelist()) != len(set(self.archive.namelist())):
            raise ValueError("duplicate archive paths")
        self.manifest = json.loads(self.read("MANIFEST.json"))
        if self.manifest.get("schema") != 1:
            raise ValueError("unsupported bundle schema")
        if not isinstance(self.manifest.get("files"), dict):
            raise ValueError("invalid bundle file hashes")

    def close(self) -> Any:
        if self.archive:
            self.archive.close()

    def read(self, name: Any) -> Any:
        path = PurePosixPath(name)
        if path.is_absolute() or ".." in path.parts or "\\" in name or ":" in name:
            raise ValueError("unsafe bundle path")
        if self.archive:
            return self.archive.read(name)
        file = (self.path / name).resolve()
        if not file.is_relative_to(self.path.resolve()):
            raise ValueError("bundle symlink escapes directory")
        return file.read_bytes()

    def inspect(self) -> Any:
        mismatches: list[Any]
        mismatches = []
        for name, digest in self.manifest["files"].items():
            try:
                if hashlib.sha256(self.read(name)).hexdigest() != digest:
                    mismatches.append(name)
            except (OSError, KeyError, ValueError):
                mismatches.append(name)
        for digest in self.manifest["blob_hashes"]:
            name = "blobs/" + digest
            if self.manifest["files"].get(name) != digest:
                mismatches.append(name)
        checkpoints: list[Any]
        checkpoints = []
        try:
            referenced: set[str] = set()
            for line in self.read("ledger.jsonl").decode().splitlines():
                item = json.loads(line)
                if item["table"] not in TABLES:
                    raise ValueError("unknown bundle table")
                referenced.update(references(item["row"]))
                if "payload_json" in item["row"]:
                    referenced.update(references(json.loads(item["row"]["payload_json"])))
                if item["table"] == "checkpoints":
                    manifest = json.loads(self.read("blobs/" + item["row"]["manifest_sha256"]))
                    validate_checkpoint(manifest)
                    referenced.update(references(manifest))
                    checkpoints.append(
                        {
                            **item["row"],
                            "excluded": manifest["excluded"],
                            "notes": manifest["notes"],
                        }
                    )
            advertised = set(self.manifest["blob_hashes"])
            for digest in referenced - advertised:
                mismatches.append("blobs/" + digest)
        except (OSError, KeyError, ValueError, TypeError):
            mismatches.append("ledger.jsonl")
        return {
            **self.manifest,
            "checkpoints": checkpoints,
            "hash_mismatches": sorted(set(mismatches)),
        }


def import_bundle(notebook: Any, ledger: Any, blobs: Any, path: Any) -> Any:
    bundle = Bundle(path)
    try:
        report = bundle.inspect()
        if report["hash_mismatches"]:
            raise ValueError("bundle hash mismatch: " + ", ".join(report["hash_mismatches"]))
        rows: list[Any]
        rows = [json.loads(line) for line in bundle.read("ledger.jsonl").decode().splitlines()]
        # Validate the entire selection before mutating storage.
        for item in rows:
            if item["table"] not in TABLES:
                raise ValueError("unknown bundle table")
            columns: set[Any]
            columns = {
                row[1] for row in ledger.db.execute("PRAGMA table_info(" + item["table"] + ")")
            }
            if set(item["row"]) != columns:
                raise ValueError("invalid bundle ledger columns")
        for digest in bundle.manifest["blob_hashes"]:
            blobs.put(bundle.read("blobs/" + digest), digest)
        with ledger.db:
            for table in TABLES:
                for item in rows:
                    if item["table"] != table:
                        continue
                    row: dict[Any, Any]
                    row = dict(item["row"])
                    if table in ("sessions", "checkpoints"):
                        row["origin"] = "bundle:" + str(Path(path).resolve())
                    if table == "sessions" and row["state"] == "running":
                        row["state"], row["end"] = "closed", now()
                    if table == "executions" and row["status"] in ("queued", "running"):
                        row["status"], row["end"] = "incomplete", now()
                    names: list[Any]
                    names = list(row)
                    # Preserve local records and provenance on idempotent imports.
                    ledger.db.execute(
                        "INSERT OR IGNORE INTO "
                        + table
                        + " ("
                        + ",".join(names)
                        + ") VALUES ("
                        + ",".join("?" for _ in names)
                        + ")",
                        tuple(row.values()),
                    )
        return {
            "imported": True,
            "session_ids": bundle.manifest["session_ids"],
            "checkpoint_ids": bundle.manifest["checkpoint_ids"],
        }
    finally:
        bundle.close()


def mutate(command: Any, request: Any, notebook: Any, ledger: Any, blobs: Any) -> Any:
    if command == "gc":
        return gc(ledger, blobs, request.get("dry_run", False))
    if command == "pin":
        ledger.one(
            "SELECT checkpoint_id FROM checkpoints WHERE checkpoint_id=?",
            (request["checkpoint_id"],),
        )
        ledger.write(
            "INSERT OR REPLACE INTO pins VALUES (?,?)", (request["name"], request["checkpoint_id"])
        )
        return {"pinned": request["name"]}
    if command == "unpin":
        ledger.write("DELETE FROM pins WHERE name=?", (request["name"],))
        return {"unpinned": request["name"]}
    if command == "publish":
        data = descriptor(ledger, blobs, request["checkpoint_id"])
        path = notebook.root / "checkpoints" / (data["checkpoint_id"] + ".json")
        write_json(path, data)
        return {"path": str(path), **data}
    if command == "bundle_export":
        return export_bundle(
            notebook,
            ledger,
            blobs,
            request["path"],
            request.get("sessions"),
            request.get("checkpoints"),
            request.get("external"),
        )
    if command == "bundle_import":
        return import_bundle(notebook, ledger, blobs, request["path"])
    if command == "import_outputs":
        sid = ledger.session(
            notebook.read()["id"], {"kind": "imported"}, origin="ipynb", state="closed"
        )
        for item in request["rows"]:
            digest = blobs.put(item["source"].encode())
            ledger.blob(digest, len(item["source"].encode()))
            eid = ledger.execution(sid, item["cell_id"], digest)
            for output in item["outputs"]:
                kind = output["output_type"]
                if kind == "stream":
                    text = output["text"]
                    ledger.event(
                        eid,
                        output["name"],
                        {
                            "text": "".join(cast(list[str], text))
                            if isinstance(text, list)
                            else text
                        },
                    )
                elif kind in ("display_data", "execute_result"):
                    bundle: dict[Any, Any]
                    bundle = {}
                    for mime, value in output["data"].items():
                        if mime in ("image/png", "image/jpeg"):
                            data = base64.b64decode(
                                "".join(cast(list[str], value))
                                if isinstance(value, list)
                                else value,
                                validate=True,
                            )
                            digest = blobs.put(data)
                            ledger.blob(digest, len(data))
                            bundle[mime] = {"blob": digest, "encoding": "binary"}
                        else:
                            bundle[mime] = {
                                "value": "".join(cast(list[str], value))
                                if isinstance(value, list) and mime != "application/json"
                                else value
                            }
                    ledger.event(
                        eid,
                        "result" if kind == "execute_result" else "display",
                        {"bundle": bundle, "metadata": output.get("metadata", {})},
                    )
                elif kind == "error":
                    ledger.event(
                        eid, "error", {k: output[k] for k in ("ename", "evalue", "traceback")}
                    )
            ledger.write("UPDATE executions SET status='ok',end=? WHERE exec_id=?", (now(), eid))
        return {"session_id": sid, "origin": "imported"}
    raise ValueError(f"unknown supervisor command: {command}")
