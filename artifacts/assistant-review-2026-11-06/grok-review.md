# Grok review: KernelLedger assistant UI and backend

Reviewer: grok. Model: Grok 4.7, xhigh reasoning, Cursor. Date of evidence: 2026-11-06. This is a read-only review of the Sol UI candidate and the parent Python backend. No implementation was changed, no live window or server was touched, and the other reviewer's files were not opened.

## What was inspected

Images actually opened:

- `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/gemma-result.png`
- `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/final.png`
- `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/generating.png`
- `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/no-thinking-result.png`
- `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06/gemma-preview-current.png`

Image reading produced a visual description of each PNG. Color, layout, and clipping notes below come from that viewing and are marked as visual. Exact control names come from the UIA JSON, which is the authority for strings. No contrast ratio was measured. `generating.png` matches the brief: it is the first incomplete-response failure, with the receipt and the provider-error banner, not an in-progress generation frame.

UI source is the Sol tree `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop`. Backend source is `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/src/kernelledger/assistant`. This worktree's older desktop tree was not used as the candidate.

Checks run for this review, all read-only: SHA-256 of `cells/error.py`, and a field summary of `raw-sse-diagnostic.json` that counted reasoning versus content deltas and finish reasons without copying reasoning text. No pytest, no UI automation, no model call, no SQLite open. The ledger execution count in `results.json` was not re-counted.

Desktop UX judgment used the parent `ui-ux-pro-max` checklist for error text, status announcements, keyboard escape, and color-independent state. Web body-font and 44px touch-target defaults were not applied to this dense GPUI notebook.

## What the session actually shows

One three-cell notebook, one `ZeroDivisionError` from `1 / 0`, five native Explain requests to Lemonade 2026.41.1 at `http://127.0.0.1:13305/api/v1`.

| Attempt | Model | Recorded result | Seconds |
|---|---|---|---:|
| 1–2 | LFM2.5-8B-A1B | `provider_error` | 15.745, 15.904 |
| 3–4 | LFM2.5-8B-A1B | `stale` | 2.912, 9.509 |
| 5 | Gemma-4-26B-A4B-it-GGUF | `done`, two citations marked verified, proposal left `proposed` | 29.300 |

`cells/error.py` is the five bytes `1 / 0`. Its SHA-256 is `95baf036f1aa2733423c14f202dbee17b0d6c29230f6484fd711c678cd64635a`, the same hash as the Gemma proposal and `results.json`. The first four native requests share receipt hash `00b7baa441eb99c7036013a8fbd061a882fcf34a126444b5efdd8f5786422ae9`. Gemma's receipt hash differs because the model name is inside the receipt. The source hash did not change between the stale failures and the successful proposal.

The successful answer names `ZeroDivisionError: division by zero` and `1 / 0`, labels a cause hypothesis as unverified, and proposes `1 / 1`. Citation chips `@cell:error` and `@execution:3b2ee1d5-4ecc-4f18-ae35-5acdf03beaf3` are marked verified. `citation-target-uia.json` shows History as the current destination and "Recorded execution error: ZeroDivisionError: division by zero" for execution 5. The settled notebook UIA still shows cell 3 source `1 / 0`. `applications` is empty. The runtime button reads "Not running".

That is existence of the cited records plus a fixture-sized edit that would remove this exception. It is not proof the hypothesis is true, and it is not a general repair for division.

Direct provider files are separate from the five native requests. `raw-sse-diagnostic.json` ends with `finish_reason` `length`, 510 reasoning deltas, and 0 content deltas. `direct-no-thinking-diagnostic.json` is schema-shaped, cites the execution id as `source` and the source hash as `cell`, and proposes leaving `1 / 0` unchanged while copying the correct hash. `direct-schema-diagnostic.json` is another incomplete response with 0 characters. None of those are native UI passes.

## Score table

5 means substantial friction. 7 means usable with meaningful gaps. 9 would mean robust and qualified. The Gemma panel is real and is scored as one success, not as production quality.

### UI/UX

