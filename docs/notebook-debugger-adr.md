# ADR: retain the native debugger subset after the DAP evaluation

Date: 2026-10-26. Status: development decision; advanced features remain opt-in.

The desktop retains its local `bdb` debugger and passive variable previews.
Debugpy is an optional disposable spike dependency, never a worker import or
production listener. The product advertises no DAP or external debugger support.

`scripts/notebook_debugger_spike.py` exercises real DAP initialize/attach,
breakpoint configuration, stack/scopes/variables, source retrieval, next and
continue against two disposable processes. One compiles a cell with a virtual
immutable execution filename and populates `linecache`; the other materializes
the same exact source at that filename. Neither attaches to a notebook owner or
writes a ledger. Raw Windows results are in
[debugger-spike.json](../artifacts/notebook-interactivity/debugger-spike.json).

With Windows CPython 3.12.10 and debugpy 1.8.22, both fixtures pause at line 10,
step to line 11 and finish with the expected value. Debugpy returns the exact
virtual source through `sourceReference`, but initially rejects that virtual
file's breakpoint because the file does not exist. Materializing the identical
source makes its breakpoint verified. This is evidence that an immutable source
bridge is feasible, with more integration needed for virtual breakpoint setup.
It does not establish that all virtual breakpoint configurations fail.

In both trials, requesting Locals variables calls the fixture's user-defined
`__repr__` once. That behavior does not meet the product's passive inspection
contract. Adopting DAP therefore needs an explicit policy for variable reads,
hover evaluation and any arbitrary evaluation request, plus durable admission
for side effects. The current debugger keeps using the same passive preview
implementation as the namespace inspector.

The spike uses the [public debugpy API](https://github.com/microsoft/debugpy/wiki/API-Reference)
and [DAP requests](https://microsoft.github.io/debug-adapter-protocol/specification).
The documented `listen` default starts a separate adapter process; subprocess
instrumentation is explicitly disabled in the fixture. The fixture's temporary
loopback port has no notebook token and is unauthenticated. It is unsuitable as
a shared notebook endpoint. Production adoption would need authenticated owner
routing, exclusive controller ownership, bounded frames and replies, paused
disconnect/crash recovery, interrupt priority, process cleanup, checkpoint
exclusion, source mapping and supported-version/platform qualification.

Reproduce in a disposable environment with `.[debugger-spike]` installed:

```console
python scripts/notebook_debugger_spike.py --output debugger-spike.json
```

This evaluation makes no performance or memory claim and does not qualify DAP
integration. Advanced CI runs it separately from the core dependency lanes.
