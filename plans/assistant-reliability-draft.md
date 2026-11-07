# Assistant reliability and usability implementation plan

| Item | Decision |
|---|---|
| Date | 2026-11-07 |
| Status | Implementation in working tree, 2026-11-07; bounded automated/native qualification recorded in [implementation notes](../docs/assistant-reliability.md). Broader release qualification remains open. |
| Scope | Repair the existing native assistant's configuration, inference, evidence review and workflow. Cover all C01-C16 review findings. |
| Basis | [Consolidated review](../artifacts/assistant-review-2026-11-06/consolidated-review.md), [Grok review](../artifacts/assistant-review-2026-11-06/grok-review.md), [Sol review](../artifacts/assistant-review-2026-11-06/sol-review.md), and [Lemonade test](../artifacts/lemonade-explain-test-2026-11-06/report.md). |
| Implementation baseline | Sol candidate desktop in `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop`; active Python backend in the parent repository. Preserve the candidate's existing UI work. |
| Plan ownership | Cross-track follow-up to [AI assistant draft](ai-assistant-draft.md) and [UI follow-up](ui-review-followup-plan.md), under [plan versioning rules](README.md). This file does not publish a new authoritative runtime/frontend version. |

## Outcome and boundaries

A user can configure a supported local provider entirely through the app, see what was actually qualified, inspect outgoing context, receive a useful explanation even when its optional edit is rejected, and recover from failure without losing the question or using a spent receipt. Citations, reports and application states describe what they actually establish.

Preserve the native GPUI layout, current typography/density preferences, adjacent notebook and assistant, source revision checks, explicit Apply/Run, destination/model binding, separate assistant process/store and OS credential storage. Keep commands copy-only. No automatic application, guessed hashes, hidden repair requests, model switching, downloads, server reloads or raw reasoning disclosure.

This increment does not unlock vision, autonomous execution, inline completion, plan sign-in or embeddings. It does not promise a numerical rating increase or broad accessibility/model reliability from a single fixture.

## Delivery sequence

Each phase produces a reviewable change, focused regressions and a short evidence report. Tests are added alongside the relevant change; phase 5 is integration qualification, not the first time testing occurs. Keep experimental profiles and notebooks disposable and isolated from the user's normal profile.

### Phase 0 — Preserve and reproduce the baseline

1. Preserve the dirty Sol implementation and unrelated parent edits before establishing the implementation checkout. Ensure that checkout contains the Sol candidate changes; the reviewers' checkouts contain older desktop code and are not the product baseline.
2. Record source revisions/patch hashes, Python executable/package origin, build mode, binary hash, profile path, model/server versions and actual inference settings. Rebuild from the selected source before new before/after captures. Do not rely on the old process id or executable as proof of current source.
3. Reproduce the capability-cache bug, provider Save loss, spent-receipt retry, stale-banner severity and answer-export routing with isolated fixtures. Classify other findings as reproduced, static risk or untested.
4. Specify the shared Python/Rust result contracts: typed failure/status metadata, partial-success warnings, effective model/profile identity, receipt inclusion/omission metadata and application states. Update method catalogs, generated schemas and consumers together when contracts change.
5. Keep schema changes confined to assistant records. For any persistence/contract/guarantee changes, coordinate runtime and frontend plan drafts under `plans/README.md`; do not silently amend the authoritative plans or runtime ledger format.

**Exit:** A reproducible Sol-based baseline, small regression fixtures and an agreed contract change list. Original evidence and user data remain intact.

### Phase 1 — Fix configuration and failure recovery

**Findings: C01-C04. This is the first implementation slice.**

