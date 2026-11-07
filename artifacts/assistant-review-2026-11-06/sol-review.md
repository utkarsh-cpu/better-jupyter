# Sol independent review - KernelLedger assistant

GPT 6.1 Sol, inherited xhigh effort. 2026-11-06. Independent read-only review; no other reviewer output was read.

## Verdict

**UI/UX 6.1/10; backend 6.2/10.** UI arithmetic: 60.5 / 10 = 6.05 exactly; backend: 62.0 / 10 = 6.2 exactly. Display uses decimal half-up rounding (6.05 becomes 6.1). Both group judgments have medium confidence. Coverage: 10/10 dimensions each; no nulls/exclusions. Coverage within dimensions is incomplete.

Useful and reviewable assistant with strong separation of proposed changes from effects, but not broadly release-qualified. Configuration loses the demonstrated schema option, capability status is incorrect, response recovery is poor, answer export targets notebook content and evidence wording overstates verification. No demonstrated critical blocker warrants P0. First release slice: F01-F07 and F14 qualification gates. Follow-ups: F08-F13 and F15; these become gates if strict deadlines, all-inference cap enforcement, crash-safe application or accessibility conformance are promised.

## Evidence and method

Sol candidate Rust was read at `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop`; active Python/docs/tests at `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6`. This managed checkout was used only for review outputs; its older desktop code was not substituted.

Saved evidence: `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06`. Actually viewed with view_image: gemma-result.png, final.png, generating.png, no-thinking-result.png and gemma-preview-current.png. The last two failure/preview rasters expose raw escaped traceback receipts; generating.png is not an active-generation capture. The successful answer has clear prose/code hierarchy and a source comparison; final.png shows the answer/citations at another scroll position. The stale error is green and the incomplete-provider error clips at the captured width.

Read raw records before auditing report.md. Inspected UI/transport/backend excerpts, docs, test code, UIA snapshots and assistant_ui_probe.py. Applied the UI/UX Pro Max skill with native desktop judgment; its local error-clarity/recovery query returned relevant all-platform Error Recovery and Error Messages guidance. No mobile touch-target or web font-size/CSS prescription imposed.

Reviewer-run checks used AST-extracted definitions, synthetic data and sqlite3 :memory: only: (1) actual Store schema plus capability query reproduces no such column: value; (2) extracted status_summary given cached structured_output=false and reservations=12345 returns structured_output=true and used_today=0; (3) unchanged synthetic source plus invented model hash raises stale with source-changed/missing wording; (4) recomputed saved canonical receipt digest matches, recorded before/after hashes agree, executions remain five, no applications, four errors and one done. Initial check 4 failed on text decoding, then passed using JSON byte-encoding detection. No package/service construction, server or live store involved. Existing suites were inspected, not executed.

## Scores

### UI/UX

| Dimension | /10 | Confidence | Rationale |
|---|---:|---|---|
| 1. Provider onboarding and configuration | 5.5 | high | Setup states, presets, discovery and connection testing help; Save drops schema/HTTP exception options and schema/timeout/reasoning controls are absent. Settings visuals only partially evidenced. |
| 2. Readability and visual hierarchy | 7.5 | medium | Viewed light-theme answer has readable prose, code and clear cards. Escaped receipt JSON overwhelms the feed; long ids dominate citations and a captured error clips. No contrast measurements. |
| 3. Context review and destination clarity | 6.5 | high | Exact copied messages, destination/model metadata, badges and one-use receipts are strong. Raw serialized items are laborious to review; question/system messages are copy-only and omitted items are not counted. |
| 4. Conversation and task workflow | 5.5 | medium | Notebook tasks, context chips, history and search exist. Sends replace a single answer; historical text lacks citation actions; conversation switching leaves old answer/proposals; answer export calls notebook export. |
| 5. Generation feedback and cancellation UX | 5.5 | low | Code has busy text, disabled send, Cancel and unverified draft. Backend emits only after the complete answer JSON string; Cancel/status follow the receipt in the feed. No active-generation/cancellation visual evidence. |
| 6. Error clarity and recovery | 4 | high | Viewed stale failure is green; incomplete-provider error clips and lacks useful cause. Spent receipt remains sendable after error. Invented model hashes are presented as source changes. |
| 7. Evidence and citation UX | 6.5 | high | Saved UIA records support cell/execution navigation; unresolved chips are labeled and disabled. Unconditional Evidence-checked badge overstates target existence. Long ids and bottom chips do not map individual claims. |
| 8. Proposal review and user control | 7.5 | medium | Viewed current/proposed source and Apply/Reject/Regenerate are clear; no effects recorded. Hash gates are strong. One malformed proposal loses the explanation; generic Apply also represents runs; accepted/applied reconciliation is incomplete. |
| 9. Keyboard and accessibility support | 6 | low | Named native buttons/edits/status and explicit Invoke handlers for key actions are positive. UIA is not a keyboard/NVDA audit. Full answer group label may be cumbersome; general errors use Status; offscreen Apply/Cancel unqualified. |
| 10. Responsive layout and space use | 6 | medium | Wide capture keeps notebook/assistant adjacent and composer docked. Resizable panel and compact overlay exist in code. Expanded receipts consume feed space; a long error clips. Narrow/DPI/compact states untested. |

