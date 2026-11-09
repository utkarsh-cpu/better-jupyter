# KernelLedger native UI refinement — implementation and qualification

**Update:** [Resumed native qualification](gpui-native-followup-2026-11-08.md) closes the screen-lock-related live checks, records two additional fixes, and supersedes the live-check status and Rust test count below.

8 November 2026. Follow-up to the [native application audit](gpui-design-audit-2026-11-08.md), implementing the agreed recovery, keyboard, component, workflow, history, undo, and qualification plan. The earlier report contains the application inventory, baseline screenshots, supported-platform assessment, and design review recorded before edits. This report describes the resulting implementation and its practical verification limits.

## 1. Executive summary

The application retains its scientific-workbench identity: continuous source, recorded results, quiet slate surfaces, and cobalt actions. The implementation corrects startup recovery, strengthens keyboard dispatch and modal focus restoration, bounds History search, adds persistent guarded structural undo, reduces provider setup complexity, and refines selection and semantic action treatment. All changes use the installed Rust/GPUI APIs; no browser UI or new dependency was introduced.

The installed Anthropic `frontend-design` skill was unavailable. The installed `frontend-skill/SKILL.md` was read and applied, together with the user's native-desktop design principles. This is not a claim that Anthropic's skill was loaded.

The application is suitable for continued development use. **Production release still requires platform and accessibility qualification.** The local automated suite passes. Native screenshots and input checks verify a useful subset of the changed flows; the desktop subsequently locked, preventing further live capture and interaction. Unfinished live checks are identified below rather than inferred from unit tests.

## 2. UI/UX quality scorecard

Scores apply to the inspected workbench and the earlier audit's native theme/size evidence. They are design judgments, not measured compliance or release certification. “Not Tested” means the full category lacks sufficient verification.

| Category | Score | Basis and limit |
| --- | --- | --- |
| Overall visual aesthetics | 8/10 | Continuous notebook, restrained chrome, compact Settings screenshots |
| Pixel-level alignment | 8/10 | Native source, navigation, controls; automated chrome geometry |
| Spacing consistency | 8/10 | Existing shared spacing/component system retained |
| Typography | 8/10 | Native system UI font, Menlo source, readable hierarchy |
| Visual hierarchy | 8/10 | Source dominates; selected Markdown uses a rail; advanced setup is collapsed |
| Component consistency | 8/10 | Shared buttons, segments, status tones and destructive variants |
| Color and theme quality | 8/10 | Existing light/dark/contrast evidence and semantic-token checks |
| Desktop usability | 7/10 | Run, navigation, dialogs and checkpoint shortcut subset verified |
| Window resizing | 8/10 | Earlier native size matrix plus current automated layout checks; full revised matrix not repeated |
| Interaction quality | 8/10 | Modal routing, undo guards, filter validation and targeted native interaction |
| Accessibility | Not Tested | No VoiceOver/NVDA/Orca qualification or real IME pass |
| Rendering stability | Not Tested | Screenshots and tests do not establish sustained performance/stability |
| GPUI implementation quality | 8/10 | Workspace tests, Clippy, shared components and bounded background work |
| Product-specific design identity | 9/10 | Source/results/recovery hierarchy remains specific to a scientific notebook |

## 3. Severity-ranked defect register

### Confirmed functional defects corrected

