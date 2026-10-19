# KernelLedger / marimo idle memory measurement

Measured 2026-10-18 on this Windows machine, using Python 3.12.10.

## Setup

- Installed marimo 0.25.1 into `artifacts/marimo-memory-2026-10-18/venv` using `python -m venv` and `python -m pip install marimo`. See installed-packages.txt.
- Compared a fresh notebook in each application: a Markdown heading/paragraph and `print(1 + 2)`. Both had executed successfully and displayed 3.
- KernelLedger used the existing native debug executable with isolated preferences and a fresh owned supervisor/worker. The user's existing runtime was not executed or restarted.
- marimo used `edit --headless --no-token --host 127.0.0.1 --skip-update-check --no-sandbox`, with an isolated XDG_CONFIG_HOME.
- marimo's editor ran in one incognito tab in T3 Code's Chromium preview, 1280 x 800 CSS pixels. This is not a standalone Chrome/Edge benchmark.
- The sole new T3 renderer PID 62068 was created when the benchmark tab opened; no other preview tabs existed. It survived navigation and disappeared when that tab closed. Only that renderer was counted; T3's main UI, server, GPU and shared browser infrastructure were excluded.
- Included each application's Python process descendants and console hosts; excluded measurement scripts.
- No pandas, datasets or models loaded in the benchmark notebooks.

## Results (MiB, median of three idle snapshots about ten seconds apart)

| Processes | Private resident RAM | Summed working sets | Private commit |
| --- | ---: | ---: | ---: |
| KernelLedger UI + backend + worker + helper | 74.30 | 191.67 | 216.94 |
| marimo Python processes | 101.06 | 140.00 | 106.33 |
| marimo tab renderer | 103.21 | 213.23 | 116.86 |
| marimo Python + tab (median of sample totals) | 204.28 | 353.24 | 223.32 |

Private resident totals ranged 74.26–74.30 MiB for KernelLedger and 197.25–207.30 MiB for marimo. Values above are rounded; totals were calculated before rounding. KernelLedger used approximately 63.6% less private resident RAM in this particular idle test. Committed private memory was similar. This is one launch per application and three snapshots, not three independent launches or a peak-workload test.

Counters: Windows `Win32_PerfFormattedData_PerfProc_Process` fields `WorkingSetPrivate`, `WorkingSet`, `PrivateBytes`. WorkingSet totals count shared pages more than once; private commit is allocated backing, not resident RAM. GPU memory excluded. Shared browser overhead excluded from marimo, but tab renderer includes the Chromium/T3 preview environment's per-renderer overhead. Standalone browsers and other workloads may differ.

## Evidence and lifecycle

- samples.json: raw process IDs, roles, command lines and counter bytes.
- summary.json: aggregate medians.
- measure.ps1: measurement script (PID-specific to this run).
- notebook.py and kernelledger-notebook/: tested notebooks.
- kernelledger.png: native app screenshot after sampling.
- launch.py, stop.py and session.json: owned process lifecycle.
- marimo screenshot: C:/Users/UTKARSH KAUSHIK/.t3/userdata/browser-artifacts/browser-screenshot-127-0-0-1-mvct7nzd-db2cff71.png

The benchmark tab, UI and runtimes were closed after measurement. The isolated installation and evidence were retained. The user's supervisor PID 10820 and worker PID 3560 remained alive.

## Release build after the footprint pass

Measured the same way on 2026-10-18, after dropping console hosts, launching a Windows venv worker without its redirector stub, and paging ledger reads. Evidence is in [`../footprint-release-2026-10-18`](../footprint-release-2026-10-18).

The owned notebook was again a Markdown cell plus `print(1 + 2)`. The release `kernelledger-desktop.exe` was used. The worker interpreter was this repo's `.venv`, rewritten to base `Python312\python.exe` with `__PYVENV_LAUNCHER__` set. Three idle snapshots were taken about ten seconds apart. The measurement script and its PowerShell sampler were excluded. GPU memory was excluded. Counters are the same three Windows fields.

| | Processes | Private resident RAM | Summed working sets | Private commit |
| --- | ---: | ---: | ---: | ---: |
| Debug idle, before this pass | UI + backend + worker + helper + redirector + 3 console hosts | 74.30 | 191.67 | 216.94 |
| Release idle, after this pass | 4 (UI, helper, supervisor, worker) | 71.02 | 150.95 | 213.25 |

No `conhost.exe` was attached, and the worker's parent was the supervisor rather than `.venv\Scripts\python.exe`. The release desktop process itself was about 25.12 MiB private resident and 161.68 MiB private commit. That commit-above-resident gap matches the debug desktop and stays inside pinned GPUI 0.3.7. `gpui_kit::application()` does not expose a device-memory or font-cache limit, so this pass leaves the renderer unchanged.

