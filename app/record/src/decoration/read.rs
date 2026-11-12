//! Typst read with typst's own parser (`typst-syntax`), so there is one reading
//! of the language: a projection of its [`SyntaxNode`] tree onto
//! [`super::value::Value`] that refuses by name whatever a block has no
//! meaning for.
//!
//! A `//` run attaches to the previous argument when no newline separates them
//! and to the next one otherwise (`priority: 50%, // moot-tier` is about the
//! priority). A run with no argument after it becomes the group's `tail`, never
//! dropped: it can be the only record of why a todo was resolved.

use typst_syntax::{SyntaxKind, SyntaxNode};

use super::DecorationError;
use super::value::{Arg, Call, Value};

/// A todo block read back: the call, and the `//` run above it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Located {
    /// The `#kind(...)[body]` call.
    pub call: Call,
    /// The comment lines immediately above it, which belong to the block.
    pub leading: Vec<String>,
}

/// The one block a rendering is, and nothing beside it.
///
/// The read half of [`super::render_verified`]: anything outside the call is
/// the escape being looked for, and a typst diagnostic is a
/// refusal, since the parser's recovery is not the text that was written.
pub(crate) fn only_block(text: &str, kind: &str) -> Result<Located, DecorationError> {
    let root = typst_syntax::parse(text);
    let (errors, _) = root.errors_and_warnings();
    if let Some(first) = errors.first() {
        return Err(DecorationError(format!(
            "typst reads this source as malformed: {}",
            first.message
        )));
    }
    let mut leading = Vec::new();
    let mut found = None;
    for child in root.children() {
        match child.kind() {
            SyntaxKind::LineComment if found.is_none() => leading.push(comment_text(child)),
            SyntaxKind::Space | SyntaxKind::Hash => {}
            SyntaxKind::FuncCall if found.is_none() => {
                let call = call_of(child, true)?;
                if call.callee != kind {
                    return Err(DecorationError(format!(
                        "expected one {kind} block, found a call to {}",
                        call.callee
                    )));
                }
                found = Some(call);
            }
            other => {
                return Err(DecorationError(format!(
                    "{other:?} beside the block: a rendering is the block and nothing else"
                )));
            }
        }
    }
    found
        .map(|call| Located { call, leading })
        .ok_or_else(|| DecorationError(format!("no {kind} block in this source")))
}

/// One typst value, parsed in code context — what a `SubTodo.body` holds.
///
/// Parsed in code context because `[..]` is a content block there and ordinary
/// text in markup.
pub fn value(text: &str) -> Result<Value, DecorationError> {
    let root = typst_syntax::parse_code(text);
    let mut found = None;
    for child in root.children() {
        if is_trivia(child.kind()) {
            continue;
        }
        if found.is_some() {
            return Err(DecorationError(format!(
                "expected one typst value, got several in {text:?}"
            )));
        }
        found = Some(value_of(child)?);
    }
    found.ok_or_else(|| DecorationError(format!("expected a typst value in {text:?}")))
}

fn is_trivia(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Space
            | SyntaxKind::Parbreak
            | SyntaxKind::Comma
            | SyntaxKind::LeftParen
            | SyntaxKind::RightParen
            | SyntaxKind::Colon
            | SyntaxKind::Semicolon
            | SyntaxKind::Hash
            | SyntaxKind::End
    )
}

/// A `LineComment`'s text, without its `//` and trimmed.
fn comment_text(node: &SyntaxNode) -> String {
    node.leaf_text()
        .strip_prefix("//")
        .unwrap_or(node.leaf_text())
        .trim()
        .to_owned()
}

/// A bracketed or quoted leaf's source without its delimiters.
fn interior(node: &SyntaxNode) -> String {
    let text = node.full_text();
    let mut chars = text.chars();
    chars.next();
    chars.next_back();
    chars.as_str().to_owned()
}

fn call_of(node: &SyntaxNode, hashed: bool) -> Result<Call, DecorationError> {
    let mut callee = None;
    let mut args = Vec::new();
    let mut trailing = Vec::new();
    let mut tail = Vec::new();
    for child in node.children() {
        match child.kind() {
            SyntaxKind::Ident if callee.is_none() => callee = Some(child.leaf_text().to_string()),
            SyntaxKind::Args => {
                let read = args_of(child)?;
                args = read.args;
                trailing = read.trailing;
                tail = read.tail;
            }
            SyntaxKind::Space | SyntaxKind::Hash => {}
            other => {
                return Err(DecorationError(format!(
                    "a call in a block is `name(...)`, found {other:?}"
                )));
            }
        }
    }
    Ok(Call {
        callee: callee.ok_or_else(|| DecorationError("a call with no name".to_owned()))?,
        args,
        trailing,
        hashed,
        tail,
    })
}

struct Read {
    args: Vec<Arg>,
    trailing: Vec<String>,
    tail: Vec<String>,
}

