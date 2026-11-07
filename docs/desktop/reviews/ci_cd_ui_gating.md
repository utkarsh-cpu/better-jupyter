# CI/CD UX Qualification Gating for KernelLedger

Based on FRONTEND_PLAN v4 (2026-10-08) and UX qualification gates F-QA-01 to F-QA-05.

---

## 🎯 Release Gate Requirements (F-QA-05)

A release requires **ALL** of the following:

| Requirement | Threshold | Description |
|---|---|---|
| **Scorecard total** | ≥ 80 points | Weighted sum across all categories |
| **Weighted coverage** | ≥ 90% | Evidence coverage across criteria |
| **No open P1 findings** | Zero | Serious task/safety/reachability/accessibility issues |
| **Direct verification** | Required | Keyboard-only, screen-reader, and recovery journeys |

---

## 📋 UX Scorecard Structure (F-QA-01)

The scorecard is versioned under `docs/desktop/` and has **18 categories** with **weighted criteria**:

### Original 16 Categories (from audit):
1. **F-UX-01 to F-UX-10** (18 criteria) - Visual design, state, interaction
2. **F-A11Y-01 to F-A11Y-09** (9 criteria) - Accessibility (WCAG 2.2 thresholds)
3. **F-PERF-01 to F-PERF-23** (16 criteria) - Performance targets
4. **F-KEY-01 to F-KEY-06** (6 criteria) - Keyboard model
5. **F-IA-01 to F-IA-26** (26 criteria) - Information architecture
6. **F-ED-01 to F-ED-20** (20 criteria) - Source editing
7. **F-RT-01 to F-RT-41** (20 criteria) - Runtime states
8. **F-IO-01 to F-IO-21** (21 criteria) - Import/export
9. **F-SET-01 to F-SET-06** (6 criteria) - Settings/environments
10. **F-GOAL-01 to F-GOAL-04** (4 criteria) - Core journeys
11. **F-CL-01 to F-CL-09** (9 criteria) - Client architecture
12. **F-OUT-01 to F-OUT-26** (26 criteria) - Output rendering
13. **F-CK-01 to F-CK-06** (6 criteria) - Checkpoints/restore
14. **F-IO-01 to F-IO-11** (part of F-IO section)
15. **F-QA-01 to F-QA-05** (5 criteria) - Qualification gates
16. **F-PKG-01 to F-PKG-07** (7 criteria) - Packaging/release

### Added 2 Categories (from FRONTEND_PLAN v4):
17. **J5, J7 - Interchange and Git** (J5: bundle import/export, J7: branch switching)
18. **F-RT-30 to -33, J8, tabs/detached windows - Input and multiple runtimes** (input prompts, runtime switching, tab management)

**Weights total 100 points** (F-QA-01). Each criterion lists requirement IDs it tests, and every journey J1–J8 has at least one criterion.

---

## 🚦 Release Gate Workflow (F-QA-05)

```
Phase F2 Exit:
  │
  ├──► Formative usability study (5-8 participants, journeys J1-J8)
  │   │
  │   ├──► Record: task success, errors, time-on-task, SUS score
  │   │
  │   └──► F-QA-02: Rescore with fixed fixture notebook
  │
  │
Phase F3 Exit:
  │
  ├──► F-QA-02: Rescore (preserve earlier evidence)
  │
  │
Phase F4 Exit:
  │
  ├──► F-QA-02: Rescore
  │
  │
Phase F5 Exit:
  │
  ├──► F-QA-02: Rescore (every row in section 10 covered or documented as deferred)
  │
  │
Phase F6 Exit (Release Gate):
  │
  ├──► F-QA-02: Final rescore with fixed fixture
  │
  ├──► F-QA-05: Verify score ≥ 80 AND coverage ≥ 90% AND no P1 findings
  │
  ├──► Direct verification: keyboard-only, screen-reader, recovery journeys
  │
  └──► Release ✅ OR Block ❌
```

---

