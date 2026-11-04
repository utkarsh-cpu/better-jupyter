# Notebook interactivity: local qualification record

Development evidence collected 2026-10-25–27. See the
[support matrix](notebook-interactivity.md) and
[open N0–N10 gates](notebook-interactivity-gates.json). Passing the checks below
does not qualify the entire roadmap for release. No release, default-owner switch
or authoritative root-plan publication was performed.

## Environment and results

Windows 11 build 26200, AMD64 Family 26 Model 112; CPython 3.12.10 for the default
and native owners. Isolated uv environments supplied CPython 3.11.16 and 3.13.15
and optional adapter dependencies. Linux tests ran in Ubuntu 24.04 under WSL with
CPython 3.12.3 and a separately compiled native daemon. These are headless Linux
checks, not a Linux graphical qualification. The
[environment inventories](../artifacts/notebook-interactivity/environment-windows312.json)
and [optional dependency inventory](../artifacts/notebook-interactivity/environment-adapters-windows312.json)
record actual distributions. Adapter evidence uses jupyter-client 8.10.0,
pyzmq 27.2.0 and IPython spike 9.17.1; numeric NumPy previews were checked with
NumPy 2.5.2 on Windows 3.12. Optional NumPy was absent in the isolated lanes.
The final [fresh Windows environment](../artifacts/notebook-interactivity/environment-isolated-windows312.json)
installs this checkout with dev/Jupyter extras and no system site packages,
NumPy, pandas, PyArrow, Matplotlib or PyTorch. Its additional skips explain the
lower pass count compared with the layered environment.

| Check | Result | Evidence |
| --- | --- | --- |
| Full Python suite in a fresh virtual environment, Windows 3.12 | 258 passed, 140 skipped | [JUnit](../artifacts/notebook-interactivity/python-display-windows312.xml) |
| Full Python suite from installed checkout, Windows 3.12 | 293 passed, 93 skipped; one Hypothesis input-generation health-check failure | [JUnit](../artifacts/notebook-interactivity/python-installed-windows312.xml) |
| Checkpoint module with the reported Hypothesis seed, isolated rerun | 14 passed | [Log](../artifacts/notebook-interactivity/checkpoint-seed-current-windows312.log) |
| Full selected native-owner, recorded-source and display tests, Windows 3.12 | 125 passed, 1 skipped | [JUnit](../artifacts/notebook-interactivity/native-display-windows312.xml) |
| Final display lifetime, helper parity and interchange fixtures, Windows 3.12 | 8 passed | [JUnit](../artifacts/notebook-interactivity/display-focused-windows312.xml) |
| New core worker/debug/widget/interchange fixtures, Windows 3.12 | 28 passed | [JUnit](../artifacts/notebook-interactivity/core-windows312.xml) |
| Optional Jupyter client and native interactivity, Windows 3.12 | 15 passed | [JUnit](../artifacts/notebook-interactivity/adapters-windows312.xml) |
| Interactivity, shell and boundary fixtures, Windows 3.11 | 52 passed, 2 skipped | [JUnit](../artifacts/notebook-interactivity/python-windows311-short-path.xml) |
| Same selected fixtures, Windows 3.13 | 52 passed, 2 skipped | [JUnit](../artifacts/notebook-interactivity/python-windows313.xml) |
| Linux Python/native worker, debugger, widgets, Jupyter, numeric and desktop contracts | 71 passed, 1 skipped | [JUnit](../artifacts/notebook-interactivity/native-linux312.xml) |
| Current Linux Python/native interactivity, display, startup interrupt, recorded source and optional Jupyter fixtures | 56 passed, 1 skipped | [JUnit](../artifacts/notebook-interactivity/display-linux312.xml) |
| Both-owner startup interrupts through legacy/desktop controls and native Jupyter regression, Windows 3.12 | 5 passed | [JUnit](../artifacts/notebook-interactivity/interrupt-startup-windows312.xml) |
| Rust workspace | 104 passed | [Log](../artifacts/notebook-interactivity/rust-display-workspace.log) |
| Disposable debugpy/DAP evaluation, Windows and Linux 3.12 | Two real-process fixtures passed on each OS; production adapter not adopted | [Windows](../artifacts/notebook-interactivity/debugger-spike.json), [Linux](../artifacts/notebook-interactivity/debugger-spike-linux312.json) |
| Ruff on source/tests and changed/new scripts; strict Pyright; Cargo Clippy with warnings denied | Passed | Commands below |
| Worker/desktop schema generation and runtime contract/hash drift checks | Passed | Generated schemas and shared fixtures |
| Wheel and native archive | Verified worker/source identity and display replay inclusion; CLI wheel smoke passed | [Wheel](../artifacts/notebook-interactivity/package-wheel.json), [native manifest](../artifacts/notebook-interactivity/package-native-windows.json) |

