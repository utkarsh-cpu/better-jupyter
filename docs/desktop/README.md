# Desktop development

This is a native Rust GPUI Kit development application over runtime plan 4 and
frontend plan 3. Opening source/history uses an offline helper and starts no worker.
The application closes by detaching; Stop runtime is a separate action.

```powershell
python -m pip install -e ".[dev]"
cd desktop
cargo run --locked -- ../analysis
```

Windows builds require MSVC C++ tools and a Windows SDK. `KL_RUNTIME_PYTHON`
selects the installed KernelLedger Python, separately from the project worker
interpreter chosen in Settings. Rust 1.99.0, GPUI Kit 0.7.0 and GPUI snapshots
0.3.7 are pinned along with the complete dependency graph in Cargo.lock.

```powershell
python -m pytest -q
python scripts/desktop_contracts.py
python scripts/desktop_smoke.py
python scripts/desktop_lifecycle.py
python scripts/desktop_scorecard.py
cd desktop
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --locked -p kernelledger-desktop
```

`python scripts/desktop_inventory.py` records the Windows target's locked Cargo
license declarations and the SQL inspection schema 2. The
[dependency inventory](dependency-licenses.json) is an input to redistribution
review; it does not replace collecting license texts or signing a release.

The smoke test edits an isolated notebook through Rust, runs with the acknowledged
hash, reconnects, recovers the original operation and checks the runtime fold.
It stops its own runtime. Existing notebooks are never used as fixtures.

Profile data is under `%LOCALAPPDATA%/KernelLedger/Desktop` or
`$XDG_STATE_HOME/kernelledger/desktop`. Drafts hold unacknowledged edits; durable
operation keys survive disconnection. Tokens remain in `.kl/token` and are never
stored in the profile. Scientific packages belong in the project interpreter.

See [coverage](coverage.md), [keyboard guide](keyboard.md), and
[platform qualification](platforms.md) before treating this build as a release.

The frontend v3 regression suite covers edit-mode run routing, IME shortcut
suppression, palette navigation, safe modal focus, deletion confirmation, filters,
and theme contrast. `python scripts/desktop_ui_probe.py --theme light` probes a
disposable native window, its minimum size, UI Automation names and rendered
output. Repeat with `dark`, `contrast`, or `system`. It measures window visibility,
not editor readiness, and does not qualify NVDA, mixed DPI or usability.

Appearance and recent notebook paths persist in the desktop profile. Follow system
also responds to Windows high contrast. See [Help](help.md) and the versioned
[UX scorecard](ux-scorecard.md) for implemented capabilities and open qualification.
