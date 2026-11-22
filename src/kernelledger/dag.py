"""Static AST & SQL dependency analysis for KernelLedger cells (pure stdlib, strictly passive)."""

from __future__ import annotations

import ast
import builtins
import re
from collections import defaultdict, deque
from typing import Any

_BUILTINS = frozenset(dir(builtins)) | {
    "__name__",
    "__file__",
    "__doc__",
    "__package__",
    "__builtins__",
    "kl",
    "display",
    "update_display",
}

_SQL_KEYWORDS = frozenset(
    {
        "select",
        "from",
        "where",
        "join",
        "inner",
        "left",
        "right",
        "full",
        "outer",
        "cross",
        "natural",
        "on",
        "using",
        "group",
        "by",
        "order",
        "having",
        "limit",
        "offset",
        "union",
        "all",
        "distinct",
        "as",
        "with",
        "recursive",
        "values",
        "lateral",
        "unnest",
        "generate_series",
        "read_csv",
        "read_csv_auto",
        "read_parquet",
        "read_json",
        "read_json_auto",
        "sqlite_master",
        "pragma",
        "dual",
    }
)

_SQL_CTE_RE = re.compile(r"\b(?:with|recursive|,)\s+([A-Za-z_][A-Za-z0-9_]*)\s+as\s*\(", re.IGNORECASE)
_SQL_TABLE_RE = re.compile(
    r"\b(?:from|join)\s+(?:(?:\"([^\"]+)\")|([A-Za-z_][A-Za-z0-9_]*))",
    re.IGNORECASE,
)


def _strip_sql_comments_and_strings(sql: str) -> str:
    out: list[str] = []
    i = 0
    n = len(sql)
    while i < n:
        ch = sql[i]
        if ch == "-" and i + 1 < n and sql[i + 1] == "-":
            nl = sql.find("\n", i)
            if nl == -1:
                break
            out.append("\n")
            i = nl + 1
        elif ch == "/" and i + 1 < n and sql[i + 1] == "*":
            end = sql.find("*/", i + 2)
            if end == -1:
                break
            out.append(" ")
            i = end + 2
        elif ch == "'":
            i += 1
            while i < n:
                if sql[i] == "'":
                    if i + 1 < n and sql[i + 1] == "'":
                        i += 2
                        continue
                    i += 1
                    break
                i += 1
            out.append(" '' ")
        else:
            out.append(ch)
            i += 1
    return "".join(out)


def analyze_sql_cell(source: str, output_var: str = "_sql_df") -> dict[str, Any]:
    """Extract output definition and referenced table/DataFrame names from a SQL cell."""
    cleaned = _strip_sql_comments_and_strings(source)
    ctes = {m.group(1) for m in _SQL_CTE_RE.finditer(cleaned)}
    refs: list[str] = []
    seen: set[str] = set()
    for match in _SQL_TABLE_RE.finditer(cleaned):
        ident = match.group(1) or match.group(2)
        if not ident:
            continue
        if ident.lower() in _SQL_KEYWORDS or ident in ctes:
            continue
        if ident.isidentifier() and ident not in seen:
            seen.add(ident)
            refs.append(ident)
    defs: list[str] = []
    out = (output_var or "").strip()
    if out and out.isidentifier() and not (out.startswith("__") and out.endswith("__")):
        defs.append(out)
    return {
        "defs": defs,
        "refs": [r for r in refs if r not in defs],
        "syntax_error": None,
    }


def _collect_target_names(node: ast.AST, out: set[str]) -> None:
    if isinstance(node, ast.Name):
        out.add(node.id)
    elif isinstance(node, (ast.Tuple, ast.List, ast.Starred)):
        elts = [node.value] if isinstance(node, ast.Starred) else node.elts
        for elt in elts:
            _collect_target_names(elt, out)


def _collect_pattern_names(pattern: ast.AST, out: set[str]) -> None:
    if isinstance(pattern, ast.MatchAs):
        if pattern.name:
            out.add(pattern.name)
        if pattern.pattern is not None:
            _collect_pattern_names(pattern.pattern, out)
    elif isinstance(pattern, ast.MatchStar):
        if pattern.name:
            out.add(pattern.name)
    elif isinstance(pattern, ast.MatchMapping):
        if pattern.rest:
            out.add(pattern.rest)
        for sub in pattern.patterns:
            _collect_pattern_names(sub, out)
    elif isinstance(pattern, ast.MatchSequence):
        for sub in pattern.patterns:
            _collect_pattern_names(sub, out)
    elif isinstance(pattern, ast.MatchClass):
        for sub in pattern.patterns:
            _collect_pattern_names(sub, out)
        for sub in pattern.kwd_patterns:
            _collect_pattern_names(sub, out)
    elif isinstance(pattern, ast.MatchOr):
        for sub in pattern.patterns:
            _collect_pattern_names(sub, out)


