"""Read-only experimental source/state association checks. No Git hooks installed."""

import hashlib
import subprocess
from pathlib import Path
from typing import Any

from .artifacts import ArtifactStore
from .experiments import inspect_experiment


def experiment_status(store: ArtifactStore, digest: str, worktree: Path) -> dict[str, Any]:
    manifest = inspect_experiment(store, digest)
    worktree = Path(worktree).resolve()
    changes: list[dict[str, Any]] = []
    for name, reference in manifest["files"].items():
        path = worktree / name
        expected = store.manifest(reference["blob"])["sha256"]
        reason = None
        actual = None
        if not path.exists():
            reason = "missing"
        elif path.is_symlink() or not path.resolve().is_relative_to(worktree):
            reason = "unsafe path"
        elif not path.is_file():
            reason = "not a file"
        else:
            hasher = hashlib.sha256()
            with path.open("rb") as stream:
                while block := stream.read(1024**2):
                    hasher.update(block)
            actual = hasher.hexdigest()
            if actual != expected:
                reason = "modified"
        if reason:
            changes.append({"path": name, "reason": reason, "expected": expected, "actual": actual})
    git: dict[str, Any] = {"available": False}
    try:
        result = subprocess.run(
            ["git", "-C", str(worktree), "rev-parse", "--show-object-format", "HEAD"],
            capture_output=True,
            timeout=10,
        )
        if result.returncode == 0:
            lines = result.stdout.decode().splitlines()
            git = {"available": True, "object_format": lines[0], "commit": lines[1]}
    except (OSError, subprocess.TimeoutExpired, UnicodeDecodeError):
        pass
    return {
        "snapshot": digest,
        "source": "stale" if changes else "matching-stored-bytes",
        "changes": changes,
        "source_coherence": manifest["source_coherence"],
        "coherent_commit_eligible": False,
        "git": git,
    }
