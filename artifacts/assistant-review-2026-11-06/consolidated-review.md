# KernelLedger assistant: Grok 4.7 and GPT 6.1 Sol reviews

2026-11-06. Both reviewers used xhigh reasoning in separate managed worktrees, the same saved Sol UI evidence and the active Python backend. No implementation changes or live UI/server interactions were made during the reviews or this consolidation.

**The assistant is usable for a bounded demonstration, with configuration and recovery gaps to fix before broader qualification.** Both reviewers rated the current experience around 6/10. Revision-checked proposals, explicit effects and separate assistant storage are stronger than model setup, capability reporting and failure recovery. Neither review demonstrated a P0 issue; that is not a full security or release certification.

| Reviewer | Assistant UI / 10 | Backend / 10 |
|---|---:|---:|
| Grok 4.7 xhigh, Cursor | 5.7 | 5.9 |
| GPT 6.1 Sol xhigh, Codex | 6.1 | 6.2 |

Means are independent reviewer judgments with medium overall confidence, not measured performance or a pooled consensus score. Arithmetic was rechecked from all 40 dimension scores. Each reviewer scored 10 UI and 10 backend dimensions, but behavioral coverage within those dimensions remains partial. Exact means and full per-dimension rationales are preserved in the original JSON files.

## Full score comparison

### UI/UX

| Dimension | Grok 4.7 | GPT 6.1 Sol |
|---|---:|---:|
| Provider onboarding and configuration | 5.5 | 5.5 |
| Readability and visual hierarchy | 6.0 | 7.5 |
| Context review and destination clarity | 5.0 | 6.5 |
| Conversation and task workflow | 6.0 | 5.5 |
| Generation feedback and cancellation UX | 5.0 | 5.5 |
| Error clarity and recovery | 4.5 | 4.0 |
| Evidence and citation UX | 7.0 | 6.5 |
| Proposal review and user control | 6.5 | 7.5 |
| Keyboard and accessibility support | 5.0 | 6.0 |
| Responsive layout and space use | 6.0 | 6.0 |
| **Mean / 10** | **5.7** | **6.1** |

Exact means: Grok 56.5 / 10 = 5.65; Sol 60.5 / 10 = 6.05. Half-up display; all ten dimensions scored.

### Backend

| Dimension | Grok 4.7 | GPT 6.1 Sol |
|---|---:|---:|
| Provider and protocol compatibility | 6.5 | 6.5 |
| Schema support and model capability qualification | 4.5 | 4.5 |
| Context grounding and citation validation | 7.0 | 7.0 |
| Proposal validation, staleness, and effect boundaries | 7.0 | 8.0 |
| Reasoning, token budgets, and deadlines | 4.0 | 5.0 |
| Streaming and cancellation | 6.0 | 6.5 |
| Persistence and concurrency | 7.5 | 7.0 |
| Privacy, credential handling, and egress controls | 6.5 | 7.5 |
| Failure diagnostics and recovery | 4.5 | 4.5 |
| Test coverage and release readiness | 5.5 | 5.5 |
| **Mean / 10** | **5.9** | **6.2** |

Exact means: Grok 59.0 / 10 = 5.9; Sol 62.0 / 10 = 6.2. Half-up display; all ten dimensions scored.


## Prioritized shortcomings to cover

Priority and order below are the parent's synthesis. They preserve both reviews' evidence while resolving differences in severity. S/M/L indicate relative effort, not dates. Static findings must receive focused regression coverage before being called fixed. The original reports contain detailed file/line references and reviewer-specific acceptance checks.

### First fixes

1. **C01 — P1 / S: Repair capability-cache status.** Status queries `value` while the capabilities table stores `result`; the broad exception handler also changes displayed usage to zero. The parent independently reproduced `no such column: value`, a cached `structured_output=false` reported as true, and actual reserved usage 12,345 reported as zero, using extracted code and in-memory SQLite only. Fix the query, separate usage/capability errors, and default unprobed capabilities to Unknown. **Acceptance:** a probe-write/status-read round trip preserves false/unknown capabilities and nonzero usage. This is a status defect; send-time budget enforcement was not shown to be bypassed. Sources: Sol F02, `parent-capability-cache-check.json`, backend `service.py:127-143` and `store.py:36`.

2. **C02 — P1 / S–M: Preserve and expose provider options.** Native Save compatible replaces the provider object with `base_url`, dropping supported `structured_output` and `allow_http_host` fields. The demonstrated setup required schema mode enabled outside the form. Merge validated settings and expose schema enforcement with its compatibility limits. **Acceptance:** Save/reopen retains URL, schema mode and a named HTTP exception, and the outgoing request matches the selected mode. Sources: both reviews; Sol UI `assistant.rs:2134`.

3. **C03 — P1 / M: Keep a valid explanation when a proposal is invalid.** A single target/hash validation failure currently fails the whole response before saving its assistant message. Preserve safe, schema-validated answer text and citation results independently; reject unsafe proposals visibly. Distinguish invented/unknown targets from source that actually changed after preview. **Acceptance:** mixed valid/invalid proposal fixtures retain the answer, keep invalid entries inapplicable, and never substitute guessed hashes. The native rejected proposal bodies were not retained, so their exact bad fields remain unknown. Sources: Sol F04; backend `service.py:343-368`, `tools.py:195-198`.

