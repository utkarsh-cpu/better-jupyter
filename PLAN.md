# KernelLedger plan

| | |
| --- | --- |
| Plan version | 5 |
| Date | 2026-10-08 |
| Status | Current. CPU contracts retained; opt-in portable training integration is experimental. Native process capture is unqualified and unavailable here. |
| Supersedes | [Version 4](plans/v4.md) |
| Track | Runtime. The desktop interface is the separate [frontend plan](FRONTEND_PLAN.md), which depends on this one. |
| Versioning | [Plan versioning rules](plans/README.md) |

Version 5 is a local notebook runtime with opt-in experimental training checkpoints. A notebook is a directory of source files. A supervisor process is the only writer of execution history. A worker process runs Python in the project's own interpreter. A checkpoint restores a supported object graph into a new worker.

Version 5 retains the version 4 CPU contracts and adds experimental portable training registration, capture and fresh worker reconstruction (section 24). Section 23 lists the changes. Earlier **(v4)** planning markers and qualification gates retain their meaning; native **(v5)** capture remains planned and unavailable on the local Windows/WSL driver stack.

## Version history

| Version | Date | Status | Product shape |
| --- | --- | --- | --- |
| 1 | Archived 2026-10-03 | Superseded | JupyterLab host, Jupytext source, protocol recording, dill checkpoints, optional CRIU resume. |
| 2 | Archived 2026-10-03 | Superseded | Owned source format, supervisor, worker, typed checkpoints, CLI. |
| 3 | Archived 2026-10-04 | Superseded | Version 2 shape with corrected checkpoint encoding and restore, project interpreter for the worker, OS file lock, rich display, and a checkpoint spike before the supervisor. |
| 4 | 2026-10-04 | Superseded 2026-10-08 | Version 3 runtime plus source service, ledger schema 2 operations and change feed, batches, input ownership, desktop protocol 1, and runtime registry. |
| 5 | 2026-10-08 | Current | Adds opt-in training checkpoint integration; native capture remains unavailable. |

Change a decision in this document by adding a new plan version. Keep the superseded document in `plans/` and point this file at it. [plans/README.md](plans/README.md) defines version numbers, statuses, requirement identifiers, and how this plan relates to the frontend plan.


## 1. Guarantees

| Operation | Guarantee |
| --- | --- |
| Open source | Recover code, Markdown, raw cells, cell ids, order, intentional metadata, and attachments from the tracked files alone. |
| Inspect a run | Show the recorded source, events, and folded outputs. Inspection executes nothing. |
| Replay a run | Execute recorded source, in recorded order, in a new worker. Report environment differences. Identical results are not part of the guarantee. |
| Restore a checkpoint | Load the published object graph into a new worker. Names the checkpoint could not represent stay unset, and the report lists them. |
| Continue | Restore a checkpoint, then run new work in a new session. Earlier sessions stay immutable. |
| Edit source **(v4)** | A source mutation applies only to the revision the client saw. A conflicting change is refused, never merged silently. A crash mid-mutation leaves either the old or the new source after recovery. |
| Learn an outcome **(v4)** | An accepted side-effecting operation has a durable id. A client that loses the reply can look the outcome up and never needs to resubmit to find out. |

A recorded history is not the live worker namespace. A checkpoint is not a snapshot of the process. Replay and continue repeat whatever side effects the source performs. Neither operation restores a socket, a remote service, or an external file that the checkpoint did not itself store.

Four checkpoint outcomes exist, and the product uses these words only for these meanings:

| Outcome | Meaning |
| --- | --- |
| Complete | Every user binding was stored and the worker was idle. |
| Partial | The stored graph is consistent. One or more bindings were left out, and each omitted binding is named. |
| Refused | Nothing was published. The worker was busy, a non-allowlisted thread was running, or the total payload limit was exceeded. |
| Failed | A crash or I/O error occurred before publish. No checkpoint id exists for that attempt. |

## 2. Out of scope

Version 4 ends at the guarantees above.

- JupyterLab, ipykernel, Jupytext, nbdime, nbclient, and papermill are not runtime dependencies. `.ipynb` import and export are converters.
- IPython magics, shell escapes, widgets, top-level `await`, and kernel subshells are not part of the worker.
- Pickle, dill, and cloudpickle are not checkpoint formats.
- Process checkpoint tools, including CRIU and DMTCP, are not a delivery phase.
- Live multi-user editing is not a delivery phase. Collaboration is Git on the source files plus an explicit bundle or published checkpoint descriptor. Several local clients of one user (CLI, desktop windows, an external text editor) coordinate through source revisions (section 5.1). That is conflict detection, not collaborative merging.
- The product does not install packages, fetch datasets, or reproduce a conda, uv, or CUDA environment. It records the interpreter it actually ran and reports drift.
- This plan does not specify a graphical editor. The [frontend plan](FRONTEND_PLAN.md) does, as a client of the contracts here. No runtime phase depends on the frontend, and the frontend may not define runtime behavior, persisted formats, or protocol messages.

## 3. System shape

```text
CLI ------------(legacy protocol)--+
                                   +-->  Supervisor  --->  Worker (project interpreter)  --->  user code
Desktop client --(desktop protocol)+         |
      |                                      +--> ledger.sqlite  (sole writer)
      |                                      +--> blobs/<sha256>
      +--(stdio)--> Desktop helper:  source service (writes tracked source),
                                     read-only ledger and blobs, never a worker
```

**(v4)** Three components join the version 3 shape. The *source service* (section 5.1) is a library inside the KernelLedger package that every source-writing process uses: the CLI, the supervisor, and the desktop helper. The *desktop protocol* (section 8.1) is a second, separately versioned listener on the supervisor. The *desktop helper* (section 8.7) is a process that writes source through the source service and answers history, checkpoint, and bundle queries from a read-only ledger, without a supervisor or worker.

The CLI talks to the supervisor. The supervisor owns the notebook directory lock, the ledger, and the blobs. It spawns the worker, sends one execution at a time, and records events. The worker never receives a path inside `.kl/`.

The supervisor stays alive between CLI commands so the worker namespace survives. The first command that needs it starts it as a detached process: `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW` on Windows, `start_new_session=True` on Linux. Its own output goes to `.kl/supervisor.log`. `kl stop` shuts both processes down. When the supervisor exits, the live namespace is gone. Checkpoints already published remain on disk.

The supervisor listens on `127.0.0.1` on an ephemeral port. The port lives in `.kl/supervisor.json` and a random authentication token lives in `.kl/token`. Every CLI command presents the token.

**Lock.** The supervisor opens `.kl/lock` and holds an exclusive OS lock on it for its whole lifetime: `msvcrt.locking` on Windows, `fcntl.flock` on Linux. The operating system releases the lock when the process dies. A second supervisor that cannot take the lock exits with an error that names the running pid. A pid is never used to decide whether a lock is stale.

**Takeover.** A supervisor that takes the lock and finds sessions still marked `running` marks their in-flight executions `incomplete` and closes those sessions as `crashed`. **(v4)** It also marks unfinished operations `unknown` or their recorded terminal state (section 8.3), and marks still-`queued` executions of an interrupted batch `canceled`.

**Lock ordering (v4).** Two locks exist. The *lifetime lock* `.kl/lock` is held by the supervisor for its whole life, or by a lock-taking CLI command while no supervisor runs. The *source lock* `.kl/source.lock` is held only for the duration of one source read-validate-publish step, typically milliseconds. A process may take the source lock while holding the lifetime lock. A process holding the source lock never waits for the lifetime lock, never sends a request to the supervisor, and never waits on a worker. Execution acceptance takes the source lock only to read and hash the cell. Those three rules make the order acyclic.

**Registry (v4).** A supervisor writes a registry entry when it starts listening and removes it on clean exit: `%LOCALAPPDATA%\KernelLedger\runtimes\<key>.json` on Windows, `$XDG_STATE_HOME/kernelledger/runtimes/<key>.json` (default `~/.local/state`) on Linux. `<key>` is the SHA-256 of the canonical project path. The entry holds the project path, notebook id, pid, ports, start time, KernelLedger version, and supported protocols, never the token. Entries are hints. A client proves an entry live only by connecting and authenticating with the project's token; an entry that fails is stale and may be deleted by any client. `kl runtimes` lists entries and their verified state.

## 4. Files

```text
analysis/                          # tracked by Git
  notebook.json                    # id, order, metadata, attachment names
  cells/
    <cell-id>.py                   # code
    <cell-id>.md                   # Markdown
    <cell-id>.raw                  # raw cell text
  attachments/
    <cell-id>/<filename>           # bytes referenced by a cell
  checkpoints/
    <checkpoint-id>.json           # published descriptors only, written by kl publish

.kl/                               # local runtime state, gitignored
  config.json                      # local settings, see below
  lock                             # held with an OS lock by the live supervisor
  source.lock                      # (v4) short-lived source mutation lock, section 5.1
  source-journal/<txid>/           # (v4) unfinished multi-file source transactions
  ledger.sqlite
  ledger.sqlite.schema1.bak        # (v4) written by kl migrate before the schema 2 upgrade
  blobs/<sha256>
  tmp/                             # unpublished checkpoint files
  supervisor.json                  # pid, port, and (v4) desktop_port and protocols
  supervisor.log
  token                            # secret used by local CLI and desktop connections
```

Git tracks `notebook.json`, `cells/`, `attachments/`, and `checkpoints/`. Git ignores `.kl/`. Descriptors point at hashes. They are not a second execution log.

`notebook.json` is the manifest because the Python standard library can read and write JSON. The writer uses stable key order, one cell object per line, a trailing newline, and UTF-8. Cell source stays in its own file so a one-cell edit is a one-file diff.

