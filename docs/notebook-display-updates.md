# Session-scoped display updates

The current desktop notebook and `.ipynb` export apply later `display_update`
events to visible displays with the same string ID and session. Execution-local
History, raw events and bundles retain their recorded evidence. No execution,
event or source row is rewritten, and no format/schema version changes.

## Lifetime and provenance

- `display(value, display_id="status")` registers each visible display occurrence.
  `update_display(value, "status")` updates every matching occurrence, including
  those in other cells and updates submitted through the shared console.
- A new accepted execution replaces that cell's current targets. Updates from
  before a display's creation do not affect the new display. Unknown IDs create
  no output. `clear_output` affects its own execution; `wait=True` takes effect
  before its next visible event, including an update with no matching target.
- Worker restart and namespace replacement create new sessions. Reusing an ID
  in the new session does not update the preceding session's recorded displays.
  Detached-thread output remains unattributed session history and is not replayed
  as an admitted execution's display update.
- Current views preserve the original cell execution/source identity and show
  **Updated by execution N**. `display_update_origin` records the updating
  execution, ordinal, event sequence and timestamp in the derived output.
  `.ipynb` MIME metadata retains this as `kernelledger_display_update_origin`.
- History folds each execution's own events, including its local updates. PDF
  and Word reports continue to present those execution-local recorded outputs.
  They do not silently replace recorded evidence with a later execution's data.

## Bounded replay

Helpers advertise `display_updates` for the read-only `display.updates` method.
Requests require inclusive `through_rowid` and exclusive `after_rowid` SQLite
row IDs, with an optional session filter. Values are integers from zero through
`2**63 - 1`, with `after_rowid <= through_rowid`. The caller freezes the upper
watermark, allowing a finite catch-up while the owner continues appending.

Each page scans at most 128 raw event rows, returns at most 16 updates, and uses a
256 KiB content budget. Non-update payloads are not selected. Update payloads are
limited to 64 KiB; identity/time fields to 128 UTF-8 bytes. Invalid or oversized
records produce an omission marker and still advance the cursor. String IDs are
nonempty, at most 256 UTF-8 bytes and exclude C0 controls and invalid Unicode.
Inline update JSON has at most 64 container nesting levels, finite floats and
integers in the signed 64-bit through unsigned 64-bit range. Unsupported inline
numeric forms are refused rather than converted to approximate large integers.
Blob-backed values remain references until the normal verified viewer/export
path reads them.

The desktop processes one replay page per poll, between ordinary commands. It
keeps at most 128 current display-bearing views, 256 output slots per view, 1,024
session/ID keys, 2 MiB per view and 8 MiB of retained view content. Latest update
content has a separate 4 MiB bound. These are serialized-content budgets, not an
allocator/RSS claim. Source text is included in the view budget.

Within one page, the desktop can apply only the last presentation for an ID.
Every raw update and callback execution is retained; callback admission or clicks
are never coalesced. A source-cell membership change refreshes registrations.
Refolding a still-current execution reapplies its latest later updates. Removed,
cleared or replaced targets release their cached content. Output comparison
retains digests rather than a second unbounded copy of every resolved view.

While catching up, the desktop shows a replay notice. If limits or omissions
prevent following an update, it warns that current outputs may be stale and
offers **Reload outputs**. Reload restarts replay without running code. A late
image or text-page response must match the current execution and blob digest;
it cannot replace a newer display's content. Historical views can independently
accept a page for their own recorded digest.

`.ipynb` export folds within one SQLite read snapshot and reads replay pages
incrementally. An omitted update refuses that export with a useful error;
raw History and bundle export remain available. Existing large notebook/export
materialization and performance qualification gates remain open.

## Evidence and remaining qualification

`tests/test_display_updates.py` covers shared real workers, immutable History,
multi-cell/multi-occurrence updates, clear/wait, re-execution, session replacement,
frozen export snapshots, bounded scans, malformed/deep/oversized/numeric payloads
and exact helper reply parity. Native interactivity invokes the same worker
lifetime fixture. Rust client/UI tests cover replay across unchanged watermarks,
view/cache bounds, source membership, refolding, stale replies and explicit reload.

The Windows advanced probe runs the update cell through native controls, observes
the earlier output and origin label, verifies untouched History and updated
`.ipynb` data, then reruns the original cell. It also adds, edits and runs a new
cell after registration changes. See the
[qualification record](notebook-interactivity-qualification.md) for exact runs.
Large-history/control-latency stress, Linux graphics and broader accessibility
qualification remain open. Active HTML/JavaScript rendering remains unavailable.
