# KernelLedger AI assistant plan, draft 1

| | |
| --- | --- |
| Status | Draft 2026-10-20. Not authoritative. No AI code exists. |
| Tracks | Cross-track proposal. On acceptance the runtime plan publishes version 6 with sections 2–9 and 11–13 of this draft, and the frontend plan publishes its next version with section 10 (versioning rule 4). |
| Baseline | [Runtime plan](../PLAN.md) version 5, [frontend plan](../FRONTEND_PLAN.md) version 4. |
| Versioning | [Plan rules](README.md). Proposed frontend requirements use `F-AI-<nn>`, scenarios `FS-AI-<nn>`, open decisions `AD-<nn>` until adopted. |
| Source | The "Explore AI Features for the Project" review of 2026-10-19: 22 candidate features, 3 connection routes, a 5-step rollout. Section 2 decides every one. |

## 1. Outcome and boundary

KernelLedger gains an optional assistant that answers from the ledger. Every factual claim cites a cell id, execution id, checkpoint id, or source hash that the user can open. The differentiating question is: *which execution produced this result, what changed since, and which saved state can I continue from?*

The assistant reads. It proposes edits, cells, and runs. It never applies them itself. An accepted proposal travels the existing paths: source writes through the source service with `manifest_revision` (runtime A5.1), executions through supervisor `run` with `expected_sha256` and a `client_request_id` (runtime A8.3). Revision checks, trust checks, and the single-writer ledger keep their meaning unchanged.

The assistant is off until the user picks a provider. Local models, an API key, and an eligible ChatGPT plan are equal routes behind one adapter interface.

Out of scope: training or fine-tuning models, bundling model weights in the installer, hosting a KernelLedger cloud service, and live inspection of a worker namespace (frontend A15 gate stays closed).

## 2. Disposition of every reviewed recommendation

### 2.1 Connection routes

| Route | Decision | Phase |
| --- | --- | --- |
| Local models (Ollama, LM Studio, OpenAI-compatible) | Adopt. First-class, tested first because it needs no account and keeps data on the machine. | A0 |
| OpenAI API key | Adopt via the Responses API with Structured Outputs and streaming. | A0 |
| ChatGPT account (Sign in with ChatGPT, plan usage) | Adopt behind a gate. OpenAI's open-source route registers the client and issues OAuth credentials (OIDC with PKCE) for eligible Plus/Pro Responses requests. Identity scopes alone grant no inference. Requires KernelLedger to qualify as an open-source client (AD-03). | A2, gated |
| Other cloud vendors | Through the generic OpenAI-compatible adapter first. Native adapters only on demand. | A3 |

### 2.2 Features

| # | Feature | Decision | Phase | Change from the review |
| --- | --- | --- | --- | --- |
| 1 | Explain and fix errors | Adopt | A1 | None. |
| 2 | Ask your notebook | Adopt | A1 | Answers must cite ids; uncited claims are labeled. |
| 3 | Generate or edit cells | Adopt | A1 | None. |
| 4 | Compare runs and branches | Adopt | A2 | Deterministic diff first, model narrates. |
| 5 | Checkpoint recovery guide | Adopt | A2 | None. |
| 6 | Data exploration assistant | Adopt, changed | A3 | No new profiling contract. The assistant proposes a visible profiling cell; the user runs it; the assistant reads its recorded output. |
| 7 | Plot interpretation | Adopt | A3 | Vision-capable models only; image sent only when selected. |
| 8 | Experiment reports | Adopt | A3 | Uses existing `export_document` (PDF, Word). |
| 9 | Training run advisor | Adopt, conditional | A3 | Only from metrics the user recorded; no inference of missing metrics. |
| 10 | Anomaly detection | Adopt | A3 | Statistics flag; model explains flags only. |
| 11 | Reproducibility review | Adopt | A2 | Moved earlier: built from existing deterministic checks. |
| 12 | Inline completion | Adopt, gated | A4 | Local fill-in-the-middle model; latency gate. |
| 13 | Explain, refactor, document | Adopt | A1 | Moved earlier: same proposal path as feature 3 at low cost. |
| 14 | Test generation | Adopt | A3 | Proposals only. |
| 15 | Stale-state guidance | Adopt, split | A2 | Stale detection is deterministic and ships without AI; rerun-order advice is labeled approximate. |
| 16 | Notebook starter builder | Adopt | A3 | Proposes a cell set reviewed as one diff. |
| 17 | Semantic history search | Adopt | A3 | SQLite FTS5 keyword index first, optional embeddings later. |
| 18 | Resume-work summary | Adopt | A2 | Every sentence links evidence. |
| 19 | Environment and GPU helper | Adopt | A2 | Install commands shown, never run (F-SET-04). |
| 20 | Bounded experiment agent | Adopt, gated | A4 | Restricted effect set, budgets, per-step approval by default. |
| 21 | External assistant integration (MCP) | Adopt, split | A2 read-only, A4 proposals | Read-only tools need no provider layer, so they move earlier. |
| 22 | Voice interaction | Defer | — | Separate speech pipeline and accessibility work; revisit after A3. |