Skips include native-only tests in the default lane, unavailable optional packages,
GPU/hardware work, and POSIX-only behavior on Windows. The current native lane
includes optional Jupyter; earlier native runs omitted it when absent. Earlier
3.11/3.13 and broader Linux reports cover prior development snapshots. The current
Windows and focused Linux rows include recorded source and session display replay.
Counts are per invocation and must not be summed as unique tests. Final focused
read/deadline/hostile-key results are recorded in
[interactive-final-windows312.xml](../artifacts/notebook-interactivity/interactive-final-windows312.xml)
and [native-final-linux312.xml](../artifacts/notebook-interactivity/native-final-linux312.xml).

The full native lane includes existing durable retries, input claims and
reply/cancel races, feeds/slow readers, crash recovery, resource extraction,
checkpoint aliases and failed-candidate isolation. The new shared tests add
suppression/source reconstruction, capture/error/file semantics, history ownership,
future imports/await/task refusal, Unicode/NFKC ranges, hostile passive hooks,
ephemeral reads without ledger events, restore-read exclusion, debug ownership,
revision-bound earlier-function breakpoints, post-mortem locals, accepted widget
callbacks, stale models and real signed Jupyter attachment/stdin/interrupt/detach.
Rust checks include console newline/shortcut isolation and a held worker-write
lock that live reads must refuse promptly.

Display tests add multi-cell/multi-occurrence broadcast, console updates, creation
ordering, clear/wait, rerun and session isolation, snapshot-consistent export,
bounded replay and malformed/deep/oversized/numeric payload parity. Rust checks
cover unchanged-watermark catch-up, bounded caches, source membership changes,
late image/text replies, explicit reload and navigation after structural resets.
The [display-update contract](notebook-display-updates.md) distinguishes current
notebook/`.ipynb` presentation from immutable execution-local History and reports.

## Native desktop journeys

`scripts/desktop_ui_probe.py --exercise-runtime --exercise-interactivity` uses
native Windows UI Automation and messages addressed only to its own application
window. It creates a disposable notebook/profile and stops its own runtime.
The light-theme journey passed run → checkpoint → restore → Variables refresh →
multiline Console run → visible console output. Dark and high-contrast journeys
passed Variables/Console/output as well. Runtime checks assert durable records;
console checks assert shared namespace values and accessible output, rather than
only taking screenshots.

Reports: [light](../artifacts/notebook-interactivity/ui-light/probe-light.json),
[dark](../artifacts/notebook-interactivity/ui-dark/probe-dark.json),
[high contrast](../artifacts/notebook-interactivity/ui-contrast/probe-contrast.json).
Each exercised minimum 800×600 logical size, four window sizes and ten resize
cycles on a 192-DPI display. Window crops supplement behavior tests. They do not
certify mixed-monitor DPI, NVDA, IME or Linux graphics. Completion
keyboard/Unicode/focus tests run in the Rust UI harness; the real completion
popup's broader keyboard and IME/screen-reader journeys remain open. The current
Enter/Escape OS checks below cover only their stated acceptance/dismissal path.

The advanced Windows journey passed with the Python and native owners/helpers:
set a last-line gutter breakpoint → Debug entry → Step over → source/caret
navigation → edit while paused → open the immutable recorded source → Continue
to the original breakpoint → finish the executed revision → click a native
widget and verify one callback → inspect the native equation/table/media actions.
Reports: [Python](../artifacts/notebook-interactivity/ui-advanced/probe-light.json)
and [native](../artifacts/notebook-interactivity/ui-advanced-native/probe-light.json).
Python-owner advanced journeys also pass in
[dark](../artifacts/notebook-interactivity/ui-advanced-dark/probe-dark.json) and
[high contrast](../artifacts/notebook-interactivity/ui-advanced-contrast/probe-contrast.json).
These checks also exposed and fixed the editor's clipped final line and a missing
command for returning to the debugger after source navigation.

