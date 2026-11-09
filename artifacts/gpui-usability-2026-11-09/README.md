# Native usability follow-up — 9 November 2026

These macOS Rust/GPUI captures use an isolated notebook/profile and no provider
credentials. They document three fixes from the hands-on UI review. This is a
short native verification, not release qualification.

- [Compact editor, 800 × 600](compact-editor-800.png): with Assistant open, run,
  execution count and output controls occupy the toolbar above source. Both
  calculation lines fit in the editor. The two-column split remains resizable.
- [Local provider, 800 × 600](provider-local-800.png): provider selection, endpoint,
  model, and one primary Save configuration action fit in the basic form. Reload,
  model discovery and compatible-server credentials live in Advanced settings.
- [OpenAI draft, 800 × 600](provider-openai-800.png): selecting OpenAI restores its
  saved URL or official default and exposes credentials. Selection changes only
  the draft. No settings save, credential write or provider call was performed.
- [History search, 1280 × 800](history-auto-search-1280.png): applying `needle`
  examines 242 executions and reaches the seeded execution 225 without clicking
  Next. Previously the user had to advance two empty pages.

Search continuation runs on the notebook service thread with either helper.
Each helper page retains the existing 100-candidate and 1 MiB/source limits.
The client stops after a page has matches, the cursor ends, ten pages, or the
250 ms budget checked between requests. It aggregates scanned and source-limit
counts. Continue search resumes a bounded empty search; completed searches show
an explicit no-match message. Ordinary populated pages retain Next and Back.
This is a time check between calls, not a timeout that interrupts a helper call.

Regression validation: 147 Rust tests pass (client 33, models 7, protocol 3,
runtime 24, UI 80); 302 Python tests pass, 142 skip. Clippy with warnings denied,
Rust formatting, Ruff lint/format, and Pyright using the project venv pass.
Desktop and daemon debug builds succeed. The toolchain still emits the existing
cache-cleanup and rust-objcopy/libLLVM build warnings; neither blocks compilation.

New regressions cover automatic empty-page continuation with preserved query and
session scope, aggregate source warnings, cursor/end/match/page-budget stop
conditions, a minimum editor width in the compact split, provider selection
without side effects, saving the selected endpoint without losing the other
provider's options, and preserving a provider draft through unrelated settings
updates. Existing backend tests cover bounded source lookup and previews.

The screenshots precede the final provider-draft completion guard, which does
not change rendering and is covered by the provider settings regression.
VoiceOver, IME, configured provider responses, long-running rendering, other OSes,
and signed packaging remain outside this follow-up. The fixture app and worker
are stopped after verification.
