"""Isolated framed stdio process. EOF/cancellation cannot block the desktop helper."""

import argparse
import json
import os
import queue
import struct
import sys
import threading
from pathlib import Path
from typing import Any, cast

from ..desktop.protocol import LIMIT, encode
from ..source import SourceError
from . import API, VERSION
from .service import Assistant


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project", required=True)
    args = parser.parse_args()
    if os.name == "nt":
        import msvcrt

        msvcrt.setmode(sys.stdin.fileno(), os.O_BINARY)
        msvcrt.setmode(sys.stdout.fileno(), os.O_BINARY)
    lock = threading.Lock()
    assistant: Assistant | None = None
    pending: queue.Queue[Any] = queue.Queue()
    busy = threading.Event()
    active_id: Any = None

    def send(value: Any) -> None:
        with lock:
            sys.stdout.buffer.write(encode(value))
            sys.stdout.buffer.flush()

    def execute(request: Any) -> None:
        assert assistant is not None
        rid = request.get("request_id")
        try:
            result = assistant.dispatch(request, lambda value: send({**value, "request_id": rid}))
            busy.clear()
            send({"kind": "done", "request_id": rid, "result": result})
        except Exception as exception:
            busy.clear()
            code = getattr(exception, "code", "internal")
            # Never echo arbitrary exception text from providers, source, or credentials.
            message = (
                str(exception) if isinstance(exception, SourceError) else "assistant request failed"
            )
            send(
                {
                    "kind": "error",
                    "request_id": rid,
                    "error": {"code": code, "message": message, "retryable": False, "details": {}},
                }
            )
        finally:
            busy.clear()

    def work() -> None:
        try:
            while True:
                request = pending.get()
                if request is None:
                    break
                execute(request)
        finally:
            if assistant:
                assistant.close()

    worker = threading.Thread(target=work, daemon=True)
    worker.start()

    try:
        while True:
            header = sys.stdin.buffer.read(4)
            if not header:
                break
            if len(header) != 4:
                break
            size = struct.unpack(">I", header)[0]
            if size > LIMIT:
                break
            raw = sys.stdin.buffer.read(size)
            if len(raw) != size:
                break
            request = json.loads(raw)
            if not isinstance(request, dict):
                break
            request = cast(dict[str, Any], request)
            if assistant is None:
                if (
                    request.get("kind") != "hello"
                    or request.get("api") != API
                    or VERSION not in request.get("versions", [])
                ):
                    send(
                        {
                            "kind": "error",
                            "error": {
                                "code": "unsupported_version",
                                "message": "expected assistant v1 hello",
                                "retryable": False,
                                "details": {},
                            },
                        }
                    )
                    break
                assistant = Assistant(Path(args.project), request.get("credential", ""))
                send({"kind": "welcome", "api": API, "version": VERSION})
                continue
            if request.get("method") == "cancel":
                if request.get("target_request_id") in (None, active_id):
                    assistant.cancel()
                send(
                    {
                        "kind": "done",
                        "request_id": request.get("request_id"),
                        "result": {"cancelled": True},
                    }
                )
                continue
            if busy.is_set():
                send(
                    {
                        "kind": "error",
                        "request_id": request.get("request_id"),
                        "error": {
                            "code": "busy",
                            "message": "another assistant request is running",
                            "retryable": True,
                            "details": {},
                        },
                    }
                )
                continue
            active_id = request.get("request_id")
            assistant.cancelled.clear()
            busy.set()
            pending.put(request)
    finally:
        if assistant:
            assistant.cancel()
            pending.put(None)
            worker.join(timeout=1)


if __name__ == "__main__":
    main()
