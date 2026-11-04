"""Optional local Jupyter transport attached to the existing notebook owner.

No ipykernel process, execution engine, or ledger writer is created here. All
side effects use durable desktop run admission and its original idempotency key.
"""

import base64
import importlib
import json
import os
import queue
import socket
import struct
import tempfile
import threading
import uuid
from pathlib import Path
from typing import Any, cast

from .blobs import BlobStore
from .desktop.protocol import API, LIMIT, encode
from .ledger import Ledger
from .notebook import Notebook, canonical, source_hash
from .source import SourceError


def execution_key(message: Any) -> str:
    return str(
        uuid.uuid5(
            uuid.NAMESPACE_URL,
            f"kernelledger:{message['header']['session']}:{message['header']['msg_id']}",
        )
    )


def kernelspec(project: Path, name: str, install: bool = False) -> Any:
    """Generate first; only an explicit install writes the user's Jupyter directory."""
    import re
    import sys

    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,63}", name):
        raise ValueError(
            "Kernelspec name must contain only letters, digits, dots, underscores and hyphens"
        )
    Notebook(project).read()
    spec = {
        "argv": [
            sys.executable,
            "-m",
            "kernelledger",
            "-C",
            str(project.resolve()),
            "jupyter-adapter",
            "-f",
            "{connection_file}",
        ],
        "display_name": f"KernelLedger · {name} (shared session)",
        "language": "python",
        "metadata": {"kernelledger": {"shutdown": "detach adapter; shared owner remains running"}},
    }
    installed = None
    if install:
        try:
            manager = importlib.import_module("jupyter_client.kernelspec").KernelSpecManager()
        except ImportError:
            raise ValueError(
                "Install KernelLedger[jupyter] in the adapter environment first"
            ) from None
        if name in manager.find_kernel_specs():
            raise ValueError("A kernelspec with this name already exists; choose a new name")
        with tempfile.TemporaryDirectory(prefix="kl-kernelspec-") as directory:
            (Path(directory) / "kernel.json").write_text(json.dumps(spec), encoding="utf-8")
            installed = manager.install_kernel_spec(directory, kernel_name=name, user=True)
    return {"name": name, "kernel_spec": spec, "installed_path": installed}


def connection_info(path: Path) -> Any:
    if path.stat().st_size > 65536:
        raise ValueError("Connection file exceeds 64 KiB")
    if os.name != "nt" and path.stat().st_mode & 0o077:
        raise ValueError("Jupyter connection file must be private (chmod 600)")
    if os.name == "nt":
        from .supervisor.platform import secure_token

        if not secure_token(path):
            raise ValueError("Could not restrict the Jupyter connection file to this Windows user")
    info = json.loads(path.read_text(encoding="utf-8"))
    if info.get("ip") not in ("127.0.0.1", "::1") or info.get("transport", "tcp") != "tcp":
        raise ValueError("Jupyter adapter supports loopback TCP only")
    if (
        not isinstance(info.get("key"), str)
        or len(info["key"]) < 16
        or info.get("signature_scheme", "hmac-sha256") != "hmac-sha256"
    ):
        raise ValueError("A signed hmac-sha256 Jupyter connection is required")
    ports = [
        info.get(channel + "_port") for channel in ("shell", "iopub", "stdin", "control", "hb")
    ]
    if (
        any(type(port) is not int or not 1 <= port <= 65535 for port in ports)
        or len(set(ports)) != 5
    ):
        raise ValueError("Connection ports must be five distinct TCP ports")
    return info


