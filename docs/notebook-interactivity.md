# Notebook interactivity: implementation and qualification

This working tree implements a custom Python interactivity subset across the
Python and opt-in Rust owners, the native GPUI desktop, and an optional local
Jupyter transport. The [N0–N10 draft](../plans/notebook-interactivity-draft.md)
remains a gated delivery plan. **This is not a release qualification or a claim
of full IPython, ipykernel, DAP or ipywidgets compatibility.**

The default owner is Python and the default engine is the standalone custom
CPython worker. Opening a notebook, browsing history and generating a kernelspec
preview install no packages and start no worker. Advanced debugger/widgets need
`KL_EXPERIMENTAL_INTERACTIVITY=1` in the owner environment before worker startup.
Changing this setting requires restarting the worker. Capability negotiation
reports what the current worker actually enables.

## Delivered behavior

| Increment | Implementation | Current boundary |
| --- | --- | --- |
| N0 | `CustomEngine`, additive protocol catalogs, worker generations, execution epochs, transient routing, measured engine spike and [ADR](notebook-engine-adr.md) | Custom engine retained; no production IPython selection or fallback |
| N1 | Token-aware suppression, timing/repeated timing/profiling, capture, optional bounded result shortcuts, command help, recorded-source reuse | Explicit subset of notebook syntax; repeated timing repeats side effects |
| N2 | Native editor completion popup and hover help; source-only fallback; idle namespace/attribute completion; Unicode offsets and stale-result guards | No Jedi/LSP process or arbitrary expression evaluation; editor services are local |
| N3 | Optional Variables panel, passive type/preview/docs/signature metadata, paged exact built-in containers and numeric NumPy arrays | Unsupported/custom/subclass values show passive metadata; no implicit `repr`, `dir`, property or user indexing hooks |
| N4 | Session future compiler flags and top-level await on a managed loop; cancellation at each cell boundary; detached-thread output attribution | No surviving async-task feature. Resistant tasks block execution and checkpoints until restart. Future flags reset on namespace restore |
| N5 | Console uses the notebook worker and ordinary durable run queue; origin labels, latest output, shared History and recorded-source insertion | Enter edits; the Run button submits. The latest result is shown; complete history stays in History |
| N6 | MIME metadata preservation, bounded typed tables/profiles, native equation subset, verified media save and explicit system-open controls, session-scoped display updates, safe fallbacks and imported-output limits | No embedded media player or active chart/webview; full TeX and larger replay/performance qualification remain open |
| N7 | Opt-in local `bdb` stepping/entry pause, revision-bound gutter breakpoints, current-line markers, source navigation, bounded stack/locals/context, post-mortem inspection, exclusive control claim and [DAP evaluation](notebook-debugger-adr.md) | Gutter edits require an idle worker and saved source. DAP/debugpy and arbitrary frame evaluation remain unavailable in production |
| N8 | Opt-in `kernelledger.widget/1` native button, checkbox, dropdown, integer stepper and read-only progress; durable callback admission, stale-handle refusal and retry deduplication | JSON subset, integer values only; no ipywidgets manager, arbitrary models, comms, floats or binary buffers. History/imported snapshots are inert |
| N9 | Exact-byte `%run` provenance; source-file preview before insertion; bounded `.ipynb` round trips and unknown-field retention; local signed Jupyter adapter and kernelspec generation | Real `BlockingKernelClient` tested; Notebook/Lab/VS Code version matrix, external debug/comms and clean-machine setup remain open |
| N10 | Shared owner tests, Windows native journeys, Windows/Linux headless evidence, optional dependency lanes, package/resource checks and explicit release-gate reporting | Remaining graphical, fault, performance, distribution and inherited migration exits remain binding |

## Use the desktop

Completion and hover help use the existing editor popup. Escape dismisses them
without executing. Live completions use the current draft and its hash; they are
discarded after source/caret, worker-generation or namespace-epoch changes.
Keywords and source names remain available while disconnected or busy.

Commands → Variables opens an optional panel. Refresh live variables runs only
while idle. Selecting a binding reads one value page; Next value page requests
the next range. Handles become stale after any execution or restore. Names and
previews never enter automatic assistant context.

Commands → Console opens input in the attached notebook session. Run in notebook
session records `origin=console`; its latest source/output appears below the
input. Commands → History gives durable source/results, including an action to
insert recorded source into a new cell without running it. Closing a panel
preserves the namespace. Commands → Load source file reads a bounded preview;
Insert as a new cell uses the normal revision-checked source mutation.

With advanced interactivity enabled, Commands → Debug selected cell pauses at
entry. The gutter and Toggle breakpoint at caret bind a line to the saved cell revision, so
editing that revision does not silently move the breakpoint. Commands → Debugger
shows stack, passive locals and current source context. Continue, Step over,
Step into, Step out and Stop use a priority transient control path while paused.
After an exception only inspection, Continue and Stop are available. One
controller owns mutations until execution ends; observers can request snapshots.
If it disconnects, runtime Interrupt/Terminate provides recovery. No frame
evaluation is hidden inside inspection. Checkpoint/restore/continue are refused
while paused, and frame snapshots are cleared on resume.

