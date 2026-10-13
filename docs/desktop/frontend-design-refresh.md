# Frontend design refresh

Date: 2026-10-10. Builds on the existing v6 working tree.

## Changes

- Shared buttons use 8px corners and medium-weight labels. Secondary actions
  have theme-aware raised surfaces, hover/pressed fills and restrained shadows.
  Primary, danger and quiet actions retain their native Kit variants and behavior.
- Settings choices sit in wrapping segmented tracks with a visible selected
  segment. Appearance, typography, layout, accessibility and interpreter settings
  have bounded, grouped surfaces. Existing preference persistence is retained.
- Notebook cells have rounded frames, a full selected boundary, tinted status
  badges and a unified header/editor/output layout. Each code cell's run action
  sits beside its execution number and remains reachable without selection.
- The notebook column is centered with an 80rem maximum measure. Existing source
  and output scrolling handles wide content. Insert actions are centered between
  cells and remain exposed for selected cells, keyboard focus and Comfortable mode.
- Shared record surfaces have rounded hairline frames. Menus have restrained
  shadows and a 100ms opacity entrance. The application's reduced-motion setting
  skips the entrance; native GPUI also handles the OS animation preference.
- The Windows probe now understands compact menu destinations, accepts the current
  destination's accessible name, captures Settings and waits for fixture shutdown.

## Verification

- `cargo test --locked --workspace`: 34 passed (8 client, 6 models, 20 UI).
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `cargo build --locked -p kernelledger-desktop`: passed.
- New interaction regressions click real Settings segments, change editor size
  and interface scale, and verify an unselected header Run submits exactly once.
- Windows native probe passed for Light, Dark and High contrast at 192 DPI.
  Each run checks native identity, output semantics, menu/destination navigation,
  ten resize cycles, four requested notebook sizes and a Settings capture.

[Native evidence](../../artifacts/frontend-design-refresh/native/) includes
compositor images, UI Automation snapshots and probe reports. Requested outer
window dimensions are not certification of exact client sizes. Captures are
cropped screen images and can include occlusion or window-edge pixels; inspect
them visually. The final Light Settings image is cropped to the client area.

## Boundaries

This is UI work; runtime protocols and notebook file formats are unchanged.
The pinned GPUI API does not provide general subtree scaling (as documented in
its popover implementation). Press compression, trigger-origin spring scaling
and true backdrop blur have therefore not been implemented. Native pressed
states, immediate dismissal, layered solid surfaces and a short fade provide
the shipped feedback. No artificial per-cell running animation is inferred from
an unknown operation state.

This pass does not certify NVDA, actual Windows IME, mixed-monitor DPI, Linux,
frame-time performance or a large-notebook stress run. Existing v6 qualification
gates remain in [the v6 ledger](frontend-v6.md).
