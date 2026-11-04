//! Bounded, inert TeX math subset for the native renderer. No macros or file I/O.
use anyhow::{Result, bail, ensure};

pub const SOURCE_LIMIT: usize = 8192;
const TOKEN_LIMIT: usize = 512;
const DEPTH_LIMIT: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MathNode {
    Text(String),
    Row(Vec<MathNode>),
    Fraction(Box<MathNode>, Box<MathNode>),
    Root(Box<MathNode>),
    Script {
        base: Box<MathNode>,
        upper: Option<Box<MathNode>>,
        lower: Option<Box<MathNode>>,
    },
}

impl MathNode {
    pub fn description(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Row(nodes) => nodes
                .iter()
                .map(Self::description)
                .collect::<Vec<_>>()
                .join(""),
            Self::Fraction(top, bottom) => format!(
                "fraction ({}) over ({})",
                top.description(),
                bottom.description()
            ),
            Self::Root(value) => format!("square root ({})", value.description()),
            Self::Script { base, upper, lower } => format!(
                "{}{}{}",
                base.description(),
                lower
                    .as_ref()
                    .map(|n| format!(" subscript ({})", n.description()))
                    .unwrap_or_default(),
                upper
                    .as_ref()
                    .map(|n| format!(" superscript ({})", n.description()))
                    .unwrap_or_default()
            ),
        }
    }
}

struct Parser {
    chars: Vec<char>,
    at: usize,
    tokens: usize,
}

impl Parser {
    fn count(&mut self) -> Result<()> {
        self.tokens += 1;
        ensure!(self.tokens <= TOKEN_LIMIT, "Equation exceeds 512 tokens");
        Ok(())
    }
    fn spaces(&mut self) {
        while self.chars.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
    }
    fn argument(&mut self, depth: usize) -> Result<MathNode> {
        self.spaces();
        self.atom(depth)
    }
    fn atom(&mut self, depth: usize) -> Result<MathNode> {
        ensure!(depth <= DEPTH_LIMIT, "Equation nesting exceeds 24 levels");
        let value = self
            .chars
            .get(self.at)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Missing equation argument"))?;
        self.at += 1;
        self.count()?;
        match value {
            '{' => self.sequence(true, depth + 1),
            '}' | '^' | '_' | '$' => bail!("Unexpected equation delimiter"),
            '\\' => {
                let start = self.at;
                while self
                    .chars
                    .get(self.at)
                    .is_some_and(|c| c.is_ascii_alphabetic())
                {
                    self.at += 1;
                }
                let command: String = self.chars[start..self.at].iter().collect();
                if command.is_empty() {
                    let escaped = self
                        .chars
                        .get(self.at)
                        .copied()
                        .ok_or_else(|| anyhow::anyhow!("Incomplete equation command"))?;
                    self.at += 1;
                    return match escaped {
                        '{' | '}' | '%' | '#' | '$' | '_' => {
                            Ok(MathNode::Text(escaped.to_string()))
                        }
                        ',' | ';' | ' ' => Ok(MathNode::Text(" ".into())),
                        '!' => Ok(MathNode::Text(String::new())),
                        _ => bail!("Unsupported equation escape"),
                    };
                }
                Ok(match command.as_str() {
                    "frac" | "dfrac" | "tfrac" => MathNode::Fraction(
                        Box::new(self.argument(depth + 1)?),
                        Box::new(self.argument(depth + 1)?),
                    ),
                    "sqrt" => MathNode::Root(Box::new(self.argument(depth + 1)?)),
                    "mathrm" | "mathbf" | "mathit" | "text" | "operatorname" => {
                        self.argument(depth + 1)?
                    }
                    "left" | "right" | "displaystyle" | "textstyle" => {
                        MathNode::Text(String::new())
                    }
                    "quad" | "qquad" => MathNode::Text("  ".into()),
                    other => MathNode::Text(
                        symbol(other)
                            .ok_or_else(|| {
                                anyhow::anyhow!("Unsupported equation command: \\{other}")
                            })?
                            .into(),
                    ),
                })
            }
            c if c.is_control() && !c.is_whitespace() => {
                bail!("Equation contains control characters")
            }
            c => Ok(MathNode::Text(if c.is_whitespace() {
                " ".into()
            } else {
                c.to_string()
            })),
        }
    }
    fn sequence(&mut self, group: bool, depth: usize) -> Result<MathNode> {
        ensure!(depth <= DEPTH_LIMIT, "Equation nesting exceeds 24 levels");
        let mut nodes = Vec::new();
        loop {
            match self.chars.get(self.at) {
                None if group => bail!("Unclosed equation group"),
                None => break,
                Some('}') if group => {
                    self.at += 1;
                    break;
                }
                Some('^' | '_') => {
                    let upper = self.chars[self.at] == '^';
                    self.at += 1;
                    self.count()?;
                    while matches!(nodes.last(), Some(MathNode::Text(text)) if text.trim().is_empty())
                    {
                        nodes.pop();
                    }
                    let base = nodes
                        .pop()
                        .ok_or_else(|| anyhow::anyhow!("Script has no base"))?;
                    let argument = Box::new(self.argument(depth + 1)?);
                    let mut script = match base {
                        MathNode::Script { .. } => base,
                        _ => MathNode::Script {
                            base: Box::new(base),
                            upper: None,
                            lower: None,
                        },
                    };
                    if let MathNode::Script {
                        upper: high,
                        lower: low,
                        ..
                    } = &mut script
                    {
                        let target = if upper { high } else { low };
                        ensure!(target.is_none(), "Duplicate equation script");
                        *target = Some(argument);
                    }
                    nodes.push(script);
                }
                _ => nodes.push(self.atom(depth)?),
            }
        }
        // Keep characters separate until scripts have bound to the last atom.
        let mut merged = Vec::new();
        for node in nodes {
            if let MathNode::Text(text) = &node
                && let Some(MathNode::Text(previous)) = merged.last_mut()
            {
                previous.push_str(text);
            } else {
                merged.push(node);
            }
        }
        Ok(if merged.len() == 1 {
            merged.remove(0)
        } else {
            MathNode::Row(merged)
        })
    }
}