Stack and profile source links reveal the matching saved cell and caret. If the
source has changed, navigation opens a read-only window of the recorded executed
revision, preserving current edits. These verified windows show at most 15 lines
and 512 bytes per line from source blobs up to 8 MiB; previous/next controls read
another window. Library, external-script and restored generated filenames retain
context-only presentation when they cannot identify a recorded execution. Source
previews start no worker and write no history.

## Syntax and output examples

```python
answer = 42;                  # effects run, final result is suppressed
%time answer * 2
%timeit -n 2 -r 3 answer * 2   # at most 10,000 total iterations
%prun sum(range(1000))         # at most 100 profile rows
answer?
answer??                     # recorded source when available
%commands
```

```python
%%capture captured
print("captured text")
```

Capture retains at most 64 KiB of text and reports truncation. A named target
must be fresh. Errors still finish the execution as errors. `%%time`, `%%timeit`
and `%%prun` support cell bodies without moving definition line numbers.
`%%writefile [-a] path` writes the exact canonical cell body, including blank
lines; POSIX `%%sh`/`%%bash` and Windows `%%cmd` use the normal process-tree
interrupt path. Unsupported platform shells report an error.

`%run path.py` records actual UTF-8 file bytes, content hash, resolved path and
working directory before execution. It executes in the shared namespace and
indexes the actual file source for reconstruction. The original command stays
the execution's source. Editor file loading is a preview/insert operation, not
a runtime `%load` transformation. Namespace restore does not undo external file,
package or shell effects.

`%history on` enables `In`, `Out`, `_`, `__`, `___` only if their names are free;
`%history clear` and `%history off` release worker-owned cached references.
Settings offers the same actions as recorded console executions. Inputs/results
are bounded to 100 entries and an 8 MiB recursively accounted content budget.
Only exact supported built-in value graphs are admitted, with cycle/alias
accounting and traversal limits; arrays/custom/native-buffer objects are refused.
This bounds content, not total allocator/RSS overhead. Direct Python stores claim
injected names, changed dynamic bindings are preserved, and clearing replaces
views rather than mutating user aliases. Same-identity dynamic assignment cannot
be distinguished from an injected binding. The durable ledger is the history
authority; this optional cache and its compiler settings are not checkpoint state.

```python
import asyncio
await asyncio.sleep(0.01, result=42)
display_table([["alpha", 42]], columns=["name", "value"])
display_media(wav_bytes, "audio/wav")
control = widget("checkbox", False, label="Enabled",
                 callback=lambda value: print(value))  # advanced opt-in
display(control)
```

```python
display("Pending", display_id="job")
# A later cell or the shared console can update every current matching display.
update_display("Done", "job")
```

Media bytes are stored as attachments with visible fallback text. Save controls
support WAV, MP3, Ogg, MP4, WebM, PNG and JPEG. They stream verified 64 KiB ranges
to a temporary file, check the complete digest, enforce a 32 MiB bound and publish
only to a new filename with the matching extension. At most two exports run
concurrently, on separate helpers so export does not queue runtime controls.
After saving, Open file explicitly invokes the system application; there is no
autoplay or embedded media player.

`text/latex` renders a native, inert subset with fractions, roots, scripts,
Greek letters and common operators. It accepts at most 8 KiB, 512 tokens and
24 nesting levels, with a bounded scrolling viewport and accessible equation
description. Unsupported commands show the original TeX; Copy TeX source stays
available. It loads no macros, files, network resources or JavaScript. Full TeX
layout is not supported. `display_table` bounds rows/columns; large supported arrays and
containers use live inspector pages. Worker MIME hooks are explicit execution
and may call user code; passive inspection does not use them. Native static
renderers never execute JavaScript. Imported/recorded widget presentation never
replays callbacks. Live widget events use the same durable queue, source record
and idempotency identity as other executions; retrying an accepted event does not
repeat it. A closed/replaced model refuses events. Widget snapshots preserve
presentation, not callbacks or a live namespace.

## Transient and persistence contracts

Desktop protocol v1 adds `interactive_reads` and `code.complete`, `code.inspect`,
`namespace.list`, `namespace.preview`, `debug.control`, `debug.breakpoints`, and
`widget.state`. Existing required worker-frame fields, artifact schema 1 and
ledger schema 2 remain unchanged. Engine identity, transformer version, features
and generation are additive session environment fields. Execution origin is
stored in existing accepted request/result/status JSON and included in exported
views; no new SQL column or second writer was introduced.

The helper separately advertises `recorded_source` for `source.execution`.
Navigation requires the recorded execution identity, matching source digest and
a bounded line number; it never accepts an arbitrary filesystem path.
The `display_updates` capability enables bounded, read-only `display.updates`
pages for current output replay. Older helpers keep execution-local output views.

