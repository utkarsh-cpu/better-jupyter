//! Instant pure-Rust static dependency & staleness analyzer for notebook code and SQL cells.
//! Mirrors `kernelledger.dag` so the GPUI workbench can update DAG badges, provenance links,
//! and "Run stale" / "Run downstream" plans on every keystroke without a worker round-trip.

use kernelledger_models::NotebookModel;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, Default)]
pub(crate) struct NotebookDag {
    pub defs: BTreeMap<String, Vec<String>>,
    pub refs: BTreeMap<String, Vec<String>>,
    pub edges: Vec<(String, String, Vec<String>)>,
    pub defined_by: BTreeMap<String, Vec<String>>,
    pub used_by: BTreeMap<String, Vec<String>>,
    pub multiple_definitions: BTreeMap<String, Vec<String>>,
    pub cycles: Vec<Vec<String>>,
    pub topological_order: Vec<String>,
    pub direct_stale: BTreeSet<String>,
    pub upstream_stale_from: BTreeMap<String, Vec<String>>,
    pub run_stale_ids: Vec<String>,
    adj: BTreeMap<String, Vec<String>>,
    disabled: BTreeSet<String>,
}

impl NotebookDag {
    /// Return topologically ordered transitive downstream non-disabled cells of `cell_id`.
    pub(crate) fn downstream_ids(&self, cell_id: &str) -> Vec<String> {
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::new();
        if let Some(nexts) = self.adj.get(cell_id) {
            for next in nexts {
                queue.push_back(next.clone());
            }
        }
        while let Some(cur) = queue.pop_front() {
            if cur == cell_id || !visited.insert(cur.clone()) {
                continue;
            }
            if let Some(nexts) = self.adj.get(&cur) {
                for next in nexts {
                    if !visited.contains(next) {
                        queue.push_back(next.clone());
                    }
                }
            }
        }
        self.topological_order
            .iter()
            .filter(|id| visited.contains(*id) && !self.disabled.contains(*id))
            .cloned()
            .collect()
    }

    pub(crate) fn is_upstream_stale(&self, cell_id: &str) -> bool {
        self.upstream_stale_from
            .get(cell_id)
            .is_some_and(|v| !v.is_empty())
    }

    pub(crate) fn has_multiple_def(&self, cell_id: &str) -> Vec<String> {
        let mut dup = Vec::new();
        if let Some(cell_defs) = self.defs.get(cell_id) {
            for name in cell_defs {
                if self.multiple_definitions.contains_key(name) {
                    dup.push(name.clone());
                }
            }
        }
        dup
    }
}

const PY_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield", "match", "case",
];

const PY_BUILTINS: &[&str] = &[
    "abs",
    "all",
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
    "delattr",
    "dict",
    "dir",
    "divmod",
    "enumerate",
    "eval",
    "exec",
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
    "Exception",
    "ValueError",
    "TypeError",
    "KeyError",
    "IndexError",
    "RuntimeError",
    "OSError",
    "FileNotFoundError",
    "StopIteration",
    "AttributeError",
    "NameError",
    "display",
    "update_display",
    "clear_output",
    "widget",
    "display_table",
    "display_media",
];

const SQL_KEYWORDS: &[&str] = &[
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
    "on",
    "as",
    "group",
    "by",
    "order",
    "having",
    "limit",
    "offset",
    "union",
    "all",
    "distinct",
    "with",
    "and",
    "or",
    "not",
    "in",
    "is",
    "null",
    "true",
    "false",
    "case",
    "when",
    "then",
    "else",
    "end",
    "asc",
    "desc",
    "into",
    "values",
    "set",
    "create",
    "table",
    "view",
    "temp",
    "temporary",
    "insert",
    "update",
    "delete",
    "drop",
    "alter",
    "between",
    "like",
    "ilike",
    "exists",
    "using",
    "natural",
    "lateral",
    "unnest",
];

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_valid_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    is_ident_start(first) && chars.all(is_ident_continue)
}

fn is_ignored_py_name(name: &str) -> bool {
    name.is_empty()
        || (name.starts_with("__") && name.ends_with("__"))
        || PY_KEYWORDS.contains(&name)
        || PY_BUILTINS.contains(&name)
}