### 2.3 Cross-cutting recommendations

| Recommendation | Where it lands |
| --- | --- |
| Credentials outside the worker and checkpoints | Section 5.4, section 9 |
| User chooses exactly what leaves the machine | Section 6 (context receipt) |
| Record model, sources, proposals, accepted actions | Section 8 |
| Schema-validated responses, still checked by KernelLedger | Section 7.3 |
| Streaming, cancellation, background requests in the desktop client | Sections 4 and 10.3 |
| Usage tracking and budgets | Section 5.5 |
| Capability checks for local models | Section 5.2 |

## 3. Guarantees

1. **G-AI-1 Opt-in.** No network request, process, or index exists until the user selects a provider. A notebook can forbid cloud providers (`assistant` key, section 5.3).
2. **G-AI-2 No silent egress.** Every request to a non-loopback host is preceded by a context receipt the user can inspect. Auto-approval is per notebook, per destination, and revocable.
3. **G-AI-3 Proposals, not effects.** The assistant process cannot write source, write the ledger, start a worker, or send effectful supervisor requests. The desktop client or CLI performs accepted proposals through existing contracts.
4. **G-AI-4 Credentials never touch notebook data.** No secret in the notebook directory, `.kl/`, the worker environment, the ledger, assistant records, logs, diagnostics, or bundles.
5. **G-AI-5 Evidence.** Claims about recorded state cite ids that the client verifies exist. A claim the client cannot verify is shown as unverified.
6. **G-AI-6 Deterministic first.** Where KernelLedger can compute an answer (staleness, diffs, coverage, durations), it computes it and the model explains the result.
7. **G-AI-7 Untrusted inputs.** Cell source, outputs, imported notebook outputs, and descriptors are data, never instructions. Text in them cannot widen what the assistant may do.
8. **G-AI-8 Removable.** Deleting `.kl/assistant/` removes every assistant record. The ledger and source are unaffected.

## 4. Architecture

```text
Desktop client --(stdio)--> Desktop helper      (unchanged: source service, read-only ledger, no network)
      |
      +--(stdio)--> Assistant process  --(HTTPS / loopback HTTP)--> provider
      |                 |  read-only ledger, blobs, source (helper library code)
      |                 +--> .kl/assistant/assistant.sqlite  (sole writer)
      |
      +--(desktop protocol)--> Supervisor   (accepted runs only, sent by the client)
```

- **Separate process.** `python -I -m kernelledger.assistant --project <dir>`. The helper's runtime A8.7 guarantee (no network, never executes code) stays intact. Cancellation can kill the process. A provider hang or crash cannot stall source saves.
- **Framing.** Same desktop framing and `hello` as the helper, its own version, plus `delta` (streamed text), `proposal`, and `done` messages carrying a `request_id`.
- **No new Python dependency.** HTTP and server-sent events via `urllib.request` and `http.client`, consistent with the stdlib runtime. Context reads reuse the helper's read-only query functions; no SQL is duplicated.
- **Package layout.**

