"""Runtime hints. Authentication, never a pid, establishes liveness."""

import hashlib
import json
import os
import socket
import struct
from pathlib import Path
from typing import Any

from .source import atomic


def directory() -> Path:
    if os.name == "nt":
        return (
            Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData/Local"))
            / "KernelLedger/runtimes"
        )
    return (
        Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state"))
        / "kernelledger/runtimes"
    )


def entry_path(root: Any) -> Path:
    canonical = os.path.normcase(str(Path(root).resolve()))
    return directory() / (hashlib.sha256(canonical.encode()).hexdigest() + ".json")


def publish(root: Any, identity: Any) -> Path:
    path = entry_path(root)
    atomic(path, json.dumps(identity).encode())
    return path


def list_runtimes(prune: bool = False) -> Any:
    result: list[Any] = []
    for path in directory().glob("*.json"):
        try:
            info: Any = json.loads(path.read_bytes())
            root = Path(info["project_path"])
            token = (root / ".kl/token").read_text()
            with socket.create_connection(("127.0.0.1", info["port"]), timeout=1) as sock:
                for frame in (
                    {"schema": 1, "kind": "auth", "token": token},
                    {"schema": 1, "kind": "request", "command": "status"},
                ):
                    data = json.dumps(frame).encode()
                    sock.sendall(struct.pack(">I", len(data)) + data)
                from .worker import receive

                response = receive(sock)
                if "error" in response:
                    raise ValueError(response["error"])
                info.update(state="verified", runtime=response["result"])
        except (OSError, ValueError, KeyError, EOFError):
            info = {"registry_path": str(path), "state": "stale"}
            if prune:
                path.unlink(missing_ok=True)
                info["pruned"] = True
        result.append(info)
    return result
