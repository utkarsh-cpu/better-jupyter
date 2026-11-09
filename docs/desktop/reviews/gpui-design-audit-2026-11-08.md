# KernelLedger native design and engineering audit

Follow-up: [implementation and updated qualification](gpui-plan-implementation-2026-11-08.md) supersedes this report’s remaining-work status. Baseline findings and captures below are retained as historical evidence.

Date: 8 November 2026. This report distinguishes source inspection, native screenshot evidence, automated regressions, and unqualified release gates. Final verification results are recorded below after implementation.

## Design review recorded before implementation

KernelLedger is a local Python notebook workbench for analysts and developers who need readable source, recorded executions, and explicit recovery. Preserve its continuous editing canvas, quiet slate chrome, cobalt actions, cell execution gutters, and separate History and Checkpoints destinations. Source and results deserve more emphasis than infrastructure metadata or assistant setup.

The installed Anthropic `frontend-design` skill was not found in the standard `.claude`, `.agents`, and `.codex` skill locations. The available `/Users/utkarsh_kaushik/.codex/skills/frontend-skill/SKILL.md` was read and applied for its app-specific restraint, hierarchy, utility copy, and limited palette. The user's native-desktop requirements take precedence over that skill's website imagery and motion defaults. No web UI is introduced.

**Visual thesis:** a precise scientific ledger with calm surfaces, readable source, and immediate feedback. **Content plan:** notebook identity → source/results → execution status → history/recovery → optional assistant context. **Interaction thesis:** immediate selection and execution feedback; short existing overlay fades only, respecting the existing Reduce motion setting; stable source positions while editing.

What works: a distinct KL mark, one primary Run all action, compact code gutters, actual execution states, platform caption reserves, virtualized notebook/palette, semantic theme tokens, bounded output renderers, deliberate destructive confirmations, and per-notebook drafts and scroll state. The current direction does not need a redesign.

What fails: a compact Assistant drawer obscures Settings controls; cached History survives new execution without refresh; macOS receives Linux font names and Control application shortcuts. Full selected Markdown frames are visually heavier than code. Success banners can linger across unrelated work. Startup failure can resemble a truly empty notebook. The last three require separate follow-up unless verified and corrected in this pass.

### Tokens and geometry

Retain six core roles; semantic danger/success/warning remain separate meaning-bearing colors.

| Role | Light | Dark | Use |
| --- | --- | --- | --- |
| Canvas | `#EEF1F5` | `#101317` | Source and quiet chrome |
| Ink | `#161B21` | `#ECEEF1` | Main text |
| Secondary ink | `#505A62` | `#A6B0B9` | Supporting labels |
| Muted surface | `#E2E6EC` | `#171B21` | Grouping/hover |
| Raised surface | `#F7F9FC` | `#1A1F25` | Menus and settings |
| Cobalt | `#0B5CAB` | `#6CB6FF` | Primary action/focus |

Keep the existing 16 px rem base scaled by 100–200% interface preferences, 14 px UI base, independent 12–24 px editor preference, medium control labels, semibold section labels, and monospaced source/identifiers. Use `.SystemUIFont` / Menlo on macOS; preserve Segoe UI / Consolas on Windows and Ubuntu / Ubuntu Mono on Linux. Native system font availability is preferable to shipping another typeface. Keep existing line-height behavior rather than globally changing code metrics.

Preserve the 4 px spacing vocabulary, 6 px controls / 8 px surfaces, 30 px minimum Compact targets and 44 px Comfortable targets, 180–300 px outline, and 800×600 logical minimum. `CellSpacing` already distinguishes intra-cell spacing from cell boundaries. Do not centralize content-specific measures into a universal gap or add offsets to disguise constraints.

Settings should own the available width. Suspend rendering of Assistant on Settings at all widths while retaining its draft/context/open state for return to the notebook. This avoids both compact obstruction and a compressed provider form on wide windows. Other destinations retain their current assistant behavior.