| # | Dimension | Score | Confidence | Rationale |
|---|---|---:|---|---|
| 1 | Provider onboarding and configuration | 5.5 | high | Presets, masked key storage, policy, budgets, Test connection, and List models exist, and this profile did reach a local Lemonade URL. The form has no structured-output or reasoning control. Save compatible writes only `base_url`. The success banner claims structured JSON for any passing test. |
| 2 | Readability and visual hierarchy | 6.0 | high | The completed answer, hypothesis label, citation row, and current/proposed diff are readable on the wide layout in `gemma-result.png` and `final.png`. The required review step is a compact JSON traceback. |
| 3 | Context review and destination clarity | 5.0 | high | Local, host, model, and Approve are explicit. The composer says "Context: 1 item" while the Gemma receipt sends a cell, an execution, and an output. The output body is one JSON line, including absolute `worker.py` paths. |
| 4 | Conversation and task workflow | 6.0 | high | Cell Explain error opens the panel, selects `error`, and uses task `explain`. History and proposals are behind icon buttons. The composer Review action always starts task `ask`. The panel shows the latest answer, not a thread. |
| 5 | Generation feedback and cancellation UX | 5.0 | medium | `after-send-uia.json` has "Request started; generating a draft…" and an enabled Cancel. No captured frame shows a growing draft. The 29.3s Gemma call has no elapsed time or token progress in the evidence. Cancel's effect was not exercised. |
| 6 | Error clarity and recovery | 4.5 | high | Failures are visible and the receipt stays on screen. The text is `respond: provider_error: …` or `respond: stale: …`. The stale banner is painted as success. Approve stays enabled after the server has dropped the receipt. |
| 7 | Evidence and citation UX | 7.0 | high | Verified chips navigate to execution 5 and back to the cell. Unverified chips are disabled in code. The green "Evidence-checked" badge is always on once an answer exists. Raw ids are the labels. |
| 8 | Proposal review and user control | 6.5 | high | The Gemma card shows `1 / 0` beside `1 / 1` with Apply, Reject, and Regenerate, and nothing was applied. The two stale native attempts produced no proposal card, so there was nothing to reject or regenerate. |
| 9 | Keyboard and accessibility support | 5.0 | medium | Buttons generally have names, including the icon header. Failures are `ControlType.StatusBar`. Focus during generation was "Copy full receipt", not Cancel. No assistant shortcut exists beside the command palette. NVDA and a keyboard-only pass were not done. |
| 10 | Responsive layout and space use | 6.0 | medium | At this wide size the notebook and assistant sit together and the diff fits. The receipt does not use that width as readable text. Narrow, dark, and high-contrast Sol captures are absent. |

UI group: 56.5 / 10 = 5.65 unrounded. Half-up to one decimal: **5.7**. Coverage 10/10, no nulls. Confidence in the mean is medium: dimensions 5, 9, and 10 depend on one wide light session and UIA snapshots.

### Backend

| # | Dimension | Score | Confidence | Rationale |
|---|---|---:|---|---|
| 1 | Provider and protocol compatibility | 6.5 | high | Compatible chat and OpenAI Responses SSE are implemented with pinned loopback resolution, no redirects, and status-only HTTP errors. `reasoning_content` is ignored. `length`, `content_filter`, and `tool_calls` share one error. |
| 2 | Schema support and model capability qualification | 4.5 | high | `structured_output` can send a strict `json_schema`, and Gemma then returned a valid edit. The probe records structured output as true after prompted JSON, and `status_summary` defaults that flag to true whenever a provider name is set. Schema validity still allowed the diagnostic's swapped citation kinds and a no-op edit. |
| 3 | Context grounding and citation validation | 7.0 | high | The explain receipt is the selected cell, its execution, and its error output, hashed and bound to destination and model. Citation checks are id lookups. A source-hash cited as kind `source` satisfies the revision lookup. |
| 4 | Proposal validation, staleness, and effect boundaries | 7.0 | high | Bad shapes never become rows. Apply rechecks the hash and goes through `source.mutate` or a stop-on-error batch. Commands are copy-only. The single code `stale` covers a missing cell, a bad hash, and a real edit. |
| 5 | Reasoning, token budgets, and deadlines | 4.0 | high | `max_output_tokens` 2048, a daily reservation, and a 60s deadline exist. The adapter sends no reasoning budget. The passing Gemma load used Lemonade `--reasoning-budget 512` with `save_options: false`, outside the app. |
| 6 | Streaming and cancellation | 6.0 | high | SSE is incremental and unit-tested, including mid-stream cancel. The UI is told the answer only after the JSON string closes. Desktop Cancel kills the process instead of sending the Python cancel method. |
| 7 | Persistence and concurrency | 7.5 | high | Records live in a separate SQLite file with a directory lock, interrupted-row recovery, retention, and purge. One owner. In-memory receipts die with the process. The live database was not opened. |
| 8 | Privacy, credential handling, and egress controls | 6.5 | high | Keys stay in the OS store, the desktop strips provider env vars, and remote HTTP is refused without an explicit host exception. Local `respond` does not require `approved_receipt`. `Always allow` can skip the remote check. The docs say automatic egress approvals are disabled. |
| 9 | Failure diagnostics and recovery | 4.5 | high | Distinct codes exist, and provider bodies are withheld. The stored LFM errors are only `provider_error` or `stale`, with empty usage. The rejected proposal body is not kept, and the receipt is already popped. |
| 10 | Test coverage and release readiness | 5.5 | high | Unit tests cover receipts, stale apply, redaction, HTTP failure, and cancel. `scripts/assistant_ui_probe.py` is a fake-provider happy path. The docs already defer real-model quality, NVDA, and Linux Secret Service. This manual session is one exception type. |