/// Strip Python comments and string literals (preserving newlines) so identifier extraction
/// never mistakes prose inside strings or comments for variable references.
fn strip_python_strings_and_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let n = bytes.len();
    let mut out = String::with_capacity(n);
    let mut i = 0;
    while i < n {
        let b = bytes[i];
        if b == b'#' {
            while i < n && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'\'' || b == b'"' {
            // Strip standalone string prefix (f, r, b, u, fr, rf, br, rb, etc.)
            let trimmed_len = out.len();
            let prefix_start = out
                .char_indices()
                .rev()
                .take_while(|&(_, c)| is_ident_continue(c))
                .last()
                .map(|(idx, _)| idx)
                .unwrap_or(trimmed_len);
            let candidate = &out[prefix_start..trimmed_len];
            if !candidate.is_empty()
                && candidate.len() <= 2
                && candidate
                    .bytes()
                    .all(|c| matches!(c.to_ascii_lowercase(), b'f' | b'r' | b'b' | b'u'))
            {
                out.truncate(prefix_start);
            }
            let quote = b;
            let triple = i + 2 < n && bytes[i + 1] == quote && bytes[i + 2] == quote;
            i += if triple { 3 } else { 1 };
            out.push(' ');
            while i < n {
                if bytes[i] == b'\\' && i + 1 < n {
                    i += 2;
                    continue;
                }
                if triple {
                    if i + 2 < n
                        && bytes[i] == quote
                        && bytes[i + 1] == quote
                        && bytes[i + 2] == quote
                    {
                        i += 3;
                        break;
                    }
                    if bytes[i] == b'\n' {
                        out.push('\n');
                    }
                    i += 1;
                } else {
                    if bytes[i] == quote || bytes[i] == b'\n' {
                        if bytes[i] == b'\n' {
                            out.push('\n');
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            out.push(' ');
            continue;
        }
        // Copy UTF-8 character safely
        if let Some(ch) = src[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    out
}

fn extract_identifiers(segment: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = segment.char_indices().peekable();
    let mut prev_dot = false;
    while let Some((idx, ch)) = chars.next() {
        if ch == '.' {
            prev_dot = true;
            continue;
        }
        if is_ident_start(ch) {
            let mut end = idx + ch.len_utf8();
            while let Some(&(next_idx, next_ch)) = chars.peek() {
                if is_ident_continue(next_ch) {
                    end = next_idx + next_ch.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let rest = segment[end..].trim_start();
            let is_kwarg = rest.starts_with('=') && !rest.starts_with("==");
            if !prev_dot && !is_kwarg {
                out.push(segment[idx..end].to_string());
            }
            prev_dot = false;
        } else if !ch.is_whitespace() {
            prev_dot = false;
        }
    }
    out
}

fn analyze_python(source: &str) -> (Vec<String>, Vec<String>) {
    // Handle `%%sql [output_var]` cell magic inside code cells
    let trimmed_start = source.trim_start();
    if let Some(rest) = trimmed_start.strip_prefix("%%sql") {
        let mut lines = rest.lines();
        let first = lines.next().unwrap_or("").trim();
        let output_var = if !first.is_empty() && is_valid_ident(first) {
            first
        } else {
            "_sql_df"
        };
        let body: String = lines.collect::<Vec<_>>().join("\n");
        return analyze_sql(&body, output_var);
    }

    let cleaned = strip_python_strings_and_comments(source);
    let mut defs = BTreeSet::new();
    let mut refs = BTreeSet::new();

    for raw_line in cleaned.lines() {
        let indent = raw_line.len() - raw_line.trim_start().len();
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('%') || line.starts_with('!') {
            continue;
        }
        // Top-level definitions (indent == 0)
        if indent == 0 {
            if let Some(after) = line
                .strip_prefix("def ")
                .or_else(|| line.strip_prefix("async def "))
            {
                if let Some(name) = after.split('(').next().map(str::trim)
                    && is_valid_ident(name)
                    && !is_ignored_py_name(name)
                {
                    defs.insert(name.to_string());
                }
                if let Some((_, rhs)) = after.split_once('(') {
                    for id in extract_identifiers(rhs) {
                        if !is_ignored_py_name(&id) {
                            refs.insert(id);
                        }
                    }
                }
                continue;
            }
            if let Some(after) = line.strip_prefix("class ") {
                if let Some(name) = after.split(['(', ':']).next().map(str::trim)
                    && is_valid_ident(name)
                    && !is_ignored_py_name(name)
                {
                    defs.insert(name.to_string());
                }
                if let Some((_, rhs)) = after.split_once('(') {
                    for id in extract_identifiers(rhs) {
                        if !is_ignored_py_name(&id) {
                            refs.insert(id);
                        }
                    }
                }
                continue;
            }
            if let Some(after) = line.strip_prefix("import ") {
                for part in after.split(',') {
                    let part = part.trim();
                    let bound = if let Some((_, alias)) = part.split_once(" as ") {
                        alias.trim()
                    } else {
                        part.split('.').next().unwrap_or("").trim()
                    };
                    if is_valid_ident(bound) && !is_ignored_py_name(bound) {
                        defs.insert(bound.to_string());
                    }
                }
                continue;
            }
            if let Some(after) = line.strip_prefix("from ")
                && let Some((_, imports)) = after.split_once(" import ")
            {
                let imports = imports.trim().trim_matches(['(', ')']);
                for part in imports.split(',') {
                    let part = part.trim();
                    let bound = if let Some((_, alias)) = part.split_once(" as ") {
                        alias.trim()
                    } else {
                        part
                    };
                    if is_valid_ident(bound) && !is_ignored_py_name(bound) {
                        defs.insert(bound.to_string());
                    }
                }
                continue;
            }
            if let Some(after) = line.strip_prefix("for ")
                && let Some((lhs, rhs)) = after.split_once(" in ")
            {
                for id in extract_identifiers(lhs) {
                    if !is_ignored_py_name(&id) {
                        defs.insert(id);
                    }
                }
                for id in extract_identifiers(rhs) {
                    if !is_ignored_py_name(&id) {
                        refs.insert(id);
                    }
                }
                continue;
            }
            // Assignment `a = expr`, `a, b = expr`, `a: int = expr`, `a += expr`
            if let Some(eq_idx) = find_top_level_assign(line) {
                let lhs = line[..eq_idx].trim();
                let rhs = &line[eq_idx + 1..];
                let aug = lhs.ends_with(['+', '-', '*', '/', '%', '&', '|', '^', '@']);
                let lhs_clean = lhs
                    .trim_end_matches(['+', '-', '*', '/', '%', '&', '|', '^', '@'])
                    .trim();
                let target_part = lhs_clean.split(':').next().unwrap_or(lhs_clean).trim();
                let is_simple_targets = !target_part.contains('.') && !target_part.contains('[');
                if is_simple_targets {
                    for id in extract_identifiers(target_part) {
                        if !is_ignored_py_name(&id) {
                            defs.insert(id.clone());
                            if aug {
                                refs.insert(id);
                            }
                        }
                    }
                } else {
                    for id in extract_identifiers(target_part) {
                        if !is_ignored_py_name(&id) {
                            refs.insert(id);
                        }
                    }
                }
                for id in extract_identifiers(rhs) {
                    if !is_ignored_py_name(&id) {
                        refs.insert(id);
                    }
                }
                continue;
            }
        }

        // Default: collect referenced identifiers
        for id in extract_identifiers(line) {
            if !is_ignored_py_name(&id) {
                refs.insert(id);
            }
        }
    }

    for d in &defs {
        refs.remove(d);
    }
    (defs.into_iter().collect(), refs.into_iter().collect())
}

fn find_top_level_assign(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut depth: i32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'=' if depth == 0 => {
                let prev = i.checked_sub(1).and_then(|p| bytes.get(p)).copied();
                let next = bytes.get(i + 1).copied();
                if matches!(
                    prev,
                    Some(b'=') | Some(b'!') | Some(b'<') | Some(b'>') | Some(b':')
                ) || next == Some(b'=')
                {
                    continue;
                }
                return Some(i);
            }
            _ => {}
        }
    }
    None
}

fn analyze_sql(source: &str, output_var: &str) -> (Vec<String>, Vec<String>) {
    let mut defs = Vec::new();
    let out = output_var.trim();
    if is_valid_ident(out) && !(out.starts_with("__") && out.ends_with("__")) {
        defs.push(out.to_string());
    }
    // Strip `--` line comments
    let cleaned: String = source
        .lines()
        .map(|l| l.split("--").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ");
    let tokens: Vec<&str> = cleaned
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '"'))
        .filter(|t| !t.is_empty())
        .collect();
    let mut refs = BTreeSet::new();
    for pair in tokens.windows(2) {
        let kw = pair[0].to_ascii_lowercase();
        if kw == "from" || kw == "join" {
            let ident = pair[1].trim_matches('"');
            if is_valid_ident(ident)
                && !SQL_KEYWORDS.contains(&ident.to_ascii_lowercase().as_str())
                && !defs.iter().any(|d| d == ident)
            {
                refs.insert(ident.to_string());
            }
        }
    }
    (defs, refs.into_iter().collect())
}

fn tarjan_cycles(nodes: &[String], adj: &BTreeMap<String, Vec<String>>) -> Vec<Vec<String>> {
    struct State<'a> {
        index: usize,
        stack: Vec<String>,
        on_stack: BTreeSet<String>,
        indices: BTreeMap<String, usize>,
        lowlink: BTreeMap<String, usize>,
        cycles: Vec<Vec<String>>,
        adj: &'a BTreeMap<String, Vec<String>>,
    }
    fn strongconnect(v: &str, st: &mut State<'_>) {
        st.indices.insert(v.to_string(), st.index);
        st.lowlink.insert(v.to_string(), st.index);
        st.index += 1;
        st.stack.push(v.to_string());
        st.on_stack.insert(v.to_string());
        if let Some(neighbors) = st.adj.get(v) {
            for w in neighbors {
                if !st.indices.contains_key(w) {
                    strongconnect(w, st);
                    let lw = st.lowlink[w];
                    let lv = st.lowlink[v];
                    st.lowlink.insert(v.to_string(), lv.min(lw));
                } else if st.on_stack.contains(w) {
                    let iw = st.indices[w];
                    let lv = st.lowlink[v];
                    st.lowlink.insert(v.to_string(), lv.min(iw));
                }
            }
        }
        if st.lowlink[v] == st.indices[v] {
            let mut scc = Vec::new();
            while let Some(w) = st.stack.pop() {
                st.on_stack.remove(&w);
                let done = w == v;
                scc.push(w);
                if done {
                    break;
                }
            }
            if scc.len() > 1 {
                scc.reverse();
                st.cycles.push(scc);
            }
        }
    }
    let mut st = State {
        index: 0,
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        indices: BTreeMap::new(),
        lowlink: BTreeMap::new(),
        cycles: Vec::new(),
        adj,
    };
    for node in nodes {
        if !st.indices.contains_key(node) {
            strongconnect(node, &mut st);
        }
    }
    st.cycles
}

/// Analyze a live `NotebookModel` into a complete dependency & cascade-staleness graph.
pub(crate) fn analyze_notebook(model: &NotebookModel) -> NotebookDag {
    let mut dag = NotebookDag::default();
    let mut order_index: BTreeMap<String, usize> = BTreeMap::new();
    let mut nodes: Vec<String> = Vec::new();

    for (idx, cid) in model.order.iter().enumerate() {
        let Some(cell) = model.cells.get(cid) else {
            continue;
        };
        if !cell.is_executable() {
            continue;
        }
        order_index.insert(cid.clone(), idx);
        nodes.push(cid.clone());
        if cell.is_disabled() {
            dag.disabled.insert(cid.clone());
        } else if cell.stale() || cell.outcome.is_empty() {
            dag.direct_stale.insert(cid.clone());
        }
        let (cell_defs, cell_refs) = if cell.disk.kind == "sql" {
            analyze_sql(&cell.text, &cell.sql_output_var())
        } else {
            analyze_python(&cell.text)
        };
        for name in &cell_defs {
            let list = dag.defined_by.entry(name.clone()).or_default();
            if !list.contains(cid) {
                list.push(cid.clone());
            }
        }
        for name in &cell_refs {
            let list = dag.used_by.entry(name.clone()).or_default();
            if !list.contains(cid) {
                list.push(cid.clone());
            }
        }
        dag.defs.insert(cid.clone(), cell_defs);
        dag.refs.insert(cid.clone(), cell_refs);
    }

    for (name, definers) in &dag.defined_by {
        if definers.len() > 1 {
            dag.multiple_definitions
                .insert(name.clone(), definers.clone());
        }
    }

    let mut edge_vars: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for cid in &nodes {
        let Some(cell_refs) = dag.refs.get(cid) else {
            continue;
        };
        for name in cell_refs {
            if let Some(definers) = dag.defined_by.get(name) {
                for definer in definers {
                    if definer == cid {
                        continue;
                    }
                    let vars = edge_vars.entry((definer.clone(), cid.clone())).or_default();
                    if !vars.contains(name) {
                        vars.push(name.clone());
                    }
                }
            }
        }
    }

    let mut indegree: BTreeMap<String, usize> = nodes.iter().map(|n| (n.clone(), 0)).collect();
    for ((from, to), vars) in edge_vars {
        dag.adj.entry(from.clone()).or_default().push(to.clone());
        *indegree.entry(to.clone()).or_insert(0) += 1;
        dag.edges.push((from, to, vars));
    }

    dag.cycles = tarjan_cycles(&nodes, &dag.adj);

    // Kahn's topological sort using notebook order as tie-breaker
    let mut ready: Vec<String> = nodes
        .iter()
        .filter(|n| indegree.get(*n).copied().unwrap_or(0) == 0)
        .cloned()
        .collect();
    ready.sort_by_key(|n| order_index.get(n).copied().unwrap_or(usize::MAX));
    let mut topo = Vec::with_capacity(nodes.len());
    while !ready.is_empty() {
        let cur = ready.remove(0);
        topo.push(cur.clone());
        if let Some(nexts) = dag.adj.get(&cur) {
            for next in nexts {
                if let Some(deg) = indegree.get_mut(next) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        ready.push(next.clone());
                    }
                }
            }
            ready.sort_by_key(|n| order_index.get(n).copied().unwrap_or(usize::MAX));
        }
    }
    // Append any remaining cyclic nodes in notebook order
    let in_topo: BTreeSet<String> = topo.iter().cloned().collect();
    for node in &nodes {
        if !in_topo.contains(node) {
            topo.push(node.clone());
        }
    }
    dag.topological_order = topo;

    // Cascade staleness downstream from directly stale cells
    let direct: Vec<String> = dag.direct_stale.iter().cloned().collect();
    for stale_id in &direct {
        for desc in dag.downstream_ids(stale_id) {
            let reasons = dag.upstream_stale_from.entry(desc).or_default();
            if !reasons.contains(stale_id) {
                reasons.push(stale_id.clone());
            }
        }
    }

    dag.run_stale_ids = dag
        .topological_order
        .iter()
        .filter(|id| {
            !dag.disabled.contains(*id)
                && (dag.direct_stale.contains(*id) || dag.is_upstream_stale(id))
        })
        .cloned()
        .collect();

    dag
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernelledger_models::SourceSnapshot;
    use serde_json::json;
    use std::path::PathBuf;

    fn snapshot_model(cells: Vec<(&str, &str, &str, serde_json::Value)>) -> NotebookModel {
        let mut model = NotebookModel {
            project: PathBuf::from("/tmp/nb"),
            ..Default::default()
        };
        let cell_json: Vec<serde_json::Value> = cells
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
        // Mark c2, c3, c4 as previously run & fresh; c1 is stale (edited after run)
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
        let model = snapshot_model(vec![
            ("a", "code", "v1 = v2 + 1\n", json!({})),
            ("b", "code", "v2 = v1 + 1\n", json!({})),
            ("c", "code", "v1 = 99\n", json!({})),
        ]);
        let dag = analyze_notebook(&model);
        assert_eq!(dag.multiple_definitions["v1"], vec!["a", "c"]);
        assert_eq!(dag.cycles.len(), 1);
    }
}
