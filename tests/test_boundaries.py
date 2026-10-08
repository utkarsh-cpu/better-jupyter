import concurrent.futures
import json
import subprocess
import sys
import time
from pathlib import Path

import pytest

from kernelledger.blobs import BlobStore
from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.ledger import Ledger
from kernelledger.notebook import Notebook
from kernelledger.sharing import Bundle


@pytest.fixture
def project(tmp_path):
    notebook = Notebook(tmp_path)
    notebook.init()
    try:
        yield notebook
    finally:
        if alive(tmp_path):
            rpc(tmp_path, {"command": "stop"})
        for _ in range(100):
            if not (tmp_path / ".kl/supervisor.json").exists():
                break
            time.sleep(0.02)


def execute(n, source):
    return rpc(n.root, {"command": "run", "source": source})


def test_stream_volume_truncation_and_denylist(project):
    (project.root / ".kl/config.json").write_text(
        json.dumps({"output_limit_bytes": 100000, "checkpoint_denylist": ["secret"]})
    )
    ensure_supervisor(project.root)
    result = execute(project, "secret='password'\nfor i in range(1000000):\n    print('line')")
    assert result["status"] == "ok"
    ledger = Ledger(project.root / ".kl/ledger.sqlite", readonly=True)
    try:
        events = ledger.events(result["exec_id"])
        assert len(events) < 100
        assert sum(len(e.get("text", "").encode()) for e in events) == 100000
        assert next(e for e in events if e["kind"] == "truncated")["dropped_bytes"] == 4900000
    finally:
        ledger.close()
    checkpoint = rpc(project.root, {"command": "checkpoint"})
    assert any(
        e["name"] == "secret" and e["reason"] == "denylisted" for e in checkpoint["excluded"]
    )


def test_busy_and_interrupt_kill(project):
    ensure_supervisor(project.root)
    with concurrent.futures.ThreadPoolExecutor() as pool:
        future = pool.submit(execute, project, "import time\ntime.sleep(60)")
        for _ in range(100):
            if alive(project.root)["in_flight"]:
                break
            time.sleep(0.02)
        assert rpc(project.root, {"command": "checkpoint"}) == {"refused": "busy"}
        rpc(project.root, {"command": "interrupt", "kill": True})
        with pytest.raises(ValueError, match="stopped"):
            future.result(timeout=10)
    assert execute(project, "assert 'time' not in globals()")["new_empty_session"]
    ledger = Ledger(project.root / ".kl/ledger.sqlite", readonly=True)
    try:
        assert any(r["status"] == "aborted" for r in ledger.rows("SELECT status FROM executions"))
        assert any(r["state"] == "killed" for r in ledger.rows("SELECT state FROM sessions"))
    finally:
        ledger.close()


def test_imported_outputs_inspect_without_worker(project, tmp_path):
    original = {
        "nbformat": 4,
        "metadata": {},
        "cells": [
            {
                "id": "one",
                "cell_type": "code",
                "metadata": {},
                "source": "raise RuntimeError('never execute')",
                "outputs": [{"output_type": "stream", "name": "stdout", "text": "imported output"}],
            }
        ],
    }
    path = tmp_path / "input.ipynb"
    path.write_text(json.dumps(original))
    rows = project.import_ipynb(path)
    from kernelledger.cli import offline_mutation

    offline_mutation(project.root, "import_outputs", {"rows": rows})
    assert not (project.root / ".kl/supervisor.json").exists()
    ledger = Ledger(project.root / ".kl/ledger.sqlite", readonly=True)
    eid = ledger.one("SELECT exec_id FROM executions")["exec_id"]
    ledger.close()
    output = subprocess.run(
        [sys.executable, "-m", "kernelledger", "-C", str(project.root), "show", eid],
        capture_output=True,
        text=True,
    )
    assert output.returncode == 0 and "imported output" in output.stdout
    assert not (project.root / ".kl/supervisor.json").exists()


def test_bundle_tamper_rejected(project, tmp_path):
    ensure_supervisor(project.root)
    execute(project, "a=[1,2,3]")
    cp = rpc(project.root, {"command": "checkpoint"})
    path = tmp_path / "bundle"
    rpc(project.root, {"command": "bundle_export", "path": str(path)})
    blob = next((path / "blobs").iterdir())
    blob.write_bytes(b"tampered")
    bundle = Bundle(path)
    try:
        assert bundle.inspect()["hash_mismatches"]
    finally:
        bundle.close()
    with pytest.raises(ValueError, match="hash mismatch"):
        rpc(project.root, {"command": "bundle_import", "path": str(path)})
    assert cp["checkpoint_id"]


@pytest.mark.parametrize(
    "stage", ["payloads_moved", "manifest_moved", "before_commit", "after_commit"]
)
def test_publish_crash_commit_point(project, stage):
    # A constructor-only test hook deterministically kills the supervisor at each boundary.
    code = f"""
import asyncio, os
from kernelledger.supervisor import Supervisor
s = Supervisor({str(project.root)!r})
def crash(stage):
    if stage == {stage!r}:
        os._exit(19)
s.publish_hook = crash
asyncio.run(s.run())
"""
    process = subprocess.Popen(
        [sys.executable, "-c", code], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    )
    try:
        for _ in range(200):
            if alive(project.root):
                break
            time.sleep(0.025)
        assert execute(project, "a=b'x'*100000")["status"] == "ok"
        with pytest.raises((OSError, EOFError)):
            rpc(project.root, {"command": "checkpoint"})
        assert process.wait(timeout=10) == 19
        ensure_supervisor(project.root)
        ledger = Ledger(project.root / ".kl/ledger.sqlite", readonly=True)
        try:
            checkpoints = ledger.rows("SELECT * FROM checkpoints")
            assert len(checkpoints) == (1 if stage == "after_commit" else 0)
            for row in checkpoints:
                BlobStore(project.root / ".kl").closure([row["manifest_sha256"]])
        finally:
            ledger.close()
        report = rpc(project.root, {"command": "gc"})
        if stage != "after_commit":
            assert report["removed"]
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)