Backend group: 59.0 / 10 = 5.90 unrounded. Half-up to one decimal: **5.9**. Coverage 10/10, no nulls. Confidence in the mean is medium-high: the mechanisms were read in source; concurrency and release claims were not re-run.

## Audit of the parent test log

The log is right about the scope. Twelve checks in `checks.json` fit this fixture. They do not prove model reliability, accessibility, or release readiness.

| Check | Audit |
|---|---|
| Native request done, model is Gemma, answer visible | Matches `results.json` request `b979370a5446434995ac92592c583397` and the answer UIA. |
| Diagnosis correct for the fixture | The exception type and expression match the cell and History. The hypothesis is labeled unverified and stays unverified. |
| Both citations verified | The app marked both ids present. That is target existence. The "Observed Traceback" paragraph restates the exception; it is not a quotation check. |
| Proposal correct, current hash, unapplied | `1 / 1` with the file hash above, state `proposed`, `applications` empty. It removes this exception. It is one possible edit. |
| Source unchanged, no new executions, runtime stopped | File hash rechecked here. Execution count 5 was taken from `results.json` only. The screenshots show Not running. |
| Citation navigation | UIA supports History for the execution and the notebook cell afterward. No separate History screenshot was in the five PNGs. |

Agreed findings from the log, after code check:

- The compatible adapter does not send a reasoning control. LFM's direct SSE stream spent the allowance on reasoning and finished `length` with no content. The UI string is "provider response was incomplete or unsupported".
- `structured_output` is a supported provider key and is absent from the settings form. The Sol save handler replaces the provider object with `{"base_url": ...}`, which drops `structured_output` and `allow_http_host`.
- "proposal source has changed or cell is missing" is raised when the cell is absent or the hash differs. The notebook hash was stable, so a real user edit is the wrong explanation for attempts 3 and 4. The native proposal body was not stored, so the exact bad field is unknown.
- The stale banner is the success color. The image of `no-thinking-result.png` shows a green border and green dot. The status color is chosen by searching the sentence for "error", "failed", or "disabled". "stale" misses that search.

The image reader also described the provider-error line as cut off at "unsup". UIA holds the full sentence, and the status bounds are 810 by 83. Clipping is possible and unmeasured. The wording and the color rule are the defects to fix first.

The log's "Connection verified" sentence is produced for every test whose result has `streaming` or `ok`. It does not mean the selected model can finish Explain error.

## Prioritized shortcomings

No item is P0. The successful path left source, executions, and the runtime untouched, and invalid proposals were not applied. P0 would need a demonstrated case of an unwanted mutation, a credential leak, or a dead end with no recovery.

### Release-blocking slice

