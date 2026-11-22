//! Instant pure-Rust static dependency & staleness analyzer for notebook code and SQL cells.
//!
//! Mirrors `kernelledger.dag` (Python, `ast` based) so the GPUI workbench can update DAG
//! badges, provenance links, and "Run stale" / "Run downstream" plans on every keystroke
//! without a worker round-trip. Both implementations follow one behavior spec and are
//! checked against the shared fixture `tests/fixtures/dag_cases.json`.
//!
//! Design:
//! - Python cells are parsed with tree-sitter-python after IPython syntax pre-processing
//!   (`!cmd`, `%magic`, `%%cell` magics). A scope-aware walker over the CST mirrors Python's
//!   symtable rules: module scope tracks "bound before read" in source order; function,
//!   lambda, comprehension, and class scopes resolve free names lexically (class scopes are
//!   invisible to nested scopes), with `global`/`nonlocal`, walrus (PEP 572), match
//!   patterns, `except ... as`, f-string interpolations, decorators, and defaults handled.
//! - SQL cells are tokenized after stripping comments and string literals.
//! - `analyze_notebook` is memoized: a thread-local single-entry cache keyed by a cheap
//!   64-bit fingerprint of every input the analysis reads, plus a bounded per-cell parse
//!   cache so editing one cell re-parses only that cell.

use kernelledger_models::{CellModel, NotebookModel};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::hash::Hasher;
use std::rc::Rc;
use tree_sitter::{Node, Parser};

#[derive(Clone, Debug, Default)]
pub(crate) struct NotebookDag {
    /// Cell id → names the cell binds at its top level (sorted).
    pub defs: BTreeMap<String, Vec<String>>,
    /// Cell id → names the cell reads from the notebook namespace (sorted).
    pub refs: BTreeMap<String, Vec<String>>,
    /// `(from, to, names)` aggregated per cell pair, sorted by `(from, to)`.
    pub edges: Vec<(String, String, Vec<String>)>,
    /// Name → defining cells in notebook order.
    pub defined_by: BTreeMap<String, Vec<String>>,
    /// Name → reading cells in notebook order.
    pub used_by: BTreeMap<String, Vec<String>>,
    /// Names defined by more than one cell (informational only).
    pub multiple_definitions: BTreeMap<String, Vec<String>>,
    /// Strongly connected components with more than one cell, members in notebook order.
    pub cycles: Vec<Vec<String>>,
    /// Kahn order with notebook-order tie-break; cycle members appended in notebook order.
    pub topological_order: Vec<String>,
    /// Cells that need a run because of their own state: edited since their last
    /// successful run, OR never run successfully in this session.
    ///
    /// Never-run cells are deliberately kept here for backward compatibility (callers
    /// treat `direct_stale` as "needs run because of itself"); use `never_run` to tell the
    /// two apart. Failed cells are reported separately in `failed`. Membership is pure
    /// per-cell status and ignores blocking; `run_stale_ids` applies `blocked`.
    pub direct_stale: BTreeSet<String>,
    /// Cells with no successful execution in the current session.
    pub never_run: BTreeSet<String>,
    /// Cells whose last execution errored, was interrupted, or was canceled.
    pub failed: BTreeSet<String>,
    /// Cell that ran successfully → ancestors (notebook order) that make its values
    /// suspect: an ancestor that needs a run, or one that re-ran after this cell did.
    pub upstream_stale_from: BTreeMap<String, Vec<String>>,
    /// Blocked cell → the disabled cell that blocks it (a disabled cell maps to itself).
    pub blocked: BTreeMap<String, String>,
    /// Cell id → syntax error message (such cells have empty defs/refs).
    pub parse_errors: BTreeMap<String, String>,
    /// (never_run ∪ direct_stale ∪ failed ∪ upstream-stale) − blocked, topological order.
    pub run_stale_ids: Vec<String>,
    adj: BTreeMap<String, Vec<String>>,
}

impl NotebookDag {
    /// Transitive descendants of `cell_id` (excluding itself) in topological order.
    ///
    /// Pure graph traversal: disabled/blocked cells are included; filter with
    /// [`NotebookDag::is_blocked`] when building a run plan.
    pub(crate) fn downstream_ids(&self, cell_id: &str) -> Vec<String> {
        let mut visited = BTreeSet::new();
        let mut queue: VecDeque<&String> = VecDeque::new();
        if let Some(nexts) = self.adj.get(cell_id) {
            queue.extend(nexts);
        }
        while let Some(cur) = queue.pop_front() {
            if cur == cell_id || !visited.insert(cur.as_str()) {
                continue;
            }
            if let Some(nexts) = self.adj.get(cur) {
                queue.extend(nexts.iter().filter(|n| !visited.contains(n.as_str())));
            }
        }
        self.topological_order
            .iter()
            .filter(|id| visited.contains(id.as_str()))
            .cloned()
            .collect()
    }

    pub(crate) fn is_upstream_stale(&self, cell_id: &str) -> bool {
        self.upstream_stale_from
            .get(cell_id)
            .is_some_and(|v| !v.is_empty())
    }

    /// True for disabled cells and every transitive descendant of a disabled cell.
    pub(crate) fn is_blocked(&self, cell_id: &str) -> bool {
        self.blocked.contains_key(cell_id)
    }

