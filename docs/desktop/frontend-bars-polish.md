# Frontend bars and buttons polish

Implements the recommendations in “Polish Frontend Bars and Buttons”
(`5cc4d6a5-6e44-4e29-bdfe-b51e3b8342f9`) on top of the existing frontend refresh.

## Implementation, in review order

1. **Button alignment:** removed cross-axis `self_start` overrides from shared
   buttons and disabled wrappers. Horizontal bars own vertical centering; menu
   columns can use full-width action rows. Enabled and disabled controls occlude
   the native drag region and stop the title bar's manual drag handler.
2. **Unified window chrome:** the app uses `TitleBar::window_options()` and renders
   the header inside the toolkit title bar, preserving native caption controls,
   drag behavior, and platform-specific decorations. The second toolbar is gone.
3. **Destination hierarchy:** Notebook, History, and Checkpoints use the Settings
   segmented track. The selected segment uses an opaque raised surface and
   explicit pressed semantics. The toolkit's generic accent selection and custom
   background mixing are overridden for this treatment.
4. **Familiar controls:** sidebar, search, and ellipsis icons replace Outline,
   Commands, and Menu text. Accessible names and shortcut tooltips remain.
5. **Runtime status:** the pill combines a colored status dot, Python, the live
   worker state, and a menu caret. Compact layouts shorten the label or use an
   icon; full accessible state and the status bar remain available. Interrupt,
   Restart empty, and Stop runtime live in its anchored menu.
6. **Actions next to content:** Run all sits beside the runtime pill. Each code
   cell keeps its own Run action. Insertion remains between cells, and the final
   row is a permanently discoverable Add cell action. An empty notebook retains
   its existing creation actions.
7. **Accent and depth:** Run all is the sole solid accent action in the title bar.
   Unavailable buttons use neutral styling. The bar uses a solid chrome fill and
   a soft shadow; high contrast retains an explicit boundary instead.
8. **Output width and tools:** output bodies no longer reserve a side gutter for
   Output tools. A quiet ellipsis in the cell header appears on hover or keyboard
   focus, and remains visible when selected, open, or in comfortable spacing.
   A cell-owned state entity controls disclosure for all its output blocks;
   copy, find, image sizing, and output limits retain their per-output state.
9. **Execution labels:** Markdown and Raw show no execution placeholder. Executed
   code cells show a plain muted ordinal beside Run; unexecuted cells have none.
10. **Failures:** recorded error rows have a red tint and red leading edge, along
    with their existing error name, message, outcome badge, and traceback control.
11. **Status bar:** Cell navigation and Editing are plain text. Detach guidance
    lives in the runtime indicator tooltip instead of a permanent trailing row.
12. **Reading width:** ordinary cells are capped at 64rem. Tables and image plots
    can expand the cell to 80rem while the source editor remains capped at 64rem.

The header adapts separately from the notebook sidebar. At the smallest window
with 200% interface size and comfortable targets, notebook switching and runtime
controls move into Menu. All three primary destinations remain visible as icons,
and the notebook name remains in the native window title. Commands are always
available through Menu and Ctrl+Shift+P. No preferences or notebook data are
changed by this layout adaptation.

## Verification and evidence

- `cargo test --workspace --locked`: 36 tests, including 22 UI tests.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` and
  `cargo fmt --all --check`.
- `cargo build --locked -p kernelledger-desktop`.
- `pytest -q tests/test_desktop.py tests/test_desktop_contracts.py`: 22 tests.
- Layout regression checks cover 800, 1024, 1280, and 1440 widths at 100%, 125%,
  150%, and 200% interface sizes with both compact and comfortable spacing.
  Visible header controls share their bar's vertical center within one pixel.
- Behavioral checks exercise Run all, runtime menu availability and restart
  confirmation, disabled alignment, cell-owned output-tools disclosure, output
  width, and table expansion without widening the source editor.
- Native Windows UI Automation checks exercise destination/menu navigation,
  output semantics, absence of bracket placeholders, alignment, and caption hit
  testing. The probe also checks that controls are outside the caption buttons,
  controls receive client clicks, and blank chrome remains draggable.

Native reports and screenshots are under
[`artifacts/frontend-bars-polish/native`](../../artifacts/frontend-bars-polish/native).
Each probe uses a temporary notebook, runtime, and profile and closes only its
own app and fixture runtime. Captures use Windows Graphics Capture restricted
to that app's HWND, including when the window is occluded. No foreground changes
or captures of other applications are required.

The native evidence qualifies this Windows host at 192 DPI. It does not certify
NVDA, IME, mixed-DPI transitions, macOS, or Linux behavior.

## Reproduce native captures

The small capture utility is evidence tooling, separate from app dependencies:

```powershell
cargo +1.99.0 build --locked --manifest-path artifacts/frontend-bars-polish/capture/Cargo.toml
.venv/Scripts/python.exe scripts/desktop_ui_probe.py --theme light `
  --output artifacts/frontend-bars-polish/native `
  --window-capture-exe artifacts/frontend-bars-polish/capture/target/debug/kl-window-capture.exe
```

Repeat with `--theme dark` and `--theme contrast`. For the largest targets, add
`--ui-scale 2 --comfortable` and use a separate output directory.
