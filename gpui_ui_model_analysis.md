# How UI/UX Can Be Seen by a Model for GPUI Apps

## Overview

GPUI Kit (the Rust desktop UI framework for KernelLedger) has a first-class testing story that enables models (AI assistants, automated checks) to inspect and interact with the UI at a deep level. The framework provides multiple "views" into the UI, each suitable for different modeling needs.

---

## 1. Headless UI Integration Tests (`#[gpui_kit::test]`)

The primary way a model can "see" the UI is through GPUI Kit's integration test framework. Tests render real components in headless windows and dispatch native events.

### How It Works

```rust
use gpui_kit::test::{TestSupportExt, TestWindowExt};
use gpui_kit::{
    Window, WindowBounds, WindowOptions,
    component::button::Button,
    prelude::*,
    px, size,
};

#[gpui_kit::test]
fn ui_interaction_test(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(640.), px(480.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| { /* render view */ }
        ).expect("open test window")
    });

    // Model can now inspect and interact
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        
        // Query UI elements
        let btn = window.find("save-button");
        
        // Assert properties
        assert!(btn.visible());
        assert_eq!(btn.label(), Some("Save"));
        
        // Dispatch events
        window.click("save-button", cx);
        window.input("some text", cx);
        
        // Check state after interaction
        assert!(status_label.visible());
    });
}
```

### What the Model Sees

| Capability | API | Use Case |
|---|---|---|
| **Element queries** | `window.find(id)`, `window.find_all(id)` | Locate buttons, inputs, lists, status areas |
| **Property inspection** | `.label()`, `.value()`, `.visible()`, `.bounds()`, `.focused()` | Read current UI state |
| **Event dispatch** | `window.click()`, `window.input()`, `window.press()`, `window.scroll()` | Simulate user interactions |
| **Accessibility tree** | `window.find_by_label()`, `window.find_all_by_role()` | Screen-reader perspective |
| **Frame snapshots** | Render frame before each assertion | Consistent state per frame |

### Test Levels

| Level | Crate | Purpose |
|---|---|---|
| `#[test]` | Rust built-in | Pure logic, no UI |
| `#[gpui_kit::test]` | gpui_kit | Entities, subscriptions, actions, async tasks; headless windows |
| `VisualTestContext` | gpui_kit | Existing GPUI window helpers; inspect real running windows |
| `TestWindowExt` | gpui_kit | Full UI flow: render → interact → assert |

---

## 2. Element Snapshot Testing

GPUI Kit provides `ElementSnapshot` for capturing and comparing UI element state across test runs.

### Usage

```rust
use gpui_kit::test::ElementSnapshot;

#[gpui_kit::test]
fn snapshot_test(cx: &mut TestAppContext) {
    let btn = window.find("save-button");
    
    // Capture snapshot
    let snap1 = btn.snapshot(cx);
    
    // Dispatch interaction
    window.click("save-button", cx);
    
    // Capture after
    let snap2 = btn.snapshot(cx);
    
    // Compare
    assert_ne!(snap1, snap2);  // State changed as expected
    assert_eq!(snap2.label(), Some("Saved"));  // New state
}
```

### What's Captured

- Accessibility label/name
- Enabled/disabled state
- Visible/invisible state
- Bounds/geometry
- Focus state
- Custom element data

---

## 3. Accessibility Perspective (AccessKit)

GPUI integrates with AccessKit for accessibility. Models can inspect the UI from a screen-reader perspective.

### Accessibility Queries

```rust
use gpui_kit::test::{TestQueryExt, TestEventExt};

// Find elements by accessible label
window.find_by_label("Save Button")
window.find_all_by_role(gpui_kit::Role::Button)

// Find all clickable elements
window.find_all_by_role(gpui_kit::Role::Button)

// Check accessibility properties
let el = window.find("btn");
el.aria_label();  // Accessible name
el.role();  // Accessibility role
el.state();  // State set (disabled, checked, etc.)
```

### Model Use Cases

- **Screen-reader validation**: Can a blind user navigate the UI?
- **Focus management**: Is focus properly constrained and returned?
- **Contrast and target sizes**: Measured against WCAG thresholds (4.5:1 text, 3:1 large text, 24x24px pointer targets)
- **Keyboard navigation**: Tab order, Escape focus return, mode changes announced

---

## 4. Visual Regression via Screenshots

Although GPUI is a native Rust framework, the project already has screenshot-based testing infrastructure (built on Windows Graphics Capture).

### Existing Infrastructure

