"""Supervisor-owned durable desktop operations, input ownership and committed feeds."""

import asyncio
import json
import os
import secrets
import time
from typing import Any, cast

from .. import __version__
from ..ledger import Ledger, now, uid
from ..notebook import canonical, source_hash
from ..source import SourceError, SourceService
from . import protocol
from .contracts import validate

EFFECTS = {
    "training-checkpoint",
    "training-restore",
    "run",
    "batch",
    "restart",
    "stop",
    "checkpoint",
    "restore",
    "continue",
    "replay",
    "pin",
    "unpin",
    "gc",
    "publish",
    "import_ipynb",
    "bundle_import",
    "migrate",
}
FINAL = {"completed", "canceled", "failed", "unknown"}


class DesktopServer:
    def __init__(self, supervisor: Any):
        self.s = supervisor
        self.instance_id, self.started = uid(), now()
        self.clients: dict[Any, Any] = {}
        self.pending: dict[str, Any] = {}
        self.canceled: set[str] = set()
        self.admission = asyncio.Lock()
        self.stop_requested = False

    def identity(self) -> Any:
        return {
            "instance_id": self.instance_id,
            "pid": os.getpid(),
            "start_time": self.started,
            "kernelledger_version": __version__,
            "distribution": self.s.distribution,
            "ledger_schema": self.s.ledger.schema,
            "migration_required": self.s.ledger.schema == 1,
            "notebook_id": self.s.document["id"],
            "project_path": str(self.s.root),
        }

    def status(self) -> Any:
        s = self.s
        return {
            **self.identity(),
            "worker_state": "waiting_input"
            if self.pending
            else "running"
            if s.active
            else "idle"
            if s.process
            else "absent",
            "session_id": s.session,
            "capabilities": protocol.CAPABILITIES,
            "interpreter": getattr(s, "environment", {}),
            "active_execution": s.active,
            "attached_client_count": len({c["client_id"] for c in self.clients.values()}),
            "record_secret_input": s.config.get("record_secret_input", False),
            "pending_inputs": [self.input_view(p) for p in self.pending.values()],
            "queue": s.ledger.rows(
                "SELECT * FROM executions WHERE status='queued' ORDER BY session_id,ordinal"
            ),
            "active_operations": s.ledger.rows(
                "SELECT * FROM operations WHERE state IN ('accepted','running') ORDER BY accepted"
            )
            if s.ledger.schema == 2
            else [],
        }

    def operation(self, request: Any) -> Any:
        if self.s.ledger.schema != 2:
            raise SourceError("migration_required", "Migrate ledger to schema 2")
        if request.get("operation_id"):
            rows = self.s.ledger.rows(
                "SELECT * FROM operations WHERE operation_id=?", (request["operation_id"],)
            )
        else:
            rows = self.s.ledger.rows(
                "SELECT * FROM operations WHERE client_id=? AND client_request_id=?",
                (request["client_id"], request["client_request_id"]),
            )
        if not rows:
            raise SourceError("not_found", "operation not found")
        row = rows[0]
        result_json = row.pop("result_json")
        row["result"] = json.loads(result_json) if result_json else None
        return row

    async def cancel(self, oid: str) -> Any:
        row = self.operation({"operation_id": oid})
        if row["state"] in FINAL:
            return {"kind": "already_final", "state": row["state"]}
        if row["state"] == "running" and row["kind"] not in ("run", "batch"):
            raise SourceError("busy", "operation is not cancelable once running")
        self.canceled.add(oid)
        accepted_result: Any = row.get("result") or {}
        with self.s.ledger.db:
            for eid in accepted_result.get("exec_ids", []):
                self.s.ledger.db.execute(
                    "UPDATE executions SET status='canceled',end=? WHERE exec_id=? AND status='queued'",
                    (now(), eid),
                )
            for batch in self.s.ledger.rows("SELECT * FROM batches WHERE operation_id=?", (oid,)):
                for eid in json.loads(batch["exec_ids_json"]):
                    self.s.ledger.db.execute(
                        "UPDATE executions SET status='canceled',end=? WHERE exec_id=? AND status='queued'",
                        (now(), eid),
                    )
        if row["state"] == "accepted":
            self.s.ledger.operation_state(oid, "canceled")
        elif row["kind"] == "run" and self.s.active:
            await self.cancel_inputs()
            await self.s.send_worker("interrupt")
        return {"kind": "cancel_requested", "state": self.operation({"operation_id": oid})["state"]}

    async def freeze(self, request: Any) -> Any:
        if request["method"] == "batch":
            if request.get("on_error", "stop") not in ("stop", "continue"):
                raise SourceError("invalid_request", "invalid on_error policy")
            ids: Any = request.get("cell_ids", "all")
            if ids != "all" and (
                not isinstance(ids, list)
                or len(set(cast(list[str], ids))) != len(cast(list[str], ids))
            ):
                raise SourceError("invalid_request", "batch requires unique ordered cell ids")
            return await asyncio.to_thread(
                SourceService(self.s.root).execution_sources,
                ids,
                request.get("expected_sha256", {}),
            )
        if "source" in request:
            source = canonical(request["source"])
            digest = source_hash(source)
            if request.get("expected_sha256") != digest:
                raise SourceError(
                    "source_conflict", "unattributed source hash differs", {"revision": digest}
                )
            return [{"id": None, "source": source, "revision": digest}]
        if not request.get("expected_sha256"):
            raise SourceError("invalid_request", "expected_sha256 is required")
        return await asyncio.to_thread(
            SourceService(self.s.root).execution_sources,
            [request["cell_id"]],
            {request["cell_id"]: request["expected_sha256"]},
        )

    async def accept(self, request: Any, client: Any, legacy: Any = None) -> Any:
        s = self.s
        request = {k: v for k, v in request.items() if k != "request_id"}
        method = request["method"]
        refuse_busy = method in ("checkpoint", "training-checkpoint", "training-restore") and (
            s.active is not None or s.command_lock.locked()
        )
        async with self.admission:
            if s.ledger.schema == 1:
                if method != "migrate":
                    raise SourceError("migration_required", "Migrate ledger to schema 2")
                if s.command_lock.locked() or s.active:
                    raise SourceError("busy", "migration requires an idle worker")
                # Migration is the only operation bootstrapped before the operations table exists.
                s.ledger.migrate(s.state / "ledger.sqlite.schema1.bak")
                with s.ledger.db:
                    s.ledger.change("migration", {"schema": 2})
            # Check for retries before reading mutable source: an accepted run stays accepted after edits.
            existing = s.ledger.rows(
                "SELECT * FROM operations WHERE client_id=? AND client_request_id=?",
                (client["client_id"], request["client_request_id"]),
            )
            if existing:
                row, _ = s.ledger.accept_operation(
                    client["client_id"], request["client_request_id"], method, request
                )
                return {"kind": "accepted", "operation_id": row["operation_id"]}
            if self.stop_requested and method != "stop":
                raise SourceError("busy", "Runtime is stopping; run again after it has stopped")
            frozen = await self.freeze(request) if method in ("run", "batch") else None
            if frozen is not None and not s.process:
                async with s.command_lock:
                    if not s.process:
                        await s.start_worker()
            s.ledger.db.commit()
            s.ledger.db.execute("PRAGMA synchronous=FULL")
            s.ledger.db.execute("BEGIN IMMEDIATE")
            row, fresh = s.ledger.accept_operation(
                client["client_id"], request["client_request_id"], method, request, commit=False
            )
            oid = row["operation_id"]
            executions: list[Any] = []
            if fresh:
                if frozen is not None:
                    try:
                        for cell in frozen:
                            digest = s.blobs.put(cell["source"].encode())
                            s.record_blob(digest)
                            eid = uid()
                            ordinal = s.ledger.db.execute(
                                "SELECT COALESCE(MAX(ordinal),0)+1 FROM executions WHERE session_id=?",
                                (s.session,),
                            ).fetchone()[0]
                            s.ledger.db.execute(
                                "INSERT INTO executions VALUES (?,?,?,?,?,?,?,?)",
                                (
                                    eid,
                                    s.session,
                                    ordinal,
                                    cell["id"],
                                    digest,
                                    "queued",
                                    now(),
                                    None,
                                ),
                            )
                            executions.append({"exec_id": eid, "session_id": s.session, **cell})
                        if method == "batch":
                            s.ledger.db.execute(
                                "INSERT INTO batches VALUES (?,?,?,?,?)",
                                (
                                    uid(),
                                    oid,
                                    s.session,
                                    request.get("on_error", "stop"),
                                    json.dumps([e["exec_id"] for e in executions]),
                                ),
                            )
                        s.ledger.db.execute(
                            "UPDATE operations SET result_json=? WHERE operation_id=?",
                            (
                                json.dumps(
                                    {
                                        "exec_ids": [e["exec_id"] for e in executions],
                                        "session_id": s.session,
                                    }
                                ),
                                oid,
                            ),
                        )
                        s.ledger.db.commit()
                    except BaseException:
                        s.ledger.db.rollback()
                        s.ledger.operation_state(oid, "failed", {"error": "acceptance failed"})
                        raise
                else:
                    s.ledger.db.commit()
                if refuse_busy:
                    s.ledger.operation_state(oid, "completed", {"refused": "busy"})
                    with s.ledger.db:
                        s.ledger.change(
                            "checkpoint_refused", {"operation_id": oid, "refused": "busy"}
                        )
                else:
                    if method == "stop":
                        self.stop_requested = True
                    s.spawn_task(self.perform(oid, request, executions, legacy))
            return {
                "kind": "accepted",
                "operation_id": oid,
                "executions": [e["exec_id"] for e in executions],
            }

    async def perform(self, oid: str, request: Any, executions: Any, legacy: Any = None) -> None:
        s = self.s
        try:
            # Stop is lifecycle control: it must not queue behind an indefinitely
            # running cell. end_worker aborts the active execution and wakes its waiter.
            if request["method"] == "stop":
                s.ledger.operation_state(oid, "running")
                await self.cancel_inputs()
                await s.end_worker()
                s.ledger.operation_state(oid, "completed", {"stopped": True})
                s.stopping.set()
                return
            async with s.command_lock:
                if self.stop_requested:
                    s.ledger.operation_state(oid, "canceled", {"reason": "runtime_stopped"})
                    for execution in executions:
                        s.ledger.write(
                            "UPDATE executions SET status='canceled',end=? WHERE exec_id=? AND status='queued'",
                            (now(), execution["exec_id"]),
                        )
                    return
                if oid in self.canceled:
                    return
                s.ledger.operation_state(oid, "running")
                method = request["method"]
                if method in ("run", "batch"):
                    results: list[Any] = []
                    for index, execution in enumerate(executions):
                        if oid in self.canceled:
                            break
                        if not s.process or execution["session_id"] != s.session:
                            raise SourceError(
                                "busy", "accepted worker session is no longer available"
                            )
                        result = await s.execute(
                            execution["source"], execution["id"], legacy, accepted=execution
                        )
                        results.append(result)
                        if result["status"] == "aborted" or (
                            result["status"] != "ok" and request.get("on_error", "stop") == "stop"
                        ):
                            for rest in executions[index + 1 :]:
                                s.ledger.write(
                                    "UPDATE executions SET status='canceled',end=? WHERE exec_id=? AND status='queued'",
                                    (now(), rest["exec_id"]),
                                )
                            break
                    result: Any = {"executions": results, "session_id": s.session}
                    if method == "run":
                        result = results[0] if results else {"status": "canceled"}
                elif method == "checkpoint":
                    result = await s.checkpoint()
                elif method == "training-checkpoint":
                    result = await s.training_checkpoint()
                elif method == "training-restore":
                    result = await s.training_restore(request)
                elif method in ("restore", "continue"):
                    result = await s.restore(request)
                elif method == "replay":
                    result = await s.replay(request)
                elif method == "restart":
                    await s.end_worker()
                    await s.start_worker()
                    result = {"session_id": s.session}
                elif method == "stop":
                    await self.cancel_inputs()
                    await s.end_worker()
                    result = {"stopped": True}
                    s.stopping.set()
                elif method == "migrate":
                    result = s.ledger.migrate(s.state / "ledger.sqlite.schema1.bak")
                else:
                    from ..sharing import mutate

                    if method == "import_ipynb":
                        rows = await asyncio.to_thread(s.notebook.import_ipynb, request["path"])
                        result = (
                            mutate("import_outputs", {"rows": rows}, s.notebook, s.ledger, s.blobs)
                            if request.get("keep_outputs")
                            else {"imported": True}
                        )
                    else:
                        result = mutate(method, request, s.notebook, s.ledger, s.blobs)
                s.ledger.operation_state(oid, "completed", result)
        except Exception as exception:
            for execution in executions:
                s.ledger.write(
                    "UPDATE executions SET status='canceled',end=? WHERE exec_id=? AND status='queued'",
                    (now(), execution["exec_id"]),
                )
            s.ledger.operation_state(oid, "failed", {"error": protocol.error(exception)})

    async def legacy_batch(self, request: Any, reader: Any, writer: Any) -> Any:
        if self.s.ledger.schema == 1:
            raise SourceError("migration_required", "kl run --all requires kl migrate")
        client = {"client_id": request.get("client_id", uid())}
        desktop = {
            "method": "batch",
            "client_request_id": request.get("client_request_id", uid()),
            "cell_ids": "all",
            "on_error": request.get("on_error", "stop"),
        }
        accepted = await self.accept(
            desktop, client, (reader, writer) if request.get("interactive") else None
        )
        oid = accepted["operation_id"]
        while True:
            row = self.operation({"operation_id": oid})
            if row["state"] in FINAL:
                if row["state"] == "failed":
                    raise ValueError(row["result"])
                return row["result"]
            await asyncio.sleep(0.05)

    @staticmethod
    def input_view(pending: Any) -> Any:
        return {k: v for k, v in pending.items() if k not in ("claimant", "writer", "reader")}

    def input_state(self, pending: Any, state: str = "pending") -> None:
        if self.s.ledger.schema == 2:
            with self.s.ledger.db:
                self.s.ledger.db.execute(
                    "UPDATE input_requests SET state=?,client_name=?,updated=? WHERE input_id=?",
                    (state, pending.get("client_name"), now(), pending["input_id"]),
                )
                self.s.ledger.change("input_state", {**self.input_view(pending), "state": state})

    async def input_requested(self, eid: str, message: Any) -> None:
        s = self.s
        s.flush_stream()
        s.ledger.db.commit()
        seq = s.ledger.db.execute("SELECT MAX(seq) FROM events WHERE exec_id=?", (eid,)).fetchone()[
            0
        ]
        iid = f"{eid}:{seq}"
        pending = {
            "input_id": iid,
            "exec_id": eid,
            "seq": seq,
            **message,
            "claimant": None,
            "client_name": None,
        }
        self.pending[iid] = pending
        if s.ledger.schema == 2:
            with s.ledger.db:
                s.ledger.db.execute(
                    "INSERT INTO input_requests VALUES (?,?,?,?,?,?,?,?)",
                    (iid, eid, seq, bool(message.get("password")), "pending", None, now(), now()),
                )
                s.ledger.change("input_state", self.input_view(pending))
        if s.replay_inputs is not None:
            reply = (
                s.replay_inputs.pop(0)
                if s.replay_inputs
                else {"error": "input was required; replay needs --reuse-input"}
            )
            await self.answer(pending, reply)
        elif s.input_client:
            from ..supervisor import read_message, send_message

            reader, writer = s.input_client
            pending.update(claimant=writer, client_name="CLI")
            self.input_state(pending)
            try:
                await send_message(
                    writer,
                    "stdin_request",
                    **message,
                    recorded=not message.get("password")
                    or s.config.get("record_secret_input", False),
                )
                incoming = await read_message(reader, s.blobs)
                if incoming["kind"] != "stdin_reply":
                    raise ValueError("expected stdin reply")
                if iid in self.pending:
                    await self.answer(
                        pending, {k: v for k, v in incoming.items() if k in ("text", "error")}
                    )
            except (OSError, ValueError, asyncio.IncompleteReadError):
                if iid in self.pending:
                    pending.update(claimant=None, client_name=None)
                    self.input_state(pending)
        elif getattr(s, "desktop_run", False):
            pass
        else:
            # Noninteractive legacy runs retain their EOF behavior. Desktop runs wait for a claim.
            running: Any = (
                s.ledger.rows(
                    "SELECT * FROM operations WHERE state='running' AND kind IN ('run','batch')"
                )
                if s.ledger.schema == 2
                else []
            )
            if not running:
                await self.answer(pending, {"error": "input was required but stdin is closed"})

    async def answer(self, pending: Any, reply: Any) -> None:
        s = self.s
        password = pending.get("password", False)
        recorded = (
            {"password": True, "redacted": True}
            if password and not s.config.get("record_secret_input", False)
            else {"password": password, **reply}
        )
        s.record(pending["exec_id"], "stdin_reply", recorded)
        self.input_state(pending, "canceled" if "error" in reply else "answered")
        self.pending.pop(pending["input_id"], None)
        await s.send_worker("stdin_reply", **reply)

    async def cancel_inputs(self) -> None:
        for pending in list(self.pending.values()):
            try:
                await self.answer(pending, {"error": "input canceled"})
            except (OSError, ValueError):
                self.input_state(pending, "canceled")
                self.pending.pop(pending["input_id"], None)

    async def input_control(self, method: str, request: Any, connection: Any) -> Any:
        pending = self.pending.get(request["input_id"])
        if pending is None:
            raise SourceError("not_found", "input request no longer pending")
        if method == "input.claim":
            if pending["claimant"] is not None and pending["claimant"] != connection:
                raise SourceError(
                    "input_claimed", "input claimed by another client", self.input_view(pending)
                )
            pending.update(claimant=connection, client_name=self.clients[connection]["client_name"])
            self.input_state(pending)
        else:
            if pending["claimant"] != connection:
                raise SourceError("input_claimed", "only the claimant may answer or release input")
            if method == "input.release":
                pending.update(claimant=None, client_name=None)
                self.input_state(pending)
            elif method == "input.cancel":
                await self.answer(pending, {"error": "input canceled"})
            elif method == "input.reply":
                if not isinstance(request.get("text"), str):
                    raise SourceError("invalid_request", "input reply must be text")
                await self.answer(pending, {"text": request["text"]})
        return self.input_view(pending)

    async def client(self, reader: Any, writer: Any) -> None:
        tasks: list[Any] = []
        write_lock = asyncio.Lock()
        last_traffic = time.monotonic()

        async def send(frame: Any) -> None:
            async with write_lock:
                await asyncio.wait_for(protocol.send(writer, frame), 5)

        try:
            greeting = await asyncio.wait_for(protocol.read(reader), 5)
            protocol.hello(greeting)
            if not isinstance(greeting.get("token"), str) or not secrets.compare_digest(
                greeting["token"], self.s.token
            ):
                raise SourceError("unauthenticated", "authentication failed")
            self.clients[writer] = greeting
            await send(
                {
                    "kind": "welcome",
                    "api": protocol.API,
                    "version": 1,
                    "capabilities": protocol.CAPABILITIES,
                    "identity": self.identity(),
                }
            )
            while not self.s.stopping.is_set():
                try:
                    request = await asyncio.wait_for(protocol.read(reader), 10)
                    last_traffic = time.monotonic()
                except TimeoutError:
                    if time.monotonic() - last_traffic >= 30:
                        break
                    await send({"kind": "ping"})
                    continue
                method = request.get("method", request.get("kind"))
                if method in ("ping", "pong"):
                    if method == "ping":
                        await send({"kind": "pong"})
                    continue
                try:
                    validate(request)
                    if self.s.ledger.schema == 1 and method not in ("runtime.status", "migrate"):
                        raise SourceError("migration_required", "Migrate ledger to schema 2")
                    role = greeting["role"]
                    if method == "blob.read":
                        if role != "blob":
                            raise SourceError(
                                "invalid_request", "blob.read requires a blob connection"
                            )
                        await self.blob(request, writer, write_lock)
                        continue
                    if method in ("subscribe", "execution.follow"):
                        if role != "subscription":
                            raise SourceError(
                                "invalid_request", "feed requires a subscription connection"
                            )
                        task = asyncio.create_task(self.follow(method, request, send))
                        tasks.append(task)
                        continue
                    if role != "control":
                        raise SourceError(
                            "invalid_request", "request requires a control connection"
                        )
                    if method == "runtime.status":
                        result = self.status()
                    elif method == "training.list":
                        result = self.s.training_list()
                    elif method == "training.inspect":
                        result = self.s.training_inspect(request["snapshot"])
                    elif method == "native.status":
                        result = await self.s.dispatch({"command": "native-checkpoint"}, None, None)
                    elif method in EFFECTS:
                        result = await self.accept(request, greeting)
                    elif method in ("interrupt", "interrupt_kill"):
                        result = await self.s.dispatch(
                            {"command": "interrupt", "kill": method == "interrupt_kill"}, None, None
                        )
                    elif method == "operation.get":
                        result = self.operation({**request, "client_id": greeting["client_id"]})
                    elif method == "operation.list":
                        result = protocol.page(
                            self.s.ledger.rows(
                                "SELECT * FROM operations ORDER BY accepted,operation_id"
                            ),
                            request,
                        )
                    elif method == "operation.cancel":
                        result = await self.cancel(request["operation_id"])
                    elif method == "changes.list":
                        result = protocol.page(
                            self.committed_changes(request.get("after_change_seq", 0)), request
                        )
                    elif method in ("input.claim", "input.release", "input.reply", "input.cancel"):
                        result = await self.input_control(method, request, writer)
                    else:
                        raise SourceError("invalid_request", "unknown desktop request")
                    await send(
                        {
                            "kind": "response",
                            "request_id": request.get("request_id"),
                            "result": result,
                        }
                    )
                except Exception as exception:
                    await send(
                        {
                            "kind": "error",
                            "request_id": request.get("request_id"),
                            "error": protocol.error(exception),
                        }
                    )
        except (asyncio.IncompleteReadError, ConnectionError, TimeoutError):
            pass
        except Exception as exception:
            try:
                await send({"kind": "error", "error": protocol.error(exception)})
            except (OSError, TimeoutError):
                pass
        finally:
            for task in tasks:
                task.cancel()
            await asyncio.gather(*tasks, return_exceptions=True)
            self.clients.pop(writer, None)
            for pending in self.pending.values():
                if pending["claimant"] == writer:
                    pending.update(claimant=None, client_name=None)
                    self.input_state(pending)
            writer.close()
            try:
                await writer.wait_closed()
            except OSError:
                pass

    def committed_changes(self, after: int) -> Any:
        ledger = Ledger(self.s.state / "ledger.sqlite", readonly=True)
        try:
            return [
                {**row, "ref": json.loads(row["ref_json"])}
                for row in ledger.rows(
                    "SELECT * FROM changes WHERE change_seq>? ORDER BY change_seq", (after,)
                )
            ]
        finally:
            ledger.close()

    async def follow(self, method: str, request: Any, send: Any) -> None:
        cursor = request.get("after_change_seq" if method == "subscribe" else "after_seq", 0)
        if method == "subscribe" and "after_change_seq" not in request:
            # No await between snapshot and watermark: both describe the same supervisor turn.
            cursor = self.s.ledger.db.execute(
                "SELECT COALESCE(MAX(change_seq),0) FROM changes"
            ).fetchone()[0]
            snapshot = self.status()
            await send({"kind": "snapshot", "change_seq": cursor, "runtime": snapshot})
        while True:
            if method == "subscribe":
                rows = await asyncio.to_thread(self.committed_changes, cursor)
                if len(rows) > 10000 or sum(len(json.dumps(row)) for row in rows) > 8388608:
                    await send({"kind": "resync_required", "after_change_seq": cursor})
                    return
                for row in rows:
                    await send(
                        {
                            "kind": "change",
                            "change_kind": row["kind"],
                            **{k: v for k, v in row.items() if k != "kind"},
                        }
                    )
                    cursor = row["change_seq"]
            else:

                def events() -> Any:
                    ledger = Ledger(self.s.state / "ledger.sqlite", readonly=True)
                    try:
                        return [e for e in ledger.events(request["exec_id"]) if e["seq"] > cursor]
                    finally:
                        ledger.close()

                rows = await asyncio.to_thread(events)
                for row in rows:
                    await send({"kind": "event", "exec_id": request["exec_id"], "event": row})
                    cursor = row["seq"]
            await asyncio.sleep(0.05)

    async def blob(self, request: Any, writer: Any, lock: Any) -> None:
        try:
            data = await asyncio.to_thread(self.s.blobs.get, request["sha256"])
        except FileNotFoundError:
            raise SourceError("not_found", "blob missing", {"sha256": request["sha256"]}) from None
        offset, length = request.get("offset", 0), request.get("length", max(1, len(data)))
        if (
            not isinstance(offset, int)
            or not isinstance(length, int)
            or offset < 0
            or offset > len(data)
            or length <= 0
        ):
            raise SourceError("invalid_request", "invalid blob range")
        end = min(len(data), offset + length)
        async with lock:
            for position in range(offset, max(offset + 1, end), protocol.LIMIT):
                chunk = data[position : min(end, position + protocol.LIMIT)]
                await protocol.send(
                    writer,
                    {
                        "kind": "blob",
                        "request_id": request.get("request_id"),
                        "sha256": request["sha256"],
                        "offset": position,
                        "byte_size": len(chunk),
                        "total_size": len(data),
                        "final": position + len(chunk) == end,
                        "verified": offset == 0
                        and end == len(data)
                        and position + len(chunk) == end,
                    },
                )
                writer.write(chunk)
                await writer.drain()