Exact mean: 60.5 / 10 = 6.05; half-up display 6.1/10. All 10 rated; no nulls. Per-dimension absolute references are retained in sol-scores.json. Confidence medium; no full behavioral certification.

### Backend

| Dimension | /10 | Confidence | Rationale |
|---|---:|---|---|
| 1. Provider and protocol compatibility | 6.5 | medium | Bounded Responses/chat SSE, retry limits and saved Lemonade success exist. Compatible servers must accept SSE, stream_options and max_tokens; no dialect negotiation/nonstream fallback. Four native failures precede one success. |
| 2. Schema support and model capability qualification | 4.5 | high | Closed local validation and optional schema enforcement are good. Trivial connection probe asserts broad JSON support; optimistic defaults and SQL column mismatch defeat observed capability status. No task/model quality qualification. |
| 3. Context grounding and citation validation | 7 | high | Explicit bounded redacted context, revisions and exact messages ground requests. Citation checks prove existence against current records, not receipt membership, revision stability or claim truth. Budget exhaustion silently omits later items. |
| 4. Proposal validation, staleness, and effect boundaries | 8 | medium | Closed schemas, hash checks, resolve-time checks, source contracts, frozen run hashes and copy-only commands form a strong boundary. No effects in evidence. Invalid proposals fail the whole response; accept/apply are separate unreconciled steps. |
| 5. Reasoning, token budgets, and deadlines | 5 | high | Context/output limits, conservative daily reservations and deadline checks exist. Reasoning/context-window qualification absent; connection probes bypass caps. Full-timeout blocking reads/retries can exceed total deadline. Reservations intentionally differ from actual usage. |
| 6. Streaming and cancellation handling | 6.5 | medium | Incrementally bounded SSE and complete-answer draft emission protect split secrets. Desktop kills isolated assistant; runtime polling remains separate. No token-by-token UX or active-cancel evidence; blocked-connect/read cancellation unqualified. |
| 7. Persistence and concurrency | 7 | medium | Separate SQLite, owner lock, transactions, interrupted recovery and retention exist. No effect/application reconciliation or duplicate application guard. Machine settings updates can race across notebooks. History has hard limits. |
| 8. Privacy, credential handling, and egress controls | 7.5 | medium | Read allowlist, inspect-only source, OS credential stdin, receipt checks, DNS pinning, no redirects and image omission are good. Redaction is best effort; local paths persist; legacy global/host approvals conflict with scoped wording/docs. No adversarial live qualification. |
| 9. Failure diagnostics and recovery | 4.5 | high | Fail-closed behavior suppresses unsafe provider bodies. Length/filter/tool-call failures collapse to one code; failed request usage/details lost, invalid proposal loses answer, receipt remains stale in UI, capability lookup exceptions swallowed. |
| 10. Test coverage and release readiness | 5.5 | medium | Substantive fake/local tests cover receipts, hashes, redaction, locks and exports; inspected, not rerun. Fake UI probe and helper-export tests missed native wiring/cache bugs. One fixture and confounded server/model settings do not qualify production, accessibility or applications. |

Exact mean: 62.0 / 10 = 6.2; half-up display 6.2/10. All 10 rated; no nulls. Per-dimension absolute references are retained in sol-scores.json. Confidence medium; no full behavioral certification.

## Prioritized backlog

Ordered first-release findings before follow-ups. S/M/L are relative engineering/qualification effort, not dates. Static risks are labeled; every change must preserve receipt binding, revision validation and explicit effects.

