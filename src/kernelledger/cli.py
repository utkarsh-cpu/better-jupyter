"""KernelLedger CLI. Inspection never starts a worker or supervisor."""

import argparse
import getpass
import json
import os
import socket
import sqlite3
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, cast

from .blobs import BlobStore
from .ledger import Ledger
from .notebook import EXTENSIONS, Notebook
from .sharing import Bundle, notebook_outputs, output_view, payload
from .supervisor.platform import DirectoryLock, secure_token
from .worker import Channel, receive


def connect(root: Any) -> Any:
    state = root / ".kl"
    info = json.loads((state / "supervisor.json").read_text())
    sock = socket.create_connection(("127.0.0.1", info["port"]), timeout=2)
    sock.settimeout(None)
    channel = Channel(sock)
    channel.send("auth", token=(state / "token").read_text())
    return sock, channel


def rpc(root: Any, request: Any) -> Any:
    sock, channel = connect(root)
    blobs: dict[str, bytes] = {}
    try:
        channel.send("request", **request)
        while True:
            message = receive(sock)
            if message["kind"] == "blob":
                blobs[message["sha256"]] = message["data"]
                continue
            if message["kind"] == "envelope":
                message = json.loads(blobs[message["blob"]])
            if message["kind"] == "notice":
                print(message["message"], flush=True)
                continue
            if message["kind"] == "stdin_request":
                print(
                    "Reply recorded in history and bundles"
                    if message.get("recorded", True)
                    else "Secret reply is not recorded",
                    file=sys.stderr,
                )
                try:
                    text = (
                        getpass.getpass(message["prompt"])
                        if message.get("password")
                        else input(message["prompt"])
                    )
                    channel.send(
                        "stdin_reply",
                        text=text,
                        **({"input_id": message["input_id"]} if "input_id" in message else {}),
                    )
                except EOFError:
                    channel.send(
                        "stdin_reply",
                        error="input was required but stdin is closed",
                        **({"input_id": message["input_id"]} if "input_id" in message else {}),
                    )
                continue
            if "error" in message:
                raise ValueError(message["error"])
            return message["result"]
    finally:
        sock.close()


def alive(root: Any) -> Any:
    try:
        return rpc(root, {"command": "status"})
    except (OSError, ValueError, KeyError):
        return None


def runtime_command(
    root: Path, python: Any = None, *, distribution: str = "installed", launch: str = "terminal"
) -> list[str]:
    mode = os.environ.get("KERNELLEDGER_RUNTIME", "python") or "python"
    if mode not in ("python", "native"):
        raise ValueError("KERNELLEDGER_RUNTIME must be python or native")
    if mode == "python":
        command = [sys.executable, "-m", "kernelledger", "--project", str(root), "serve"]
        if python:
            command.extend(["--python", str(python)])
        return command
    binary = Path(os.environ.get("KERNELLEDGER_DAEMON", ""))
    if not binary.is_absolute() or not binary.is_file():
        raise ValueError(
            "Native startup requires an absolute KERNELLEDGER_DAEMON path to an installed binary"
        )
    from .supervisor import interpreter

    config_path = root / ".kl/config.json"
    config: dict[str, Any] = json.loads(config_path.read_text()) if config_path.exists() else {}
    selected, _ = interpreter(root, python, config, distribution, launch)
    return [
        str(binary),
        "serve-experimental",
        "--project",
        str(root.resolve()),
        "--python",
        os.path.abspath(selected),
    ]