Transient reads carry request identity, a mandatory worker generation and, for
completion, source revision plus Unicode code-point cursor offsets. Namespace
handles/cursors include an execution epoch. Python routes at most four pending
reads per connection and 32 globally, superseding earlier completions on that
connection; native connections serialize reads and share a 32-read global limit.
Both expire replies after two seconds and bypass the durable execution queue.
Requests are limited to 60,000 encoded bytes; responses stay within the 64 KiB
worker inline frame and never become blob envelopes. Completion bounds candidate
count to 100, scanning 10,000 names; namespace pages scan at most 100 bindings,
with 48,000-byte content bounds. Value pages contain at most 100 elements.
Debugging bounds 100 breakpoints, 32 frames and 100 passive locals. Widget state
bounds 128 models, 64 KiB per model, 100 options and signed 32-bit integers.

Live namespace reads run on the execution thread while idle and refuse when user
background threads are present. Explicit ordinary source execution remains the
way to evaluate expressions. Transient requests/replies/debug snapshots never
become durable operations, events, blobs, exports or assistant context. Durable
widget callbacks and console/Jupyter executions retain the existing storage rules.
The native restore candidate is private until admission; live reads are refused
through reconstruction in either owner. Restoring/restarting invalidates handles
and clears active widget/debug presentation.

Async code uses the session loop. All managed tasks are canceled at the cell
boundary with a 0.2-second cleanup deadline. Cancellation-resistant tasks produce
`background_tasks=true` and refuse new execution/checkpoints until restart.
Both owners retain an interrupt received after execution admission but before
the worker's running acknowledgment, then deliver it to that execution. It cannot
leak into the next execution.
This is not a snapshot of coroutines, threads, executors or native resources.
User-thread output is explicitly unattributed to a foreground execution; it is
not relabeled as output of the next cell.

`.ipynb` import is bounded to 64 MiB and executes nothing. Unknown top-level/cell
fields, MIME metadata, vendor JSON arrays, binary media and unknown output kinds
are retained or given explicit fallbacks. Large imported text/JSON is blob backed;
oversized metadata is archived for explicit export. Imported execution counts are
metadata labels; export does not invent an executed state from those labels.
The current notebook and `.ipynb` export apply later updates to matching display
IDs in the same session. History keeps execution-local folding and immutable raw
events. Rerunning a cell retires its old targets; restarting/restoring isolates
the new session. Updated displays identify the updating execution. Replay and
cache limits produce a visible stale-output notice rather than silently dropping
evidence; Reload outputs replays without executing. See the
[display-update contract](notebook-display-updates.md) for precise bounds and
export/report behavior.

## Optional Jupyter setup

Install the `jupyter` extra in the adapter environment explicitly. Its declared
bounds are `jupyter-client>=8.6,<9` and `pyzmq>=26,<28`; the project worker does not
need either dependency. Start/attach the notebook owner normally first. Preview
a notebook-specific kernelspec, then install it only when intended:

```console
kl -C path/to/notebook jupyter-kernelspec --name my-notebook
kl -C path/to/notebook jupyter-kernelspec --name my-notebook --install
kl -C path/to/notebook jupyter-adapter -f path/to/private-connection.json
```

The adapter requires five distinct loopback TCP ports, a nonempty HMAC-SHA256 key
of at least 16 characters and a private connection file (POSIX 0600; restricted
Windows user ACL). Kernelspec installation refuses an existing name. The adapter
attaches to the owner; it creates no competing execution engine, worker or ledger
writer. It supports a Jupyter messaging 5.3 subset: execute, complete, inspect,
bounded history, stdin claims, interrupt, kernel info and detach shutdown.
Stable client-session/message identities map to durable execution keys. Unknown
side-effect outcomes are not automatically retried. Shutdown detaches the adapter
and leaves the shared runtime running. General comms/debug requests are unsupported.
This is local trusted-client access, not remote hosting or sandboxing.

## Qualification and remaining work

Raw evidence lives in [artifacts/notebook-interactivity](../artifacts/notebook-interactivity/).
The [gate record](notebook-interactivity-gates.json) lists every N0–N10 exit still
open. Run `python scripts/notebook_interactivity_gates.py --require-passed` to
fail closed for release; normal reporting exits successfully without declaring
the milestone qualified. [Inherited native gates](runtime-v6-gates.json) remain
unchanged. No Rust default switch, root plan publication or release was performed.

Reproduction commands and final local results are recorded in the
[qualification report](notebook-interactivity-qualification.md). CI adds optional
adapter/engine lanes for Python 3.11–3.13 on Windows/Linux and includes the shared
interactivity fixtures in native qualification. Existing broader Python-version
lanes are retained. Local headless tests and native UIA checks do not certify
Linux desktop behavior, NVDA/IME, every external frontend, clean-machine upgrade,
all fault conditions, full license-text distribution or release performance.