| Severity / location | Before and root cause | Correction / source | Evidence and status |
| --- | --- | --- | --- |
| P1 · startup / unavailable source | A fixed application Python could fail to import KernelLedger, with no in-app interpreter recovery. Worker loss and helper failure were easily confused. | Background environment discovery, explicit override, bounded hello handshake and child cleanup; Retry loading / Choose application Python; retain detailed failure. `client/src/lib.rs`, `client/src/service.rs`, `ui/src/lib.rs`, `ui/src/settings.rs` | Automatic discovery loaded the native notebook without `KL_RUNTIME_PYTHON`. Replacement/error handling passes a GPUI test including the file chooser, preserved edits, preserved worker session and subsequent recovery status. Live failure-to-recovery picker flow remains unverified. |
| P1 · History / Search executions | Search could fetch every execution and its source before applying local filters. Work and memory grew with the ledger. | Service-side keyset paging; indexed metadata filters; at most 100 candidate rows per request; source read/preview limits. Python `desktop/history.py`, `desktop/helper.py`, `ledger.py`; Rust `runtime/src/helper.rs`, generated `schema.sql`; `ui/src/history.rs`, `ui/src/lib.rs` | Python/native parity finds a match after two empty 100-row pages, including a match beyond the preview. Native first empty search page shows Next page guidance. Full native multi-page journey not completed before screen lock. |
| P2 · cell structure | Delete/merge and other structural changes lacked a source undo path. Editor undo could not restore stable IDs, order or attachments. | Shared on-disk bounded patches; hash/revision guards and existing publication journal; command-mode Undo/Redo with editor text undo preserved. Python `source.py`; Rust `runtime/src/source.rs`, `ui/src/lib.rs`, `ui/src/shortcuts.rs` | Exact byte/ID restoration across add, duplicate, delete, move, split, merge and convert; attachment restoration; newer-edit rejection; unrelated-edit preservation; Python/native interoperability; corrupt-index recovery, oversize limits and canonical-path validation pass. Keyboard dispatch passes GPUI tests. Native physical-keyboard structural undo remains unqualified. |
| P2 · shifted commands / native dialogs | Application dispatch relied on an early interceptor; uppercase shifted keys and focus after a canceled Open dialog needed qualification. | Native action bindings plus existing interception, case-insensitive key matching, platform-modifier suppression during composition/modals, restored prior focus after Open and Python chooser. `ui/src/shortcuts.rs`, `ui/src/lib.rs` | Shifted dispatch/modal unit checks pass. Native CGEvent Command+Shift+S created a checkpoint, then created another after canceling Open. This verifies native posted events, not a physical keyboard or real IME. |
| P2 · Runtime settings | Version formatting expected the validation API's `version`, while worker status reports `python`. That produced unavailable text for an idle worker. | Read string/array validation versions and worker `python`; do not treat the version as an executable path. `ui/src/settings.rs` | Worker hello/server contract inspected; final Rust checks pass. The saved Runtime screenshot predates the last fallback correction; refreshed screenshot unavailable after screen lock. |
| P2 · checkpoint coverage | Missing metadata was always presented as unknown even when it could be read from the saved manifest. | Report timestamp, originating session and saved-value count through Python/native checkpoint descriptors; preserve “unknown” for genuinely absent counts. `sharing.py`, `runtime/src/helper.rs`, `ui/src/checkpoints.rs` | Helper parity and automated checks pass. No complete native Restore/Continue or compatibility qualification is claimed. |

### Confirmed visual defects corrected

| Severity / location | Before | Correction / source | Evidence and status |
| --- | --- | --- | --- |
| P2 · toolbar | Long runtime labels could compete with Run all's layout slot. | Explicit bounded runtime-label width inside its existing slot; truncation remains intentional. `ui/src/shell.rs` | Narrow native header and automated geometry checks. |
| P2 · destructive controls | Shared action styling omitted delete, merge and training restore from its destructive branch. | Use shared danger semantics for those actions. `ui/src/accessibility.rs` | Code/build verification; prior dialog screenshots establish composition. Revised native destructive-state capture remains pending. |
| P3 · selected Markdown | Full fill/frame made a selected preview heavier than neighboring code. | Selection rail for ordinary preview; full editor focus/contrast treatment remains meaningful. `ui/src/notebook.rs` | Native automatic-discovery notebook screenshot shows the rail. |
| P3 · success feedback | A saved-checkpoint banner persisted across unrelated destinations. | Clear checkpoint notice when selecting another surface. `ui/src/lib.rs` | Native checkpoint → History captures show scoped feedback. |

### Design improvement recommendations implemented

Provider setup now presents Model, Base URL, provider save actions and API key before optional coding-model, inference, schema and request-limit controls. “Advanced settings” reveals those controls without changing their saved payload. “Test saved connection” and its hint identify that saved configuration is tested. The compact native screenshot verifies the basic form; the expanded advanced form needs a later live pass. Source: `ui/src/assistant.rs`.

### Further verification and known limits