/// The arguments of a call, an array or a dictionary, with every `//` run
/// attributed by the module header's rule.
fn args_of(node: &SyntaxNode) -> Result<Read, DecorationError> {
    let mut args: Vec<Arg> = Vec::new();
    let mut trailing = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    // Whether a newline has passed since the last argument was read.
    let mut broke = true;
    for child in node.children() {
        match child.kind() {
            SyntaxKind::Space | SyntaxKind::Parbreak => {
                if child.leaf_text().contains('\n') {
                    broke = true;
                }
            }
            SyntaxKind::LeftParen | SyntaxKind::RightParen | SyntaxKind::Comma => {}
            SyntaxKind::LineComment => {
                let text = comment_text(child);
                match args.last_mut() {
                    Some(last) if !broke => last.rationale.push(text),
                    _ => pending.push(text),
                }
            }
            // typst's tree puts a call's trailing `[body]` inside `Args`.
            SyntaxKind::ContentBlock if node.kind() == SyntaxKind::Args => {
                trailing.push(interior_markup(child)?);
            }
            SyntaxKind::Named => {
                args.push(named_of(child, std::mem::take(&mut pending))?);
                broke = false;
            }
            _ => {
                args.push(Arg {
                    value: value_of(child)?,
                    name: None,
                    rationale: std::mem::take(&mut pending),
                });
                broke = false;
            }
        }
    }
    Ok(Read {
        args,
        trailing,
        tail: pending,
    })
}

fn named_of(node: &SyntaxNode, rationale: Vec<String>) -> Result<Arg, DecorationError> {
    let mut name = None;
    let mut value = None;
    for child in node.children() {
        match child.kind() {
            SyntaxKind::Ident if name.is_none() => name = Some(child.leaf_text().to_string()),
            SyntaxKind::Space | SyntaxKind::Colon => {}
            _ => value = Some(value_of(child)?),
        }
    }
    match (name, value) {
        (Some(name), Some(value)) => Ok(Arg {
            value,
            name: Some(name),
            rationale,
        }),
        _ => Err(DecorationError(
            "a named argument is `name: value`".to_owned(),
        )),
    }
}

/// The markup inside a `[..]` block, as source.
fn interior_markup(node: &SyntaxNode) -> Result<String, DecorationError> {
    for child in node.children() {
        if child.kind() == SyntaxKind::Markup {
            return Ok(child.full_text().to_string());
        }
    }
    Err(DecorationError(
        "a content block with no markup in it".to_owned(),
    ))
}

/// One expression node as a [`Value`].
///
/// Anything with no arm is refused by name, never approximated.
fn value_of(node: &SyntaxNode) -> Result<Value, DecorationError> {
    match node.kind() {
        SyntaxKind::Int | SyntaxKind::Float | SyntaxKind::Numeric => {
            Ok(Value::Num(node.leaf_text().to_string()))
        }
        SyntaxKind::Str => Ok(Value::Str(interior(node))),
        // Typst's keywords `none`, `auto`, `true`, `false` are plain identifiers
        // to a block, which has no use for the distinction.
        SyntaxKind::Ident
        | SyntaxKind::Bool
        | SyntaxKind::None
        | SyntaxKind::Auto
        | SyntaxKind::FieldAccess => Ok(Value::Ident(node.full_text().to_string())),
        SyntaxKind::ContentBlock => Ok(Value::Content(interior_markup(node)?)),
        SyntaxKind::Array | SyntaxKind::Dict | SyntaxKind::Parenthesized => {
            let read = args_of(node)?;
            Ok(Value::Group {
                args: read.args,
                tail: read.tail,
            })
        }
        SyntaxKind::FuncCall => Ok(Value::Call(Box::new(call_of(node, false)?))),
        other => Err(DecorationError(format!(
            "{other:?} is outside the block's value grammar: {:?}",
            node.full_text()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> Located {
        ["todo", "ekaTodo", "suzateryTodo"]
            .iter()
            .find_map(|kind| only_block(text, kind).ok())
            .unwrap_or_else(|| panic!("one block in {text:?}"))
    }

    /// A `#todo(` inside a body is prose about a todo, not a record.
    #[test]
    fn a_call_inside_a_body_is_not_a_block() {
        let found = one("#todo(id: \"a\")[see \\#todo(id: \"b\") above]");
        assert_eq!(found.call.args.len(), 1);
        assert_eq!(found.call.args[0].name.as_deref(), Some("id"));
    }

    #[test]
    fn escapes_in_a_body_survive_as_source() {
        let found = one("#todo(id: \"a\")[a \\] b \\@ c]");
        assert_eq!(found.call.trailing, vec!["a \\] b \\@ c".to_owned()]);
    }

    /// Anything beside the block is the escape the check looks for, and so are another
    /// constructor and what typst's parser recovers from as malformed: a recovery is not the text.
    #[test]
    fn only_block_refuses_anything_but_the_one_block() {
        for text in [
            "#todo(id: \"a\")[B] and then prose",
            "#todo(id: \"a\")[B]\n#todo(id: \"b\")[C]",
            "= A heading\n#todo(id: \"a\")[B]",
            "",
            "#ekaTodo(id: \"a\")[B]",
            "#todo(id: \"a\"[B]",
        ] {
            assert!(only_block(text, "todo").is_err(), "{text:?}");
        }
    }
}