```text
Notebook:  [outline] | source + recorded output | [optional assistant]
Settings:            section navigation | full settings form
Return:    [outline] | same source position    | [same assistant draft]
```

History refresh must preserve the current list kind and request scope; it must not replace an inspected execution detail or discard filters merely to make data current. Tests should cover this boundary and Settings geometry at minimum/large interface sizes.

### Self-critique

Hiding an unrelated inspector on Settings improves task access without adding a new layout system. Native fonts and Command application actions support the host platform without changing notebook execution conventions. Refreshing the current History list improves correctness rather than visual taste. No new cards, gradients, dependencies, or ornamental motion are justified. Selection-frame reduction is a design recommendation, not a release-blocking defect; avoid changing it without a focused keyboard/focus review.

The following sections record the completed scoped audit, corrections, evidence, and remaining qualification gates.

## 1. Executive summary

The inspected workbench has a coherent product identity and a solid native foundation. This pass corrected obstructed Settings, stale History on navigation, clipped source continuation rows, misleading empty/source states, and Mac platform conventions. It preserved runtime protocols, notebook formats, execution behavior, and the existing palette. The result is materially better for everyday notebook work, but it is **not a qualified production release**. Helper startup recovery, accessibility, large-history behavior, and platform qualification still need work.

Evidence comes from a rebuilt development executable on macOS Apple Silicon, real OS keyboard/pointer events, window captures, pinned dependency source, and Cargo tests. Tests use disposable services; native execution uses a disposable notebook/profile. No personal notebook, credential, remote assistant request, restore commitment, or actual cell deletion was used. The full definition of done is not claimed.

## 2. UI/UX quality scorecard

Scores are design-review judgments of the inspected subset after correction, not measured compliance or a cross-platform average. Unverified release-wide categories remain unscored.

| Category | Score | Evidence / limit |
| --- | --- | --- |
| Overall visual aesthetics | 8/10 | Restrained canvas; selected Markdown and assistant setup remain heavy |
| Pixel-level alignment | 7/10 | Shared controls align; long connection status still crowds the header |
| Spacing consistency | 8/10 | Consistent cell rhythm and settings rows; dense provider form |
| Typography | 8/10 | Native Mac UI/mono families; independent editor size; long-source fix |
| Visual hierarchy | 7/10 | Source first; setup and persistent notices still compete |
| Component consistency | 8/10 | Shared button/menu/segment implementations; destructive confirmation still blue |
| Color and theme quality | 8/10 | Light/Dark/High contrast captured; contrast regressions pass |
| Desktop usability | 7/10 | Command actions, run conventions, native picker, canceled confirmation verified |
| Window resizing | 7/10 | Native 800–1440 widths; automated 1920 geometry; mixed DPI unqualified |
| Interaction quality | 7/10 | History refresh, settings round trip, menus, execution; several recovery journeys untested |
| Accessibility | Not Tested | Semantic/focus regressions exist; no VoiceOver/NVDA/Orca qualification |
| Rendering stability | Not Tested | Repeated native rendering observed; no sustained/frame-time/stress qualification |
| GPUI implementation quality | 7/10 | Pinned APIs and shared primitives; large Workspace and manual dispatch remain |
| Product-specific design identity | 8/10 | Scientific ledger, execution gutters, recorded history and explicit trust boundaries |

## 3. Discovery and application inventory

The Cargo workspace contains `app`, `ui`, `client`, `models`, `protocol`, `runtime`, and `daemon`. `app/src/main.rs` opens one 1280×850 window with an 800×600 logical minimum. Multiple notebooks live inside that window; detached windows are not implemented. GPUI is `gpui-pre = 0.3.7`; `gpui-kit = 0.7.0` reexports `gpui-component = 0.7.0`. The installed registry sources were consulted for TitleBar, fonts, EditorState wrapping, and Keystroke behavior. No HTML/CSS/browser solution or new dependency was introduced.

