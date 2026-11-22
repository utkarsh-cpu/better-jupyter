"""Static AST & SQL dependency analysis for KernelLedger cells (pure stdlib, strictly passive).

The behaviour is pinned by ``tests/fixtures/dag_cases.json``, which the Rust desktop
re-implementation (``desktop/crates/ui/src/dag.rs``) is tested against as well.
"""

from __future__ import annotations

import ast
import builtins
import heapq
import keyword
import re
from collections import defaultdict, deque
from collections.abc import Callable, Iterable, Mapping
from typing import Any, cast

# Names the KernelLedger worker injects into every namespace; treated like builtins.
_KERNEL_NAMES = frozenset(
    {"display", "update_display", "clear_output", "widget", "display_table", "display_media"}
)
_BUILTINS = frozenset(dir(builtins)) | _KERNEL_NAMES


def _ignored_name(name: str) -> bool:
    """Dunders and the bare `_` (IPython's last-result alias) never take part in the graph."""
    return name == "_" or name.startswith("__")


# ---------------------------------------------------------------------------
# SQL
# ---------------------------------------------------------------------------

# Identifiers after FROM/JOIN that are never notebook variables.
_SQL_RESERVED = frozenset(
    {
        "select",
        "from",
        "where",
        "join",
        "on",
        "using",
        "values",
        "lateral",
        "unnest",
        "with",
        "recursive",
        "as",
        "sqlite_master",
        "pragma",
        "dual",
    }
)
# Words that may follow a table reference and therefore cannot be a bare alias.
_SQL_ALIAS_STOP = frozenset(
    {
        "where",
        "join",
        "inner",
        "left",
        "right",
        "full",
        "outer",
        "cross",
        "natural",
        "positional",
        "asof",
        "anti",
        "semi",
        "lateral",
        "on",
        "using",
        "group",
        "order",
        "having",
        "limit",
        "offset",
        "fetch",
        "union",
        "intersect",
        "except",
        "window",
        "qualify",
        "as",
        "tablesample",
        "sample",
        "pivot",
        "unpivot",
        "returning",
        "set",
        "values",
        "select",
        "from",
        "into",
        "for",
    }
)
# `FUNC(... FROM ...)` forms whose inner FROM is not a table reference.
_SQL_FROM_FUNCTIONS = frozenset({"extract", "substring", "trim", "overlay"})
# Schema qualifiers stripped from `schema.table` (DuckDB's default catalogs/schemas).
_SQL_KNOWN_SCHEMAS = frozenset({"main", "memory", "temp"})
_SQL_IDENT_RE = re.compile(r"[^\W\d][\w$]*")

_SqlToken = tuple[str, str]  # (kind, text); kind in ident|quoted|str|num|punct


def _sql_tokens(sql: str) -> list[_SqlToken]:
    """Tokenize SQL, dropping comments and replacing string literals with a placeholder."""
    tokens: list[_SqlToken] = []
    i = 0
    n = len(sql)
    while i < n:
        ch = sql[i]
        if ch.isspace():
            i += 1
        elif sql.startswith("--", i):
            nl = sql.find("\n", i)
            i = n if nl == -1 else nl + 1
        elif sql.startswith("/*", i):
            end = sql.find("*/", i + 2)
            i = n if end == -1 else end + 2
        elif ch in "'\"`":
            close = ch
            i += 1
            buf: list[str] = []
            while i < n:
                if sql[i] == close:
                    if i + 1 < n and sql[i + 1] == close:
                        buf.append(close)
                        i += 2
                        continue
                    i += 1
                    break
                buf.append(sql[i])
                i += 1
            tokens.append(("str", "") if ch == "'" else ("quoted", "".join(buf)))
        elif ch.isdigit():
            j = i + 1
            while j < n and (sql[j].isalnum() or sql[j] in "._"):
                j += 1
            tokens.append(("num", sql[i:j]))
            i = j
        else:
            match = _SQL_IDENT_RE.match(sql, i)
            if match:
                tokens.append(("ident", match.group(0)))
                i = match.end()
            else:
                tokens.append(("punct", ch))
                i += 1
    return tokens


def _is_punct(tokens: list[_SqlToken], i: int, ch: str) -> bool:
    return i < len(tokens) and tokens[i] == ("punct", ch)


def _is_word(tokens: list[_SqlToken], i: int, *words: str) -> bool:
    return i < len(tokens) and tokens[i][0] == "ident" and tokens[i][1].lower() in words


def _is_name(tokens: list[_SqlToken], i: int) -> bool:
    return i < len(tokens) and tokens[i][0] in ("ident", "quoted")


def _skip_parens(tokens: list[_SqlToken], i: int) -> int:
    """`tokens[i]` is `(`; return the index just past its matching `)`."""
    depth = 0
    while i < len(tokens):
        if tokens[i] == ("punct", "("):
            depth += 1
        elif tokens[i] == ("punct", ")"):
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return i