| Artifact | Purpose |
|---|---|
| `artifacts\frontend-bars-polish\native\` | Baseline screenshots (light/dark/contrast, 3 window sizes) |
| `artifacts\desktop-exports-2026-10-18\` | Flow-through screenshots (export, commands, notebook) |
| `kl-window-capture.exe` | Rust Windows Graphics Capture tool |
| `review.ps1` | PowerShell script: inspect UI tree + capture screenshot |

### Model-Automated Screenshot Comparison

```powershell
# 1. Launch KernelLedger
# 2. Capture baseline
.\artifacts\frontend-bars-polish\capture\target\release\kl-window-capture.exe <HWND> baseline.png

# 3. Run UI interaction (e.g., via RPC or automation)
# 4. Capture after state
.\artifacts\frontend-bars-polish\capture\target\release\kl-window-capture.exe <HWND> after.png

# 5. Compare using image diff tools
# Options:
# - Percy.io, TestMu AI, SmartUI (web-based)
# - Pixel diff with noise filtering
# - Perceptual hashing (pHash, dHash)
```

### Comparison Tools Ecosystem (from web search)

| Tool | Approach | GPUI Fit |
|---|---|---|
| **Percy** | Cloud-based pixel diff + AI filtering | Web-focused, needs adaptation |
| **TestMu AI** | ML-powered visual comparison | Can be adapted for native window diff |
| **SmartUI** | Baseline comparison with dynamic content filtering | Good for repetitive UI elements |
| **WebBrowserBot** | Automated visual testing | Web only |
| **Custom Rust crates** | `image` crate + `ssim` or `perceptualhash` | Native fit for GPUI |

---

## 5. Property-Based UI Testing

GPUI supports property-based testing via `#[gpui_kit::test(iterations = N)]` with a `StdRng`.

### Example: Property Testing UI Behavior

```rust
use gpui_kit::test::StdRng;

#[gpui_kit::test(iterations = 50)]
fn property_test_counter(cx: &mut TestAppContext, mut rng: StdRng) {
    let counter = cx.new(|cx| Counter::new(cx));
    let mut count = 0i32;

    for _ in 0..100 {
        let delta: i32 = rng.random_range(-10..=10);
        count += delta;
        counter.update(cx, |c, cx| { c.count += delta; cx.notify(); });
    }

    let actual = counter.read_with(cx, |c, _| c.count);
    assert_eq!(actual, count, "Counter property violated after random operations");
}
```

### Property Testing UI Invariants

| Invariant | Description |
|---|---|
| **State consistency** | After any sequence of operations, UI state matches internal state |
| **No panics** | User interactions (clicks, key presses) never panic |
| **Focus invariants** | Focus is always on some element or nowhere; Escape returns focus |
| **Layout stability** | Window resize preserves core layout; overflow moves items to menu |
| **Accessibility invariants** | All interactive elements have accessible labels; focus order is logical |

---

## 6. Distributed/Interleaving Testing

For apps with multiple runtimes/sessions (relevant for data scientists/data engineers), GPUI supports distributed testing.

### Distributed UI Test Example

```rust
#[gpui_kit::test]
fn test_multi_session_ui(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    // Create separate UI sessions
    let comp_a = cx_a.new(|_| MyComponent::new("Session A"));
    let comp_b = cx_b.new(|_| MyComponent::new("Session B"));

    // Interleave operations
    for i in 0..20 {
        if rng.random_bool(0.5) {
            comp_a.update(cx_a, |c, cx| c.operation(i, cx));
        } else {
            comp_b.update(cx_b, |c, cx| c.operation(i, cx));
        }
    }

    // Run pending ops
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    // Verify both sessions' UI is consistent
    let state_a = comp_a.read_with(cx_a, |c, _| c.ui_state);
    let state_b = comp_b.read_with(cx_b, |c, _| c.ui_state);
    
    assert!(state_a.is_valid());
    assert!(state_b.is_valid());
}
```

---

## 7. Crash-Free State Management Testing

The most critical GPUI bugs are entity re-entrancy panics (updating/reading an entity during render/update). Tests must prove these don't occur.

### Re-entrancy Test Pattern

```rust
#[gpui_kit::test]
async fn no_reentrancy_panics(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |w, c| { /* view */ })
            .expect("open window")
    });

    // Drive component the way a user does
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.within("language").click("input", cx);
        window.press("down", cx);
        window.press("enter", cx);
    }).unwrap();

    // Wait for commit (not straight after interaction)
    cx.wait_for(handle.into(), Duration::from_millis(500), |window, _| {
        window.find("language").expanded() == Some(false)
            && window.find("language").value() == Some("Go")
    }).await;

    // Verify application state agrees
    cx.update(|c| assert_eq!(language.read(c).selected_value(), Some(&"Go")));
}
```

---

## 8. Model Analysis Workflows

### Workflow A: Headless CI Tests (Recommended)

