# Performance campaign outline (live UX workstream F)

Do not invent numbers. Record machine, backend (Windows HW / Linux HW / Linux llvmpipe), binary hash, and fixture hashes.

## Fixtures

1. Empty notebook cold start
2. Forecast fixture (5 cells) first `print(1)` after attach
3. 1,000-cell notebook open (virtualized; no 1,000 live editors)
4. 100 MiB stream inspect via bounded paging
5. Output flood + Interrupt remains responsive
6. History list with ≥10k recorded executions (pagination bound)

## Procedure

```sh
# Linux preview (software render — label results as such)
python artifacts/frontend-live-ux-2026-11-07/launch_linux.py
# Time window map / first paint with compositor tools on the qualified OS.
```

Windows: reuse `artifacts/frontend-v6/native/launch.py` patterns and timing harness from F-PERF notes in `FRONTEND_PLAN.md`.

## Exit

A filled table in this directory (`perf-results-<host>.json`) or an explicit deferral with plan id. Mixing llvmpipe timings with Windows HW timings is a documentation failure.
