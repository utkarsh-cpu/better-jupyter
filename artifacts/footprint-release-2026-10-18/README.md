# Release idle footprint

Measured 2026-10-18 after the footprint pass, on the same Windows counters as
[`../marimo-memory-2026-10-18/README.md`](../marimo-memory-2026-10-18/README.md).

`measure.py` creates an owned two-cell notebook, runs `print(1 + 2)`, opens
`desktop/target/release/kernelledger-desktop.exe`, and samples three times.
`sample.ps1` records `WorkingSetPrivate`, `WorkingSet`, and `PrivateBytes` for
the UI process and the owned supervisor, plus their descendants. It then closes
only that UI and supervisor.

Median of the three samples: 4 processes, 0 console hosts, 71.02 MiB private
resident, 150.95 MiB summed working set, 213.25 MiB private commit. The desktop
process was 25.12 MiB private resident and 161.68 MiB private commit. The worker
command line is base `Python312\python.exe`, parented by the supervisor.

The desktop commit remains well above its resident set. That reservation is in
the pinned GPUI renderer, which this pass does not reconfigure.