Windows x64 is the primary release target in the repository's platform plan. Linux is preview; macOS Apple Silicon is a development target; Intel Mac is unqualified. This run builds and inspects only Apple Silicon. Runtime support and release packaging must not be inferred from a screenshot.

`theme.rs` owns palette/font/radius/contrast configuration and `CellSpacing`; `components.rs` owns pointer targets, button variants, menus, tooltips, settings geometry, and row surfaces. `Workspace` owns per-notebook models, editor entities/subscriptions, inspection stacks, menu focus, and service event folding. Background services perform I/O. Notebook cells use `ListState`; palette commands use `uniform_list`. History search can still load all executions, and supporting record pages construct loaded rows eagerly. Large-data performance is not demonstrated by the small fixture.

### Screen and state inventory

| Surface | Accessible states identified in source | Evidence in this pass |
| --- | --- | --- |
| Welcome | First launch, recent notebooks, missing recent path, help, open/create/sample | Native returning-user welcome and open picker; others source/test review |
| Notebook | Loading, source unavailable, uninitialized folder, empty, populated; command/edit; compact/comfortable; outline/drawer | Native populated/empty/unavailable, all themes, narrow/wide; automated loading and scale/layout |
| Cell controls | Run, Markdown edit/preview, insert kinds, duplicate, move, split/merge, delete confirmation, conflict/draft actions | Native insert/save/canceled delete and run/error; existing automated structural regressions |
| Output | Text/result/error/traceback, pages, hidden, stale, tables, math, image cache/fit, media/attachment, widget snapshots | Native text and exception; other output types inspected in source and covered by existing unit tests where present |
| History | Executions/sessions, pending/applied filters, fields/dates, cursor/back, source comparison, return to cell, replay | Native empty/cached/refreshed; automated filters, scope, details preservation; no replay commitment |
| Checkpoints | Empty/list/detail; included/excluded values; restore/continue confirmation; advanced training/native coverage | Native empty/list/coverage; no restore/continue commitment; advanced paths source only |
| Settings | Appearance, editor/UI size, layout/motion/outline, runtime/interpreter, assistant/provider/privacy | Native appearance/provider, all themes, 200%; other sections source/automated coverage |
| Assistant | Unconfigured/testing/ready/error; conversation/search, composer, selected context/receipt, streaming answer/citations, proposals/apply, recovery | Native unconfigured composer and settings round trip; configured flows source/test review only |
| Diagnostics | Health/loading/failure, operations, reconnect/reload, interpreter candidates, drafts, bundle review/import, export/restoration results | Native health/operations; recovery/import commitments not exercised |
| Runtimes | List, current worker state, open/stop, runtime menu/interpreter | Native list/menu; no stop/restart commitment through UI |
| Help / About | Searchable help, shortcut table, build identity, about dialog | Native Help and shortcut labels; About source only |
| Variables | Unrefreshed, passive list, bounded value preview, stale/generation mismatch | Native unrefreshed page; passive runtime inspection not fully exercised |
| Console | Editor, execute in shared session, history/output | Native empty editor; no console execution in this pass |
| Debugger / source preview | Disabled/paused/postmortem, stack/locals/stepping, breakpoints, recorded-source navigation/paging, file preview/insert | Native inactive debugger; other states source/existing automated regressions |
| Overlays / native dialogs | Commands, workspace/notebook/runtime/export/cell/insert menus; confirmation; attachment; native open/save | Native palette/export/open/delete; other overlays source/automated review |

This inventory identifies the application's reachable state families; it does not assert every combination was opened or tested.

## 4. Detailed defect register

### Confirmed functional and visual defects corrected