- Fix the capabilities lookup (`result`, not `value`). Handle capability-cache and usage errors independently. Unknown/unprobed/expired capabilities must remain Unknown, rather than optimistic booleans. Retain genuine false results and nonzero reserved usage. Key observations by the effective endpoint/model/profile.
- Merge validated provider settings on Save. Expose the existing structured-output option and preserve supported fields the form does not edit, including named HTTP exceptions. Show effective saved values after reopening. Reject unsupported settings clearly rather than silently dropping them.
- Validate the response envelope independently from individual proposals. A malformed JSON/envelope still fails closed. For a valid bounded/redacted answer, resolve citations and validate each proposal separately; retain the explanation with explicit warnings when an optional proposal is rejected. Only valid proposals receive Apply/Run controls. Never repair an id/hash by guessing.
- Separate invalid generated targets/base hashes from source that genuinely changed after preview. Use typed failure codes and severity through backend, transport and UI; remove substring-based banner coloring. Preserve safe diagnostic metadata such as finish reason and configured limits, without storing raw reasoning or unsafe provider bodies.
- Model receipt state explicitly: prepared, sending, consumed or expired. After a failed send, remove the active Send action and offer **Rebuild context**, preserving question, selected context, task and effective model. Rebuilding never silently sends. A new receipt receives the applicable existing review/approval flow.

**Likely files:** backend `service.py`, `settings.py`, `tools.py`, `store.py`, `contracts.py`; Sol UI/client `assistant.rs`, UI `lib.rs`, shared protocol/models where needed.

**Acceptance:**

1. Cached `structured_output=false` and reserved usage 12,345 survive a status read. Cache failure does not turn known usage into zero.
2. Save/reopen preserves URL, schema mode and an existing HTTP exception; mocked outgoing requests match the form.
3. A valid explanation plus invalid proposal remains readable with a warning; mixed proposals isolate rejected entries. Malformed top-level output remains a failure. A real post-preview source edit remains protected.
4. Stale/invalid/timeout/budget/policy/transport failures have correct severity and readable wrapping. Double-send after failure cannot reuse a consumed receipt; rebuilding creates one fresh preview without losing input.

### Phase 2 — Make inference setup and limits predictable

**Findings: C05, C06, C11, C12. Depends on phase 1 status/error contracts.**

- Relabel Test connection as observed transport/JSON connectivity. Distinguish prompted JSON, enforced schema support and representative task qualification. Do not infer task quality from the tiny connected response.
- Define an effective inference profile: endpoint, provider adapter, actual task model, schema mode, output/context limits, deadline and any supported reasoning setting. Show it consistently and include its relevant fingerprint in the receipt/request record. A changed profile invalidates a prepared receipt.
- For Lemonade, provide reproducible local setup guidance and expose only documented, tested request controls that the adapter can actually support. If a server-side setting cannot be read back or controlled, label it unknown/external. Do not claim a server setting is saved when the test used `save_options: false`.
- Use known context limits for conservative admission. When the context window is unknown, say so and retain bounded context. Separate output-limit exhaustion, refusal, unsupported schema, tool-call-only output and transport interruption, with appropriate next steps.
- Apply common admission/accounting to every inference, including connection tests. Label per-notebook/provider UTC caps, conservative reservations and known actual usage separately. Preserve conservative charging when partial usage is unavailable.
- Carry one monotonic deadline through connection, retry, headers and reads. Keep elapsed state and Cancel visible, preserve the privacy-safe complete-answer draft policy and ignore late events from an older request. Qualify process-kill cancellation and recovery without changing notebook runtime polling.

**Likely files:** `providers.py`, `capabilities.py`, `context.py`, `settings.py`, `service.py`, `store.py`; client assistant transport; UI settings/composer/status.

**Acceptance:** A reasoning-only length fixture explains that no final answer was produced and offers supported choices. Unknown limits remain unknown. Low caps refuse repeated probes before inference. Stalled/trickling/retry fixtures obey the documented deadline plus a measured grace. Native Cancel reaches idle and a subsequent request succeeds without old events overwriting it.

Run a small matched Lemonade check here: same source/context, fixed explicit settings and repeated attempts for each selected model/profile. Record failures as well as successes. Do not interpret different server settings as an isolated comparison of model quality.