1. **P1. Name output-limit and reasoning starvation separately from a dead provider.**
   - Trigger: LFM Explain with the default 2,048 output cap. Attempts 1 and 2 returned `provider_error` in about 16 seconds. The direct SSE diagnostic, which is not a UI pass, finished `length` after 510 reasoning deltas and no content.
   - User consequence: the banner says the response was incomplete or unsupported. A passed connection test still reads as if the model is qualified. The user cannot tell whether to raise the output cap, reduce reasoning, or change model.
   - Evidence: `src/kernelledger/assistant/providers.py` lines 241–248; `desktop/crates/ui/src/lib.rs` lines 891–897; `results.json`; `raw-sse-diagnostic.json` summary above.
   - Change: when `finish_reason` is `length`, say the output cap was reached and include `max_output_tokens`. When the stream carried reasoning and no answer content, say the reasoning tokens used the cap. Keep `content_filter` and transport failures on their own sentences. Leave the connection banner as a statement of what was observed.
   - Acceptance: a length finish shows the cap and a next step. A refused HTTP status does not use that sentence. The connection banner no longer says the Explain task is qualified.
   - Effort: M. Confidence: high.

2. **P1. Keep schema enforcement visible and stable across Save.**
   - Trigger: `provider-before-schema.json` has only `base_url`. The isolated `assistant.json` now has `structured_output: true`, edited outside the form. Save compatible rebuilds the provider entry from the URL field.
   - User consequence: the next save silently turns schema mode off, clears the open receipt, and marks the connection untested. Models that only follow the prompt then fail later, or waste the output cap.
   - Evidence: Sol `desktop/crates/ui/src/assistant.rs` lines 2134–2135; `providers.py` lines 189–193; `settings.py` lines 80–83; the two JSON profiles.
   - Change: add a structured-output checkbox for the compatible provider. On save, preserve `structured_output` and `allow_http_host` when the form does not edit them.
   - Acceptance: save, reload, and the file still contains `structured_output: true` and any allow-host exception. The checkbox matches the file.
   - Effort: S. Confidence: high.

3. **P1. Split proposal-target failures from real staleness, and paint them as failures.**
   - Trigger: native attempts 3 and 4, error `stale`, while `cells/error.py` still hashes to the receipt revision.
   - User consequence: the panel says the source changed or the cell is missing. The notebook did not change. There is no proposal card, so Regenerate is absent. The banner uses the success color.
   - Evidence: `tools.py` lines 197–198; Sol `assistant.rs` lines 1014–1024; `no-thinking-result-uia.json` name `respond: stale: proposal source has changed or cell is missing`; `no-thinking-result.png`.
   - Change: use separate codes for unknown cell, hash mismatch, and a revision that changed after a valid proposal. Map codes to danger styling. Keep the reject. Show the model cell id and hash prefix next to the current revision when it is safe to do so.
   - Acceptance: an unchanged notebook plus a bad hash does not say the source changed. A real edit after a valid proposal still cannot apply. The failure banner is danger-colored even if the word "error" is absent.
   - Effort: S. Confidence: high that the message is wrong for this session; medium on which native field mismatched, because the rejected body was not stored.

4. **P1. Make the receipt match what will be sent, in readable form.**
   - Trigger: Explain error. Composer UIA says "Context: 1 item" and "Remove 1 / 0 from context". `gemma-receipt.json` has three items. The output item is compact JSON. `generating.png`, `gemma-preview-current.png`, and `no-thinking-result.png` show that JSON traceback, with escaped newlines and `worker.py` paths, above Approve.
   - User consequence: Approve is supposed to be an inspection. The count understates the payload, and the exception is hard to read. Absolute paths are easy to miss before a remote send.
   - Evidence: `context.py` lines 17–18 and 41–70; Sol `assistant.rs` lines 318–327 and 749–753; `gemma-receipt.json`.
   - Change: render cell source, execution status, and traceback as text, with kind, bytes, truncation, and redaction. Count every receipt item. Keep Copy receipt as the exact JSON. State that explain will attach the selected cell's recorded execution.
   - Acceptance: a reviewer can see `1 / 0` and `ZeroDivisionError` without parsing JSON, and the on-screen count equals `items.length`.
   - Effort: M. Confidence: high.