def ensure_supervisor(root: Any, python: Any = None) -> Any:
    if alive(root):
        return
    Notebook(root).read()
    state = root / ".kl"
    state.mkdir(exist_ok=True)
    args = runtime_command(root, python)
    env: dict[Any, Any]
    env = dict(os.environ)
    env["PYTHONPATH"] = str(Path(__file__).parents[1]) + os.pathsep + env.get("PYTHONPATH", "")
    from .supervisor.platform import HIDDEN_CONSOLE

    kwargs: dict[str, Any] = (
        {"creationflags": HIDDEN_CONSOLE} if os.name == "nt" else {"start_new_session": True}
    )
    with (state / "supervisor.log").open("ab") as log:
        process = subprocess.Popen(
            args, stdin=subprocess.DEVNULL, stdout=log, stderr=log, env=env, **kwargs
        )
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if alive(root):
            return
        if process.poll() is not None:
            # Another starting CLI may have won the lock.
            if alive(root):
                return
            raise ValueError(
                "supervisor failed to start: "
                + (state / "supervisor.log").read_text(errors="replace")[-4000:]
            )
        time.sleep(0.1)
    raise ValueError("supervisor startup timed out; see .kl/supervisor.log")


def parser() -> Any:
    p = argparse.ArgumentParser(
        prog="kl", description="Source notebooks with durable Python execution history"
    )
    p.add_argument("--project", "-C", default=".", help="notebook directory")
    sub = p.add_subparsers(dest="command", required=True)
    from .assistant.cli import add_parser

    add_parser(sub)
    gpu = sub.add_parser("experimental-gpu", help="isolated v5 prototypes; no runtime migration")
    gpu.add_argument("--store", default=".kl-gpu-experimental")
    gpu_commands = gpu.add_subparsers(dest="gpu_command", required=True)
    gpu_commands.add_parser("inventory", help="inspect metadata without loading a GPU framework")
    gpu_probe = gpu_commands.add_parser("probe", help="run a disposable active portable probe")
    gpu_probe.add_argument("--python", default=sys.executable)
    gpu_probe.add_argument("--device", default="cpu")
    gpu_probe.add_argument("--timeout", type=float, default=60)
    gpu_inspect = gpu_commands.add_parser("inspect")
    gpu_inspect.add_argument("snapshot")
    gpu_export = gpu_commands.add_parser("export")
    gpu_export.add_argument("snapshot")
    gpu_export.add_argument("destination")
    gpu_import = gpu_commands.add_parser("import")
    gpu_import.add_argument("source")
    gpu_gc = gpu_commands.add_parser("gc")
    gpu_gc.add_argument("--apply", action="store_true")
    gpu_source = gpu_commands.add_parser("source-status")
    gpu_source.add_argument("snapshot")
    gpu_materialize = gpu_commands.add_parser("materialize")
    gpu_materialize.add_argument("snapshot")
    gpu_materialize.add_argument("destination")
    for name in ("init", "stop", "status", "restart", "log", "checkpoint", "retarget"):
        sub.add_parser(name)
    for name in ("training-checkpoint", "training-list", "native-checkpoint"):
        sub.add_parser(name)
    training_inspect = sub.add_parser("training-inspect")
    training_inspect.add_argument("snapshot")
    training_restore = sub.add_parser("training-restore")
    training_restore.add_argument("snapshot")
    training_restore.add_argument("--trust", action="store_true")
    training_restore.add_argument("--device", default="cpu")
    training_restore.add_argument("--python")
    gc = sub.add_parser("gc")
    gc.add_argument("--dry-run", action="store_true")
    sub.add_parser("migrate")
    runtimes = sub.add_parser("runtimes")
    runtimes.add_argument("--prune", action="store_true")
    operation = sub.add_parser("operation")
    operation.add_argument("operation_id")
    cancel = sub.add_parser("cancel")
    cancel.add_argument("operation_id")
    serve = sub.add_parser("serve")
    serve.add_argument("--python")
    serve.add_argument("--distribution", choices=("installed", "private"), default="installed")
    serve.add_argument("--launch", choices=("terminal", "desktop"), default="terminal")
    doctor = sub.add_parser("doctor")
    doctor.add_argument("--repair-ids", action="store_true")
    add = sub.add_parser("add")
    add.add_argument("type", choices=list(EXTENSIONS))
    add.add_argument("--id")
    add.add_argument("--source", default="")
    add.add_argument("--file")
    rm = sub.add_parser("rm")
    rm.add_argument("cell_id")
    run = sub.add_parser("run")
    run.add_argument("cell_id", nargs="?")
    run.add_argument("--all", action="store_true")
    run.add_argument("--keep-going", action="store_true")
    run.add_argument("--source")
    run.add_argument("--python")
    run.add_argument("--non-interactive", action="store_true")
    interrupt = sub.add_parser("interrupt")
    interrupt.add_argument("--kill", action="store_true")
    show = sub.add_parser("show")
    show.add_argument("exec_id")
    show.add_argument("--events", action="store_true")
    show.add_argument("--mime")
    show.add_argument("--out")
    show.add_argument("--source", action="store_true")
    for name in ("restore", "continue", "replay"):
        command = sub.add_parser(name)
        command.add_argument("session_id" if name == "replay" else "checkpoint_id")
        command.add_argument("--trust", action="store_true")
        command.add_argument("--python")
        command.add_argument("--allow-python-mismatch", action="store_true")
        command.add_argument("--reuse-input", action="store_true")
    imp = sub.add_parser("import")
    imp.add_argument("path")
    imp.add_argument("--keep-outputs", action="store_true")
    exp = sub.add_parser("export")
    exp.add_argument("path")
    exp.add_argument("--session")
    pub = sub.add_parser("publish")
    pub.add_argument("checkpoint_id")
    fetch = sub.add_parser("fetch")
    fetch.add_argument("path")
    pin = sub.add_parser("pin")
    pin.add_argument("name")
    pin.add_argument("checkpoint_id")
    unpin = sub.add_parser("unpin")
    unpin.add_argument("name")
    bundle = sub.add_parser("bundle").add_subparsers(dest="bundle_command", required=True)
    for name in ("export", "show", "import"):
        command = bundle.add_parser(name)
        command.add_argument("path")
        if name == "export":
            command.add_argument("--session", action="append", dest="sessions")
            command.add_argument("--checkpoint", action="append", dest="checkpoints")
            command.add_argument("--external", action="append")
    return p