`.kl/config.json` holds machine-local settings. It is never tracked and never bundled. Every key is optional:

| Key | Default | Meaning |
| --- | --- | --- |
| `python` | absent | Path to the worker interpreter. See section 6. |
| `env_allowlist` | `[]` | Environment variable names whose values are recorded on the session. |
| `checkpoint_denylist` | `[]` | Binding names that are always excluded from checkpoints. |
| `thread_allowlist` | `["tqdm._monitor.TMonitor"]` | Threads that do not refuse a checkpoint. Each entry matches a thread `name` or the thread's `module.qualname` class. |
| `output_limit_bytes` | `268435456` | Stream bytes recorded per execution before truncation. See section 9. |
| `siblings` | `[]` | Other working copies `kl doctor` checks for a shared notebook id. |
| `record_secret_input` **(v4)** | `false` | When `false`, replies to `getpass` requests are recorded as redacted. See section 8.5. |

## 5. Document format

`notebook.json` schema 1:

```json
{
  "schema": 1,
  "id": "8f1c0c3e-7b2a-4e1d-9c0a-6d5e4f3a2b10",
  "metadata": {},
  "cells": [
    {"id": "intro", "type": "markdown", "attachments": ["diagram.png"]},
    {"id": "load", "type": "code"}
  ]
}
```

| Field | Rule |
| --- | --- |
| `schema` | Integer. The supervisor refuses a newer schema than it knows. |
| `id` | Notebook UUID, lowercase, assigned at `kl init`. Stable across rename and clone. |
| `metadata` | Intentional notebook metadata. Execution counts, outputs, and timing never go here. |
| `cells` | Order is the order of this array. |
| `cells[].id` | Filename stem. Pattern: one to 64 characters, `[A-Za-z0-9_-]`, first character alphanumeric. Unique in the file. |
| `cells[].type` | `code`, `markdown`, or `raw`. |
| `cells[].attachments` | File names only. No path separators. Bytes live under `attachments/<cell-id>/`. |