def _sql_cte_names(tokens: list[_SqlToken]) -> set[str]:
    names: set[str] = set()
    for i in range(len(tokens)):
        if not _is_word(tokens, i, "with"):
            continue
        j = i + 1
        if _is_word(tokens, j, "recursive"):
            j += 1
        while _is_name(tokens, j):
            name = tokens[j][1]
            j += 1
            if _is_punct(tokens, j, "("):
                j = _skip_parens(tokens, j)
            if not _is_word(tokens, j, "as"):
                break
            j += 1
            if _is_word(tokens, j, "not"):
                j += 1
            if _is_word(tokens, j, "materialized"):
                j += 1
            if not _is_punct(tokens, j, "("):
                break
            names.add(name.casefold())
            j = _skip_parens(tokens, j)
            if not _is_punct(tokens, j, ","):
                break
            j += 1
    return names


def analyze_sql_cell(source: str, output_var: str = "_sql_df") -> dict[str, Any]:
    """Extract output definition and referenced table/DataFrame names from a SQL cell."""
    tokens = _sql_tokens(source)
    ctes = _sql_cte_names(tokens)
    refs: dict[str, None] = {}

    def add_ref(parts: list[_SqlToken]) -> None:
        while len(parts) > 1 and parts[0][1].lower() in _SQL_KNOWN_SCHEMAS:
            parts = parts[1:]
        kind, name = parts[0]
        if kind == "ident" and name.lower() in _SQL_RESERVED:
            return
        if not name.isidentifier() or keyword.iskeyword(name) or _ignored_name(name):
            return
        if name.casefold() in ctes:
            return
        refs.setdefault(name)

    def table_list(j: int, allow_comma: bool) -> None:
        while j < len(tokens):
            if _is_punct(tokens, j, "("):
                j = _skip_parens(tokens, j)  # subquery: scanned by the outer loop
            elif _is_word(tokens, j, "lateral"):
                j += 1
                continue
            elif _is_name(tokens, j):
                parts = [tokens[j]]
                j += 1
                while _is_punct(tokens, j, ".") and _is_name(tokens, j + 1):
                    parts.append(tokens[j + 1])
                    j += 2
                if _is_punct(tokens, j, "("):
                    j = _skip_parens(tokens, j)  # table function such as read_csv(...)
                else:
                    add_ref(parts)
            else:
                return
            aliased = False
            if _is_word(tokens, j, "as"):
                j += 1
                if _is_name(tokens, j):
                    j += 1
                    aliased = True
            elif _is_name(tokens, j) and not (
                tokens[j][0] == "ident" and tokens[j][1].lower() in _SQL_ALIAS_STOP
            ):
                j += 1
                aliased = True
            if aliased and _is_punct(tokens, j, "("):
                j = _skip_parens(tokens, j)  # alias column list `t(a, b)`
            if allow_comma and _is_punct(tokens, j, ","):
                j += 1
                continue
            return

    # Paren stack: True when the paren belongs to EXTRACT/SUBSTRING/TRIM/OVERLAY.
    stack: list[bool] = []
    for i, tok in enumerate(tokens):
        if tok == ("punct", "("):
            stack.append(i > 0 and _is_word(tokens, i - 1, *_SQL_FROM_FUNCTIONS))
        elif tok == ("punct", ")"):
            if stack:
                stack.pop()
        elif _is_word(tokens, i, "from", "join"):
            if stack and stack[-1]:
                continue
            table_list(i + 1, allow_comma=tok[1].lower() == "from")

    defs: list[str] = []
    out = (output_var or "").strip()
    if out and out.isidentifier() and not _ignored_name(out):
        defs.append(out)
    return {"defs": defs, "refs": list(refs), "builtin_refs": [], "syntax_error": None}


# ---------------------------------------------------------------------------
# Python scoping
# ---------------------------------------------------------------------------

_COMPREHENSIONS = (ast.ListComp, ast.SetComp, ast.GeneratorExp, ast.DictComp)


def _collect_target_names(node: ast.AST, out: set[str]) -> None:
    if isinstance(node, ast.Name):
        out.add(node.id)
    elif isinstance(node, ast.Starred):
        _collect_target_names(node.value, out)
    elif isinstance(node, (ast.Tuple, ast.List)):
        for elt in node.elts:
            _collect_target_names(elt, out)


def _arg_names(args: ast.arguments) -> set[str]:
    names = {arg.arg for arg in (*args.posonlyargs, *args.args, *args.kwonlyargs)}
    if args.vararg:
        names.add(args.vararg.arg)
    if args.kwarg:
        names.add(args.kwarg.arg)
    return names


def _arg_outer_nodes(args: ast.arguments) -> list[ast.AST]:
    """Defaults and annotations: evaluated in the enclosing scope at definition time."""
    nodes: list[ast.AST] = [*args.defaults]
    nodes.extend(d for d in args.kw_defaults if d is not None)
    for arg in (*args.posonlyargs, *args.args, *args.kwonlyargs, args.vararg, args.kwarg):
        if arg is not None and arg.annotation is not None:
            nodes.append(arg.annotation)
    return nodes