    pub(crate) fn has_multiple_def(&self, cell_id: &str) -> Vec<String> {
        self.defs
            .get(cell_id)
            .map(|names| {
                names
                    .iter()
                    .filter(|n| self.multiple_definitions.contains_key(*n))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// `sorted(dir(builtins))` from CPython 3.12.
const PY_BUILTINS: &[&str] = &[
    "ArithmeticError",
    "AssertionError",
    "AttributeError",
    "BaseException",
    "BaseExceptionGroup",
    "BlockingIOError",
    "BrokenPipeError",
    "BufferError",
    "BytesWarning",
    "ChildProcessError",
    "ConnectionAbortedError",
    "ConnectionError",
    "ConnectionRefusedError",
    "ConnectionResetError",
    "DeprecationWarning",
    "EOFError",
    "Ellipsis",
    "EncodingWarning",
    "EnvironmentError",
    "Exception",
    "ExceptionGroup",
    "False",
    "FileExistsError",
    "FileNotFoundError",
    "FloatingPointError",
    "FutureWarning",
    "GeneratorExit",
    "IOError",
    "ImportError",
    "ImportWarning",
    "IndentationError",
    "IndexError",
    "InterruptedError",
    "IsADirectoryError",
    "KeyError",
    "KeyboardInterrupt",
    "LookupError",
    "MemoryError",
    "ModuleNotFoundError",
    "NameError",
    "None",
    "NotADirectoryError",
    "NotImplemented",
    "NotImplementedError",
    "OSError",
    "OverflowError",
    "PendingDeprecationWarning",
    "PermissionError",
    "ProcessLookupError",
    "RecursionError",
    "ReferenceError",
    "ResourceWarning",
    "RuntimeError",
    "RuntimeWarning",
    "StopAsyncIteration",
    "StopIteration",
    "SyntaxError",
    "SyntaxWarning",
    "SystemError",
    "SystemExit",
    "TabError",
    "TimeoutError",
    "True",
    "TypeError",
    "UnboundLocalError",
    "UnicodeDecodeError",
    "UnicodeEncodeError",
    "UnicodeError",
    "UnicodeTranslateError",
    "UnicodeWarning",
    "UserWarning",
    "ValueError",
    "Warning",
    "ZeroDivisionError",
    "__build_class__",
    "__debug__",
    "__doc__",
    "__import__",
    "__loader__",
    "__name__",
    "__package__",
    "__spec__",
    "abs",
    "aiter",
    "all",
    "anext",
    "any",
    "ascii",
    "bin",
    "bool",
    "breakpoint",
    "bytearray",
    "bytes",
    "callable",
    "chr",
    "classmethod",
    "compile",
    "complex",
    "copyright",
    "credits",
    "delattr",
    "dict",
    "dir",
    "divmod",
    "enumerate",
    "eval",
    "exec",
    "exit",
    "filter",
    "float",
    "format",
    "frozenset",
    "getattr",
    "globals",
    "hasattr",
    "hash",
    "help",
    "hex",
    "id",
    "input",
    "int",
    "isinstance",
    "issubclass",
    "iter",
    "len",
    "license",
    "list",
    "locals",
    "map",
    "max",
    "memoryview",
    "min",
    "next",
    "object",
    "oct",
    "open",
    "ord",
    "pow",
    "print",
    "property",
    "quit",
    "range",
    "repr",
    "reversed",
    "round",
    "set",
    "setattr",
    "slice",
    "sorted",
    "staticmethod",
    "str",
    "sum",
    "super",
    "tuple",
    "type",
    "vars",
    "zip",
];

/// Names the KernelLedger worker injects into every namespace; treated like builtins.
const KERNEL_NAMES: &[&str] = &[
    "clear_output",
    "display",
    "display_media",
    "display_table",
    "update_display",
    "widget",
];

/// `keyword.kwlist` (hard keywords only; soft keywords are valid identifiers).
const PY_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

fn is_builtin(name: &str) -> bool {
    PY_BUILTINS.binary_search(&name).is_ok() || KERNEL_NAMES.contains(&name)
}

/// Dunders and the bare `_` (IPython's last-result alias) never take part in the graph.
fn is_ignored_name(name: &str) -> bool {
    name == "_" || name.starts_with("__")
}

fn is_python_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || unicode_ident::is_xid_start(first))
        && chars.all(unicode_ident::is_xid_continue)
}

/// Result of analyzing one cell in isolation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CellAnalysis {
    pub defs: Vec<String>,
    pub refs: Vec<String>,
    /// Builtin names read before any binding; they become refs only when some other
    /// notebook cell defines (shadows) them.
    pub builtin_refs: Vec<String>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------------------
// IPython pre-processing
// ---------------------------------------------------------------------------------------

/// Line magics whose argument is a Python statement to analyze.
const EXEC_LINE_MAGICS: &[&str] = &["%time", "%timeit", "%prun"];
/// Cell magics whose body is Python.
const PYTHON_CELL_MAGICS: &[&str] = &["%%time", "%%timeit", "%%capture"];
/// Magic options that take a value (`-n 10`).
const MAGIC_VALUE_FLAGS: &[&str] = &["-n", "-r", "-p", "-s", "-l", "-T", "-D"];

fn is_word_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

#[derive(Default)]
struct LexState {
    depth: usize,
    triple: Option<u8>,
    continuation: bool,
}

impl LexState {
    fn at_logical_start(&self) -> bool {
        self.depth == 0 && self.triple.is_none() && !self.continuation
    }

    /// Advance bracket / triple-quote / backslash state over one physical line.
    fn scan(&mut self, line: &str) {
        let b = line.as_bytes();
        let n = b.len();
        let mut i = 0;
        let mut in_comment = false;
        while i < n {
            if let Some(q) = self.triple {
                if b[i] == b'\\' {
                    i += 2;
                } else if b[i] == q && b.get(i + 1) == Some(&q) && b.get(i + 2) == Some(&q) {
                    self.triple = None;
                    i += 3;
                } else {
                    i += 1;
                }
                continue;
            }
            match b[i] {
                b'#' => {
                    in_comment = true;
                    break;
                }
                q @ (b'\'' | b'"') => {
                    if b.get(i + 1) == Some(&q) && b.get(i + 2) == Some(&q) {
                        self.triple = Some(q);
                        i += 3;
                        continue;
                    }
                    i += 1;
                    while i < n && b[i] != q {
                        i += if b[i] == b'\\' { 2 } else { 1 };
                    }
                }
                b'(' | b'[' | b'{' => self.depth += 1,
                b')' | b']' | b'}' => self.depth = self.depth.saturating_sub(1),
                _ => {}
            }
            i += 1;
        }
        self.continuation =
            !in_comment && self.triple.is_none() && line.trim_end_matches('\r').ends_with('\\');
    }
}

/// Strip leading magic options (`-q`, `-n 10`, ...) from a line-magic argument.
fn strip_magic_options(mut rest: &str) -> &str {
    while let Some(after_dash) = rest.strip_prefix('-') {
        let word_len: usize = after_dash
            .chars()
            .take_while(|c| is_word_char(*c))
            .map(char::len_utf8)
            .sum();
        if word_len == 0 {
            break;
        }
        let flag = &rest[..1 + word_len];
        rest = rest[1 + word_len..].trim_start();
        if MAGIC_VALUE_FLAGS.contains(&flag) && !rest.is_empty() {
            let value_len = rest.find(char::is_whitespace).unwrap_or(rest.len());
            rest = rest[value_len..].trim_start();
        }
    }
    rest
}

/// `names = !cmd` / `names = %magic` → `names = None`.
fn rewrite_assign_magic(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    let mut rest = trimmed;
    let mut names_end;
    loop {
        let mut chars = rest.chars();
        let first = chars.next()?;
        if !(first == '_' || first.is_alphabetic()) {
            return None;
        }
        let ident_len: usize = rest
            .chars()
            .take_while(|c| is_word_char(*c))
            .map(char::len_utf8)
            .sum();
        rest = &rest[ident_len..];
        names_end = trimmed.len() - rest.len();
        let after_ws = rest.trim_start();
        match after_ws.strip_prefix(',') {
            Some(next) => rest = next.trim_start(),
            None => {
                rest = after_ws;
                break;
            }
        }
    }
    let rhs = rest.strip_prefix('=')?;
    if rhs.starts_with('=') {
        return None;
    }
    let rhs = rhs.trim_start();
    if !(rhs.starts_with('!') || rhs.starts_with('%')) {
        return None;
    }
    Some(format!("{indent}{} = None", &trimmed[..names_end]))
}

/// Rewrite one logical line that uses IPython syntax into plain Python.
fn rewrite_magic_line(line: &str) -> Option<String> {
    let stripped = line.trim_start();
    let indent = &line[..line.len() - stripped.len()];
    if stripped.starts_with('!') {
        return Some(format!("{indent}pass"));
    }
    if stripped.starts_with('%') {
        let (cmd, arg) = match stripped.split_once(char::is_whitespace) {
            Some((cmd, arg)) => (cmd, strip_magic_options(arg.trim_start())),
            None => (stripped, ""),
        };
        let cmd = cmd.to_lowercase();
        if EXEC_LINE_MAGICS.contains(&cmd.as_str()) && !arg.trim().is_empty() {
            return Some(format!("{indent}{arg}"));
        }
        return Some(format!("{indent}pass"));
    }
    if let Some(rewritten) = rewrite_assign_magic(line) {
        return Some(rewritten);
    }
    // `obj?` / `obj??` help syntax.
    if let Some(base) = stripped.strip_suffix('?') {
        let base = base.trim_end_matches('?').trim_end();
        if !base.trim().is_empty() {
            return Some(format!("{indent}{base}"));
        }
    }
    None
}

/// IPython syntax pre-processing, shared with the Python analyzer.
///
/// Returns `None` for a non-Python cell magic (no defs/refs), otherwise the rewritten
/// source plus extra defs (`%%capture out` binds `out`). Rewrites keep the line count and
/// indentation, so error positions still point at the original line.
fn preprocess_ipython(src: &str) -> Option<(String, Option<String>)> {
    if !src.contains(['%', '!', '?']) {
        return Some((src.to_owned(), None));
    }
    let mut lines: Vec<String> = src.split('\n').map(str::to_owned).collect();
    let mut extra_def = None;
    let first = lines[0].trim_start();
    if first.starts_with("%%") {
        let parts: Vec<&str> = first.split_whitespace().collect();
        let cmd = parts[0].to_lowercase();
        if !PYTHON_CELL_MAGICS.contains(&cmd.as_str()) {
            return None;
        }
        if cmd == "%%capture"
            && let Some(name) = parts[1..].iter().find(|p| !p.starts_with('-'))
            && is_python_identifier(name)
            && !is_ignored_name(name)
        {
            extra_def = Some((*name).to_owned());
        }
        lines[0].clear();
    }
    let mut st = LexState::default();
    for line in &mut lines {
        if st.at_logical_start()
            && let Some(rewritten) = rewrite_magic_line(line)
        {
            *line = rewritten;
        }
        st.scan(line);
    }
    Some((lines.join("\n"), extra_def))
}

// ---------------------------------------------------------------------------------------
// Python scope walker
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ScopeKind {
    Module,
    Function,
    Class,
    Comprehension,
}

struct Scope {
    kind: ScopeKind,
    parent: usize,
    bound: HashSet<String>,
    globals: HashSet<String>,
    nonlocals: HashSet<String>,
    reads: Vec<String>,
}

const MODULE: usize = 0;

struct Walker<'s> {
    src: &'s [u8],
    scopes: Vec<Scope>,
    cur: usize,
    /// Names bound at module level so far, in source order (top-level reads).
    bound_now: HashSet<String>,
    /// Every name bound anywhere at module level (free reads in nested scopes).
    toplevel_bound: HashSet<String>,
    defs: BTreeSet<String>,
    refs: BTreeSet<String>,
    builtin_refs: BTreeSet<String>,
    error: Option<String>,
}

fn named_children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn fields<'t>(node: Node<'t>) -> Vec<(Option<&'static str>, Node<'t>)> {
    (0..node.child_count() as u32)
        .filter_map(|i| node.child(i).map(|c| (node.field_name_for_child(i), c)))
        .collect()
}

fn position(node: Node<'_>) -> String {
    let p = node.start_position();
    format!("line {}, column {}", p.row + 1, p.column + 1)
}

impl<'s> Walker<'s> {
    fn new(src: &'s [u8]) -> Self {
        Self {
            src,
            scopes: vec![Scope {
                kind: ScopeKind::Module,
                parent: MODULE,
                bound: HashSet::new(),
                globals: HashSet::new(),
                nonlocals: HashSet::new(),
                reads: Vec::new(),
            }],
            cur: MODULE,
            bound_now: HashSet::new(),
            toplevel_bound: HashSet::new(),
            defs: BTreeSet::new(),
            refs: BTreeSet::new(),
            builtin_refs: BTreeSet::new(),
            error: None,
        }
    }

    fn text(&self, node: Node<'_>) -> String {
        node.utf8_text(self.src).unwrap_or_default().to_owned()
    }

    fn fail(&mut self, node: Node<'_>, what: &str) {
        if self.error.is_none() {
            self.error = Some(format!("invalid syntax ({what}) at {}", position(node)));
        }
    }

    fn push_scope(&mut self, kind: ScopeKind, parent: usize) -> usize {
        self.scopes.push(Scope {
            kind,
            parent,
            bound: HashSet::new(),
            globals: HashSet::new(),
            nonlocals: HashSet::new(),
            reads: Vec::new(),
        });
        self.scopes.len() - 1
    }

    fn read(&mut self, name: String) {
        if self.cur != MODULE {
            self.scopes[self.cur].reads.push(name);
        } else if !is_ignored_name(&name) && !self.bound_now.contains(&name) {
            if is_builtin(&name) {
                self.builtin_refs.insert(name);
            } else {
                self.refs.insert(name);
            }
        }
    }

    fn bind_into(&mut self, scope: usize, name: String) {
        if scope == MODULE {
            if !is_ignored_name(&name) {
                self.defs.insert(name.clone());
                self.bound_now.insert(name.clone());
                self.toplevel_bound.insert(name);
            }
        } else {
            self.scopes[scope].bound.insert(name);
        }
    }

    fn bind(&mut self, name: String) {
        self.bind_into(self.cur, name);
    }

    /// PEP 572: a walrus binds in the nearest enclosing non-comprehension scope.
    fn bind_walrus(&mut self, name: String) {
        let mut scope = self.cur;
        while self.scopes[scope].kind == ScopeKind::Comprehension {
            scope = self.scopes[scope].parent;
        }
        self.bind_into(scope, name);
    }

    fn visit_in(&mut self, scope: usize, node: Node<'_>) {
        let saved = self.cur;
        self.cur = scope;
        self.visit(node);
        self.cur = saved;
    }

    fn visit_children(&mut self, node: Node<'_>) {
        for child in named_children(node) {
            self.visit(child);
        }
    }