## 📊 Example CI/CD Pipeline YAML

### GitHub Actions Workflow

```yaml
name: UI/UX Qualification

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main, develop]

jobs:
  ui-tests:
    runs-on: ubuntu-latest
    timeout-minutes: 60
    
    steps:
      - uses: actions/checkout@v3
      
      - name: Set up Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          features: "test-support"
        
      - name: Set up Python
        uses: actions/setup-python@v4
        with:
          python-version: "3.12"
      
      - name: Install Python dependencies
        run: |
          python -m pip install --upgrade pip
          pip install -e ".[dev]"
          pip install pytest hypothesis ruff pyright
      
      - name: ✅ Run Python runtime tests
        run: |
          python -m pytest tests/test_desktop.py tests/test_runtime.py tests/test_source.py -v
          # Exit code 0 = all 36 pass
      
      - name: 🔨 Build Rust UI capture tool
        run: |
          cd artifacts/frontend-bars-polish/capture
          cargo build --release 2>&1
      
      - name: 📸 Capture baseline screenshots
        run: |
          # Start KernelLedger (headless or windowed)
          # Then capture:
          ./artifacts/frontend-bars-polish/capture/target/release/kl-window-capture.exe \
            <WINDOW_HWND> artifacts/ci-screenshots/baseline.png
          # Compare against artifacts/frontend-bars-polish/baseline/
      
      - name: 🧪 Run GPUI UI integration tests (if applicable)
        run: |
          # If KernelLedger has gpui_kit::test modules:
          cargo test --features test-support --test ui -- --nocapture
          # Or: cargo test -p gpui-kit --features test-support --test ui --locked
      
      - name: 📊 UX Scorecard rescore
        run: |
          # Run the UX qualification script
          # This should:
          # 1. Read the current scorecard from docs/desktop/
          # 2. Rescore with the fixed fixture notebook
          # 3. Report total score and weighted coverage
          python scripts/rescore_ux_scorecard.py
          # Should output: score / 100, coverage %, open P1 findings
      
      - name: 🚨 Check release gate conditions
        run: |
          # Parse the scorecard output
          # FAIL gate if:
          #   score < 80 OR coverage < 90% OR P1 findings exist
          # Details in FRONTEND_PLAN F-QA-05
          
          # Example check (bash):
          SCORE=$(grep -oP '(?<=score: )[0-9]+' ux-scoreresult.txt)
          COVERAGE=$(grep -oP '(?<=coverage: )[0-9]+' ux-scoreresult.txt)
          P1_COUNT=$(grep -oP '(?<=P1 findings: )[0-9]+' ux-scoreresult.txt)
          
          if [ "$SCORE" -lt 80 ] || [ "$COVERAGE" -lt 90 ] || [ "$P1_COUNT" -gt 0 ]; then
            echo "❌ Release gate failed: score=$SCORE coverage=$COVERAGE P1=$P1_COUNT"
            exit 1
          else
            echo "✅ Release gate passed: score=$SCORE coverage=$COVERAGE P1=$P1_COUNT"
          fi
```

---

## 📈 Scorecard Rescore Script (Python skeleton)