Reserved Windows device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1` through `COM9`, `LPT1` through `LPT9`) are rejected as cell ids, compared case-insensitively. Two ids that differ only in case are rejected, because Windows and macOS file systems treat them as one file.

Cell files are UTF-8 without a BOM. The canonical newline is `\n`. The source hash is SHA-256 of the canonical UTF-8 bytes. `kl import` writes canonical files. `kl doctor` reports a file that still contains a BOM or `\r`. Execution sends the canonical text to the worker.

A cell id is minted when a cell is created. Import keeps an `.ipynb` cell id when it already matches the grammar. Otherwise import mints an id and stores the original under `metadata.kernelledger.imported_ids`. Opening a notebook never rewrites ids. Duplicate or missing ids are a doctor error. `kl doctor --repair-ids` is the explicit repair: it mints replacement ids, prints the map, and rewrites the manifest.

Deleting a cell removes it from `notebook.json` and deletes that cell file. Blobs already stored for earlier executions stay. History can still show the source that ran.

Copying a project directory copies the notebook id. `kl doctor` reports two working copies that share an id when it can see a configured sibling, and it always prints the id. `kl retarget` assigns a new notebook id when the user means to make an independent notebook. It does not run as part of open.

Attachments are ordinary files in Git. Markdown source refers to them by file name. The importer writes image and other notebook attachments here as decoded bytes.

### 5.1 Source mutations (v4)

Version 3 CLI commands (`kl add`, `kl rm`, `kl import`, `kl retarget`, `kl doctor --repair-ids`) write tracked files directly without a lock. Version 4 routes every tracked-source write through one source service in `kernelledger.source`. It owns validation, canonical serialization, and hashing. No other component, and no other language, writes tracked source.

**Revisions.** The *manifest revision* is the SHA-256 of the canonical bytes of `notebook.json`. The *cell revision* is the cell's source hash (section 5). Revisions are content hashes, not counters, because an external editor or Git can change files without the service. Equal hashes mean equal content.

**Mutation rule.** Every mutation names the manifest revision it expects and the revision of every cell it reads or replaces. Under the source lock the service re-reads the files, validates them, refuses with `source_conflict` (listing the current revisions) when any expectation fails, writes, and releases the lock. A mutation never merges.

| Mutation | Files touched |
| --- | --- |
| Edit cell text | One cell file. Atomic replace. |
| Add, duplicate (new id), delete, move, convert type, split, merge, paste with attachment remapping | Manifest plus cell and attachment files. Journaled. |
| Set notebook or cell metadata | Manifest. Atomic replace. |
| Import, retarget, repair ids | Manifest plus files. Journaled. |

**Journal.** A multi-file mutation writes `.kl/source-journal/<txid>/` first: the intended files, the prior revisions, the list of files to remove, and finally a `commit` marker written and fsynced last. It then replaces files with `os.replace`, writing the manifest last, deletes obsolete files, and deletes the journal directory. Recovery runs under the source lock before the next mutation and at supervisor start. A journal without `commit` is discarded. A journal with `commit` is rolled forward. Content that a mutation removes is kept in the journal until its directory is deleted.

**Validation states.** Reading source returns `ok`, `conflict_markers` (Git merge markers in the manifest or a cell, reported with the files), `malformed` (with the JSON or schema error), `unsupported_schema` (newer `schema`, with both numbers), or `missing` (manifest or listed cell file absent). Any state other than `ok` refuses mutation and execution of the affected cells. It never prevents reading history, because history stores its own source blobs.

**External editors.** Editors and Git do not take the source lock. The service detects their changes through revisions, so a stale client gets `source_conflict` rather than overwriting. File-system watching is a client concern. It is a hint, never evidence.

## 6. Worker

**Interpreter.** The worker runs in the project's interpreter, not the supervisor's. The supervisor picks it in this order, and records the choice on the session:

1. `--python <path>` on the command that starts the supervisor.
2. `python` in `.kl/config.json`.
3. `.venv/` in the project directory or the nearest ancestor that has one.
4. The `VIRTUAL_ENV` environment variable of the starting command.
5. The supervisor's own interpreter.

**(v4) Launch-aware fallback.** The supervisor records its *distribution*: `installed` when it runs from a user-installed package (pip, pipx, uv tool), `private` when it runs from the interpreter bundled with the desktop application. Rule 4 applies only when the starting command is a terminal CLI invocation, because a desktop application inherits no project shell. Rule 5 applies only to the `installed` distribution. A `private` supervisor never runs user code in its own interpreter: when rules 1–4 find nothing, worker start fails with `interpreter_required` and a list of *candidates* (the Windows `py` launcher list, `python3` and `python` on `PATH`, and `.venv` directories under the project). Candidates are offered, never chosen automatically. A desktop client that receives a selection passes it as rule 1 and writes it to `.kl/config.json` only when the user asks to remember it.

The worker interpreter must be CPython 3.11 or newer. It does not need to match the supervisor's version, because the protocol is JSON. Only restore compares Python versions (section 12).

**Launch.** The worker source is one stdlib-only module file, `worker.py`, shipped inside the KernelLedger package. The supervisor runs it by path with `-I -X utf8`, so nothing has to be installed into the project environment and the user's `PYTHONPATH` and user site do not leak into the worker. The worker then puts the project directory first on `sys.path` and uses it as the working directory, so `import mymodule` finds project files.

On Windows the supervisor places the worker in a Job Object with kill-on-close, created through `ctypes`. On Linux the worker leads its own process group. Killing the worker kills every process it started.

**Execution.** User code runs on the worker main thread, in a dedicated globals dict, with `__name__` set to `"__main__"`.

Each execution compiles under its own filename, `cells/<cell-id>.py@<exec-id>`, or `<unattributed>@<exec-id>` when there is no cell. Before compiling, the worker writes the executed source into `linecache` under that filename and keeps it for the worker's lifetime. Tracebacks, `inspect.getsource`, and checkpoint source capture therefore read the source that ran, never the current cell file.

The worker parses each execution with `ast` and keeps an index of every `def`, `async def`, `class`, and `lambda` in it, by line, column, and qualname. Section 11 uses this index.

If the cell's last statement is an expression, the worker executes the preceding statements and evaluates that expression. The value is emitted as a result when it is not `None`.

The worker replaces `sys.stdout` and `sys.stderr` with writers that emit protocol events. Bytes written straight to the process pipes, bypassing those objects, are recorded by the supervisor as stdout or stderr events with `"capture": "pipe"`. Their position relative to protocol events is approximate.

`input()` and `getpass.getpass()` emit a stdin request and wait for the supervisor's reply. A non-interactive replay fails that execution with an error that says input was required, unless the replay was given a recorded reply to reuse.

**Interrupt.** `kl interrupt` sends a control message, and the worker calls `_thread.interrupt_main()`. `time.sleep` and other interruptible waits wake up. A tight C extension may keep running until it returns to Python. The execution then ends as `interrupted`. `kl interrupt --kill` terminates the worker and its Job Object or process group. The execution ends as `aborted` and the session as `killed`.

**Worker death.** If the worker exits while the supervisor is alive, the in-flight execution ends as `aborted` and the session as `crashed`. The next `kl run` starts a new worker with an empty namespace and a new session, and says so before running.

Names beginning with `_kl_` are reserved for the worker. The checkpoint walk skips them. User bindings named `__builtins__` are skipped and listed as excluded.

The worker does not implement magics or `!` commands. Imported notebook source that contains them is preserved and then fails as Python if it is run.

## 7. Display

The worker installs `display(obj, display_id=None)`, `update_display(obj, display_id)`, and `clear_output(wait=False)` in the user globals.

`display` and a cell result build a MIME bundle from the object:

1. `_repr_mimebundle_()` when present.
2. Otherwise each present method among `_repr_html_`, `_repr_markdown_`, `_repr_svg_`, `_repr_png_`, `_repr_jpeg_`, `_repr_latex_`, and `_repr_json_`.
3. `text/plain` from `repr()`, always.

A repr method that raises is recorded as a stderr event, and the bundle continues without that MIME type. This is what makes pandas, NumPy, and most plotting libraries show rich output without IPython.

**matplotlib.** The worker sets `MPLBACKEND=agg` when the variable is unset. When `matplotlib.pyplot` has been imported, `plt.show()` and the end of every execution emit each open figure as an `image/png` display and then close it.

## 8. Protocol

The worker connects back to the supervisor. The supervisor opens a one-shot listener on `127.0.0.1`, passes its port and a one-time token to the worker in the environment variables `KL_WORKER_PORT` and `KL_WORKER_TOKEN`, and accepts exactly one connection whose first message is `hello` with that token. The worker removes both variables from `os.environ` before any user code runs.

Framing is a 4-byte big-endian length followed by a UTF-8 JSON object. A binary frame is a JSON header with `kind`, `sha256`, and `byte_size`, then exactly that many raw bytes. The supervisor hashes the bytes as they arrive, writes them to the blob store, and rejects a frame whose hash does not match.

Every message carries an integer protocol `schema`. A peer refuses a newer schema.

Supervisor to worker:

| Message | When |
| --- | --- |
| `execute` | `{exec_id, cell_id, source_sha256, source}`. Sent only after the ledger row exists. |
| `stdin_reply` | Response to a worker `stdin_request`. |
| `interrupt` | User asked to stop the in-flight execution. |
| `checkpoint` | Worker is idle. Asks for a graph walk. Carries the denylist, the thread allowlist, and the limits. |
| `restore` | Asks the worker to install a graph the supervisor has already validated. Blobs follow as binary frames. |
| `shutdown` | Exit cleanly. |

Worker to supervisor:

| Message | When |
| --- | --- |
| `hello` | First message. Token, worker Python version, implementation, executable, platform. |
| `ready` | Process is up and the user namespace is empty, or restore finished. |
| `stdout`, `stderr` | Captured text, or a blob hash when the chunk is large. |
| `display` | `{bundle, display_id?}`. Each MIME entry is inline or a blob hash. |
| `display_update` | Replaces the visible item with the same `display_id`. |
| `result` | The cell's final expression, as a MIME bundle. |
| `error` | `{ename, evalue, traceback}`. |
| `clear` | `{wait: bool}`. |
| `stdin_request` | `{prompt, password}`. |
| `status` | `idle` after the execution's events are finished. |
| `checkpoint_offer` | The manifest, sent after every binary frame it references, or a refusal reason. A manifest above the inline limit is itself a binary frame and the offer carries its hash. |
| `restore_result` | Bindings installed, and any that failed during load. |

Inline text and inline JSON payloads are limited to 64 KiB. Larger payloads are binary frames and blobs.

The supervisor writes the `execute` row with status `queued` before it sends `execute`. Status moves to `running` when the worker accepts it, then to `ok`, `error`, `interrupted`, or `aborted`. If the supervisor dies after `queued` or `running`, the next supervisor start sets that row to `incomplete`.

`cell_id` is null for source the user submitted outside a cell. The row still stores the exact source hash. Inspection labels those executions unattributed.

One execution is in flight per supervisor. A second `execute` waits in the supervisor queue, in the order the commands were accepted. The ledger ordinal is that acceptance order.

The worker protocol stays at schema 1 in version 4. Sections 8.1–8.8 describe the client side.

### 8.1 Client protocols (v4)

The *legacy protocol* is the version 3 CLI protocol: one authenticated framed request per connection on the port in `supervisor.json`, protocol schema 1. Version 4 does not change its messages. `kl` keeps using it.

The *desktop protocol* has the API name `kernelledger.desktop` and integer versions starting at 1. It uses a separate loopback listener whose port is `desktop_port` in `supervisor.json`, so a version 3 parser never receives a desktop message and a desktop client never talks to a legacy parser. Framing is the section 8 framing: a 4-byte big-endian length, a UTF-8 JSON object, and binary frames with `sha256` and `byte_size` headers. The server checks the declared length against the limits below before allocating.

**Handshake.** The first frame on every desktop connection is `hello` with `api`, the list of `versions` the client speaks, the token from `.kl/token`, a `client_id` (a UUID stable for one application installation), a display `client_name`, and the connection `role` (`control`, `subscription`, or `blob`). The server answers `welcome` with the chosen version (the highest common one), its capability names, and the runtime identity: a random `instance_id` minted per supervisor start, pid, start time, KernelLedger version, distribution (section 6), ledger schema, notebook id, and canonical project path. With no common version it answers `unsupported_version` with its supported range and closes. A client identifies a runtime by `instance_id`, never by pid or port alone.

**Connections.** A client opens one `control` connection for requests and responses, one `subscription` connection for the change feed and followed executions, and `blob` connections for payloads. A flood on one connection cannot delay another. Interrupt and input replies use the control connection and are handled without waiting for the command queue.

**Capabilities.** Capability names gate optional behavior: `batches`, `input_claims`, `secret_input_redaction`, `operations`, `change_feed`, `blob_ranges`, `gc_preview`, `migrate`. A client hides or labels features whose capability is absent. Adding a capability does not change the protocol version. Changing the meaning of an existing message does.

**Limits.** A JSON frame on any connection is at most 1 MiB. Larger payloads are blob references. A blob chunk is at most 1 MiB. The server answers an oversized declared length with `too_large` and closes. Handshake must finish within 5 seconds. Both sides send `ping` after 10 seconds without traffic and treat 30 seconds of silence as a dead connection. No other request has a protocol-level timeout: execution may run indefinitely, and a client timeout never implies failure (section 8.3).

**Errors.** Every error response is `{code, message, retryable, details}`. Codes: `unauthenticated`, `unsupported_version`, `migration_required`, `capability_missing`, `invalid_request`, `too_large`, `not_found`, `source_conflict`, `source_invalid`, `busy`, `trust_required`, `python_mismatch`, `interpreter_required`, `input_claimed`, `key_reused`, `internal`. `retryable` is true only where repeating the same request cannot repeat a side effect.

**Schema 1 ledgers.** A version 4 supervisor on a schema 1 ledger serves the legacy protocol normally. Its desktop listener completes the handshake with `migration_required` in the identity and accepts only discovery, diagnostics, and the `migrate` operation (section 9.1) until the ledger is migrated.

### 8.2 Desktop request groups (v4)

| Group | Requests | Served by |
| --- | --- | --- |
| Discovery | `hello`, `ping`, `runtime.status` (worker state, session, interpreter, active operation, queue, pending inputs, attached client count) | Supervisor |
| Execution | `run` (cell id or unattributed source, `expected_sha256`), `batch` (section 8.4), `interrupt`, `interrupt_kill`, `restart`, `stop` | Supervisor |
| Operations | `operation.get`, `operation.list`, `operation.cancel` (section 8.3) | Supervisor |
| Feed | `subscribe`, `changes.list`, `execution.follow` (section 8.6) | Supervisor |
| Input | `input.claim`, `input.release`, `input.reply`, `input.cancel` (section 8.5) | Supervisor |
| Checkpoints | `checkpoint`, `restore`, `continue` (with `trust` and `allow_python_mismatch`), `replay` (with `trust`, `reuse_input`), `pin`, `unpin`, `gc` (with `dry_run`), `publish` | Supervisor |
| Interchange | `import_ipynb` (with `keep_outputs`), `bundle_import`, `migrate` | Supervisor |
| Blobs | `blob.read` (`sha256`, optional `offset` and `length`) | Supervisor or helper |
| Source | `source.read`, `source.mutate`, `source.recover`, `source.watch` | Helper |
| Inspection | `sessions.list`, `executions.list`, `execution.get`, `events.list`, `fold.get`, `checkpoints.list`, `checkpoint.get`, `bundle.inspect`, `descriptor.inspect`, `export_ipynb`, `bundle_export`, `doctor` (read-only), `interpreter.candidates`, `interpreter.validate` | Helper |

Lists are paginated by an opaque cursor and a page size of at most 500. `fold.get` returns the section 10 fold computed by the runtime. A client may fold incrementally for display but must reconcile with `fold.get`. `blob.read` streams binary frames. A whole-blob read is verified against its hash before the final frame is marked `verified`. A ranged read returns the declared total size, and the client verifies the hash once it has every range. A missing or corrupt blob is the error `not_found` or `internal` with the hash, never empty content.

### 8.3 Operations (v4)

Every side-effecting supervisor request (each Execution, Checkpoints, and Interchange request except `interrupt` and `interrupt_kill`) carries a client-chosen `client_request_id` UUID. The supervisor inserts an `operations` row with `synchronous=FULL` before it answers `accepted {operation_id}`. A repeated request with the same `(client_id, client_request_id)` and the same request hash returns the existing operation. The same key with a different request hash is `key_reused`. A client never resubmits a timed-out request under a new key. It looks up the old one.

| State | Meaning |
| --- | --- |
| `accepted` | Recorded and queued. |
| `running` | The supervisor started carrying it out. |
| `completed` | The runtime carried it out. The result holds the domain outcome: execution status, checkpoint outcome (`complete`, `partial`, `refused`, `failed`), restore report, replay session, gc report. |
| `canceled` | Canceled before it had any effect. |
| `failed` | The runtime could not carry it out (invalid state, missing blob, trust or version refusal, internal error). The result holds the error. |
| `unknown` | Set at takeover when the previous supervisor died while it was `running` and the ledger cannot prove the outcome. The result lists what is known, such as a checkpoint row or an execution status. |

`operation.cancel` answers `cancel_requested` or `already_final` with the state. For a queued operation, cancellation is final and `canceled`. For a running `run`, it equals a soft interrupt, and the operation ends `completed` with execution status `interrupted`. A running checkpoint, restore, import, or migration is not cancelable once it is running. Operation rows are retained like history, so a lookup after any reconnection succeeds.

`interrupt` and `interrupt_kill` are idempotent controls and are not operations. Each records a session event as in version 3.

### 8.4 Batches (v4)

`batch` names an ordered list of cell ids or `all` (code cells in manifest order), an `on_error` policy of `stop` (default) or `continue`, and optional expected hashes per cell. At acceptance the supervisor reads every listed code cell under the source lock, refuses the whole batch with `source_conflict` when any expected hash differs, stores each source blob, and in one transaction inserts the operation, a `batches` row, and one `queued` execution row per cell. The batch runs the frozen source. Editing or deleting a cell after acceptance does not change the batch.

A batch runs contiguously: no other execution interleaves, and later runs from any client queue behind it. With `stop`, an execution that ends `error`, `interrupted`, or `aborted` marks every remaining queued execution of the batch `canceled`. With `continue`, the batch proceeds after `error` and `interrupted` but still stops after `aborted`, because a dead worker means a new empty session, and the rest of a batch never runs silently in a different namespace. `operation.cancel` on a batch cancels its not-yet-started executions and lets the current one finish. `kl run --all` becomes a batch, and `--keep-going` selects `continue`.

### 8.5 Input requests (v4)

Version 3 forwards a `stdin_request` only to the connection that started the run, and the worker reader waits on that client. Version 4 decouples them. The supervisor records the request, assigns `input_id` (the execution id and event seq), publishes an `input_state` change, and continues reading worker messages. A client never blocks the worker reader.

A legacy interactive `kl run` holds an implicit claim on inputs from its own execution. Otherwise the first `input.claim` wins and every subscriber sees the claimant's `client_name`. Only the claimant may `input.reply` or `input.cancel`. A claimant that disconnects releases its claim, and the request stays pending. There is no input timeout. Interrupting the execution closes the request as `canceled`. `input.cancel` replies to the worker with the version 3 error reply.

**Secret input.** A request with `password: true` comes from `getpass`. Version 3 records the reply text in clear in the `stdin_reply` event, and bundles carry it. In version 4, with `record_secret_input` false (the default), the event records `{"password": true, "redacted": true}` and no text. Replay with `reuse_input` that reaches a redacted reply ends that execution as an error with `secret_input_not_recorded` and stops. Ordinary `input()` replies are still recorded, and clients must say so before the user types. Schema 1 history is append-only and is not rewritten. `kl doctor` counts recorded password replies, and `bundle export` names them in its warning.

### 8.6 Change feed and subscriptions (v4)

The `changes` table (section 9.1) records, in the same transaction as the change itself, every state change a client displays: session start and end, execution status, operation state, input state, checkpoint publication or refusal, pin changes, imports, and migration. Each row has a monotonic `change_seq`. Output events are not copied into `changes`. Their cursor is `(exec_id, seq)`.

`subscribe` without a cursor returns a `snapshot` (runtime status, queue, pending inputs, active operations) tagged with the current `change_seq` as its watermark, then live changes after it. `subscribe` with `after_change_seq` replays committed changes after that cursor, then goes live. `changes` rows are retained like history, so a durable cursor never expires. Each subscriber has a bounded live queue (10,000 messages or 8 MiB). On overflow the server sends `resync_required` with the last delivered `change_seq`, drops the queue, and the client pages through `changes.list`. `execution.follow` with `after_seq` streams an execution's committed events in `seq` order and then live ones.

Version 4 sends only committed events, so live output latency is bounded by the event commit interval (section 9). Delivery is at least once. Clients deduplicate by `change_seq` and `(exec_id, seq)`. Blocking disk work and folding happen outside the worker reader task.

### 8.7 Desktop helper (v4)

The desktop helper is `python -I -m kernelledger.desktop.helper --project <dir>`. It speaks the desktop framing over stdio, with its own `hello` and version, and serves the Source and Inspection groups. It runs the source service for source writes, opens the ledger read-only, and never starts a supervisor or worker, never writes the ledger or blobs, and never executes notebook code. `interpreter.validate` runs the candidate with `-I -c` and a 10-second timeout to read its version and codec availability. Exports write only to the destination the user chose.

Source writes go through the helper whether or not a supervisor runs, so there is one write path. Source mutations are not operations. A client that loses a mutation reply re-reads and compares: an applied mutation shows the intended content, and an unapplied one can be retried with the same expectations.

When the project directory is read-only and no supervisor is running, the helper opens SQLite with `immutable=1` and reports `read_only_snapshot`. When `.kl/` is absent, inspection reports an empty history, and source reading still works.

### 8.8 Contracts and fixtures (v4)

`schemas/desktop-protocol.schema.json` and `schemas/desktop-helper.schema.json` define every message. `tests/fixtures/desktop/` holds request, response, error, change, and fold fixtures generated by the Python implementation. The Rust client tests consume the same files. A state diagram for operations, batches, and input requests lives in `docs/desktop-protocol.md`. Like section 17, the runtime validates by hand.

## 9. Ledger

SQLite is the only live history. `PRAGMA user_version` is the ledger schema. The supervisor uses WAL mode and is the only process that writes the file. Inspection commands open it read-only. Export builds JSONL from a query. Nothing in the running system appends to a JSONL file.

Schema 1 tables:

| Table | Role |
| --- | --- |
| `sessions` | One worker lifetime. Columns include notebook id, parent session, restored checkpoint id, state (`running`, `closed`, `crashed`, `killed`), start and end timestamps, the worker environment object, and the supervisor's KernelLedger and Python versions. |
| `executions` | One row per accepted execution. `exec_id`, `session_id`, monotonic `ordinal`, nullable `cell_id`, `source_sha256`, status, start, end. Unique `(session_id, ordinal)`. |
| `events` | Append-only. Primary key `(exec_id, seq)`. `kind`, timestamp, `payload_json`, nullable `blob_sha256`. |
| `checkpoints` | Published checkpoints only. Id, session, boundary execution id, `complete` or `partial`, manifest blob hash, timestamp. |
| `blobs` | `sha256`, `byte_size`, `media_type`. |
| `pins` | Named pins pointing at checkpoint ids the user chose to keep. |

Timestamps are UTC RFC 3339. Order comes from `ordinal` and `seq`, not from the clock.

The environment object records the worker's Python version, implementation name, executable path, how the interpreter was chosen (section 6), `platform.platform()`, and the worker cwd. Process environment variables are omitted except the names in `env_allowlist`.

**Writes.** Execution rows and status changes commit immediately with `synchronous=FULL`. Events commit in batches, at most every 100 ms and always before `status: idle` is recorded, with `synchronous=NORMAL`. A supervisor crash can lose up to the last batch of events. The execution is then `incomplete`, so no truncated sequence is ever marked `ok`.

**Coalescing.** Consecutive `stdout` chunks, or consecutive `stderr` chunks, that arrive within 50 ms of each other merge into one event until the merged text reaches 64 KiB. A loop that prints a million lines produces thousands of rows, not a million.

**Truncation.** After an execution has recorded `output_limit_bytes` of stream text, further stream output is dropped and one `truncated` event records the dropped byte count. Displays and results are not truncated. They are limited by the checkpoint and blob rules.

**Blobs.** Blob bytes are written to a temporary file, fsynced, moved into `blobs/<sha256>` with `os.replace`, and only then referenced by a committed ledger row. A blob that already exists is not rewritten. Rewriting a blob path in place is not allowed.

`kl gc` deletes blobs that no execution, event, checkpoint, or pin references. It runs inside the supervisor, or takes the lock itself when no supervisor is running, so it cannot race a write. It is explicit. Startup deletes `.kl/tmp` entries left by a previous process. Startup does not run `kl gc`. **(v4)** `kl gc --dry-run` and the desktop `gc` request with `dry_run` list the blobs that would be removed and their total bytes without deleting anything.

### 9.1 Ledger schema 2 (v4)

Schema 2 adds tables and one execution status. It changes no schema 1 column and no existing row.

| Table | Role |
| --- | --- |
| `operations` | `operation_id`, `client_id`, `client_request_id`, `kind`, `request_sha256`, state (section 8.3), `result_json`, accepted, started, and finished timestamps. Unique `(client_id, client_request_id)`. |
| `batches` | `batch_id`, `operation_id`, `session_id`, `on_error`, and the ordered `exec_id` list. |
| `changes` | `change_seq` (integer primary key), `kind`, `ref_json` (the ids involved), timestamp. |
| `input_requests` | `input_id`, `exec_id`, `seq`, `password`, state (`pending`, `answered`, `canceled`), claimant `client_name`, timestamps. |

The execution status `canceled` means accepted and never started. In schema 1, a queued execution that never ran is `aborted`, and those rows stay as they are.

**Migration.** `kl migrate`, or the desktop `migrate` operation, upgrades schema 1 to 2. The CLI command takes the lifetime lock. The desktop operation runs inside the supervisor and requires an idle worker; the live namespace survives it. Either path first writes `.kl/ledger.sqlite.schema1.bak` with the SQLite backup API and verifies it with `PRAGMA quick_check`, then runs the DDL and sets `user_version = 2` in one transaction. A failure leaves schema 1 intact and the backup in place. Opening a notebook, starting a supervisor, and running the helper never migrate. After migration, a version 3 `kl` refuses the ledger as newer (section 17), which is the intended signal to upgrade. To downgrade, restore the backup, which loses history recorded after migration. `kl migrate` says this before it starts.

## 10. Visible outputs

Events are the record. The view folds them in `seq` order:

- `stdout`, `stderr`, `display`, `result`, and `error` append an item.
- `display_update` replaces the latest item with the same `display_id`.
- `clear` drops the current items. `wait: true` defers the drop until the next output item, matching the usual notebook animation behavior.
- `truncated` appends a notice with the dropped byte count.
- Folding never deletes events.

`kl show` prints the folded view, using the richest MIME type the terminal can print, which is normally `text/plain`. `kl show --events` prints the raw sequence. `kl show --mime <type> --out <path>` writes one item's payload to a file. An execution whose source hash differs from the current cell file is labeled with both hashes.

Hiding or collapsing output in a client is view state. No command deletes or folds away recorded events. A client that offers "clear outputs" hides the folded view for the current source revision, and the history stays intact.

## 11. Checkpoints

A checkpoint is taken between executions, while the worker is idle. `checkpoint` during a run is refused. Nothing is published.

Before walking memory, the worker lists Python threads with `threading.enumerate()`. Any thread besides the main thread, the worker's protocol thread, and threads matching `thread_allowlist` refuses the checkpoint with reason `background_threads` and the offending thread names. The refusal is an event on the session. There is no checkpoint id. Native threads that never enter Python, such as BLAS pools, are not visible and do not refuse.

The walk starts at the user globals. It skips `_kl_` names and `__builtins__`. Names in `checkpoint_denylist` go straight to the excluded list with reason `denylisted`. Every other binding is in the offer or in the excluded list.

### 11.1 Encoding

The manifest is a JSON object:

```json
{
  "schema": 1,
  "python": "3.12.6",
  "codecs": {"numpy": "1", "arrow": "1"},
  "bindings": {"df": {"k": "ref", "id": "o3"}, "n": {"k": "int", "v": "42"}},
  "objects": {"o3": {"k": "table", "codec": "arrow", "blob": "9f2c..."}},
  "excluded": [{"name": "conn", "type": "sqlite3.Connection", "path": "conn", "reason": "resource"}],
  "notes": []
}
```

JSON `null`, `true`, and `false` are `None`, `True`, and `False`. Every other value is an object with a kind field `k`, so a stored string is never confused with a stored number, float, or date.

Objects with identity live in `objects` under an id assigned the first time the walk sees them, and every other mention is `{"k": "ref", "id": ...}`. Identity is preserved for every list, tuple, dict, set, frozenset, bytearray, function, class, instance, array, and table. It is not preserved for `int`, `float`, `complex`, `str`, `bytes`, and the other value kinds, which Python does not guarantee anyway.

Supported kinds in schema 1:

| Kind | Stored as |
| --- | --- |
| `int` | `v` as a decimal string |
| `float` | `v` as a JSON number, or `"nan"`, `"inf"`, `"-inf"` |
| `complex` | `re` and `im` under the float rule |
| `str` | `v` inline, or `blob` above 64 KiB |
| `bytes`, `bytearray` | `b64` inline up to 4 KiB, otherwise `blob` |
| `list`, `tuple`, `set`, `frozenset` | `items`, an array of values. For sets, membership is the meaning and order is not. |
| `dict` | `items`, an array of `[key, value]` pairs in insertion order. Keys may be any supported hashable value. |
| `date`, `time`, `datetime` | ISO string `v`, plus `tz`: `null` for naive, `{"offset_s": n}` for fixed offsets, `{"zone": "Asia/Kolkata"}` for `zoneinfo`. `fold` is stored. Naive values stay naive. |
| `timedelta` | `days`, `seconds`, `microseconds` |
| `Decimal` | `v` as the exact string |
| `Fraction` | `num` and `den` as decimal strings |
| `UUID` | canonical string |
| `range` | `start`, `stop`, `step` |
| `path` | class name (`PosixPath`, `WindowsPath`, `PurePosixPath`, `PureWindowsPath`) and `v`. A concrete path class that cannot be instantiated on the restoring platform fails as `unimportable`. |
| `enum` | class reference and member name |
| `module` | module name. Restore imports it. |
| `import` | module and qualname, for any object that importing that module and following that qualname returns as the same object. Restore imports it. Builtins are imports from `builtins`. |
| `function` | notebook-defined function, section 11.2 |
| `class` | notebook-defined class, section 11.2 |
| `instance` | class reference plus `__dict__`, for notebook-defined classes whose instances have a `__dict__` and no `__slots__` |
| `ndarray` | optional NumPy codec, section 11.4 |
| `table` | optional DataFrame and Series codec, section 11.4 |

A notebook-defined function or class is one whose `__module__` is `"__main__"` and whose code, or whose methods' code, compiled under an execution filename from section 6.

Instances store the class reference and the instance `__dict__`. The checkpoint does not call `__getstate__` or `__reduce__`. Restore creates the instance with `cls.__new__(cls)` and fills `__dict__` directly, so `__init__` and `__setattr__` do not run.

An imported module binding is a `module` reference. Assignments made to that module's globals after import are outside the graph. The manifest records `imports_reimported: true` whenever it contains a `module` or `import` node.

### 11.2 Functions and classes

**Source capture.** A function's source is found from its `co_filename` and `co_firstlineno` in the execution index from section 6. A lambda is found the same way when exactly one lambda starts on that line, and otherwise is `source_unavailable`. A class's source is found from `__firstlineno__` on Python 3.13 and newer. On older versions it is found from the execution filename of any function defined in its body, plus its `__qualname__`. A function or class whose source cannot be located is excluded with reason `source_unavailable`.

**Functions** store the source of the definition with its decorators removed, plus `__name__`, `__qualname__`, `__defaults__`, `__kwdefaults__`, the function `__dict__`, and the value of every closure cell, named by `co_freevars`. A cell that is still empty is stored as empty.

A decorated binding is whatever the decorator returned. When that is a wrapper function defined in the notebook, it is stored as a function whose closure holds the original function, so the decorator is not re-run at restore. When it is a wrapper or object from a library that is not importable by qualname, the binding is unsupported.

**Restore of a function.** The loader compiles the stored definition inside a generated factory function whose parameters are the free variable names, calls it with the restored closure values, and takes the returned function. It then sets defaults, keyword defaults, `__dict__`, and `__qualname__`. The function's `__globals__` is the restored user globals, so global names resolve exactly as they did in the original namespace. Two closures that shared one cell in the original get separate cells after restore. The manifest lists that under `notes` as `closure_cells_split`.

**Classes** store the source of the full class statement, including decorators, because decorators such as `@dataclass` generate the class's methods. Restore executes that statement in the restored user globals. Decorators and the class body therefore run again, with any side effects they have. Class attributes changed or added after the class was defined are not restored. Every class restored this way is listed in `notes` as `class_from_source`.

A restored instance refers to the restored class. An instance of a class that failed to restore is excluded with the class.

### 11.3 Loading order

The loader works in this order, so a value is complete before anything needs to hash it or close over it:

1. Validate the manifest and every blob hash.
2. Resolve `module` and `import` nodes.
3. Build value kinds and decode arrays and tables.
4. Create empty mutable containers (lists, dicts, sets, bytearrays) and empty instances.
5. Execute class statements, then build functions, in the order of their defining execution ordinal and line.
6. Build tuples and frozensets once their members exist.
7. Fill instance `__dict__`s, then lists, then dicts and sets.
8. Install bindings by name. Remove any name that step 5 created in the globals and the checkpoint does not bind.

The walk rejects a back reference whose target is a tuple or frozenset still under construction, or a dict key or set member still under construction, with reason `unsupported_cycle`. Such a graph cannot be rebuilt in Python, so it is excluded rather than published wrong.

A failure in step 2 or step 5 puts the affected binding on the restore report, leaves the name unset, and excludes every other binding that reaches the failed object.

### 11.4 Optional codecs

Codecs live in `worker.py` and activate only when their library imports in the worker. A missing codec library at checkpoint time excludes the binding with `missing_codec`. At restore time it leaves the name unset with `missing_codec`. Restore never substitutes a different type under the same name.

**NumPy.** An array is stored as a `.npy` blob written with `numpy.lib.format` and `allow_pickle=False`, which records dtype, byte order, shape, and memory order. Object arrays and structured dtypes containing objects are `unsupported_type`. Restore reads the blob, checks dtype, shape, and hash, and copies it into a new array. An array that is a view of another stored array is stored as an independent copy and listed in `notes` as `view_copied`, because the restored arrays no longer share memory. `numpy.random.Generator` state is stored as its `bit_generator.state` dict.

**Tables.** A pandas `DataFrame` or `Series` is stored as an Arrow IPC stream blob, written through `pyarrow` with the pandas metadata that preserves index, column names, categoricals, timezone-aware datetimes, and nullable dtypes. When pandas imports but `pyarrow` does not, tables are `missing_codec`. A table whose round trip through Arrow does not compare equal under `DataFrame.equals` at checkpoint time is `unsupported_type`. It is never published lossy.

### 11.5 Random state and limits

The standard-library `random` module state is stored under the reserved binding `_kl_random`, hidden from the user namespace and applied on restore. Other generators in user code are ordinary bindings and follow the support rules. `PYTHONHASHSEED` and RNGs inside other libraries are not captured.

Limits:

| Limit | Default | When exceeded |
| --- | --- | --- |
| Objects reachable from one binding | 100,000 | That binding is excluded with reason `limit`. Other bindings stay. |
| New payload bytes in one checkpoint | 2 GiB | The checkpoint is refused. |

If any object reachable from a binding is unsupported, that binding is excluded. A field is never dropped while the parent is reported as saved. A shared child that is unsupported excludes every binding that reaches it. Each excluded entry has the binding name, the Python type name, the path that failed, and a reason:

| Reason | Meaning |
| --- | --- |
| `unsupported_type` | No codec for this object |
| `unsupported_cycle` | A cycle through an immutable or hashed position |
| `missing_codec` | The type is known and its optional codec is not installed |
| `source_unavailable` | Function or class source could not be located |
| `unimportable` | The module or qualname could not be imported at restore |
| `source_exec_failed` | Saved function or class source failed during restore |
| `resource` | File, socket, generator, thread, task, lock, or similar live resource |
| `limit` | The binding exceeded the per-binding object limit |
| `denylisted` | The name is in `checkpoint_denylist` |

### 11.6 Publish

1. The worker streams every new blob as a binary frame, then sends `checkpoint_offer` with the manifest.
2. The supervisor writes each blob under `.kl/tmp/<attempt-id>/` and fsyncs it. A blob that already exists in `blobs/` is acknowledged without being written again.
3. The supervisor moves each new blob into `blobs/<sha256>` with `os.replace`.
4. The supervisor hashes the manifest, stores it as a blob, and inserts the blob rows and the checkpoint row in one SQLite transaction with `synchronous=FULL`.
5. The transaction commit is the moment the checkpoint exists.
6. The supervisor deletes the temp directory.

A crash before commit leaves no checkpoint id. Blobs already moved into `blobs/` are unreferenced and are removed by the next `kl gc`. A crash after commit leaves a valid checkpoint. The manifest blob is the manifest. The ledger row is the publication record.

Because blobs are addressed by hash, an array or table that has not changed since an earlier checkpoint produces the same blob and is not stored again. That is deduplication, not an incremental format. The walk still serializes and hashes every value.

Python version and the codec names and versions are fields in the manifest. Restore refuses a manifest whose `schema` is newer. It refuses a worker Python major.minor that differs from the manifest's, unless the user passes `--allow-python-mismatch`. The override is recorded on the new session. Platform differences are reported and do not, by themselves, refuse restore.

## 12. Restore, replay, and continue

**Restore** starts a new worker and a new session whose `restored_checkpoint_id` is set. The supervisor checks manifest hashes and blob presence, sends `restore`, and records `restore_result`. Missing blobs refuse the restore. The new namespace contains the restored bindings. Names that failed are absent and listed.

**Replay** starts a new worker with an empty namespace and executes stored source in ordinal order. It uses the stored source bytes, including source for cells that were later edited or deleted. It records a new session. The parent session id links back to the session being replayed. Environment fields that differ are printed before the first cell runs. Replay of an execution that issued `stdin_request` uses the stored reply only when the user passes `--reuse-input`. Otherwise that execution ends as an error and replay stops.

**Continue** is restore followed by ordinary new executions. New executions append to the new session. The restored session is unchanged.

**Restart** ends the current session as `closed` and starts a new worker with an empty namespace and a new session.

**Inspect** reads the ledger and blobs. It does not start a worker.

Replay and continue run user source. Writes, deletes, and network calls in that source happen again. The CLI states that when the command starts.

## 13. Import, export, and bundles

`kl import` reads an `.ipynb` with the standard-library JSON parser.

- Code, Markdown, and raw cells become cell files.
- Cell ids are kept when legal, otherwise minted, with the original saved in metadata.
- Attachments are base64-decoded into `attachments/`.
- Outputs and execution counts are not written into the tracked files.
- With `--keep-outputs`, those outputs become a synthetic session labeled `imported`. Imported events have no worker and no claim that this runtime executed them. Their source hash is the imported cell source.

`kl export` writes an `.ipynb` from the current source plus a selected session's folded outputs. Each code cell records the source hash that produced the attached outputs. When that hash differs from the current cell file, the exported cell metadata carries both hashes and `kernelledger.output_mismatch: true`.

`kl bundle export` writes a directory or zip:

```text
bundle/
  MANIFEST.json          # schema, notebook id, session ids, checkpoint ids, file hashes
  notebook/              # notebook.json, cells, attachments
  ledger.jsonl           # sessions, executions, events, checkpoint rows for the selection
  blobs/<sha256>
