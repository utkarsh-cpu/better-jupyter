"""Build KernelLedger's interactive Python visualization (index.html).

Reads the source inventory JSON produced by `visualize_inventory.py`, turns it
into a deterministic left-to-right layered dependency graph, and bakes everything
into a single self-contained HTML file under `artifacts/visualize/`.

Pipeline:
  visualize_inventory.py  ->  artifacts/visualize/inv-src.json (+ groups/top)
  build_visual.py         ->  artifacts/visualize/index.html

Run both, then open index.html in any browser.
"""

from __future__ import annotations

import json
from math import log10
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "artifacts" / "visualize"

SRC = json.loads((DATA / "inv-src.json").read_text(encoding="utf-8"))
GROUPS = json.loads((DATA / "inv-groups.json").read_text(encoding="utf-8"))
TOP = json.loads((DATA / "inv-top.json").read_text(encoding="utf-8"))

TEMPLATE_PATH = DATA / "visual.template.html"

# --- geometry constants (SVG user units) ------------------------------------
COL_GAP = 236      # horizontal distance between rank columns
LEFT_PAD = 90
NODE_H = 46
ROW_GAP = 60
TOP_PAD = 64
DOMAIN_ORDER = ["entry", "worker", "supervisor", "desktop", "core", "gpu"]


def domain_of(name: str) -> str:
    seg = name.split(".")[1] if name.startswith("kernelledger.") else name
    if seg in ("cli", "__main__"):
        return "entry"
    if seg == "worker":
        return "worker"
    if seg == "supervisor":
        return "supervisor"
    if seg == "desktop":
        return "desktop"
    if seg in ("experiments", "artifacts", "git_state", "execution") or seg == "gpu":
        return "gpu"
    return "core"  # ledger, blobs, source, notebook, registry, sharing


def short(name: str) -> str:
    return name[len("kernelledger."):] if name.startswith("kernelledger.") else name


def node_w(lines: int, name_len: int) -> int:
    # width is the larger of: a log-scaled "size" hint, or room for the label
    norm = log10(lines + 1) / log10(1360) if lines > 0 else 0.08
    size_hint = 96 + 46 * norm
    fit = name_len * 6.7 + 26
    return int(max(size_hint, fit))