4. **C04 — P1 / M: Make errors truthful and retry actionable.** Severity is inferred from words in a status string, so `stale` appears green. Provider finish reasons collapse into one generic error, long messages clip, and the UI keeps Approve enabled after the backend consumes the receipt. Carry typed severity/retryability, wrap errors, clear spent receipts, and offer Rebuild context with the user's prompt/selection intact. **Acceptance:** length, refusal, timeout, budget, policy, invalid target and real staleness show distinct guidance; a failed send cannot reuse its consumed receipt. Sources: both reviews; backend `providers.py:241-248`, `service.py:274`; Sol UI `assistant.rs:1014`, `lib.rs:787`.

5. **C05 — P1 / M–L: Separate connectivity from task qualification.** A tiny connected-JSON response does not establish that Explain error will finish, produce useful edits or cite correct evidence. Report exactly what the probe observed; qualify representative tasks by endpoint, model, schema mode and budget profile. **Acceptance:** connection success is labeled SSE/JSON connectivity, and a repeated task matrix records success denominators, actual profiles, citation/target correctness and latency. Sources: both reviews; backend `capabilities.py`; saved native failures/success.

6. **C06 — P1 / M: Provide supported reasoning and context controls.** Reasoning can consume the output allowance without final answer text. Current working reasoning/context settings were supplied transiently through Lemonade, outside the app. Add provider-supported controls or clear server setup guidance, context-window admission and output-limit recovery. **Acceptance:** a reasoning-only length fixture yields a useful next step; unsupported parameters are not sent indiscriminately. Do not fix this merely by raising every output limit. Sources: both reviews; direct SSE diagnostic and server load records. The direct diagnostic is not proof of every native failure's exact cause.

7. **C07 — P1 / M: Make answer PDF/Word export export the answer.** The answer's buttons call notebook export without the answer `markdown` argument. The parent verified the call path: the document helper therefore selects notebook content. Pass a captured, reviewed answer/report payload or label the action as notebook export. **Acceptance:** an answer export contains the reviewed answer and intended citations, while a separate notebook export still contains the notebook. Sources: Sol F06, Grok item 13; Sol UI `assistant.rs:1419`, `interchange.rs:117`; backend `documents.py:388`. This is a static wiring finding, not a live export test.

8. **C08 — P1 / S–M: State what citation verification means.** The unconditional Evidence-checked badge can imply more than target existence. Show no/partial/full target resolution, approved-context membership and source revision separately from factual correctness. **Acceptance:** zero citations, mixed citations and an unrelated existing id never produce a claim of fact checking; current navigation and hypothesis labels remain. Sources: both reviews; Sol UI `assistant.rs:1378`; backend `service.py:378`.

9. **C09 — P1 for context-review clarity / M: Make outgoing context scannable.** Escaped JSON tracebacks obscure the evidence being approved. The composer counts selected chips, whereas Explain additionally includes execution/output records; later evidence can also be omitted when context limits are reached. Distinguish **selected**, **included**, **omitted** and **truncated**, rather than forcing these concepts into one count. Summarize source/status/traceback, with exact payload/system-message inspection and copy still available. **Acceptance:** a long or over-budget selection explains every included/omitted item, shows destination/model clearly, and keeps status/actions reachable. Sources: both reviews; backend `context.py:41`; Sol receipt rendering.

### Follow-ups and qualification

10. **C10 — P2 / M: Keep conversation, task and model state coherent.** Follow-ups currently replace a single answer; switching conversations can leave previous answer/proposals; composer Review uses `ask` even after Explain; task-model overrides are not consistently reflected in the header/cache. Scope turns, proposals, receipts and effective model to the actual conversation/task. **Acceptance:** two conversations/two turns plus reload show only their own evidence and proposals; Explain follow-ups disclose their execution context, and coding overrides are visible before send. Sources: both reviews; Sol F12/F13, Grok item 10.

11. **C11 — P2 / M: Account for all inference and label usage honestly.** Connection probes bypass normal request admission/reservations. Failed requests remain conservatively reserved by design, and per-notebook/provider UTC caps differ from actual billing totals. Include probes in accounting and show reserved versus known actual usage and cap scope. **Acceptance:** a small cap blocks repeated probes before inference; success/error/cancel accounting remains conservative and understandable. Sources: Sol F08, Grok item 15.

12. **C12 — P2 / M: Bound deadlines and qualify cancellation/progress.** Per-socket/retry timeouts may exceed a total deadline. The native draft appears only after its answer JSON string closes, so long reasoning phases can look idle. Keep elapsed/status/Cancel visible without exposing reasoning; prevent late events from overwriting a newer request. **Acceptance:** isolated stalled/trickling/retry fixtures respect the declared deadline; native Cancel settles and a new request recovers cleanly. Sources: both reviews; Sol F09. No live hang or successful native cancellation was demonstrated.

