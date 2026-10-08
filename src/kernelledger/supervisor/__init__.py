"""Authenticated asyncio supervisor; sole writer of runtime state."""

import asyncio
import hashlib
import json
import os
import secrets
import shutil
import struct
import sys
from pathlib import Path
from typing import Any

from ..blobs import BlobStore, references
from ..ledger import Ledger, now, uid
from ..notebook import Notebook, write_json
from ..worker import INLINE, json_bytes, validate_checkpoint
from .platform import DirectoryLock, ProcessTree, secure_token


async def read_message(reader: Any, blobs: Any) -> Any:
    size = struct.unpack(">I", await reader.readexactly(4))[0]
    if size > INLINE:
        raise ValueError("protocol inline limit exceeded")
    message = json.loads(await reader.readexactly(size))
    if message.get("schema") != 1:
        raise ValueError("unsupported protocol schema")
    if message.get("kind") == "blob":
        digest = await blobs.receive(reader, message["sha256"], message["byte_size"])
        return {"kind": "blob", "sha256": digest, "byte_size": message["byte_size"]}
    if message.get("kind") == "envelope":
        message = json.loads(blobs.get(message["blob"]))
        if message.get("schema") != 1:
            raise ValueError("unsupported envelope schema")
    return message


async def send_message(writer: Any, kind: Any, **fields: Any) -> Any:
    message: dict[Any, Any]
    message = {"schema": 1, "kind": kind, **fields}
    data = json_bytes(message)
    if len(data) > INLINE:
        digest = await send_blob(writer, data)
        data = json_bytes({"schema": 1, "kind": "envelope", "blob": digest})
    writer.write(struct.pack(">I", len(data)) + data)
    await writer.drain()


async def send_blob(writer: Any, data: Any) -> Any:
    digest = hashlib.sha256(data).hexdigest()
    header = json_bytes({"schema": 1, "kind": "blob", "sha256": digest, "byte_size": len(data)})
    writer.write(struct.pack(">I", len(header)) + header)
    for offset in range(0, len(data), 1048576):
        writer.write(data[offset : offset + 1048576])
        await writer.drain()
    return digest


def interpreter(
    root: Any, explicit: Any, config: Any, distribution: str = "installed", launch: str = "terminal"
) -> Any:
    if explicit:
        # Resolving a Linux venv symlink selects system Python and loses its site-packages.
        return os.path.abspath(explicit), "command"
    if config.get("python"):
        return os.path.abspath(config["python"]), "config"
    for parent in (root, *root.parents):
        candidate = parent / ".venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        if candidate.is_file():
            return str(candidate), "project_venv"
    if launch == "terminal" and os.environ.get("VIRTUAL_ENV"):
        return str(
            Path(os.environ["VIRTUAL_ENV"])
            / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        ), "virtual_env"
    if distribution == "installed":
        return sys.executable, "supervisor"
    from ..desktop.helper import candidates
    from ..source import SourceError

    raise SourceError(
        "interpreter_required", "Select a project interpreter", {"candidates": candidates(root)}
    )