def test_schema_contracts(project, tmp_path):
    import jsonschema

    ensure_supervisor(project.root)
    assert execute(project, "value={'a':[1,2]}")["status"] == "ok"
    cp = rpc(project.root, {"command": "checkpoint"})
    desc = rpc(project.root, {"command": "publish", "checkpoint_id": cp["checkpoint_id"]})
    bundle_path = tmp_path / "schema-bundle"
    rpc(project.root, {"command": "bundle_export", "path": str(bundle_path)})
    schemas = Path(__file__).parents[1] / "schemas"

    def validate(name, value):
        jsonschema.validate(value, json.loads((schemas / (name + ".schema.json")).read_text()))

    validate("notebook", project.read())
    validate("descriptor", desc)
    validate("bundle", json.loads((bundle_path / "MANIFEST.json").read_text()))
    manifest = json.loads(BlobStore(project.root / ".kl").get(desc["manifest_sha256"]))
    validate("checkpoint", manifest)
    validate(
        "protocol",
        {
            "schema": 1,
            "kind": "hello",
            "token": "test",
            "python": sys.version.split()[0],
            "implementation": sys.implementation.name,
            "executable": sys.executable,
            "platform": sys.platform,
            "cwd": str(project.root),
        },
    )
    ledger = Ledger(project.root / ".kl/ledger.sqlite", readonly=True)
    try:
        from kernelledger.ledger import TABLES

        validate(
            "ledger",
            {
                "schema": 1,
                "tables": {table: ledger.rows("SELECT * FROM " + table) for table in TABLES},
            },
        )
    finally:
        ledger.close()


def test_soft_interrupt_sleep(project):
    ensure_supervisor(project.root)
    with concurrent.futures.ThreadPoolExecutor() as pool:
        future = pool.submit(execute, project, "import time\ntime.sleep(60)")
        for _ in range(100):
            if alive(project.root)["in_flight"]:
                break
            time.sleep(0.02)
        rpc(project.root, {"command": "interrupt"})
        assert future.result(timeout=5)["status"] == "interrupted"
    assert execute(project, "2+2")["status"] == "ok"


def test_recheckpoint_functions_and_dataclass_redefinition(project):
    ensure_supervisor(project.root)
    execute(
        project, "from dataclasses import dataclass\n@dataclass\nclass A:\n    x: int\nold = A(1)"
    )
    execute(project, "@dataclass\nclass A:\n    y: int\nnew = A(2)\ng = 4\ndef f():\n    return g")
    first = rpc(project.root, {"command": "checkpoint"})
    assert first["status"] == "complete", first
    rpc(project.root, {"command": "restore", "checkpoint_id": first["checkpoint_id"]})
    assert execute(project, "assert old.x==1 and new.y==2 and f()==4")["status"] == "ok"
    second = rpc(project.root, {"command": "checkpoint"})
    assert second["status"] == "complete", second
    rpc(project.root, {"command": "restore", "checkpoint_id": second["checkpoint_id"]})
    assert execute(project, "g=8\nassert f()==8 and old.x==1 and new.y==2")["status"] == "ok"


def test_token_permissions_and_standalone_worker(project, tmp_path):
    from kernelledger.supervisor.platform import secure_token

    isolated = tmp_path / "isolated"
    subprocess.run([sys.executable, "-m", "venv", "--without-pip", str(isolated)], check=True)
    python = isolated / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
    ensure_supervisor(project.root, str(python))
    assert (
        execute(project, "import sys\nassert 'kernelledger' not in sys.modules\nx=4")["status"]
        == "ok"
    )
    assert Path(alive(project.root)["worker_python"]).resolve() == python.resolve()
    assert secure_token(project.root / ".kl/token", verify=True)
    cp = rpc(project.root, {"command": "checkpoint"})
    assert cp["status"] == "complete"
    rpc(project.root, {"command": "restore", "checkpoint_id": cp["checkpoint_id"]})
    assert execute(project, "assert x==4")["status"] == "ok"


def test_interactive_input_and_replay_reuse(project):
    command = [
        sys.executable,
        "-m",
        "kernelledger",
        "-C",
        str(project.root),
        "run",
        "--source",
        "answer=input('value: ')\nassert answer=='recorded'\nprint(answer)",
    ]
    response = subprocess.run(
        command, input="recorded\n", capture_output=True, text=True, timeout=20
    )
    assert response.returncode == 0, response.stderr
    sid = alive(project.root)["session_id"]
    replay = rpc(project.root, {"command": "replay", "session_id": sid})
    assert replay["executions"][0]["status"] == "error"
    replay = rpc(project.root, {"command": "replay", "session_id": sid, "reuse_input": True})
    assert replay["executions"][0]["status"] == "ok"


def test_matplotlib_and_pandas_display(project):
    pytest.importorskip("matplotlib")
    pytest.importorskip("pandas")
    ensure_supervisor(project.root)
    result = execute(
        project,
        "import matplotlib.pyplot as plt\nimport pandas as pd\nplt.plot([1,2,3]); plt.show()\npd.DataFrame({'x':[1,2]})",
    )
    assert result["status"] == "ok"
    ledger = Ledger(project.root / ".kl/ledger.sqlite", readonly=True)
    try:
        events = ledger.events(result["exec_id"])
        assert any("image/png" in e.get("bundle", {}) for e in events)
        assert any("text/html" in e.get("bundle", {}) for e in events)
    finally:
        ledger.close()