```yaml
# .github/workflows/ui-tests.yml
name: UI Tests
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    features: test-support
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --features test-support -- --test-threads=1
      - run: cargo test --features test-support --test ui -- --nocapture
```

**Model sees**: Deterministic test results, pass/fail, no UI rendering required (headless).

### Workflow B: Screenshot + AI Analysis

```
1. Capture screenshot via kl-window-capture.exe
2. Send to vision model (Claude, GPT-4V, etc.) with prompt:
   "Analyze this GPUI desktop application UI for:
   - Accessibility issues (contrast, target sizes)
   - Visual regressions vs baseline
   - Widget state correctness (enabled/disabled, visible/invisible)
   - Focus management
   - Error state clarity
3. Model returns structured feedback
```

**Model sees**: Natural language UI analysis, visual defect detection.

### Workflow C: RPC-Based Automation

KernelLedger's RPC protocol allows programmatic UI control:

```python
from kernelledger.cli import alive, ensure_supervisor, rpc

client = rpc("127.0.0.1", 12345)  # Connect to supervisor
# Submit cells, check outputs, inspect history
# Less direct than GPUI test, but works over network
```

**Model sees**: KernelLedger operations (run cells, check history, manage checkpoints), not raw pixel UI.

### Workflow D: Accessibility Tree Inspection

```rust
// Model can query the accessibility tree structure
fn inspect_a11y_tree(window: &Window) {
    let root = window.root(&mut cx).unwrap();
    let elements = root.elements(&mut cx);
    
    for el in &elements {
        // Model can "read":
        // - el.label() - accessible name
        // - el.role() - ARIA role equivalent
        // - el.state() - state flags
        // - el.bounds() - geometry
        // - el.focused() - focus status
    }
}
```

**Model sees**: Structured UI tree, independent of pixel layout.

---

## 9. What Model "Sees" — Comparison Table

| Modeling Approach | What Model Receives | Best For |
|---|---|---|
| **`#[gpui_kit::test]`** | Rust test results (pass/fail), element states, snapshots | Automated regression, CI/CD |
| **Accessibility tree** | Structured tree: labels, roles, states, bounds | Screen-reader validation, a11y compliance |
| **Screenshot + Vision AI** | Image + natural language analysis | Visual regression, layout issues, general UX |
| **RPC/programmatic** | KernelLedger operation results (cell execution, history) | Data scientist workflows, execution flow |
| **Property tests** | Invariants hold across random operations | State machine correctness, edge cases |
| **Distributed tests** | Multi-session consistency results | Data engineer pipelines, multi-user |

---

## 10. Recommendations for KernelLedger

Based on the existing infrastructure and GPUI Kit capabilities:

### Immediate (F0/F1 phase)

1. **Add `test-support` feature** to gpui-kit dependency (already documented in FRONTEND_PLAN F0 gate)
2. **Write `#[gpui_kit::test]` for core journeys J1-J8**:
   - J1: Notebook open/edit/run cells
   - J2: Reopen and inspect history
   - J4: Publish/restore checkpoints
   - J7: Git branch switching
3. **Element snapshot tests** for key surfaces (Notebook sidebar, Runtime toolbar, History list)
4. **Accessibility tests** using `find_by_label` and `find_all_by_role` for WCAG thresholds

### Medium Term (F2/F3 phase)

5. **Screenshot comparison** with baseline artifacts in `artifacts\frontend-bars-polish\native\`
6. **Property-based tests** for UI invariants (no panic on interactions, focus consistency)
7. **Distributed tests** for multi-notebook sessions (relevant for data engineers)

### Long Term (F4-F6 phase)

8. **Vision AI integration**: Send screenshots to model with structured prompts for UX analysis
9. **Cross-platform testing**: macOS (Metal), Linux (WGPU) headless rendering
10. **CI/CD gated**: UX scorecard (F-QA-05) requires score ≥ 80, coverage ≥ 90%

---

## Summary

A model can "see" GPUI app UI/UX through multiple lenses:

1. **Deterministic headless tests** (`#[gpui_kit::test]`) — pass/fail, element states, snapshots
2. **Accessibility tree** — structured UI tree for screen-reader validation
3. **Screenshots + Vision AI** — natural language visual analysis
4. **Property-based tests** — invariants across random operations
5. **Distributed tests** — multi-session consistency
6. **RPC/programmatic** — operation-level results (not pixel-level)

The **GPUI Kit testing framework** is the most comprehensive and integrated approach, as it tests real components in headless windows with native event dispatch. The existing KernelLedger artifact infrastructure (screenshots, UIA JSON files) provides a screenshot-based complement.

For a data scientist, the most valuable model view is **RPC-based operation inspection** (cell execution, history, checkpoints). For a data engineer, **Git branch switching tests** and **distributed session tests** are most relevant.