def emit(value: Any) -> Any:
    print(json.dumps(value, ensure_ascii=False, indent=2))


def render(view: Any, blobs: Any) -> Any:
    print(f"{view['exec_id']}  {view['status']}  cell={view['cell_id'] or 'unattributed'}")
    if view["output_mismatch"]:
        print(
            f"source: {view['source_sha256']}  current: {view['current_source_sha256'] or 'deleted'}"
        )
    for event in view["outputs"]:
        kind = event["kind"]
        if kind in ("stdout", "stderr"):
            print(
                event.get("text") if "text" in event else blobs.get(event["blob"]).decode(), end=""
            )
        elif kind in ("display", "result"):
            value = event["bundle"].get("text/plain") or event["bundle"].get("text/markdown")
            if value:
                print(payload(value, blobs))
        elif kind == "error":
            print("".join(event["traceback"]))
        elif kind == "truncated":
            print(f"<stream output truncated: {event['dropped_bytes']} bytes dropped>")


def offline_mutation(root: Any, command: Any, request: Any) -> Any:
    from .sharing import mutate

    if alive(root):
        return rpc(root, {"command": command, **request})
    state = root / ".kl"
    lock = DirectoryLock(state).acquire()
    ledger = None
    try:
        ledger = Ledger(state / "ledger.sqlite")
        return mutate(command, request, Notebook(root), ledger, BlobStore(state))
    finally:
        if ledger:
            ledger.close()
        lock.close()