def _arg_names(args: ast.arguments) -> set[str]:
    names: set[str] = set()
    for group in (args.posonlyargs, args.args, args.kwonlyargs):
        for arg in group:
            names.add(arg.arg)
    if args.vararg:
        names.add(args.vararg.arg)
    if args.kwarg:
        names.add(args.kwarg.arg)
    return names


def _collect_local_bindings(body: list[ast.stmt]) -> set[str]:
    """Collect names bound locally inside a function body (excluding nested scopes)."""
    bound: set[str] = set()
    globals_or_nonlocals: set[str] = set()

    def walk(stmts: list[ast.stmt]) -> None:
        for stmt in stmts:
            if isinstance(stmt, (ast.Global, ast.Nonlocal)):
                globals_or_nonlocals.update(stmt.names)
            elif isinstance(stmt, ast.Assign):
                for target in stmt.targets:
                    _collect_target_names(target, bound)
            elif isinstance(stmt, ast.AnnAssign):
                _collect_target_names(stmt.target, bound)
            elif isinstance(stmt, ast.AugAssign):
                _collect_target_names(stmt.target, bound)
            elif isinstance(stmt, (ast.For, ast.AsyncFor)):
                _collect_target_names(stmt.target, bound)
                walk(stmt.body)
                walk(stmt.orelse)
            elif isinstance(stmt, (ast.With, ast.AsyncWith)):
                for item in stmt.items:
                    if item.optional_vars is not None:
                        _collect_target_names(item.optional_vars, bound)
                walk(stmt.body)
            elif isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                bound.add(stmt.name)
            elif isinstance(stmt, ast.Import):
                for alias in stmt.names:
                    bound.add(alias.asname or alias.name.split(".", 1)[0])
            elif isinstance(stmt, ast.ImportFrom):
                for alias in stmt.names:
                    if alias.name != "*":
                        bound.add(alias.asname or alias.name)
            elif isinstance(stmt, ast.If):
                walk(stmt.body)
                walk(stmt.orelse)
            elif isinstance(stmt, ast.While):
                walk(stmt.body)
                walk(stmt.orelse)
            elif isinstance(stmt, ast.Try):
                walk(stmt.body)
                for handler in stmt.handlers:
                    if handler.name:
                        bound.add(handler.name)
                    walk(handler.body)
                walk(stmt.orelse)
                walk(stmt.finalbody)
            elif hasattr(ast, "TryStar") and isinstance(stmt, ast.TryStar):
                walk(stmt.body)
                for handler in stmt.handlers:
                    if handler.name:
                        bound.add(handler.name)
                    walk(handler.body)
                walk(stmt.orelse)
                walk(stmt.finalbody)
            elif hasattr(ast, "Match") and isinstance(stmt, ast.Match):
                for case in stmt.cases:
                    _collect_pattern_names(case.pattern, bound)
                    walk(case.body)

    walk(body)
    return bound - globals_or_nonlocals