```

The manifest lists every blob the selection references. External things the notebook merely named, such as a CSV path the source opened, are listed under `external` from paths the user passed in. The bundle does not try to discover every file the source touched. `.kl/config.json` is never bundled.

`kl bundle show` prints ids, statuses, excluded bindings, notes, and hash mismatches. It does not execute source and does not restore.

`kl bundle import` copies blobs and ledger rows into a local `.kl/` after hash checks. Rows imported this way are marked with their origin. Restore and replay of that data still require the trust flag in section 15.

A published descriptor is a small JSON file under `checkpoints/<checkpoint-id>.json` containing the checkpoint id, manifest hash, blob hashes, status, excluded names, and notes. `kl publish` writes it. Payloads stay in `.kl/` or in a bundle. `kl fetch` reads a descriptor and reports every missing blob. A descriptor is a selected snapshot pointer, not a live log.

## 14. Git

Independent edits to different cell files merge as ordinary text. Reordering cells changes `notebook.json`. Two reorders can conflict in that file. Two edits to the same cell file conflict in that file. The resolution is a normal Git resolution. `kl doctor` after a merge checks ids, file presence, attachment names, and canonical newlines. **(v4)** It also reports Git conflict markers by file, and the source service reports them as the `conflict_markers` state (section 5.1), distinct from a malformed file. A branch switch is an external change like any other: clients see new revisions, and nothing already recorded changes.

The runtime does not install a custom merge driver in version 4.

Concurrent supervisors on one notebook directory are rejected by the lock. Two notebooks are two directories and two ledgers. There is no merged execution timeline.

## 15. Security

The checkpoint format is JSON plus raw blobs. Loading scalars, collections, arrays, and tables does not call user-defined reduce functions. NumPy blobs are read with `allow_pickle=False`.

Restore executes saved function and class source. Replay executes cell source. Both are code execution. `kl restore`, `kl replay`, and `kl continue` refuse to run without `--trust` when the data was not created by this local ledger, which includes every row from `kl bundle import` and every fetched descriptor. Data this supervisor wrote can be restored by the same user without an extra prompt. `kl show` and `kl bundle show` never need `--trust`.

The local socket accepts commands only with the token from `.kl/token`. The token file is created with mode `0600` on Linux. On Windows it gets an ACL that grants access to the current user only, set through `ctypes`. If the ACL cannot be set, `kl doctor` reports it. The worker connection uses its own one-time token (section 8). **(v4)** The desktop listener accepts the same token and is bound to `127.0.0.1` only. A desktop client reads `.kl/token` itself, as the CLI does. The token never appears in the registry, in logs, in protocol responses, or in diagnostic exports.

Checkpoint values are the user's variables. A bundle can contain secrets that were in those variables, in outputs, and in recorded `input()` replies. Export prints that warning. **(v4)** `getpass` replies are redacted by default (section 8.5). The product does not claim to find every secret. `checkpoint_denylist` forces named bindings into the excluded list.

Imported `.ipynb` outputs are data. They are shown as outputs. They are not executed. HTML outputs are never rendered by the CLI.

## 16. CLI

| Command | Effect |
| --- | --- |
| `kl init` | Create `notebook.json`, `cells/`, and `.kl/`, and add `.kl/` to `.gitignore`. |
| `kl serve [--python <path>]` | Run the supervisor in the foreground. |
| `kl stop` | Shut the worker and supervisor down. |
| `kl status` | Supervisor pid, worker interpreter, session id, and in-flight execution. |
| `kl doctor` | Report id, newline, attachment, ledger, interpreter, and token permission problems. |
| `kl doctor --repair-ids` | Mint ids for duplicates and record the map. |
| `kl retarget` | Assign a new notebook id. |
| `kl add code\|markdown\|raw` | Append a cell and print its id. |
| `kl rm <cell-id>` | Remove the cell from the manifest and delete its file. |
| `kl run <cell-id>` | Execute one code cell. |
| `kl run --all` | Execute code cells in manifest order. |
| `kl interrupt [--kill]` | Interrupt the in-flight execution, or kill the worker. |
| `kl restart` | New worker, empty namespace, new session. |
| `kl log` | List sessions and executions. |
| `kl show <exec-id>` | Folded outputs. `--events` for the raw log. `--mime` and `--out` to extract a payload. |
| `kl checkpoint` | Publish a complete or partial checkpoint, or print a refusal. |
| `kl restore <checkpoint-id>` | New worker, restored graph, new session. |
| `kl replay <session-id>` | New worker, recorded source, in order. |
| `kl continue <checkpoint-id>` | Restore, then accept new runs in that session. |
| `kl import`, `kl export` | `.ipynb` conversion. |
| `kl bundle export`, `kl bundle show`, `kl bundle import` | Self-contained archive. |
| `kl publish <checkpoint-id>` | Write a Git descriptor. |
| `kl fetch <descriptor>` | Report missing blobs for a descriptor. |
| `kl pin <name> <checkpoint-id>`, `kl unpin <name>` | Keep a checkpoint through `kl gc`. |
| `kl gc` | Delete unreferenced blobs. **(v4)** `--dry-run` lists them and their bytes. |
| `kl run --all --keep-going` **(v4)** | Batch that continues after an error (section 8.4). |
| `kl run <cell-id> --expect <sha256>` **(v4)** | Refuse with a conflict when the cell no longer has that hash. |
| `kl operation <operation-id>`, `kl operation --list` **(v4)** | Show an operation's state and result. |
| `kl cancel <operation-id>` **(v4)** | Cancel a queued operation, or the not-yet-started part of a batch. |
| `kl migrate` **(v4)** | Back up and upgrade the ledger to schema 2 (section 9.1). |
| `kl runtimes` **(v4)** | List registry entries with their verified state (section 3). `--prune` removes stale entries. |

**(v4)** `kl add`, `kl rm`, `kl import`, `kl retarget`, and `kl doctor --repair-ids` use the source service (section 5.1). They accept `--expect-revision <sha256>` for scripts that need the same conflict check as the desktop.

`kl run` starts the supervisor when it is not already running. Markdown and raw cells are skipped by `kl run --all` and can be printed by `kl show`.

Commands that need a live namespace fail with a clear error when the supervisor is down, and they do not invent an empty success. Restore is the way back to a saved namespace.

## 17. Compatibility

| Artifact | Version field | Rule |
| --- | --- | --- |
| `notebook.json` | `schema` | Refuse newer. Migrate older with an explicit `kl migrate` once an older schema exists. |
| Ledger | `PRAGMA user_version` | Same rule. |
| Protocol | `schema` on each message | Refuse newer. |
| Desktop protocol **(v4)** | `api` and negotiated version in `hello` | Highest common version. No common version closes the connection with the supported range. |
| Desktop helper **(v4)** | Version in its `hello` | Same rule as the desktop protocol. |
| Registry entry **(v4)** | `schema` | Ignore an entry with a newer schema. Entries are hints. |
| Checkpoint manifest | `schema` | Refuse newer. |
| Bundle `MANIFEST.json` | `schema` | Refuse newer. |
| Descriptor | `schema` | Refuse newer. |

Schema 1 is the first schema for every artifact. There is no silent best-effort load of an unknown field that changes meaning. Unknown fields in a known schema are preserved on notebook metadata and ignored on runtime messages only when the schema number matches and the field is outside the required set. Required fields that fail validation refuse the file.

Each artifact has a JSON Schema document under `schemas/` in the repository. Tests validate every written artifact against it. The runtime validates by hand and does not depend on a JSON Schema library. The schema files are what a later implementation in another language would build against.

**Version numbers are independent (v4).** Plan versions, artifact schema numbers, protocol versions, and package release versions are separate sequences ([plans/README.md](plans/README.md)). A plan version states which artifact versions it defines:

| Plan version | notebook | ledger | worker protocol | legacy protocol | desktop protocol | desktop helper | checkpoint, bundle, descriptor |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 3 | 1 | 1 | 1 | 1 | — | — | 1 |
| 4 | 1 | 1 and 2 (2 after `kl migrate`) | 1 | 1 | 1 | 1 | 1 |
| 5 | 1 | 1 and 2 (explicit migration) | 1 with training extensions | 1 with training extensions | 1 with capability-advertised training extensions | 1 with offline training inspection | 1; experimental training state/artifact schemas 1 |

A version 4 supervisor reads and serves schema 1 ledgers through the legacy protocol and refuses desktop work on them until migration (section 8.1). A desktop client release declares the desktop protocol versions it speaks. The packaged service it ships is one KernelLedger release, and an older running supervisor is handled by negotiation, not by assumption.

## 18. Implementation stack

Every component is Python. The worker has to be Python because it runs user code and walks live Python objects. The expensive work in the supervisor (hashing, SQLite, socket I/O) already runs in C through the standard library, so another language would mostly improve CLI startup and distribution, not checkpoint or restore time. Section 21 keeps a native supervisor as a deferred choice.

| Layer | Choice |
| --- | --- |
| Supervisor and CLI runtime | CPython 3.11 or newer, standard library only |
| Worker runtime | The project's CPython 3.11 or newer. `worker.py` is one stdlib-only file. |
| Supervisor concurrency | `asyncio`. The default Proactor loop handles subprocess pipes and sockets on Windows. |
| Ledger | `sqlite3`, WAL mode |
| Hashing | `hashlib.sha256`, streamed |
| Windows process control and ACLs | `ctypes` calls to Job Object and security APIs |
| Optional codecs | NumPy (`.npy` format), pandas with pyarrow (Arrow IPC) |
| Packaging | `pyproject.toml` with hatchling, a `kl` console script, installed with `uv tool install` or `pipx` |
| Development | `uv` for environments and locking |
| Tests | `pytest`, `hypothesis` for property tests that round-trip generated object graphs, and subprocess kill tests for every crash scenario |
| Static checks | `ruff` for lint and format, `pyright` in strict mode |
| CI | GitHub Actions, Windows and Ubuntu, Python 3.11, 3.12, and 3.13, each with and without NumPy, pandas, and pyarrow installed |

Source layout:

```text
src/kernelledger/
  cli.py
  notebook.py          # notebook.json, cell files, doctor, import/export
  ledger.py
  blobs.py
  supervisor/          # asyncio server, worker process control, lock
  worker.py            # stdlib only, run by path in the project interpreter
  source.py            # (v4) source service, section 5.1
  desktop/             # (v4) desktop listener, operations, feed, input requests, helper
  registry.py          # (v4) runtime registry, section 3