### F01 - Expose and preserve schema/provider settings (P1)

**Static application defect and missing control; first release; effort M; confidence high.**

**Trigger:** Save replaces providers[provider] with base_url alone. The successful recorded profile required structured_output=true set outside the form.

**Consequence:** An ordinary save silently removes the working schema mode or named HTTP exception; users cannot reproduce the demonstrated setup entirely in the form.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:2124`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:2134`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/providers.py:189`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/settings.py:79`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/report.md`.

**Change:** Merge validated provider fields; expose supported schema mode and preserve named HTTP exceptions. Show effective saved settings; do not blanket-enable schemas on incompatible servers.

**Acceptance:** Round-trip base_url, structured_output=true and allow_http_host through native Save/reopen and inspect mocked outgoing response_format. An unsupported schema endpoint receives an actionable capability result.

### F02 - Fix capability-cache reads and default to Unknown (P1)

**Reproduced backend defect; first release; effort S; confidence high.**

**Trigger:** status_summary selects value, but Store defines result; broad exception handler resets used_today=0. In-memory execution reproduces both consequences.

**Consequence:** Cached observations never hydrate; usage appears zero and optimistic streaming/structured defaults can appear qualified. Actual send-time Store.used enforcement remains intact.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:127`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:138`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:140`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:143`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/store.py:36`.

**Change:** Query result, separate usage/capability failure handling, safely surface diagnostics, default to unknown/unqualified and use effective task-model identity.

**Acceptance:** Cache structured_output=false and Store.used=12345: status must return false and 12345. Unprobed/expired/error status remains Unknown. Add probe-write/status-read round-trip coverage.

### F03 - Separate connectivity from task qualification and budget policy (P1)

**Model limitation plus qualification/control gap; first release; effort L; confidence high.**

**Trigger:** Tiny connected probe succeeds but four LFM Explain requests fail. Adapter has no reasoning option; separate 512-token diagnostic ends at length with no final content.

**Consequence:** Connection success suggests readiness for a task that cannot complete. Users cannot select an informed tested model/output/reasoning profile. The diagnostic does not prove every native failure mechanism.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/capabilities.py:12`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/lib.rs:891`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/providers.py:167`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/settings.py:24`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/results.json`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/raw-sse-diagnostic.json`.

**Change:** Call the probe SSE/JSON connectivity only. Qualify representative tasks per endpoint/model/schema/budget profile; add supported provider-specific reasoning controls or documented server instructions, context-window admission and distinct output-limit recovery.

**Acceptance:** Repeat Explain fixtures across supported profiles and report correct answers/citations/hashes with denominators and latency. Reasoning-heavy exhaustion offers safe supported choices. No hidden repairs, silent model switch or automatic model load.

### F04 - Preserve valid answers when optional proposals fail (P1)

**Observed failure mode and static validation design; first release; effort M; confidence high.**

**Trigger:** Every proposal is validated before finish; any failure marks the whole request error. Native stale requests have no saved assistant message. Invented hash with unchanged source reproduces stale.

**Consequence:** A usable explanation is lost because an optional edit is malformed; user is told their source changed. Native invalid payloads were not retained, so exact failing fields remain unknown.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:343`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:351`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:367`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/tools.py:195`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/results.json`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/no-thinking-result.png`.

**Change:** Persist the safe answer/citations independently with rejected-proposal warnings. Distinguish a target/hash absent from the approved context from a real subsequent source change; never silently substitute a hash.

**Acceptance:** Mixed valid/invalid proposals preserve answer and valid evidence; unsafe entries are inapplicable. Test invented id/hash, genuine post-preview edit, mixed proposals and malformed top-level JSON separately.

### F05 - Use typed errors, readable banners and fresh-receipt recovery (P1)

**Observed UI defect and static recovery defect; first release; effort M; confidence high.**

**Trigger:** Stale failure is green because severity uses string matching. After respond consumes receipt and errors, UI retains enabled Approve & send. Long provider error clips in saved PNG.

