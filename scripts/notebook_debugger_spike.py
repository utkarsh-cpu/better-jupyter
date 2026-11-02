"""Disposable DAP evaluation; never imported by the worker or desktop.

Run with an explicitly installed debugpy 1.8.x environment. The fixture deliberately
measures whether a variables request calls user repr. This unauthenticated local
debug port belongs only to the disposable fixture and is not a product endpoint.
"""

import argparse
import hashlib
import importlib.metadata
import json
import linecache
import os
import platform
import socket
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = """repr_calls = 0
class Watch:
    def __repr__(self):
        global repr_calls
        repr_calls += 1
        return 'WATCH'
value = 40
watch = Watch()
debugpy.breakpoint()
value += 2
value += 1
"""


def child(directory):
    import debugpy

    config = json.loads((directory / "fixture.json").read_text())
    filename = config["filename"]
    debugpy.configure(subProcess=False)
    address = debugpy.listen(("127.0.0.1", 0))
    endpoint = directory / "endpoint.json"
    pending = directory / "endpoint.pending"
    pending.write_text(json.dumps({"host": address[0], "port": address[1]}))
    pending.replace(endpoint)
    debugpy.wait_for_client()
    linecache.cache[filename] = (len(SOURCE), None, SOURCE.splitlines(True), filename)
    namespace = {"debugpy": debugpy, "__name__": "__main__"}
    exec(compile(SOURCE, filename, "exec"), namespace)
    (directory / "result.json").write_text(
        json.dumps({key: namespace[key] for key in ("value", "repr_calls")})
    )


class Dap:
    def __init__(self, endpoint):
        self.socket = socket.create_connection((endpoint["host"], endpoint["port"]), timeout=20)
        self.buffer = bytearray()
        self.seq = 0
        self.pending = []
        self.commands = []
        self.bytes_read = 0

    def send(self, command, **arguments):
        self.seq += 1
        self.commands.append(command)
        message = json.dumps(
            {"seq": self.seq, "type": "request", "command": command, "arguments": arguments}
        ).encode()
        self.socket.sendall(f"Content-Length: {len(message)}\r\n\r\n".encode() + message)
        return self.seq

    def receive(self):
        def more():
            chunk = self.socket.recv(65536)
            if not chunk:
                raise EOFError("DAP fixture disconnected")
            self.bytes_read += len(chunk)
            if self.bytes_read > 4 * 1024 * 1024:
                raise ValueError("DAP spike response budget exceeded")
            self.buffer.extend(chunk)

        while b"\r\n\r\n" not in self.buffer:
            if len(self.buffer) > 8192:
                raise ValueError("Oversized DAP header")
            more()
        header, _, body = self.buffer.partition(b"\r\n\r\n")
        fields = dict(line.split(b":", 1) for line in bytes(header).split(b"\r\n"))
        length = int(fields[b"Content-Length"].strip())
        if not 0 < length <= 262144:
            raise ValueError("Oversized DAP body")
        self.buffer = body
        while len(self.buffer) < length:
            more()
        message = json.loads(self.buffer[:length])
        del self.buffer[:length]
        return message

    def wait(self, predicate):
        deadline = time.monotonic() + 20
        while True:
            for index, message in enumerate(self.pending):
                if predicate(message):
                    return self.pending.pop(index)
            remaining = deadline - time.monotonic()
            if remaining <= 0 or len(self.pending) >= 512:
                raise TimeoutError("DAP fixture reply deadline exceeded")
            self.socket.settimeout(remaining)
            self.pending.append(self.receive())

    def response(self, sequence):
        message = self.wait(
            lambda item: item.get("type") == "response" and item.get("request_seq") == sequence
        )
        if not message.get("success"):
            raise RuntimeError(message)
        return message.get("body", {})

    def request(self, command, **arguments):
        return self.response(self.send(command, **arguments))

    def event(self, event):
        return self.wait(
            lambda item: item.get("type") == "event" and item.get("event") == event
        ).get("body", {})