| ID / severity | Surface and observed defect | Root cause / source | Correction / verification |
| --- | --- | --- | --- |
| F1 / P1 | Settings at 800×600: Assistant covers controls and provider content | `lib.rs`, workbench center and compact absolute drawer render independently of destination | Settings owns full width while Assistant state remains retained. `assistant.rs` returns to Notebook when Open Assistant is invoked. Native before/final captures; geometry regression at 800/1024/1280/1440/1920 and 100/150/200% |
| F2 / P1 | Narrow source: continuation of a wrapped code line is cut by output boundary | `notebook.rs:editor`; `lib.rs:editor_height` counts source newlines, while Kit defaults to soft wrapping | Disable source soft wrap using pinned EditorState API; horizontal scroll preserves line geometry. Native narrow capture and long-source caret/scroll regression |
| F3 / P2 | History reports no runs after successful execution until Apply | `lib.rs:select_surface` returns cached inspections without rereading list | `history.rs:history_refresh_request` repeats current list method/page request on navigation. Preserve filters/session/cursor; leave details unchanged. Avoid accumulating Back entries for identical refreshes. Native cached→new rows without Apply; event/navigation regressions |
| F4 / P2 | Source startup failure looks like an empty, saved notebook with insertion controls | `lib.rs:empty_notebook`, outline and `runtime.rs:status_bar` infer emptiness solely from absent cells | Separate loading/unavailable/uninitialized/real empty states; remove blank source-state banner; show Cells unavailable / Source not loaded. Native failure injection with `/usr/bin/false` and real empty fixture; state regression. Helper discovery/restart itself remains open |
| F5 / P2 | Mac application commands advertise and require Control | `shortcuts.rs`, Workspace keybindings, Kbd/help/hints | Add initial workspace focus, native Command bindings, and central translation for existing application registry/labels, keeping Control aliases and execution chords. Native Command navigation/palette/open/insert/delete confirmation; startup focus fix verifies palette before any pointer click; automated save/checkpoint dispatch. Native checkpoint shortcut remains a qualification gap below |
| F6 / P2 | Mac requests Ubuntu / Ubuntu Mono | `theme.rs:apply_theme` used `cfg(not(windows))` | Native `.SystemUIFont` / Menlo on Mac; Windows/Linux unchanged. Native theme captures and geometry regressions |
| F7 / P2 | Mac font refinement initially truncates Checkpoints even in wide header | `shell.rs` fixed 96 px tabs and separate collapse budget | Share 112 px destination tab geometry with collapse arithmetic. Final native captures show full label; existing multi-scale chrome tests pass |
| F8 / P3 | Palette's Checkpoint wording differs from Create checkpoint button | `lib.rs:COMMANDS` | Use Create checkpoint. Build/test verified; button creation verified natively |

Source entry points: [workspace](../../../desktop/crates/ui/src/lib.rs), [notebook editor](../../../desktop/crates/ui/src/notebook.rs), [History](../../../desktop/crates/ui/src/history.rs), [theme](../../../desktop/crates/ui/src/theme.rs), [shortcuts](../../../desktop/crates/ui/src/shortcuts.rs), [shell](../../../desktop/crates/ui/src/shell.rs), [runtime chrome](../../../desktop/crates/ui/src/runtime.rs), [regressions](../../../desktop/crates/ui/src/tests.rs). Function names are used rather than stale line ranges.

### Confirmed remaining defects and design recommendations

