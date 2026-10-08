"""Create real Git worktrees containing code, data, and portable kernel checkpoints.

Run after installing KernelLedger: python examples/worktrees/demo.py --output <new-directory>
The destination must not exist. All runtimes are stopped on exit; artifacts are retained.
"""

import argparse
import hashlib
import json
import platform
import shutil
import subprocess
import sys
import time
from pathlib import Path

from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.notebook import Notebook
from kernelledger.sharing import Bundle

TEMPLATE = Path(__file__).parent / "template"


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8", newline="\n")


def git(root, *args):
    result = subprocess.run(
        ["git", "-C", str(root), *args], capture_output=True, text=True, check=True
    )
    return result.stdout.strip()


def execute(root, **request):
    result = rpc(root, {"command": "run", **request})
    if result["status"] != "ok":
        raise RuntimeError(f"Execution failed in {root}: {result}; inspect with kl show")
    return result


def stop(root):
    if alive(root):
        rpc(root, {"command": "stop"})
    deadline = time.monotonic() + 10
    while (root / ".kl/supervisor.json").exists():
        if time.monotonic() >= deadline:
            raise RuntimeError(f"Supervisor did not stop in {root}")
        time.sleep(0.05)


def snapshot(root, name, parent=None):
    execute(
        root,
        source="pathlib.Path('.kl/result.json').write_text(json.dumps({"
        "'model': model, 'metrics': metrics, 'training_steps': training_steps}))",
    )
    values = json.loads((root / ".kl/result.json").read_text())
    checkpoint = rpc(root, {"command": "checkpoint"})
    if checkpoint.get("status") != "complete" or checkpoint.get("excluded"):
        raise RuntimeError(f"Example requires a complete checkpoint: {checkpoint}")
    cid = checkpoint["checkpoint_id"]
    rpc(root, {"command": "publish", "checkpoint_id": cid})
    bundle_path = root / "artifacts" / cid
    rpc(
        root,
        {"command": "bundle_export", "path": str(bundle_path), "checkpoints": [cid]},
    )
    bundle = Bundle(bundle_path)
    try:
        report = bundle.inspect()
        if report["hash_mismatches"] or report["checkpoint_ids"] != [cid]:
            raise RuntimeError("Snapshot bundle failed verification")
    finally:
        bundle.close()
    # These files must be staged alongside the bundle. Hashes expose stale source/data.
    files = [root / "notebook.json", root / "parameters.json", root / "requirements.txt"]
    files.append(root / ".python-version")
    files.extend([root / ".gitattributes", root / ".gitignore"])
    files.extend(sorted((root / "cells").glob("*.py")))
    files.extend(sorted((root / "data").glob("*.json")))
    record = {
        "schema": 1,
        "name": name,
        "parent_checkpoint_id": parent,
        "checkpoint_id": cid,
        "bundle": bundle_path.relative_to(root).as_posix(),
        "checkpoint_status": checkpoint["status"],
        "excluded": checkpoint["excluded"],
        "notes": checkpoint["notes"],
        "parameters": json.loads((root / "parameters.json").read_text()),
        "result": values,
        "environment": {"python": platform.python_version(), "platform": platform.platform()},
        "files": {
            path.relative_to(root).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in files
        },
    }
    write_json(root / "experiment.json", record)
    git(root, "add", "--all")
    git(root, "commit", "-m", f"Save {name}: code, data, and kernel checkpoint")
    if git(root, "ls-files", ".kl"):
        raise RuntimeError("Live runtime files must never be committed")
    if git(root, "status", "--porcelain"):
        raise RuntimeError("Experiment commit left uncommitted files")
    return record


def restore(root, record):
    # Git supplies the tracked source. Bundle import supplies history and payloads only.
    for name, digest in record["files"].items():
        if hashlib.sha256((root / name).read_bytes()).hexdigest() != digest:
            raise RuntimeError(f"Tracked file does not match snapshot: {name}")
    ensure_supervisor(root, sys.executable)
    rpc(root, {"command": "bundle_import", "path": str(root / record["bundle"])})
    result = rpc(
        root, {"command": "restore", "checkpoint_id": record["checkpoint_id"], "trust": True}
    )
    if result.get("excluded"):
        raise RuntimeError(f"Restore excluded values: {result}")
    return result


