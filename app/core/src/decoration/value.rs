//! The writing half of a todo block's typst: typst's value language narrowed to
//! what a block uses, and how it is laid out.
//!
//! `//` runs are data ([`Arg::rationale`], `Group::tail`), since they are the only
//! record of why a priority is what it is. Layout is a pure function of the
//! value, so re-rendering a parse is idempotent. This is the only renderer: the
//! containment check ([`super::verified`]) renders here too, so what a write
//! measures and what a block shows are one function.

use std::fmt::Write as _;

/// One argument of a call or a group; positional ones make a `Group` an array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg {
    /// The argument's value.
    pub value: Value,
    /// `name:` before it, or `None` when it is positional.
    pub name: Option<String>,
    /// The `//` lines immediately above it, stripped of their `//` and trimmed.
    pub rationale: Vec<String>,
}

impl Arg {
    /// A named argument with no rationale.
    #[must_use]
    pub fn named(name: &str, value: Value) -> Arg {
        Arg {
            value,
            name: Some(name.to_owned()),
            rationale: Vec::new(),
        }
    }

    /// A named argument carrying the `//` run that explains it.
    #[must_use]
    pub fn explained(name: &str, value: Value, rationale: Vec<String>) -> Arg {
        Arg {
            value,
            name: Some(name.to_owned()),
            rationale,
        }
    }

    /// A positional argument — what makes a `Group` an array.
    #[must_use]
    pub(crate) fn positional(value: Value) -> Arg {
        Arg {
            value,
            name: None,
            rationale: Vec::new(),
        }
    }
}

/// A typst value, narrowed to the todo block's subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// A numeric literal, source text kept verbatim. It may carry a trailing
    /// `%` or a unit: in typst those are part of the literal, not an operator.
    Num(String),
    /// A string literal's source between the quotes, escapes kept: unescaping
    /// and re-escaping is lossy (`\"` against `\u{22}`).
    Str(String),
    /// A bare or dotted name: `none`, `true`, `category.home`.
    Ident(String),
    /// `[markup]`, kept as opaque source.
    Content(String),
    /// `(...)` — typst's dict and array are one syntax, so this is one node.
    Group {
        /// Its arguments; named ones make it a dict, positional ones an array.
        args: Vec<Arg>,
        /// Comments after the last argument, which are about no single argument.
        tail: Vec<String>,
    },
    /// `fn(args)` plus any trailing `[content]` blocks.
    Call(Box<Call>),
}

/// A call: `fn(args)` and the `[content]` blocks attached to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The function's name.
    pub(crate) callee: String,
    /// Its arguments.
    pub args: Vec<Arg>,
    /// `#fn(...)[body][more]` — content blocks attached to the call.
    pub trailing: Vec<String>,
    /// Whether the source wrote `#fn(...)`: required in markup, forbidden in code.
    pub(crate) hashed: bool,
    /// Comments after the last argument — see [`Value::Group`]'s `tail`.
    pub tail: Vec<String>,
}

/// One level of indentation.
pub(crate) const INDENT: &str = "  ";
/// The column an argument list is allowed to reach before it breaks.
pub(crate) const WIDTH: usize = 88;

impl Value {
    /// The source this value renders to at `indent`.
    #[must_use]
    pub fn render(&self, indent: usize) -> String {
        match self {
            Value::Num(raw) | Value::Ident(raw) => raw.clone(),
            Value::Str(raw) => format!("\"{raw}\""),
            Value::Content(raw) => format!("[{raw}]"),
            Value::Group { args, tail } => render_args(args, tail, indent, ""),
            Value::Call(call) => {
                let head = if call.hashed {
                    format!("#{}", call.callee)
                } else {
                    call.callee.clone()
                };
                let mut out = render_args(&call.args, &call.tail, indent, &head);
                for block in &call.trailing {
                    let _ = write!(out, "[{block}]");
                }
                out
            }
        }
    }
}

/// A `//` run as source lines, each under `pad`. An empty line renders as a
/// bare `//` rather than as `// ` with a trailing space.
fn comment_lines(lines: &[String], pad: &str) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            if line.is_empty() {
                format!("{pad}//")
            } else {
                format!("{pad}// {line}")
            }
        })
        .collect()
}

/// One line if it fits and nothing forbids it, several otherwise.
///
/// Forbidden, for correctness: a comment (it would comment out the rest of the
/// call), a multi-line `[content]` (reflowing markup edits it), and overflow.
/// The width counts characters, not bytes, so an em dash is one column.
fn render_args(args: &[Arg], tail: &[String], indent: usize, prefix: &str) -> String {
    if args.is_empty() && tail.is_empty() {
        return format!("{prefix}()");
    }
    let rendered: Vec<String> = args.iter().map(|arg| render_arg(arg, indent)).collect();
    let inline = tail.is_empty()
        && args.iter().all(|arg| arg.rationale.is_empty())
        && !rendered.iter().any(|text| text.contains('\n'));
    if inline {
        let one = format!("{prefix}({})", rendered.join(", "));
        if one.chars().count() + indent * INDENT.chars().count() <= WIDTH {
            return one;
        }
    }
    let pad = INDENT.repeat(indent + 1);
    let mut lines = vec![format!("{prefix}(")];
    for arg in args {
        lines.extend(comment_lines(&arg.rationale, &pad));
        lines.push(format!("{pad}{},", render_arg(arg, indent + 1)));
    }
    lines.extend(comment_lines(tail, &pad));
    lines.push(format!("{})", INDENT.repeat(indent)));
    lines.join("\n")
}

fn render_arg(arg: &Arg, indent: usize) -> String {
    let value = arg.value.render(indent);
    match &arg.name {
        None => value,
        Some(name) => format!("{name}: {value}"),
    }
}

/// A top-level todo block with the comment run above it.
#[must_use]
pub(crate) fn render_block(call: &Call, leading: &[String]) -> String {
    let mut out = String::new();
    for line in comment_lines(leading, "") {
        let _ = writeln!(out, "{line}");
    }
    out.push_str(&Value::Call(Box::new(call.clone())).render(0));
    out
}
