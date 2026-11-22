//! Python source generated from user data: column names, file paths, variable
//! names. User data only ever appears inside a literal from [`py_str_literal`],
//! never in an f-string or `.format` template, so it cannot execute.

use std::collections::BTreeSet;

/// Python 3 keywords and soft keywords that cannot name a variable.
const PY_KEYWORDS: [&str; 35] = [
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

/// A double-quoted Python string literal that evaluates to exactly `value`.
///
/// Printable characters stay literal so generated code reads naturally.
/// Backslashes, quotes and every control, format, separator or otherwise
/// invisible character are escaped, so the literal is valid on one line and
/// cannot hide characters from a reader.
pub(crate) fn py_str_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if is_printable(ch) => out.push(ch),
            ch if (ch as u32) <= 0xff => out.push_str(&format!("\\x{:02x}", ch as u32)),
            ch if (ch as u32) <= 0xffff => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push_str(&format!("\\U{:08x}", ch as u32)),
        }
    }
    out.push('"');
    out
}

/// Characters that render visibly and safely inside a one-line literal.
/// Combining marks are printable: they attach to the preceding character.
fn is_printable(ch: char) -> bool {
    if ch == ' ' {
        return true;
    }
    if ch.is_control() || ch.is_whitespace() {
        return false;
    }
    !matches!(
        ch as u32,
        // Soft hyphen and Arabic/Syriac/Mongolian format characters.
        0xad | 0x600..=0x605 | 0x61c | 0x6dd | 0x70f | 0x180e
        // Zero-width, joiners, bidi marks, separators, invisible operators.
        | 0x200b..=0x200f | 0x2028..=0x202e | 0x2060..=0x206f
        // Byte-order mark, interlinear annotation, surrogates are not chars.
        | 0xfeff | 0xfff9..=0xfffb
        // Private use and noncharacters.
        | 0xe000..=0xf8ff | 0xfdd0..=0xfdef | 0xfffe | 0xffff
        // Tags and variation selectors supplement, supplementary private use.
        | 0xe0000..=0xe0fff | 0xf0000..=0x10ffff
    ) && (ch as u32) & 0xfffe != 0xfffe
}

/// Whether `name` can be assigned in Python: an ASCII identifier that is not
/// a keyword. Non-ASCII identifiers are valid Python but are refused so a
/// name always reads the same in every font and normalization form.
pub(crate) fn is_py_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        && !PY_KEYWORDS.contains(&name)
}

/// A readable identifier derived from arbitrary text such as a file stem:
/// lowercase ASCII words joined by `_`, prefixed when it would start with a
/// digit or collide with a keyword. Falls back to `fallback`.
pub(crate) fn py_identifier(text: &str, fallback: &str) -> String {
    let mut name = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            name.push(ch.to_ascii_lowercase());
        } else if !name.is_empty() && !name.ends_with('_') {
            name.push('_');
        }
    }
    let name = name.trim_end_matches('_');
    if name.is_empty() {
        return fallback.to_owned();
    }
    if name.starts_with(|c: char| c.is_ascii_digit()) || !is_py_identifier(name) {
        return format!("{fallback}_{name}");
    }
    name.to_owned()
}

/// `base`, or `base_2`, `base_3`, … whichever is first not in `taken`.
pub(crate) fn unique_name(base: &str, taken: &BTreeSet<String>) -> String {
    if !taken.contains(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base}_{n}"))
        .find(|name| !taken.contains(name))
        .expect("an unused suffix always exists")
}

/// A loader cell for a workspace file, or `None` for unsupported formats.
/// The variable is named after the file stem and is unique among `taken`.
pub(crate) fn loader_source(rel_path: &str, taken: &BTreeSet<String>) -> Option<String> {
    let path = std::path::Path::new(rel_path);
    let suffix = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let quoted = py_str_literal(rel_path);
    let frame = |reader: &str, extra: &str| {
        let var = unique_name(&py_identifier(&stem, "df"), taken);
        format!("import pandas as pd\n{var} = pd.{reader}({quoted}{extra})\n{var}.head()\n")
    };
    Some(match suffix.as_str() {
        "csv" => frame("read_csv", ""),
        "tsv" => frame("read_csv", ", sep=\"\\t\""),
        "parquet" | "pq" => frame("read_parquet", ""),
        "jsonl" => frame("read_json", ", lines=True"),
        "xlsx" | "xls" => frame("read_excel", ""),
        "json" => {
            let var = unique_name(&py_identifier(&stem, "data"), taken);
            format!(
                "import json\nfrom pathlib import Path\n{var} = json.loads(Path({quoted}).read_text())\n"
            )
        }
        "db" | "sqlite" | "sqlite3" => {
            let var = unique_name(&format!("{}_conn", py_identifier(&stem, "db")), taken);
            format!("import sqlite3\n{var} = sqlite3.connect({quoted})\n")
        }
        _ => return None,
    })
}