The current display journeys add a native Run of an update cell, observe the
earlier cell's new output and origin label, verify original History and updated
`.ipynb` data, rerun the target, then add/edit/run a fresh cell and return to rich
output. Reports: [Python](../artifacts/notebook-interactivity/ui-display-python/probe-light.json)
and [native](../artifacts/notebook-interactivity/ui-display-native/probe-light.json).
These light-theme runs include the advanced debugger/widget/media journey above.
Python-owner runs also pass in
[dark](../artifacts/notebook-interactivity/ui-display-dark/probe-dark.json) and
[high contrast](../artifacts/notebook-interactivity/ui-display-contrast/probe-contrast.json).
All four runs type a namespace-only completion prefix, accept with Enter, dismiss
the next popup with Escape and prove Enter resumes editing without recording an
execution. The current UIA tree does not expose editor `HasKeyboardFocus`
reliably; subsequent editing, rather than that property, is the focus evidence.
This does not qualify popup announcement or IME behavior.

The media save journey accepts the suggested new filename in the Windows picker,
verifies the resulting file byte-for-byte and checks that Open file is offered.
It launches no system player. Common-dialog controls missing from UIA's descendant
view are located only under the probe's own process/windows and activated through
native messages. This behavior check does not qualify renaming with assistive
technology or keyboard-only use of that system dialog. Unit tests separately
verify cancellation, no-overwrite publication, digest/size checks and the two-export
concurrency limit. Linux graphical, multi-window/reconnect, paused disconnect/crash
and the broader accessibility matrix remain open gates.

## Measurements

The [engine ADR](notebook-engine-adr.md) retains the custom engine. Three
alternating custom/IPython process trials record engine-initialization time,
process wall time, post-fixture RSS/private/USS, compatibility outcomes and
checkpoint exclusions. IPython history and caching are disabled in the spike.
Custom reconstruction had one exclusion; IPython had nine with the current
source-index bridge. The spike does not qualify a production IPython adapter.

The local [Python](../artifacts/notebook-interactivity/latency-python-windows312.json)
and [native](../artifacts/notebook-interactivity/latency-native-windows312.json)
read probes use one client and 1,000 names, with 50 requests for each idle method
and 20 busy completion refusals:

| Probe | Python owner | Native owner |
| --- | --- | --- |
| Idle completion p95 | 0.55 ms | 0.64 ms |
| Passive scalar inspection p95 | 0.35 ms | 0.42 ms |
| 100-binding namespace page p95 | 3.18 ms | 4.29 ms |
| Busy completion refusal p95 | 0.23 ms | 0.33 ms |
| One cancellation while producing 2 MB of output | 70.59 ms | 142.65 ms |

These development-snapshot measurements include exact hashes in their raw
records. Later deadline/hostile-key tightening is covered by focused regressions;
these numbers are not a new frozen release baseline. They are owner round trips,
not end-to-end editor popup latency. A single cancellation observation is not a
p95. No performance improvement, owner superiority or release-scale memory claim
is inferred. Loaded fan-out, 10,000-cell histories, 32/256 MiB and 1 GiB graphs,
allocator peaks and retained-memory stress remain open.

## Reproduction

Use the declared optional bounds, or explicitly install `.[dev,jupyter,ipython-spike,debugger-spike]`
in a disposable environment. Set `PYTHONPATH` to the absolute `src` directory when
running tests from a source checkout in an uninstalled uv environment; pytest's
own `pythonpath` configuration does not propagate to subprocesses.

```console
python -m pytest tests -q
cargo build --manifest-path desktop/Cargo.toml -p kernelledger-daemon -p kernelledger-desktop
python -m pytest tests -q -k native
cargo test --manifest-path desktop/Cargo.toml --workspace --no-fail-fast
cargo clippy --manifest-path desktop/Cargo.toml --workspace --all-targets -- -D warnings
python -m ruff check src tests
python -m pyright src
python scripts/schema_builder.py
python scripts/desktop_contracts.py
python scripts/runtime_schema.py --check
python scripts/runtime_contracts.py --check
python scripts/desktop_ui_probe.py --exercise-runtime --exercise-interactivity
python scripts/desktop_ui_probe.py --exercise-advanced
python scripts/notebook_engine_spike.py
python scripts/notebook_debugger_spike.py
python scripts/notebook_interactivity_probe.py --output latency-python.json
python scripts/notebook_interactivity_probe.py --native-daemon /absolute/daemon --output latency-native.json
python scripts/notebook_interactivity_gates.py --require-passed
```