```python
#!/usr/bin/env python3
"""Rescore the UX qualification scorecard per F-QA-02."""

import json
import subprocess
from pathlib import Path

def rescore_scorecard(fixture_notebook_path: Path) -> dict:
    """Rescore the UX scorecard using a fixed fixture notebook.
    
    Per F-QA-02: rescored with fixed fixture at every phase exit,
    preserving earlier evidence and recording build/environment.
    """
    
    # 1. Read current scorecard
    scorecard_path = Path("docs/desktop/ux-scorecard.json")
    if not scorecard_path.exists():
        raise FileNotFoundError("UX scorecard not found")
    
    with open(scorecard_path) as f:
        scorecard = json.load(f)
    
    # 2. Run fixture notebook through all journeys J1-J8
    #    and collect evidence for each criterion
    evidence = {}
    
    # ... (run each journey, check each criterion, record pass/fail)
    # This is the heavy lift - each criterion from F-QA-01 needs
    # automated verification against the fixture notebook
    
    # 3. Calculate weighted score
    total_weight = 0
    scored_weight = 0
    open_p1 = []
    
    for criterion in scorecard["criteria"]:
        weight = criterion["weight"]
        total_weight += weight
        
        # Check if criterion passed with fixture
        cid = criterion["id"]
        if cid in evidence and evidence[cid]["passed"]:
            scored_weight += weight
        else:
            # Check if P1 severity
            if criterion.get("severity") == "P1":
                open_p1.append(criterion["id"])
    
    # 4. Compute results
    score = (scored_weight / total_weight * 100) if total_weight > 0 else 0
    coverage = (len(evidence) / len(scorecard["criteria"]) * 100) if scorecard["criteria"] else 0
    
    result = {
        "score": round(score, 1),
        "coverage": round(coverage, 1),
        "total_criteria": len(scorecard["criteria"]),
        "scored_criteria": len([c for c in evidence if evidence[c]["passed"]]),
        "open_p1": open_p1,
        "evidence_count": len(evidence),
        "build": subprocess.check_output(["git", "describe", "--always"]).decode().strip(),
        "environment": {
            "os": subprocess.check_output(["uname", "-a"]).decode().strip(),
            "python": subprocess.check_output(["python", "--version"]).decode().strip(),
        }
    }
    
    return result


if __name__ == "__main__":
    import sys
    fixture = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("tests/fixtures/desktop")
    result = rescore_scorecard(fixture)
    
    # Output results
    print(f"UX Scorecard Rescore")
    print(f"====================")
    print(f"Score:       {result['score']}/100")
    print(f"Coverage:    {result['coverage']}% ({result['scored_criteria']}/{result['total_criteria']} criteria)")
    print(f"Evidence:    {result['evidence_count']} criteria have evidence")
    print(f"Open P1:     {len(result['open_p1'])}")
    if result["open_p1"]:
        print(f"  P1 IDs: {', '.join(result['open_p1'])}")
    print(f"Build:       {result['build']}")
    print(f"Environment: {result['environment']['os']}")
    print()
    
    # Exit with code based on gate conditions
    if result["score"] < 80:
        print("❌ Score < 80, release gate would fail")
        sys.exit(1)
    elif result["coverage"] < 90:
        print("❌ Coverage < 90%, release gate would fail")
        sys.exit(1)
    elif result["open_p1"]:
        print("❌ Open P1 findings, release gate would fail")
        sys.exit(1)
    else:
        print("✅ All gate conditions met")
        sys.exit(0)
```

---

## 🎯 Gate Condition Checks (F-QA-05)

Your CI/CD should FAIL (block release) if ANY of:

| Condition | Threshold | Action |
|---|---|---|
| **Score < 80** | `score < 80` | Block release, investigate findings |
| **Coverage < 90%** | `coverage < 90` | Block release, add missing evidence |
| **Open P1 findings** | `len(open_p1) > 0` | Block release, fix serious issues |
| **No keyboard verification** | `!keyboard_verified` | Block release, verify J1-J8 by keyboard |
| **No screen-reader verification** | `!a11y_verified` | Block release, test with NVDA/Orca |
| **No recovery journey verification** | `!recovery_verified` | Block release, test crash recovery flows |

**All 5 conditions must PASS for release.**

---

## 📋 Minimal CI Setup (to start today)

### GitHub Actions (minimal)

```yaml
name: UI Qualification

on:
  push:
    paths:
      - 'src/**'
      - 'tests/**'
      - 'docs/desktop/**'

jobs:
  qualify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Run runtime tests
        run: python -m pytest tests/test_desktop.py -v
        # Must pass (36/36)
      
      - name: Check no P1 findings in scorecard
        run: |
          # Quick check: look for P1 in scorecard
          if grep -r "P1" docs/desktop/ 2>/dev/null | grep -v ".md" | grep -v "# "; then
            echo "⚠️  P1 findings detected in scorecard"
            exit 1
          else
            echo "✅ No P1 findings detected"
          fi
```

