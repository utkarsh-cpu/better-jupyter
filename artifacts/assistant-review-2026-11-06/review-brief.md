# Independent review: KernelLedger assistant UI and backend

User request: start Grok 4.7 and GPT 6.1 Sol subagents to rate the assistant UI and backend and identify the shortcomings we need to cover. Perform a substantive, evidence-based review, not implementation. The other agent receives this identical brief. Do not read its review or scores before submitting yours. Do the work yourself; do not spawn more agents.

## Source of truth and scope

The application under review is the **Sol UI candidate**, not the older main-worktree desktop UI. Your managed worktree is for isolated review artifacts and any safe scratch work. It will not contain the Sol candidate's dirty implementation or the parent's untracked evidence. Read the absolute locations below. Do not create another worktree yourself.

- Parent repository and active Python backend: `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6`
- Sol desktop source: `C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-562311ac/desktop`
- Assistant UI: Sol `desktop/crates/ui/src/assistant.rs`, `lib.rs`, related settings/input/status code.
- Desktop assistant transport: Sol `desktop/crates/client/src/assistant.rs`, related desktop process/credential handling.
- Backend: parent `src/kernelledger/assistant/`, including `context.py`, `providers.py`, `capabilities.py`, `service.py`, `settings.py`, `store.py`, `tools.py`, and relevant source/proposal/runtime boundary code.
- Documentation: parent `docs/assistant.md`; relevant tests and `scripts/assistant_ui_probe.py`.
- UI skill: parent `.agents/skills/ui-ux-pro-max/SKILL.md`. Apply relevant desktop/UX guidance with judgment. This app uses native GPUI/Rust and Python, not web ARIA/CSS. Do not blindly impose web/mobile font-size or touch-target defaults on an expert desktop notebook.

The native Sol app is already running for the user, PID 13204 / HWND 198202 at the time the evidence was collected. **Do not manipulate, close, relaunch, resize, or send input to it. Do not change its profile, model, server, notebook, or source.** Both reviewers must avoid live UI/server contention. Use the saved screenshots, UIA snapshots, code and read-only records. Do not send model requests, download/load models, read credentials, or apply/run proposals. Do not start a second writer against the live assistant store. SQLite inspection, if needed, must be `mode=ro`.

## Evidence pack

Absolute directory: `C:/Users/UTKARSH KAUSHIK/Documents/Projects/KernelLedger-v6/artifacts/lemonade-explain-test-2026-11-06`

Start with the actual PNGs and raw records. Then read `report.md` as the parent's test log and hypotheses to audit, not an authoritative review:

- `gemma-result.png`, `final.png`: completed assistant answer, verified citation chips, current/proposed source diff, unapplied proposal controls.
- `generating.png`: despite its filename, captured after the first incomplete-response failure; includes the context receipt and provider-error banner.
- `no-thinking-result.png`: stale-proposal validation failure with its actual banner treatment.
- `gemma-preview-current.png`: context-review UI and selected model.
- `provider-settings-uia.json`, `configured-uia.json`, `gemma-connection-uia.json`: provider/settings controls and connection status. There is no complete screenshot of every settings state.
- `gemma-result-uia.json`, `final-settled-uia.json`, `citation-target-uia.json`, `first-request-failed-uia.json`: native semantics/state/navigation.
- `results.json`, `checks.json`, `before-send.json`, `gemma-receipt.json`, `approved-receipt.json`: exact request records, outgoing context, answer/citations/proposal, source hashes and unchanged execution counts.
- `server-*-load.json`, `server-before-reload.json`: actual transient server setup.
- `direct-no-thinking-diagnostic.json`, `direct-schema-diagnostic.json`, `raw-sse-diagnostic.json`: separate direct-provider diagnostic calls; never represent these as native UI passes. Do not reproduce raw reasoning traces in your report.
- `ui_control.py`, `collect_results.py`: capture/inspection procedures; inspect as needed, do not run UI interactions.