| ID / severity / kind | Surface / evidence | Expected behavior and recommendation | Status / root cause |
| --- | --- | --- | --- |
| R1 / P1 / functional recovery | Fresh helper cannot start; source-unavailable capture | Specific helper/Python error, usable interpreter discovery and retry, distinct from execution disconnection | Presentation corrected; helper startup still requires a working installed runtime Python. `client/service.rs:Session::new/start`, UI generic transport mapping. No helper-discovery fix claimed |
| R2 / P2 / visual | Source-failure capture: Connection lost crowds Run all | Bound/truncate header status and retain full wording in hint/menu | Confirmed; fixed-width runtime container includes an intrinsically wider button label, `shell.rs:header` |
| R3 / P2 / usability recommendation | Provider form: many advanced options compete with model/endpoint/key setup | Group advanced inference/context settings behind an explicit secondary disclosure | Native provider screenshot; `assistant.rs:ai_settings_provider_group`; requires focused design change |
| R4 / P2 / visual semantics | Delete confirmation uses cobalt primary action | Use the shared danger treatment for destructive commitments while retaining Cancel-first focus | Native canceled-delete image; `accessibility.rs:confirm_overlay`; no behavior change made |
| R5 / P2 / capability gap | Structural undo unavailable and disclosed in confirmation | Reversible cell operations with stable IDs and transaction-aware history | Source and native copy confirm gap. Requires model/runtime work, not a spacing patch |
| R6 / P2 / scalability | Search executions can load every recorded execution | Bounded indexed search and virtualized large result sets | Source/help review; no stress qualification, `history.rs` and client all-pages search |
| R7 / P3 / design recommendation | Full selected Markdown fill/frame is heavier than neighboring code | Explore focus rail and quieter selection, preserving independent keyboard focus indication | Confirmed visual difference in all themes; not a functional defect, `notebook.rs:cell` |
| R8 / P3 / attention | Checkpoint success banner persists after navigation | Consider timed or destination-scoped success feedback; retain inspection access in Checkpoints | Native checkpoint then other destinations; `runtime.rs:banners` retains notice until dismissed |
| R9 / P3 / copy | Runtime settings says version becomes available after start even for an active worker | Explain unavailable metadata or show validated interpreter version | Existing earlier macOS evidence and current source; not requalified as a runtime metadata fix |

### Issues requiring further verification

- Native Command+Shift+S automation did not produce a checkpoint in one scripted attempt, although GPUI dispatch regression passes and pointer Create checkpoint saved successfully. Investigate platform event delivery/focus and verify with a physical keyboard; do not equate the unit test with a complete native shortcut pass.
- Checkpoint saved-value counts/compatibility summaries require runtime metadata verification. The detail honestly shows unknown counts rather than inventing coverage.
- VoiceOver reading order, actions, editor announcements, real Tab journeys and focus restoration need an assistive-technology pass. Earlier System Events inspection exposed limited app contents; that is not proof of accessibility compliance or unusability.
- Clipboard round trips, real IME/composition, native context-menu collision, double-click/drag behavior, reduced-motion OS integration, multi-monitor/mixed-DPI changes, long-running interruption/recovery, and large outputs/notebooks require live qualification.
- Advanced debugger, live widgets, assistant streaming/approved context/proposal application, imports and all export fidelities require their own fixture/configuration journeys. No service was configured or remote data sent here.

No P0 was observed in the tested subset. This does not certify that the untested states contain no P0/P1.

## 5. Screen-by-screen design critique

**Notebook:** The left outline, code run gutter, source, and recorded output form a recognizable analytical workspace. The notebook name remains meaningful and full paths are available in the switcher hint. Compact spacing avoids dashboard-card repetition. Selection frames on Markdown remain disproportionately prominent. Long source now stays on its original line and scrolls horizontally; this is an intentional code-editor behavior, not text loss. Failure color is paired with the Failed label and exception text.

**Settings:** Full width resolves the confirmed obstruction. Appearance segments and provider labels align, long endpoint controls remain accessible, and vertical scrolling handles content at 200%. At that scale the first setting extends below the viewport; this is expected scrollable content, not proof every control is visible simultaneously. Advanced provider fields still make the first setup task unnecessarily dense. Open Assistant now returns to the notebook with the same context/draft.

**History:** Rows put source, outcome, run time, duration, and View in aligned columns. Technical details stay secondary. Reopening lists now reads current records without requiring Apply. Existing filters and detailed inspection state remain intact. Large histories need bounded loading; fixture success does not establish scalability.

**Checkpoints:** Empty-state guidance and one clear Create checkpoint action work. Pointer creation produced a saved banner and an inspectable row; the coverage report separates restore and continue consequences. The persistent success banner and unknown coverage metadata are the main remaining hierarchy issues. Restore/Continue were not committed.