def _scope_bindings(
    nodes: Iterable[ast.AST], *, module: bool
) -> tuple[set[str], set[str], set[str]]:
    """Names bound by `nodes` in their own scope, plus `global` / `nonlocal` declarations.

    Nested function/lambda/class bodies are not entered (only the parts evaluated in this
    scope are), comprehensions are entered except for their (comprehension-local) targets so
    that walrus targets inside them bind here (PEP 572). At module level `del x` and
    `except ... as e` are not bindings.
    """
    bound: set[str] = set()
    globals_: set[str] = set()
    nonlocals: set[str] = set()

    def visit(node: ast.AST) -> None:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            bound.add(node.name)
            for sub in (*node.decorator_list, *_arg_outer_nodes(node.args)):
                visit(sub)
            if node.returns is not None:
                visit(node.returns)
            return
        if isinstance(node, ast.ClassDef):
            bound.add(node.name)
            for sub in (*node.decorator_list, *node.bases, *(k.value for k in node.keywords)):
                visit(sub)
            return
        if isinstance(node, ast.Lambda):
            for sub in _arg_outer_nodes(node.args):
                visit(sub)
            return
        if isinstance(node, _COMPREHENSIONS):
            for gen in node.generators:
                visit(gen.iter)
                for cond in gen.ifs:
                    visit(cond)
            if isinstance(node, ast.DictComp):
                visit(node.key)
                visit(node.value)
            else:
                visit(node.elt)
            return
        if isinstance(node, ast.Name):
            if isinstance(node.ctx, ast.Store) or (isinstance(node.ctx, ast.Del) and not module):
                bound.add(node.id)
            return
        if isinstance(node, ast.ExceptHandler):
            if node.name and not module:
                bound.add(node.name)
        elif isinstance(node, (ast.MatchAs, ast.MatchStar)):
            if node.name:
                bound.add(node.name)
        elif isinstance(node, ast.MatchMapping):
            if node.rest:
                bound.add(node.rest)
        elif isinstance(node, ast.Import):
            bound.update(alias.asname or alias.name.split(".", 1)[0] for alias in node.names)
        elif isinstance(node, ast.ImportFrom):
            bound.update(alias.asname or alias.name for alias in node.names if alias.name != "*")
        elif isinstance(node, ast.Global):
            globals_.update(node.names)
        elif isinstance(node, ast.Nonlocal):
            nonlocals.update(node.names)
        for child in ast.iter_child_nodes(node):
            visit(child)

    for node in nodes:
        visit(node)
    return bound, globals_, nonlocals


class _Scope:
    __slots__ = ("kind", "locals", "globals", "nonlocals")

    def __init__(
        self,
        kind: str,
        locals_: set[str],
        globals_: set[str] | None = None,
        nonlocals: set[str] | None = None,
    ) -> None:
        self.kind = kind  # "function" (def/lambda) | "class" | "comprehension"
        self.locals = locals_
        self.globals = globals_ or set()
        self.nonlocals = nonlocals or set()


