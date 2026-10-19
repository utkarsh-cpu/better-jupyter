"""Analyze the KernelLedger Python source tree and emit a JSON inventory.

Produces:
  artifacts/visualize/inv-src.json      inventory of src/kernelledger (modules,
                                        line counts, class/def counts, docstrings,
                                        and an intra-package import graph)
  artifacts/visualize/inv-groups.json   grouped totals (src / tests / scripts /
                                        examples / artifacts) with file counts

The output is fed to a self-contained HTML visualization (see html).
"""

from __future__ import annotations

import ast
import json
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PKG_DIR = ROOT / "src" / "kernelledger"
OUT_DIR = ROOT / "artifacts" / "visualize"


@dataclass
class Module:
    name: str  # dotted module name, e.g. kernelledger.gpu.backends.probe
    file: str  # repo-relative path
    lines: int = 0
    classes: int = 0
    functions: int = 0
    public_classes: list[str] = field(default_factory=list)
    public_functions: list[str] = field(default_factory=list)
    doc: str = ""
    imports_internal: list[str] = field(default_factory=list)  # top-level kernelledger.* dots
    local_internal: list[str] = field(default_factory=list)  # lazy (in-function) kernelledger.* deps
    imports_external: list[str] = field(default_factory=list)  # third-party roots
    stdlib: list[str] = field(default_factory=list)
    optional: bool = False  # True if it imports torch / numpy / pandas / pyarrow


def count_lines(path: Path) -> int:
    try:
        return len(path.read_text(encoding="utf-8", errors="replace").splitlines())
    except Exception:  # noqa: BLE001
        return 0


def first_docstring(tree: ast.AST) -> str:
    doc = ast.get_docstring(tree) or ""
    # collapse whitespace, cap length
    doc = " ".join(doc.split())
    return doc[:240]


def is_stdlib(name: str) -> bool:
    import sys

    return name in set(sys.stdlib_module_names)


OPTIONAL_MARKERS = {"torch", "numpy", "pandas", "pyarrow"}


def resolve_package(rel_path: Path) -> tuple[str, list[str]]:
    """Return (module_dotted_name, package_parts) for a file under PKG_DIR."""
    rel_parts = list(rel_path.relative_to(PKG_DIR).parts)
    if rel_parts[-1] == "__init__.py":
        stem_parts = rel_parts[:-1]
    else:
        stem_parts = rel_parts[:-1] + [rel_parts[-1][:-3]] if rel_parts[-1].endswith(".py") else rel_parts
    full = ["kernelledger", *stem_parts]
    # enclosing package: for --init__ it's the full name; for a module, drop last part
    if rel_parts[-1] == "__init__.py":
        package_parts = full
    else:
        package_parts = ["kernelledger"] + (rel_parts[:-1])
    return ".".join(full), package_parts


def analyze_module(rel_path: Path, name: str) -> Module:
    text = rel_path.read_text(encoding="utf-8", errors="replace")
    _, package_parts = resolve_package(rel_path)
    mod = Module(name=name, file=str(rel_path.relative_to(ROOT)).replace("\\", "/"), lines=count_lines(rel_path))
    try:
        tree = ast.parse(text)
    except SyntaxError:
        return mod
    mod.doc = first_docstring(tree)
    if not tree.body:
        return mod
    # Count classes/functions at top level (skip dunders)
    top_names: dict[str, str] = {}
    for node in tree.body:
        if isinstance(node, ast.ClassDef):
            mod.classes += 1
            if not node.name.startswith("_"):
                mod.public_classes.append(node.name)
                top_names[node.name] = "class"
        elif isinstance(node, ast.FunctionDef) or isinstance(node, ast.AsyncFunctionDef):
            mod.functions += 1
            if not node.name.startswith("_"):
                mod.public_functions.append(node.name)
                top_names[node.name] = "def"
    # Imports: collect top-level vs lazy (inside a function) internal deps,
    # plus external (third-party) and stdlib roots.
    internal: set[str] = set()
    local_internal: set[str] = set()
    external: set[str] = set()
    std: set[str] = set()

    def classify(name_dots: str, local: bool) -> None:
        if not local:
            internal.add(name_dots)
        else:
            local_internal.add(name_dots)

    def walk(node: ast.AST, in_func: bool) -> None:
        for child in ast.iter_child_nodes(node):
            if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                walk(child, in_func or isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)))
            elif isinstance(child, ast.Import):
                for a in child.names:
                    root = a.name.split(".")[0]
                    if a.name.startswith("kernelledger"):
                        if a.name != "kernelledger":
                            classify(a.name, in_func)
                    elif is_stdlib(root):
                        std.add(root)
                    else:
                        external.add(root)
            elif isinstance(child, ast.ImportFrom):
                if child.module is None and child.level == 0:
                    continue
                if child.level and child.level > 0:
                    base = list(package_parts)
                    if child.level - 1 > 0:
                        base = base[: len(base) - (child.level - 1)]
                    base = [p for p in base if p]
                    if child.module:
                        classify(".".join(base + [child.module]), in_func)
                    elif child.names:
                        for a in child.names:
                            classify(".".join(base + [a.name]), in_func)
                    else:
                        classify(".".join(base), in_func)
                else:
                    root = child.module.split(".")[0]
                    if child.module == "kernelledger" or child.module.startswith("kernelledger."):
                        classify(child.module, in_func)
                    elif is_stdlib(root):
                        std.add(root)
                    else:
                        external.add(root)
            else:
                walk(child, in_func)

    walk(tree, False)
    mod.imports_internal = sorted(internal)
    mod.local_internal = sorted(local_internal)
    mod.imports_external = sorted(external)
    mod.stdlib = sorted(std)
    mod.optional = bool(external & OPTIONAL_MARKERS)
    return mod