### Phase 3 — Make context, evidence and reports understandable

**Findings: C07-C09; essential keyboard/error accessibility from C15.**

- Present the question, destination, effective model/profile and selected/included/omitted/truncated counts before the detailed payload. Show attached cell source, execution state and a readable traceback instead of JSON-escaped prose. Explain automatically attached execution/output context.
- Keep collapsible exact payload/system-message inspection and full copy. UI summaries must not silently change what is sent. Show every context omission/truncation and its reason.
- Keep status, Cancel and review/send actions reachable without scrolling through an expanded receipt. Wrap long errors and identifiers; preserve full selectable values and useful native accessible names.
- Replace unconditional Evidence-checked wording with target-resolution counts. Track approved-context membership and relevant revision separately. Do not imply factual truth or remap citation namespaces. Preserve navigation and unverified hypothesis labels.
- Capture the reviewed answer/report when Export PDF/Word is invoked, and pass it through the helper's existing `markdown` path. Preserve intended citation/provenance text. Keep notebook export a separate action; changes to the visible answer while a file dialog is open must not change the captured export silently.

**Likely files:** UI `assistant.rs`, `interchange.rs`, components/accessibility integration; backend `context.py`, `service.py`, `documents.py` and the existing export contract.

**Acceptance:** A long traceback and an over-budget selection remain inspectable, with honest inclusion counts. Zero/mixed/unrelated-existing citation fixtures never imply fact checking. Answer exports contain the captured answer, not unrelated notebook content; notebook export continues to work. At narrow/high-scale panel sizes, error text and primary actions remain reachable by keyboard.

### Phase 4 — Make conversations and applications durable

**Findings: C10, C13, C14.**

- Keep request-linked turns, citations, receipts and proposals scoped to their conversation. Restore navigable historical evidence on reload; clear or rehydrate the current answer/proposals when switching conversations. Disclose history limits and load more explicitly where supported.
- Preserve task intent and relevant execution context in follow-ups. Resolve default/coding task overrides consistently for composer, preview, probe/cache and send; changing task/model clears incompatible prepared context.
- Separate proposed, accepted, applying, applied, failed and unknown outcomes. Persist application intent with a stable identity and expected revisions before invoking existing source/runtime effect paths; record the resulting manifest revision or durable runtime request id afterward.
- Reconcile interrupted application attempts on restart. Prevent duplicate insertions/runs and never blindly retry an uncertain effect. Where an outcome cannot be established, show **Needs review**. Do not claim exactly-once transactions across separate stores.
- Keep grants explicitly scoped to notebook plus normalized destination. Reveal/migrate or deactivate broader legacy host grants with clear product guidance, and make Revoke remove the effective permission. Align UI/API/CLI documentation. Retain the current explicit UI Send; no new silent sends.
- Make genuinely destructive bulk assistant-data deletion deliberate. Keep ordinary reviewed edits/runs direct and clearly labeled, without adding repetitive confirmation dialogs.

**Likely files:** UI `assistant.rs`/`lib.rs`; client `service.rs`; backend `service.py`, `store.py`, settings/CLI/docs. Any assistant-store migration must preserve existing conversations and handle interrupted records.

**Acceptance:** Two conversations with multiple turns and different task models reload without mixed answers/proposals. Fault injection before effect, after effect and before application recording produces truthful recoverable states with no duplicate effects. Post-acceptance source conflicts remain refused. Scoped/legacy grant fixtures show their true scope and revocation works. Migration and reopening existing assistant stores pass.

### Phase 5 — Qualify the complete native workflow

**Findings: C15, C16; verifies every previous phase.**

Use fresh binaries, isolated profiles and owned notebooks. Save source/binary hashes, environment, UI geometry/DPI/theme, effective inference profiles, request outcomes and exact reproduction commands under a new dated evidence directory.