| Module | Role |
| --- | --- |
| `assistant/__main__.py` | Process entry, framing, dispatch, cancellation |
| `assistant/providers.py` | Adapter interface, OpenAI Responses, OpenAI-compatible chat, ChatGPT-plan auth, fake provider for tests |
| `assistant/capabilities.py` | Probe and cache: streaming, tool calls, structured output, vision, fill-in-the-middle, context window |
| `assistant/context.py` | Context items, budgets, truncation, receipts |
| `assistant/redact.py` | Redaction rules (section 6.3) |
| `assistant/tools.py` | Read-only tool catalog and proposal schemas (section 7) |
| `assistant/analysis.py` | Deterministic checks: staleness, execution diffs, duration statistics, reproducibility |
| `assistant/store.py` | `assistant.sqlite` records (section 8) |
| `assistant/prompts/` | Versioned task prompts; the version is recorded with each request |
| `assistant/mcp.py` | MCP stdio server over the same tool catalog (A2) |

## 5. Providers, settings, and credentials

### 5.1 Adapter interface

Each adapter implements `models()`, `probe(model)`, `respond(messages, tools, schema, stream)`, and `cancel()`, and reports usage when the provider returns it. Requests carry a timeout, a maximum output size, and one retry on retryable status codes.

| Adapter | Endpoint | Auth |
| --- | --- | --- |
| `openai` | Responses API | API key |
| `openai-compatible` | Chat completions at a user-given base URL (Ollama, LM Studio, vLLM, other vendors) | None or bearer key |
| `chatgpt-plan` | Responses API with OAuth access token | OIDC + PKCE through a loopback redirect; refresh token in the credential store |
| `fake` | In-process | None. Tests only, scripted responses. |

### 5.2 Capability probe

On first use of a model, a small structured request checks streaming, tool calling, JSON-schema output, and, if requested, image input. Results are cached per endpoint and model with a timestamp. A feature needing a missing capability is disabled with the reason ("This model does not support images"), never silently degraded. Local models without tool calling fall back to a single-shot prompt with pre-collected context.

A provider is labeled **Local** only when its host resolves to a loopback address. A LAN or remote OpenAI-compatible server is **Remote** and gets receipts.

### 5.3 Settings

- **Machine settings:** `%LOCALAPPDATA%\KernelLedger\assistant.json` on Windows, `$XDG_CONFIG_HOME/kernelledger/assistant.json` on Linux. Providers, base URLs, default model per task, budgets, retention. Never secrets. Versioned like `workbench.json`.
- **Notebook policy:** a new `.kl/config.json` key `assistant`: `"off"`, `"local_only"`, or `"allowed"` (default `"allowed"`, which still requires a configured provider). Validated like other config keys; the settings page edits it (F-SET-02).

### 5.4 Credentials

- **Desktop:** the Rust client stores API keys and refresh tokens in the OS credential store (Windows Credential Manager; Secret Service on Linux) through the `keyring` crate. It passes the credential to the assistant process in the `hello` over stdin, never in argv or environment. The assistant holds it in memory only.
- **CLI:** `KL_AI_API_KEY`, then `OPENAI_API_KEY`, from the CLI's own environment. `kl ai login` stores a key in Windows Credential Manager via `ctypes` (as token ACLs already do). On Linux it uses `secret-tool` when present, otherwise environment-only (AD-02).
- The supervisor never receives assistant credentials, so the worker cannot read them through KernelLedger. A key the user exported into their own shell environment is visible to user code as before; the docs say so.

### 5.5 Usage and budgets

Each request records input, output, and cached token counts when the provider reports them. Budgets: per-request maximum output, per-day token cap per provider, and an optional cost estimate from a user-editable price table (no prices hard-coded, AD-07). Exceeding a budget refuses the request with the reason.

## 6. Context selection and privacy

### 6.1 Context items

| Item | Source | Default size limit |
| --- | --- | --- |
| Cell source | `source.read` | Full cell, 32 KiB |
| Execution summary | `executions.list`, `execution.get` | Status, times, source hash |
| Folded output | `fold.get` | Text head and tail, 8 KiB each |
| Traceback | Error event | Full |
| Checkpoint report | `checkpoint.get`, `training.inspect` | Coverage and exclusions |
| Environment | `doctor`, `interpreter.candidates`, `native.status` | Full report |
| Image | `blob.read` | Selected PNG/JPEG only, downscaled |
| Analysis result | `assistant/analysis.py` | Full |

