"""Isolated native fixture; no credentials or personal notebook state are used."""

import argparse
import json
import os
import signal
import subprocess
import tempfile
from pathlib import Path

from kernelledger.blobs import BlobStore
from kernelledger.ledger import Ledger
from kernelledger.notebook import Notebook

REPO = Path(__file__).resolve().parents[2]
SESSION = Path(tempfile.gettempdir()) / "kl-gpui-refinement-session.json"


def launch(python):
    if SESSION.exists():
        raise SystemExit("Stop the previous fixture before launching another.")
    root = Path(tempfile.mkdtemp(prefix="kl-native-refinement-"))
    project = root / "Precision notebook"
    project.mkdir()
    notebook = Notebook(project)
    notebook.init()
    notebook.add(
        "markdown", "# Experiment ledger\n\nCompare recorded results and current source.", "intro"
    )
    notebook.add("code", "values = [2.4, 2.7, 3.1]\nprint(sum(values) / len(values))", "measure")
    notebook.add(
        "code", 'description = "' + "precision source — " * 12 + '"\nprint(description)', "longline"
    )
    ledger = Ledger(project / ".kl/ledger.sqlite")
    blobs = BlobStore(project / ".kl")
    ledger.db.execute(
        "INSERT INTO sessions(session_id,notebook_id,state,start,environment_json,supervisor_json) VALUES ('review-session','fixture','stopped','2026-11-08T00:00:00Z','{}','{}')"
    )
    for i in range(240):
        source = "needle\n" + "value = 42\n" * 200 if i == 225 else f"value = {i}"
        digest = blobs.put(source.encode())
        ledger.db.execute(
            "INSERT INTO executions VALUES (?,?,?,?,?,?,?,?)",
            (
                f"exec{i}",
                "review-session",
                i + 1,
                "measure",
                digest,
                "ok",
                "2026-11-08T00:00:00Z",
                None,
            ),
        )
    ledger.db.commit()
    ledger.db.close()
    profile = root / "profile/kernelledger/desktop"
    profile.mkdir(parents=True)
    (profile / "appearance.txt").write_text("dark")
    env = {
        **os.environ,
        "XDG_STATE_HOME": str(root / "profile"),
        "XDG_CONFIG_HOME": str(root / "config"),
    }
    env.pop("KL_RUNTIME_PYTHON", None)
    if python:
        env["KL_RUNTIME_PYTHON"] = python
    with (root / "app.log").open("w") as log:
        app = subprocess.Popen(
            [str(REPO / "desktop/target/debug/kernelledger-desktop"), str(project)],
            cwd=REPO,
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
    state = {"pid": app.pid, "root": str(root), "project": str(project)}
    SESSION.write_text(json.dumps(state))
    print(json.dumps(state, indent=2))


def stop():
    state = json.loads(SESSION.read_text())
    current = subprocess.run(
        ["ps", "-p", str(state["pid"]), "-o", "command="],
        capture_output=True,
        text=True,
        check=False,
    ).stdout.strip()
    if current and not (
        current.startswith(str(REPO / "desktop/target/debug/kernelledger-desktop"))
        and state["project"] in current
    ):
        raise SystemExit("Fixture PID belongs to another process; it was not terminated.")
    if current:
        os.kill(state["pid"], signal.SIGTERM)
    subprocess.run(
        [
            str(REPO / ".venv/bin/python"),
            "-m",
            "kernelledger",
            "--project",
            state["project"],
            "stop",
        ],
        cwd=REPO,
        check=False,
        timeout=15,
    )
    SESSION.unlink()
    print("Fixture stopped; its disposable notebook remains available for inspection.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("launch", "stop"))
    parser.add_argument("--python", help="Explicit helper Python; omit to verify discovery")
    args = parser.parse_args()
    launch(args.python) if args.action == "launch" else stop()