| Area | Required journeys/checks |
|---|---|
| Setup/status | Fresh profile; save/reopen; schema unsupported; cached false/Unknown; task-model override; budget display/admission; credentials unavailable. |
| Model tasks | Explain at least division-by-zero, missing-name and missing-import errors; Ask; edit/refactor; repeated attempts per supported profile. Include wrong hashes/ids, no-op edits, malformed/mixed proposals, refusal and output-limit cases. |
| Context/evidence | Exact receipt/profile binding; changed destination/model/settings; large UTF-8 input; omissions/truncation; unrelated existing citations; copied and exported content. |
| Effects/recovery | Apply, Reject, Regenerate; edit/insert/run separation; frozen hashes and post-preview conflicts; stopped/running runtime; crashes and duplicate attempts; retention/purge and owner recovery. |
| Transport | SSE fragmentation; interrupted/slow streams; deadlines; cancel before/during generation; retries; next-request recovery and late-event isolation. |
| Native usability | Keyboard-only and NVDA journeys; focus return/error announcements; narrow panel/window and large scale; Compact/Comfortable; light/dark/high contrast; long output/ids/receipts. Measure contrast where claiming conformance. |
| Privacy/policy | Local-only/off policy; destination changes and grant revocation; secret redaction across fragments; no credentials in artifacts or exports. Use controlled fixtures for remote-policy tests. |

Start with deterministic fake/local fixtures, then real Lemonade profiles for task quality/latency, then native journeys. Do not require a cloud account to qualify the local path; any untested provider/platform remains explicitly unqualified.

**Exit:** Publish supported versions/profiles, attempt denominators, failures, latency, application outcomes and exclusions. Deterministic correctness checks must pass. An unreliable real model stays unqualified for the affected task instead of being hidden by retries or averages. No supported journey may have an unresolved known unintended effect, wrong-content export, false-success status or broken recovery path.

## Verification strategy

Use the existing `tests/test_assistant.py`, HTTP fixtures, generated-contract checks, native client/UI tests and `scripts/assistant_ui_probe.py`; extend their missing seams rather than duplicating the implementation in tests. In particular, test real Store/probe/status round trips and actual native Save/export command construction: those were missed by prior isolated helper tests.

Run focused tests after each change, then the affected Python and Rust suites and required lint/type/format checks. Run Cargo checks from the chosen checkout's `desktop/` so its configuration is applied. Build a fresh native executable before desktop qualification. Do not rerun expensive unrelated suites repeatedly without a new failure or change to justify it.

Implementation verification and remaining qualification gates are recorded in the linked implementation notes and dated evidence directory; historical review scores are not acceptance results.

## Coverage and dependencies

| Finding | Delivery phase |
|---|---|
| C01 capability cache; C02 provider Save; C03 answer/proposal isolation; C04 error/receipt recovery | 1 |
| C05 connectivity/task qualification; C06 reasoning/context setup; C11 inference accounting; C12 deadline/cancel | 2, verified in 5 |
| C07 export; C08 citation semantics; C09 readable complete receipts | 3 |
| C10 conversation/task/model state; C13 application reconciliation; C14 approval scopes/docs | 4 |
| C15 accessibility/layout; C16 native/provider matrix | Throughout; final qualification in 5 |

The ordering is **0 → 1 → 2 → 3 → 4 → 5**. Shared Python/Rust contracts are integrated in one implementation line. Do not have separate agents edit the same `assistant.rs`, `service.py` or store schema concurrently. If later parallel implementation is explicitly requested, partition ownership and integrate agreed contracts before dependent UI work.

Phase 1 is the smallest useful first milestone. A dependable end-to-end assistant requires the remaining phases; do not label a phase-1 build broadly qualified. The largest uncertainty is application crash reconciliation in phase 4. Resolve its persistence design early in phase 0, without claiming its untested guarantees before fault-injection checks pass.
