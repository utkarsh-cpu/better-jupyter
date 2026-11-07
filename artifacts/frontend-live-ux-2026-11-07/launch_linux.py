#!/usr/bin/env python3
"""Launch an isolated KernelLedger desktop fixture on Linux (live UX Phase 0).

Uses XDG_STATE_HOME for the desktop profile and KL_RUNTIME_PYTHON for the
worker. Does not touch the caller's notebooks or preferences.
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from kernelledger.cli import ensure_supervisor, rpc
from kernelledger.notebook import Notebook

repo = Path(__file__).resolve().parents[2]
out = Path(__file__).resolve().parent
exe = repo / "desktop/target/debug/kernelledger-desktop"
if not exe.is_file():
    raise SystemExit(
        f"missing {exe}; build with: (cd desktop && cargo build -p kernelledger-desktop)"
    )

directory = Path(tempfile.mkdtemp(prefix="kl-live-ux-"))
project = directory / "Forecast analysis - October experiment"
nb = Notebook(project)
nb.init()
nb.add(
    "markdown",
    "# Forecast analysis\nCompare baseline results and save a recoverable checkpoint.",
    "intro",
)
nb.add(
    "code",
    "observations = [12, 18, 15, 21]\n"
    "mean = sum(observations) / len(observations)\n"
    "print(f'Mean: {mean:.1f}')",
    "baseline",
)
nb.add(
    "code",
    "print('Preparing forecast...')\nforecast = mean / 0",
    "forecast",
)
nb.add(
    "markdown",
    "## Next experiment\nCorrect the denominator, rerun the forecast, then compare its recorded source.",
    "notes",
)
nb.add(
    "code",
    "for day, value in enumerate(observations, 1):\n    print(f'Day {day}: {value}')",
    "report",
)

state_home = directory / "xdg-state"
profile = state_home / "kernelledger/desktop"
profile.mkdir(parents=True)
(profile / "appearance.txt").write_text("light", encoding="utf-8")

python = sys.executable
ensure_supervisor(project, python)
for cell in ["baseline", "forecast", "report"]:
    try:
        rpc(project, {"command": "run", "cell_id": cell})
    except Exception:
        # forecast is expected to fail; other cells should succeed.
        if cell != "forecast":
            raise
rpc(project, {"command": "checkpoint"})

env = {
    **os.environ,
    "XDG_STATE_HOME": str(state_home),
    "XDG_CONFIG_HOME": str(directory / "xdg-config"),
    "KL_RUNTIME_PYTHON": python,
    "DISPLAY": os.environ.get("DISPLAY", ":1"),
}
# Prefer lavapipe when no discrete GPU ICD is usable.
icd = Path("/usr/share/vulkan/icd.d/lvp_icd.json")
if icd.is_file() and "VK_ICD_FILENAMES" not in env:
    env["VK_ICD_FILENAMES"] = str(icd)
    env.setdefault("LIBGL_ALWAYS_SOFTWARE", "1")

stdout_log = (out / "app.stdout.log").open("wb")
stderr_log = (out / "app.stderr.log").open("wb")
app = subprocess.Popen(
    [str(exe), str(project)],
    cwd=repo,
    env=env,
    stdout=stdout_log,
    stderr=stderr_log,
)

# Wait until a KernelLedger window appears or the process dies.
window_id = None
xdotool = shutil.which("xdotool")
for _ in range(200):
    if app.poll() is not None:
        stderr_log.flush()
        raise RuntimeError(
            f"app exited {app.returncode}; see {out / 'app.stderr.log'}"
        )
    if xdotool:
        found = subprocess.run(
            [xdotool, "search", "--pid", str(app.pid), "--name", "KernelLedger"],
            check=False,
            capture_output=True,
            text=True,
        )
        ids = [line for line in found.stdout.splitlines() if line.strip()]
        if ids:
            window_id = ids[0]
            subprocess.run(
                [xdotool, "windowactivate", "--sync", window_id],
                check=False,
            )
            break
    time.sleep(0.1)

state = {
    "pid": app.pid,
    "window_id": window_id,
    "project": str(project),
    "fixture_root": str(directory),
    "xdg_state_home": str(state_home),
    "display": env.get("DISPLAY"),
    "vk_icd": env.get("VK_ICD_FILENAMES"),
    "python": python,
    "exe": str(exe),
}
(out / "session.json").write_text(json.dumps(state, indent=2), encoding="utf-8")
print(json.dumps(state, indent=2))