class Supervisor:
    def __init__(
        self,
        root: Any,
        python: Any = None,
        distribution: str = "installed",
        launch: str = "terminal",
    ) -> None:
        self.root = Path(root).resolve()
        self.notebook = Notebook(self.root)
        self.document = self.notebook.read()
        self.state = self.root / ".kl"
        self.state.mkdir(exist_ok=True)
        self.lock = DirectoryLock(self.state)
        self.explicit_python = python
        self.distribution, self.launch = distribution, launch
        self.config: dict[str, Any] = (
            json.loads((self.state / "config.json").read_text())
            if (self.state / "config.json").exists()
            else {}
        )
        self.process, self.tree, self.writer, self.session = None, None, None, None
        self.active = None
        self.command_lock = asyncio.Lock()
        self.worker_send_lock = asyncio.Lock()
        self.stopping = asyncio.Event()
        self.reader_task, self.monitor_task = None, None
        self.waiter = None
        self.input_client = None
        self.bytes_recorded, self.bytes_dropped = 0, 0
        self.stream = None
        self.event_timer = None
        self.last_commit = 0
        self.retained_tasks: set[Any]
        self.retained_tasks = set()
        self.client_writers: set[Any] = set()
        self.publish_hook: Any = None

    def spawn_task(self, coroutine: Any) -> Any:
        task = asyncio.create_task(coroutine)
        self.retained_tasks.add(task)
        task.add_done_callback(self.retained_tasks.discard)
        return task

    async def send_worker(self, kind: Any, **fields: Any) -> Any:
        async with self.worker_send_lock:
            if self.writer is None:
                raise ValueError("worker is down")
            await send_message(self.writer, kind, **fields)

    def record_blob(self, digest: Any) -> Any:
        self.ledger.blob(digest, len(self.blobs.get(digest)))

    def flush_stream(self) -> Any:
        if self.stream:
            kind, text, eid = self.stream
            payload: dict[Any, Any]
            payload = {"text": text}
            if len(text.encode()) > INLINE:
                digest = self.blobs.put(text.encode())
                self.record_blob(digest)
                payload: dict[Any, Any]
                payload = {"blob": digest, "encoding": "utf8"}
            self.ledger.event(eid, kind, payload, payload.get("blob"), commit=False)
            self.stream = None

    def record(self, eid: Any, kind: Any, payload: Any) -> Any:
        if kind in ("stdout", "stderr"):
            text = payload.get("text", "")
            encoded = text.encode()
            limit = self.config.get("output_limit_bytes", 268435456)
            accepted = encoded[: max(0, limit - self.bytes_recorded)]
            self.bytes_dropped += len(encoded) - len(accepted)
            self.bytes_recorded += len(accepted)
            text = accepted.decode("utf-8", errors="ignore")
            if not text:
                return
            if self.stream and (
                self.stream[0] != kind
                or len(self.stream[1].encode()) + len(accepted) > INLINE
                or payload.get("capture")
            ):
                self.flush_stream()
            if payload.get("capture"):
                self.ledger.event(eid, kind, {"text": text, "capture": "pipe"}, commit=False)
            elif self.stream:
                self.stream = (kind, self.stream[1] + text, eid)
            else:
                self.stream = (kind, text, eid)
        else:
            self.flush_stream()
            for digest in references(payload):
                self.record_blob(digest)
            self.ledger.event(eid, kind, payload, commit=False)

    async def flush_loop(self) -> Any:
        while not self.stopping.is_set():
            await asyncio.sleep(0.05)
            self.flush_stream()
            self.ledger.db.commit()

    async def worker_reader(self, reader: Any) -> Any:
        try:
            while True:
                message = await read_message(reader, self.blobs)
                self = getattr(self, "_admitted_to", self)
                kind = message.pop("kind")
                message.pop("schema", None)
                eid = message.pop("exec_id", None)
                if kind == "blob":
                    continue
                if kind == "status" and eid:
                    if message["state"] == "running":
                        self.ledger.write(
                            "UPDATE executions SET status='running' WHERE exec_id=?", (eid,)
                        )
                        self.record(eid, "status", message)
                    else:
                        # Drain any pipe output already available before completion.
                        await asyncio.sleep(0.02)
                        self.flush_stream()
                        if self.bytes_dropped:
                            self.ledger.event(
                                eid,
                                "truncated",
                                {"dropped_bytes": self.bytes_dropped},
                                commit=False,
                            )
                        self.record(eid, "status", message)
                        self.ledger.db.commit()
                        self.ledger.write(
                            "UPDATE executions SET status=?,end=? WHERE exec_id=?",
                            (message["outcome"], now(), eid),
                        )
                        self.active = None
                        if self.waiter and not self.waiter.done():
                            self.waiter.set_result({"exec_id": eid, "status": message["outcome"]})
                elif kind == "restore_result":
                    self.restore_report = message
                elif kind in ("ready", "checkpoint_offer", "training_result"):
                    if self.waiter and not self.waiter.done():
                        self.waiter.set_result(
                            self.restore_report
                            if kind == "ready" and self.restore_report is not None
                            else message
                        )
                    if kind == "ready":
                        self.restore_report = None
                elif eid:
                    self.record(eid, kind, message)
                    if kind == "stdin_request":
                        self.spawn_task(self.desktop.input_requested(eid, message))
                elif self.session:
                    self.ledger.session_event(self.session, kind, message)
        except (asyncio.IncompleteReadError, OSError, ValueError) as error:
            self = getattr(self, "_admitted_to", self)
            if self.tree:
                self.tree.kill()
            if self.waiter and not self.waiter.done():
                self.waiter.set_exception(ValueError(f"worker disconnected: {error}"))

    async def pipe_reader(self, pipe: Any, kind: Any) -> Any:
        import codecs

        decoder = codecs.getincrementaldecoder("utf-8")("replace")
        while True:
            data = await pipe.read(65536)
            self = getattr(self, "_admitted_to", self)
            if not data:
                break
            text = decoder.decode(data)
            if self.active:
                self.record(self.active, kind, {"text": text, "capture": "pipe"})
            else:
                print(f"worker {kind}: {text}", file=sys.stderr, flush=True)

    async def monitor(self, process: Any, sid: Any, temporary: Any = None) -> Any:
        await process.wait()
        if temporary is not None:
            temporary.cleanup()
        self = getattr(self, "_admitted_to", self)
        if self.session == sid and self.process is process:
            await self.desktop.cancel_inputs()
            self.flush_stream()
            if self.active:
                self.ledger.write(
                    "UPDATE executions SET status='aborted',end=? WHERE exec_id=?",
                    (now(), self.active),
                )
                self.active = None
            self.ledger.write(
                "UPDATE sessions SET state='crashed',end=? WHERE session_id=? AND state='running'",
                (now(), sid),
            )
            if self.waiter and not self.waiter.done():
                self.waiter.set_exception(
                    ValueError("worker exited; next run starts an empty session")
                )
            if self.tree:
                self.tree.close()
                self.tree = None
            self.process, self.writer = None, None

    async def start_worker(
        self, parent: Any = None, checkpoint: Any = None, override: Any = False
    ) -> Any:
        import tempfile

        self.restore_report: Any = None
        self.start_stage = "select interpreter"
        worker_python, choice = interpreter(
            self.root, self.explicit_python, self.config, self.distribution, self.launch
        )
        if not Path(worker_python).is_file():
            raise ValueError(f"worker interpreter not found: {worker_python}")
        token, accepted = secrets.token_hex(32), asyncio.get_running_loop().create_future()

        async def accept(reader: Any, writer: Any) -> Any:
            try:
                hello = await asyncio.wait_for(read_message(reader, self.blobs), 10)
                if (
                    hello.get("kind") != "hello"
                    or not secrets.compare_digest(hello.get("token", ""), token)
                    or accepted.done()
                ):
                    writer.close()
                    return
                accepted.set_result((reader, writer, hello))
            except Exception as error:
                print(f"worker handshake failed: {error!r}", file=sys.stderr, flush=True)
                writer.close()

        listener = await asyncio.start_server(accept, "127.0.0.1", 0)
        self.start_stage = "launch process"
        port = listener.sockets[0].getsockname()[1]
        env: dict[Any, Any]
        self.worker_temporary = tempfile.TemporaryDirectory(prefix="kl-worker-session-")
        env = {
            **os.environ,
            "KL_WORKER_PORT": str(port),
            "KL_WORKER_TOKEN": token,
            "KL_TRAINING_TEMP_ROOT": self.worker_temporary.name,
        }
        kwargs: dict[str, Any] = (
            {"creationflags": 0x08000000 | 0x00000200}
            if os.name == "nt"
            else {"start_new_session": True}
        )
        try:
            self.process = await asyncio.create_subprocess_exec(
                worker_python,
                "-I",
                "-X",
                "utf8",
                str(Path(__file__).parents[1] / "worker.py"),
                str(self.root),
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                env=env,
                **kwargs,
            )
            self.spawn_task(self.pipe_reader(self.process.stdout, "stdout"))
            self.spawn_task(self.pipe_reader(self.process.stderr, "stderr"))
            self.tree = ProcessTree(self.process.pid)
            self.start_stage = "await hello"
            reader, writer, hello = await asyncio.wait_for(accepted, 20)
        except Exception:
            if self.process:
                self.process.kill()
                await self.process.communicate()
            if self.tree:
                self.tree.close()
                self.tree = None
            self.process = None
            self.worker_temporary.cleanup()
            raise
        finally:
            listener.close()
            # Python 3.12+ waits for accepted connections too. The worker connection
            # remains live for the session, so only close the listening socket here.
        self.writer = writer
        self.worker_reader_stream = reader
        self.start_stage = "create session"
        hello.pop("token", None)
        hello.pop("kind", None)
        hello.pop("schema", None)
        hello.update(
            interpreter_choice=choice,
            env={name: os.environ.get(name) for name in self.config.get("env_allowlist", [])},
            allow_python_mismatch=override,
        )
        self.session = self.ledger.session(self.document["id"], hello, parent, checkpoint)
        self.start_stage = "await ready"
        self.waiter = asyncio.get_running_loop().create_future()
        self.reader_task = self.spawn_task(self.worker_reader(reader))
        self.monitor_task = self.spawn_task(
            self.monitor(self.process, self.session, self.worker_temporary)
        )
        await asyncio.wait_for(self.waiter, 20)
        self.environment = hello
        self.start_stage = "ready"

    async def end_worker(self, state: Any = "closed") -> Any:
        if not self.process:
            return
        await self.desktop.cancel_inputs()
        process, sid = self.process, self.session
        self.ledger.write(
            "UPDATE sessions SET state=?,end=? WHERE session_id=?", (state, now(), sid)
        )
        self.ledger.write(
            "UPDATE executions SET status='aborted',end=? WHERE session_id=? AND status='queued'",
            (now(), sid),
        )
        if self.active:
            self.ledger.write(
                "UPDATE executions SET status='aborted',end=? WHERE exec_id=?", (now(), self.active)
            )
            self.active = None
        self.process = None
        if self.waiter and not self.waiter.done():
            self.waiter.set_exception(ValueError("worker was stopped"))
        if self.writer:
            if state == "closed":
                try:
                    await self.send_worker("shutdown")
                    await asyncio.wait_for(process.wait(), 2)
                except (TimeoutError, OSError):
                    pass
            self.writer.close()
            self.writer = None
        if self.tree:
            self.tree.close()
            self.tree = None
        if process.returncode is None:
            process.kill()
        await process.wait()
        self.worker_temporary.cleanup()

    async def execute(
        self,
        source: Any,
        cid: Any = None,
        client: Any = None,
        inputs: Any = None,
        notify: Any = None,
        accepted: Any = None,
    ) -> Any:
        empty = not self.process
        if empty:
            await self.start_worker()
            if notify:
                await send_message(
                    notify, "notice", message=f"Started empty worker session {self.session}"
                )
        digest = self.blobs.put(source.encode())
        self.record_blob(digest)
        if accepted and accepted["session_id"] == self.session:
            eid = accepted["exec_id"]
        else:
            if accepted:
                self.ledger.write(
                    "UPDATE executions SET status='aborted',end=? WHERE exec_id=? AND status='queued'",
                    (now(), accepted["exec_id"]),
                )
            eid = self.ledger.execution(self.session, cid, digest)
        self.active = eid
        self.input_client, self.replay_inputs = client, inputs
        self.bytes_recorded, self.bytes_dropped = 0, 0
        self.waiter = asyncio.get_running_loop().create_future()
        await self.send_worker(
            "execute", exec_id=eid, cell_id=cid, source_sha256=digest, source=source
        )
        result = await self.waiter
        return {**result, "new_empty_session": empty, "session_id": self.session}

    async def checkpoint(self) -> Any:
        if not self.process:
            raise ValueError("no live worker; run or restore first")
        existing = {path.name for path in self.blobs.root.iterdir()}
        self.waiter = asyncio.get_running_loop().create_future()
        await self.send_worker(
            "checkpoint",
            denylist=self.config.get("checkpoint_denylist", []),
            thread_allowlist=self.config.get("thread_allowlist", ["tqdm._monitor.TMonitor"]),
        )
        offer = await self.waiter
        if "manifest" not in offer:
            self.ledger.session_event(
                self.session,
                "checkpoint_refused" if "refused" in offer else "checkpoint_failed",
                offer,
            )
            return offer
        manifest = offer["manifest"]
        validate_checkpoint(manifest)
        if self.publish_hook:
            self.publish_hook("payloads_moved")
        digest = self.blobs.put(json_bytes(manifest))
        blobs = self.blobs.closure([digest])
        status = "partial" if manifest["excluded"] else "complete"
        if self.publish_hook:
            self.publish_hook("manifest_moved")
        checkpoint_id = uid()
        boundary = self.ledger.rows(
            "SELECT exec_id FROM executions WHERE session_id=? ORDER BY ordinal DESC LIMIT 1",
            (self.session,),
        )
        with self.ledger.db:
            for blob in blobs:
                self.record_blob(blob)
            self.ledger.db.execute(
                "INSERT INTO checkpoints VALUES (?,?,?,?,?,?,?)",
                (
                    checkpoint_id,
                    self.session,
                    boundary[0]["exec_id"] if boundary else None,
                    status,
                    digest,
                    now(),
                    "local",
                ),
            )
            if self.publish_hook:
                self.publish_hook("before_commit")
        if self.publish_hook:
            self.publish_hook("after_commit")
        return {
            "checkpoint_id": checkpoint_id,
            "status": status,
            "excluded": manifest["excluded"],
            "notes": manifest["notes"],
            "walk_seconds": offer.get("walk_seconds"),
            "new_blob_bytes": sum(
                path.stat().st_size
                for path in self.blobs.root.iterdir()
                if path.name not in existing
            ),
        }

    async def restore(self, request: Any) -> Any:
        row = self.ledger.one(
            "SELECT * FROM checkpoints WHERE checkpoint_id=?", (request["checkpoint_id"],)
        )
        if row["origin"] != "local" and not request.get("trust"):
            raise ValueError("imported checkpoint executes saved source; pass --trust")
        hashes = self.blobs.closure([row["manifest_sha256"]])
        manifest = json.loads(self.blobs.get(row["manifest_sha256"]))
        validate_checkpoint(manifest)
        await self.end_worker()
        await self.start_worker(
            row["session_id"], row["checkpoint_id"], request.get("allow_python_mismatch", False)
        )
        if self.environment["python"].split(".")[:2] != manifest["python"].split(".")[
            :2
        ] and not request.get("allow_python_mismatch"):
            await self.end_worker()
            raise ValueError("Python major.minor mismatch; pass --allow-python-mismatch")
        self.waiter = asyncio.get_running_loop().create_future()
        async with self.worker_send_lock:
            for digest in hashes:
                await send_blob(self.writer, self.blobs.get(digest))
            await send_message(self.writer, "restore", manifest=manifest)
        result = await self.waiter
        self.ledger.session_event(self.session, "restore_result", result)
        return {
            "session_id": self.session,
            "platform_drift": manifest.get("platform") != self.environment["platform"],
            **result,
        }

    async def replay(self, request: Any, notify: Any = None) -> Any:
        sid = request["session_id"]
        row = self.ledger.one("SELECT * FROM sessions WHERE session_id=?", (sid,))
        if row["origin"] != "local" and not request.get("trust"):
            raise ValueError("imported replay executes source; pass --trust")
        executions = self.ledger.rows(
            "SELECT * FROM executions WHERE session_id=? ORDER BY ordinal", (sid,)
        )
        sources: list[Any]
        sources = [self.blobs.get(e["source_sha256"]).decode() for e in executions]
        await self.end_worker()
        await self.start_worker(parent=sid)
        previous = json.loads(row["environment_json"])
        drift: dict[Any, Any]
        drift = {
            key: {"recorded": previous.get(key), "current": self.environment.get(key)}
            for key in set(previous) | set(self.environment)
            if previous.get(key) != self.environment.get(key)
        }
        if notify:
            await send_message(
                notify,
                "notice",
                message="Environment differences before replay: " + json.dumps(drift),
            )
        results: list[Any]
        results = []
        for execution, source in zip(executions, sources):
            inputs: list[Any]
            inputs = []
            if request.get("reuse_input"):
                inputs: list[Any]
                inputs = [
                    {"error": "secret_input_not_recorded"}
                    if event.get("redacted")
                    else {k: v for k, v in event.items() if k in ("text", "error")}
                    for event in self.ledger.events(execution["exec_id"])
                    if event["kind"] == "stdin_reply"
                ]
            result = await self.execute(source, execution["cell_id"], inputs=inputs)
            results.append(result)
            if result["status"] != "ok":
                break
        return {"session_id": self.session, "drift": drift, "executions": results}

    def training_inspect(self, snapshot: str) -> Any:
        from ..artifacts import ArtifactStore
        from ..experiments import inspect_experiment

        return inspect_experiment(
            ArtifactStore(self.state / "training-artifacts", readonly=True), snapshot
        )

    def training_list(self) -> Any:
        directory = self.state / "training-artifacts" / "roots"
        return {
            "items": [
                {"snapshot": path.name, "mode": "portable-training", "native_qualified": False}
                for path in sorted(directory.iterdir())
                if path.is_file()
            ]
            if directory.exists()
            else []
        }

    async def training_checkpoint(self) -> Any:
        import tempfile

        from ..artifacts import ArtifactStore

        if not self.process:
            raise ValueError("no live worker; run and register training first")
        with tempfile.TemporaryDirectory(prefix="kl-training-transfer-") as temporary:
            destination = Path(temporary) / "export"
            self.waiter = asyncio.get_running_loop().create_future()
            await self.send_worker("training_checkpoint", destination=str(destination))
            offer = await self.waiter
            if "failed" in offer:
                raise ValueError(offer["failed"])
            # Worker-supplied paths are never accepted; verify our own staging dir.
            store = ArtifactStore(self.state / "training-artifacts")
            selection_path = destination / "selection.json"
            if selection_path.is_symlink() or selection_path.stat().st_size > 8 * 1024**2:
                raise ValueError("invalid training selection")
            selection = json.loads(selection_path.read_bytes())
            if selection.get("blob") != offer["result"]["snapshot"]:
                raise ValueError("training offer selection mismatch")
            snapshot = await asyncio.to_thread(store.import_directory, destination)
            if snapshot != offer["result"]["snapshot"]:
                raise ValueError("training offer selection mismatch")
            result = {**offer["result"], "mode": "portable-training", "native_qualified": False}
            self.ledger.session_event(self.session, "training_checkpoint", result)
            return result

    async def training_restore(self, request: Any) -> Any:
        import copy
        import tempfile

        from ..artifacts import ArtifactStore
        from ..desktop.server import DesktopServer

        if request.get("trust") is not True:
            raise ValueError(
                "training reconstruction requires --trust for captured factory execution"
            )
        snapshot = request["snapshot"]
        self.training_inspect(snapshot)  # Framework-free validation before starting a candidate.
        store = ArtifactStore(self.state / "training-artifacts")
        with tempfile.TemporaryDirectory(prefix="kl-training-admission-") as temporary:
            source = Path(temporary) / "export"
            await asyncio.to_thread(store.export, snapshot, source)
            candidate = copy.copy(self)
            for name in ("process", "tree", "writer", "session", "active", "waiter", "stream"):
                setattr(candidate, name, None)
            candidate.retained_tasks = set()
            candidate.worker_send_lock = asyncio.Lock()
            candidate.desktop = DesktopServer(candidate)
            try:
                await candidate.start_worker(parent=self.session)
                candidate.waiter = asyncio.get_running_loop().create_future()
                await candidate.send_worker(
                    "training_restore",
                    source=str(source),
                    snapshot=snapshot,
                    trust=True,
                    device=request.get("device", "cpu"),
                )
                offer = await asyncio.wait_for(candidate.waiter, 120)
                if "failed" in offer:
                    raise ValueError(offer["failed"])
                if candidate.process is None or candidate.process.returncode is not None:
                    raise ValueError("candidate exited before admission")
            except BaseException:
                await candidate.end_worker("restore_failed")
                await asyncio.gather(*candidate.retained_tasks, return_exceptions=True)
                raise
            await self.end_worker()
            for name in (
                "process",
                "tree",
                "writer",
                "session",
                "waiter",
                "reader_task",
                "monitor_task",
                "worker_reader_stream",
                "worker_temporary",
                "environment",
                "start_stage",
            ):
                setattr(self, name, getattr(candidate, name))
            candidate._admitted_to = self
            for task in candidate.retained_tasks:
                self.retained_tasks.add(task)
                task.add_done_callback(self.retained_tasks.discard)
            result = {
                **offer["result"],
                "session_id": self.session,
                "snapshot": snapshot,
                "mode": "portable-training",
                "native_qualified": False,
            }
            self.ledger.session_event(self.session, "training_restore", result)
            return result

    async def dispatch(self, request: Any, reader: Any, writer: Any) -> Any:
        command = request["command"]
        if command == "training-list":
            return self.training_list()
        if command == "training-inspect":
            return self.training_inspect(request["snapshot"])
        if command == "native-checkpoint":
            from ..gpu.backends.capabilities import inventory

            return {
                "refused": "native_backend_unavailable",
                "native_qualified": False,
                "capabilities": inventory(),
            }
        if command == "status":
            return {
                "pid": os.getpid(),
                "session_id": self.session,
                "worker_python": getattr(self, "environment", {}).get("executable"),
                "in_flight": self.active,
                "worker_start_stage": getattr(self, "start_stage", None),
                "instance_id": self.desktop.instance_id,
                "attached_client_count": len(self.desktop.clients),
            }
        if command == "interrupt":
            await self.desktop.cancel_inputs()
            if self.session:
                self.ledger.session_event(
                    self.session, "interrupt_kill" if request.get("kill") else "interrupt", {}
                )
            if request.get("kill"):
                await self.end_worker("killed")
            elif self.active:
                await self.send_worker("interrupt")
            else:
                raise ValueError("no in-flight execution")
            return {"interrupted": True}
        if (
            command in ("checkpoint", "training-checkpoint", "training-restore")
            and self.command_lock.locked()
        ):
            if self.session:
                self.ledger.session_event(self.session, "checkpoint_refused", {"refused": "busy"})
            return {"refused": "busy"}
        if command == "stop":
            await self.end_worker()
            self.stopping.set()
            return {"stopped": True}
        if command == "operation":
            return self.desktop.operation(request)
        if command == "cancel":
            return await self.desktop.cancel(request["operation_id"])
        if command == "batch":
            return await self.desktop.legacy_batch(request, reader, writer)
        accepted: Any = None
        source: Any = None
        cid: Any = None
        if command == "run":
            from ..notebook import canonical

            if "source" in request:
                source = canonical(request["source"])
            else:
                from ..source import SourceService

                cell = await asyncio.to_thread(
                    SourceService(self.root).execution_sources, [request["cell_id"]]
                )
                source, cid = cell[0]["source"], cell[0]["id"]
            if self.process and self.session:
                digest = self.blobs.put(source.encode())
                self.record_blob(digest)
                accepted = {
                    "exec_id": self.ledger.execution(self.session, cid, digest),
                    "session_id": self.session,
                }
        async with self.command_lock:
            if command == "run":
                return await self.execute(
                    source,
                    cid,
                    (reader, writer) if request.get("interactive") else None,
                    notify=writer,
                    accepted=accepted,
                )
            if command == "restart":
                await self.end_worker()
                await self.start_worker()
                return {"session_id": self.session}
            if command == "checkpoint":
                return await self.checkpoint()
            if command == "training-checkpoint":
                return await self.training_checkpoint()
            if command == "training-restore":
                return await self.training_restore(request)
            if command in ("restore", "continue"):
                return await self.restore(request)
            if command == "replay":
                return await self.replay(request, writer)
            if command == "migrate":
                return self.ledger.migrate(self.state / "ledger.sqlite.schema1.bak")
            from ..sharing import mutate

            return mutate(command, request, self.notebook, self.ledger, self.blobs)

    async def client(self, reader: Any, writer: Any) -> Any:
        self.client_writers.add(writer)
        try:
            # Authentication is checked before accepting binary data or reading notebook paths.
            size = struct.unpack(">I", await asyncio.wait_for(reader.readexactly(4), 10))[0]
            if size > INLINE:
                raise ValueError("authentication frame too large")
            auth = json.loads(await asyncio.wait_for(reader.readexactly(size), 10))
            if (
                auth.get("schema") != 1
                or auth.get("kind") != "auth"
                or not secrets.compare_digest(auth.get("token", ""), self.token)
            ):
                raise ValueError("authentication failed")
            while True:
                request = await read_message(reader, self.blobs)
                if request["kind"] != "blob":
                    break
            result = await self.dispatch(request, reader, writer)
            await send_message(writer, "response", result=result)
        except Exception as error:
            try:
                await send_message(writer, "response", error=str(error))
            except (OSError, ConnectionError):
                pass
        finally:
            self.client_writers.discard(writer)
            writer.close()
            await writer.wait_closed()

    async def run(self) -> Any:
        self.lock.acquire()
        from ..source import SourceService

        await asyncio.to_thread(SourceService(self.root).recover)
        self.ledger = Ledger(self.state / "ledger.sqlite")
        self.ledger.takeover()
        for entry in (self.state / "tmp").glob("*"):
            if entry.is_symlink():
                entry.unlink()
            elif entry.is_dir():
                if not entry.resolve().is_relative_to((self.state / "tmp").resolve()):
                    raise ValueError("temporary directory escapes notebook state")
                shutil.rmtree(entry)
            else:
                entry.unlink()
        self.blobs = BlobStore(self.state)
        self.replay_inputs = None
        self.token = secrets.token_hex(32)
        token_path = self.state / "token"
        fd = os.open(token_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(fd, "w") as file:
            file.write(self.token)
        try:
            secure_token(token_path)
            (self.state / "token-permission-error").unlink(missing_ok=True)
        except OSError as error:
            (self.state / "token-permission-error").write_text(str(error))
        server = await asyncio.start_server(self.client, "127.0.0.1", 0)
        from ..desktop.server import DesktopServer
        from ..registry import publish

        self.desktop = DesktopServer(self)
        desktop_server = await asyncio.start_server(self.desktop.client, "127.0.0.1", 0)
        info = {
            **self.desktop.identity(),
            "port": server.sockets[0].getsockname()[1],
            "desktop_port": desktop_server.sockets[0].getsockname()[1],
            "protocols": {"legacy": [1], "desktop": [1]},
        }
        write_json(
            self.state / "supervisor.json",
            info,
        )
        registry_path = publish(self.root, info)
        self.event_timer = self.spawn_task(self.flush_loop())
        try:
            async with server, desktop_server:
                await self.stopping.wait()
                # Python 3.12+ Server.wait_closed also waits for accepted client
                # transports. Stop must close attached clients before context exit.
                for writer in (*self.client_writers, *self.desktop.clients):
                    writer.close()
        finally:
            await self.end_worker()
            for task in list(self.retained_tasks):
                task.cancel()
            await asyncio.gather(*self.retained_tasks, return_exceptions=True)
            self.flush_stream()
            self.ledger.db.commit()
            self.ledger.close()
            (self.state / "supervisor.json").unlink(missing_ok=True)
            registry_path.unlink(missing_ok=True)
            self.lock.close()


def main(
    root: Any, python: Any = None, distribution: str = "installed", launch: str = "terminal"
) -> Any:
    asyncio.run(Supervisor(root, python, distribution, launch).run())
