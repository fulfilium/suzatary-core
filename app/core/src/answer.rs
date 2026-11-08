//! An answer in blocks: the secretary's Typst, split by typst's own parser at top-level blank
//! lines (never inside raw, an equation or a content block, never between items of one list).
//!
//! The blocks' sources concatenate to the answer byte for byte; the page's streamed blocks rest
//! on that law.
//!
//! A block is rendered only through [`Renderable`], compiled in the closed world, whose library
//! has no `html` module: the model writes this Typst, and `html.elem` could put any element on
//! the page, so the world, not a scan of the source, keeps it out of reach.

use serde::{Deserialize, Serialize};
use typst_syntax::{SyntaxKind, SyntaxNode};

/// What a block is, read off its first node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "BlockKind")]
pub enum Kind {
    /// A paragraph, or anything else.
    Prose,
    /// A `=` heading.
    Heading,
    /// A bullet, numbered or term list.
    List,
    /// Raw code.
    Code,
    /// An equation on its own.
    Equation,
}

/// One block of an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    /// Its Typst, with the blank line that ends it.
    pub source: String,
    /// Its words, as the parser reads them.
    pub plain: String,
    /// What it is.
    pub kind: Kind,
}

/// An answer read as prose (its blocks' words, a blank line between), as the page's Copy gives
/// it; for a field that never renders markup, such as a decision's rationale.
#[must_use]
pub fn prose(answer: &str) -> String {
    blocks(answer)
        .into_iter()
        .map(|block| block.plain)
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Split an answer into its blocks. An answer of only whitespace has none.
#[must_use]
pub fn blocks(answer: &str) -> Vec<Block> {
    let root = typst_syntax::parse(answer);
    let children: Vec<&SyntaxNode> = root.children().collect();
    let mut spans: Vec<Vec<&SyntaxNode>> = Vec::new();
    let mut current: Vec<&SyntaxNode> = Vec::new();
    for (at, child) in children.iter().enumerate() {
        current.push(child);
        if child.kind() == SyntaxKind::Parbreak
            && current.iter().any(|node| !blank(node))
            && children[at + 1..].iter().any(|node| !blank(node))
            && !one_list(before(&children, at), after(&children, at))
        {
            spans.push(std::mem::take(&mut current));
        }
    }
    spans.push(current);
    let texts: Vec<String> = spans
        .iter()
        .map(|span| {
            span.iter()
                .map(|node| node.full_text().to_string())
                .collect()
        })
        .collect();
    spans
        .iter()
        .zip(cut(answer, &texts))
        .filter(|(span, _)| span.iter().any(|node| !blank(node)))
        .map(|(span, source)| block(span, source))
        .collect()
}

/// Each span's source, cut from the answer itself, because typst's tree can drop characters (a raw
/// fence's trailing space): a dropped character stays with its span, the last takes the rest.
fn cut(answer: &str, texts: &[String]) -> Vec<String> {
    let mut rest = answer;
    let mut sources = Vec::with_capacity(texts.len());
    for (at, text) in texts.iter().enumerate() {
        let end = if at + 1 == texts.len() {
            rest.len()
        } else {
            let mut end = 0;
            for wanted in text.chars() {
                if let Some(found) = rest[end..].find(wanted) {
                    end += found + wanted.len_utf8();
                }
            }
            end
        };
        sources.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    sources
}

fn block(span: &[&SyntaxNode], source: String) -> Block {
    let mut plain = String::new();
    for node in span {
        markup(node, &mut plain);
    }
    Block {
        source,
        plain: plain.trim().to_owned(),
        kind: span
            .iter()
            .find(|node| !blank(node))
            .map_or(Kind::Prose, |node| kind_of(node)),
    }
}

fn blank(node: &SyntaxNode) -> bool {
    matches!(node.kind(), SyntaxKind::Space | SyntaxKind::Parbreak)
}

fn before<'a>(children: &[&'a SyntaxNode], at: usize) -> Option<&'a SyntaxNode> {
    children[..at]
        .iter()
        .rev()
        .find(|node| !blank(node))
        .copied()
}

fn after<'a>(children: &[&'a SyntaxNode], at: usize) -> Option<&'a SyntaxNode> {
    children[at + 1..].iter().find(|node| !blank(node)).copied()
}

/// A blank line between two items of the same list is a loose list, not two
/// blocks.
fn one_list(before: Option<&SyntaxNode>, after: Option<&SyntaxNode>) -> bool {
    match (before, after) {
        (Some(a), Some(b)) => {
            a.kind() == b.kind()
                && matches!(
                    a.kind(),
                    SyntaxKind::ListItem | SyntaxKind::EnumItem | SyntaxKind::TermItem
                )
        }
        _ => false,
    }
}

fn kind_of(node: &SyntaxNode) -> Kind {
    match node.kind() {
        SyntaxKind::Heading => Kind::Heading,
        SyntaxKind::ListItem | SyntaxKind::EnumItem | SyntaxKind::TermItem => Kind::List,
        SyntaxKind::Raw => Kind::Code,
        SyntaxKind::Equation => Kind::Equation,
        _ => Kind::Prose,
    }
}