**Consequence:** Failure looks successful, guidance blames source and retry sends an invalid receipt. Error text cannot be fully read at the captured width.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1014`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1028`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1268`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/lib.rs:787`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:274`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/client/src/assistant.rs:144`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/generating.png`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/no-thinking-result.png`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/first-request-failed-uia.json`.

**Change:** Carry code/severity/retryability through Rust events; use wrapping error semantics, clear spent receipt and offer Rebuild context while preserving prompt/selection. Keep request busy/status independent of unrelated status/settings results.

**Acceptance:** At captured/minimum panel widths show full stale/timeout/budget/policy/invalid/cancelled errors. Spent receipts have no active Send; retry previews once and obtains required approval. Verify native error announcement.

### F06 - Export the answer from its PDF/Word buttons (P1)

**Static end-to-end wiring defect; first release; effort M; confidence high.**

**Trigger:** Answer buttons call export_dialog, which passes path/format without markdown. documents.export_document defaults to notebook_blocks when markdown is absent.

**Consequence:** Export from the answer saves notebook content instead of the visible report and may include unrelated notebook data the user did not intend to share.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1419`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1431`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/interchange.rs:89`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/interchange.rs:117`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/documents.py:388`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/tests/test_assistant.py:605`.

**Change:** Capture the reviewed answer and selected provenance at click time in an answer-specific export path; pass markdown explicitly, name it as a report and preserve verification limits.

**Acceptance:** Answer-only marker and notebook-only marker: exports from the answer in both formats contain the answer marker and exclude unrelated notebook marker. Tab switching/cancel preserves the originating answer.

### F07 - Describe target verification without implying factual truth (P1)

**Static trust UX defect; first release; effort M; confidence high.**

**Trigger:** All answer cards receive green Evidence-checked, even with zero/unverified citations. Validator checks existence in current records, not receipt inclusion or claim entailment.

**Consequence:** A structurally valid answer looks fact-checked; unrelated existing ids can receive verified=true.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1378`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1381`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:378`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:365`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/final.png`.

**Change:** Show verified-target counts and visible limits; distinguish no/partial/full resolvability. Track approved-context membership/revision separately, preserve navigation and hypothesis labels, and avoid factual guarantees.

**Acceptance:** Zero, mixed, unrelated-existing and fully resolvable citation fixtures never imply truth verification; unrelated ids are not approved-context evidence. Existing navigation remains functional.

### F14 - Gate release on failure-path and native qualification matrix (P1)

**Qualification gap, not proof all features fail; first release; effort L; confidence high.**

**Trigger:** One three-cell ZeroDivisionError native fixture yielded four failures/one success. Fake/local suites and UI probe do not establish native reliability or accessibility; tests missed cache hydration/export wiring.

**Consequence:** Twelve bespoke checks cannot substantiate production quality, effect recovery, model compatibility or full accessibility.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/results.json`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/checks.json`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/tests/test_assistant.py:497`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/tests/test_assistant.py:605`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/tests/test_assistant.py:690`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/scripts/assistant_ui_probe.py`.

**Change:** Add focused F01-F07 regressions, then a versioned provider/model/task matrix, application fault injection, keyboard-only/NVDA flows, narrow/DPI/theme cases, context omissions, policy/credentials and active cancellation. Publish bounded supported scope and untested areas.

**Acceptance:** Record fresh regression passes plus repeated task attempts with denominators and precise profiles. In disposable profiles qualify preview/send/apply/run separation, no unintended effects, keyboard/screen-reader recovery; document exclusions.

### F08 - Account for connection-test inference and label cap scope (P2)

**Static backend accounting gap; follow-up; effort M; confidence high.**

**Trigger:** providers.test calls probe/respond before the respond admission/reservation path and stores capabilities only. Store.used sums request reservations per notebook/provider.

**Consequence:** Repeated probes can consume paid inference beyond the configured cap. Failed requests remain charged by design; reserved tokens are not billing totals.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:244`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/capabilities.py:13`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:298`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/store.py:55`.

**Change:** Use common admission/accounting for all inference including tests. Keep sanitized known usage and finish reasons on failure; label notebook/provider scope, UTC reset and reserved/actual usage; retain conservative unknown-usage fallback.

**Acceptance:** Low cap blocks repeated probes before inference; success/failure/cancel probes record reservations and known usage. Multiple notebooks clearly show separate caps until machine-wide accounting exists.

### F09 - Bound total deadlines and qualify blocked cancellation (P2)

**Static robustness gap; no demonstrated live hang; follow-up; effort M; confidence medium.**

**Trigger:** Each retry/socket uses full timeout and elapsed check occurs before blocking readline. Desktop Cancel kills child and immediately sets local busy=false.

