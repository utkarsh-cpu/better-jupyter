# Notebook assistant

The optional assistant connects the native desktop and CLI to a separate Python
process. It reads recorded notebook data, prepares a context receipt, calls a
provider, checks its response and citation targets, and records proposed changes.
Source edits are applied by the client through the existing revision-checked
helper. Runs use the supervisor's frozen hashes and durable request ids. Shell
commands are copy-only.

Open **Assistant** in the status bar, or **Ask assistant** in the command palette.
**Settings → AI** configures a model, endpoint, token budgets, retention, OS-stored
API key and notebook policy. Set an OpenAI-compatible endpoint such as
`http://127.0.0.1:11434/v1` for a local server, or
`https://api.openai.com/v1` for the OpenAI Responses adapter. Nothing downloads a
model automatically. Set the endpoint explicitly when switching provider types.
The adapter follows the official [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs?api-mode=responses)
and [Responses streaming](https://developers.openai.com/api/docs/guides/streaming-responses)
formats. Report export is advertised by helper revision 2's `document_reports`
capability while the desktop framing protocol remains version 1.

Select context cells, enter a question, and choose **Review context**. The receipt
shows the destination, model, redactions, truncated items, byte counts and token
estimates. Copying the full receipt includes the exact system prompt and messages.
**Approve context and send** authorizes that receipt once. A changed destination,
model or policy requires a fresh preview. Every remote request is reviewed;
automatic egress approvals are disabled. A LAN server is Remote. Remote HTTP needs
an explicit `allow_http_host` setting; HTTPS is the default requirement.

Cell menus offer Ask, Explain error, Suggest edit and Document. Add executions
from History detail to explain outputs or compare two runs. Checkpoint detail can
prepare a recovery explanation. The inspector also provides resume summaries and
reproducibility review. Citations open their recorded targets; unresolved citations
and uncited claims are labeled unverified. Citation existence does not prove that
the model's interpretation is correct.

Edit and insertion proposals show current and proposed source. Applying rechecks
source revisions. Multiple inserted cells publish as one source transaction.
Run proposals use stop-on-error batches with the proposed hashes. Changed cells
are refused. Every applied proposal records its resulting manifest revision or
durable runtime request id. The assistant itself cannot mutate notebook source,
write the runtime ledger, start a worker, or submit supervisor effects.

Answers can be exported as reviewed Markdown reports through the existing PDF or
Word exporter. Requests, receipts, conversations, proposals and applications are
stored separately in `.kl/assistant/assistant.sqlite`, with a single owner lock.
If the desktop owns these records, close that notebook before using the CLI to
apply a proposal. The runtime ledger and normal notebook/bundle exports do not
include assistant records. Conversations can be exported explicitly to Markdown.

## CLI

```sh
kl -C path/to/notebook ai configure --provider openai-compatible --model YOUR_MODEL --base-url http://127.0.0.1:11434/v1
kl -C path/to/notebook ai status
kl -C path/to/notebook ai test
kl -C path/to/notebook ai ask "Which source produced this result?" --cell first --receipt
kl -C path/to/notebook ai ask "Explain this cell" --cell first
kl -C path/to/notebook ai explain EXECUTION_ID
kl -C path/to/notebook ai compare EXECUTION_A EXECUTION_B
kl -C path/to/notebook ai summary
kl -C path/to/notebook ai proposals
kl -C path/to/notebook ai apply PROPOSAL_ID
kl -C path/to/notebook ai search "loss"
kl -C path/to/notebook ai export CONVERSATION_ID conversation.md
kl -C path/to/notebook ai purge
kl -C path/to/notebook mcp
```

Remote CLI requests require `--approve`; `--receipt` prints context and exits
without inference. `ai apply` applies source proposals; run proposals require the
desktop's runtime controls. `ai login --provider openai` reads a key with a masked
terminal prompt. `ai logout` removes the active provider's key. Windows uses
Credential Manager; Linux uses Secret Service through `secret-tool`. The desktop
uses pinned `keyring` 3.6.3 against the same credential identity.

`KL_AI_API_KEY`, or `OPENAI_API_KEY` for OpenAI, override the CLI's OS-stored key.
Keys exported into a shell can reach user code when that shell starts a supervisor;
`ai status` reports this condition. Desktop keys pass to the assistant in its stdin
hello, never argv, logs, notebook data, or the supervisor. Assistant redaction is
best effort: inspect receipts when source or outputs contain sensitive information.
Images and active MIME output bodies are omitted from text context.

## Implemented gates and limits

The implementation covers the A0/A1 pipeline and deterministic staleness,
execution comparisons, checkpoint context, reproducibility, resume summaries,
read-only MCP, FTS5 recorded-source search and reviewed report export. Profiling,
test generation, starter notebooks and training advice use the same reviewed cell
proposal path and versioned task prompts; they do not run automatically.

ChatGPT plan sign-in remains unavailable pending client registration and desktop
release eligibility. Vision input, inline completion, autonomous experiments and
MCP proposal tools remain disabled pending their capability/latency/effect gates.
Cross-worktree comparisons and optional embeddings are not yet exposed. The
OpenAI-compatible adapter uses collected context and does not require tool calls;
the connection probe records observed SSE/JSON support and marks unqualified
capabilities unavailable. Invalid structured responses fail closed for regeneration
instead of automatically sending additional unreviewed repair context.

Streaming transports handle SSE incrementally. For privacy, the UI receives the
complete decoded answer field as a redacted draft before final proposal validation;
raw JSON and partial secret fragments are not forwarded. Cancelling the desktop
request terminates its assistant process, preserving autosave and runtime polling.
The next use restarts the process; interrupted requests remain recorded.

Daily caps use conservative input/output reservations per notebook and provider,
on UTC days. Failed or cancelled requests remain charged because partial provider
usage can be unknown. Actual provider usage is retained when reported. Price
estimation and machine-wide aggregation are not qualified. Retention defaults to
90 days; purging deletes assistant records while leaving source and ledger intact.

Offline tests use a fake provider and local HTTP fixtures for both APIs, including
malformed streams, timeouts, cancellation, receipt enforcement and stale proposals.
The native interaction suite verifies preparation never sends or applies anything.
`scripts/assistant_ui_probe.py` exercises explicit approval against a fake provider
in a disposable Windows profile. `desktop/.cargo/config.toml` supplies larger
thread stacks for GPUI debug tests. Run Cargo checks from `desktop/` so its config
is loaded. Real-model quality, NVDA, Linux Secret Service and the
draft's model/latency qualification gates require separate manual validation.