def main() -> None:
    # Drop the bare package root node and any edge pointing at it (it is only
    # reached via `from .. import __version__`, a same-unit attribute import).
    all_modules = {m["name"]: m for m in SRC["modules"]}
    nodes: dict[str, dict] = {}
    for name, m in all_modules.items():
        if name == "kernelledger":
            continue
        nodes[name] = m

    def is_internal_edge(e: dict) -> bool:
        return e["from"] in nodes and e["to"] in nodes

    solid = [e for e in SRC["edges"] if is_internal_edge(e)]
    dashed = [e for e in SRC.get("dashed", []) if is_internal_edge(e)]

    # --- layering (rank = longest dependency path) via capped relaxation ----
    rank = {n: 0 for n in nodes}
    for _ in range(len(nodes)):
        changed = False
        for e in solid:
            v, d = e["from"], e["to"]
            if rank[v] <= rank[d] and rank[d] + 1 > rank[v]:
                rank[v] = rank[d] + 1
                changed = True
        if not changed:
            break
    max_rank = max(rank.values())

    # --- group columns -------------------------------------------------------
    columns: dict[int, list[str]] = {r: [] for r in range(max_rank + 1)}
    for name in nodes:
        columns[rank[name]].append(name)

    def col_order(ids: list[str]) -> list[str]:
        key = lambda n: (DOMAIN_ORDER.index(domain_of(n)), short(n))
        return sorted(ids, key=key)

    for r in columns:
        columns[r] = col_order(columns[r])

    # --- barycenter sweeps to reduce edge crossings --------------------------
    def place_sweep(direction: str) -> None:
        # Re-order each column's nodes by the mean vertical index of their
        # neighbors in the adjacent, already-ordered column.
        if direction == "l2r":
            cols = range(1, max_rank + 1)
        else:
            cols = range(max_rank - 1, -1, -1)
        for r in cols:
            neighbor_r = r - 1 if direction == "l2r" else r + 1
            placed = {n: i for i, n in enumerate(columns.get(neighbor_r, []))}

            def score(name: str) -> float:
                vals = []
                for e in solid:
                    if e["from"] == name and e["to"] in placed:
                        vals.append(placed[e["to"]])
                    if e["to"] == name and e["from"] in placed:
                        vals.append(placed[e["from"]])
                return (sum(vals) / len(vals)) if vals else 1e6

            stable = {name: i for i, name in enumerate(columns[r])}
            columns[r] = sorted(columns[r], key=lambda n: (score(n), stable[n]))

    place_sweep("l2r")
    place_sweep("r2l")
    place_sweep("l2r")

    # --- coordinates ---------------------------------------------------------
    col_heights = {r: len(cols) for r, cols in columns.items()}
    max_rows = max(col_heights.values())
    board_h = TOP_PAD * 2 + (max_rows - 1) * ROW_GAP + NODE_H
    # center each column around the vertical midpoint
    mid_y = TOP_PAD + (max_rows - 1) * ROW_GAP / 2 + NODE_H / 2

    positions: dict[str, dict] = {}
    for r in columns:
        ids = columns[r]
        col_h = (len(ids) - 1) * ROW_GAP + NODE_H
        start_y = mid_y - col_h / 2 + NODE_H / 2
        for i, name in enumerate(ids):
            x = LEFT_PAD + (max_rank - r) * COL_GAP + 90  # rank 0 (leaves) on the right
            y = start_y + i * ROW_GAP
            m = nodes[name]
            positions[name] = {"x": x, "y": y, "w": node_w(m["lines"], len(short(name))), "h": NODE_H, "rank": r}

    # --- build edge paths ----------------------------------------------------
    def anchor(name: str, side: str) -> tuple[int, int]:
        p = positions[name]
        return (p["x"] + p["w"] / 2 if side == "right" else p["x"] - p["w"] / 2, p["y"])

    def edge_path(e: dict) -> str:
        sx, sy = anchor(e["from"], "right" if positions[e["from"]]["x"] > positions[e["to"]]["x"] else "left")
        tx, ty = anchor(e["to"], "left" if positions[e["to"]]["x"] < positions[e["from"]]["x"] else "right")
        mx = (sx + tx) / 2
        return f"M {sx:.1f} {sy:.1f} C {mx:.1f} {sy:.1f}, {mx:.1f} {ty:.1f}, {tx:.1f} {ty:.1f}"

    board_w = max(p["x"] + p["w"] / 2 for p in positions.values()) + LEFT_PAD

    # --- assemble payload ----------------------------------------------------
    out_nodes = []
    for name, m in nodes.items():
        p = positions[name]
        out_nodes.append({
            "id": name,
            "name": short(name),
            "path": m["file"],
            "lines": m["lines"],
            "classes": m["classes"],
            "functions": m["functions"],
            "optional": m["optional"],
            "domain": domain_of(name),
            "doc": m["doc"],
            "symbols": sorted(m["public_classes"] + m["public_functions"]),
            "x": p["x"],
            "y": p["y"],
            "w": p["w"],
            "h": p["h"],
        })
    out_edges = [
        {**e, "d": edge_path(e),
         "ax": anchor(e["from"], "right"), "ay": 0,
         "sx": anchor(e["from"], "right")[0], "sy": anchor(e["from"], "right")[1],
         "tx": anchor(e["to"], "left")[0], "ty": anchor(e["to"], "left")[1]}
        for e in solid
    ]
    out_lazy = [
        {**e, "d": edge_path(e), "sx": anchor(e["from"], "right")[0], "sy": anchor(e["from"], "right")[1],
         "tx": anchor(e["to"], "left")[0], "ty": anchor(e["to"], "left")[1]}
        for e in dashed
    ]

    stdlib_core = [n["name"] for n in out_nodes if not n["optional"]]
    gpu = [n["name"] for n in out_nodes if n["optional"]]

    payload = {
        "title": "KernelLedger — Python module map",
        "board": {"w": board_w, "h": board_h},
        "domains": DOMAIN_ORDER,
        "nodes": out_nodes,
        "edges": out_edges,
        "lazy": out_lazy,
        "groups": GROUPS,
        "totals": {"src_src": SRC.get("total"),
                   "top": TOP.get("total")},
        "split": {"stdlib": len(stdlib_core), "gpu": len(gpu),
                  "names": dict(stdlib=stdlib_core, gpu=gpu)},
        "legend_note": "Arrows point from a module to what it imports at load time. "
                       "Dashed arrows are lazy (in-function) imports, shown off by default.",
    }

    data_json = json.dumps(payload, separators=(",", ":")).replace("</", "<\\/")
    template = TEMPLATE_PATH.read_text(encoding="utf-8")
    out = template.replace("/*__DATA__*/null", data_json)
    out_path = DATA / "index.html"
    out_path.write_text(out, encoding="utf-8")
    print(f"Wrote {out_path}")
    print(f"nodes={len(out_nodes)} solid_edges={len(out_edges)} lazy_edges={len(out_lazy)}")
    print(f"board={board_w:.0f}x{board_h:.0f}")


if __name__ == "__main__":
    main()