def build_src_inventory() -> dict:
    modules: list[Module] = []
    for py in sorted(PKG_DIR.rglob("*.py")):
        if "__pycache__" in py.parts:
            continue
        rel = py
        # dotted name from the path relative to the package root
        parts = list(py.relative_to(PKG_DIR).parts)  # e.g. ['gpu', 'backends', 'probe.py'] or ['__init__.py']
        if parts[-1] == "__init__.py":
            parts = parts[:-1]
        else:
            parts[-1] = parts[-1][:-3]  # strip .py
        name = "kernelledger" + ("." + ".".join(parts) if parts else "")
        modules.append(analyze_module(rel, name))

    by_name = {m.name: m for m in modules}

    # Build graph edges (internal imports). Targets are resolved to the nearest
    # existing module by longest-prefix match, so attribute imports like
    # `from .. import __version__` collapse onto the root package node while real
    # submodule imports (`kernelledger.desktop.protocol`) stay precise.
    def resolve(target: str) -> str | None:
        parts = target.split(".")
        for i in range(len(parts), 0, -1):
            candidate = ".".join(parts[:i])
            if candidate in by_name:
                return candidate
        return None

    # Split internal deps into top-level (solid) vs lazy/in-function (dashed).
    # A target present in both is treated as solid.
    solid_edges: list[dict] = []
    dashed_edges: list[dict] = []
    seen = set()
    for m in modules:
        top = set(m.imports_internal)
        lazy = set(m.local_internal)
        for target in top | lazy:
            to = resolve(target)
            if to is None or to == m.name:
                continue
            key = (m.name, to)
            if key in seen:
                continue
            seen.add(key)
            e = {"from": m.name, "to": to}
            (dashed_edges if target in lazy and target not in top else solid_edges).append(e)

    edges = solid_edges  # primary (top-level) edges

    total_lines = sum(m.lines for m in modules)
    total_classes = sum(m.classes for m in modules)
    total_funcs = sum(m.functions for m in modules)

    # group by top-level subpackage
    groups: dict[str, dict] = {}
    for m in modules:
        seg = m.name.split(".")[1] if m.name.startswith("kernelledger.") else ""
        g = groups.setdefault(seg, {"lines": 0, "modules": 0, "classes": 0, "functions": 0})
        g["lines"] += m.lines
        g["modules"] += 1
        g["classes"] += m.classes
        g["functions"] += m.functions

    return {
        "package": "kernelledger",
        "root_dir": str(PKG_DIR),
        "total": {
            "modules": len(modules),
            "lines": total_lines,
            "classes": total_classes,
            "functions": total_funcs,
        },
        "groups": groups,
        "modules": [m.__dict__ for m in modules],
        "edges": solid_edges,
        "dashed": dashed_edges,
    }


def build_group_totals() -> dict:
    groups_dir = {
        "src": ROOT / "src",
        "tests": ROOT / "tests",
        "scripts": ROOT / "scripts",
        "examples": ROOT / "examples",
        "artifacts": ROOT / "artifacts",
        "analysis": ROOT / "analysis",
    }
    out: dict[str, dict] = {}
    for label, d in groups_dir.items():
        if not d.exists():
            continue
        files = [p for p in d.rglob("*.py") if "__pycache__" not in p.parts and ".venv" not in p.parts]
        lines = sum(count_lines(p) for p in files)
        out[label] = {"files": len(files), "lines": lines}
    out["_total"] = {"files": sum(v["files"] for k, v in out.items()), "lines": sum(v["lines"] for k, v in out.items())}
    return out


def build_top_modules() -> dict:
    inv = build_src_inventory()
    ranked = sorted(inv["modules"], key=lambda m: m["lines"], reverse=True)
    top = []
    for m in ranked[:25]:
        top.append({
            "name": m["name"],
            "file": m["file"],
            "lines": m["lines"],
            "classes": m["classes"],
            "functions": m["functions"],
            "optional": m["optional"],
            "doc": m["doc"],
            "public_classes": m["public_classes"][:12],
            "public_functions": m["public_functions"][:16],
        })
    return {"top_modules": top, "total": inv["total"]}


if __name__ == "__main__":
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    inv = build_src_inventory()
    (OUT_DIR / "inv-src.json").write_text(json.dumps(inv, indent=2), encoding="utf-8")
    groups = build_group_totals()
    (OUT_DIR / "inv-groups.json").write_text(json.dumps(groups, indent=2), encoding="utf-8")
    tops = build_top_modules()
    (OUT_DIR / "inv-top.json").write_text(json.dumps(tops, indent=2), encoding="utf-8")
    print("Wrote inventory.")
    print("groups:", json.dumps(groups, indent=2))
    print("top modules by lines:")
    for m in tops["top_modules"][:25]:
        flag = "*" if m["optional"] else " "
        print(f"  {flag} {m['lines']:>6}  {m['name']}  ({m['classes']} classes, {m['functions']} fns)")
