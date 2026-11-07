# Assistant reliability implementation

Implemented in the parent working tree on 2026-11-07. The Sol desktop candidate is
preserved and incorporated; the original candidate worktree was not edited. No commit
or release was made. Existing unrelated documentation changes remain in place.

## Build and provenance

- Base: `dd68f53c96f85299a19851db7ae6804eb2a05560`, plus the dirty Sol candidate and this increment.
- Pre-existing parent and Sol patches: `parent-before.patch`, `sol-baseline.patch`.
- Per-file baseline hashes and original worktree: `baseline.json`.
- Final source/build identity and verification: `final-verification.json`.
- Executable: `../../desktop/target/debug/kernelledger-desktop.exe`, Windows dev/unoptimized build.
- Python: repository `.venv/Scripts/python.exe`, Python 3.14.7; Windows 11 build 26200.

Run the app from this checkout with this Python environment. The desktop uses the
Python backend as well as the executable; copying the executable alone is not an installation.

## Delivered changes

| Review finding | Implementation and evidence |
|---|---|
| C01 capability status | Correct `result` column; independent cache/usage error handling; false retained and Unknown for missing/expired/corrupt observations. Real Store/probe/status regression. |
| C02 settings | Save merges provider options; native schema, deadline, context-window and explicit Lemonade direct-answer controls. Provider Save/reopen and request-option checks. |
| C03 partial answers | Validate envelope and proposals separately. Valid explanations survive optional invalid edits, with persisted warnings, including reload and report export. Malformed envelopes fail closed. |
| C04 recovery | Typed errors and explicit failure severity; consumed receipts cannot send again. Rebuild preserves task/question/context. Background status refresh cannot turn a failure green. |
| C05 qualification | Effective profile fingerprints and 24-hour observations; probe copy says SSE/one JSON response, not task quality. Repeated task matrix below. |
| C06 limits/reasoning | Optional configured context admission; explicit `enable_thinking: false` extension; actionable reasoning-only output-limit failure. No blanket parameter/model/server changes. |
| C07 export | Capture answer Markdown before PDF/Word dialogs; preserve citations and rejected-proposal warnings. Native file contents verified. Notebook export remains separate. |
| C08 citations | Target-found and direct approved-context counts; unresolved links disabled. No factual correctness badge or guessed namespace remapping. |
| C09 receipts | Readable source/error tracebacks; included/omitted/truncated counts; exact JSON/system messages and copy; fixed status/composer outside the long feed. |
| C10 history | Request-linked turns/citations; task-model selection; conversation replacement and latest-answer restoration, including warnings and applied proposal state. |
| C11 accounting | Probes use reservation/admission; scope explicitly notebook/provider/UTC day. Failed/cancelled reservations remain conservative; known numeric usage retained. |
| C12 transport | Shared deadline across DNS/connect/retry/headers/read; socket watchdog; cancel generation suppresses late events. Deterministic DNS/trickle checks and native cancel/recovery. |
| C13 applications | Durable application intent and stable runtime batch id; idempotent recording; legacy success reconciliation; interrupted effects become Needs review and cannot be blindly repeated. |
| C14 grants | Only notebook plus normalized URL grants apply; active Revoke removes equivalent scoped URLs and prepared receipts. Legacy grants ignored/removable. Docs/catalog/schema aligned. |
| C15 usability/accessibility | Native accessible button activation; answer document/paragraph/heading nodes; wrapping errors; reachable status/Cancel; deliberate bulk deletion. Native UIA/theme/narrow checks below, not NVDA certification. |
| C16 qualification | Isolated real-provider/native evidence and automated regressions recorded. Remaining release gates are explicit below. |

Assistant SQLite migration adds warnings and application intents, retaining existing
conversations. Runtime ledger/source formats are unchanged. The framed assistant API
remains v1 with additive metadata and `prune`/`approval.revoke` catalog entries.

## Automated validation

- Full Python suite: **276 passed, 141 skipped**, 353.08 seconds; `python-tests-final.xml`.
  This run preceded the final safe-usage/receipt-hash regressions and the small history-warning change.
- Final affected Python suite: **82 passed**; `assistant-tests-final.xml`. Includes the
  subsequently added metadata secret-filter and exact auto-approved receipt hash tests,
  plus persistence of partial-answer warnings through conversation reload.