**Assistant:** The composer/context-review distinction is clear and preserves an explicit send boundary. At compact widths it intentionally overlays source and has a close control; this remains secondary context rather than a second full editor. Its amber setup surface dominates the panel. Conversation/proposal/streaming states were inspected in source, not tested against a remote provider.

**Diagnostics and runtimes:** Operational controls are grouped and JSON is secondary, but duplicate health-check controls and technical wording create cognitive load. Runtime identity is a long filesystem path and deserves a stronger human-readable summary. “No issues reported” refers to that returned check, not whole-application health. Failed helper startup still lacks a direct in-place repair workflow.

**Help, variables, console, debugger:** Native Help now matches Command application actions and preserves Control execution. The shortcut table and long topic list need scrolling. Variables has an honest unrefreshed state; Console gives the editor one clear run-in-session action. Debugger shows an inactive session with disabled stepping controls; no stepping or postmortem functionality is inferred from this capture.

**Menus and dialogs:** Export options have clear format labels, the palette supports filtering and Escape, and deletion clearly discloses structural undo limitations. The blue destructive commitment is an avoidable semantic inconsistency. Native Open opens a directory picker and cancels without replacing the notebook. Save/export completion are covered by earlier macOS evidence, not repeated as part of this fix pass.

## 6. Design direction and token review

The pre-implementation token review above remains the chosen direction. No palette redesign, decorative gradient, extra icon system, new animation, or blanket card treatment is warranted. Preserve semantic warning/danger colors and separate selected/focused/error meanings. Existing high-contrast white/yellow boundaries are functional and intentionally stronger than normal-theme hairlines.

Use one shared geometry value in both layout prediction and rendering; the destination-tab correction demonstrates why. Retain code-specific line-height and source-size controls. The new Mac font choice improves platform familiarity without making typography depend on Linux fonts being installed. No font-availability certification is claimed for other machines.

## 7. Implementation summary

Changed Rust modules:

- `lib.rs`: source-state presentation, initial keyboard focus, Command actions, History refresh routing and Back-stack guard, Settings/Assistant composition, consistent checkpoint command label.
- `notebook.rs`: source wrapping policy aligned with source-line height and initial caret at the leading edge.
- `history.rs`: scoped cached-list refresh request.
- `assistant.rs`: native review selector and functional Settings → Open Assistant return.
- `shell.rs`: shared destination-tab width/collapse budget.
- `theme.rs`: macOS font branch.
- `shortcuts.rs`, `components.rs`, `help.rs`: platform binding/display conversion, preserving Control execution and aliases.
- `settings.rs`: geometry inspection selector.
- `runtime.rs`: accurate unloaded-source status and suppression of blank source-state banner.
- `tests.rs`: five new regressions for Settings geometry/return, source states, long-source scrolling, History freshness/detail preservation, and platform shortcut dispatch.

Documentation updates: keyboard conventions/horizontal source scrolling, platform font notes, review index, this report, and curated native evidence. No runtime business logic, data schemas, Cargo dependencies, or application packaging changed.

## 8. Before-and-after verification

[Evidence directory and capture notes](../../../artifacts/gpui-design-audit-2026-11-08/README.md).