/// A matplotlib cell plotting column `y` against `x` of the frame `expr`.
/// `expr` is the inspector's own expression for the variable; column names
/// are user data and appear only as literals.
pub(crate) fn chart_source(expr: &str, kind: &str, x: &str, y: &str) -> String {
    let plot = if kind == "line" { "plot" } else { "bar" };
    let (x, y) = (py_str_literal(x), py_str_literal(y));
    format!(
        "import matplotlib.pyplot as plt\n\n\
         fig, ax = plt.subplots(figsize=(7, 3.5))\n\
         ax.{plot}({expr}[{x}], {expr}[{y}])\n\
         ax.set_xlabel({x})\n\
         ax.set_ylabel({y})\n\
         ax.set_title({y} + \" by \" + {x})\n\
         plt.tight_layout()\n\
         plt.show()"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `script` with `input` on stdin using the host Python. `None` when
    /// no Python is installed, so these checks skip rather than fail.
    ///
    /// `-X utf8` makes stdin/stdout UTF-8 everywhere: on Windows they otherwise use
    /// the console code page, which cannot decode e.g. a combining accent. (`-I`
    /// ignores `PYTHONUTF8`/`PYTHONIOENCODING`, so the flag must be explicit.)
    fn python(script: &str, input: &str) -> Option<std::process::Output> {
        use std::io::Write;
        let mut child = std::process::Command::new("python3")
            .args(["-I", "-X", "utf8", "-c", script])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;
        child.stdin.take()?.write_all(input.as_bytes()).ok()?;
        child.wait_with_output().ok()
    }

    fn python_parses(source: &str) -> Option<bool> {
        python("import ast,sys; ast.parse(sys.stdin.read())", source).map(|o| o.status.success())
    }

    /// The code points a literal evaluates to in Python.
    fn python_value(literal: &str) -> Option<Vec<u32>> {
        let output = python(
            "import ast,sys; print(' '.join(str(ord(c)) for c in ast.literal_eval(sys.stdin.read())))",
            literal,
        )?;
        assert!(output.status.success(), "Python rejected {literal}");
        Some(
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect(),
        )
    }

    const HOSTILE: [&str; 10] = [
        "say \"hi\" and 'bye'",
        "C:\\data\\new\\table",
        "line one\nline two\r\n\ttabbed",
        "cafe\u{301}",
        "zero\u{200b}width",
        "rocket \u{1f680} \u{1f469}\u{200d}\u{1f52c}",
        "{__import__('os').getcwd()}",
        "{x} {{y}} }",
        "nul\0bell\u{7}del\u{7f}",
        "rtl\u{202e}override\u{feff}bom",
    ];

    #[test]
    fn literals_escape_syntax_and_invisible_characters() {
        assert_eq!(py_str_literal("plain"), "\"plain\"");
        assert_eq!(py_str_literal("a\"b"), "\"a\\\"b\"");
        assert_eq!(py_str_literal("a\\b"), "\"a\\\\b\"");
        assert_eq!(py_str_literal("a\nb\rc\td"), "\"a\\nb\\rc\\td\"");
        // Printable Unicode, including a combining accent and emoji, stays literal.
        assert_eq!(py_str_literal("cafe\u{301}"), "\"cafe\u{301}\"");
        assert_eq!(py_str_literal("\u{1f680}"), "\"\u{1f680}\"");
        assert_eq!(py_str_literal("{x}"), "\"{x}\"");
        // Invisible and control characters are spelled out.
        assert_eq!(py_str_literal("a\u{200b}b"), "\"a\\u200bb\"");
        assert_eq!(py_str_literal("\0\u{7f}"), "\"\\x00\\x7f\"");
        assert_eq!(py_str_literal("\u{a0}"), "\"\\xa0\"");
        assert_eq!(py_str_literal("\u{e0001}"), "\"\\U000e0001\"");
        for value in HOSTILE {
            let literal = py_str_literal(value);
            assert!(!literal[1..literal.len() - 1].contains(['\n', '\r']));
            assert!(
                !literal.contains("\\u{"),
                "Rust escapes are not Python: {literal}"
            );
        }
    }

    #[test]
    fn literals_round_trip_through_python_when_available() {
        for value in HOSTILE {
            let literal = py_str_literal(value);
            match python_value(&literal) {
                None => return, // No Python on this host.
                Some(points) => assert_eq!(
                    points,
                    value.chars().map(|c| c as u32).collect::<Vec<_>>(),
                    "{literal}"
                ),
            }
        }
    }

    #[test]
    fn generated_chart_and_loaders_parse_and_never_interpolate_user_data() {
        let taken = BTreeSet::new();
        for column in HOSTILE {
            let chart = chart_source("df", "bar", column, column);
            assert!(
                !chart.contains("(f\"") && !chart.contains(" f\""),
                "{chart}"
            );
            assert!(!chart.contains(".format("), "{chart}");
            let loader = loader_source(&format!("data/{column}.csv"), &taken).unwrap();
            for source in [chart, loader] {
                if let Some(ok) = python_parses(&source) {
                    assert!(ok, "generated source must parse:\n{source}");
                }
            }
        }
    }

    #[test]
    fn identifiers_are_sanitized_and_deduplicated() {
        assert!(is_py_identifier("sql_result"));
        assert!(is_py_identifier("_private2"));
        for bad in ["", "2x", "class", "None", "a-b", "a b", "caf\u{e9}"] {
            assert!(!is_py_identifier(bad), "{bad:?}");
        }
        assert_eq!(py_identifier("Sales Q1-2024", "df"), "sales_q1_2024");
        assert_eq!(py_identifier("2024 report", "df"), "df_2024_report");
        assert_eq!(py_identifier("class", "df"), "df_class");
        assert_eq!(py_identifier("\u{1f680}", "df"), "df");
        let taken: BTreeSet<String> = ["sales".into(), "sales_2".into()].into();
        assert_eq!(unique_name("sales", &taken), "sales_3");
        assert_eq!(unique_name("orders", &taken), "orders");
        assert_eq!(
            loader_source("in/Sales.csv", &taken).unwrap(),
            "import pandas as pd\nsales_3 = pd.read_csv(\"in/Sales.csv\")\nsales_3.head()\n"
        );
        assert!(
            loader_source("x.tsv", &taken)
                .unwrap()
                .contains("pd.read_csv(\"x.tsv\", sep=\"\\t\")")
        );
        assert!(
            loader_source("Lab.sqlite", &taken)
                .unwrap()
                .contains("lab_conn = sqlite3.connect(")
        );
        assert_eq!(loader_source("notes.txt", &taken), None);
    }
}