pub fn parse(source: &str) -> Result<MathNode> {
    ensure!(
        source.len() <= SOURCE_LIMIT,
        "Equation source exceeds 8 KiB"
    );
    let mut source = source.trim();
    for (start, end) in [("$$", "$$"), ("$", "$"), ("\\[", "\\]"), ("\\(", "\\)")] {
        if source.starts_with(start)
            && source.ends_with(end)
            && source.len() >= start.len() + end.len()
        {
            source = &source[start.len()..source.len() - end.len()];
            break;
        }
    }
    ensure!(!source.is_empty(), "Empty equation");
    Parser {
        chars: source.chars().collect(),
        at: 0,
        tokens: 0,
    }
    .sequence(false, 0)
}

fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" | "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" | "vartheta" => "θ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" | "varpi" => "π",
        "rho" | "varrho" => "ρ",
        "sigma" | "varsigma" => "σ",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" | "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "sum" => "∑",
        "prod" => "∏",
        "int" => "∫",
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "times" => "×",
        "cdot" => "⋅",
        "pm" => "±",
        "mp" => "∓",
        "div" => "÷",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ne" | "neq" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "to" | "rightarrow" => "→",
        "leftarrow" => "←",
        "leftrightarrow" => "↔",
        "in" => "∈",
        "notin" => "∉",
        "subset" => "⊂",
        "subseteq" => "⊆",
        "cup" => "∪",
        "cap" => "∩",
        "emptyset" => "∅",
        "forall" => "∀",
        "exists" => "∃",
        "ldots" | "cdots" => "…",
        "langle" => "⟨",
        "rangle" => "⟩",
        "vert" | "mid" => "|",
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "log" => "log",
        "ln" => "ln",
        "exp" => "exp",
        "lim" => "lim",
        "min" => "min",
        "max" => "max",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_math_and_scripts_bind_to_the_last_atom() {
        let tree = parse(r"$$\frac{x_i^2 + \alpha}{\sqrt{1+y}} = 42$$").unwrap();
        assert!(
            tree.description().contains(
                "fraction (x subscript (i) superscript (2) + α) over (square root (1+y))"
            )
        );
        assert_eq!(parse("ab^2").unwrap().description(), "ab superscript (2)");
        assert!(parse("x^2^3").is_err());
    }
    #[test]
    fn arbitrary_tex_and_excessive_work_are_refused() {
        for text in [
            r"\input{secret}",
            r"\href{https://example.com}{x}",
            r"\def\x{y}",
            "{x",
            "x}",
            "^x",
            "x$",
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
        assert!(parse(&"x".repeat(SOURCE_LIMIT + 1)).is_err());
        assert!(parse(&"x".repeat(TOKEN_LIMIT + 1)).is_err());
        assert!(parse(&format!("{}x{}", "{".repeat(30), "}".repeat(30))).is_err());
    }
}