5. **P1. After a failed send, stop offering Approve for a receipt the server already dropped.**
   - Trigger: `respond` pops the receipt before the provider call. On `provider_error` or `stale`, the UI leaves the receipt up and leaves Approve enabled. `first-request-failed-uia.json` shows that button enabled beside the incomplete-response status.
   - User consequence: a second click is the natural recovery and raises `receipt_required` until the user previews again. The button label still says Approve.
   - Evidence: `service.py` lines 274–276 and 367–375; `tests/test_assistant.py` `test_remote_receipt_required_and_one_use`; Sol `lib.rs` lines 805 and 811–813.
   - Change: on those failures, disable Approve and say to review the context again. Do not automatically send a second request. Restoring the in-memory receipt is acceptable only when the provider connection never started.
   - Acceptance: after an incomplete response, Approve is disabled and the status tells the user to preview again. A fresh preview can be approved once.
   - Effort: S. Confidence: high for the code path. This session did not capture the second click; later attempts look like new previews because the receipt hash stayed stable.

6. **P1. Qualify the connection test as a connection test.**
   - Trigger: `gemma-connection-uia.json` name "Connection verified (streaming & structured JSON qualified)." LFM had already passed a connection test and then failed Explain four times.
   - User consequence: setup looks finished for a model that cannot complete the task under the current cap.
   - Evidence: Sol `lib.rs` lines 891–897; `capabilities.py` lines 12–32; `service.py` lines 127–129.
   - Change: report SSE observed, whether `response_format` was sent, and whether the body matched the schema. Say that task quality, reasoning headroom, and context window were not measured.
   - Acceptance: with `structured_output` false, a prompt-only JSON success does not say structured output is qualified. A later Explain failure does not contradict the earlier sentence.
   - Effort: S. Confidence: high.

### Follow-ups

7. **P2. Reasoning headroom is outside the product.** Gemma's passing load set `--reasoning-budget 512` and context 8192 through Lemonade `/api/v1/load` with `save_options: false` (`server-gemma-load.json`). The adapter body has model, messages, `max_tokens`, and optional `response_format` only. Settings have no reasoning field and the form has no timeout field (default 60 seconds; this success used 29.3). Add a portable note beside max output tokens, and a request field only where the provider documents one. Effort: M. Confidence: high.

8. **P2. Generation feedback is one sentence for the whole wait.** The draft emitter sends text only after the answer JSON string decodes, and only once (`service.py` lines 320–331). `reasoning_content` never becomes a draft. Cancel is present in `after-send-uia.json` and was not used. Desktop cancel kills the assistant process (`client/src/assistant.rs` lines 169–172), which the docs describe, and which also drops the in-memory receipt. Show elapsed time and "no answer text yet". Keep raw reasoning out of the panel. Effort: M. Confidence: medium, because no mid-Gemma frame was saved.

9. **P2. Always allow disagrees with the documented egress rule and with itself.** The docs say every remote request is reviewed and automatic egress approvals are disabled (`docs/assistant.md` lines 25–26). The button writes `approvals`, and remote `respond` skips the receipt check when that key is set (`service.py` lines 289–297). The same settings update clears the receipt and sets the connection back to untested (Sol `lib.rs` lines 832–840). This profile's `approvals` object is empty, so the control was not used. Remove it or make it a deliberate remote opt-in that does not destroy the open receipt. Do not auto-send. Effort: S. Confidence: high.

10. **P2. Review context from the composer abandons the explain task.** The composer always calls `ai_preview("ask", …)` (Sol `assistant.rs` lines 977–979). Explain attaches the execution only when the task is `explain` (lines 318–327), and it does not add that execution to the visible chips. A follow-up from the composer sends the cell alone under `ask`. Preserve the current task. Effort: S. Confidence: high.

11. **P2. "Evidence-checked" is stronger than the check.** The badge is unconditional (`assistant.rs` line 1378). The tooltip is the accurate sentence. `verify_citation` accepts a cell revision for kind `source` (`service.py` lines 378–381). The direct no-thinking diagnostic swapped kinds; that file is not a UI pass, and those ids would mostly come back unverified, which is a better outcome than a false verified mark. Show the badge only when every citation is verified, and keep the unverified-claims sentence. Reject an id that matches the wrong namespace. Effort: S. Confidence: high.

12. **P2. Failure announcements and keyboard recovery are thin.** Panel failures use a status role, seen as `ControlType.StatusBar`. Connection failure in settings uses an alert. There is no assistant key binding next to the palette entry "Ask assistant". During generation, focus was on Copy receipt while Cancel was enabled and unfocused. Several settings controls have empty names. A keyboard path and a screen-reader pass are still open. Move failure text to an alert without stealing focus from the editor, and add Cancel and Review shortcuts. Effort: M. Confidence: medium.

