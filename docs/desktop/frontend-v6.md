# Frontend v6 implementation and qualification

Date: 2026-10-09. This records the implementation of the
[v6 draft](../../plans/frontend-v6-draft.md) in the working tree. It preserves
the existing v5 changes and the published frontend v4/runtime v5 plans.
The contract-independent UI work is implemented. The draft's full release
acceptance is **not complete**: service contracts, platform/accessibility,
large-history performance, participant studies and packaging remain gates.

## Implemented behavior

| Requirement | Implementation | Qualification limit |
| --- | --- | --- |
| F-APL-01 | Adaptive header; disclosed notebook switcher; one Add cell menu; contextual cell actions; Commands/Runtime move into Menu at compact widths; Run all remains in Commands at the largest compact text size | Whole platform/width matrix remains open |
| F-APL-02 | One stable insertion slot per boundary; hover/focus disclosure; selected boundary and Comfortable density remain visible; cell More and Commands provide structural actions | Speech and touch qualification pending |
| F-APL-03 | Short source fits its line count; shared source/output canvas and alignment; prose width limit; Succeeded/Failed outcomes; traceback beside the error | Real Windows IME and large virtualized notebooks pending |
| F-APL-04 | Semantic theme colors and subtle decorative dividers; audited Kit syntax/control colors; interface scale 100/125/150/200%; independent editor size; theme font synchronization | OS text-scale integration and mixed-monitor validation pending |
| F-APL-05 | Trigger-bound notebook/runtime/application menus clamped to the window and updated after resizing; Menu accessibility role; Escape and focus-return handling | Native screen-reader traversal pending |
| F-APL-06 | Kit press feedback; guarded availability; source/runtime states remain separate; copied acknowledgment; checkpoint result link; restored-session banner with actual returned session id | End-to-end disconnect/operation reconciliation qualification pending |
| F-APL-07 | Existing structural service actions retain stable ids, expected revisions and conflict guards; delete confirmation; recovered draft comparison stays in Diagnostics | Structural Undo/Redo and multiple selection are unavailable; inverse/metadata/attachment guarantees are unverified |
| F-APL-08 | Dedicated checkpoint list/report; labeled exclusions and notes; ordinary empty states; unknown counts/context remain unknown; trust-gated consequence confirmation; raw details optional | Authoritative provenance/source/environment fields are missing; no compatibility guarantee |
| F-APL-09 | Dedicated History list/details; ordinary Cell/Session/date fields and validation; optional query syntax; per-notebook filters/scroll; unchanged source shown once; compare disclosure; return to editor; cursor request scope preserved | Existing search-all helper hydration is unbounded; offline/large partial-page qualification pending |
| F-APL-10 | Native/scaled geometry regressions; 44 logical-pixel Comfortable targets; compact shell and outline drawer; core controls remain reachable at 800 x 600 with 200% interface scale | Does not substitute for all requested DPI/platform runs |
| F-APL-11 | Instant destination/overlay changes and immediate native button states; reduced-motion preference retained; no new moving panels or gestures | OS reduced-motion detection and frame-time profiling pending; springs are not applicable to instant panels |
| F-APL-12 | Native accessible names, real disabled states, menu/dialog distinction, modal focus handling, keyboard command alternatives | NVDA and complete assistive-technology journey pending |
| F-APL-13 | Dedicated bounded Settings/History/Checkpoints compositions; content-sized buttons; compact Commands footer; element-owned tooltips; notebook mode status scoped to Notebook | Full advanced-action/parity and tooltip/platform qualification pending |

Interface scale is stored in preferences schema 2. Loading schema 1 preserves
existing layout fields and migrates safely; invalid scale is normalized and a
future unknown schema is refused. Runtime source formats and protocols were not
changed by this work.

## Verification

| Check | Result | Evidence |
| --- | --- | --- |
| Locked Rust workspace tests | 32 passed: client 8, models 6, UI 18 | [rust-tests.log](../../artifacts/frontend-v6/rust-tests.log) |
| Strict Clippy, workspace/all targets | Passed with warnings denied | [clippy.log](../../artifacts/frontend-v6/clippy.log) |
| Locked native debug build | Passed | [build.log](../../artifacts/frontend-v6/build.log) |
| Python suite | 86 passed, 43 skipped | [python-tests.log](../../artifacts/frontend-v6/python-tests.log) |
| Real Windows native app | Populated notebook, edited/saved/executed source, recorded History, checkpoint creation/report and actual restore into a new session | [native evidence](../../artifacts/frontend-v6/native/README.md) |

The UI tests cover keyboard run bindings and composition guards, dialog focus and
deletion, Commands, history date validation and query preservation, session cursor
scope, return-to-editor, structural command ids/revisions, theme/syntax contrast,
Comfortable hit targets, ten outline resize cycles, and core control geometry at
four widths and four interface scales. These are meaningful regressions, not
claims that real Windows IME or NVDA passed.

