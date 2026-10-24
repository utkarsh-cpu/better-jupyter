# Assistant implementation checks

Local Windows validation, 2026-10-20:

- 53 assistant tests pass. HTTP fixtures exercise Responses and compatible chat
  streaming, bounded errors, retry, timeout and cancellation without internet calls.
- 79 assistant/source/document/desktop-contract integration tests pass.
- 57 Rust workspace tests pass, including shared assistant frames and native
  interaction checks that context preparation cannot send or apply anything.
- Ruff check/format, strict Pyright, Cargo fmt and workspace clippy pass.
- The desktop application builds. The native Windows smoke probe passes startup,
  recorded-output semantics, navigation, ten resize cycles, bar alignment and
  caption hit testing. The isolated assistant UI probe passes
  **preview → explicit approval → fake answer**, with source unchanged and no
  runtime ledger or provider network call.
- The broader Python suite produced 150 passes, 42 skips and three failures in
  standalone worker/interpreter selection. All three failures were reproduced
  using an extracted, untouched `HEAD` source tree with the same interpreter:
  `test_token_permissions_and_standalone_worker` and both parameterizations of
  `test_pip_installs_into_selected_standalone_environment`. This predates the
  assistant change. The four additional assistant regressions added afterward
  pass in the 79-test integration run.

The native probe caught and fixed offscreen accessibility activation of the Send
button and exposure of the answer text to UI Automation. These checks do not
qualify NVDA or replace a keyboard/screen-reader walkthrough. GPUI debug tests use
the larger test-thread stack configured in `desktop/.cargo/config.toml`.

The [implementation guide](assistant.md) lists intentionally unavailable features
and remaining live-model, platform and performance qualification. No API key or
real provider account was required for these checks.
