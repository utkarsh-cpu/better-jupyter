# Implementation notes

The package is organized around the version 3 source, ledger, worker, and sharing
boundaries. `worker.py` can run by its absolute path in an interpreter that does not
have KernelLedger installed. It does not import package modules or receive a runtime
state path. Optional codecs are discovered in that interpreter.

Execution source is canonical UTF-8, stored by hash before the request is sent. Each
execution gets a distinct compilation filename and a permanent linecache entry.
The checkpoint AST index captures definitions, strips function decorators and defaults
before loading, and uses a factory for closure values. Dataclass definitions are
registered by class identity, so redefining a class does not change earlier instances'
source. Definitions restored from a checkpoint retain their provenance for later saves.

The graph walker publishes JSON values and hashed bytes, never pickle or reduce hooks.
Unsupported descendants exclude every reachable root. Alias/cycle handling applies to
mutable objects; construction cycles through immutable or hashed positions are refused
per binding. Restored functions use the same global dictionary as the worker. Instance
dictionaries are filled directly, without invoking constructors or attribute setters.
Imports and class statements can execute code; foreign histories require explicit trust.

The supervisor is an asyncio server with authenticated loopback connections. Only it
writes an active ledger. Commands needing mutations while it is stopped acquire the
same OS lock. Event commits happen every 50 ms, with durable lifecycle commits before
completion. Stream chunks coalesce up to 64 KiB, and one truncation notice records all
dropped stream bytes. Direct process pipe output has approximate ordering.

Binary frames are hashed while being streamed to a temporary file, fsynced, and moved
into the immutable blob store. Checkpoint publication is the SQLite transaction commit.
Tests terminate the supervisor after payload moves, after manifest moves, before commit,
and after commit. Only the last case leaves a checkpoint row. Unreferenced payloads
remain until explicit garbage collection. All recorded executions and checkpoints are
retained; pins name selected checkpoints and protect them alongside normal references.
There is no history deletion command in schema 1.

Soft interruption is delivered by the protocol thread. Worker `time.sleep` uses a
cooperative event so a soft interrupt wakes it on Windows as well. C extensions can
remain uninterruptible until they return; `--kill` terminates the process tree. Namespace
state is lost when a worker dies, and the next run starts a new session.

`bundle import` imports the verified history and blobs into the destination ledger.
The source is available under `notebook/` inside the bundle; importing runtime history
does not overwrite the destination's tracked source. A descriptor names payload hashes;
`fetch` verifies what is available locally and reports missing hashes. No remote backend
or dependency installer is included.

Benchmark timing depends on hardware, Python, and codecs. The checked-in results are
observations on the development Windows machine, not performance guarantees. No
incremental format, magics, widgets, or process snapshots are included.

The desktop development app and runtime v4 additions are described in
[desktop documentation](desktop/README.md) and the
[protocol contract](desktop-protocol.md). The [coverage report](desktop/coverage.md)
records remaining work against both current plans, including frontend release gates.
