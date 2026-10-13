# Implementation coverage

This is a development build, not an F6-qualified release. The plans remain the
specifications. Compilation and headless tests do not prove graphics, accessibility,
performance or clean-machine packaging gates.

The [frontend v6 implementation ledger](frontend-v6.md) records the current
clarity, scaling, History and checkpoint work, native evidence, and unresolved
contract/platform gates. It does not qualify a release. The
[frontend v5 implementation ledger](frontend-v5.md) records the preceding
workbench redesign, new export/output/settings interactions, regression evidence,
and remaining work. Older versioned verification sections below remain historical
evidence and do not qualify the redesigned application.

| Area | Implemented | Remaining work |
| --- | --- | --- |
| Source | Locked revisions, journal recovery, canonical files, staged CLI converters, structural service actions | Exhaustive process/power-loss crash matrix; clipboard attachment UI and structural undo |
| Ledger | New schema 2 ledgers; explicit migration and verified schema 1 backup; operations, batches, changes, inputs | Large-history qualification |
| Protocol | Authenticated separate listener; roles; bounded frames; durable keys; expected source hashes; committed feed; inspection; verified blobs | Large-response blob indirection; bounded SQL pagination; subscriber stress qualification |
| Interpreter/input | Launch-aware private fallback refusal; candidates/validation; claims/handoff; getpass redaction; replay refusal | Clean-machine private distribution and composed input accessibility |
| Editor | Tabs; virtualized cells; lazy Python editors; autosave; drafts; conflict views; Markdown/raw | F0 virtualized selection/undo/IME retention; remaining command map; source compare; notebook replace; full external-change UX |
| Runtime UI | Save before run; batches; interrupt/kill; checkpoint; namespace-changing confirmation; active-operation cancellation | Composed queue view; complete capability gating |
| Training (frontend plan 4) | Separate idle Training checkpoint action; offline snapshot list/coverage; trust confirmation; candidate fresh worker restore; native capability report | Native backend and hardware qualification; full draft adoption; manual desktop GPU/DPI/accessibility qualification |
| Outputs | Committed text/errors; verified PNG/JPEG; native simple-table extraction; JSON/Markdown; stale badges; hide/show | 100 MiB bounded paging; image cache eviction; attachment preview; MIME chooser/export; output search |
| Workflows | History; checkpoint reports/restore/Continue; replay; explicit trust/mismatch switches; operation recovery; runtime discovery | Automated preflight reports and typed-error recovery dialogs; pin/publish; interchange dialogs; global stop/quit management; detached windows |
| Settings | Persistent light/dark/system/high-contrast preferences; interpreter validation/selection | OS theme-switch qualification; full config fields; key rebinding; externalized strings |
| UX audit (frontend plan 3) | Edit-mode run variants; IME shortcut suppression; keyboard palette/lists; delete confirmation; safe modal focus and wrapping; 800 x 600 minimum and compact toolbar; Stopped reconciliation; readable records; surface data/selection/scroll and Back; measured cell reveal; history filters; persisted themes; initialization/sample/recent paths; Help/glossary/About; window identity/contextual UIA names; versioned scorecard | Qualification below; live rebinding-driven shortcut sheet and linked glossary/help destinations; private runtime packaging; full record presentation and pagination; structural undo |
| Release | Locked Windows executable; dependency license declarations; runtime/client/Windows-build CI definitions | CI execution; license-text redistribution review; private Python bundle; signed MSI; updates; accessibility, IME, GPU/DPI, performance and clean-machine qualification |

The frontend currently polls committed helper folds on its background service.
Rust transport supports subscriptions and followed executions, and Python tests
verify them. Application feed integration and overflow/reconnection measurements
remain work. Oversized responses fail with a typed `too_large` error instead of
silently truncating ledger data. Failed/conflicted saves retain drafts and block runs.

Dependency decisions: Kit tree-sitter Python; webview deferred; native simple table
text extraction with Styler CSS ignored; LaTeX source fallback; one helper per
notebook service; 500 ms autosave. The remaining F0/F6 decision and measurement
records are open. No platform is declared qualified by this document.

## Frontend v3 verification (2026-10-05)

The baseline [audit](../../artifacts/ui-ux-audit/REPORT.md) remains 51.7/100 with
88.5% weighted coverage. It has not been rescored. The versioned
[scorecard](ux-scorecard.md) contains 72 criteria, requirement/journey mappings,
weights totaling 100, and explicit NT scores until an assessment is performed.
No usability participant results have been invented.

| Scenario | Implementation and evidence | Qualification still required |
| --- | --- | --- |
| FS-20 | GPUI tests exercise all three run bindings in edit and command mode, unchanged source, plain newline and IME suppression | Actual Windows IME composition and virtualization retention |
| FS-21 | Palette filtering, arrow navigation, activation and focus-return tests; sidebar and record-list key handlers | Pointer-free traversal of every surface with NVDA |
| FS-22 | Named delete confirmation, safe Cancel, Escape leaves source unchanged | Failed-delete and retained metadata qualification; structural undo unavailable |
| FS-23 | Native minimum-size enforcement, compact toolbar/sidebar, wrapping dialogs and modal focus trap | Screenshots of every surface/dialog at all requested DPI levels and themes |
| FS-24 | Real service regression: sample creation, idle stop, busy stop, other-client stop, offline history and restart-on-run | Full graphical operation timing and focus qualification |
| FS-25 | Disposable Windows UIA probe verifies title, seven named navigation destinations/current state, contextual Run and rendered stdout | Complete repeated-control, keyboard and screen-reader audit |
| FS-26 | Appearance loaded before window construction and persisted off render thread; system theme/high contrast handling | Relaunch and live OS theme changes across the platform matrix |
| FS-27 | Initialize/create/sample actions use the CLI/source service; real service sample test; recent paths persisted | Complete first-use task study and clean-machine packaging |
| FS-28 | Isolated surface inspections, saved selection/scroll, visible Back/Alt+Left and combinable history filters | Full drilled-list interaction and large-history performance |
| FS-29 | Measured list reveal after acknowledged insertion/duplicate/split/move and outline selection | Large-output screenshots and focus checks |

Verification commands: Python suite (69 tests), Rust workspace (19 tests), strict
Clippy, locked Windows build, `scripts/desktop_lifecycle.py`,
`scripts/desktop_scorecard.py`, and `scripts/desktop_ui_probe.py`. Native probe
reports are under `artifacts/frontend-v3/`; their startup measurement is time to
visible window, not editor readiness. Screenshots are deliberately unclaimed:
GPU-backed native capture was not available in this environment.

F0/F6 platform qualification, NVDA, mixed DPI, actual IME, large-output/history
performance, expert rescore, formative/summative participant studies and packaged
private-runtime identity remain open. The implementation supplies regression and
scorecard infrastructure; those external measurements are required before any
release-quality score or qualification claim.