**Consequence:** Slow headers/lines/retries may exceed nominal total deadline; transport error may overwrite friendly cancellation state. No saved live cancellation result establishes actual behavior.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/providers.py:93`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/providers.py:99`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/providers.py:128`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/providers.py:204`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1063`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/client/src/assistant.rs:169`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/tests/test_assistant.py:522`.

**Change:** Carry monotonic deadline through connect, retries, headers and reads; bound remaining waits. Keep Cancel visible outside receipts; settle UI state using request/generation identity.

**Acceptance:** Isolated fake/mocked stalled DNS/connect/headers, trickled unterminated lines and retries stay within specified grace. Cancel reaches idle; late drafts/errors cannot overwrite a newer request. No live server needed.

### F10 - Reconcile proposal acceptance with application outcomes (P2)

**Static persistence/effect recovery gap; follow-up; effort L; confidence medium.**

**Trigger:** resolve commits accepted before desktop save/apply; application.record is later and lacks uniqueness. Crash/helper conflict can occur between stages.

**Consequence:** Accepted proposals may have no application; effects may lack audit records. Accepted styling can imply success after application failure. Saved fixture never applied anything.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:399`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:449`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/store.py:35`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/client/src/service.rs:727`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/client/src/service.rs:769`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/lib.rs:847`.

**Change:** Represent accepted/applying/applied/failed/unknown separately. Persist an idempotent application identity and reconcile manifest/runtime request ids after restart; never retry effects blindly.

**Acceptance:** Inject failures before effect, after effect and before record; restart shows truthful recoverable states without duplicate insertions/runs. Preserve revision/frozen-hash gates.

### F11 - Make receipts scannable and expose omitted selections (P2)

**Observed usability issue and static completeness gap; follow-up; effort M; confidence high.**

**Trigger:** Receipt renders raw JSON-escaped traceback/path strings; add silently returns when shared budget exhausted. Status/Cancel follow receipt in the feed.

**Consequence:** Review is laborious, later selected evidence can vanish without omitted counts, and status is far from the header. Long opaque ids stretch controls.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/gemma-preview-current.png`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/generating.png`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1178`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1238`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1984`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/context.py:41`.

**Change:** Summarize question, selected records, destination and model first; provide collapsible exact payload/system messages plus full copy. Show selected/included/omitted counts and truncation scope; short labels retain full values. Keep status/Cancel visible.

**Acceptance:** Over-budget selection lists every omitted/truncated item and preserves exact inspect/copy. Long traceback/id at 360px, high scale and compact overlay does not hide status/actions.

### F12 - Keep turns and historical evidence coherent (P2)

**Static conversation workflow defect; follow-up; effort M; confidence high.**

**Trigger:** Preview clears answer/proposals; respond replaces answer without appending transcript. conversation.get assigns text messages only; switching leaves previous answer/proposals. Historical rendering omits citation controls.

**Consequence:** Follow-up context exists in backend but continuity disappears visually; an old transcript can sit beside another conversation answer/proposals.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:298`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/lib.rs:815`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/lib.rs:866`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1898`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1910`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:249`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/store.py:134`.

**Change:** Maintain request-linked turns and scoped proposals/receipts, rehydrate navigable citation metadata, replace current answer/proposals on conversation change and disclose limits/pagination.

**Acceptance:** Two turns, two conversations and reload show only their own ordered messages, evidence, proposals and failed-request markers. Preview includes exactly the history shown as selected.

### F13 - Show and test the effective task model (P2)

**Static model-identity mismatch; follow-up; effort M; confidence high.**

**Trigger:** Backend selects task_models[task]; composer/header and status cache use config.model. UI exposes edit/refactor/tests override.

**Consequence:** Coding request can show default model until receipt; connection badge may qualify a different model than the task uses.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:54`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/context.py:146`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:139`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:900`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:399`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:2147`.

**Change:** Resolve effective model consistently for composer, receipt, capability cache and explicit tests; label active override and qualification scope.

**Acceptance:** Default A/coding B: edit/refactor/tests show B before preview, optionally test B, send B and retain receipt binding. Ask displays/uses A.

### F15 - Align approval scope, revocation and privacy documentation (P2)

**Static privacy-contract inconsistency; follow-up; effort M; confidence high.**

**Trigger:** Docs say automatic egress approvals disabled; backend accepts notebook+URL, notebook+host and global-host entries. UI offers Always allow and scoped revoke. Current UI send still explicitly approves each receipt.