- P1 qualification gate: live helper failure → choose valid Python → restored source, including cancellation and failed selection, still needs a native pass. Automated chooser/error/state coverage is passing.
- P1 release gate: real screen readers, IME composition, full Tab journeys, mixed DPI/multiple monitors, and Windows/Linux hardware qualification remain open.
- P2 search tradeoff: free text scans candidate pages, not a full-text index. A page may contain no matches and still offer Next page. Sources above 1 MiB or failing integrity/availability checks are flagged and omitted from source-text matching. Metadata filters can narrow the search. Returned source previews are at most 1 KiB.
- P2 undo tradeoff: at most 20 operations and 16 MiB of referenced patches, with a 4 MiB encoded patch limit per operation. Oversize operations clear structural history. A crash between history-index publication and source publication can strand a guarded entry; it cannot authorize overwriting source against an unknown base. Normal source editing recovers a corrupt optional history index. Both backends reject path aliases (dot segments, repeated separators and backslashes) before applying patches, so they cannot bypass the affected-file guards. Undo does not undo executions or external side effects.
- Physical Command+Shift+S delivery was inconsistent through System Events on this host. Direct native CGEvent delivery passed. No operating-system shortcut settings were changed.
- The later screen-lock failures are host limitations, not evidence of an application layout or accessibility defect. Captures after the lock were excluded.

## 4. Screen-by-screen analysis

| Screen / state | Result |
| --- | --- |
| Notebook / Markdown selection | Readable continuous canvas; rail preserves selection without framing the preview as a card. Long source remains horizontally scrollable. |
| Notebook / executed source | Native run-all executed two code cells; recorded mean and long-line output displayed. Existing execution/selection semantics retained. |
| Checkpoint success / canceled Open | Posted native shortcut saved checkpoints; cancellation returned to the application and another shortcut saved successfully. |
| History / populated page | One bounded page; aligned record content and explicit outcomes/times. |
| History / empty candidate page | Message distinguishes an empty search page from no recorded history; Next page remains available. Backend parity verifies later matches. |
| Settings / Runtime | Application Python is distinct from project interpreter. Choose/Retry is reachable in the compact layout. Final version fallback is code-verified after this screenshot. |
| Settings / Assistant | Basic provider fields are visible at 800 × 600; advanced controls are collapsed. Content continues below the viewport in the existing scroll container. Expanded state requires a fresh live pass. |
| Startup failure / recovery chooser | New actions and error separation are implemented and tested in GPUI. Revised rendered failure/recovery states were not captured before screen lock. |
| Help / Diagnostics / runtime menus / other surfaces | Existing audit inventory and screenshots remain the baseline. Help now documents discovery, search limits and structural undo. These surfaces were not all revisited in this follow-up. |

## 5. Design direction and token review

Preserve the precise scientific-ledger direction. The important hierarchy is notebook identity → source and results → execution state → History and recovery → optional assistant context. Do not add decorative gradients, shadows, animated entrances or blanket card treatment.

Six core roles remain unchanged:

| Token role | Light | Dark |
| --- | --- | --- |
| Canvas | `#EEF1F5` | `#101317` |
| Main ink | `#161B21` | `#ECEEF1` |
| Secondary ink | `#505A62` | `#A6B0B9` |
| Muted surface | `#E2E6EC` | `#171B21` |
| Raised surface | `#F7F9FC` | `#1A1F25` |
| Cobalt | `#0B5CAB` | `#6CB6FF` |

Semantic success, warning and danger retain their separate meanings. Preserve the existing 16 px rem base and shared component geometry; UI scale remains 100–200%, with independent editor size 12–24 px. macOS uses `.SystemUIFont` and Menlo; existing Windows/Linux face choices remain intact. Shared spacing and component methods are preferred to new offsets.

Self-critique: source recovery and guarded undo add necessary behavior, but they should not dominate the notebook. Provider settings still contain substantial expert functionality; progressive disclosure is an incremental improvement, not a claim that setup is effortless. Bounded scan paging prioritizes predictable resource use over a one-request global search. A future indexed text search should preserve these limits and improve discoverability rather than silently loading everything again.

## 6. Implementation summary

