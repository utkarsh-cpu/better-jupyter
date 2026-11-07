# Explain error with Lemonade — native Sol UI test

Tested 2026-11-06 in the already-running Sol candidate, PID 13204. **The native Explain error workflow passed with Gemma 4 26B after provider configuration. LFM2.5 did not pass the same fixture.** This qualifies one recorded division-by-zero example, not general model reliability.

## Successful native test

- Server: Lemonade 2026.41.1, `http://127.0.0.1:13305/api/v1`.
- Provider: `openai-compatible`, model `Gemma-4-26B-A4B-it-GGUF`, already downloaded.
- Notebook: `artifacts/ui-candidates-2026-11-05/candidate-a/Sales-analysis-notebook`.
- Selected cell: `error`, source `1 / 0`.
- Recorded execution: `3b2ee1d5-4ecc-4f18-ae35-5acdf03beaf3`, ordinal 5, `ZeroDivisionError: division by zero`.
- Request: `b979370a5446434995ac92592c583397`, persisted status `done`, no error.
- Native request latency: **29.300 seconds**, excluding model loading and connection testing.
- Reported usage: 1,847 prompt tokens + 882 completion tokens = 2,729 tokens.

The actual UI sequence was provider setup → Test connection → failing cell's Explain error → inspect/copy context receipt → Approve context and send. The receipt included only the selected cell, its recorded execution metadata and its error output/traceback, plus the system prompt/schema. The destination was loopback. No API key was entered.

The answer correctly identified division by zero in `1 / 0`, separated observed evidence from a labeled hypothesis, and proposed changing the denominator to 1. Both citation targets were verified by the application: cell `error` and the recorded execution. Clicking the execution citation opened History at execution 5; clicking the cell citation returned to the error cell and focused its editor.

The UI displayed the answer and a current/proposed diff, `1 / 0` → `1 / 1`, with Apply, Reject and Regenerate controls. The proposal carries the correct current source hash, `95baf036f1aa2733423c14f202dbee17b0d6c29230f6484fd711c678cd64635a`. It remains **proposed and unapplied**. This change removes the fixture's exception; it is not a general fix for an unknown calculation's intended semantics.

Twelve explicit checks passed in `checks.json`. Notebook source and manifest hashes match the before-send snapshot, runtime executions remain 5, and the assistant applications table remains empty. The runtime stayed stopped. No repository implementation was changed.

## Failures retained in the test record

Four native LFM2.5 requests preceded the successful Gemma request. Every attempt used the same source/execution/traceback and the default Explain error question. LFM requests had the same receipt hash; only the model and receipt hash changed for Gemma.

| Native attempt | Model / configuration | Result | Seconds |
|---|---|---|---:|
| 1 | LFM2.5, default compatible provider | `provider_error`: incomplete response; 2,048 output tokens reached | 15.745 |
| 2 | LFM2.5, schema enforcement enabled | Same incomplete-response failure | 15.904 |
| 3 | LFM2.5, schema enforcement, reasoning budget 0 | `stale`: returned proposal failed cell/revision validation | 2.912 |
| 4 | LFM2.5, schema enforcement, reasoning budget 1,024 | Same proposal-validation failure | 9.509 |
| 5 | Gemma 4 26B, schema enforcement, reasoning budget 512 | `done`, correct diagnosis, two verified citations, valid edit proposal | 29.300 |

Additional direct-provider diagnostic calls are saved separately and **are not counted as native UI passes**. Raw SSE inspection showed LFM spending a 512-token diagnostic allowance on `reasoning_content`, with no final answer content and `finish_reason: length`. A separate no-thinking diagnostic returned schema-valid JSON but incorrect citation types and an unchanged `1 / 0` edit. This supports treating LFM quality as unresolved rather than equating connectivity or JSON validity with task success.

## Configuration left for the user

The Sol UI remains open with the successful answer and unapplied proposal. Its isolated profile now selects Gemma and the local Lemonade endpoint. Settings live under:

`artifacts/ui-candidates-2026-11-05/candidate-a/profile/KernelLedger/assistant.json`

The existing supported provider option `providers.openai-compatible.structured_output` was set to `true` in that profile file. The native settings form has no control for it. Maximum output tokens remains 2,048.

The loaded Gemma backend has context size 8,192 and `--parallel 1 --reasoning-budget 512`. These were supplied through `/api/v1/load` with **`save_options: false`**; they are active for the current loaded model and were not persisted in Lemonade's recipe options. The previous LFM backend is no longer loaded. No model downloads were needed.

## Findings to address

1. **Expose reasoning controls or explain exhausted output budgets.** The compatible adapter does not send a reasoning toggle/budget. LFM's final answer was starved by reasoning, but the UI showed only “provider response was incomplete or unsupported.” The connection test passed before this failure, so its success should not imply the selected model is qualified for Explain error.
2. **Expose and preserve schema enforcement.** The adapter supports `structured_output`, but the native provider form does not expose it. Its Save compatible handler replaces the provider object with only `base_url`, dropping the manually enabled option. See the Sol worktree's `desktop/crates/ui/src/assistant.rs` around the provider-save handler and `src/kernelledger/assistant/providers.py` for request construction.
3. **Make proposal-validation failures easier to understand.** The UI says “proposal source has changed or cell is missing” even though the notebook source did not change in this test. An invalid model-supplied target/hash can produce that message too. Keep the validation gate, but distinguish an invalid generated proposal from a source revision that actually changed.
4. **Use a red error treatment for stale failures.** The captured `respond: stale` banner had a green border/dot. Long provider errors were clipped horizontally. Both made troubleshooting harder.

These are test findings, not fixes made in this turn. Acceptance/application, real reruns, cancellation, network failures and other exception classes were not exercised.

## Evidence

- `results.json`: read-only snapshots of native request records, accepted answer, verified citations, proposal, unchanged source hashes, execution counts and current server state.
- `checks.json`: twelve successful end-to-end checks.
- `gemma-receipt.json`: exact approved context for the successful request.
- `gemma-connection-uia.json`: native connection success.
- `gemma-result-uia.json`, `gemma-result.png`: completed answer/proposal UI.
- `citation-target-uia.json`, `final-settled-uia.json`: citation navigation.
- `first-request-failed-uia.json`, `generating.png`, `no-thinking-result.png`: failed native attempts; `generating.png` was captured after the first request had already failed.
- `server-*-load.json`: exact transient backend configuration requests.
- `ui_control.py`: interaction/capture helper restricted to the launched Sol window.
- `collect_results.py`: repeatable read-only evidence collector; does not acquire a second assistant-store writer.

Primary documentation consulted: [Lemonade model loading and non-persistent options](https://lemonade-server.ai/docs/api/lemonade/) and [Lemonade chat/reasoning API](https://lemonade-server.ai/docs/api/openai/). The installed llama-server's `--help` confirmed support for `--reasoning-budget`.
