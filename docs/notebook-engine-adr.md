# ADR: retain the custom worker engine for notebook interactivity

Date: 2026-10-26. Status: implemented development decision; release gates open.
Scope: N0 of the [notebook interactivity draft](../plans/notebook-interactivity-draft.md).

## Decision

Retain the standalone custom CPython engine as the only production worker path.
Use an optional disposable IPython `InteractiveShell` spike for comparison.
Do not add IPython/ipykernel to normal startup, open a second history database,
change a running engine or fall back silently between engines.

`CustomEngine` owns compilation, persistent future flags, final-expression
evaluation, the managed asyncio loop, timing/profiling/capture and transient
result caching. `Worker` retains transport, input, source indexing, passive
reads, display events, checkpoint encoding and restore. Keeping this support in
one standalone resource preserves extraction/package identity in both owners.
The session environment records `custom/1` and transformer
`kernelledger.notebook/2`; schemas remain additive. Future flags and runtime
objects have explicit checkpoint limits rather than implicit process recovery.

## Evidence and tradeoff

`scripts/notebook_engine_spike.py` starts alternating disposable custom/IPython
processes under the same interpreter. It disables IPython history and result
caching, indexes original sources, and checks aliases, functions/classes, final
expressions, timing syntax, future annotations, top-level await, errors and
checkpoint exclusions. Raw trials and the exact worker hash are in
[engine-spike.json](../artifacts/notebook-interactivity/engine-spike.json).

The Windows CPython 3.12.10 / IPython 9.17.1 trials measured
0.077–0.122 seconds custom engine initialization and 0.199–0.396 seconds IPython
initialization (medians 0.083 and 0.210 seconds). Post-fixture RSS was roughly
34–36 MB versus 43–46 MB; private bytes roughly 19–21 MB versus 29–31 MB
(decimal MB). These are local development measurements,
not frozen-baseline release benchmarks; the linked rerun is authoritative for
exact samples, platform, versions and memory definitions. Do not sum RSS as unique
memory or interpret three samples as a tail-latency result.

Both engines ran the spike syntax cases, with the intended ValueError in the
error fixture. Under the current source index, custom reconstruction reported
one exclusion (`__future__.annotations` feature object); IPython reported nine,
including injected runtime helpers and functions/classes whose transformed
filenames were outside the index. This does not establish that IPython cannot
be integrated. It shows that adopting it requires a separate provenance bridge,
display/input publisher integration, exclusion policy and lifecycle qualification.
The spike does not qualify IPython interrupts, stdin, MIME publication or full
checkpoint reconstruction, so there is no advertised IPython adapter capability.

IPython would reduce the maintenance burden of a broad interactive syntax and
completion ecosystem. It also adds optional dependencies, startup/memory cost,
injected bindings and a compatibility/version/license inventory. The optional
extra is bounded to IPython 8.18–9.x, letting package resolution choose a version
supported by project Python 3.11–3.13. The dependency is BSD licensed; full
dependency license texts and redistributable package review remain distribution
gates. `ipykernel`, Jedi/LSP, debugpy and a general widget manager are not adopted
by the worker. The optional Jupyter transport uses jupyter-client/pyzmq outside it.

## Consequences

The custom path must maintain its explicit, tested syntax subset; it must not
claim IPython equivalence. Completion uses source names/keywords plus passive
idle namespace metadata. Debugging uses a bounded local `bdb` subset; DAP/debugpy
has a separate [compatibility evaluation](notebook-debugger-adr.md). Widgets use versioned native JSON models
and signed integer values; general ipywidgets/binary comms and active JavaScript
renderers remain unsupported until separately qualified.

Passive reads execute no user hooks and never enter the ledger/assistant context.
Async tasks cannot survive a cell boundary; resistant tasks require restart.
Native/Python owners share worker fixtures but retain their own durable admission
and existing migration gates. No successful spike, benchmark or test closes the
[N0–N10 release gates](notebook-interactivity-gates.json) automatically.