def execute_cli(args: Any) -> Any:
    root = Path(args.project).resolve()
    notebook, command = Notebook(root), args.command
    if command == "ai":
        from .assistant.cli import execute

        return execute(root, args)
    if command == "mcp":
        from .assistant.mcp import main

        main(root)
        return 0
    if command == "experimental-gpu":
        from .artifacts import ARTIFACT_SCHEMA, ArtifactStore
        from .experiments import inspect_experiment, materialize_source
        from .git_state import experiment_status
        from .gpu.backends.capabilities import inventory, probe
        from .gpu.portable import inspect_state

        action = args.gpu_command
        if action == "inventory":
            emit(inventory())
        elif action == "probe":
            report = probe(python=args.python, device=args.device, timeout=args.timeout)
            emit(report)
            return 0 if report["status"] == "experimental" else 1
        else:
            store = ArtifactStore(
                root / args.store, readonly=action in ("inspect", "source-status")
            )
            if action == "inspect":
                metadata = store.metadata(args.snapshot)
                if metadata.get("schema") == ARTIFACT_SCHEMA:
                    emit(store.verify(args.snapshot))
                elif metadata.get("kind") == "training-experiment":
                    emit(inspect_experiment(store, args.snapshot))
                else:
                    emit(inspect_state(store, args.snapshot))
            elif action == "export":
                store.export(args.snapshot, Path(args.destination))
                emit({"snapshot": args.snapshot, "destination": args.destination})
            elif action == "import":
                emit({"snapshot": store.import_directory(Path(args.source))})
            elif action == "gc":
                emit(store.gc(dry_run=not args.apply))
            elif action == "source-status":
                emit(experiment_status(store, args.snapshot, root))
            elif action == "materialize":
                materialize_source(store, args.snapshot, Path(args.destination))
                emit({"snapshot": args.snapshot, "destination": args.destination})
        return 0
    if command == "serve":
        if os.environ.get("KERNELLEDGER_RUNTIME", "python") not in ("", "python"):
            return subprocess.call(
                runtime_command(
                    root, args.python, distribution=args.distribution, launch=args.launch
                )
            )
        from .supervisor import main

        main(root, args.python, args.distribution, args.launch)
        return 0
    if command == "runtimes":
        from .registry import list_runtimes

        emit(list_runtimes(args.prune))
        return 0
    if command == "migrate":
        print(
            "Migration saves ledger.sqlite.schema1.bak. Version 3 clients will refuse schema 2; restoring the backup discards later history."
        )
        if alive(root):
            emit(rpc(root, {"command": "migrate"}))
        else:
            lock = DirectoryLock(root / ".kl").acquire()
            ledger = None
            try:
                ledger = Ledger(root / ".kl/ledger.sqlite")
                emit(ledger.migrate(root / ".kl/ledger.sqlite.schema1.bak"))
            finally:
                if ledger:
                    ledger.close()
                lock.close()
        return 0
    if command == "init":
        notebook.init()
        emit({"id": notebook.read()["id"], "project": str(root)})
        return 0
    if command == "bundle" and args.bundle_command == "show":
        bundle = Bundle(args.path)
        try:
            report = bundle.inspect()
            emit(report)
            return 1 if report["hash_mismatches"] else 0
        finally:
            bundle.close()
    if command != "doctor":
        notebook.read()
    if command == "add":
        print(
            notebook.add(
                args.type,
                Path(args.file).read_text(encoding="utf-8") if args.file else args.source,
                args.id,
            )
        )
    elif command == "rm":
        notebook.remove(args.cell_id)
    elif command == "retarget":
        if alive(root):
            rpc(root, {"command": "stop"})
            for _ in range(100):
                if not (root / ".kl/supervisor.json").exists():
                    break
                time.sleep(0.02)
        from .source import SourceService

        source = SourceService(root)
        current = source.read()
        result = source.mutate(
            {"action": "retarget", "manifest_revision": current["manifest_revision"]}
        )
        emit({"id": result["manifest"]["id"]})
    elif command == "doctor":
        report = notebook.doctor(args.repair_ids)
        state = root / ".kl"
        if (state / "ledger.sqlite").exists():
            ledger = Ledger(state / "ledger.sqlite", readonly=True)
            try:
                result = ledger.db.execute("PRAGMA quick_check").fetchone()[0]
                if result != "ok":
                    report["issues"].append("ledger: " + result)
                report["recorded_password_replies"] = ledger.db.execute(
                    "SELECT COUNT(*) FROM events WHERE kind='stdin_reply' AND json_extract(payload_json,'$.password')=1 AND json_extract(payload_json,'$.text') IS NOT NULL"
                ).fetchone()[0]
            finally:
                ledger.close()
        if (state / "token-permission-error").exists():
            report["issues"].append("token ACL: " + (state / "token-permission-error").read_text())
        if (state / "token").exists():
            try:
                if not secure_token(state / "token", verify=True):
                    report["issues"].append(
                        "token permissions must grant access to the current user only"
                    )
            except OSError as error:
                report["issues"].append("cannot check token permissions: " + str(error))
        from .supervisor import interpreter

        config: dict[str, Any] = (
            json.loads((state / "config.json").read_text())
            if (state / "config.json").exists()
            else {}
        )
        python, choice = interpreter(root, None, config)
        report["interpreter"] = {"path": python, "choice": choice}
        if not Path(python).is_file():
            report["issues"].append("worker interpreter missing")
        for sibling in config.get("siblings", []):
            try:
                other = Notebook(sibling)
                if other.root != root and other.read()["id"] == report["id"]:
                    report["issues"].append(f"shared notebook id: {other.root}")
            except (ValueError, OSError):
                pass
        emit(report)
        return 1 if report["issues"] else 0
    elif command == "import":
        rows = notebook.import_ipynb(args.path)
        if args.keep_outputs:
            emit(offline_mutation(root, "import_outputs", {"rows": rows}))
    elif command == "export":
        outputs: dict[Any, Any]
        outputs = {}
        if args.session:
            ledger = Ledger(root / ".kl" / "ledger.sqlite", readonly=True)
            try:
                outputs = notebook_outputs(ledger, BlobStore(root / ".kl"), args.session)
            finally:
                ledger.close()
        notebook.export_ipynb(args.path, outputs)
    elif command in ("log", "show"):
        if command == "show":
            try:
                cell = notebook.cell(args.exec_id)
                if cell["type"] != "code":
                    print(notebook.source(cell), end="")
                    return 0
            except ValueError:
                pass
        ledger = Ledger(root / ".kl" / "ledger.sqlite", readonly=True)
        try:
            if command == "log":
                emit(
                    {
                        "sessions": ledger.rows("SELECT * FROM sessions ORDER BY start"),
                        "executions": ledger.rows(
                            "SELECT * FROM executions ORDER BY session_id,ordinal"
                        ),
                        "checkpoints": ledger.rows("SELECT * FROM checkpoints ORDER BY timestamp"),
                    }
                )
            else:
                blobs = BlobStore(root / ".kl")
                view = output_view(notebook, ledger, blobs, args.exec_id, args.events)
                if args.source:
                    print(view["source"], end="")
                elif args.mime:
                    if not args.out:
                        raise ValueError("--mime requires --out")
                    for event in reversed(view["outputs"]):
                        if args.mime in event.get("bundle", {}):
                            value = payload(event["bundle"][args.mime], blobs)
                            Path(args.out).write_bytes(
                                value
                                if isinstance(value, bytes)
                                else (
                                    value if isinstance(value, str) else json.dumps(value)
                                ).encode()
                            )
                            break
                    else:
                        raise ValueError("requested MIME type is absent")
                elif args.events:
                    emit(view)
                else:
                    render(view, blobs)
        finally:
            ledger.close()
    elif command == "fetch":
        data = json.loads(Path(args.path).read_text())
        if data.get("schema") != 1:
            raise ValueError("unsupported descriptor schema")
        missing: list[Any]
        invalid: list[Any]
        blobs, missing, invalid = BlobStore(root / ".kl"), [], []
        for digest in data["blob_hashes"]:
            try:
                blobs.get(digest)
            except FileNotFoundError:
                missing.append(digest)
            except ValueError:
                invalid.append(digest)
        emit(
            {
                "checkpoint_id": data["checkpoint_id"],
                "missing_blobs": missing,
                "invalid_blobs": invalid,
                "origin": "descriptor",
            }
        )
        return 1 if missing or invalid else 0
    elif command in ("gc", "pin", "unpin", "publish", "bundle"):
        request = dict(cast(dict[str, Any], vars(args)))
        if "path" in request:
            request["path"] = str(Path(request["path"]).resolve())
        request.pop("command", None)
        emit(
            offline_mutation(
                root, "bundle_" + args.bundle_command if command == "bundle" else command, request
            )
        )
    elif command == "run":
        if sum([bool(args.cell_id), args.all, args.source is not None]) != 1:
            raise ValueError("choose one cell id, --all, or --source")
        ensure_supervisor(root, args.python)
        if args.all:
            result = rpc(
                root,
                {
                    "command": "batch",
                    "interactive": not args.non_interactive,
                    "on_error": "continue" if args.keep_going else "stop",
                },
            )
            ledger = Ledger(root / ".kl/ledger.sqlite", readonly=True)
            try:
                blobs = BlobStore(root / ".kl", readonly=True)
                for execution in result["executions"]:
                    render(output_view(notebook, ledger, blobs, execution["exec_id"]), blobs)
            finally:
                ledger.close()
            return 1 if any(e["status"] != "ok" for e in result["executions"]) else 0
        requests = (
            [{"cell_id": cell["id"]} for cell in notebook.read()["cells"] if cell["type"] == "code"]
            if args.all
            else [{"source": args.source}]
            if args.source is not None
            else [{"cell_id": args.cell_id}]
        )
        for request in requests:
            result = rpc(
                root, {"command": "run", "interactive": not args.non_interactive, **request}
            )
            ledger = Ledger(root / ".kl" / "ledger.sqlite", readonly=True)
            try:
                blobs = BlobStore(root / ".kl")
                render(output_view(notebook, ledger, blobs, result["exec_id"]), blobs)
            finally:
                ledger.close()
            if result["status"] != "ok":
                return 1
    elif command in ("restore", "continue", "replay", "training-restore"):
        print(
            "This command executes saved source; its file and network side effects can happen again."
        )
        ensure_supervisor(root, args.python)
        request = dict(cast(dict[str, Any], vars(args)))
        request.pop("project")
        emit(rpc(root, request))
    elif command in ("training-list", "training-inspect", "native-checkpoint"):
        from .desktop.helper import Helper

        method = {
            "training-list": "training.list",
            "training-inspect": "training.inspect",
            "native-checkpoint": "native.status",
        }[command]
        emit(
            Helper(root).dispatch(
                {
                    "method": method,
                    **({"snapshot": args.snapshot} if command == "training-inspect" else {}),
                }
            )
        )
    elif command == "status":
        emit(alive(root) or {"running": False})
    else:
        if not alive(root):
            raise ValueError("supervisor is down; run or restore to start a namespace")
        emit(
            rpc(root, {k: v for k, v in cast(dict[str, Any], vars(args)).items() if k != "project"})
        )
    return 0


def main() -> Any:
    args = parser().parse_args()
    try:
        code = execute_cli(args)
    except (OSError, ValueError, KeyError, EOFError, sqlite3.DatabaseError) as error:
        print(f"kl: {error}", file=sys.stderr)
        code = 1
    except KeyboardInterrupt:
        print("kl: command interrupted; use kl interrupt to interrupt the worker", file=sys.stderr)
        code = 130
    raise SystemExit(code)