13. **C13 — P2 / L: Reconcile accepted, applying and applied effects.** Acceptance is recorded before the separate desktop effect/application record. A crash or helper conflict can leave accepted-but-unapplied or applied-but-unrecorded states. Use truthful states, idempotent application identities and restart reconciliation. Also qualify retention/purge recovery and confirm genuinely destructive bulk deletion. **Acceptance:** fault injection before/after effect and audit recording cannot duplicate inserts/runs or report an uncertain effect as successfully applied. Sources: Sol F10; both qualification lists. This is a static risk; the saved session applied nothing.

14. **C14 — P2 / M: Align approval scopes and documentation.** Docs say automatic egress approvals are disabled, but scoped/legacy grants exist. A scoped revocation may leave a broader legacy grant effective. Choose and document one consent model; reveal and revoke effective grants. **Acceptance:** notebook+URL, notebook+host and global-host fixtures show their true scope, and Revoke stops the displayed destination's effective auto-approval. The current UI still explicitly approves sends; neither review demonstrated silent remote sending. Sources: both reviews; Sol F15.

15. **C15 — P2 qualification / M: Exercise keyboard, accessibility and constrained layouts.** Named UIA controls are a foundation, not proof of keyboard/NVDA behavior. Test announcement severity, focus restoration, offscreen actions, long answer labels/ids, narrow panels, DPI/scale, compact overlay, dark/high-contrast themes and measured contrast. **Acceptance:** a disposable-profile keyboard/screen-reader journey can inspect context, send/cancel/retry, follow citations and review proposals without obscured actions. Sources: both reviews. Missing evidence is not a claim that all of these fail.

16. **C16 — P1 before broad qualification / L: Add a representative native/provider regression matrix.** Cover multiple errors/tasks and repeated attempts with exact versions/settings; schema/finish-reason failures; mixed proposals; unrelated citations; context omission; provider Save/status/export wiring; apply/reject/regenerate, frozen-hash runs, crash recovery, policy/credentials and active cancellation. **Acceptance:** publish supported profiles, denominators, observed effects and remaining exclusions. Fake-provider suites and twelve fixture checks cannot replace this. Sources: both reviews; Sol F14.

## Recommended order

Start with the small deterministic defects C01/C02, then C03/C04 recovery and their regressions. Next cover C05/C06 supported model setup, C07 export and C08/C09 evidence review. Run C16 qualification against that slice. Progress C10-C15 according to the guarantees the release advertises; application recovery, strict timeouts, paid-probe caps or accessibility claims make their associated qualification a release gate.

## Critique of the reviews

- **The scores are subjective and correlated.** Configuration, qualification and recovery recur across dimensions. Keep the independent scores; do not average them into an objective product-quality claim. The largest UI differences are readability and context review, not disagreement that recovery is weak.
- **Sol found an additional concrete defect.** Its capability-cache finding was independently reproduced by the parent. Grok correctly emphasized the enabled Approve button after receipt consumption. These complement one another.
- **Context counts need clearer concepts.** Grok's “1 versus 3” finding is useful, but selected chips and serialized receipt records need not share one flat count. The improvement is disclosure of all included/omitted evidence.
- **Do not overcorrect citation handling.** Kind-specific existence checking already rejects many wrong-kind ids. A source hash is intentionally a source id. The additional gaps are approved-context membership, revision continuity and the overstrong badge, not a new namespace rule applied blindly.
- **Prioritize wrong-content export above visual polish.** Grok called it P3 and Sol P1. The parent adopts P1 for a feature labeled answer/report export because it currently selects different content. The call path is verified; a native export regression remains needed.
- **Avoid repetitive confirmations.** Preserve explicit reviewed effects. Label a run action as running the reviewed cells rather than automatically adding another modal to every Apply. Bulk deletion warrants different treatment.
- **Do not infer model superiority from this session.** LFM and Gemma had different context/reasoning settings; direct diagnostics differ from native requests. The saved request rows do not retain each failed native response's full usage/finish details. One Gemma success and four LFM failures are observations, not reliability estimates.

## Preserve

Keep explicit preview/send, destination/model binding, one-use receipts, revision-checked proposals, visible before/after source, separate assistant process/store, copy-only commands, OS credential handling and citation navigation. Improve error recovery without guessed hashes, automatic application, hidden repairs/model switches, unbounded generation or raw reasoning disclosure. Keep compact expert notebook layouts and density preferences.

## Files and verification

- [Grok report](grok-review.md), [Grok original scores](grok-scores.json)
- [Sol report](sol-review.md), [Sol original scores](sol-scores.json)
- [Interactive score comparison](comparison.html), [normalized scores](comparison.json), [score tables](score-tables.md)
- [Parent capability reproduction](parent-capability-cache-check.json), [source-report hashes](provenance.json)

Parent verification: all four group means recomputed with Decimal half-up rounding; original reviewer files preserved by SHA-256; capability bug reproduced in memory; provider-save and export call paths read directly. No full suites, native application effects or live model/server requests were run for this consolidation. No implementation fixes were made.