/// The words of a markup node: its text, its code and equations as written,
/// never a marker, a comment or the code around a content block.
fn markup(node: &SyntaxNode, out: &mut String) {
    match node.kind() {
        // A line already ended (a heading, a list item) needs no space after it.
        SyntaxKind::Space if out.ends_with('\n') => {}
        SyntaxKind::Space => out.push(' '),
        SyntaxKind::Parbreak => out.push_str("\n\n"),
        SyntaxKind::Linebreak => out.push('\n'),
        SyntaxKind::Escape => out.push_str(node.leaf_text().as_str().trim_start_matches('\\')),
        SyntaxKind::Text
        | SyntaxKind::Shorthand
        | SyntaxKind::SmartQuote
        | SyntaxKind::Link
        | SyntaxKind::RefMarker => out.push_str(node.leaf_text().as_str()),
        SyntaxKind::Raw => raw(node, out),
        SyntaxKind::Equation => {
            out.push_str(node.full_text().trim_matches('$').trim());
        }
        SyntaxKind::ListItem | SyntaxKind::EnumItem | SyntaxKind::TermItem => {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("• ");
            node.children()
                .skip_while(|child| {
                    matches!(
                        child.kind(),
                        SyntaxKind::ListMarker
                            | SyntaxKind::EnumMarker
                            | SyntaxKind::TermMarker
                            | SyntaxKind::Space
                    )
                })
                .for_each(|child| markup(child, out));
        }
        SyntaxKind::Heading => {
            node.children().for_each(|child| markup(child, out));
            out.push('\n');
        }
        SyntaxKind::Markup | SyntaxKind::Strong | SyntaxKind::Emph => {
            node.children().for_each(|child| markup(child, out));
        }
        kind if node.children().len() > 0 && !kind.is_trivia() => code(node, out),
        _ => {}
    }
}

/// Code says nothing itself; only the content blocks it holds are words.
fn code(node: &SyntaxNode, out: &mut String) {
    for child in node.children() {
        if child.kind() == SyntaxKind::ContentBlock {
            child.children().for_each(|inner| markup(inner, out));
        } else {
            code(child, out);
        }
    }
}

fn raw(node: &SyntaxNode, out: &mut String) {
    for child in node.children() {
        match child.kind() {
            SyntaxKind::RawDelim | SyntaxKind::RawLang => {}
            _ => out.push_str(&child.full_text()),
        }
    }
}

/// A block for the box to render, in the closed world and nowhere else: `Answers` takes only
/// this, and it is the value the answer template (`rendering::ANSWER`) is called with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Renderable {
    source: String,
}

impl Renderable {
    /// The model's block, as it wrote it.
    #[must_use]
    pub fn of(source: &str) -> Self {
        Self {
            source: source.to_owned(),
        }
    }

    /// The Typst.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.source
    }
}

/// A block's name, as the stream and the page spell it: the sha256 of its
/// source, in hex. The browser names a block by this, never by its text.
#[must_use]
pub fn sha(source: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(source.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(answer: &str) -> Vec<String> {
        blocks(answer)
            .into_iter()
            .map(|block| block.source)
            .collect()
    }

    #[test]
    fn an_answer_splits_at_top_level_blank_lines_only() {
        let answer = "= The permit\n\nIt is due *Friday*.\n\n```\nfn a() {}\n\nfn b() {}\n```\n\n$ x^2 $\n\n- one\n\n- two\n\n#panel[a\n\nb]\n";
        let found = blocks(answer);
        assert_eq!(
            found.iter().map(|block| block.kind).collect::<Vec<_>>(),
            vec![
                Kind::Heading,
                Kind::Prose,
                Kind::Code,
                Kind::Equation,
                Kind::List,
                Kind::Prose
            ]
        );
        assert_eq!(found[0].plain, "The permit");
        assert_eq!(found[1].plain, "It is due Friday.");
        assert_eq!(found[2].plain, "fn a() {}\n\nfn b() {}");
        assert_eq!(found[3].plain, "x^2");
        assert_eq!(found[4].plain, "• one\n\n• two");
        assert_eq!(found[5].plain, "a\n\nb");
        assert_eq!(found[0].source, "= The permit\n\n");
    }

    #[test]
    fn whitespace_is_no_block_and_one_paragraph_is_one() {
        assert!(blocks("").is_empty());
        assert!(blocks(" \n\n ").is_empty());
        assert_eq!(sources("\n\nhello\n\n"), vec!["\n\nhello\n\n"]);
    }

    #[test]
    fn a_sha_is_the_source_s() {
        assert_eq!(sha("a").len(), 64);
        assert_ne!(sha("a"), sha("a\n\n"));
    }

    proptest::proptest! {
        /// Block sources concatenate to the answer, whatever the model wrote.
        #[test]
        fn block_sources_concatenate_to_the_answer(
            pieces in proptest::collection::vec(
                proptest::sample::select(vec![
                    "word", " ", "\n", "\n\n", "= H", "- item", "+ n", "/ t: d", "*b*", "_e_",
                    "```", "`r`", "$", "$ x $", "#f[a\n\nb]", "#let x = 1", "[", "]", "\\#", "//c",
                ]),
                0..40,
            )
        ) {
            let answer: String = pieces.concat();
            let found = blocks(&answer);
            let joined: String = found.iter().map(|block| block.source.as_str()).collect();
            let expected = if answer.trim().is_empty() { String::new() } else { answer.clone() };
            proptest::prop_assert_eq!(joined, expected);
            for block in &found {
                proptest::prop_assert!(!block.source.trim().is_empty());
            }
        }
    }
}