The native pytest invocation requires `KL_NATIVE_DAEMON` pointing to the compiled
daemon. Windows uses an `.exe` path. Optional adapters and hardware dependencies
are installed explicitly; normal worker execution needs neither IPython nor
Jupyter packages. The last gate command is expected to exit **1**, with explicit
remaining feature and inherited native gates. Package checks used a temporary
directory: `uv build --wheel`, wheel contents/hash inspection, an isolated wheel
`kl --help` invocation, and `scripts/native_package.py` manifest/content/resource
verification. Large binary packages are outside the repository; manifests are
retained. This does not substitute for clean-machine install/upgrade/rollback.

## Failures found and resolved, and platform limits

The native input reply/cancel race exposed a late interrupt during terminal
debugger cleanup. Removing filesystem work from that terminal path and limiting
asynchronous interrupts to foreground execution fixed the regression; the native
input race and full native suite passed afterward. Restore review found a native
candidate could publish its public socket before admission; candidate publication
is now deferred, and live reads are excluded through reconstruction in both owners.

The expanded Linux run exposed an interrupt sent after admission but before the
worker's running acknowledgment. The not-yet-active worker could ignore it,
leaving the Jupyter execute reply pending. Both owners now bind early interrupt
delivery to that execution and reconcile it on the running acknowledgment.
Native delivery also coalesces an early interrupt with a durable cancellation
at that boundary. A delayed real worker fixture reproduces the race through
both protocols and verifies the next execution is unaffected. The
[initial failed Linux report](../artifacts/notebook-interactivity/display-linux312-pre-interrupt-failure.xml)
is retained; the final expanded Linux run passes.

Initial optional-client tests assumed the next shell reply belonged to execution
after readiness. Bare Jupyter clients can issue repeated kernel-info requests;
tests now correlate by parent message identity and retry only read-only readiness
while the adapter process is starting. Execute requests are not retried by this
test correction. Initial uninstalled uv runs also omitted the subprocess source
path; corrected reruns passed. Failed setup artifacts are retained with
`setup-failure` names, separately from the passing reports.

The first Windows 3.11 shell/package run encountered the machine's Windows path
limit inside bundled setuptools. Qualification used a fresh short `C:/Temp`
test directory and passed. No registry/system long-path setting was changed;
the [original failed report](../artifacts/notebook-interactivity/python-windows311.xml)
is retained. Clean-machine path/reparse and installation behavior remains a gate.

Initial UI automation targeted an uninvokable command row and then expected a
bare output label. The probe now searches and activates commands with the native
keyboard path and asserts the actual accessible output group. The latest native
journeys pass. The display journey found two product regressions: output tracking
could omit newly added source cells, and revealing a cell after structural reset
could use invalid estimated list heights. Source membership now refreshes the
output watch; unmeasured reveal targets anchor directly in the virtual list.
The real completion journey also found Escape dismissal propagating into notebook
command mode. The workspace now dismisses the focused editor overlay before that
parent action, preserving edit focus. A Rust regression covers completion/help,
subsequent editing and a second Escape returning to command mode.
The media probe now scrolls until Save is inside the viewport before invoking it,
instead of relying on a fixed scroll count that could overshoot. Screenshots and
earlier failure captures are supplementary, not
release proof. Advanced renderer/DAP/ipywidgets/external-client functionality and
platform qualification still have the explicit limits listed in the support and
gate records.

The final broad Windows run initially generated desktop schemas using an editable
installation from another checkout. Regeneration with this checkout installed
and the explicit source path fixed the drift check. Desktop qualification now
also installs this checkout for the isolated `-I` helper process; setting only
the parent's `PYTHONPATH` cannot select that helper. Failed setup/probe logs are
retained separately from the successful JSON reports.

The layered Windows environment's full run has one remaining Hypothesis
input-generation health-check failure: one draw exceeded its startup budget.
The same seed passes the checkpoint module (14 tests), the property with every
test module collected, and a profiled 115-test preceding-order run. The profile
shows Hypothesis's local-constant/module scan dominating generation, rather than
checkpoint serialization. No health check or assertion was disabled. This failed
full run remains recorded; it is not counted as a green release gate.
The fresh virtual environment subsequently passed the full suite (258 passed,
140 skipped), retaining the same health checks and assertions.
