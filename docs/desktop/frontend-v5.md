# Frontend v5 implementation and qualification ledger

Updated 2026-10-09. This is an implementation of the [v5 draft](../../plans/frontend-v5-draft.md) in progress, not an F6-qualified release. Frontend v4 remains the published baseline. Requirements below are not withdrawn when evidence is incomplete.

## Implemented workbench

The native GPUI application uses a compact notebook switcher, Notebook / History / Checkpoints navigation, Commands, Runtime, and Menu. The outline belongs only to Notebook, clamps to 180–300 logical pixels, becomes a compact drawer, and preserves manual collapse through resizing. Cells form a continuous document with separate selection and editor focus. Existing runtime, source, history, draft, and trust paths remain in the service layer.

UI modules now separate theme, components, shell, notebook, outputs, history, checkpoints, runtime, settings, help, diagnostics, interchange, and accessibility. The shared typed runtime action selector provides page-independent checkpoint availability and blocks runtime mutations when connection state is unknown. UI preference schema 1 persists outline, density, editor size, and reduced-motion preferences without replacing appearance, recent notebook, or draft storage.

## Requirement coverage

- **F-RD-01–03:** Neutral light/dark semantic colors, high contrast, compact controls, flat cell composition, native typography, header and menus implemented. All-surface visual review and syntax/disabled contrast qualification remain open.
- **F-RD-04–05:** Contextual constrained outline, compact drawer, persistent collapse, keyboard-accessible width settings, independent surface details/scroll/filter storage implemented. Headless resize regression covers four requested widths and ten transitions. Optional inspector, full text-zoom and mixed-DPI matrix remain open.
- **F-RD-06:** Continuous cells, insertion, duplicate/delete/reorder, saved-source split at Unicode cursor, confirmed same-type merge, stale-source Compare, editor focus and run shortcuts implemented. Multiple-cell selection and structural undo remain open. Virtualized native IME/selection retention needs qualification.
- **F-RD-07:** Loaded-output copy/find/collapse/size controls, expandable traceback, fit/actual images, bounded native tables and text fallback implemented. Full output paging, image cache eviction, MIME export and complete table keyboard navigation remain open. Text blob previews are bounded to 64 KiB; display-size controls do not fetch omitted bytes.
- **F-RD-08:** All existing destinations remain reachable. History adds outcome filters and advanced cell/session/date query; checkpoint page creates snapshots and displays coverage; Diagnostics has actionable findings and redacted copy; Settings exposes persisted layout controls; Help is searchable; Commands prioritizes common work. Cursor-based Next page and Back navigation are implemented. Rich history date/session grouping, structured date controls, bounded backend queries and import review remain open.
- **F-RD-09–10:** Shared runtime availability, explicit unknown connection state, durable failure messages, checkpoint completion link/list refresh, separate save feedback, compact technical details and safe confirmations implemented. Exhaustive multi-client/disconnection/accessibility qualification remains open.
- **F-RD-11–12:** Namespace coverage, exclusions, restore/continue confirmation, conservative trust and mismatch-specific override implemented. Training creation is contextual and capability-gated; experimental scope is explicit. Source and bundle exports use native Save As and save acknowledged source through the service. Export results remain associated with their notebook. Authoritative provenance/environment preflight, import review, pin/publish and full advanced workflow parity remain open.
- **F-RD-13:** Named controls, selectable outline/palette rows, modal trapping/return, output accessibility text, keyboard alternatives and headless IME shortcut protection implemented. Actual NVDA task completion and complete repeated-control/splitter/table semantics remain open.
- **F-RD-14–15:** Module separation, common components, typed runtime decisions and versioned preference migration implemented. The generic record renderer still needs further decomposition. Contract dependencies are listed below.
- **F-RD-16–20:** Work spans R1–R4 with automated/native evidence below. R0 direction approval, complete R2/R3 journeys and R5 qualification have not passed. No expert score, participant results, performance qualification or release completion is claimed.

## Contract and implementation dependencies

1. **Structural undo:** Source service supports atomic split/merge/duplicate/move, but no verified inverse transaction preserves all identifiers, metadata, attachments and revisions. Do not implement undo by rewriting notebook files from the view. Extend and test the source contract before advertising undo.
2. **Checkpoint preflight:** The current report establishes recorded coverage, not authoritative artifact provenance or environment/source compatibility. Keep conservative trust confirmation. A mismatch override is only offered after a matching checkpoint operation reports `python_mismatch`.
3. **Scale:** Existing helper folds and client history hydration can load all pages. Oversized protocol responses, bounded SQL pagination and output byte-range loading need contract/client work before the 100 MiB and large-history gates can pass. Image-cache eviction is remaining client work, not inherently a protocol blocker.
4. **Subscriptions:** The app polls committed helper folds; transport support for subscriptions alone does not establish application reconnect/backpressure behavior.
5. **Interchange:** Export contracts exist and are connected. Bundle export requires recorded history. Import review and pin/publish remain UI/contract integration work; they are not represented as completed or universally blocked by the backend.
6. **Qualification:** NVDA, actual Windows IME, mixed monitors, clean-machine packaging, participant studies and independent visual judgment require their specified environments/participants. Automated checks do not replace them.

## Evidence and limits

- Rust workspace: 26 tests passed on 2026-10-09, including source/save state, action gating, palette/dialog focus, run bindings, headless IME suppression, Unicode split/merge gating, preference migration and ten-cycle outline resizing.
- Python source/desktop/contract tests: 34 passed using the locked development environment.
- Strict Clippy across all workspace targets and the locked Windows debug build passed. A debug build is not signed/private-runtime installation qualification.
- `scripts/desktop_smoke.py`: passed in this implementation session after fixing temporary-directory teardown timing.
- `scripts/desktop_lifecycle.py`: passed sample creation, stop idle/busy/from another client, offline history, restart on run, save-before-source-export offline bundle export and cursor-based offline history paging.
- [Native artifacts](../../artifacts/frontend-v5/native/): light, dark and high-contrast reports, UIA trees and screenshots. Native probes passed with populated stdout, error and checkpoint fixtures at 192 DPI, four requested sizes and ten resize cycles. Menu/destination and output UIA checks passed. Captures precede the subsequent cursor-navigation change. Reports explicitly distinguish time to visible window from editor-ready latency.
- Desktop screenshots are captured only while the launched application owns foreground focus. Physical screen limits/occlusion still require visual review. Capture success is not a complete control reachability test.

FS-30 has automated/headless and limited native evidence. FS-32/34/35/36/37/41 have partial regression evidence. FS-31/33/38/39/40 and all formal visual, performance, packaging and participant gates remain incomplete. None of FS-30–41 is claimed fully qualified solely by this ledger.

## Next implementation work

Complete multiple-cell operations and source-backed undo; bounded history queries/grouping; import review and pin/publish; bounded image/output lifecycle; authoritative checkpoint preflight; full keyboard/accessibility semantics. Then execute the platform, performance, recovery, visual and user-study matrix before reconciling and publishing frontend v5 as Current.