### Azure DevOps (minimal)

```yaml
trigger:
  branches:
    include:
      - main
      - develop

pr:
  branches:
    include:
      - main
      - develop

jobs:
- job: UI_Qualification
  displayName: "UI/UX Qualification Gate"
  pool:
    vmImage: 'ubuntu-latest'
  steps:
  - checkout: self
  
  - task: UsePythonVersion@0
    inputs:
      versionSpec: '3.12'
  
  - script: |
      python -m pytest tests/test_desktop.py -v
      echo "Runtime tests: $?"
    displayName: "Run Python runtime tests"
  
  - script: |
      # Check scorecard for P1 findings
      if findstr /S /M "P1" docs\desktop\*.json 2>nul; then
        echo "⚠️ P1 findings in scorecard"
        exit 1
      else
        echo "✅ No P1 findings"
      fi
    displayName: "Check for P1 findings"
```

---

## 📝 FRONTEND_PLAN Reference Gates

| Phase | Gate | What Must Pass |
|---|---|---|
| **F0** | F-STACK-10 to -13 | Platform probes, native MIME samples |
| **F1** | FS-03, FS-10 | Vertical slice end-to-end |
| **F2** | FS-01 to FS-08, FS-13 to FS-29 | Editing works without runtime, first scorecard |
| **F3** | FS-03 to FS-08, FS-17 to FS-24 | Execution/output renderers |
| **F4** | FS-09 to FS-11, FS-28 | Durable workflows (history, checkpoints) |
| **F5** | FS-12, FS-14 to FS-19, all section 10 | Full UX + interchange |
| **F6** | F-QA-05 | **Release gate: score ≥ 80, coverage ≥ 90%, no P1** |
| **Every phase from F2** | F-QA-02 | Rescore with fixed fixture |

---

## 🚀 Recommended CI/CD Progression

### Phase 1 (F0-F1) - Today
```yaml
# Minimal: just run the 36 Python tests
- run python -m pytest tests/test_desktop.py -v
```

### Phase 2 (F2) - Next
```yaml
# Add: screenshot comparison, basic element state checks
- build Rust capture tool
- capture baseline screenshots
- run GPUI integration tests if available
```

### Phase 3 (F3-F4) - Medium term
```yaml
# Add: UX scorecard rescore, accessibility checks
- run Python scorecard rescore script
- verify coverage ≥ 90%
- check no P1 findings
```

### Phase 4 (F5-F6) - Release
```yaml
# Full gate: all F-QA-05 conditions
- score ≥ 80
- coverage ≥ 90%  
- no P1 findings
- keyboard verification
- screen-reader verification
- recovery journey verification
```

---

## 🔔 CI/CD Integration Checklist

- [ ] Python runtime tests pass (36/36)
- [ ] Rust capture tool builds (if applicable)
- [ ] Screenshot baselines exist and are versioned
- [ ] UX scorecard is under `docs/desktop/`
- [ ] Scorecard rescore script works (F-QA-02)
- [ ] Gate conditions coded (score ≥ 80, coverage ≥ 90%, no P1)
- [ ] Keyboard-only verification documented
- [ ] Screen-reader verification documented (NVDA/Orca)
- [ ] Recovery journeys documented (crash recovery flows)
- [ ] CI job fails appropriately on gate failure
- [ ] Release notes include UX scorecard results

---

*Following FRONTEND_PLAN v4 qualification gates. Release readiness requires direct verification of keyboard-only, screen-reader, and recovery journeys in addition to the numerical scorecard gate.*</parameter>
</function>
</tool_call>
<tool_call>
<function=shell>
<parameter=command>
cd C:\Users\UTKARSH KAUSHIK\Documents\Projects\KernelLedger && python -m pytest tests/test_desktop.py -v 2>&1 | tail -5