**Consequence:** API/CLI grants may be broader than notebook/destination wording; revoking a scoped key may leave effective legacy grants. Consent model is unclear.

**Evidence:** `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/docs/assistant.md:26`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:74`; `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:289`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1295`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1320`; `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/ui/src/assistant.rs:1329`.

**Change:** Choose/document explicit consent semantics; scope new grants to notebook+normalized endpoint, migrate/reveal legacy scopes and revoke the effective grant. Do not claim current UI silently sends without a click.

**Acceptance:** In-memory configs for scoped URL, notebook-host and global-host entries reveal true scope; Revoke stops auto-approval for displayed destination. Docs/UI/API/CLI agree and exact-remote-receipt checks remain enforced.

## Audit of the parent test conclusions

**Supported for the fixture.** results.json records four LFM errors (two provider_error, two stale), then one Gemma done. Request timestamps round to 29.3 seconds, usage is 1,847 prompt + 882 completion = 2,729 tokens. The answer identifies the recorded ZeroDivisionError and the proposal has the correct base hash, two resolvable citation targets and state proposed. Before/after recorded notebook hashes agree, executions remain five, applications are empty. UIA supports execution-citation navigation to History and returning to the error cell. These are saved observations checked for internal consistency, not fresh live verification.

**Citation existence is not truth.** The cell and execution match this simple diagnosis, but validator existence does not prove all claims follow. Intentional testing/placeholder intent remains explicitly unverified. Replacing 0 with 1 removes this exception but can be wrong for the intended calculation.

**Configuration diagnoses confirmed.** The form lacks a structured-output control and Save drops that supported setting. This is an application defect independent of whether a particular model can emit valid JSON or choose correct targets. The capability-cache column defect is additional to the parent findings.

**Stale diagnosis needs qualification.** The validator and synthetic check show a wrong model id/hash can produce source-changed wording without source mutation. Native invalid proposal bodies are not retained in results.json, so the exact failing field in each attempt cannot be identified. Preserve the gate; do not blame source mutation or patch hashes without evidence.

**Reasoning diagnosis is suggestive, not complete native proof.** Separate raw-sse-diagnostic.json has 513 events, 512 completion tokens, 2,056 reasoning-content characters, no final content and finish_reason=length. Direct no-thinking diagnostic has allowed structure but mismatched citation kinds and a no-op 1 / 0 proposal. Neither is a native success. Native failures retain empty usage/generic errors, so the parent claim that the first native attempts reached exactly 2,048 tokens is not independently established by their persisted request records. No raw reasoning traces are reproduced.

**Model comparison is confounded.** Load records show LFM ctx_size 4,096 with reasoning budgets 0/1,024 and Gemma ctx_size 8,192 with budget 512; save_options=false confirms transient setup. Initial attempts had other settings. Receipt hash does not encode schema enforcement, output limit or server reasoning setup; equal LFM receipts do not imply identical HTTP bodies/inference settings. The successful model switch cannot establish causal model superiority or reliability.

**Twelve checks describe a narrow checklist.** Raw records support a completed answer, fixture-level diagnosis/hash/citations, no observed effects and navigation. checks.json is a parent-authored summary without an independently supplied evaluator for its quality booleans. It does not exercise Apply/Reject/Regenerate success, actual reruns, cancellation, network recovery, other errors/tasks, repeated success, keyboard traversal or screen-reader usability. All twelve passing is not general quality evidence.

**Documentation differs from code.** C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/docs/assistant.md:26 says automatic egress approvals disabled; UI/backend have scoped and legacy grants (F15). Line 43 describes answer export, while the native button passes no answer markdown (F06). Complete-answer-field draft emission matches the documented privacy choice; do not describe it as token-by-token native answer streaming. Per-notebook/provider UTC reservations are intentional conservative admission, not actual billing totals.

## Preserve implemented strengths

- Keep the read-only tool allowlist, inspect_only source access, separate assistant records and process, exact redacted receipts, destination/model rechecks and one-use authorization (`C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/tools.py:8`, `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant/service.py:274`).

- Keep remote approval checks, DNS pinning, HTTPS/named-host policy, OS-store credentials via stdin hello, bounded transport, image omission and suppression of raw provider error bodies. Redaction is best effort, so exact user review must remain available.

- Keep source-hash/resolve validation, atomic source transactions, frozen-hash stop-on-error run batches and copy-only commands (`C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop/crates/client/src/service.rs:727`). Preserve explicit Apply/Reject/Regenerate and before/after source; never automatically apply the model fix.

- Preserve selectable prose/code, citation navigation and unverified labels, docked composer, resizable inspector, notebook scope, tasks/search and durable request/application records. Improve these foundations without replacing the notebook workflow.

## Recommendations that could backfire for experts

- Auto-filling guessed hashes, remapping target ids or accepting stale source makes apparent success unsafe. Salvage valid explanations while rejecting unsafe proposals.

- Mobile-sized controls, compulsory 16px text/animations or a full-screen chat takeover harm dense notebook work. Keep scale/density preferences, keyboard operation and adjacent evidence review.

- Default repair/retry prompts, silent model switching/loading or unconditional provider-specific schema/reasoning parameters change cost/privacy/latency/compatibility. Prefer explicit tested profiles and fresh receipts where needed.

- Simplifying receipts must not remove exact payload/system-message access. Faster-looking streaming must not reveal raw JSON, split secrets or reasoning traces. Summaries plus inspectable exact data are preferable.

- Do not turn Evidence-checked into a factual guarantee or hide useful full code/ids permanently. Preserve hypotheses, selectable full values and provenance behind concise labels.

## Remaining qualification gaps

- Provider/model/task matrix with repeats and precise versions; multiple exception types/non-error tasks; source/checkpoint citations and unrelated existing ids; large UTF-8/truncated/omitted context; malformed/mixed proposals and refusal/length finishes.

- Native edit/insert acceptance/rejection/regeneration, frozen-hash run proposals and stopped/running runtime states; conflicts after acceptance, crashes between effect/record, duplicate application, owner recovery, retention/purge, long history and concurrent machine settings.

- Real generation/cancel/retry, focus restoration, keyboard-only review/copy/citations/apply, NVDA reading/announcements, offscreen Invoke, narrow/DPI/compact layouts, dark/high-contrast themes and measured contrast.

- OS-store errors/platform support, adversarial redaction/prompt injection, destination/DNS changes, legacy/global grant revocation, paid-probe accounting and UTC cap scope. Vision/inline/autonomous effects/plan sign-in/embeddings are intentional unavailable gates, not failures of the demonstrated path.

## Limitations and self-critique

- No live app/server interactions, model requests/load/downloads, credentials, notebook/store/profile writes, implementation changes, other agents, other review reads or commits. Only the two prescribed outputs were written.

- Sol Rust was read at its absolute candidate path; active backend/docs/tests and evidence were read from the parent. Managed-checkout desktop sources were not used.

- Five actual PNGs were viewed. generating.png is failed-request aftermath, not active generation. No complete settings raster, busy/cancel capture, hover/focus traversal, contrast measurements, dark/high-contrast pass, resize/DPI/compact test, NVDA or cross-platform qualification.

- One three-cell fixture, one error type and one successful Gemma request cannot estimate reliability. Model and transient server context/reasoning settings changed together.

- Existing suites were inspected, not executed. Reviewer checks were extracted-code/in-memory SQLite and saved-record consistency only. No localhost server, package/service construction or live store inspection.

- Direct-provider diagnostics were kept separate from native results. Only aggregate reasoning/content metadata was reported; no raw reasoning trace reproduced.

- Static export, deadline, conversation and application recovery findings were not live reproductions. Source fingerprints identify files at review completion, not the exact binary build; saved screenshots and mutable sources may describe different moments.

- Verified citations mean targets exist, not that claims are true or were in approved context. The 1 / 1 proposal removes the fixture exception but does not establish intended calculation semantics.

- Recorded unchanged source hashes/counts were checked for internal consistency, not freshly verified against live source/runtime. Lock ownership, application crash recovery and persistence races were not fault-injected.

Safety-oriented code inspection probably rewards explicit revision/effect boundaries and penalizes recovery friction. Visual scores are biased toward a single light-theme wide-window capture. Test names indicate breadth, not a fresh green suite. Low-confidence generation/accessibility and static deadline/crash findings need disposable-profile validation. Means summarize review judgment, not measured statistical quality.

## Deliverables

- `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-8de56f2d/artifacts/assistant-review-2026-11-06/sol-review.md`

- `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-8de56f2d/artifacts/assistant-review-2026-11-06/sol-scores.json`

No implementation changes or commits.