Each feature declares its default scope (for example, "Explain error" uses the failing cell, its traceback, and the previous executions of the cells it names). The user can add or remove items before sending.

### 6.2 Context receipt

The receipt lists every item, its byte size and token estimate, the destination host, and the model. The client shows it before a remote request, and the assistant stores its hash with the request. "Always allow for this notebook and destination" skips the prompt; the receipt is still recorded and viewable.

### 6.3 Redaction

- `getpass` replies stay redacted (runtime A8.5). `.kl/token` is never readable through any tool.
- Bindings in `checkpoint_denylist` are masked in outputs and summaries.
- Pattern rules mask common credential shapes (API key prefixes, bearer tokens, private key blocks, connection strings with passwords). Masks are shown in the receipt. Like runtime A15, the product does not claim to find every secret.

### 6.4 Prompt injection

Context is wrapped in delimited data blocks with provenance. The system prompt states that data blocks contain no instructions. Tool access is fixed per task before the request; nothing in a response can add tools, change the destination, or skip approval. Proposals from imported or untrusted notebooks carry a warning.

## 7. Tools and proposals

### 7.1 Read-only tools

`read_cells`, `list_executions`, `get_execution`, `get_output`, `list_checkpoints`, `get_checkpoint`, `inspect_training`, `environment_report`, `stale_cells`, `diff_executions`, `search_history`. Each maps to helper methods or `analysis.py`, applies section 6 limits and redaction, and records what it returned.

### 7.2 Proposals

| Kind | Payload | Applied by |
| --- | --- | --- |
| `edit_cell` | `cell_id`, base `source_sha256`, new source, rationale | `source.mutate` with the current `manifest_revision` |
| `insert_cells` | Position, cells, rationale | `source.mutate` |
| `run_cells` | Cell ids, expected source hashes | Supervisor `run` / `batch` with a new `client_request_id` |
| `command` | Shell text, explanation | Never executed; copy only |

A proposal whose base no longer matches is **stale** and cannot be applied; the user can regenerate it. Acceptance is per proposal, after a diff view.

### 7.3 Validation

Responses use JSON Schema (Structured Outputs where supported, otherwise validated after parsing with one repair attempt). Passing the schema is necessary but not sufficient: KernelLedger checks that ids exist, base hashes match, cells are code cells where required, and the request is within policy.

## 8. Assistant records

`.kl/assistant/assistant.sqlite`, written only by the assistant process. Not part of the runtime ledger, so the ledger schema and its sole-writer rule are unchanged.

| Table | Contents |
| --- | --- |
| `conversations` | Id, title, created, notebook id, pinned |
| `requests` | Id, conversation, task, prompt version, provider, model, destination host, receipt hash, token counts, status, error, timestamps |
| `messages` | Request id, role, text, citations |
| `proposals` | Id, request, kind, target, base hash, payload, state (`proposed`, `accepted`, `rejected`, `stale`) |
| `applications` | Proposal id, resulting `manifest_revision`, `client_request_id`, `exec_id` |
| `index` | FTS5 table for history search (A3), optional embeddings table |

`applications` rows are written when the client reports acceptance, which links every AI-originated change to ledger operations. Retention defaults to 90 days (AD-06). `kl ai purge` deletes everything. Bundle and ipynb exports exclude `.kl/assistant/`; an opt-in export of one conversation as Markdown exists.

## 9. Security summary for runtime A15

- New process with network access, isolated from the helper, supervisor, and worker.
- Credentials: section 5.4. Never logged; diagnostic exports omit the hello.
- Loopback detection for Local labels; TLS required for non-loopback hosts unless the user explicitly allows plain HTTP for a named host.
- OAuth: PKCE, state parameter, loopback redirect bound to `127.0.0.1` on an ephemeral port, closed after one callback.
- Agent mode (A4) may only request `run_cells` and `edit_cell`/`insert_cells`. Never `restart`, `stop`, `restore`, `continue`, `replay`, `gc`, `publish`, `pin`, imports, or interpreter changes. Checkpoints are not a sandbox, and the UI says so.