Lemonade was at `http://127.0.0.1:13305/api/v1`, version 2026.41.1. Four native LFM2.5-8B-A1B requests failed (two output-limit/incomplete-response failures, two proposal revision/target validation failures). A native Gemma-4-26B-A4B-it-GGUF request completed in 29.3s, with two verified citations and a valid `1 / 0` → `1 / 1` edit proposal. Source stayed unchanged, five runtime executions remained five, and no proposals were applied. The runtime stayed stopped. This is one three-cell fixture and one exception type; it cannot establish model reliability, full accessibility, or production readiness.

Supported `structured_output: true` was manually enabled in the isolated profile because the form has no toggle. Model reasoning budgets were set through transient Lemonade load options, not persisted. Independently verify the suspected shortcomings instead of assuming the test log's diagnoses are correct. Distinguish model limitations, application defects, missing capabilities, and untested areas.

## Review method and deliverables

Inspect PNGs as real images using an image-capable tool. Explicitly state which images you actually saw. If raster viewing fails, disclose it; do not invent visual inspection or visual scores. Use `null` for dimensions you cannot substantiate. UIA roles/names alone do not certify accessibility. Static screenshots alone do not certify loading/cancel/focus/hover behavior. Backend conclusions need concrete file/line evidence or recorded behavior. Safe focused tests may be used if necessary, but no writes to the parent's implementation, live notebook/store/profile or server. Clearly distinguish tests run by you from existing test evidence.

Rate each of these dimensions **1–10**, using half-point increments if useful, with a short rationale and high/medium/low confidence. Treat 5 as substantial friction/unfinished behavior, 7 as usable with meaningful gaps, 9 as robust and well qualified. Do not inflate scores from the single successful demonstration. Compute the exact unrounded arithmetic mean for each group; show the denominator and use half-up rounding to one decimal for display. Exclude nulls explicitly; never treat them as zero or claim a full-group mean if coverage is incomplete.

UI/UX dimensions (10):
1. Provider onboarding and configuration
2. Readability and visual hierarchy
3. Context review and destination clarity
4. Conversation and task workflow
5. Generation feedback and cancellation UX
6. Error clarity and recovery
7. Evidence and citation UX
8. Proposal review and user control
9. Keyboard and accessibility support
10. Responsive layout and space use

Backend dimensions (10):
1. Provider and protocol compatibility
2. Schema support and model capability qualification
3. Context grounding and citation validation
4. Proposal validation, staleness, and effect boundaries
5. Reasoning, token budgets, and deadlines
6. Streaming and cancellation handling
7. Persistence and concurrency
8. Privacy, credential handling, and egress controls
9. Failure diagnostics and recovery
10. Test coverage and release readiness

Deliver:
- A score table for each group and justified UI/backend group means, with coverage/confidence limitations.
- A prioritized backlog: the 10–15 most important concrete shortcomings, plus a compact list of remaining qualification gaps. For each main item include severity P0/P1/P2/P3, observed trigger, user consequence, evidence (absolute file path + line or artifact), recommended change, acceptance check, estimated S/M/L effort, and confidence. Assign P0 only to demonstrated critical blockers.
- Separate a first release-blocking slice from follow-ups. Include features already implemented that should be preserved, and recommendations that could backfire for expert users.
- Audit the parent's test conclusions, distinguish verified citation existence from factual truth, and identify where evidence is insufficient. No claims that all 12 checks prove general quality.
- A short self-critique identifying likely score bias or uncertainty.

Write **only** `artifacts/assistant-review-2026-11-06/<reviewer>-review.md` and `<reviewer>-scores.json` inside your own managed worktree, with `<reviewer>` supplied by your launch task. The JSON should retain model name, image-viewing disclosure, dimension scores/confidence/rationale, exact means, and limitations. Do not commit or change implementation. Finish with the group ratings, five highest-priority findings and absolute output paths.