| Finding | Before | After | Observed result |
| --- | --- | --- | --- |
| Settings obstruction | [800 px baseline](../../../artifacts/gpui-design-audit-2026-11-08/before-settings-800.png) | [800 px final](../../../artifacts/gpui-design-audit-2026-11-08/final-settings-800.png), [provider form](../../../artifacts/gpui-design-audit-2026-11-08/final-provider-settings-800.png) | Settings uses the available width; assistant restores through its action |
| Stale History | [empty despite execution](../../../artifacts/gpui-design-audit-2026-11-08/before-history-stale.png) | [refreshed on navigation](../../../artifacts/gpui-design-audit-2026-11-08/after-history-refreshed.png) | New run rows appear without Apply |
| Wrapped source clipping | [narrow/wrapped source](../../../artifacts/gpui-design-audit-2026-11-08/after-notebook-800x600.png) | [final narrow source](../../../artifacts/gpui-design-audit-2026-11-08/final-notebook-800x600.png) | Source uses horizontal scroll; no half-visible continuation row |
| Unknown source shown empty | Earlier macOS review and original source branch | [unavailable](../../../artifacts/gpui-design-audit-2026-11-08/final-source-unavailable.png), [real empty](../../../artifacts/gpui-design-audit-2026-11-08/final-empty-notebook.png) | Unread source is explicitly distinct from a valid notebook containing zero cells |
| Mac typography / navigation | [baseline](../../../artifacts/gpui-design-audit-2026-11-08/before-notebook-dark.png) | [final notebook](../../../artifacts/gpui-design-audit-2026-11-08/final-notebook-dark.png), [Help](../../../artifacts/gpui-design-audit-2026-11-08/after-help.png) | Native faces and platform labels; Checkpoints fits |

Baseline captures include compositor shadow padding; later captures omit it. Compare content regions, not raw image edges. The fixture gained cells/executions during interaction checks, so these are behavior comparisons rather than identical-data pixel diffs. `after-*` images are intermediate post-change evidence; `final-*` images supersede them for final appearance. Failed automation captures are explicitly excluded in the evidence notes.

## 9. Regression results and platform limits

`cargo test --locked --workspace`: **134 passed**, zero failed (client 31, models 7, protocol 3, runtime 23, UI 70). The new tests add real event/state/layout checks rather than snapshotting implementation constants. Existing tests cover palette/modal routing, composition suppression, filter validation, output paging, recorded source, destructive confirmations, cell structure, contrast, and multi-scale chrome. Passing simulated composition tests does not qualify a real IME.

`cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo build --locked -p kernelledger-desktop`, and `git diff --check`: passed. Used the locally installed pinned Rust 1.99.0 toolchain directly; the default PATH resolves an older compiler. The environment emits existing Cargo cache-permission and optional rust-objcopy/libLLVM stripping warnings plus the dependency future-compatibility notice for `block 0.1.6`. Builds/tests still exit successfully; no toolchain or dependency workaround was committed.

| Requested logical window | Native actual | Evidence / qualification |
| --- | --- | --- |
| 800×600 | 800×600 | Notebook and full-width Settings/provider captures |
| 1024×768 | 1024×768 | Notebook capture |
| 1280×800 | 1280×800 | Main surfaces, themes, dialogs |
| 1440×900 | 1440×900 | Notebook capture |
| 1920×1080 | 1512×950 | Host clamps to display; **native 1920 not tested**. Automated 1920 geometry passed |

Native PNG dimensions for shadowless captures are twice the logical window dimensions on this display. This is one 2× capture configuration, not proof of arbitrary scaling or mixed-monitor behavior. Window metadata is saved separately. Automated geometry checks include 100/150/200% settings widths through 1920; existing chrome tests include 100/125/150/200% and Compact/Comfortable. Light/Dark/High contrast and 200% Settings have real captures. System-following and reduced-motion OS changes were inspected in code but not fully exercised.

No Python/runtime tests were rerun because no Python/runtime implementation changed. Their absence must not be described as a full runtime qualification. Live fixture runs cover UI execution and recorded errors only.

## 10. Final design critique and production readiness

The application has a recognizable, professional scientific-workbench identity. The ledger metaphor is expressed through source, execution gutters, history and recovery rather than decorative branding. This pass improved correctness and native familiarity while retaining established workflow and visual conventions.

It still **requires significant refinement before production qualification**. The corrected flows are ready for development use, but helper recovery, semantic destructive treatment, large-history limits, complete accessibility/keyboard journeys, real IME/clipboard, mixed DPI, sustained rendering/performance, configured assistant/advanced interactivity, cross-platform regression, and signed packaging remain open. Selected Markdown treatment and provider setup complexity are smaller design follow-ups. The report's scores describe the inspected interface; they do not override these release gates or claim the full definition of done has been met.
