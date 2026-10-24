"""Task guidance covered by prompt version ledger-1."""

TASKS = {
    "ask": "Answer only from selected context. Cite recorded facts and label missing evidence.",
    "explain": "Explain the recorded exception. Distinguish observed traceback from hypotheses. Offer a minimal edit proposal using the base hash.",
    "edit": "Explain the intended improvement and propose an edit using the recorded source hash.",
    "refactor": "Propose a clean, behavior-preserving refactor using the recorded source hash and explain each improvement.",
    "document": "Propose clear documentation without changing program behavior.",
    "compare": "Narrate the deterministic comparison. Observed differences do not establish causes.",
    "checkpoint": "Explain what this checkpoint captures, which bindings are stored or omitted, and what restore or training-restore will affect.",
    "summary": "Summarize recent work and checkpoints. Link each recorded fact to its evidence.",
    "reproducibility": "Review hashes, checkpoint coverage and recorded environment. Do not claim external inputs are reproducible without evidence.",
    "environment": "Explain environment and GPU reports. Any installation commands must be copy-only proposals.",
    "profile": "Propose a visible profiling cell. Do not invent data statistics or claim to run the cell.",
    "explore": "Propose a visible profiling cell for data exploration. Do not invent data statistics or claim to run the cell.",
    "report": "Draft a Markdown experiment report from recorded evidence for user review and export.",
    "tests": "Propose tests as new cells for review. Do not claim they were executed.",
    "stale": "Explain which cells are stale relative to their last recorded execution and recommend a safe rerun order.",
    "starter": "Propose a coherent cell set as one insert_cells change for review.",
    "training": "Advise only from metrics explicitly recorded in selected outputs. State which metrics are missing.",
    "anomaly": "Narrate the deterministic execution duration statistics and flag anomalous runs. Distinguish statistical outliers from proven causes.",
}