## 10. Desktop frontend proposals

### 10.1 Surfaces

- **F-AI-01** An **Assistant** tab in the right inspector region (frontend A3 has fixed regions, so no new dock). It holds the conversation, context chips, the receipt, streamed answers with citation links, and proposals.
- **F-AI-02** Cell actions: *Explain error* on failed outputs, *Ask about this cell*, *Suggest edit*, *Document*. History and checkpoint detail gain *Explain* and *Compare with…*.
- **F-AI-03** Proposal review reuses the "Copy recorded source into current cell" preview pattern (F-IA-22): side-by-side diff, Apply, Reject, Regenerate. Stale proposals show why.
- **F-AI-04** Citations open the cited cell, execution, or checkpoint with the standard Back behavior (F-IA-25).
- **F-AI-05** Settings → AI: providers, connection test, model per task, budgets, retention, per-notebook policy, receipt auto-approvals with revoke.
- **F-AI-06** The status bar shows the active provider with a Local or Remote label while a request runs, and a Cancel action.
- **F-AI-07** Command palette entries for every assistant action, each with an unavailable reason (F-IA-11).

### 10.2 Accessibility

- **F-AI-08** Streamed text is not announced token by token (F-A11Y-03). Start, completion, errors, and new proposals are announced. Focus never moves on stream updates.
- **F-AI-09** Diffs, receipts, and citations are in the accessibility tree and keyboard reachable. Reduced motion disables streaming animation, not streaming.

### 10.3 Client changes

- `crates/client`: an assistant process manager alongside the helper manager; `Command::Assistant { method, fields }` and `Command::AssistantCancel`; `Event::AssistantDelta`, `AssistantProposal`, `AssistantDone`, `AssistantError`. Background thread like the helper, with request ids for cancellation.
- `crates/client/src/preferences.rs` stays presentation-only; assistant settings live in `assistant.json` through the assistant process.
- New dependency: `keyring` (pinned) for credentials.
- `crates/ui`: `assistant.rs` surface, settings page, diff component reuse, shortcuts in `shortcuts.rs`.

## 11. CLI

| Command | Effect |
| --- | --- |
| `kl ai status` | Provider, model, capability cache, notebook policy, budget use |
| `kl ai login [--provider ...]` / `kl ai logout` | Store or remove a credential (section 5.4) |
| `kl ai test` | Connection and capability probe |
| `kl ai ask "<question>" [--cell id] [--exec id]` | Cited answer to stdout; `--receipt` prints the receipt and exits |
| `kl ai explain <exec-id>` | Error explanation and an `edit_cell` proposal printed as a diff |
| `kl ai apply <proposal-id>` | Apply through the source service, refusing stale proposals |
| `kl ai compare <exec-a> <exec-b>` | Run comparison (A2) |
| `kl ai summary` | Resume-work summary (A2) |
| `kl ai purge` | Delete assistant records |
| `kl mcp` | Read-only MCP server over stdio (A2) |

The CLI runs the assistant library in-process; it never starts the supervisor for read-only tasks (runtime A16 rule for `kl log` and `kl show`).

## 12. Protocol and contracts

Assistant methods: `providers.list`, `providers.test`, `models.list`, `context.preview`, `respond` (streams), `cancel`, `conversations.list`, `conversation.get`, `proposals.list`, `proposal.resolve`, `application.record`, `search` (A3), `purge`. `schemas/desktop-assistant.schema.json` is generated from a catalog in the style of `desktop/contracts.py`. Fixtures in `tests/fixtures/desktop/assistant/` are consumed by Rust tests (runtime A8.8).

## 13. Delivery phases and exit gates

### A0 Foundation

Assistant process and framing, adapters (`openai`, `openai-compatible`, `fake`), capability probe, settings files, credential storage, context items and receipts, redaction, `assistant.sqlite`, budgets, cancellation, `kl ai status|login|logout|test`, Settings → AI, connection test UI.

