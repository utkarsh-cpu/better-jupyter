# Assistant reliability increment

This working-tree increment implements the [reliability plan](../plans/assistant-reliability-draft.md)
on the preserved Sol desktop candidate. It retains GPUI, explicit reviewed effects,
the separate assistant process/store and the existing notebook/runtime formats.

## Configuration and qualification

Settings exposes compatible-server schema enforcement, a request deadline, an optional
configured context window, and an explicit Lemonade direct-answer extension. Save
merges provider options, retaining fields the form does not edit. Selecting the
Lemonade preset fills `http://127.0.0.1:13305/api/v1`; it does not load/download a model.

The direct-answer option sends `enable_thinking: false` only when explicitly selected
for an OpenAI-compatible provider. This is a [Lemonade extension](https://lemonade-server.ai/docs/api/openai/),
not a portable promise about every compatible server. Unchecking removes the option
and lets the server use its default. A configured context window is an estimate for
admission, not an observation of the running server's actual context size.

Effective profiles identify endpoint, actual task model, schema mode, output/context
allowances, deadline and optional thinking setting. Receipts bind the profile;
changing it requires rebuilding context. Model overrides are displayed for the
active task. Connection probes are cached against the effective profile for 24 hours.
Unprobed, expired or unreadable observations remain Unknown. A passing probe means
SSE plus one schema-shaped JSON response, not useful/correct answers on notebook tasks.

Connection probes reserve tokens before inference, just like ordinary requests.
The displayed cap is per notebook/provider and UTC day. Reservations include failed
or cancelled inference and are intentionally not a billing total. Known provider
usage and safe finish metadata are retained when available. Status-cache errors do
not erase known reservations. No provider body, credential or raw reasoning trace
is stored for troubleshooting.

## Responses and recovery

The service validates the response envelope before individual proposals. Malformed
JSON/envelopes fail closed. A valid redacted answer and citation results can complete
with proposal warnings. Invalid shapes, unsupplied targets/base hashes, no-op edits
and changed source remain inapplicable. The service never guesses a replacement hash.

Length, refusal, unsupported tool calls, interrupted transport, deadline, budget and
configuration failures carry distinct codes. Native errors use explicit failure
state rather than searching their text for the word error. A failed send consumes
its receipt; **Rebuild context** preserves the question/task/context without sending.
Status/Cancel stay outside the long scrolling receipt. Elapsed time is shown while
busy. Late events from a process cancelled by the desktop are suppressed.

Address resolution has a bounded wait. HTTP connection, retries, headers and stream
reads share a deadline; the watchdog interrupts a trickling/blocked socket. Desktop
Cancel still terminates only the isolated assistant, preserving source and runtime
polling. Raw reasoning is not forwarded as progress or answer text.

## Evidence, reports and history

Receipts separate selected items from included records, omissions and truncations.
Cell source and exception/traceback content are rendered readably; exact payload and
system messages remain inspectable and copyable. These presentation changes do not
rewrite the approved messages.

Citation indicators report target existence and direct approved-context membership,
not factual truth. Incorrect citation kinds remain unverified, and unresolved links
are disabled. Full ids remain in accessible names/hints behind shorter visible labels.
The main answer exposes native document/paragraph/heading semantics rather than one
long group name. This is not a screen-reader conformance certification.

Answer PDF/Word actions capture the reviewed answer and citation notes before opening
the file dialog. They send that Markdown through the helper's report path. Notebook
export remains separate. Exporting does not apply or rerun a proposed change.

Turns retain request ids and citation metadata. Switching conversations replaces the
current answer, proposals and task; the latest answer is rehydrated with report
actions. History is bounded to 200 requests/400 messages and discloses its limit.
Connection-test bookkeeping does not clutter the conversation list.

## Application records and migration

Only the client/CLI can apply reviewed effects through existing revision/runtime
contracts. A new assistant-store table records application intent and stable identity
before the effect. Run batches use that durable identity. Success records the result
once; repeating application recording does not repeat the effect or create duplicates.

An uncertain outcome is **Needs review**, not Applied. Restart marks interrupted
intent unknown, and the same proposal cannot be blindly applied again. A source
conflict remains refused by the helper. Known legacy successful records are reconciled
to Applied when their recorded manifest/runtime identity is present. This is not an
exactly-once transaction across source, runtime and assistant stores.

Migration adds request warning storage and application intents under the existing
single-owner lock. Existing conversation/proposal records are retained; no source or
runtime-ledger schema changes occur. The framed assistant API stays v1 with additive
result metadata and catalog/schema entries for pruning and scoped grant revocation.

New automatic grants are interpreted only for a notebook plus normalized endpoint.
Revoking the active destination removes equivalent scoped URL spellings. Broader
legacy grants are ignored rather than silently widened; the settings list allows
their explicit removal. Deleting all assistant data in the UI requires the labeled
confirmation action. Ordinary reviewed edits/runs do not gain extra confirmation loops.

## Verification scope

Evidence and the final build identity live in
`artifacts/assistant-implementation-2026-11-07/`. Automated regressions cover real
Store/probe/status round trips, invalid proposals with retained explanations,
receipt/profile changes, context omissions, scoped grants, application recording
and interrupted/legacy application states, safe finish metadata, DNS and trickling
deadlines. Native component tests cover typed failure/rebuild and conversation state.

The owned Windows/Lemonade checks include repeated Explain requests for three
recorded exceptions, Ask/Refactor, native provider Save, explicit preview/send,
reviewed Apply without rerun, and native PDF/Word answer-content verification.
Some model citations were incorrect and correctly remained unverified. Useful-looking
edits are still suggestions; these checks do not establish intended program semantics.

NVDA speech/focus qualification, broad provider/model/platform reliability, adversarial
privacy qualification and exhaustive crash fault injection remain release gates.
Vision, autonomous execution, inline completion and other previously gated features
stay unavailable. See the [evidence report](../artifacts/assistant-implementation-2026-11-07/implementation-report.md)
for exact attempts, skips and limitations.