The Python suite initially lacked Windows timezone data. Installing `tzdata` in
the existing local virtual environment allowed the full rerun to pass. No project
dependency manifest was changed. Optional/platform tests remain skipped.

The final native `release-*` captures use the real GPUI executable, an isolated
temporary project/profile and the Python service. The edit/run journey persisted
`# v6 verified edit` and recorded Mean: 16.5 as execution 4. The final restore
returned session `024ac0e2-fd4e-476f-8aaa-cfc800b153b4`; its banner uses that result,
not a synthetic UI state. UI Automation reports idle Interrupt, Markdown Run and
untrusted Restore as disabled, and Copy as a nonfocusable status acknowledgment.

Native display: 2880 x 1800 physical pixels, 192 DPI (200%). Measured clients include
800 x 600, 1024 x 768 and 1280 x 800 logical pixels. The requested 1440 x 900
client was constrained by Windows to 1440 x 879.5; it is not evidence for a full
900-pixel client. Interface scaling is additional to display DPI. Latest captures
include Light, Dark, High contrast, compact 200% interface size, Settings, Commands,
Help, Diagnostics and Runtimes. This is not an all-DPI or all-platform matrix.

At 200% interface size a menu scrolls and supporting settings extend below the
viewport. The toolkit can expose clipped descendants as `offscreen=false`, and
UIA Invoke on a clipped setting did not activate it. Resizing until it was visible
allowed activation. Native assistive-technology scrolling and offscreen reporting
remain an explicit accessibility gate; successful screenshots do not settle it.

## Acceptance scenario status

| Scenario | Evidence and remaining work |
| --- | --- |
| FS-42 | Implemented; native populated compact/wide notebook captures and Commands access |
| FS-43 | Implemented; short-source composition and automated source/selection/composition regressions; real IME/paste/virtualization qualification pending |
| FS-44 | Partial: automated width/scale/contrast and resize checks; native sampled sizes/themes; all DPI, mixed monitors and platform coverage pending |
| FS-45 | Partial: native run/copy/checkpoint/restore feedback; inherited model guards; drag-away and disconnect recovery qualification pending |
| FS-46 | Honest unavailable structural Undo; no unsupported successful undo claim; service inverse contract still required |
| FS-47 | Implemented common filters/detail/compare/return and cursor regressions; large/offline/partial history qualification pending |
| FS-48 | Native readable report, trust-disabled/enabled confirmation and successful new-session restore; authoritative preflight/failure matrix pending |
| FS-49 | Partial native keyboard/menu and automated focus semantics; NVDA, OS preferences and platform journey pending |
| FS-50 | Native dedicated compositions, disabled UIA semantics, ordinary report text and compact controls; full tooltip/assistive-technology qualification pending |

## Follow-up gates

1. Verify and document atomic structural inverses covering source, ids, order,
   kind, metadata, attachments and external revision conflicts before exposing
   structural Undo/Redo. Do not rewrite notebook files from a view to imitate it.
2. Add authoritative checkpoint provenance, creation context, source/environment
   compatibility and saved-count contracts before presenting verified preflight.
3. Bound helper-side history querying/pagination and complete inherited output,
   cache, 10,000-record and large-notebook performance gates.
4. Run NVDA, actual IME, 100/125/150/200% DPI and mixed-monitor tests, native
   keyboard/touch/pointer cancellation checks, and complete advanced/import/export,
   pin/publish, packaging and inherited parity gates.
5. Run the required participant studies, SUS and formal coverage/no-P1 release
   review. Publish a new authoritative frontend plan only through plan versioning.

## Updated apple-design heuristic

Same weights as the [baseline native review](apple-design-live-review-2026-10-09.md).
This is a provisional agent assessment of observed native behavior, not a user
study, external review or release score.

| Principle | Weight | Baseline | Current |
| --- | --- | --- | --- |
| Purpose | 12% | 6.0 | 7.5 |
| Agency | 14% | 5.5 | 7.0 |
| Responsibility | 12% | 6.5 | 7.5 |
| Familiarity | 10% | 6.0 | 7.5 |
| Flexibility | 12% | 5.5 | 7.0 |
| Simplicity | 14% | 3.5 | 7.5 |
| Craft | 18% | 3.5 | 7.0 |
| Delight | 8% | 3.0 | 6.5 |

Weighted overall: **7.2/10**, compared with **4.9/10**. Visual composition:
**7/10**, compared with **4/10**. The redundant framing, button stretch and
unchanged-source duplication are substantially improved. Recovery contracts,
large-history behavior and unqualified assistive-technology details still limit
confidence. The draft's 8/10 target and no-principle-below-7 target are not met.
