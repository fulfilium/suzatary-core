//! What a dependent reads of a source file instead of the file: its public items' signatures and
//! doc comments, as rustdoc would show them.
//!
//! The reading is by lines, not by a parser, so its count is an estimate: it takes every `pub`
//! item whatever its module's visibility, and it cuts a signature at its first `{`.

/// The production part of a Rust file: everything before its first line `#[cfg(test)]`, the rule
/// `scripts/size.sh` counts by.
#[must_use]
pub fn production(source: &str) -> &str {
    source
        .match_indices("#[cfg(test)]")
        .find(|(at, _)| *at == 0 || source.as_bytes()[at - 1] == b'\n')
        .map_or(source, |(at, _)| &source[..at])
}

/// The tests of a Rust file: everything from its first line `#[cfg(test)]`.
#[must_use]
pub fn tests(source: &str) -> &str {
    &source[production(source).len()..]
}

/// The public interface of a Rust file: module docs, and each `pub` item with its docs and
/// attributes; a function's signature without its body, a struct's, an enum's or a trait's body
/// without its methods' bodies.
#[must_use]
pub fn rust(source: &str) -> String {
    let mut out = Vec::new();
    let mut pending = Vec::new();
    let mut lines = production(source).lines();
    while let Some(line) = lines.next() {
        let item = line.trim_start();
        if item.starts_with("//!") {
            out.push(line);
        } else if item.starts_with("///") || item.starts_with("#[") {
            pending.push(line);
        } else if item.starts_with("pub ") {
            out.append(&mut pending);
            let header = keyword(item);
            match header {
                Header::Signature => signature(line, &mut lines, &mut out),
                Header::Body { methods } => body(line, &mut lines, &mut out, methods),
                Header::Line => out.push(line),
            }
        } else {
            pending.clear();
        }
    }
    joined(&out)
}

/// The exported declarations of a TypeScript module, each with the comment above it: a
/// function's signature, a type's whole text, any other export's first line.
#[must_use]
pub fn typescript(source: &str) -> String {
    let mut out = Vec::new();
    let mut pending = Vec::new();
    let mut lines = source.lines().peekable();
    while let Some(line) = lines.next() {
        let item = line.trim_start();
        if item.starts_with("//") || item.starts_with("/*") || item.starts_with('*') {
            pending.push(line);
        } else if let Some(rest) = item.strip_prefix("export ") {
            out.append(&mut pending);
            let declared = rest.strip_prefix("declare ").unwrap_or(rest);
            if declared.starts_with("function ") || declared.starts_with("async function ") {
                signature(line, &mut lines, &mut out);
            } else if declared.starts_with("type ") || declared.starts_with("interface ") {
                statement(line, &mut lines, &mut out);
            } else {
                out.push(line);
            }
        } else {
            pending.clear();
        }
    }
    joined(&out)
}

/// The public definitions of a Typst file: each `#let` whose name does not start with `_`, its
/// first line and the comment above it.
#[must_use]
pub fn typst(source: &str) -> String {
    let mut out = Vec::new();
    let mut pending = Vec::new();
    for line in source.lines() {
        if line.starts_with("//") {
            pending.push(line);
        } else if line.starts_with("#let ") && !line.starts_with("#let _") {
            out.append(&mut pending);
            out.push(line);
        } else {
            pending.clear();
        }
    }
    joined(&out)
}

/// How much of a public item a reader sees.
enum Header {
    /// Up to the body: a function.
    Signature,
    /// The braces' whole contents: a struct, an enum, a union or a trait.
    Body {
        /// Whether a method's body inside it is shown.
        methods: Methods,
    },
    /// The first line: a module, a use, a constant, a type alias.
    Line,
}

/// Whether the functions inside a body are shown whole.
#[derive(Clone, Copy)]
enum Methods {
    /// A trait's default methods: only their signatures.
    Hidden,
    /// A struct's fields and an enum's variants, whose nested braces are data.
    Shown,
}

/// The header of a `pub` item, by its keyword.
fn keyword(item: &str) -> Header {
    let words = item
        .split_whitespace()
        .skip(1)
        .find(|word| !matches!(*word, "const" | "async" | "unsafe" | "extern" | "\"C\""));
    match words {
        Some("fn") => Header::Signature,
        Some("trait") => Header::Body {
            methods: Methods::Hidden,
        },
        Some("struct" | "enum" | "union") => Header::Body {
            methods: Methods::Shown,
        },
        _ => {
            if item.contains(" fn ") {
                Header::Signature
            } else {
                Header::Line
            }
        }
    }
}

/// A formatted statement: `first` and each line after it that is indented deeper, or that
/// closes or continues it at the same indent (`}`, `)`, `|`, `&`).
fn statement<'a>(
    first: &'a str,
    lines: &mut std::iter::Peekable<std::str::Lines<'a>>,
    out: &mut Vec<&'a str>,
) {
    let indent = |line: &str| line.len() - line.trim_start().len();
    out.push(first);
    while let Some(next) = lines.next_if(|next| {
        let rest = next.trim_start();
        !rest.is_empty()
            && (indent(next) > indent(first)
                || (indent(next) == indent(first) && rest.starts_with(['}', ')', '|', '&'])))
    }) {
        out.push(next);
    }
}