    fn visit(&mut self, node: Node<'_>) {
        match node.kind() {
            "identifier" => {
                let name = self.text(node);
                self.read(name);
            }
            "comment"
            | "string_start"
            | "string_content"
            | "string_end"
            | "escape_sequence"
            | "escape_interpolation"
            | "type_conversion"
            | "integer"
            | "float"
            | "true"
            | "false"
            | "none"
            | "ellipsis"
            | "pass_statement"
            | "break_statement"
            | "continue_statement"
            | "keyword_separator"
            | "positional_separator"
            | "line_continuation"
            | "wildcard_import"
            | "import_prefix" => {}
            "attribute" => {
                if let Some(object) = node.child_by_field_name("object") {
                    self.visit(object);
                }
            }
            "keyword_argument" => {
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit(value);
                }
            }
            "expression_statement" => {
                let children = named_children(node);
                if children.len() == 1 && children[0].kind() == "named_expression" {
                    self.fail(node, "unparenthesized assignment expression");
                }
                for child in children {
                    self.visit(child);
                }
            }
            "assignment" => self.assignment(node),
            "augmented_assignment" => {
                let left = node.child_by_field_name("left");
                if let Some(left) = left {
                    if left.kind() == "identifier" {
                        let name = self.text(left);
                        self.read(name);
                    } else {
                        self.visit(left);
                    }
                }
                if let Some(right) = node.child_by_field_name("right") {
                    self.visit(right);
                }
                if let Some(left) = left {
                    self.target(left);
                }
            }
            "named_expression" => {
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit(value);
                }
                if let Some(name) = node.child_by_field_name("name") {
                    let name = self.text(name);
                    self.bind_walrus(name);
                }
            }
            "for_statement" => {
                if let Some(right) = node.child_by_field_name("right") {
                    self.visit(right);
                }
                if let Some(left) = node.child_by_field_name("left") {
                    self.target(left);
                }
                for field in ["body", "alternative"] {
                    if let Some(child) = node.child_by_field_name(field) {
                        self.visit(child);
                    }
                }
            }
            "as_pattern" => {
                self.as_pattern(node, false);
            }
            "as_pattern_target" => {
                for child in named_children(node) {
                    self.target(child);
                }
            }
            "except_clause" | "except_group_clause" => self.except_clause(node),
            "function_definition" => self.function(node),
            "class_definition" => self.class(node),
            "lambda" => self.lambda(node),
            "list_comprehension"
            | "set_comprehension"
            | "dictionary_comprehension"
            | "generator_expression" => self.comprehension(node),
            "import_statement" => {
                let mut cursor = node.walk();
                let names: Vec<Node<'_>> =
                    node.children_by_field_name("name", &mut cursor).collect();
                for name in names {
                    let bound = if name.kind() == "aliased_import" {
                        name.child_by_field_name("alias").map(|a| self.text(a))
                    } else {
                        named_children(name).first().map(|first| self.text(*first))
                    };
                    if let Some(bound) = bound {
                        self.bind(bound);
                    }
                }
            }
            "import_from_statement" | "future_import_statement" => {
                let mut cursor = node.walk();
                let names: Vec<Node<'_>> =
                    node.children_by_field_name("name", &mut cursor).collect();
                for name in names {
                    let bound = if name.kind() == "aliased_import" {
                        name.child_by_field_name("alias").map(|a| self.text(a))
                    } else {
                        Some(self.text(name))
                    };
                    if let Some(bound) = bound {
                        self.bind(bound);
                    }
                }
            }
            "global_statement" | "nonlocal_statement" => {
                if self.cur != MODULE {
                    let global = node.kind() == "global_statement";
                    for child in named_children(node) {
                        if child.kind() == "identifier" {
                            let name = self.text(child);
                            let scope = &mut self.scopes[self.cur];
                            if global {
                                scope.globals.insert(name);
                            } else {
                                scope.nonlocals.insert(name);
                            }
                        }
                    }
                }
            }
            "match_statement" => {
                for (field, child) in fields(node) {
                    match field {
                        Some("subject") => self.visit(child),
                        Some("body") => {
                            for case in named_children(child) {
                                if case.kind() == "case_clause" {
                                    self.case_clause(case);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            "type_alias_statement" => self.type_alias(node),
            "member_type" => {
                if let Some(first) = named_children(node).first() {
                    self.visit(*first);
                }
            }
            "dotted_name" => {
                if let Some(first) = named_children(node).first() {
                    let name = self.text(*first);
                    self.read(name);
                }
            }
            "print_statement" => self.fail(node, "Python 2 print statement"),
            "exec_statement" => self.fail(node, "Python 2 exec statement"),
            _ => self.visit_children(node),
        }
    }

    fn assignment(&mut self, node: Node<'_>) {
        let mut targets = Vec::new();
        let mut annotations = Vec::new();
        let mut value = None;
        let mut cur = node;
        loop {
            if let Some(left) = cur.child_by_field_name("left") {
                targets.push(left);
            }
            if let Some(ty) = cur.child_by_field_name("type") {
                annotations.push(ty);
            }
            match cur.child_by_field_name("right") {
                Some(right) if right.kind() == "assignment" => cur = right,
                Some(right) if right.kind() == "augmented_assignment" => {
                    self.fail(right, "chained augmented assignment");
                    break;
                }
                right => {
                    value = right;
                    break;
                }
            }
        }
        if let Some(value) = value {
            self.visit(value);
        }
        // Annotations of function locals are never evaluated.
        if self.scopes[self.cur].kind != ScopeKind::Function {
            for annotation in annotations {
                self.visit(annotation);
            }
        }
        for target in targets {
            self.target(target);
        }
    }

    /// Bind an assignment-like target; attribute/subscript targets read their bases.
    fn target(&mut self, node: Node<'_>) {
        match node.kind() {
            "identifier" => {
                let name = self.text(node);
                self.bind(name);
            }
            "pattern_list"
            | "tuple_pattern"
            | "list_pattern"
            | "tuple"
            | "list"
            | "expression_list"
            | "parenthesized_expression"
            | "as_pattern_target"
            | "list_splat_pattern"
            | "list_splat" => {
                for child in named_children(node) {
                    self.target(child);
                }
            }
            "comment" => {}
            _ => self.visit(node),
        }
    }

    /// `expr as target` in `with` items; returns the alias node for `except` clauses.
    fn as_pattern<'t>(&mut self, node: Node<'t>, except_binding: bool) -> Option<Node<'t>> {
        let alias = node.child_by_field_name("alias");
        for child in named_children(node) {
            if Some(child) != alias {
                self.visit(child);
            }
        }
        match alias {
            Some(alias) if except_binding => return Some(alias),
            Some(alias) => self.visit(alias),
            None => {}
        }
        None
    }

    fn except_clause(&mut self, node: Node<'_>) {
        // Python 3.14 (PEP 758) reads `except A, B:` as a tuple of exception types.
        let comma_list = fields(node)
            .iter()
            .any(|(_, c)| !c.is_named() && c.kind() == ",");
        let alias_field = node.child_by_field_name("alias");
        let mut header = Vec::new();
        let mut blocks = Vec::new();
        for child in named_children(node) {
            match child.kind() {
                "block" => blocks.push(child),
                "comment" => {}
                _ if Some(child) == alias_field => {}
                _ => header.push(child),
            }
        }
        let mut alias = None;
        for (i, child) in header.iter().enumerate() {
            if child.kind() == "as_pattern" {
                alias = self.as_pattern(*child, true);
            } else if node.kind() == "except_group_clause" && i == 1 {
                alias = Some(*child);
            } else {
                self.visit(*child);
            }
        }
        if let Some(a) = alias_field {
            if comma_list {
                self.visit(a);
            } else {
                alias = Some(a);
            }
        }
        let alias = alias.map(|a| {
            if a.kind() == "as_pattern_target" {
                named_children(a).first().copied().unwrap_or(a)
            } else {
                a
            }
        });
        // `except E as e`: `e` is visible only inside the handler (Python deletes it
        // afterwards), so at module level it suppresses refs there but is never a def.
        let mut handler_name = None;
        match alias {
            Some(a) if a.kind() == "identifier" => {
                let name = self.text(a);
                if self.cur != MODULE {
                    self.scopes[self.cur].bound.insert(name);
                } else if !is_ignored_name(&name) {
                    let was_bound = !self.bound_now.insert(name.clone());
                    handler_name = (!was_bound).then_some(name);
                }
            }
            Some(a) => self.target(a),
            None => {}
        }
        for block in blocks {
            self.visit(block);
        }
        if let Some(name) = handler_name {
            self.bound_now.remove(&name);
        }
    }

    fn collect_param_names(&self, node: Node<'_>, out: &mut Vec<String>) {
        if node.kind() == "identifier" {
            out.push(self.text(node));
        } else {
            for child in named_children(node) {
                self.collect_param_names(child, out);
            }
        }
    }

    /// Collect parameter names; defaults are evaluated in `default_scope` and annotations
    /// in `annotation_scope` (both at definition time, outside the new scope).
    fn parameters(
        &mut self,
        params: Option<Node<'_>>,
        default_scope: usize,
        annotation_scope: usize,
    ) -> Vec<String> {
        let mut names = Vec::new();
        let Some(params) = params else {
            return names;
        };
        for param in named_children(params) {
            match param.kind() {
                "identifier" => names.push(self.text(param)),
                "list_splat_pattern" | "dictionary_splat_pattern" | "tuple_pattern" => {
                    self.collect_param_names(param, &mut names);
                }
                "typed_parameter" | "default_parameter" | "typed_default_parameter" => {
                    let ty = param.child_by_field_name("type");
                    let value = param.child_by_field_name("value");
                    for child in named_children(param) {
                        if Some(child) != ty && Some(child) != value && child.kind() != "comment" {
                            self.collect_param_names(child, &mut names);
                        }
                    }
                    if let Some(value) = value {
                        self.visit_in(default_scope, value);
                    }
                    if let Some(ty) = ty {
                        self.visit_in(annotation_scope, ty);
                    }
                }
                _ => {}
            }
        }
        names
    }

    /// PEP 695 type parameters live in an annotation scope between the definition's
    /// enclosing scope and its body.
    fn type_param_scope(&mut self, type_params: Option<Node<'_>>) -> Option<usize> {
        let type_params = type_params?;
        let scope = self.push_scope(ScopeKind::Function, self.cur);
        let mut later = Vec::new();
        for ty in named_children(type_params) {
            let Some(inner) = named_children(ty).first().copied() else {
                continue;
            };
            match inner.kind() {
                "identifier" => {
                    let name = self.text(inner);
                    self.scopes[scope].bound.insert(name);
                }
                "splat_type" | "constrained_type" => {
                    let parts = named_children(inner);
                    let mut first = parts.first().copied();
                    while let Some(node) = first.filter(|n| n.kind() != "identifier") {
                        first = named_children(node).first().copied();
                    }
                    if let Some(first) = first {
                        let name = self.text(first);
                        self.scopes[scope].bound.insert(name);
                    }
                    later.extend(parts.into_iter().skip(1));
                }
                _ => later.push(inner),
            }
        }
        for node in later {
            self.visit_in(scope, node);
        }
        Some(scope)
    }

    fn function(&mut self, node: Node<'_>) {
        let outer = self.cur;
        let annotation = self
            .type_param_scope(node.child_by_field_name("type_parameters"))
            .unwrap_or(outer);
        let params = self.parameters(node.child_by_field_name("parameters"), outer, annotation);
        if let Some(ret) = node.child_by_field_name("return_type") {
            self.visit_in(annotation, ret);
        }
        let body_scope = self.push_scope(ScopeKind::Function, annotation);
        self.scopes[body_scope].bound.extend(params);
        if let Some(body) = node.child_by_field_name("body") {
            self.visit_in(body_scope, body);
        }
        if let Some(name) = node.child_by_field_name("name") {
            let name = self.text(name);
            self.bind(name);
        }
    }

    fn class(&mut self, node: Node<'_>) {
        let outer = self.cur;
        let annotation = self
            .type_param_scope(node.child_by_field_name("type_parameters"))
            .unwrap_or(outer);
        if let Some(bases) = node.child_by_field_name("superclasses") {
            self.visit_in(annotation, bases);
        }
        let body_scope = self.push_scope(ScopeKind::Class, annotation);
        if let Some(body) = node.child_by_field_name("body") {
            self.visit_in(body_scope, body);
        }
        if let Some(name) = node.child_by_field_name("name") {
            let name = self.text(name);
            self.bind(name);
        }
    }

    fn lambda(&mut self, node: Node<'_>) {
        let outer = self.cur;
        let params = self.parameters(node.child_by_field_name("parameters"), outer, outer);
        let scope = self.push_scope(ScopeKind::Function, outer);
        self.scopes[scope].bound.extend(params);
        if let Some(body) = node.child_by_field_name("body") {
            self.visit_in(scope, body);
        }
    }

    fn comprehension(&mut self, node: Node<'_>) {
        let outer = self.cur;
        let body = node.child_by_field_name("body");
        let scope = self.push_scope(ScopeKind::Comprehension, outer);
        let mut first = true;
        for clause in named_children(node) {
            match clause.kind() {
                "for_in_clause" => {
                    let mut cursor = clause.walk();
                    let rights: Vec<Node<'_>> = clause
                        .children_by_field_name("right", &mut cursor)
                        .collect();
                    // The outermost iterable is evaluated in the enclosing scope.
                    let iter_scope = if first { outer } else { scope };
                    for right in rights {
                        self.visit_in(iter_scope, right);
                    }
                    if let Some(left) = clause.child_by_field_name("left") {
                        self.cur = scope;
                        self.target(left);
                        self.cur = outer;
                    }
                    first = false;
                }
                "if_clause" => {
                    for child in named_children(clause) {
                        self.visit_in(scope, child);
                    }
                }
                _ => {}
            }
        }
        if let Some(body) = body {
            self.visit_in(scope, body);
        }
    }

    fn type_alias(&mut self, node: Node<'_>) {
        let outer = self.cur;
        let mut name = None;
        let mut type_params = None;
        if let Some(left) = node.child_by_field_name("left") {
            let inner = named_children(left).first().copied();
            match inner {
                Some(n) if n.kind() == "identifier" => name = Some(self.text(n)),
                Some(n) if n.kind() == "generic_type" => {
                    for child in named_children(n) {
                        match child.kind() {
                            "identifier" => name = Some(self.text(child)),
                            "type_parameter" => type_params = Some(child),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        let scope = self.type_param_scope(type_params).unwrap_or_else(|| {
            // The alias value is lazily evaluated in its own annotation scope.
            self.push_scope(ScopeKind::Function, outer)
        });
        if let Some(right) = node.child_by_field_name("right") {
            self.visit_in(scope, right);
        }
        if let Some(name) = name {
            self.bind(name);
        }
    }

    fn case_clause(&mut self, node: Node<'_>) {
        for child in named_children(node) {
            match child.kind() {
                "case_pattern" => self.pattern(child),
                "if_clause" | "block" => self.visit(child),
                _ => {}
            }
        }
    }

    fn pattern(&mut self, node: Node<'_>) {
        match node.kind() {
            "case_pattern" | "union_pattern" | "list_pattern" | "tuple_pattern" => {
                for child in named_children(node) {
                    self.pattern(child);
                }
            }
            "as_pattern" | "splat_pattern" => {
                for child in named_children(node) {
                    if child.kind() == "identifier" {
                        let name = self.text(child);
                        self.bind(name);
                    } else {
                        self.pattern(child);
                    }
                }
            }
            "keyword_pattern" => {
                for child in named_children(node).into_iter().skip(1) {
                    self.pattern(child);
                }
            }
            "dict_pattern" => {
                for (field, child) in fields(node) {
                    if !child.is_named() {
                        continue;
                    }
                    if field == Some("key") {
                        if child.kind() == "dotted_name" {
                            self.visit(child);
                        }
                    } else {
                        self.pattern(child);
                    }
                }
            }
            "class_pattern" => {
                for child in named_children(node) {
                    if child.kind() == "dotted_name" {
                        self.visit(child);
                    } else {
                        self.pattern(child);
                    }
                }
            }
            "dotted_name" => {
                let parts = named_children(node);
                if parts.len() == 1 {
                    let name = self.text(parts[0]);
                    self.bind(name);
                } else {
                    self.visit(node);
                }
            }
            _ => {}
        }
    }

    /// Resolve a free name read inside a nested scope: does it reach module scope?
    fn reaches_module(&self, start: usize, name: &str) -> bool {
        let mut scope = start;
        loop {
            if scope == MODULE {
                return true;
            }
            let s = &self.scopes[scope];
            // Class scopes are only visible to their own body, never to nested scopes.
            if scope == start || s.kind != ScopeKind::Class {
                if s.globals.contains(name) {
                    return true;
                }
                if s.bound.contains(name) || s.nonlocals.contains(name) {
                    return false;
                }
            }
            scope = s.parent;
        }
    }

    fn finish(mut self) -> CellAnalysis {
        // `global x; x = ...` in a function/class body defines x in the notebook namespace.
        for scope in &self.scopes[1..] {
            for name in scope.globals.intersection(&scope.bound) {
                if !is_ignored_name(name) {
                    self.defs.insert(name.clone());
                }
            }
        }
        let mut refs = std::mem::take(&mut self.refs);
        let mut builtin_refs = std::mem::take(&mut self.builtin_refs);
        for (idx, scope) in self.scopes.iter().enumerate().skip(1) {
            for name in &scope.reads {
                if is_ignored_name(name)
                    || self.toplevel_bound.contains(name)
                    || !self.reaches_module(idx, name)
                {
                    continue;
                }
                if is_builtin(name) {
                    builtin_refs.insert(name.clone());
                } else {
                    refs.insert(name.clone());
                }
            }
        }
        CellAnalysis {
            defs: self.defs.into_iter().collect(),
            refs: refs.into_iter().collect(),
            builtin_refs: builtin_refs.into_iter().collect(),
            error: None,
        }
    }
}

/// First ERROR/MISSING node, or a statement whose indentation Python would reject
/// (tree-sitter silently accepts e.g. an unexpected indent at module level).
fn find_syntax_error(root: Node<'_>) -> Option<String> {
    fn first_error(node: Node<'_>) -> Option<Node<'_>> {
        if node.is_error() || node.is_missing() {
            return Some(node);
        }
        if !node.has_error() {
            return None;
        }
        (0..node.child_count() as u32)
            .filter_map(|i| node.child(i))
            .find_map(first_error)
            .or(Some(node))
    }
    fn bad_indent(node: Node<'_>) -> Option<Node<'_>> {
        if matches!(node.kind(), "module" | "block") {
            let stmts: Vec<Node<'_>> = named_children(node)
                .into_iter()
                .filter(|c| c.kind() != "comment")
                .collect();
            if node.kind() == "block" && stmts.is_empty() {
                return Some(node);
            }
            if let Some(first) = stmts.first() {
                let expected = if node.kind() == "module" {
                    0
                } else {
                    first.start_position().column
                };
                let mut prev_row = None;
                for stmt in &stmts {
                    let pos = stmt.start_position();
                    if prev_row != Some(pos.row) && pos.column != expected {
                        return Some(*stmt);
                    }
                    prev_row = Some(stmt.end_position().row);
                }
            }
        }
        named_children(node).into_iter().find_map(bad_indent)
    }
    if root.has_error() {
        let node = first_error(root).unwrap_or(root);
        return Some(format!("invalid syntax at {}", position(node)));
    }
    bad_indent(root).map(|node| {
        let what = if node.kind() == "block" {
            "expected an indented block"
        } else {
            "unexpected indent"
        };
        format!("{what} at {}", position(node))
    })
}

thread_local! {
    static PY_PARSER: RefCell<Option<Parser>> = const { RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static PARSE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Analyze one Python cell: names bound at its top level and names it reads from the
/// notebook namespace (see the shared spec for the exact rules).
pub(crate) fn analyze_python_cell(source: &str) -> CellAnalysis {
    #[cfg(test)]
    PARSE_COUNT.with(|c| c.set(c.get() + 1));
    let Some((source, extra_def)) = preprocess_ipython(source) else {
        return CellAnalysis::default();
    };
    let tree = PY_PARSER.with(|cell| {
        let mut slot = cell.borrow_mut();
        let parser = slot.get_or_insert_with(|| {
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_python::LANGUAGE.into())
                .expect("tree-sitter-python grammar is ABI compatible");
            parser
        });
        parser.parse(&source, None)
    });
    let Some(tree) = tree else {
        return CellAnalysis {
            error: Some("parser failed".into()),
            ..Default::default()
        };
    };
    let root = tree.root_node();
    if let Some(error) = find_syntax_error(root) {
        return CellAnalysis {
            error: Some(error),
            ..Default::default()
        };
    }
    let mut walker = Walker::new(source.as_bytes());
    walker.visit(root);
    if let Some(error) = walker.error.take() {
        return CellAnalysis {
            error: Some(error),
            ..Default::default()
        };
    }
    walker.defs.extend(extra_def);
    walker.finish()
}

// ---------------------------------------------------------------------------------------
// SQL cells
// ---------------------------------------------------------------------------------------

/// Identifiers after FROM/JOIN that are never notebook variables.
const SQL_RESERVED: &[&str] = &[
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
];

/// Words that may follow a table reference and therefore cannot be a bare alias.
const SQL_ALIAS_STOP: &[&str] = &[
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
];

/// `FUNC(... FROM ...)` forms whose inner FROM is not a table reference.
const SQL_FROM_FUNCTIONS: &[&str] = &["extract", "substring", "trim", "overlay"];

/// Schema qualifiers stripped from `schema.table` (DuckDB's default catalogs/schemas).
const SQL_KNOWN_SCHEMAS: &[&str] = &["main", "memory", "temp"];

fn in_word_list(list: &[&str], word: &str) -> bool {
    list.iter().any(|w| w.eq_ignore_ascii_case(word))
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SqlTok {
    Ident(String),
    Quoted(String),
    Str,
    Num,
    Punct(char),
}

impl SqlTok {
    fn is_word(&self, words: &[&str]) -> bool {
        matches!(self, SqlTok::Ident(w) if in_word_list(words, w))
    }

    fn name(&self) -> Option<&str> {
        match self {
            SqlTok::Ident(n) | SqlTok::Quoted(n) => Some(n),
            _ => None,
        }
    }
}

/// Tokenize SQL, dropping `--` / `/* */` comments and replacing `'...'` literals.
fn sql_tokens(sql: &str) -> Vec<SqlTok> {
    let mut tokens = Vec::new();
    let mut i = 0;
    let n = sql.len();
    while i < n {
        let rest = &sql[i..];
        let ch = rest.chars().next().unwrap_or(' ');
        if ch.is_whitespace() {
            i += ch.len_utf8();
        } else if rest.starts_with("--") {
            i = rest.find('\n').map_or(n, |nl| i + nl + 1);
        } else if let Some(body) = rest.strip_prefix("/*") {
            i = body.find("*/").map_or(n, |end| i + 2 + end + 2);
        } else if matches!(ch, '\'' | '"' | '`') {
            let mut buf = String::new();
            let mut chars = rest[1..].char_indices().peekable();
            let mut consumed = rest.len();
            while let Some((j, c)) = chars.next() {
                if c == ch {
                    if chars.peek().is_some_and(|(_, next)| *next == ch) {
                        buf.push(ch);
                        chars.next();
                        continue;
                    }
                    consumed = 1 + j + 1;
                    break;
                }
                buf.push(c);
            }
            i += consumed;
            tokens.push(if ch == '\'' {
                SqlTok::Str
            } else {
                SqlTok::Quoted(buf)
            });
        } else if ch.is_ascii_digit() {
            let len: usize = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '.' | '_'))
                .map(char::len_utf8)
                .sum();
            i += len.max(ch.len_utf8());
            tokens.push(SqlTok::Num);
        } else if ch == '_' || ch.is_alphabetic() {
            let len: usize = rest
                .chars()
                .take_while(|c| *c == '_' || *c == '$' || c.is_alphanumeric())
                .map(char::len_utf8)
                .sum();
            tokens.push(SqlTok::Ident(rest[..len].to_owned()));
            i += len;
        } else {
            tokens.push(SqlTok::Punct(ch));
            i += ch.len_utf8();
        }
    }
    tokens
}

fn is_punct(tokens: &[SqlTok], i: usize, ch: char) -> bool {
    tokens.get(i) == Some(&SqlTok::Punct(ch))
}

fn is_word(tokens: &[SqlTok], i: usize, words: &[&str]) -> bool {
    tokens.get(i).is_some_and(|t| t.is_word(words))
}

fn is_name(tokens: &[SqlTok], i: usize) -> bool {
    tokens.get(i).is_some_and(|t| t.name().is_some())
}

/// `tokens[i]` is `(`; return the index just past its matching `)`.
fn skip_parens(tokens: &[SqlTok], mut i: usize) -> usize {
    let mut depth = 0isize;
    while i < tokens.len() {
        match tokens[i] {
            SqlTok::Punct('(') => depth += 1,
            SqlTok::Punct(')') => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    i
}

/// Lower-cased CTE names: `WITH [RECURSIVE] name [(cols)] AS [NOT] [MATERIALIZED] (...)`.
fn sql_cte_names(tokens: &[SqlTok]) -> HashSet<String> {
    let mut names = HashSet::new();
    for i in 0..tokens.len() {
        if !is_word(tokens, i, &["with"]) {
            continue;
        }
        let mut j = i + 1;
        if is_word(tokens, j, &["recursive"]) {
            j += 1;
        }
        while let Some(name) = tokens.get(j).and_then(SqlTok::name) {
            let name = name.to_lowercase();
            j += 1;
            if is_punct(tokens, j, '(') {
                j = skip_parens(tokens, j);
            }
            if !is_word(tokens, j, &["as"]) {
                break;
            }
            j += 1;
            if is_word(tokens, j, &["not"]) {
                j += 1;
            }
            if is_word(tokens, j, &["materialized"]) {
                j += 1;
            }
            if !is_punct(tokens, j, '(') {
                break;
            }
            names.insert(name);
            j = skip_parens(tokens, j);
            if !is_punct(tokens, j, ',') {
                break;
            }
            j += 1;
        }
    }
    names
}

struct SqlRefs<'a> {
    tokens: &'a [SqlTok],
    ctes: HashSet<String>,
    refs: BTreeSet<String>,
}

impl SqlRefs<'_> {
    fn add_ref(&mut self, mut parts: &[&SqlTok]) {
        while parts.len() > 1
            && parts[0]
                .name()
                .is_some_and(|n| in_word_list(SQL_KNOWN_SCHEMAS, n))
        {
            parts = &parts[1..];
        }
        let Some(name) = parts[0].name() else {
            return;
        };
        if matches!(parts[0], SqlTok::Ident(_)) && in_word_list(SQL_RESERVED, name) {
            return;
        }
        if !is_python_identifier(name)
            || PY_KEYWORDS.contains(&name)
            || is_ignored_name(name)
            || self.ctes.contains(&name.to_lowercase())
        {
            return;
        }
        self.refs.insert(name.to_owned());
    }

    /// Table list after FROM (comma joins allowed) or JOIN.
    fn table_list(&mut self, mut j: usize, allow_comma: bool) {
        let tokens = self.tokens;
        while j < tokens.len() {
            if is_punct(tokens, j, '(') {
                j = skip_parens(tokens, j); // subquery: scanned by the outer loop
            } else if is_word(tokens, j, &["lateral"]) {
                j += 1;
                continue;
            } else if is_name(tokens, j) {
                let mut parts = vec![&tokens[j]];
                j += 1;
                while is_punct(tokens, j, '.') && is_name(tokens, j + 1) {
                    parts.push(&tokens[j + 1]);
                    j += 2;
                }
                if is_punct(tokens, j, '(') {
                    j = skip_parens(tokens, j); // table function such as read_csv(...)
                } else {
                    self.add_ref(&parts);
                }
            } else {
                return;
            }
            let mut aliased = false;
            if is_word(tokens, j, &["as"]) {
                j += 1;
                if is_name(tokens, j) {
                    j += 1;
                    aliased = true;
                }
            } else if is_name(tokens, j) && !is_word(tokens, j, SQL_ALIAS_STOP) {
                j += 1;
                aliased = true;
            }
            if aliased && is_punct(tokens, j, '(') {
                j = skip_parens(tokens, j); // alias column list `t(a, b)`
            }
            if allow_comma && is_punct(tokens, j, ',') {
                j += 1;
                continue;
            }
            return;
        }
    }
}

/// Analyze one SQL cell: defs = `[output_var]`, refs = tables read via FROM / JOIN.
pub(crate) fn analyze_sql_cell(source: &str, output_var: &str) -> CellAnalysis {
    #[cfg(test)]
    PARSE_COUNT.with(|c| c.set(c.get() + 1));
    let tokens = sql_tokens(source);
    let mut sql = SqlRefs {
        ctes: sql_cte_names(&tokens),
        tokens: &tokens,
        refs: BTreeSet::new(),
    };
    // Paren stack: true when the paren belongs to EXTRACT/SUBSTRING/TRIM/OVERLAY.
    let mut stack: Vec<bool> = Vec::new();
    for (i, tok) in tokens.iter().enumerate() {
        match tok {
            SqlTok::Punct('(') => {
                stack.push(i > 0 && is_word(&tokens, i - 1, SQL_FROM_FUNCTIONS));
            }
            SqlTok::Punct(')') => {
                stack.pop();
            }
            _ if tok.is_word(&["from", "join"]) => {
                if stack.last() == Some(&true) {
                    continue;
                }
                sql.table_list(i + 1, tok.is_word(&["from"]));
            }
            _ => {}
        }
    }
    let out = output_var.trim();
    let defs = (is_python_identifier(out) && !is_ignored_name(out))
        .then(|| out.to_owned())
        .into_iter()
        .collect();
    CellAnalysis {
        defs,
        refs: sql.refs.into_iter().collect(),
        builtin_refs: Vec::new(),
        error: None,
    }
}

// ---------------------------------------------------------------------------------------
// Caches
// ---------------------------------------------------------------------------------------

/// Small, fast, non-cryptographic hasher (FxHash-style) for cache fingerprints.
struct FastHasher(u64);

impl FastHasher {
    const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(Self::SEED);
    }

    fn str(&mut self, s: &str) {
        self.write(s.as_bytes());
        self.add(s.len() as u64);
    }
}

impl Hasher for FastHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (chunks, rest) = bytes.as_chunks::<8>();
        for chunk in chunks {
            self.add(u64::from_le_bytes(*chunk));
        }
        if !rest.is_empty() {
            let mut word = [0u8; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word) ^ ((rest.len() as u64) << 56));
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

struct CellCacheEntry {
    kind: String,
    output_var: String,
    source: String,
    analysis: Rc<CellAnalysis>,
}

/// Two-generation cache: when `current` fills up it becomes `previous`, so entries used
/// recently survive while the total stays bounded at `2 * CELL_CACHE_CAPACITY`.
#[derive(Default)]
struct CellCache {
    current: HashMap<u64, CellCacheEntry>,
    previous: HashMap<u64, CellCacheEntry>,
}

const CELL_CACHE_CAPACITY: usize = 1024;

impl CellCache {
    fn insert(&mut self, key: u64, entry: CellCacheEntry) {
        if self.current.len() >= CELL_CACHE_CAPACITY {
            self.previous = std::mem::take(&mut self.current);
        }
        self.current.insert(key, entry);
    }

    fn get(
        &mut self,
        key: u64,
        kind: &str,
        source: &str,
        output_var: &str,
    ) -> Option<Rc<CellAnalysis>> {
        let matches =
            |e: &CellCacheEntry| e.kind == kind && e.output_var == output_var && e.source == source;
        if let Some(entry) = self.current.get(&key).filter(|e| matches(e)) {
            return Some(entry.analysis.clone());
        }
        let entry = self.previous.remove(&key).filter(|e| matches(e))?;
        let analysis = entry.analysis.clone();
        self.insert(key, entry);
        Some(analysis)
    }
}

thread_local! {
    static CELL_CACHE: RefCell<CellCache> = RefCell::new(CellCache::default());
    static NOTEBOOK_CACHE: RefCell<Option<(u64, Rc<NotebookDag>)>> = const { RefCell::new(None) };
}

fn cell_analysis(kind: &str, source: &str, output_var: &str) -> Rc<CellAnalysis> {
    let mut h = FastHasher(0);
    h.str(kind);
    h.str(output_var);
    h.str(source);
    let key = h.finish();
    if let Some(hit) = CELL_CACHE.with(|c| c.borrow_mut().get(key, kind, source, output_var)) {
        return hit;
    }
    let analysis = Rc::new(if kind == "sql" {
        analyze_sql_cell(source, output_var)
    } else {
        analyze_python_cell(source)
    });
    CELL_CACHE.with(|c| {
        c.borrow_mut().insert(
            key,
            CellCacheEntry {
                kind: kind.to_owned(),
                output_var: output_var.to_owned(),
                source: source.to_owned(),
                analysis: analysis.clone(),
            },
        )
    });
    analysis
}

/// Fingerprint of everything `analyze_notebook` reads from the model.
fn notebook_key(model: &NotebookModel) -> u64 {
    let mut h = FastHasher(0x9e37_79b9_7f4a_7c15);
    h.add(model.order.len() as u64);
    for id in &model.order {
        h.str(id);
        let Some(cell) = model.cells.get(id) else {
            h.add(0);
            continue;
        };
        h.add(1);
        h.str(&cell.disk.kind);
        if !cell.is_executable() {
            continue;
        }
        h.str(&cell.text);
        h.add(u64::from(cell.is_disabled()));
        if cell.disk.kind == "sql" {
            h.str(&cell.sql_output_var());
        }
        match &cell.executed_revision {
            Some(rev) => {
                h.add(1);
                h.str(rev);
            }
            None => h.add(0),
        }
        h.add(cell.ordinal.map_or(0, |o| o.wrapping_add(1)));
        h.str(&cell.outcome);
        h.add(u64::from(from_other_session(model, cell)));
    }
    h.finish()
}

// ---------------------------------------------------------------------------------------
// Notebook graph
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RunStatus {
    /// Last run succeeded and the source is unchanged since.
    Fresh,
    /// Last run succeeded but the source changed since.
    Edited,
    NeverRun,
    Failed,
}

/// A run recorded in a kernel session other than the live one (e.g. before a
/// restart) left nothing in the current namespace. Unknown sessions are trusted.
fn from_other_session(model: &NotebookModel, cell: &CellModel) -> bool {
    matches!((&model.live_session, &cell.session_id), (Some(live), Some(ran)) if live != ran)
}

fn run_status(model: &NotebookModel, cell: &CellModel) -> RunStatus {
    let outcome = cell.outcome.trim().to_ascii_lowercase();
    if outcome.is_empty() || from_other_session(model, cell) {
        RunStatus::NeverRun
    } else if matches!(
        outcome.as_str(),
        "error" | "failed" | "interrupted" | "canceled" | "cancelled" | "timeout" | "crashed"
    ) {
        RunStatus::Failed
    } else if cell.stale() {
        RunStatus::Edited
    } else {
        RunStatus::Fresh
    }
}

struct CellInput {
    id: String,
    analysis: Rc<CellAnalysis>,
    disabled: bool,
    status: RunStatus,
    ordinal: Option<u64>,
}

fn tarjan_sccs(adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct State<'a> {
        adj: &'a [Vec<usize>],
        index: usize,
        indices: Vec<Option<usize>>,
        lowlink: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        out: Vec<Vec<usize>>,
    }
    fn connect(v: usize, st: &mut State<'_>) {
        st.indices[v] = Some(st.index);
        st.lowlink[v] = st.index;
        st.index += 1;
        st.stack.push(v);
        st.on_stack[v] = true;
        for &w in &st.adj[v] {
            match st.indices[w] {
                None => {
                    connect(w, st);
                    st.lowlink[v] = st.lowlink[v].min(st.lowlink[w]);
                }
                Some(iw) if st.on_stack[w] => st.lowlink[v] = st.lowlink[v].min(iw),
                Some(_) => {}
            }
        }
        if Some(st.lowlink[v]) == st.indices[v] {
            let mut scc = Vec::new();
            while let Some(w) = st.stack.pop() {
                st.on_stack[w] = false;
                scc.push(w);
                if w == v {
                    break;
                }
            }
            if scc.len() > 1 {
                scc.sort_unstable();
                st.out.push(scc);
            }
        }
    }
    let n = adj.len();
    let mut st = State {
        adj,
        index: 0,
        indices: vec![None; n],
        lowlink: vec![0; n],
        on_stack: vec![false; n],
        stack: Vec::new(),
        out: Vec::new(),
    };
    for v in 0..n {
        if st.indices[v].is_none() {
            connect(v, &mut st);
        }
    }
    st.out.sort();
    st.out
}

fn reachable(adj: &[Vec<usize>], start: usize) -> Vec<bool> {
    let mut seen = vec![false; adj.len()];
    let mut queue: VecDeque<usize> = adj[start].iter().copied().collect();
    while let Some(v) = queue.pop_front() {
        if v == start || seen[v] {
            continue;
        }
        seen[v] = true;
        queue.extend(adj[v].iter().copied().filter(|w| !seen[*w]));
    }
    seen
}

fn build_dag(cells: &[CellInput]) -> NotebookDag {
    let n = cells.len();
    let mut dag = NotebookDag::default();

    let mut definers: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, cell) in cells.iter().enumerate() {
        for name in &cell.analysis.defs {
            definers.entry(name.as_str()).or_default().push(i);
        }
    }
    // Builtins become refs only when another cell shadows them.
    let refs: Vec<Vec<String>> = cells
        .iter()
        .enumerate()
        .map(|(i, cell)| {
            let shadowed = cell.analysis.builtin_refs.iter().filter(|b| {
                definers
                    .get(b.as_str())
                    .is_some_and(|d| d.iter().any(|&j| j != i))
            });
            let mut all: Vec<String> = cell.analysis.refs.iter().chain(shadowed).cloned().collect();
            all.sort();
            all.dedup();
            all
        })
        .collect();

    for (i, cell) in cells.iter().enumerate() {
        for name in &cell.analysis.defs {
            dag.defined_by
                .entry(name.clone())
                .or_default()
                .push(cell.id.clone());
        }
        for name in &refs[i] {
            dag.used_by
                .entry(name.clone())
                .or_default()
                .push(cell.id.clone());
        }
        dag.defs.insert(cell.id.clone(), cell.analysis.defs.clone());
        dag.refs.insert(cell.id.clone(), refs[i].clone());
        if let Some(error) = &cell.analysis.error {
            dag.parse_errors.insert(cell.id.clone(), error.clone());
        }
    }
    dag.multiple_definitions = dag
        .defined_by
        .iter()
        .filter(|(_, d)| d.len() > 1)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Edge from the nearest preceding definer, else the nearest following one.
    let mut edge_names: BTreeMap<(usize, usize), BTreeSet<&str>> = BTreeMap::new();
    for (i, cell_refs) in refs.iter().enumerate() {
        for name in cell_refs {
            let Some(ds) = definers.get(name.as_str()) else {
                continue;
            };
            let from = ds
                .iter()
                .rev()
                .find(|&&j| j < i)
                .or_else(|| ds.iter().find(|&&j| j > i));
            if let Some(&from) = from {
                edge_names
                    .entry((from, i))
                    .or_default()
                    .insert(name.as_str());
            }
        }
    }
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut radj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for &(from, to) in edge_names.keys() {
        adj[from].push(to);
        radj[to].push(from);
        indegree[to] += 1;
    }
    let mut edges: Vec<(String, String, Vec<String>)> = edge_names
        .into_iter()
        .map(|((from, to), names)| {
            (
                cells[from].id.clone(),
                cells[to].id.clone(),
                names.into_iter().map(str::to_owned).collect(),
            )
        })
        .collect();
    edges.sort();
    dag.edges = edges;
    for (from, nexts) in adj.iter().enumerate() {
        if !nexts.is_empty() {
            dag.adj.insert(
                cells[from].id.clone(),
                nexts.iter().map(|&t| cells[t].id.clone()).collect(),
            );
        }
    }

    dag.cycles = tarjan_sccs(&adj)
        .into_iter()
        .map(|scc| scc.into_iter().map(|i| cells[i].id.clone()).collect())
        .collect();

    // Kahn with notebook-order tie-break; leftover cycle members in notebook order.
    let mut ready: BTreeSet<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
    let mut placed = vec![false; n];
    let mut topo: Vec<usize> = Vec::with_capacity(n);
    while let Some(cur) = ready.pop_first() {
        placed[cur] = true;
        topo.push(cur);
        for &next in &adj[cur] {
            indegree[next] -= 1;
            if indegree[next] == 0 {
                ready.insert(next);
            }
        }
    }
    topo.extend((0..n).filter(|&i| !placed[i]));
    dag.topological_order = topo.iter().map(|&i| cells[i].id.clone()).collect();

    // Staleness.
    let needs_run: Vec<bool> = cells.iter().map(|c| c.status != RunStatus::Fresh).collect();
    for cell in cells {
        match cell.status {
            RunStatus::NeverRun => {
                dag.never_run.insert(cell.id.clone());
                dag.direct_stale.insert(cell.id.clone());
            }
            RunStatus::Edited => {
                dag.direct_stale.insert(cell.id.clone());
            }
            RunStatus::Failed => {
                dag.failed.insert(cell.id.clone());
            }
            RunStatus::Fresh => {}
        }
    }
    let succ_ordinal = |c: &CellInput| {
        matches!(c.status, RunStatus::Fresh | RunStatus::Edited)
            .then_some(c.ordinal)
            .flatten()
    };
    for (i, cell) in cells.iter().enumerate() {
        if !matches!(cell.status, RunStatus::Fresh | RunStatus::Edited) || radj[i].is_empty() {
            continue;
        }
        let mine = succ_ordinal(cell);
        let ancestors = reachable(&radj, i);
        let reasons: Vec<String> = (0..n)
            .filter(|&a| ancestors[a])
            .filter(|&a| {
                needs_run[a]
                    || matches!((succ_ordinal(&cells[a]), mine), (Some(up), Some(me)) if up > me)
            })
            .map(|a| cells[a].id.clone())
            .collect();
        if !reasons.is_empty() {
            dag.upstream_stale_from.insert(cell.id.clone(), reasons);
        }
    }
    for cell in cells.iter().filter(|c| c.disabled) {
        dag.blocked.insert(cell.id.clone(), cell.id.clone());
    }
    for (d, cell) in cells.iter().enumerate().filter(|(_, c)| c.disabled) {
        let below = reachable(&adj, d);
        for (i, _) in below.iter().enumerate().filter(|(_, r)| **r) {
            dag.blocked
                .entry(cells[i].id.clone())
                .or_insert_with(|| cell.id.clone());
        }
    }
    dag.run_stale_ids = topo
        .iter()
        .filter(|&&i| {
            let id = &cells[i].id;
            (needs_run[i] || dag.upstream_stale_from.contains_key(id)) && !dag.is_blocked(id)
        })
        .map(|&i| cells[i].id.clone())
        .collect();
    dag
}

/// Analyze a live `NotebookModel` into a shared, memoized dependency & staleness graph.
///
/// Repeated calls with an unchanged model return the cached `Rc` without re-analysis.
pub(crate) fn analyze_notebook_shared(model: &NotebookModel) -> Rc<NotebookDag> {
    let key = notebook_key(model);
    let hit = NOTEBOOK_CACHE.with(|c| {
        c.borrow()
            .as_ref()
            .filter(|(k, _)| *k == key)
            .map(|(_, dag)| dag.clone())
    });
    if let Some(hit) = hit {
        return hit;
    }
    let inputs: Vec<CellInput> = model
        .order
        .iter()
        .filter_map(|id| model.cells.get(id).map(|cell| (id, cell)))
        .filter(|(_, cell)| cell.is_executable())
        .map(|(id, cell)| {
            let output_var = if cell.disk.kind == "sql" {
                cell.sql_output_var()
            } else {
                String::new()
            };
            CellInput {
                id: id.clone(),
                analysis: cell_analysis(&cell.disk.kind, &cell.text, &output_var),
                disabled: cell.is_disabled(),
                status: run_status(model, cell),
                // Ordinals are per session; a previous session's are not comparable.
                ordinal: cell.ordinal.filter(|_| !from_other_session(model, cell)),
            }
        })
        .collect();
    let dag = Rc::new(build_dag(&inputs));
    NOTEBOOK_CACHE.with(|c| *c.borrow_mut() = Some((key, dag.clone())));
    dag
}

/// Analyze a live `NotebookModel` into a complete dependency & cascade-staleness graph.
///
/// Memoized (see [`analyze_notebook_shared`]); this returns an owned copy of the cached
/// graph because some callers move fields out of it.
pub(crate) fn analyze_notebook(model: &NotebookModel) -> NotebookDag {
    (*analyze_notebook_shared(model)).clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernelledger_models::SourceSnapshot;
    use serde_json::{Value, json};
    use std::path::PathBuf;

    /// Shared with `tests/test_dag_spec.py`; both analyzers must match it.
    const FIXTURE: &str = include_str!("../../../../tests/fixtures/dag_cases.json");

    fn snapshot_model(cells: Vec<(&str, &str, &str, Value)>) -> NotebookModel {
        let mut model = NotebookModel {
            project: PathBuf::from("/tmp/nb"),
            ..Default::default()
        };
        let cell_json: Vec<Value> = cells
            .into_iter()
            .map(|(id, kind, text, metadata)| {
                json!({
                    "id": id,
                    "type": kind,
                    "source": text,
                    "revision": format!("r-{id}"),
                    "metadata": metadata,
                })
            })
            .collect();
        let snap: SourceSnapshot = serde_json::from_value(json!({
            "state": "ok",
            "manifest_revision": "rev1",
            "cells": cell_json,
        }))
        .unwrap();
        model.apply_snapshot(snap);
        model
    }

    fn code_model(cells: &[(&str, &str)]) -> NotebookModel {
        snapshot_model(
            cells
                .iter()
                .map(|(id, src)| (*id, "code", *src, json!({})))
                .collect(),
        )
    }

    /// Record a finished run of `id` with the current source.
    fn ran(model: &mut NotebookModel, id: &str, outcome: &str, ordinal: u64) {
        let cell = model.cells.get_mut(id).unwrap();
        cell.outcome = outcome.into();
        cell.ordinal = Some(ordinal);
        cell.executed_revision = Some(kernelledger_client::hash(cell.text.as_bytes()));
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn set(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn sorted_strings(value: &Value) -> Vec<String> {
        let mut out: Vec<String> = value
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out.dedup();
        out
    }

    fn sorted_dedup(mut v: Vec<String>) -> Vec<String> {
        v.sort();
        v.dedup();
        v
    }

    #[test]
    fn shared_fixture_cases_match() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let mut failures = Vec::new();
        let mut total = 0;
        for case in fixture["python_cells"].as_array().unwrap() {
            total += 1;
            let got = analyze_python_cell(case["source"].as_str().unwrap());
            let want_error = case["error"].as_bool().unwrap_or(false);
            let (defs, refs) = (sorted_dedup(got.defs), sorted_dedup(got.refs));
            if defs != sorted_strings(&case["defs"])
                || refs != sorted_strings(&case["refs"])
                || got.error.is_some() != want_error
            {
                failures.push(format!(
                    "python_cells/{}: got defs={defs:?} refs={refs:?} error={:?}; want defs={} refs={} error={want_error}",
                    case["name"], got.error, case["defs"], case["refs"]
                ));
            }
        }
        for case in fixture["sql_cells"].as_array().unwrap() {
            total += 1;
            let got = analyze_sql_cell(
                case["source"].as_str().unwrap(),
                case["output_var"].as_str().unwrap_or("_sql_df"),
            );
            let (defs, refs) = (sorted_dedup(got.defs), sorted_dedup(got.refs));
            if defs != sorted_strings(&case["defs"]) || refs != sorted_strings(&case["refs"]) {
                failures.push(format!(
                    "sql_cells/{}: got defs={defs:?} refs={refs:?}; want defs={} refs={}",
                    case["name"], case["defs"], case["refs"]
                ));
            }
        }
        for case in fixture["notebooks"].as_array().unwrap() {
            total += 1;
            let cells: Vec<(&str, &str, &str, Value)> = case["cells"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| {
                    let mut meta = json!({"disabled": c["disabled"].as_bool().unwrap_or(false)});
                    if let Some(var) = c["output_var"].as_str() {
                        meta["output_var"] = json!(var);
                    }
                    (
                        c["id"].as_str().unwrap(),
                        c["type"].as_str().unwrap_or("code"),
                        c["source"].as_str().unwrap_or(""),
                        meta,
                    )
                })
                .collect();
            let dag = analyze_notebook(&snapshot_model(cells));
            let mut edges: Vec<(String, String, Vec<String>)> = dag
                .edges
                .iter()
                .map(|(f, t, n)| (f.clone(), t.clone(), sorted_dedup(n.clone())))
                .collect();
            edges.sort();
            let mut want_edges: Vec<(String, String, Vec<String>)> = case["edges"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| {
                    (
                        e[0].as_str().unwrap().to_owned(),
                        e[1].as_str().unwrap().to_owned(),
                        sorted_strings(&e[2]),
                    )
                })
                .collect();
            want_edges.sort();
            let mut cycles: Vec<Vec<String>> =
                dag.cycles.iter().map(|c| sorted_dedup(c.clone())).collect();
            cycles.sort();
            let mut want_cycles: Vec<Vec<String>> = case["cycles"]
                .as_array()
                .unwrap()
                .iter()
                .map(sorted_strings)
                .collect();
            want_cycles.sort();
            let topo_ok = case
                .get("topological_order")
                .is_none_or(|want| *want == json!(dag.topological_order));
            if edges != want_edges || cycles != want_cycles || !topo_ok {
                failures.push(format!(
                    "notebooks/{}: got edges={edges:?} cycles={cycles:?} topo={:?}",
                    case["name"], dag.topological_order
                ));
            }
        }
        for failure in &failures {
            eprintln!("MISMATCH {failure}");
        }
        eprintln!("checked {total} shared fixture cases");
        assert!(total >= 100, "fixture looks truncated: {total} cases");
        assert!(
            failures.is_empty(),
            "{} of {total} fixture cases mismatch:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn dag_extracts_defs_refs_sql_and_cascade_stale() {
        let mut model = snapshot_model(vec![
            ("c1", "code", "x = 10\ny = 20\n", json!({})),
            ("c2", "code", "z = x + y\n", json!({})),
            (
                "c3",
                "sql",
                "SELECT * FROM sales WHERE v > 0",
                json!({"output_var": "q_df"}),
            ),
            ("c4", "code", "out = z + len(q_df)\n", json!({})),
            ("c5", "code", "skip_me = 1\n", json!({"disabled": true})),
        ]);
        // c2, c3, c4 previously ran & fresh; c1 was edited after its run.
        for cid in ["c2", "c3", "c4"] {
            let c = model.cells.get_mut(cid).unwrap();
            c.outcome = "ok".into();
            c.executed_revision = Some(kernelledger_client::hash(c.text.as_bytes()));
        }
        let c1 = model.cells.get_mut("c1").unwrap();
        c1.outcome = "ok".into();
        c1.executed_revision = Some("old-hash".into());

        let dag = analyze_notebook(&model);
        assert_eq!(dag.defined_by.get("x").unwrap(), &vec!["c1".to_string()]);
        assert_eq!(dag.defined_by.get("q_df").unwrap(), &vec!["c3".to_string()]);
        assert_eq!(dag.downstream_ids("c1"), vec!["c2", "c4"]);
        assert!(dag.is_upstream_stale("c2"));
        assert!(dag.is_upstream_stale("c4"));
        assert_eq!(dag.run_stale_ids, vec!["c1", "c2", "c4"]);
        assert!(!dag.run_stale_ids.contains(&"c5".to_string()));
    }

    #[test]
    fn dag_detects_cycles_and_multiple_defs() {
        let model = code_model(&[
            ("a", "v1 = v2 + 1\n"),
            ("b", "v2 = v1 + 1\n"),
            ("c", "v1 = 99\n"),
        ]);
        let dag = analyze_notebook(&model);
        assert_eq!(dag.multiple_definitions["v1"], vec!["a", "c"]);
        assert_eq!(dag.cycles, vec![ids(&["a", "b"])]);
        assert_eq!(dag.has_multiple_def("c"), ids(&["v1"]));
    }

    #[test]
    fn runs_from_a_previous_kernel_session_need_a_rerun() {
        let mut model = code_model(&[("c1", "x = 1"), ("c2", "y = x + 1")]);
        ran(&mut model, "c1", "ok", 1);
        ran(&mut model, "c2", "ok", 2);
        for id in ["c1", "c2"] {
            model.cells.get_mut(id).unwrap().session_id = Some("s1".into());
        }
        model.live_session = Some("s1".into());
        assert!(analyze_notebook(&model).run_stale_ids.is_empty());
        // After a restart the namespace is empty even though outputs are still shown.
        model.live_session = Some("s2".into());
        let dag = analyze_notebook(&model);
        assert_eq!(dag.never_run, set(&["c1", "c2"]));
        assert_eq!(dag.run_stale_ids, ids(&["c1", "c2"]));
        // Re-running c1 in the new session leaves only c2 needing a run.
        ran(&mut model, "c1", "ok", 1);
        model.cells.get_mut("c1").unwrap().session_id = Some("s2".into());
        assert_eq!(analyze_notebook(&model).run_stale_ids, ids(&["c2"]));
    }

    #[test]
    fn never_run_cells_need_a_run_but_are_not_upstream_stale() {
        let model = code_model(&[("c1", "x = 1\n"), ("c2", "y = x\n")]);
        let dag = analyze_notebook(&model);
        assert_eq!(dag.never_run, set(&["c1", "c2"]));
        // Backward compatible: never-run cells are also direct_stale.
        assert_eq!(dag.direct_stale, set(&["c1", "c2"]));
        assert!(dag.failed.is_empty());
        assert!(dag.upstream_stale_from.is_empty());
        assert_eq!(dag.run_stale_ids, ids(&["c1", "c2"]));
    }

    #[test]
    fn edited_cell_is_direct_stale_and_cascades() {
        let mut model = code_model(&[("c1", "x = 1\n"), ("c2", "y = x\n"), ("c3", "z = 3\n")]);
        ran(&mut model, "c1", "ok", 1);
        ran(&mut model, "c2", "ok", 2);
        ran(&mut model, "c3", "ok", 3);
        assert!(analyze_notebook(&model).run_stale_ids.is_empty());
        model.cells.get_mut("c1").unwrap().edit("x = 2\n".into());
        let dag = analyze_notebook(&model);
        assert_eq!(dag.direct_stale, set(&["c1"]));
        assert!(dag.never_run.is_empty());
        assert_eq!(dag.upstream_stale_from["c2"], ids(&["c1"]));
        assert_eq!(dag.run_stale_ids, ids(&["c1", "c2"]));
    }

    #[test]
    fn failed_cell_needs_a_run_and_is_reported_separately() {
        let mut model = code_model(&[("c1", "x = 1\n"), ("c2", "y = x\n"), ("c3", "w = y\n")]);
        ran(&mut model, "c1", "ok", 1);
        ran(&mut model, "c2", "error", 2);
        ran(&mut model, "c3", "ok", 3);
        let dag = analyze_notebook(&model);
        assert_eq!(dag.failed, set(&["c2"]));
        assert!(dag.direct_stale.is_empty());
        assert_eq!(dag.upstream_stale_from["c3"], ids(&["c2"]));
        assert_eq!(dag.run_stale_ids, ids(&["c2", "c3"]));
        ran(&mut model, "c2", "interrupted", 4);
        assert_eq!(analyze_notebook(&model).failed, set(&["c2"]));
    }

    #[test]
    fn upstream_rerun_after_downstream_marks_downstream_stale() {
        let mut model = code_model(&[("c1", "x = 1\n"), ("c2", "y = x\n"), ("c3", "z = y\n")]);
        ran(&mut model, "c1", "ok", 1);
        ran(&mut model, "c2", "ok", 2);
        ran(&mut model, "c3", "ok", 3);
        assert!(analyze_notebook(&model).run_stale_ids.is_empty());
        // Re-running c1 after c2/c3 leaves them holding values computed from the old x.
        ran(&mut model, "c1", "ok", 4);
        let dag = analyze_notebook(&model);
        assert!(dag.direct_stale.is_empty());
        assert_eq!(dag.upstream_stale_from["c2"], ids(&["c1"]));
        assert_eq!(dag.upstream_stale_from["c3"], ids(&["c1"]));
        assert_eq!(dag.run_stale_ids, ids(&["c2", "c3"]));
        // Re-running c2 refreshes c2; c3 now predates both of its ancestors.
        ran(&mut model, "c2", "ok", 5);
        let dag = analyze_notebook(&model);
        assert!(!dag.is_upstream_stale("c2"));
        assert_eq!(dag.upstream_stale_from["c3"], ids(&["c1", "c2"]));
        assert_eq!(dag.run_stale_ids, ids(&["c3"]));
    }

    #[test]
    fn disabled_cells_block_their_descendants_transitively() {
        let model = snapshot_model(vec![
            ("c1", "code", "a = 1\n", json!({"disabled": true})),
            ("c2", "code", "b = a\n", json!({})),
            ("c3", "code", "c = b\n", json!({})),
            ("c4", "code", "d = 1\n", json!({})),
            ("c5", "code", "e = d\n", json!({"disabled": true})),
        ]);
        let dag = analyze_notebook(&model);
        let blocked: Vec<(&str, &str)> = dag
            .blocked
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            blocked,
            vec![("c1", "c1"), ("c2", "c1"), ("c3", "c1"), ("c5", "c5")]
        );
        assert!(dag.is_blocked("c3") && !dag.is_blocked("c4"));
        assert_eq!(dag.run_stale_ids, ids(&["c4"]));
        // downstream_ids stays a pure graph traversal.
        assert_eq!(dag.downstream_ids("c1"), ids(&["c2", "c3"]));
        assert_eq!(dag.downstream_ids("c4"), ids(&["c5"]));
    }

    #[test]
    fn run_stale_ids_follow_topological_order() {
        let model = code_model(&[("c1", "z = y + 1\n"), ("c2", "w = 5\n"), ("c3", "y = w\n")]);
        let dag = analyze_notebook(&model);
        assert_eq!(dag.topological_order, ids(&["c2", "c3", "c1"]));
        assert_eq!(dag.run_stale_ids, ids(&["c2", "c3", "c1"]));
    }

    #[test]
    fn parse_error_cell_has_no_defs_refs_or_edges() {
        let model = code_model(&[
            ("c1", "x = = 1\n"),
            ("c2", "print(x)\n"),
            ("c3", "%%bash\nls\n"),
        ]);
        let dag = analyze_notebook(&model);
        assert!(dag.parse_errors["c1"].contains("line 1"));
        assert!(dag.defs["c1"].is_empty() && dag.refs["c1"].is_empty());
        assert!(dag.edges.is_empty());
        assert_eq!(dag.refs["c2"], ids(&["x"]));
        assert!(!dag.parse_errors.contains_key("c3"));
        assert!(analyze_python_cell("x = 1\n  y = 2\n").error.is_some());
        assert!(analyze_python_cell("print x\n").error.is_some());
    }

    #[test]
    fn repeated_analysis_is_served_from_cache() {
        let parses = || PARSE_COUNT.with(std::cell::Cell::get);
        let mut model = snapshot_model(vec![
            ("c1", "code", "a = 1\n", json!({})),
            ("c2", "code", "b = a\n", json!({})),
            ("c3", "sql", "SELECT * FROM b", json!({"output_var": "q"})),
        ]);
        let before = parses();
        let first = analyze_notebook_shared(&model);
        assert_eq!(parses() - before, 3);
        let second = analyze_notebook_shared(&model);
        assert!(
            Rc::ptr_eq(&first, &second),
            "unchanged model must hit the cache"
        );
        assert_eq!(analyze_notebook(&model).edges, first.edges);
        assert_eq!(parses() - before, 3);

        // Typing in one cell re-parses only that cell.
        model
            .cells
            .get_mut("c2")
            .unwrap()
            .edit("b = a + 1\n".into());
        let third = analyze_notebook_shared(&model);
        assert!(!Rc::ptr_eq(&second, &third));
        assert_eq!(parses() - before, 4);

        // Execution state changes rebuild the graph without re-parsing anything.
        ran(&mut model, "c1", "ok", 1);
        let fourth = analyze_notebook_shared(&model);
        assert!(!fourth.never_run.contains("c1"));
        assert_eq!(parses() - before, 4);

        // Reverting the edit is served from the per-cell cache.
        model.cells.get_mut("c2").unwrap().edit("b = a\n".into());
        analyze_notebook_shared(&model);
        assert_eq!(parses() - before, 4);
    }

    #[test]
    fn known_line_tokenizer_failures_are_fixed() {
        let cases: &[(&str, &[&str], &[&str])] = &[
            ("x = x + 1\n", &["x"], &["x"]),
            ("df = df.dropna()\n", &["df"], &["df"]),
            ("def f(a):\n    return a + b\n", &["f"], &["b"]),
            ("sq = [i * i for i in nums]\n", &["sq"], &["nums"]),
            ("if flag:\n    y = 1\nelse:\n    y = 2\n", &["y"], &["flag"]),
            ("s = f\"{total} {d['k']}\"\n", &["s"], &["d", "total"]),
            ("a = b = 0\n", &["a", "b"], &[]),
            ("from m import (a,\n    b)\n", &["a", "b"], &[]),
            ("print(x); y = 2\n", &["y"], &["x"]),
            ("données = 1\n", &["données"], &[]),
            ("%timeit z = g(w)\n", &["z"], &["g", "w"]),
            ("if (n := len(a)) > 10:\n    pass\n", &["n"], &["a"]),
            ("x: int\n", &["x"], &[]),
            (
                "def f():\n    global counter\n    counter = 1\n",
                &["counter", "f"],
                &[],
            ),
        ];
        for (src, defs, refs) in cases {
            let got = analyze_python_cell(src);
            assert_eq!(got.error, None, "{src:?}");
            assert_eq!(got.defs, ids(defs), "defs of {src:?}");
            assert_eq!(got.refs, ids(refs), "refs of {src:?}");
        }
        let sql = analyze_sql_cell(
            "WITH recent AS (SELECT * FROM sales) SELECT * FROM recent \
             WHERE note = 'from users' /* join audit */",
            "out",
        );
        assert_eq!((sql.defs, sql.refs), (ids(&["out"]), ids(&["sales"])));
    }
}