def run_demo(destination):
    destination = Path(destination).resolve()
    if destination.exists():
        raise ValueError("Output directory must not exist; choose a new path")
    if shutil.which("git") is None:
        raise ValueError("Git must be installed")
    destination.mkdir(parents=True)
    baseline = destination / "baseline"
    roots = [baseline]
    try:
        print("Saving baseline code, dataset, and trained kernel", file=sys.stderr, flush=True)
        notebook = Notebook(baseline)
        notebook.init()
        for name in ("parameters.json", "data"):
            source = TEMPLATE / name
            if source.is_dir():
                shutil.copytree(source, baseline / name)
            else:
                shutil.copy2(source, baseline / name)
        parameters_path = baseline / "parameters.json"
        write_json(parameters_path, json.loads(parameters_path.read_text()))
        for cid in ("load", "train"):
            notebook.add("code", (TEMPLATE / f"{cid}.py").read_text(), cid)
        (baseline / "requirements.txt").write_text(
            "# This experiment uses only the Python standard library.\n", encoding="utf-8"
        )
        (baseline / ".python-version").write_text(platform.python_version() + "\n")
        (baseline / ".gitattributes").write_text(
            "# Snapshot hashes require identical bytes on every checkout.\n* -text\n",
            encoding="utf-8",
        )
        git(baseline, "init", "-b", "main")
        # Identity/configuration are local to this generated repository.
        git(baseline, "config", "user.name", "KernelLedger Example")
        git(baseline, "config", "user.email", "example@kernelledger.invalid")
        git(baseline, "config", "commit.gpgsign", "false")
        git(baseline, "config", "core.autocrlf", "false")
        ensure_supervisor(baseline, sys.executable)
        execute(baseline, cell_id="load")
        execute(baseline, cell_id="train")
        base = snapshot(baseline, "baseline")
        git(baseline, "tag", "baseline")
        records = {"baseline": base}
        for name in ("learning-rate", "dataset"):
            print(f"Restoring baseline into {name} worktree", file=sys.stderr, flush=True)
            root = destination / name
            git(baseline, "worktree", "add", "-b", f"experiment/{name}", str(root), "baseline")
            roots.append(root)
            if (root / ".kl").exists():
                raise RuntimeError("New worktree unexpectedly copied live kernel state")
            restore(root, base)
            execute(
                root,
                source="assert training_steps == 10 and model_alias is model\n"
                "assert config['learning_rate'] == 0.05",
            )
            parameters = dict(base["parameters"])
            if name == "learning-rate":
                parameters["learning_rate"] = 0.01
            else:
                parameters["dataset"] = "data/shifted.json"
            write_json(root / "parameters.json", parameters)
            # Preserve the restored model and resume training. Loading the cell would reset it.
            execute(root, source="config = json.loads(pathlib.Path('parameters.json').read_text())")
            if name == "dataset":
                execute(
                    root, source="samples = json.loads(pathlib.Path(config['dataset']).read_text())"
                )
            execute(root, cell_id="train")
            execute(root, source="assert training_steps == 20 and model_alias is model")
            records[name] = snapshot(root, name, base["checkpoint_id"])
        execute(
            baseline,
            source="assert training_steps == 10 and config['learning_rate'] == 0.05\n"
            "assert config['dataset'] == 'data/baseline.json' and samples[0] == [0, 1]",
        )
        for name in ("learning-rate", "dataset"):
            root = destination / name
            record = records[name]
            execute(root, source=f"assert config == {record['parameters']!r}")
        # A clean clone cannot see any donor .kl directory or worktree runtime.
        print("Verifying saved state in a fresh clone", file=sys.stderr, flush=True)
        for root in roots:
            stop(root)
        clone = destination / "restored-clone"
        git(
            baseline,
            "-c",
            "core.autocrlf=true",
            "clone",
            "--no-local",
            "--branch",
            "experiment/learning-rate",
            str(baseline),
            str(clone),
        )
        roots.append(clone)
        if (clone / ".kl").exists():
            raise RuntimeError("Clone unexpectedly included live state")
        saved = json.loads((clone / "experiment.json").read_text())
        restore(clone, saved)
        execute(
            clone,
            source=f"assert model == {saved['result']['model']!r}\n"
            f"assert metrics == {saved['result']['metrics']!r}",
        )
        execute(
            clone,
            source="assert training_steps == 20 and model_alias is model\n"
            "assert config['learning_rate'] == 0.01\n"
            "train(1)\nassert training_steps == 21",
        )
        result = {
            "output": str(destination),
            "worktrees": {name: str(destination / name) for name in records},
            "checkpoints": {name: record["checkpoint_id"] for name, record in records.items()},
            "verified": [
                "kernel isolation",
                "complete checkpoints",
                "fresh clone restore",
                "resumed training",
            ],
            "restored_clone": str(clone),
        }
        write_json(destination / "summary.json", result)
        return result
    finally:
        for root in reversed(roots):
            stop(root)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="new destination directory")
    args = parser.parse_args()
    print(json.dumps(run_demo(args.output), indent=2))


if __name__ == "__main__":
    main()
