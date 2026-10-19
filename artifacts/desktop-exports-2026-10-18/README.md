# Desktop shortcuts and document export verification

These captures come from the rebuilt native Windows app using an isolated
notebook/profile. `forecast.pdf` and `forecast.docx` were produced by the native
Export menu and save dialog. Their source, recorded output and recorded error
content were checked. `pdf-page.png` renders the first exported PDF page.

- `export-menu-final.png`: final Export button and format chooser.
- `commands-shortcuts.png`: shortcut hints in Commands.
- `word-complete.png`: export completion with Open file and Show in folder.
- `forecast.pdf` / `forecast.docx`: example exports from the disposable fixture.

Validation: 40 Rust workspace tests passed, including 26 UI tests. Clippy and
Rust formatting passed. Seven Python export tests passed, including Unicode,
long-cell pagination, images, stale-output labels, source-only/empty notebooks,
and invalid-format destination preservation. Changed Python files pass Ruff,
formatting and Pyright.

The full Python suite had 134 passes, one skip, and one unrelated GPU failure:
`test_training_capture_candidate_failure_and_exact_next_step[cuda:0]` reports
`CUDA error: device kernel image is invalid` / `hipErrorInvalidImage`.
Repository-wide formatting also finds existing issues in `blobs.py`,
`supervisor/__init__.py`, and `test_desktop.py`; Pyright finds the existing
`Supervisor._admitted_to` attribute error. Those files were not changed here.

The native fixtures and their runtimes were closed after verification.