class _CellAnalyzer(ast.NodeVisitor):
    """Walks a cell in execution order, recording top-level defs and notebook-level refs."""

    def __init__(self, toplevel_bound: set[str]) -> None:
        # Every name bound anywhere at the cell's top level (used for nested scopes).
        self.toplevel_bound = toplevel_bound
        # Names bound so far at the top level (used for top-level reads).
        self.bound_now: set[str] = set()
        self.scopes: list[_Scope] = []
        self.defs: dict[str, None] = {}
        self.refs: dict[str, None] = {}
        self.builtin_refs: dict[str, None] = {}

    # -- name resolution -------------------------------------------------
    def _bind_toplevel(self, name: str) -> None:
        if _ignored_name(name):
            return
        self.defs.setdefault(name)
        self.bound_now.add(name)

    def _store(self, name: str) -> None:
        # Nested-scope bindings are precomputed into the scope's locals.
        if not self.scopes:
            self._bind_toplevel(name)

    def _load(self, name: str) -> None:
        if _ignored_name(name):
            return
        innermost = True
        for scope in reversed(self.scopes):
            if scope.kind == "class" and not innermost:
                continue  # class bodies are invisible to nested scopes
            innermost = False
            if name in scope.globals:
                break
            if name in scope.locals or name in scope.nonlocals:
                return
        bound = name in self.toplevel_bound if self.scopes else name in self.bound_now
        if bound:
            return
        if name in _BUILTINS:
            self.builtin_refs.setdefault(name)
        else:
            self.refs.setdefault(name)

    # -- names & assignments ---------------------------------------------
    def visit_Name(self, node: ast.Name) -> None:
        if isinstance(node.ctx, ast.Store):
            self._store(node.id)
        else:
            self._load(node.id)

    def visit_Assign(self, node: ast.Assign) -> None:
        self.visit(node.value)
        for target in node.targets:
            self.visit(target)

    def visit_AnnAssign(self, node: ast.AnnAssign) -> None:
        # Annotations of function locals are never evaluated.
        if not (self.scopes and self.scopes[-1].kind == "function"):
            self.visit(node.annotation)
        if node.value is not None:
            self.visit(node.value)
        self.visit(node.target)

    def visit_AugAssign(self, node: ast.AugAssign) -> None:
        if isinstance(node.target, ast.Name):
            self._load(node.target.id)
            self.visit(node.value)
            self._store(node.target.id)
        else:
            self.visit(node.target)
            self.visit(node.value)

    def visit_NamedExpr(self, node: ast.NamedExpr) -> None:
        self.visit(node.value)
        # Walrus binds in the nearest non-comprehension scope (PEP 572).
        if not any(scope.kind != "comprehension" for scope in self.scopes):
            self._bind_toplevel(node.target.id)

    def visit_Import(self, node: ast.Import) -> None:
        for alias in node.names:
            self._store(alias.asname or alias.name.split(".", 1)[0])

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        for alias in node.names:
            if alias.name != "*":
                self._store(alias.asname or alias.name)

    # -- control flow ----------------------------------------------------
    def visit_For(self, node: ast.For) -> None:
        self._visit_for(node)

    def visit_AsyncFor(self, node: ast.AsyncFor) -> None:
        self._visit_for(node)

    def _visit_for(self, node: ast.For | ast.AsyncFor) -> None:
        self.visit(node.iter)
        self.visit(node.target)
        for stmt in (*node.body, *node.orelse):
            self.visit(stmt)

    def visit_ExceptHandler(self, node: ast.ExceptHandler) -> None:
        if node.type is not None:
            self.visit(node.type)
        name = node.name
        if name and not self.scopes and not _ignored_name(name):
            # `e` is visible inside the handler only and is never a cell def.
            was_bound = name in self.bound_now
            self.bound_now.add(name)
            for stmt in node.body:
                self.visit(stmt)
            if not was_bound:
                self.bound_now.discard(name)
            return
        for stmt in node.body:
            self.visit(stmt)

    def visit_MatchAs(self, node: ast.MatchAs) -> None:
        if node.pattern is not None:
            self.visit(node.pattern)
        if node.name:
            self._store(node.name)

    def visit_MatchStar(self, node: ast.MatchStar) -> None:
        if node.name:
            self._store(node.name)

    def visit_MatchMapping(self, node: ast.MatchMapping) -> None:
        for key in node.keys:
            self.visit(key)
        for pattern in node.patterns:
            self.visit(pattern)
        if node.rest:
            self._store(node.rest)

    # -- nested scopes ---------------------------------------------------
    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        self._visit_function(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        self._visit_function(node)

    def _push_type_params(self, node: ast.AST) -> bool:
        """Open the PEP 695 annotation scope holding ``[T, *Ts, **P]`` names, if any."""
        params: list[ast.AST] = getattr(node, "type_params", None) or []
        if not params:
            return False
        names = {name for p in params if isinstance(name := getattr(p, "name", None), str)}
        self.scopes.append(_Scope("function", names))
        for param in params:
            for attr in ("bound", "default_value"):
                value = getattr(param, attr, None)
                if isinstance(value, ast.AST):
                    self.visit(value)
        return True

    def _visit_function(self, node: ast.FunctionDef | ast.AsyncFunctionDef) -> None:
        for sub in node.decorator_list:
            self.visit(sub)
        type_scope = self._push_type_params(node)
        for sub in _arg_outer_nodes(node.args):
            self.visit(sub)
        if node.returns is not None:
            self.visit(node.returns)
        if type_scope:
            self.scopes.pop()
        self._store(node.name)
        bound, globals_, nonlocals = _scope_bindings(node.body, module=False)
        for name in sorted(bound & globals_):
            # `global x; x = ...` inside a function defines x in the notebook namespace.
            if not _ignored_name(name):
                self.defs.setdefault(name)
        local_names = (bound | _arg_names(node.args)) - globals_ - nonlocals
        self.scopes.append(_Scope("function", local_names, globals_, nonlocals))
        for stmt in node.body:
            self.visit(stmt)
        self.scopes.pop()

    def visit_TypeAlias(self, node: ast.AST) -> None:
        # `type Alias[T] = value`: the value is lazily evaluated in an annotation scope.
        type_scope = self._push_type_params(node)
        if not type_scope:
            self.scopes.append(_Scope("function", set()))
        value = getattr(node, "value", None)
        if isinstance(value, ast.AST):
            self.visit(value)
        self.scopes.pop()
        name = getattr(node, "name", None)
        if isinstance(name, ast.Name):
            self._store(name.id)

    def visit_Lambda(self, node: ast.Lambda) -> None:
        for sub in _arg_outer_nodes(node.args):
            self.visit(sub)
        bound, _, _ = _scope_bindings([node.body], module=False)
        self.scopes.append(_Scope("function", bound | _arg_names(node.args)))
        self.visit(node.body)
        self.scopes.pop()

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        for sub in node.decorator_list:
            self.visit(sub)
        type_scope = self._push_type_params(node)
        for sub in (*node.bases, *(k.value for k in node.keywords)):
            self.visit(sub)
        if type_scope:
            self.scopes.pop()
        bound, globals_, nonlocals = _scope_bindings(node.body, module=False)
        for name in sorted(bound & globals_):
            if not _ignored_name(name):
                self.defs.setdefault(name)
        self.scopes.append(_Scope("class", bound - globals_ - nonlocals, globals_, nonlocals))
        for stmt in node.body:
            self.visit(stmt)
        self.scopes.pop()
        self._store(node.name)

    def _visit_comprehension(
        self, generators: list[ast.comprehension], elements: list[ast.expr]
    ) -> None:
        # The outermost iterable is evaluated in the enclosing scope.
        self.visit(generators[0].iter)
        targets: set[str] = set()
        for gen in generators:
            _collect_target_names(gen.target, targets)
        self.scopes.append(_Scope("comprehension", targets))
        for idx, gen in enumerate(generators):
            if idx:
                self.visit(gen.iter)
            self._visit_comprehension_target(gen.target)
            for cond in gen.ifs:
                self.visit(cond)
        for elt in elements:
            self.visit(elt)
        self.scopes.pop()

    def _visit_comprehension_target(self, target: ast.AST) -> None:
        if isinstance(target, ast.Name):
            return
        if isinstance(target, ast.Starred):
            self._visit_comprehension_target(target.value)
        elif isinstance(target, (ast.Tuple, ast.List)):
            for elt in target.elts:
                self._visit_comprehension_target(elt)
        else:
            self.visit(target)

    def visit_ListComp(self, node: ast.ListComp) -> None:
        self._visit_comprehension(node.generators, [node.elt])

    def visit_SetComp(self, node: ast.SetComp) -> None:
        self._visit_comprehension(node.generators, [node.elt])

    def visit_GeneratorExp(self, node: ast.GeneratorExp) -> None:
        self._visit_comprehension(node.generators, [node.elt])

    def visit_DictComp(self, node: ast.DictComp) -> None:
        self._visit_comprehension(node.generators, [node.key, node.value])


# ---------------------------------------------------------------------------
# IPython syntax
# ---------------------------------------------------------------------------

_EXEC_LINE_MAGICS = frozenset({"%time", "%timeit", "%prun"})
_PYTHON_CELL_MAGICS = frozenset({"%%time", "%%timeit", "%%capture"})
_MAGIC_VALUE_FLAGS = frozenset({"-n", "-r", "-p", "-s", "-l", "-T", "-D"})
_MAGIC_FLAG_RE = re.compile(r"-\w+\s*")
_MAGIC_VALUE_RE = re.compile(r"\S+\s*")
_ASSIGN_MAGIC_RE = re.compile(r"^(\s*)([^\W\d]\w*(?:\s*,\s*[^\W\d]\w*)*)\s*=(?!=)\s*[!%]")


def _strip_magic_options(rest: str) -> str:
    while rest.startswith("-"):
        flag = _MAGIC_FLAG_RE.match(rest)
        if flag is None:
            break
        rest = rest[flag.end() :]
        if flag.group(0).strip() in _MAGIC_VALUE_FLAGS:
            value = _MAGIC_VALUE_RE.match(rest)
            if value is not None:
                rest = rest[value.end() :]
    return rest


def _scan_line(line: str, depth: int, triple: str | None) -> tuple[int, str | None, bool]:
    """Advance bracket depth / open triple-quote state over one physical line.

    Returns (depth, open_triple_quote, ends_with_backslash_continuation).
    """
    i = 0
    n = len(line)
    in_comment = False
    while i < n:
        ch = line[i]
        if triple is not None:
            if ch == "\\":
                i += 2
            elif line.startswith(triple, i):
                triple = None
                i += 3
            else:
                i += 1
            continue
        if ch == "#":
            in_comment = True
            break
        if ch in "'\"":
            if line.startswith(ch * 3, i):
                triple = ch * 3
                i += 3
                continue
            i += 1
            while i < n and line[i] != ch:
                i += 2 if line[i] == "\\" else 1
            i += 1
            continue
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth = max(0, depth - 1)
        i += 1
    continued = not in_comment and triple is None and line.rstrip("\r").endswith("\\")
    return depth, triple, continued


def _preprocess_ipython(source: str) -> tuple[str, list[str]] | None:
    """Rewrite IPython syntax into plain Python. Returns None for non-Python cell magics.

    Rewrites keep the line count and indentation so syntax errors point at the right line.
    """
    lines = source.split("\n")
    extra_defs: list[str] = []
    first = lines[0].lstrip()
    if first.startswith("%%"):
        parts = first.split()
        cmd = parts[0].lower()
        if cmd not in _PYTHON_CELL_MAGICS:
            return None
        if cmd == "%%capture":
            names = [p for p in parts[1:] if not p.startswith("-")]
            if names and names[0].isidentifier() and not _ignored_name(names[0]):
                extra_defs.append(names[0])
        lines[0] = ""
    out: list[str] = []
    depth, triple, continued = 0, None, False
    for line in lines:
        if depth == 0 and triple is None and not continued:
            stripped = line.lstrip()
            indent = line[: len(line) - len(stripped)]
            if stripped.startswith("!"):
                line = indent + "pass"
            elif stripped.startswith("%"):
                parts = stripped.split(None, 1)
                rest = _strip_magic_options(parts[1]) if len(parts) == 2 else ""
                if parts[0].lower() in _EXEC_LINE_MAGICS and rest.strip():
                    line = indent + rest
                else:
                    line = indent + "pass"
            elif (assign := _ASSIGN_MAGIC_RE.match(line)) is not None:
                line = f"{assign.group(1)}{assign.group(2)} = None"
            elif stripped.endswith("?") and stripped.rstrip("?").strip():
                # `obj?` / `obj??` help syntax.
                line = indent + stripped.rstrip("?").rstrip()
        out.append(line)
        depth, triple, continued = _scan_line(line, depth, triple)
    return "\n".join(out), extra_defs


def analyze_python_cell(source: str) -> dict[str, Any]:
    """Parse a Python code cell and return top-level defs, free refs, and any syntax error.

    `builtin_refs` lists builtin names read from the notebook namespace; they become real refs
    only when another cell of the notebook defines them (see `analyze_cells`).
    """
    pre = _preprocess_ipython(source)
    if pre is None:
        return {"defs": [], "refs": [], "builtin_refs": [], "syntax_error": None}
    cleaned, extra_defs = pre
    try:
        tree = ast.parse(cleaned)
        toplevel_bound, _, _ = _scope_bindings(tree.body, module=True)
        analyzer = _CellAnalyzer(toplevel_bound)
        for stmt in tree.body:
            analyzer.visit(stmt)
    except (SyntaxError, ValueError, RecursionError) as exc:
        error: dict[str, Any]
        if isinstance(exc, SyntaxError):
            error = {"message": exc.msg or "syntax error", "line": exc.lineno, "offset": exc.offset}
        else:
            error = {"message": str(exc) or type(exc).__name__, "line": None, "offset": None}
        return {"defs": [], "refs": [], "builtin_refs": [], "syntax_error": error}
    defs = list(analyzer.defs)
    defs.extend(name for name in extra_defs if name not in analyzer.defs)
    return {
        "defs": defs,
        "refs": list(analyzer.refs),
        "builtin_refs": list(analyzer.builtin_refs),
        "syntax_error": None,
    }


# ---------------------------------------------------------------------------
# Notebook graph
# ---------------------------------------------------------------------------


def _find_cycles(nodes: list[str], adj: Mapping[str, list[str]]) -> list[list[str]]:
    """Strongly connected components of size > 1 (iterative Tarjan), in input node order."""
    index_of: dict[str, int] = {}
    lowlink: dict[str, int] = {}
    on_stack: set[str] = set()
    stack: list[str] = []
    position = {node: i for i, node in enumerate(nodes)}
    cycles: list[list[str]] = []
    counter = 0
    for root in nodes:
        if root in index_of:
            continue
        work: list[tuple[str, int]] = [(root, 0)]
        while work:
            v, child_idx = work.pop()
            if child_idx == 0:
                index_of[v] = lowlink[v] = counter
                counter += 1
                stack.append(v)
                on_stack.add(v)
            children = adj.get(v, [])
            recursed = False
            while child_idx < len(children):
                w = children[child_idx]
                child_idx += 1
                if w not in index_of:
                    work.append((v, child_idx))
                    work.append((w, 0))
                    recursed = True
                    break
                if w in on_stack:
                    lowlink[v] = min(lowlink[v], index_of[w])
            if recursed:
                continue
            if lowlink[v] == index_of[v]:
                scc: list[str] = []
                while True:
                    w = stack.pop()
                    on_stack.discard(w)
                    scc.append(w)
                    if w == v:
                        break
                if len(scc) > 1:
                    cycles.append(sorted(scc, key=lambda c: position.get(c, 0)))
            if work:
                parent = work[-1][0]
                lowlink[parent] = min(lowlink[parent], lowlink[v])
    cycles.sort(key=lambda scc: position.get(scc[0], 0))
    return cycles


def analyze_cells(cells: list[dict[str, Any]]) -> dict[str, Any]:
    """Build a static dependency graph across notebook code and SQL cells.

    Each ref of a cell gets one edge, from the nearest preceding cell that defines the name,
    or (out-of-order dependency) from the nearest following definer.
    """
    cell_entries: list[dict[str, Any]] = []
    order_index: dict[str, int] = {}
    defined_by: dict[str, list[str]] = defaultdict(list)
    builtin_refs: dict[str, list[str]] = {}

    for cell in cells:
        cid = str(cell.get("id") or "")
        ctype = str(cell.get("type") or "code")
        source = str(cell.get("source") or "")
        raw_meta = cell.get("metadata")
        meta: dict[str, Any] = cast(dict[str, Any], raw_meta) if isinstance(raw_meta, dict) else {}
        if ctype == "code":
            parsed = analyze_python_cell(source)
        elif ctype == "sql":
            parsed = analyze_sql_cell(source, output_var=str(meta.get("output_var") or "_sql_df"))
        else:
            continue
        order_index[cid] = len(cell_entries)
        cell_entries.append(
            {
                "id": cid,
                "type": ctype,
                "defs": list(parsed["defs"]),
                "refs": list(parsed["refs"]),
                "syntax_error": parsed["syntax_error"],
                "disabled": bool(meta.get("disabled", False)),
            }
        )
        builtin_refs[cid] = list(parsed["builtin_refs"])
        for name in parsed["defs"]:
            if cid not in defined_by[name]:
                defined_by[name].append(cid)

    # Builtins count as refs only when another cell of the notebook redefines them.
    for entry in cell_entries:
        cid = entry["id"]
        for name in builtin_refs[cid]:
            if any(d != cid for d in defined_by.get(name, ())) and name not in entry["refs"]:
                entry["refs"].append(name)

    used_by: dict[str, list[str]] = defaultdict(list)
    edge_vars: dict[tuple[str, str], list[str]] = {}
    for entry in cell_entries:
        target_id = entry["id"]
        pos = order_index[target_id]
        for ref in entry["refs"]:
            if target_id not in used_by[ref]:
                used_by[ref].append(target_id)
            definers = [d for d in defined_by.get(ref, ()) if d != target_id]
            before = [d for d in definers if order_index[d] < pos]
            after = [d for d in definers if order_index[d] > pos]
            source_id = before[-1] if before else (after[0] if after else None)
            if source_id is None:
                continue
            names = edge_vars.setdefault((source_id, target_id), [])
            if ref not in names:
                names.append(ref)

    node_ids = [e["id"] for e in cell_entries]
    adj: dict[str, list[str]] = defaultdict(list)
    in_degree = dict.fromkeys(node_ids, 0)
    edges: list[dict[str, Any]] = []
    for (src_id, dst_id), vars_list in sorted(
        edge_vars.items(), key=lambda item: (order_index[item[0][0]], order_index[item[0][1]])
    ):
        edges.append({"from": src_id, "to": dst_id, "vars": vars_list})
        adj[src_id].append(dst_id)
        in_degree[dst_id] += 1

    # Kahn's algorithm, ties broken by notebook position; cycle members appended in order.
    ready = [order_index[cid] for cid in node_ids if in_degree[cid] == 0]
    heapq.heapify(ready)
    topo: list[str] = []
    while ready:
        current = node_ids[heapq.heappop(ready)]
        topo.append(current)
        for nxt in adj.get(current, ()):
            in_degree[nxt] -= 1
            if in_degree[nxt] == 0:
                heapq.heappush(ready, order_index[nxt])
    placed = set(topo)
    topo.extend(cid for cid in node_ids if cid not in placed)

    return {
        "cells": cell_entries,
        "edges": edges,
        "defined_by": dict(sorted(defined_by.items())),
        "used_by": dict(sorted(used_by.items())),
        "multiple_definitions": {
            name: cids for name, cids in sorted(defined_by.items()) if len(cids) > 1
        },
        "cycles": _find_cycles(node_ids, adj),
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


def _upstream_cells(graph: dict[str, Any], cell_id: str) -> set[str]:
    rev: dict[str, list[str]] = defaultdict(list)
    for edge in graph.get("edges", ()):
        rev[edge["to"]].append(edge["from"])
    seen: set[str] = set()
    q: deque[str] = deque(rev.get(cell_id, ()))
    while q:
        cur = q.popleft()
        if cur in seen or cur == cell_id:
            continue
        seen.add(cur)
        q.extend(rev.get(cur, ()))
    return seen


# ---------------------------------------------------------------------------
# Staleness
# ---------------------------------------------------------------------------

_OK_STATUS = "ok"
# Executions that never produced an outcome; they neither refresh nor fail a cell.
_PENDING_STATUSES = frozenset({"queued", "running", "canceled"})


def stale_plan(
    cells: list[dict[str, Any]],
    executed_revisions: Mapping[str, str | None] | None = None,
    *,
    executions: Iterable[Mapping[str, Any]] | None = None,
) -> dict[str, Any]:
    """Compute per-cell staleness, blocking and a topological run plan.

    `executions` are the execution records of the CURRENT kernel session (keys `cell_id`,
    `source_sha256`, `ordinal`, `status`); executions of other sessions must not be passed.
    When omitted, `executed_revisions` (cell id -> last successfully executed source revision)
    is used instead, without ordinals or outcomes.

    Per-cell `state` is one of: disabled, failed, never_run, stale (edited since its last
    successful run), upstream_stale, current.
    """
    graph = analyze_cells(cells)
    by_id = {str(c.get("id")): c for c in cells}

    last_ok_rev: dict[str, str] = {}
    last_ok_ordinal: dict[str, int] = {}
    last_status: dict[str, str] = {}
    last_ordinal: dict[str, int] = {}
    if executions is not None:
        records = [e for e in executions if e.get("cell_id") is not None]
        records.sort(key=lambda e: int(e.get("ordinal") or 0))
        for record in records:
            cid = str(record["cell_id"])
            status = str(record.get("status") or "")
            if status in _PENDING_STATUSES:
                continue
            ordinal = int(record.get("ordinal") or 0)
            last_status[cid] = status
            last_ordinal[cid] = ordinal
            if status == _OK_STATUS:
                last_ok_rev[cid] = str(record.get("source_sha256") or "")
                last_ok_ordinal[cid] = ordinal
    elif executed_revisions is not None:
        for cid, rev in executed_revisions.items():
            if rev is not None:
                last_ok_rev[str(cid)] = rev
                last_status[str(cid)] = _OK_STATUS

    entries = {entry["id"]: entry for entry in graph["cells"]}
    direct_state: dict[str, str] = {}
    for cid, entry in entries.items():
        rev = by_id.get(cid, {}).get("revision")
        if entry["disabled"]:
            direct_state[cid] = "disabled"
        elif cid in last_status and last_status[cid] != _OK_STATUS:
            direct_state[cid] = "failed"
        elif cid not in last_ok_rev:
            direct_state[cid] = "never_run"
        elif rev is not None and last_ok_rev[cid] != rev:
            direct_state[cid] = "stale"
        else:
            direct_state[cid] = "current"
    needs_run = {cid for cid, s in direct_state.items() if s in ("never_run", "stale", "failed")}

    disabled_ids = [cid for cid, entry in entries.items() if entry["disabled"]]
    blocked_by: dict[str, list[str]] = defaultdict(list)
    for cid in disabled_ids:
        for blocked in (cid, *downstream_cells(graph, cid)):
            if cid not in blocked_by[blocked]:
                blocked_by[blocked].append(cid)

    topo_pos = {cid: i for i, cid in enumerate(graph["topological_order"])}
    upstream_reasons: dict[str, list[str]] = {}
    for cid in entries:
        if direct_state[cid] == "disabled" or cid not in last_ok_rev:
            continue  # only cells that themselves ran can hold outdated upstream values
        own = last_ok_ordinal.get(cid)
        reasons = [
            a
            for a in _upstream_cells(graph, cid)
            if direct_state[a] != "disabled"
            and (
                a in needs_run
                or (own is not None and a in last_ok_ordinal and last_ok_ordinal[a] > own)
            )
        ]
        upstream_reasons[cid] = sorted(reasons, key=lambda a: topo_pos.get(a, 0))

    cell_states: list[dict[str, Any]] = []
    run_stale: set[str] = set()
    for cid, entry in entries.items():
        state = direct_state[cid]
        reasons = upstream_reasons.get(cid, [])
        if state == "current" and reasons:
            state = "upstream_stale"
        if (cid in needs_run or reasons) and cid not in blocked_by:
            run_stale.add(cid)
        cell_states.append(
            {
                "cell_id": cid,
                "state": state,
                "upstream_stale_from": reasons,
                "defs": entry["defs"],
                "refs": entry["refs"],
                "disabled": entry["disabled"],
                "syntax_error": entry["syntax_error"],
                "blocked": cid in blocked_by,
                "blocked_by": blocked_by.get(cid, []),
                "last_status": last_status.get(cid),
                "last_ordinal": last_ordinal.get(cid),
                "last_ok_ordinal": last_ok_ordinal.get(cid),
            }
        )

    def ids(predicate: Callable[[str], bool]) -> list[str]:
        return [
            cid for cid in graph["topological_order"] if cid not in blocked_by and predicate(cid)
        ]

    return {
        "graph": graph,
        "cells": cell_states,
        "run_stale_ids": ids(lambda c: c in run_stale),
        "never_run_ids": ids(lambda c: direct_state[c] == "never_run"),
        "direct_stale_ids": ids(lambda c: direct_state[c] == "stale"),
        "failed_ids": ids(lambda c: direct_state[c] == "failed"),
        "upstream_stale_ids": ids(lambda c: bool(upstream_reasons.get(c))),
        "blocked_ids": [cid for cid in graph["topological_order"] if cid in blocked_by],
        "disabled_ids": disabled_ids,
    }


def current_session_id(ledger: Any) -> str | None:
    """The live kernel session: the most recently started session still `running`."""
    rows = ledger.rows(
        "SELECT session_id FROM sessions WHERE state='running' "
        "ORDER BY start DESC, rowid DESC LIMIT 1"
    )
    return str(rows[0]["session_id"]) if rows else None


def notebook_stale_plan(
    notebook: Any, ledger: Any = None, session_id: str | None = None
) -> dict[str, Any]:
    """Compute a complete stale_plan directly from a Notebook and optional Ledger.

    Only executions of `session_id` (default: the running session) count; with no live
    session every executable cell is `never_run`.
    """
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
    executions: list[dict[str, Any]] = []
    sid = session_id
    if ledger is not None:
        if sid is None:
            sid = current_session_id(ledger)
        if sid is not None:
            executions = ledger.rows(
                "SELECT cell_id, source_sha256, ordinal, status FROM executions "
                "WHERE session_id=? AND cell_id IS NOT NULL ORDER BY ordinal ASC",
                (sid,),
            )
    plan = stale_plan(cells, executions=executions)
    plan["session_id"] = sid
    return plan