13. **P3. PDF and Word on the answer card export the notebook document.** `export_dialog` sends `export_document` with path and format only (Sol `interchange.rs` lines 112–120). `export_document` uses the assistant markdown only when that argument is supplied (`documents.py` lines 388–391). The buttons were not clicked in this session. Pass the reviewed answer, or label the buttons as notebook export. Effort: S. Confidence: high for the call path.

14. **P3. Destructive confirms are uneven.** Delete all AI data calls purge immediately. `run_cells` acceptance submits a runtime batch with no second confirm (`client/src/service.rs` lines 739–764). Neither was exercised. Keep a single Apply for a reviewed one-cell edit. Confirm run proposals and purge. Effort: S. Confidence: high for the code, low for frequency.

15. **P3. Failed requests still reserve the daily budget, and the UI does not say so.** `Store.used` sums `reserved`, including errors with empty usage. Four LFM failures reserved the receipt estimate plus 2,048 each, against a 100,000 cap, so this session did not hit the cap. Surface the reservation when a request fails. Effort: S. Confidence: high.

### Qualification gaps

- One exception type, one notebook, two local GGUF models, one successful native request.
- Apply, Reject, Regenerate, Cancel, purge, remote HTTPS, `allow_http_host`, policy `off` and `local_only`, and a second click on Approve were not exercised in the native UI.
- No narrow, dark, or high-contrast capture of this Sol session. No NVDA pass. No Linux Secret Service pass.
- Native stale payloads were not retained, so the mismatched field is inferred from the stable file hash and the error code.
- Execution count 5 was not re-queried from the ledger.
- Direct diagnostics must stay out of any "native pass" count.

## Preserve

- The explicit preview before send, and the rule that a changed model or destination invalidates the receipt.
- Hash-checked proposals that stay `proposed` until Apply, with Reject and a visible diff.
- The assistant process does not write notebook source or the runtime ledger. Apply goes through the existing mutation and batch contracts. Commands stay copy-only.
- OS credential storage, env stripping on the desktop child, and redaction of credential-shaped provider errors.
- Local versus remote labeling, HTTPS for remote hosts, and pinned resolution.
- Citation navigation that opens History or the cell, and prompt text that requires an unverified label on hypotheses.
- Fail-closed handling of schema-invalid proposals, the single-owner assistant store, and the fake-provider UI probe as a regression for "preview then approve".

## Changes that would backfire

- A second confirmation on every local one-line edit. The diff is the review. Extra confirms will train people to click through.
- Auto-apply when JSON validates. The no-thinking diagnostic was valid JSON and a useless edit.
- Raising the output cap without a bound, as a blanket fix for reasoning models. That spends the daily reservation and the 60s deadline.
- Turning structured output off because LFM failed. Gemma's valid proposal happened with the flag on. The flag needs to be visible, not removed.
- Showing raw reasoning traces in the panel. They are long, they can echo the prompt, and they hid the answer in the SSE diagnostic.
- Forcing remote-style friction onto every loopback request. Keep the local click. Spend the friction on a readable receipt.
- Treating the connection test as a model-quality gate, or treating this Gemma answer as a benchmark.

## Self-critique

The failed LFM attempts dominate the scores. The completed Gemma panel is better than a 5.7 average suggests, and a reviewer who weighted only that panel would land near 7 on hierarchy, citations, and proposals. Weighting only the failures would push errors and context toward 3. The mean is a blend, and one fixture can still be luck.

Visual clipping is the weakest visual claim. The image description says the provider-error line ends at "unsup"; the accessible name is complete and the control is 810 pixels wide. Color for the stale banner is on firmer ground because the screenshot description and the substring color rule agree.

Backend scores for persistence and proposal gates are from reading code and tests, not from re-running them. A bug in an untested apply path would not show up here. The live store was left alone, so "five executions, none applied" depends on the parent collector plus the file hash checked in this review.

I did not see a keyboard-only or screen-reader session, so the accessibility score should not be read as a WCAG result. I also did not open the other review.