class _ScopeAnalyzer(ast.NodeVisitor):
    """Extracts top-level definitions and free variable references from a Python AST."""

    def __init__(self) -> None:
        self.defs_order: list[str] = []
        self.defs_set: set[str] = set()
        self.refs_order: list[str] = []
        self.refs_set: set[str] = set()
        self._scopes: list[set[str]] = []

    def _record_def(self, name: str) -> None:
        if not self._scopes:
            if name not in self.defs_set:
                self.defs_set.add(name)
                self.defs_order.append(name)

    def _record_ref(self, name: str) -> None:
        if name in _BUILTINS:
            return
        for scope in reversed(self._scopes):
            if name in scope:
                return
        if name in self.defs_set:
            return
        if name not in self.refs_set:
            self.refs_set.add(name)
            self.refs_order.append(name)

    def visit_Name(self, node: ast.Name) -> None:
        if isinstance(node.ctx, (ast.Load, ast.Del)):
            self._record_ref(node.id)
        elif isinstance(node.ctx, ast.Store):
            self._record_def(node.id)

    def visit_Assign(self, node: ast.Assign) -> None:
        # Evaluate RHS before binding LHS so `x = x + 1` records `x` as both ref and def.
        self.visit(node.value)
        for target in node.targets:
            self._visit_target(target)

    def visit_AnnAssign(self, node: ast.AnnAssign) -> None:
        self.visit(node.annotation)
        if node.value is not None:
            self.visit(node.value)
        if node.value is not None or not isinstance(node.target, ast.Name):
            self._visit_target(node.target)
        else:
            # Bare `x: int` without value still declares `x` at module level.
            self._record_def(node.target.id)

    def visit_AugAssign(self, node: ast.AugAssign) -> None:
        if isinstance(node.target, ast.Name):
            self._record_ref(node.target.id)
        else:
            self.visit(node.target)
        self.visit(node.value)
        self._visit_target(node.target)

    def visit_NamedExpr(self, node: ast.NamedExpr) -> None:
        self.visit(node.value)
        if isinstance(node.target, ast.Name):
            if self._scopes:
                self._scopes[-1].add(node.target.id)
            else:
                self._record_def(node.target.id)

    def _visit_target(self, target: ast.AST) -> None:
        if isinstance(target, ast.Name):
            if not self._scopes:
                self._record_def(target.id)
        elif isinstance(target, (ast.Tuple, ast.List)):
            for elt in target.elts:
                self._visit_target(elt)
        elif isinstance(target, ast.Starred):
            self._visit_target(target.value)
        elif isinstance(target, ast.Attribute):
            # Mutating `obj.attr = val` references `obj`, does not define a new binding.
            self.visit(target.value)
        elif isinstance(target, ast.Subscript):
            # Mutating `df["col"] = val` references `df` and slice expr.
            self.visit(target.value)
            self.visit(target.slice)

    def visit_Import(self, node: ast.Import) -> None:
        for alias in node.names:
            self._record_def(alias.asname or alias.name.split(".", 1)[0])

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        for alias in node.names:
            if alias.name != "*":
                self._record_def(alias.asname or alias.name)

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        self._visit_function(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        self._visit_function(node)

    def _visit_function(self, node: ast.FunctionDef | ast.AsyncFunctionDef) -> None:
        for dec in node.decorator_list:
            self.visit(dec)
        for default in node.args.defaults:
            self.visit(default)
        for kw_default in node.args.kw_defaults:
            if kw_default is not None:
                self.visit(kw_default)
        for arg in (*node.args.posonlyargs, *node.args.args, *node.args.kwonlyargs):
            if arg.annotation is not None:
                self.visit(arg.annotation)
        if node.args.vararg and node.args.vararg.annotation is not None:
            self.visit(node.args.vararg.annotation)
        if node.args.kwarg and node.args.kwarg.annotation is not None:
            self.visit(node.args.kwarg.annotation)
        if node.returns is not None:
            self.visit(node.returns)
        self._record_def(node.name)
        local_scope = _arg_names(node.args) | _collect_local_bindings(node.body) | {node.name}
        self._scopes.append(local_scope)
        for stmt in node.body:
            self.visit(stmt)
        self._scopes.pop()

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        for dec in node.decorator_list:
            self.visit(dec)
        for base in node.bases:
            self.visit(base)
        for kw in node.keywords:
            self.visit(kw.value)
        self._record_def(node.name)
        local_scope = _collect_local_bindings(node.body) | {node.name}
        self._scopes.append(local_scope)
        for stmt in node.body:
            self.visit(stmt)
        self._scopes.pop()

    def visit_Lambda(self, node: ast.Lambda) -> None:
        for default in node.args.defaults:
            self.visit(default)
        for kw_default in node.args.kw_defaults:
            if kw_default is not None:
                self.visit(kw_default)
        self._scopes.append(_arg_names(node.args))
        self.visit(node.body)
        self._scopes.pop()

    def _visit_comp(self, generators: list[ast.comprehension], elements: list[ast.AST]) -> None:
        comp_scope: set[str] = set()
        self._scopes.append(comp_scope)
        for idx, gen in enumerate(generators):
            if idx == 0:
                # Outermost iterable is evaluated in enclosing scope.
                top = self._scopes.pop()
                self.visit(gen.iter)
                self._scopes.append(top)
            else:
                self.visit(gen.iter)
            _collect_target_names(gen.target, comp_scope)
            for cond in gen.ifs:
                self.visit(cond)
        for elt in elements:
            self.visit(elt)
        self._scopes.pop()

    def visit_ListComp(self, node: ast.ListComp) -> None:
        self._visit_comp(node.generators, [node.elt])

    def visit_SetComp(self, node: ast.SetComp) -> None:
        self._visit_comp(node.generators, [node.elt])

    def visit_GeneratorExp(self, node: ast.GeneratorExp) -> None:
        self._visit_comp(node.generators, [node.elt])

    def visit_DictComp(self, node: ast.DictComp) -> None:
        self._visit_comp(node.generators, [node.key, node.value])

    def visit_For(self, node: ast.For) -> None:
        self.visit(node.iter)
        self._visit_target(node.target)
        for stmt in node.body:
            self.visit(stmt)
        for stmt in node.orelse:
            self.visit(stmt)

    def visit_AsyncFor(self, node: ast.AsyncFor) -> None:
        self.visit(node.iter)
        self._visit_target(node.target)
        for stmt in node.body:
            self.visit(stmt)
        for stmt in node.orelse:
            self.visit(stmt)

    def visit_With(self, node: ast.With) -> None:
        for item in node.items:
            self.visit(item.context_expr)
            if item.optional_vars is not None:
                self._visit_target(item.optional_vars)
        for stmt in node.body:
            self.visit(stmt)

    def visit_AsyncWith(self, node: ast.AsyncWith) -> None:
        for item in node.items:
            self.visit(item.context_expr)
            if item.optional_vars is not None:
                self._visit_target(item.optional_vars)
        for stmt in node.body:
            self.visit(stmt)

    def visit_ExceptHandler(self, node: ast.ExceptHandler) -> None:
        if node.type is not None:
            self.visit(node.type)
        if node.name:
            self._scopes.append({node.name})
            for stmt in node.body:
                self.visit(stmt)
            self._scopes.pop()
        else:
            for stmt in node.body:
                self.visit(stmt)


def _preprocess_magics(source: str) -> tuple[str, list[str]]:
    """Strip known KernelLedger line/cell magics so valid Python bodies still parse."""
    extra_defs: list[str] = []
    lines = source.splitlines()
    if not lines:
        return source, extra_defs
    first = lines[0].lstrip()
    if first.startswith("%%"):
        parts = first.split()
        cmd = parts[0].lower()
        if cmd == "%%capture" and len(parts) >= 2 and parts[1].isidentifier():
            extra_defs.append(parts[1])
        if cmd in ("%%time", "%%timeit", "%%prun", "%%capture"):
            return "\n".join(lines[1:]), extra_defs
        if cmd == "%%writefile":
            return "", extra_defs
    cleaned: list[str] = []
    for line in lines:
        stripped = line.lstrip()
        if stripped.startswith("%"):
            parts = stripped.split(None, 1)
            cmd = parts[0].lower()
            if cmd in ("%time", "%timeit", "%prun") and len(parts) == 2:
                rest = parts[1].lstrip()
                # Strip `-n 2 -r 3` style flags for `%timeit`.
                while rest.startswith("-"):
                    tokens = rest.split(None, 2)
                    if len(tokens) < 3:
                        rest = ""
                        break
                    rest = tokens[2]
                cleaned.append(rest)
            else:
                cleaned.append("")
        elif stripped.endswith("?") and len(stripped) > 1:
            candidate = stripped.rstrip("?").strip()
            cleaned.append(candidate if candidate else "")
        else:
            cleaned.append(line)
    return "\n".join(cleaned), extra_defs


def analyze_python_cell(source: str) -> dict[str, Any]:
    """Parse a Python code cell and return top-level defs, free refs, and any syntax error."""
    cleaned, extra_defs = _preprocess_magics(source)
    try:
        tree = ast.parse(cleaned)
    except SyntaxError as exc:
        return {
            "defs": extra_defs,
            "refs": [],
            "syntax_error": {
                "message": exc.msg or "syntax error",
                "line": exc.lineno,
                "offset": exc.offset,
            },
        }
    analyzer = _ScopeAnalyzer()
    analyzer.visit(tree)
    defs = list(analyzer.defs_order)
    for name in extra_defs:
        if name not in analyzer.defs_set:
            defs.append(name)
    # Exclude private temp `_` from inter-cell dependencies unless explicitly referenced.
    defs = [d for d in defs if d != "_"]
    refs = [r for r in analyzer.refs_order if r != "_"]
    return {
        "defs": defs,
        "refs": refs,
        "syntax_error": None,
    }


def _find_cycles(nodes: list[str], adj: dict[str, list[str]]) -> list[list[str]]:
    """Find strongly connected components that form dependency cycles (Tarjan's algorithm)."""
    index = 0
    stack: list[str] = []
    on_stack: set[str] = set()
    indices: dict[str, int] = {}
    lowlink: dict[str, int] = {}
    cycles: list[list[str]] = []

    def strongconnect(v: str) -> None:
        nonlocal index
        indices[v] = index
        lowlink[v] = index
        index += 1
        stack.append(v)
        on_stack.add(v)
        for w in adj.get(v, ()):
            if w not in indices:
                strongconnect(w)
                lowlink[v] = min(lowlink[v], lowlink[w])
            elif w in on_stack:
                lowlink[v] = min(lowlink[v], indices[w])
        if lowlink[v] == indices[v]:
            scc: list[str] = []
            while True:
                w = stack.pop()
                on_stack.remove(w)
                scc.append(w)
                if w == v:
                    break
            if len(scc) > 1:
                cycles.append(list(reversed(scc)))

    for node in nodes:
        if node not in indices:
            strongconnect(node)
    return cycles


def analyze_cells(cells: list[dict[str, Any]]) -> dict[str, Any]:
    """Build a static dependency graph across notebook code and SQL cells."""
    cell_entries: list[dict[str, Any]] = []
    order_index: dict[str, int] = {}
    defined_by: dict[str, list[str]] = defaultdict(list)
    used_by: dict[str, list[str]] = defaultdict(list)

    for idx, cell in enumerate(cells):
        cid = str(cell.get("id") or "")
        ctype = str(cell.get("type") or "code")
        source = str(cell.get("source") or "")
        meta = cell.get("metadata") if isinstance(cell.get("metadata"), dict) else {}
        disabled = bool(meta.get("disabled", False))
        order_index[cid] = idx
        if ctype == "code":
            parsed = analyze_python_cell(source)
        elif ctype == "sql":
            output_var = str(meta.get("output_var") or "_sql_df")
            parsed = analyze_sql_cell(source, output_var=output_var)
        else:
            continue
        entry = {
            "id": cid,
            "type": ctype,
            "defs": parsed["defs"],
            "refs": parsed["refs"],
            "syntax_error": parsed["syntax_error"],
            "disabled": disabled,
        }
        cell_entries.append(entry)
        for name in parsed["defs"]:
            if cid not in defined_by[name]:
                defined_by[name].append(cid)
        for name in parsed["refs"]:
            if cid not in used_by[name]:
                used_by[name].append(cid)

    edge_vars: dict[tuple[str, str], list[str]] = defaultdict(list)
    adj: dict[str, list[str]] = defaultdict(list)
    rev_adj: dict[str, list[str]] = defaultdict(list)
    node_ids = [e["id"] for e in cell_entries]

    for entry in cell_entries:
        target_id = entry["id"]
        for ref in entry["refs"]:
            for def_id in defined_by.get(ref, ()):
                if def_id == target_id:
                    continue
                if ref not in edge_vars[(def_id, target_id)]:
                    edge_vars[(def_id, target_id)].append(ref)

    edges: list[dict[str, Any]] = []
    for (src_id, dst_id), vars_list in sorted(
        edge_vars.items(), key=lambda item: (order_index.get(item[0][0], 0), order_index.get(item[0][1], 0))
    ):
        edges.append({"from": src_id, "to": dst_id, "vars": vars_list})
        adj[src_id].append(dst_id)
        rev_adj[dst_id].append(src_id)

    # Topological sort (Kahn's algorithm, deterministic tie-breaking by notebook position).
    in_degree = {cid: len(rev_adj.get(cid, ())) for cid in node_ids}
    ready = sorted([cid for cid in node_ids if in_degree[cid] == 0], key=lambda c: order_index[c])
    topo: list[str] = []
    while ready:
        current = ready.pop(0)
        topo.append(current)
        for nxt in adj.get(current, ()):
            in_degree[nxt] -= 1
            if in_degree[nxt] == 0:
                ready.append(nxt)
                ready.sort(key=lambda c: order_index[c])
    # Append any cycle participants in notebook order so every executable cell appears once.
    topo_set = set(topo)
    for cid in node_ids:
        if cid not in topo_set:
            topo.append(cid)

    multiple_definitions = {
        name: cids for name, cids in sorted(defined_by.items()) if len(cids) > 1
    }
    cycles = _find_cycles(node_ids, adj)

    return {
        "cells": cell_entries,
        "edges": edges,
        "defined_by": dict(sorted(defined_by.items())),
        "used_by": dict(sorted(used_by.items())),
        "multiple_definitions": multiple_definitions,
        "cycles": cycles,
        "topological_order": topo,
    }


def downstream_cells(graph: dict[str, Any], cell_id: str) -> list[str]:
    """Return all transitive downstream cell IDs of `cell_id` in topological order."""
    adj: dict[str, list[str]] = defaultdict(list)
    for edge in graph.get("edges", ()):
        adj[edge["from"]].append(edge["to"])
    visited: set[str] = set()
    q: deque[str] = deque(adj.get(cell_id, ()))
    while q:
        cur = q.popleft()
        if cur in visited or cur == cell_id:
            continue
        visited.add(cur)
        for nxt in adj.get(cur, ()):
            if nxt not in visited:
                q.append(nxt)
    topo_pos = {cid: i for i, cid in enumerate(graph.get("topological_order", ()))}
    return sorted(visited, key=lambda c: topo_pos.get(c, 10**9))


def stale_plan(
    cells: list[dict[str, Any]],
    executed_revisions: dict[str, str | None],
) -> dict[str, Any]:
    """Compute direct and upstream-cascade staleness plus topological run plan."""
    graph = analyze_cells(cells)
    by_id = {str(c.get("id")): c for c in cells}
    adj: dict[str, list[str]] = defaultdict(list)
    for edge in graph["edges"]:
        adj[edge["from"]].append(edge["to"])

    direct_state: dict[str, str] = {}
    disabled_ids: set[str] = set()
    for entry in graph["cells"]:
        cid = entry["id"]
        if entry["disabled"]:
            disabled_ids.add(cid)
            direct_state[cid] = "disabled"
            continue
        cell = by_id.get(cid, {})
        rev = cell.get("revision")
        exec_rev = executed_revisions.get(cid)
        if exec_rev is None:
            direct_state[cid] = "never_run"
        elif rev is not None and exec_rev != rev:
            direct_state[cid] = "stale"
        else:
            direct_state[cid] = "current"

    # Propagate staleness downstream from `stale` and `never_run` cells.
    upstream_reasons: dict[str, list[str]] = defaultdict(list)
    for cid, state in direct_state.items():
        if state in ("stale", "never_run"):
            for desc in downstream_cells(graph, cid):
                if desc not in disabled_ids and cid not in upstream_reasons[desc]:
                    upstream_reasons[desc].append(cid)

    cell_states: list[dict[str, Any]] = []
    run_stale_set: set[str] = set()
    for entry in graph["cells"]:
        cid = entry["id"]
        state = direct_state[cid]
        reasons = upstream_reasons.get(cid, [])
        if state == "current" and reasons:
            state = "upstream_stale"
        if state in ("stale", "upstream_stale", "never_run"):
            run_stale_set.add(cid)
        cell_states.append(
            {
                "cell_id": cid,
                "state": state,
                "upstream_stale_from": reasons,
                "defs": entry["defs"],
                "refs": entry["refs"],
                "disabled": entry["disabled"],
                "syntax_error": entry["syntax_error"],
            }
        )

    run_stale_ids = [cid for cid in graph["topological_order"] if cid in run_stale_set]
    return {
        "graph": graph,
        "cells": cell_states,
        "run_stale_ids": run_stale_ids,
    }


def notebook_stale_plan(notebook: Any, ledger: Any = None) -> dict[str, Any]:
    """Compute a complete stale_plan directly from a Notebook and optional Ledger."""
    import hashlib as _hashlib

    data = notebook.read()
    cells: list[dict[str, Any]] = []
    for cell in data.get("cells", []):
        source = notebook.source(cell)
        cells.append(
            {
                "id": cell["id"],
                "type": cell["type"],
                "source": source,
                "revision": _hashlib.sha256(source.encode("utf-8")).hexdigest(),
                "metadata": cell.get("metadata") or {},
            }
        )
    executed_revisions: dict[str, str | None] = {}
    if ledger is not None:
        rows = ledger.rows(
            "SELECT cell_id, source_sha256 FROM executions "
            "WHERE cell_id IS NOT NULL ORDER BY rowid ASC"
        )
        for row in rows:
            cid = row["cell_id"]
            if cid is not None:
                executed_revisions[str(cid)] = row["source_sha256"]
    return stale_plan(cells, executed_revisions)