schemas/               # JSON Schema for every versioned artifact
tests/
```

`worker.py` imports nothing from the `kernelledger` package. A test enforces that.

## 19. Phases

Dates stay unset until phase 0 has measured checkpoint time and blob growth on a representative notebook.

| Phase | Deliverable | Completion evidence |
| --- | --- | --- |
| 0. Checkpoint spike | `worker.py` execution index, graph walk, encoding, loader, and NumPy and table codecs, driven by a test harness without a supervisor | Every checkpoint fixture in section 20 round-trips in a second process or is excluded with the expected reason. Hypothesis tests round-trip generated graphs of the builtin kinds with aliases and cycles. The section 20 measurements are recorded. |
| 1. Source | `notebook.json`, cell files, attachments, import/export, doctor | A second clone of the tracked files alone preserves ids, order, Markdown, raw cells, metadata, and attachments. Running a cell does not modify tracked files. A non-canonical newline fails doctor. |
| 2. Ledger | Supervisor, lock, worker launch in the project interpreter, protocol, display, sessions, events, blobs, log/show, interrupt, restart | The same cell run twice keeps two executions and both outputs. Editing the cell afterward still shows the historical source and historical traceback lines. A killed supervisor reopens with the in-flight execution `incomplete` and without a corrupt ledger row. A second supervisor is rejected. A pandas result records `text/html`. Binary display payloads survive. A million-line print stays under the row and byte limits. `kl interrupt --kill` ends a stuck native call. Inspection does not start a worker. |
| 3. Checkpoints | Publish rule, restore, continue | The phase 0 fixtures pass through the supervisor. A busy worker and a worker with a non-allowlisted thread publish nothing. A crash at each publish step leaves either no checkpoint id or a checkpoint whose hashes match. A restored function reads globals from the restored namespace. |
| 4. Replay and bundles | Replay, bundle export/import, publish descriptor, pins, gc | Replay uses historical source, including a deleted cell's stored source, and prints environment drift. Continue appends to a new session and leaves the old session unchanged. A bundle contains every referenced blob. Import of a tampered blob fails the hash check. `kl gc` removes only unreferenced blobs and keeps pinned checkpoints. |
| 5. Shared descriptors | Descriptor fetch, denylist, allowlists, trust flag, token ACL | A second clone can read a published descriptor, list missing blobs, and merge a cell-file edit with Git. Restore of imported or fetched data requires `--trust`. A denylisted name is excluded. |

Phase 0 and phase 1 can proceed in parallel. Phase 3 depends on phases 0 and 2. Phases 0–5 are the version 3 scope and are delivered.

Version 4 phases. Each one leaves the CLI fully working and keeps every version 3 scenario in section 20 passing.

| Phase | Deliverable | Completion evidence |
| --- | --- | --- |
| 6. Source service and schema 2 | Section 5.1 service and journal, CLI source commands routed through it, validation states, `kl migrate`, schema 2 tables, `canceled`, launch-aware interpreter rule, registry and `kl runtimes` | Killing a process at every journal step leaves the old or the new source after recovery. Two writers with the same expected revision: one succeeds and one gets `source_conflict`. Conflict markers are reported, not parsed as malformed JSON. A migration killed mid-way leaves schema 1 and a valid backup. A `private` supervisor with no project interpreter fails with candidates instead of running user code. |
| 7. Desktop protocol core | Section 8.1 listener and handshake, Discovery, `run` with `expected_sha256`, operations, change feed, `execution.follow`, committed-event streaming, `blob.read`, the helper's Source group and enough Inspection to show one notebook. JSON schemas and shared fixtures. | A minimal reference client (Python test client, then the frontend vertical slice) opens, edits, runs, and shows text output. A reply dropped after commit is recovered by `operation.get` without a second execution. A subscriber that overflows resyncs to the same final state. A legacy `kl` client works against the same supervisor at the same time. |
| 8. Batches and input | Section 8.4 batches, section 8.5 input claims and secret redaction, `kl run --all` as a batch, `kl cancel`, `kl operation` | Stop-on-error cancels the rest. A CLI run submitted during a batch runs after it. An input request survives its claimant disconnecting and is answered by another client. A `getpass` reply is redacted, and replay with `--reuse-input` stops with `secret_input_not_recorded`. |
| 9. Remaining groups | Checkpoints, Interchange, the rest of Inspection, `gc --dry-run`, `doctor` additions, the `docs/desktop-protocol.md` state diagrams | Every CLI command in section 16 has a desktop request or is listed as CLI-only in `docs/desktop-protocol.md`. Every fixture validates against its schema in both the Python and the Rust test suites. |

Phase 7 is complete only when a real client consumes it. It is designed together with the frontend vertical slice ([frontend plan](FRONTEND_PLAN.md), phase F1), so the protocol is shaped by a consumer rather than designed in isolation.

## 20. Validation

Checkpoint fixtures:

- Two names bound to one list, a self-referential list, and a tuple that contains a list that contains the tuple.
- A tuple that contains itself through a frozenset, expected `unsupported_cycle`.
- A dict with tuple and int keys, and a set of tuples.
- Integers outside the JSON safe range, non-finite floats, `-0.0`, a complex number, a `Decimal`, and a `Fraction`.
- A naive datetime, a fixed-offset datetime, and a `zoneinfo` datetime with `fold=1`.
- A function that reads a global, a nested closure, a function with a decorator defined in the notebook, a lambda, and a `@dataclass` with an instance.
- A function and a class defined in a cell, then a later edit to that cell before checkpoint.
- A `functools.lru_cache` function, expected `unsupported_type`.
- A module binding and an imported function binding.
- A NumPy array, a non-contiguous view of it, and an object array. A DataFrame with string, categorical, nullable integer, and timezone-aware columns and a named index.
- An open file, a socket, and a generator in the user namespace.
- A thread started by user code, and a tqdm progress bar.
- A list of 200,000 ints, expected `limit` for that binding only.

Runtime fixtures:

- Code, Markdown, raw text, and an attachment.
- A cell that calls `display` twice with one `display_id`, then `clear_output`.
- A cell that calls `input()`.
- A large binary display and a matplotlib figure.
- A cell that prints a million lines.
- Two executions of one unchanged cell.
- A project `.venv` whose Python minor version differs from the supervisor's.

Required scenarios:

| Scenario | Result |
| --- | --- |
| Run a cell | Tracked files are unchanged. Ledger gains an execution and events. |
| Edit, delete, and reorder after a run | Historical execution still resolves its source blob, traceback lines, and original cell id. |
| Identical source, two runs | Two outputs remain. |
| Second supervisor on the same directory | Rejected while the lock is held. |
| Kill the supervisor, start again | The lock is free without any pid check. In-flight execution is `incomplete`. |
| Kill the supervisor mid-run | No partial event sequence is marked `ok`. |
| Kill the supervisor after blobs move and before the checkpoint transaction commits | That attempt has no checkpoint id. `kl gc` removes its blobs. |
| Restore a complete checkpoint | Aliases compare equal with `is`. Cycles terminate. |
| Restore after the defining cell was edited | The restored function matches the checkpointed source, not the edited file. |
| Restore a function that reads a global | It sees the restored value of that global. |
| Unsupported binding | Checkpoint status is partial and names the binding, path, and reason. |
| Non-allowlisted thread or in-flight run | Refusal, no checkpoint row. |
| Allowlisted tqdm thread | Checkpoint proceeds. |
| Round-trip an array or table | Values match. A codec that cannot preserve values excludes the binding rather than publishing. |
| Worker interpreter differs from supervisor | Runs normally. The session records the worker interpreter. |
| `kl show` | No worker process is created. |
| Replay | Stored source runs in order. Drift is printed. Side effects are allowed because the command was replay. |
| Bundle | Every hash in the manifest is present. An edited blob is rejected. |
| `kl gc` | Unreferenced blobs disappear. Blobs referenced by a pin remain. |
| Text-only clone | Ids and attachments match without any `.kl/` directory. |
| Concurrent source writers **(v4)** | Exactly one of two same-revision mutations applies. The other is a conflict. |
| Crash during a journaled mutation **(v4)** | Recovery yields the complete old or complete new source. Removed content is recoverable until commit. |
| Run with a stale expected hash **(v4)** | Refused. No execution row. |
| Lost reply **(v4)** | Same `client_request_id` returns the original operation. No duplicate execution, checkpoint, restore, import, or publication. |
| Supervisor dies mid-operation **(v4)** | Takeover sets the operation's recorded outcome or `unknown`. Never `completed` without evidence. |
| Batch edited after acceptance **(v4)** | Frozen source runs. Recorded hashes match the acceptance snapshot. |
| Worker dies in a batch **(v4)** | Current execution `aborted`. Remaining `canceled`. No execution runs in the replacement session. |
| Input claimant disconnects **(v4)** | Worker keeps running its reader. The request stays pending and can be claimed again. |
| Secret input **(v4)** | No reply text in the ledger or in an exported bundle. |
| Feed overflow **(v4)** | `resync_required`, then catch-up reaches the same state as a client that never overflowed. |
| Desktop request to a legacy parser **(v4)** | Impossible by construction: separate listeners. Tested by connecting each client to the other port. |
| Schema 1 ledger with a version 4 supervisor **(v4)** | Legacy protocol works. Desktop gets `migration_required`. Nothing upgrades without `kl migrate`. |
| Private distribution without a project interpreter **(v4)** | `interpreter_required` with candidates. User code never runs in the bundled interpreter. |
| Stale registry entry **(v4)** | Fails authentication or connection and is reported stale. A reused port owned by another process is never treated as the runtime. |

Measure, in phase 0 and again in phase 3, on a notebook that holds a few hundred megabytes of arrays and tables: the time to checkpoint, the time to restore, the size of new blobs, the size of new blobs on a second checkpoint with nothing changed, and the time the worker spends inside the walk. Those numbers decide later incremental checkpoints. Version 4 does not specify an incremental format. A reused variable name or a matching cell hash is not evidence that an object was unchanged.

## 21. Deferred choices

These are real product choices. They are not hidden inside a phase exit.

| Choice | When it has to be made |
| --- | --- |
| Incremental checkpoints | After phase 3 measurements. A new plan version specifies the format. |
| Native supervisor and CLI (Rust or Go) | After phase 4, if a single-binary install without Python is wanted. The worker stays Python. The `schemas/` documents are the contract. |
| Graphical editor | Decided: the [frontend plan](FRONTEND_PLAN.md) specifies it, and phases 6–9 provide its contracts. |
| Provisional live output | When committed-event latency (at most one commit interval) proves too slow in the frontend. A new protocol capability marks provisional events and reconciles them to committed ones. |
| Redacting secrets already in history | Only with a history rewrite or deletion contract, which schema 2 does not have. Until then, doctor and export report recorded password replies. |
| Remote or multi-user desktop access | Not before a separate security plan. The desktop listener is loopback only. |
| Top-level `await` in cells | When a user need appears. `ast.PyCF_ALLOW_TOP_LEVEL_AWAIT` makes it possible. |
| Remote blob storage | After local bundles work. Descriptors already refer to hashes so the payload location can change. |
| Installing dependencies for replay | Only if replay should do more than report drift. |
| Codecs for GPU tensors, model objects, and open-file recreation | Each codec is an explicit addition with a round-trip test. |
| Restoring class attributes changed after definition | Only if `class_from_source` notes show it matters in practice. |
| Custom Git merge driver | Only if manifest conflicts become a practical problem. Cell source conflicts are ordinary text conflicts. |

## 22. Lineage

Version 1's guarantee split is the product. History, object restore, and process resume are different operations. Versions 2 and 3 keep the first two. Process resume is removed rather than postponed.

Version 1's publish rule is the crash rule: durable payloads first, then one commit point, then a visible id. The commit point is the SQLite transaction that inserts the checkpoint row.

Version 1's recording rule is the recording rule: persist the request before execution, key it by request id, keep events after a clear, and label missing cell attribution.

Version 2's shape is the system: the supervisor, the worker, and the typed graph.

Version 4 applies version 1's recording rule to clients: persist the request before acknowledging it, key it by a request id, and report an unknown outcome as unknown.

## 23. Changes from version 4

Version 4 is archived in [plans/v4.md](plans/v4.md). Version 5 retains its CPU guarantees and adds the bounded contract in section 24: explicit notebook training registration, separate portable capture and inspection, candidate worker admission, and native capability refusal. Frontend requirements F-TRAIN-01-04 depend on these additions. Ledger and existing protocol version numbers are unchanged. The experimental artifact schemas remain explicitly namespaced.

## 24. Opt-in notebook training checkpoints (version 5)

This successor adopts a bounded subset of the [version 5 draft](plans/v5-draft.md).
The draft's remaining gates, transports, warm cache and native backends remain planned;
publishing this contract does not qualify them. Sections 1–23 retain their CPU meaning.

Notebook code explicitly imports the installed optional `kernelledger.gpu.notebook`
plugin and registers one experiment. The standalone worker does not import the
KernelLedger package or frameworks on ordinary CPU startup. The project interpreter
must contain the plugin and its chosen framework; missing dependencies are reported.
All model/optimizer mutations must occur in `training_step`; loader drain and the
next-batch cursor are caller responsibilities. No arbitrary namespace is saved.

Legacy commands `training-checkpoint`, `training-list`, `training-inspect`,
`training-restore` and `native-checkpoint` are additive worker protocol 1 extensions.
Desktop protocol 1 advertises `portable_training` and `native_snapshot_status`;
clients must check capabilities. Effects `training-checkpoint` and `training-restore`
use durable operation ids. Read methods `training.list`, `training.inspect` execute
no source. Capture refuses a busy notebook worker. It uses a supervisor-created OS
temporary transfer directory outside `.kl`, verified before importing into the
separate `.kl/training-artifacts` chunk store. The supervisor alone records results
in existing session events. CPU GC and bundles do not own these separately rooted
artifacts. Training export/import remains the explicit experimental store API.

Restore requires explicit trust, validates captured closure and source, and launches
a candidate fresh worker with isolated temporary captured source. The saved recipe
is executed only there. Factory source is expected to reconstruct the declared
objects without loading the original namespace. The healthy worker is replaced
only after reconstruction succeeds; failed candidates are stopped. The restored
plugin is bound as `training`, with its objects available through `training.objects`.
No replay or notebook namespace restoration is implied. Strict environment/RNG
policy is the default. Process-global RNG makes a fresh worker mandatory.

Native capture returns an explicit refusal and capability evidence without invoking
CRIU or stopping the worker. Windows and the local AMD WSL driver route cannot supply
native GPU capture/reconnect. Compatible Linux qualification and a native backend
remain **(v5)**; portable success is never native qualification.