Exit: all tests pass offline against the fake provider and a stdlib `http.server` fixture emulating both APIs, including streaming, malformed responses, timeouts, and cancellation mid-stream. Manual qualification with one Ollama model and one OpenAI model. A test proves no credential appears in `.kl/`, logs, the worker environment, or a bundle.

### A1 First useful release

Explain and fix errors, ask your notebook, generate or edit cells, explain/refactor/document. Assistant tab, cell actions, proposal review, citations, `kl ai ask|explain|apply`.

Exit: evaluation set (section 14) passes its thresholds on the qualified models. Stale proposals are always refused. Every answer's citations resolve or are marked unverified. Keyboard-only and NVDA walkthroughs complete.

### A2 Ledger-native value

Compare runs and worktree branches (`git_state`), checkpoint recovery guide, resume-work summary, deterministic stale-cell indicator (also shown without AI in the sidebar), reproducibility review, environment and GPU helper, read-only `kl mcp`, ChatGPT-plan adapter if AD-03 passes.

Exit: deterministic analyses have unit tests independent of any model. Comparisons separate observed differences from suggested causes in the rendered output. MCP tools are verified against at least one MCP client.

### A3 Deeper analysis

Data exploration through proposed profiling cells, plot interpretation, experiment report drafts into `export_document`, test generation, starter builder, history search (FTS5, then optional local embeddings), anomaly flags, training run advisor, other vendors through `openai-compatible`.

Exit: report drafts export to PDF and Word through the existing path. Search returns results with ids on a 1,000-execution fixture within the frontend A13 interaction targets. Vision features are disabled with a reason on non-vision models.

### A4 Automation and latency-sensitive editing

Bounded experiment agent (step, time, and token budgets; per-step approval by default; optional auto-approval of `run_cells` within a budget), MCP proposal tools, inline completion with a local fill-in-the-middle model.

Exit: agent never issues a forbidden method (fuzzed against scripted adversarial responses). Inline completion meets a measured p95 latency target on the reference machine (AD-05) or stays disabled.

### Deferred

Voice interaction. Revisit after A3 with a speech-pipeline and accessibility plan.

## 14. Evaluation

- A versioned set of fixture notebooks under `tests/fixtures/assistant/`: common tracebacks, stale cells, partial checkpoints, two-run comparisons, worktree divergence, environment failures, and notebooks with injected instructions in outputs.
- Automatic checks on every response: schema validity, citation resolution, proposal applicability, receipt fidelity (nothing sent beyond the receipt), and refusal of injected instructions.
- Model-quality checks (does the fix resolve the error when run) are manual or nightly against qualified models, never in offline CI. Results record model, prompt version, and date.

## 15. Open decisions

| Id | Decision | Options | Closes in | Default |
| --- | --- | --- | --- | --- |
| AD-01 | Assistant process scope | One per notebook, one per application | A0 | One per notebook, matching FD-07 |
| AD-02 | Linux CLI credential storage | `secret-tool`, environment only, encrypted file | A0 | `secret-tool` if present, else environment |
| AD-03 | ChatGPT plan usage | Register as an open-source client, skip | A2 | Register if the repository is public and OpenAI's terms are met; otherwise skip |
| AD-04 | Recommended local models | Named list per task and memory size | A1 | Documented list, user-chosen, nothing downloaded automatically |
| AD-05 | Inline completion target | p95 latency, model class | A4 | 300 ms p95 or disabled |
| AD-06 | Record retention | Days | A0 | 90 days |
| AD-07 | Cost estimates | User price table, none | A0 | User-editable table, empty by default |
| AD-08 | Embedding search | Local embedding model, provider embeddings, FTS5 only | A3 | FTS5 only |

## 16. Adoption

- **Runtime version 6** adds: an Assistant section (sections 3–9, 11–13 here), the new process in A3 (System shape), `.kl/assistant/` in A4 (Files), the `assistant` config key, assistant credentials and egress in A15, `kl ai` and `kl mcp` in A16, the assistant protocol version in A17. Affected frontend requirements: F-SET-01, F-SET-02, F-IA-07, F-A11Y-03.
- **Frontend next version** adds F-AI-01 to F-AI-09, an "Assistant" row in A3, and an Assistant phase gate, with `Requires` naming runtime version 6.