class RuntimeConnection:
    def __init__(self, project: Path, identity: str) -> None:
        info = json.loads((project / ".kl/supervisor.json").read_text(encoding="utf-8"))
        self.socket = socket.create_connection(("127.0.0.1", info["desktop_port"]), timeout=5)
        self.socket.settimeout(5)
        self.file = self.socket.makefile("rb")
        self.socket.sendall(
            encode(
                {
                    "kind": "hello",
                    "api": API,
                    "versions": [1],
                    "client_id": identity,
                    "client_name": "Jupyter adapter",
                    "role": "control",
                    "token": (project / ".kl/token").read_text(encoding="utf-8"),
                }
            )
        )
        if self.read().get("kind") != "welcome":
            self.close()
            raise ValueError("Owner rejected the Jupyter adapter")

    def read(self) -> Any:
        header = self.file.read(4)
        if len(header) != 4:
            raise EOFError("Runtime disconnected; execution outcome may be unknown")
        size = struct.unpack(">I", header)[0]
        if size > LIMIT:
            raise ValueError("Owner frame exceeds 1 MiB")
        data = self.file.read(size)
        if len(data) != size:
            raise EOFError("Runtime disconnected during reply")
        return json.loads(data)

    def call(self, method: str, **fields: Any) -> Any:
        request_id = str(uuid.uuid4())
        self.socket.sendall(encode({"method": method, "request_id": request_id, **fields}))
        while True:
            reply = self.read()
            if reply.get("kind") == "ping":
                self.socket.sendall(encode({"kind": "pong"}))
                continue
            if reply.get("request_id") != request_id:
                raise ValueError("Uncorrelated owner response")
            if reply.get("kind") == "error":
                raise SourceError(reply["error"]["code"], reply["error"]["message"])
            return reply["result"]

    def close(self) -> None:
        self.file.close()
        self.socket.close()