/// A signature: `first` and the lines after it up to the one that opens the body or ends the
/// item, the body itself skipped.
fn signature<'a>(
    first: &'a str,
    lines: &mut impl Iterator<Item = &'a str>,
    out: &mut Vec<&'a str>,
) {
    let mut line = first;
    loop {
        if let Some(open) = outside_strings(line).find('{') {
            out.push(line[..open].trim_end());
            skip_block(braces(&line[open..]), lines);
            return;
        }
        out.push(line);
        if line.trim_end().ends_with(';') {
            return;
        }
        match lines.next() {
            Some(next) => line = next,
            None => return,
        }
    }
}

/// A body: `first` and every line up to its closing brace; with [`Methods::Hidden`], the lines
/// nested deeper than the body's own items are left out.
fn body<'a>(
    first: &'a str,
    lines: &mut impl Iterator<Item = &'a str>,
    out: &mut Vec<&'a str>,
    methods: Methods,
) {
    out.push(first);
    let mut depth = braces(first);
    let mut opened = depth > 0 || outside_strings(first).contains('{');
    if first.trim_end().ends_with(';') && !opened {
        return;
    }
    for line in lines.by_ref() {
        let before = depth;
        depth += braces(line);
        opened = opened || outside_strings(line).contains('{');
        match methods {
            Methods::Shown => out.push(line),
            Methods::Hidden if before <= 1 => out.push(line),
            Methods::Hidden => {}
        }
        if opened && depth <= 0 {
            return;
        }
        if !opened && line.trim_end().ends_with(';') {
            return;
        }
    }
}

/// Skip the lines of a block whose opening line left `depth` braces open.
fn skip_block<'a>(mut depth: i64, lines: &mut impl Iterator<Item = &'a str>) {
    while depth > 0 {
        match lines.next() {
            Some(line) => depth += braces(line),
            None => return,
        }
    }
}

/// The braces a line opens minus those it closes, outside strings, characters and comments.
fn braces(line: &str) -> i64 {
    outside_strings(line)
        .chars()
        .map(|c| match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        })
        .sum()
}

/// A line with its string and character literals blanked and its trailing comment cut, so a
/// brace inside one is not counted.
fn outside_strings(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' | '`' => {
                let mut escaped = false;
                for inner in chars.by_ref() {
                    if !escaped && inner == c {
                        break;
                    }
                    escaped = !escaped && inner == '\\';
                }
                out.push(' ');
            }
            '\'' => {
                // A character literal is `'x'` or an escape such as `'\u{10ffff}'`; a lifetime has no
                // closing quote, and is kept.
                let ahead: Vec<char> = chars.clone().take(11).collect();
                let length = match ahead.as_slice() {
                    ['\\', rest @ ..] => rest.iter().position(|c| *c == '\'').map(|end| end + 2),
                    [_, '\'', ..] => Some(2),
                    _ => None,
                };
                for _ in 0..length.unwrap_or(0) {
                    chars.next();
                }
                out.push(' ');
            }
            '/' if chars.peek() == Some(&'/') => break,
            _ => out.push(c),
        }
    }
    out
}

fn joined(lines: &[&str]) -> String {
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One reading of each item's shape: what a dependent sees, and that a body never shows.
    #[test]
    fn a_public_item_shows_its_signature_and_docs_and_hides_its_body() {
        let source = r#"//! The crate.
use std::fmt;

/// A thing.
#[derive(Debug)]
pub struct Thing {
    /// Its name.
    pub name: String,
}

/// Make one.
pub fn make(name: &str) -> Thing {
    let secret = "{ not a brace";
    Thing { name: name.to_owned() }
}

fn private() {}

/// What a thing does.
pub trait Does {
    /// Do it.
    fn does(&self) -> u8 {
        let hidden = 1;
        hidden
    }
}

impl Thing {
    /// Its length.
    pub const fn len(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    pub fn not_shown() {}
}
"#;
        let shown = rust(source);
        for kept in [
            "//! The crate.",
            "/// A thing.",
            "pub name: String,",
            "pub fn make(name: &str) -> Thing",
            "fn does(&self) -> u8",
            "pub const fn len(&self) -> usize",
        ] {
            assert!(shown.contains(kept), "{kept:?} missing from:\n{shown}");
        }
        for hidden in ["secret", "private", "hidden", "not_shown", "use std", "0\n"] {
            assert!(!shown.contains(hidden), "{hidden:?} shown in:\n{shown}");
        }
    }

    #[test]
    fn a_typescript_export_shows_its_signature_and_its_type() {
        let source = "import { x } from \"./x\"\n\n// Doubles.\nexport function double(n: number): number {\n  return n * 2\n}\n\nconst hidden = 1\n\nexport type T =\n  | { type: \"A\" }\n  | { type: \"B\" }\n";
        let shown = typescript(source);
        assert!(shown.contains("// Doubles.\nexport function double(n: number): number"));
        assert!(shown.contains("| { type: \"B\" }"), "{shown}");
        assert!(
            !shown.contains("return") && !shown.contains("hidden") && !shown.contains("import")
        );
    }
}