def trial(directory, materialized):
    directory.mkdir()
    filename = directory / "cells" / f"probe.py@{uuid.uuid4()}"
    if materialized:
        filename.parent.mkdir()
        filename.write_text(SOURCE, encoding="utf-8")
    (directory / "fixture.json").write_text(json.dumps({"filename": str(filename)}))
    dap = None
    with (directory / "child.log").open("w+", encoding="utf-8") as log:
        process = subprocess.Popen(
            [sys.executable, str(Path(__file__).resolve()), "--child", str(directory)],
            stdout=log,
            stderr=log,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
        )
        try:
            deadline = time.monotonic() + 25
            endpoint = directory / "endpoint.json"
            while not endpoint.exists():
                if time.monotonic() > deadline or process.poll() is not None:
                    raise RuntimeError("Debugpy fixture did not start")
                time.sleep(0.025)
            dap = Dap(json.loads(endpoint.read_text()))
            capabilities = dap.request(
                "initialize",
                adapterID="debugpy",
                clientID="kernelledger-disposable-spike",
                pathFormat="path",
                linesStartAt1=True,
                columnsStartAt1=True,
            )
            attach = dap.send("attach", justMyCode=False, subProcess=False)
            dap.event("initialized")
            breakpoints = dap.request(
                "setBreakpoints", source={"path": str(filename)}, breakpoints=[{"line": 11}]
            )
            dap.request("configurationDone")
            dap.response(attach)
            stopped = dap.event("stopped")
            thread_id = stopped["threadId"]
            frame = dap.request("stackTrace", threadId=thread_id, startFrame=0, levels=1)[
                "stackFrames"
            ][0]
            assert frame["line"] == 10, frame
            scopes = dap.request("scopes", frameId=frame["id"])["scopes"]
            locals_scope = next(scope for scope in scopes if scope["name"] == "Locals")
            variables = dap.request(
                "variables",
                variablesReference=locals_scope["variablesReference"],
                start=0,
                count=100,
            )["variables"]
            source = frame.get("source", {})
            source_matches = None
            if source.get("sourceReference"):
                source_matches = (
                    dap.request("source", sourceReference=source["sourceReference"], source=source)[
                        "content"
                    ]
                    == SOURCE
                )
            dap.request("next", threadId=thread_id)
            stepped = dap.event("stopped")
            next_frame = dap.request("stackTrace", threadId=stepped["threadId"], levels=1)[
                "stackFrames"
            ][0]
            assert next_frame["line"] == 11, next_frame
            # Remove the breakpoint before resuming; it may coincide with this step.
            dap.request("setBreakpoints", source={"path": str(filename)}, breakpoints=[])
            dap.request("continue", threadId=stepped["threadId"])
            process.wait(timeout=20)
            assert process.returncode == 0
            result = json.loads((directory / "result.json").read_text())
            assert result["value"] == 43, result
            return {
                "materialized_source": materialized,
                "initial_breakpoints": breakpoints,
                "stopped_line": frame["line"],
                "stepped_line": next_frame["line"],
                "source_reference": source.get("sourceReference", 0),
                "source_response_matches": source_matches,
                "watch_variable_present": any(row["name"] == "watch" for row in variables),
                "user_repr_calls_after_variables_request": result["repr_calls"],
                "supports_evaluate_for_hovers": capabilities.get("supportsEvaluateForHovers"),
                "dap_commands": dap.commands,
                "value_after_resume": result["value"],
            }
        finally:
            if dap:
                dap.socket.close()
            if process.poll() is None:
                if os.name == "nt":
                    subprocess.run(
                        ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                        capture_output=True,
                        timeout=15,
                    )
                else:
                    process.terminate()
                process.wait(timeout=10)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--child", type=Path)
    parser.add_argument(
        "--output", type=Path, default=ROOT / "artifacts/notebook-interactivity/debugger-spike.json"
    )
    args = parser.parse_args()
    if args.child:
        child(args.child)
        return
    with tempfile.TemporaryDirectory(prefix="kl-dap-spike-") as temporary:
        results = [trial(Path(temporary) / mode, mode == "file") for mode in ("virtual", "file")]
    report = {
        "schema": "kernelledger.debugger-spike/1",
        "platform": platform.platform(),
        "python": sys.version,
        "debugpy": importlib.metadata.version("debugpy"),
        "fixture_sha256": hashlib.sha256(SOURCE.encode()).hexdigest(),
        "worker_sha256": hashlib.sha256(
            (ROOT / "src/kernelledger/worker.py").read_bytes()
        ).hexdigest(),
        "production_adopted": False,
        "transport": "disposable unauthenticated loopback DAP; no notebook owner or ledger",
        "trials": results,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