- **Client:** `lib.rs` bounds startup handshake and diagnostic reads, terminates failed children; `service.rs` discovers/selects application Python on its owning thread without resubmitting operations; `assistant.rs` switches subsequent assistant transport to the selected Python.
- **Runtime/source:** `source.rs` and Python `source.py` share the persistent guarded structural-history format and use existing source transaction publication. No change to execution history or imported-trust rules.
- **History:** new Python `desktop/history.py`, matching Rust `helper.rs` search path, metadata indexes in `ledger.py` and regenerated `schema.sql`; UI requests and pager preserve the query/scope. Maximum 100 rows keeps the existing record renderer bounded without adding a new list architecture.
- **UI:** shared shortcut actions and focus guards; bounded toolbar status; danger styling; Markdown selection rail; provider advanced disclosure; interpreter metadata and checkpoint counts; scoped success notice. Earlier audit corrections remain in the same working tree.
- **Tests/docs:** source-history, search bounds/parity, recovery-state and keyboard regressions; updated Help and keyboard documentation. No dependency, packaging, CI secret, system setting, personal notebook, commit or PR change was introduced.

## 7. Before-and-after verification

See the [curated native evidence](../../../artifacts/gpui-plan-implementation-2026-11-08/README.md) and the earlier audit's [baseline evidence](../../../artifacts/gpui-design-audit-2026-11-08/README.md).

| Change | Evidence |
| --- | --- |
| Quieter Markdown selection and automatic helper discovery | `01-auto-discovery-notebook.png` |
| Native checkpoint shortcut | `11-final-checkpoint-shortcut.png` |
| Shortcut after canceling native Open | `12-final-after-open-cancel.png` plus disposable ledger count increase from 2 to 3 |
| Bounded History / empty search candidates | `13-final-history-page-one.png`, `14-final-history-empty-candidates.png` |
| Application Python / project interpreter separation | `16-final-runtime-settings-800.png` (before final version fallback correction) |
| Simplified basic provider form | `17-final-provider-basic-800.png` |

Screenshots are unmodified native captures at 2× backing resolution. They are not generated mockups. Earlier captures measured 1280 × 800 and 800 × 600 logical windows; PNG dimensions are twice those values. The previous audit records other logical sizes and the host's clamping of 1920 × 1080. This follow-up does not claim a fresh complete size/DPI matrix.

## 8. Regression results

Final command transcripts are in the evidence directory.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Pass |
| `cargo test --locked --workspace` | 138 passed: client 31, models 7, protocol 3, runtime 24, UI 73 |
| Desktop + daemon debug build | Pass |
| Python `pytest -q` | 302 passed, 142 skipped; optional integrations/hardware capabilities are not qualified by skipped tests |
| Ruff source/test/example checks and changed-file formatting | Pass |
| Pyright with the project venv | 0 errors, 0 warnings |
| Native startup without Python override | Pass; source loaded from discovered project environment |
| Native run-all and checkpoint events | Two code-cell executions; checkpoint shortcut and post-dialog shortcut passed with direct native events |
| Native bounded-search message / compact provider form | Captured and inspected |
| Full changed-theme/size matrix, live recovery picker, expanded provider state, real IME/clipboard and screen-reader journeys | Not completed; desktop locked before remaining live checks |

The local Rust toolchain reports existing nonfatal warnings about optional debug stripping (`rust-objcopy` lacks its LLVM library), Cargo cache permissions, and the `block` crate's future compatibility. These are environmental/dependency observations, not suppressed application errors. No release or signed package was produced.

## 9. Final design critique

The workbench is coherent and recognizable. Continuous cells, stable gutters, source-first typography, slate surfaces and limited cobalt emphasis suit analysts and developers better than a generic card dashboard. The strongest improvement is making recovery and recorded history predictable while preserving that composition. The basic provider form is easier to scan; its expanded state still deserves hands-on review. The remaining risk is incomplete platform qualification, not a need for decorative redesign.

## 10. Production readiness assessment

**Requires further qualification before production release.** The scoped implementation is complete and locally checked. Release sign-off still needs the live recovery and expanded-provider passes, physical keyboard/IME/clipboard journeys, assistive technology, sustained large-data rendering, mixed DPI, target-platform hardware, and signed/distributable packages. The complete definition of done is not certified by this report.