- Rust workspace: **127 passed** (client 31, models 7, protocol 3, runtime 23, UI 63).
- Ruff lint/format, strict Pyright, Rust formatting, Clippy with warnings denied,
  generated assistant-contract checks and native build passed.
- Final Windows fake-provider UIA smoke: preview -> explicit approval -> answer,
  without network or source/runtime effects. See `final-verification.json`.

Full-suite skips cover optional/platform/qualification dependencies; they are not
evidence of qualification. The final affected checks supplement the full run rather
than claiming all tests were repeated after every final change.

## Real Lemonade task check

Server: `http://127.0.0.1:13305/api/v1`, Lemonade 2026.41.1.
Already-downloaded model: `Gemma-4-26B-A4B-it-GGUF`. Explicit profile: schema mode on,
`enable_thinking: false`, 2,048 output tokens, configured context 8,192, deadline 60 s.
Server was loaded at context 8,192 with `save_options: false`; this was a temporary
test setting, not a saved server-default claim. No model was downloaded.

| Task | Attempts completed | Latency seconds |
|---|---:|---|
| Explain ZeroDivisionError | 2/2 | 21.450, 20.139 |
| Explain NameError | 2/2 | 25.832, 22.494 |
| Explain ModuleNotFoundError | 2/2 | 19.805, 19.330 |
| Ask | 1/1 | 7.014 |
| Refactor | 1/1 | 7.309 |

Evidence: `qualification-022534/model-matrix.json`. All six Explain answers name the
recorded exception. Three also cite an execution UUID under the wrong `source` kind;
these correctly remain unverified. Suggestions such as assigning a placeholder or
catching an import exception do not establish the user's intended program semantics.
No matrix proposal was applied. This is a bounded local smoke profile, not a broad
model-quality certification or a before/after rating.

## Native checks

All effects used the isolated `qualification-022534/native-notebook` and profile.
Test processes were identified by PID/window ownership. Original user notebooks and
the separate Sol candidate worktree were not targeted.

1. Native provider Save/reopen, connectivity and context review: `native-setup-check.json`,
   `native-settings-uia.json`, `native-connected-uia.json`, `native-approved-receipt.json`.
2. Real Explain completion: `native-response.json`, `native-answer-uia.json`, `native-answer.png`.
   One incorrect citation remained unverified; app target resolution does not verify model prose.
3. Reviewed Apply changed only `1 / 0` to inspected `1 / 1`. No rerun: execution count
   stayed three and runtime stayed stopped. `native-application-check.json`, `native-applied.png`.
4. PDF and Word contain the captured answer/citation notes and exclude unrelated
   notebook cells: `native-export-check.json`, `native-word-export-check.json`.
   Files are inside the isolated notebook. Dialog filename automation used the default
   report filename; custom filename editing was not qualified.
5. Restart/conversation load restored the answer and kept the applied proposal's Apply
   control disabled: `native-restored-uia.json`, `native-restored.png`.
6. Cancel during real Summary generation, rebuild and a new explicitly approved Summary
   succeeded. Interrupted request stayed interrupted, no late answer replaced recovery,
   source stayed `1 / 1`, execution count stayed three. `native-cancel-recovery-check.json`.
7. Light, dark and high-contrast captures: `native-light.png`, `native-restored.png`,
   `native-contrast.png`. Narrow window approximately 800x600 logical pixels at DPI 192:
   `native-narrow.png`, `native-narrow-uia.json`. Assistant status/composer remain reachable;
   the adjacent notebook is narrow and some source/error rows clip, so broader narrow-layout
   qualification remains open.
   No measured contrast/conformance claim is made. Earlier captures show placeholder `?`
   separators; the final build uses plain punctuation.

Final binary smoke and source hashes are recorded separately so earlier captures are
not misrepresented as captures of an identical executable.

## Remaining qualification gates

NVDA/Narrator speech, focus announcements and a complete keyboard-only journey were
not qualified. UIA activation/headless focus checks are narrower evidence. High-scale
Compact/Comfortable matrix, broader providers/models/platforms, adversarial privacy
testing and exhaustive crash injection at every effect boundary remain release gates.
Application reconciliation deliberately shows uncertainty rather than claiming an
exactly-once transaction across separate stores. OpenAI Responses and cloud paths
were not exercised live. No release, new numerical UI score or universal model
support is claimed. Vision, autonomous execution and inline completion stay gated.

See [product notes](../../docs/assistant-reliability.md) and
[implementation plan](../../plans/assistant-reliability-draft.md).