class Adapter:
    """Bounded local adapter. ZMQ sockets stay exclusively on the poll thread."""

    def __init__(self, project: Path, connection_file: Path) -> None:
        try:
            zmq: Any = importlib.import_module("zmq")
            Session: Any = importlib.import_module("jupyter_client.session").Session
        except ImportError:
            raise ValueError(
                "Install KernelLedger[jupyter] in the adapter environment; the project worker needs no Jupyter packages"
            ) from None
        info = connection_info(connection_file)
        self.zmq: Any = zmq
        self.session: Any = Session(key=info["key"].encode(), signature_scheme="hmac-sha256")
        self.project = project
        self.identity = str(uuid.uuid5(uuid.NAMESPACE_URL, str(connection_file.resolve())))
        # Attach only: fail before binding any public transport if no owner exists.
        client = RuntimeConnection(project, self.identity)
        try:
            self.runtime = client.call("runtime.status")
        finally:
            client.close()
        self.context: Any = zmq.Context()
        self.sockets: dict[str, Any] = {}
        self.poller: Any = zmq.Poller()
        try:
            for channel, kind in (
                ("shell", zmq.ROUTER),
                ("control", zmq.ROUTER),
                ("stdin", zmq.ROUTER),
                ("iopub", zmq.PUB),
                ("hb", zmq.REP),
            ):
                stream = self.context.socket(kind)
                stream.setsockopt(zmq.LINGER, 0)
                stream.setsockopt(zmq.SNDHWM, 64)
                stream.setsockopt(zmq.RCVHWM, 64)
                stream.setsockopt(zmq.MAXMSGSIZE, 1024 * 1024)
                stream.setsockopt(zmq.SNDTIMEO, 1000)
                address = f"[{info['ip']}]" if ":" in info["ip"] else info["ip"]
                stream.bind(f"tcp://{address}:{info[channel + '_port']}")
                self.sockets[channel] = stream
                if channel != "iopub":
                    self.poller.register(stream, zmq.POLLIN)
        except BaseException:
            self.close()
            raise
        self.stopping = threading.Event()
        self.outputs: queue.Queue[Any] = queue.Queue(maxsize=64)
        self.inputs: queue.Queue[str] = queue.Queue(maxsize=1)
        self.execution: threading.Thread | None = None
        self.input_parent: str | None = None
        self.input_identity: Any = None

    def send(
        self, channel: str, message_type: str, content: Any, parent: Any, identity: Any = None
    ) -> Any:
        return self.session.send(
            self.sockets[channel], message_type, content=content, parent=parent, ident=identity
        )

    def enqueue(self, item: Any) -> None:
        while not self.stopping.is_set():
            try:
                self.outputs.put(item, timeout=0.1)
                return
            except queue.Full:
                continue

    def event(self, event: Any, blobs: BlobStore, count: int) -> tuple[str, Any] | None:
        payload = json.loads(event["payload_json"])
        kind = event["kind"]
        if kind in ("stdout", "stderr"):
            text = payload.get("text")
            if text is None and payload.get("blob"):
                digest = payload["blob"]
                if (blobs.root / digest).stat().st_size <= 65536:
                    text = blobs.get(digest).decode(errors="replace")
            return "stream", {
                "name": kind,
                "text": (text or "[large stream available in KernelLedger History]")[:65536],
            }
        if kind in ("display", "display_update", "result"):
            data: dict[str, Any] = {}
            for mime, item in payload.get("bundle", {}).items():
                if "value" in item:
                    data[mime] = item["value"]
                elif "blob" in item and (blobs.root / item["blob"]).stat().st_size <= 65536:
                    value = blobs.get(item["blob"])
                    data[mime] = (
                        base64.b64encode(value).decode()
                        if item.get("encoding") == "binary"
                        else json.loads(value)
                        if item.get("encoding") == "json"
                        else value.decode(errors="replace")
                    )
            data.setdefault(
                "text/plain", "[Recorded output; larger attachments are available in KernelLedger]"
            )
            result: dict[str, Any] = {"data": data, "metadata": payload.get("metadata", {})}
            if payload.get("display_id"):
                result["transient"] = {"display_id": payload["display_id"]}
            if kind == "result":
                result["execution_count"] = count
            return {
                "display": "display_data",
                "display_update": "update_display_data",
                "result": "execute_result",
            }[kind], result
        if kind == "error":
            return "error", {
                key: payload.get(key, [] if key == "traceback" else "")
                for key in ("ename", "evalue", "traceback")
            }
        if kind == "clear":
            return "clear_output", {"wait": bool(payload.get("wait"))}
        return None

    def execute(self, identities: Any, message: Any) -> None:
        client: RuntimeConnection | None = None
        ledger: Ledger | None = None
        count = 0
        try:
            client = RuntimeConnection(self.project, self.identity)
            source = canonical(message["content"]["code"])
            accepted = client.call(
                "run",
                source=source,
                expected_sha256=source_hash(source),
                origin="jupyter",
                client_request_id=execution_key(message),
            )
            oid = accepted["operation_id"]
            # Retries return the original operation, which may already be final.
            operation = client.call("operation.get", operation_id=oid)
            execution_ids = accepted.get("executions", []) or operation.get("result", {}).get(
                "exec_ids", []
            )
            if not execution_ids and operation.get("result", {}).get("exec_id"):
                execution_ids = [operation["result"]["exec_id"]]
            if not execution_ids:
                raise ValueError("Execution was not admitted; inspect operation " + oid)
            eid = execution_ids[0]
            ledger = Ledger(self.project / ".kl/ledger.sqlite", readonly=True)
            count = ledger.one("SELECT ordinal FROM executions WHERE exec_id=?", (eid,))["ordinal"]
            if not message["content"].get("silent", False):
                self.enqueue(
                    (
                        "iopub",
                        "execute_input",
                        {"code": source, "execution_count": count},
                        message,
                        None,
                    )
                )
            blobs = BlobStore(self.project / ".kl", readonly=True)
            cursor = 0
            announced: set[str] = set()
            final: Any = None
            while not self.stopping.is_set():
                rows = ledger.rows(
                    "SELECT * FROM events WHERE exec_id=? AND seq>? ORDER BY seq LIMIT 100",
                    (eid, cursor),
                )
                for row in rows:
                    cursor = row["seq"]
                    converted = self.event(row, blobs, count)
                    if converted and not message["content"].get("silent", False):
                        self.enqueue(("iopub", converted[0], converted[1], message, None))
                if final is not None and not rows:
                    break
                operation = client.call("operation.get", operation_id=oid)
                if operation["state"] in ("completed", "failed", "unknown", "canceled"):
                    final = operation
                    continue
                status = client.call("runtime.status")
                for pending in status.get("pending_inputs", []):
                    if pending["exec_id"] != eid or pending["input_id"] in announced:
                        continue
                    iid = pending["input_id"]
                    if not message["content"].get("allow_stdin", True):
                        client.call("input.cancel", input_id=iid)
                        continue
                    client.call("input.claim", input_id=iid)
                    announced.add(iid)
                    self.enqueue(
                        (
                            "stdin",
                            "input_request",
                            {
                                "prompt": pending.get("prompt", ""),
                                "password": pending.get("password", False),
                            },
                            message,
                            identities,
                        )
                    )
                    while not self.stopping.is_set():
                        try:
                            text = self.inputs.get(timeout=0.1)
                        except queue.Empty:
                            continue
                        client.call("input.reply", input_id=iid, text=text)
                        break
                self.stopping.wait(0.02)
            outcome: Any = final.get("result", {}).get("status") if final else None
            content = cast(
                Any,
                {
                    "status": "ok",
                    "execution_count": count,
                    "payload": [],
                    "user_expressions": {
                        name: {
                            "status": "error",
                            "ename": "UnsupportedEvaluation",
                            "evalue": "Use an accepted execution for expressions",
                            "traceback": [],
                        }
                        for name in message["content"].get("user_expressions", {})
                    },
                }
                if outcome == "ok"
                else {
                    "status": "error",
                    "execution_count": count,
                    "ename": "KernelLedgerExecution",
                    "evalue": str(outcome or "unknown outcome; inspect operation " + oid),
                    "traceback": [],
                },
            )
            self.enqueue(("shell", "execute_reply", content, message, identities))
        except Exception as error:
            self.enqueue(
                (
                    "shell",
                    "execute_reply",
                    {
                        "status": "error",
                        "execution_count": count,
                        "ename": type(error).__name__,
                        "evalue": str(error),
                        "traceback": [],
                    },
                    message,
                    identities,
                )
            )
        finally:
            if ledger:
                ledger.close()
            if client:
                client.close()
            self.enqueue(("iopub", "status", {"execution_state": "idle"}, message, None))

    def handle(self, channel: str, identities: Any, message: Any) -> bool:
        kind, content = message["header"]["msg_type"], message["content"]
        reply: Any = {}
        if kind == "kernel_info_request":
            reply = {
                "protocol_version": "5.3",
                "implementation": "kernelledger-adapter",
                "implementation_version": "0.1.0",
                "language_info": {
                    "name": "python",
                    "version": self.runtime.get("interpreter", {}).get("python", "3"),
                    "mimetype": "text/x-python",
                    "file_extension": ".py",
                },
                "banner": "KernelLedger shared notebook session (local adapter; widgets/debug messages unsupported)",
                "help_links": [],
            }
        elif kind == "execute_request":
            if self.execution and self.execution.is_alive():
                reply = {
                    "status": "error",
                    "ename": "Busy",
                    "evalue": "One execute request per adapter; retry with the same message identity",
                    "traceback": [],
                }
            else:
                self.send("iopub", "status", {"execution_state": "busy"}, message)
                self.execution = threading.Thread(
                    target=self.execute, args=(identities, message), daemon=True
                )
                self.execution.start()
                return True
        elif kind == "shutdown_request":
            self.send(
                channel,
                "shutdown_reply",
                {"restart": bool(content.get("restart"))},
                message,
                identities,
            )
            return False  # Detach the adapter; never stop the shared owner.
        elif kind == "input_reply" and channel == "stdin":
            if self.input_parent == message["header"].get(
                "session"
            ) and self.input_identity == tuple(identities):
                try:
                    text = content.get("value", "")
                    if type(text) is not str or len(text.encode()) > 65536:
                        raise ValueError("Input reply exceeds 64 KiB")
                    self.inputs.put_nowait(text)
                except queue.Full:
                    pass
                self.input_parent = None
            return True
        else:
            client = RuntimeConnection(self.project, self.identity)
            try:
                status = client.call("runtime.status")
                generation = status.get("interactive", {}).get("worker_generation")
                if kind == "interrupt_request":
                    client.call("interrupt")
                    reply = {"status": "ok"}
                elif kind == "complete_request":
                    result = client.call(
                        "code.complete",
                        source=content["code"],
                        cursor=content["cursor_pos"],
                        source_revision=source_hash(content["code"]),
                        worker_generation=generation,
                    )
                    reply = {
                        "status": "ok",
                        "matches": result.get("candidates", []),
                        "cursor_start": result.get("start", content["cursor_pos"]),
                        "cursor_end": result.get("end", content["cursor_pos"]),
                        "metadata": {"kernelledger": result.get("error", {})},
                    }
                elif kind == "inspect_request":
                    import re

                    match = re.search(r"[\w.]+$", content["code"][: content["cursor_pos"]])
                    result = client.call(
                        "code.inspect", name=match[0] if match else "", worker_generation=generation
                    )
                    reply = {
                        "status": "ok",
                        "found": not bool(result.get("error")),
                        "data": {
                            "text/plain": result.get("documentation", result.get("preview", ""))
                        },
                        "metadata": {},
                    }
                elif kind == "is_complete_request":
                    reply = {"status": "unknown"}
                elif kind == "history_request":
                    history = Ledger(self.project / ".kl/ledger.sqlite", readonly=True)
                    try:
                        sid = status["session_id"]
                        size = min(max(int(content.get("n", 100)), 1), 100)
                        rows = history.rows(
                            "SELECT ordinal,source_sha256 FROM executions WHERE session_id=? ORDER BY ordinal DESC LIMIT ?",
                            (sid, size),
                        )
                        blobs = BlobStore(self.project / ".kl", readonly=True)
                        items: list[Any] = []
                        for row in reversed(rows):
                            if content.get("hist_access_type") == "range" and not content.get(
                                "start", 0
                            ) <= row["ordinal"] < content.get("stop", 2**63 - 1):
                                continue
                            digest = row["source_sha256"]
                            source = (
                                blobs.get(digest).decode()
                                if (blobs.root / digest).stat().st_size <= 65536
                                else "[source exceeds the adapter history page limit]"
                            )
                            if content.get("hist_access_type") == "search":
                                import fnmatch

                                if not fnmatch.fnmatchcase(source, content.get("pattern", "*")):
                                    continue
                            items.append(
                                (
                                    0,
                                    row["ordinal"],
                                    (source, "") if content.get("output") else source,
                                )
                            )
                        reply = {
                            "history": items,
                            "kernelledger": {
                                "session_id": sid,
                                "limit": 100,
                                "output_cache": "recorded output is available in KernelLedger History",
                            },
                        }
                    finally:
                        history.close()
                elif kind == "comm_info_request":
                    reply = {"comms": {}}
                else:
                    reply = {
                        "status": "error",
                        "ename": "UnsupportedMessage",
                        "evalue": kind,
                        "traceback": [],
                    }
            finally:
                client.close()
        self.send(channel, kind.replace("_request", "_reply"), reply, message, identities)
        if kind == "kernel_info_request":
            self.send("iopub", "status", {"execution_state": "idle"}, message)
        return True

    def run(self) -> None:
        try:
            while not self.stopping.is_set():
                for _ in range(64):
                    try:
                        channel, kind, content, parent, identities = self.outputs.get_nowait()
                    except queue.Empty:
                        break
                    self.send(channel, kind, content, parent, identities)
                    if channel == "stdin":
                        # Session.send generates the request header. The reply
                        # is authenticated; track the active parent execution.
                        self.input_parent = parent["header"]["session"]
                        self.input_identity = tuple(identities)
                ready = dict(self.poller.poll(20))
                for channel in ("control", "hb", "stdin", "shell"):
                    stream = self.sockets[channel]
                    if stream not in ready:
                        continue
                    if channel == "hb":
                        stream.send(stream.recv())
                        continue
                    try:
                        identities, message = self.session.recv(stream, mode=0)
                    except ValueError:
                        # Invalid signatures are discarded without dispatch.
                        continue
                    if message is None:
                        continue
                    try:
                        if not self.handle(channel, identities, message):
                            return
                    except Exception as error:
                        kind = message["header"]["msg_type"]
                        self.send(
                            channel,
                            kind.replace("_request", "_reply"),
                            {
                                "status": "error",
                                "ename": type(error).__name__,
                                "evalue": str(error),
                                "traceback": [],
                            },
                            message,
                            identities,
                        )
        finally:
            self.stopping.set()
            if self.execution:
                self.execution.join(timeout=1)
            self.close()

    def close(self) -> None:
        for stream in self.sockets.values():
            stream.close(linger=0)
        self.context.term()


def main(project: Path, connection_file: Path) -> None:
    Adapter(project, connection_file).run()
