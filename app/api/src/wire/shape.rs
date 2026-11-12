//! A wire type as ts-rs prints it, read back as the shape the browser's decoder folds over.
//!
//! The reading is checked by the one that matters: every decoder is printed annotated with the
//! type it was read from, so `tsc` refuses a decoder that misread its type, and a misreading is
//! a build error, never a payload decoded wrong. A former ts-rs does not print for this wire is
//! refused by name rather than guessed at.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// One TypeScript type former among those ts-rs prints: the JSON values of that type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// `string`.
    Str,
    /// `number`.
    Num,
    /// `boolean`.
    Bool,
    /// `unknown`: any value, kept whole.
    Unknown,
    /// `null`.
    Null,
    /// A string literal: that string alone.
    Lit(String),
    /// An alias the wire declares, by name.
    Named(String),
    /// `Array<T>` or `T[]`.
    Array(Box<Shape>),
    /// `Record<string, T>`: any keys, each value a `T`.
    Record(Box<Shape>),
    /// `{ a: A, … }`: an object with these fields.
    Object(Vec<Field>),
    /// `A | B | …`: a value of any one.
    Union(Vec<Shape>),
    /// `A & B & …`: a value of every one, which is how ts-rs prints a flattened field.
    Both(Vec<Shape>),
}

/// A field of an [`Shape::Object`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// Its name on the wire.
    pub name: String,
    /// What its value may be.
    pub shape: Shape,
    /// `a?: A`: it may be left out.
    pub optional: bool,
}

/// Text that is not a declaration ts-rs prints, or a declaration a decoder cannot be folded from.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the wire's TypeScript: {0}")]
pub struct ShapeError(String);

fn refuse<T>(why: impl Into<String>) -> Result<T, ShapeError> {
    Err(ShapeError(why.into()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    Quoted(String),
    Punct(char),
}

/// The text's tokens, its `/* */` comments left out.
fn tokens(text: &str) -> Result<Vec<Token>, ShapeError> {
    let word = |c: &char| c.is_alphanumeric() || *c == '_' || *c == '$';
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {}
            '/' if chars.next_if_eq(&'*').is_some() => {
                let mut last = '\0';
                while !(last == '*' && chars.peek() == Some(&'/')) {
                    last = chars
                        .next()
                        .map_or_else(|| refuse("a comment never closes"), Ok)?;
                }
                chars.next();
            }
            '"' => {
                let mut quoted = String::new();
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => quoted.extend(chars.next()),
                        Some(c) => quoted.push(c),
                        None => return refuse("a string never closes"),
                    }
                }
                out.push(Token::Quoted(quoted));
            }
            c if word(&c) => {
                let mut name = String::from(c);
                while let Some(c) = chars.next_if(word) {
                    name.push(c);
                }
                out.push(Token::Word(name));
            }
            '{' | '}' | '[' | ']' | '<' | '>' | '(' | ')' | ',' | ':' | ';' | '|' | '&' | '?'
            | '=' => {
                out.push(Token::Punct(c));
            }
            c => return refuse(format!("{c:?} is in no type this wire prints")),
        }
    }
    Ok(out)
}

/// `A & (B | C)` as `(B & A) | (C & A)`: how ts-rs prints a flattened sum, read as the sum it is,
/// each arm's own fields first so its tag leads.
fn distributed(members: Vec<Shape>) -> Shape {
    let mut sums: Vec<Vec<Shape>> = Vec::new();
    let mut rest: Vec<Shape> = Vec::new();
    for member in members {
        match member {
            Shape::Union(arms) => sums.push(arms),
            one => rest.push(one),
        }
    }
    if sums.is_empty() {
        return Shape::Both(rest);
    }
    // Every choice of one arm from each sum, the arms first, then what every arm shares.
    let choices = sums.into_iter().fold(vec![Vec::new()], |chosen, arms| {
        chosen
            .iter()
            .flat_map(|before| {
                arms.iter().map(move |arm| {
                    let mut both = before.clone();
                    both.push(arm.clone());
                    both
                })
            })
            .collect::<Vec<Vec<Shape>>>()
    });
    Shape::Union(
        choices
            .into_iter()
            .map(|mut both| {
                both.extend(rest.iter().cloned());
                Shape::Both(both)
            })
            .collect(),
    )
}

/// A recursive descent over [`tokens`], one method per precedence level.
struct Parser {
    tokens: std::iter::Peekable<std::vec::IntoIter<Token>>,
}

impl Parser {
    fn eat(&mut self, c: char) -> bool {
        self.tokens.next_if_eq(&Token::Punct(c)).is_some()
    }

    fn expect(&mut self, c: char) -> Result<(), ShapeError> {
        if self.eat(c) {
            Ok(())
        } else {
            refuse(format!("expected {c:?}, found {:?}", self.tokens.peek()))
        }
    }

    fn word(&mut self) -> Result<String, ShapeError> {
        match self.tokens.next() {
            Some(Token::Word(word)) => Ok(word),
            other => refuse(format!("expected a name, found {other:?}")),
        }
    }

    /// `item (sep item)*`, one item as itself and more as `many` of them.
    fn joined(
        &mut self,
        sep: char,
        item: fn(&mut Self) -> Result<Shape, ShapeError>,
        many: fn(Vec<Shape>) -> Shape,
    ) -> Result<Shape, ShapeError> {
        let mut items = vec![item(self)?];
        while self.eat(sep) {
            items.push(item(self)?);
        }
        Ok(if items.len() == 1 {
            items.remove(0)
        } else {
            many(items)
        })
    }

    fn union(&mut self) -> Result<Shape, ShapeError> {
        self.eat('|');
        self.joined(
            '|',
            |p| p.joined('&', Self::postfix, distributed),
            Shape::Union,
        )
    }

    fn postfix(&mut self) -> Result<Shape, ShapeError> {
        let mut shape = self.primary()?;
        while self.eat('[') {
            self.expect(']')?;
            shape = Shape::Array(Box::new(shape));
        }
        Ok(shape)
    }

    fn primary(&mut self) -> Result<Shape, ShapeError> {
        match self.tokens.next() {
            Some(Token::Punct('{')) => self.object(),
            Some(Token::Punct('(')) => {
                let shape = self.union()?;
                self.expect(')')?;
                Ok(shape)
            }
            Some(Token::Quoted(text)) => Ok(Shape::Lit(text)),
            Some(Token::Word(word)) => self.named(word),
            other => refuse(format!("expected a type, found {other:?}")),
        }
    }

    fn named(&mut self, word: String) -> Result<Shape, ShapeError> {
        let mut arguments = Vec::new();
        if self.eat('<') {
            arguments.push(self.union()?);
            while self.eat(',') {
                arguments.push(self.union()?);
            }
            self.expect('>')?;
        }
        Ok(match (word.as_str(), arguments.as_slice()) {
            ("string", []) => Shape::Str,
            ("number", []) => Shape::Num,
            ("boolean", []) => Shape::Bool,
            ("unknown", []) => Shape::Unknown,
            ("null", []) => Shape::Null,
            ("Array", [element]) => Shape::Array(Box::new(element.clone())),
            ("Record", [Shape::Str, value]) => Shape::Record(Box::new(value.clone())),
            (_, []) => Shape::Named(word),
            _ => {
                return refuse(format!(
                    "{word} with arguments is in no type this wire prints"
                ));
            }
        })
    }

    fn object(&mut self) -> Result<Shape, ShapeError> {
        let mut fields = Vec::new();
        while !self.eat('}') {
            let name = match self.tokens.next() {
                Some(Token::Word(name) | Token::Quoted(name)) => name,
                other => return refuse(format!("expected a field, found {other:?}")),
            };
            let optional = self.eat('?');
            self.expect(':')?;
            let shape = self.union()?;
            fields.push(Field {
                name,
                shape,
                optional,
            });
            self.eat(',');
        }
        Ok(Shape::Object(fields))
    }
}

/// `type Name = …;`, as ts-rs's `decl` prints it, read as its name and shape.
///
/// # Errors
/// Text that is not one such declaration, or a generic one, which no one decoder decodes.
pub fn declaration(text: &str) -> Result<(String, Shape), ShapeError> {
    let mut parser = Parser {
        tokens: tokens(text)?.into_iter().peekable(),
    };
    if parser.word()? != "type" {
        return refuse(format!("not a type declaration: {text}"));
    }
    let name = parser.word()?;
    if parser.eat('<') {
        return refuse(format!(
            "{name} is generic, and a generic type has no one decoder"
        ));
    }
    parser.expect('=')?;
    let shape = parser.union()?;
    parser.eat(';');
    match parser.tokens.peek() {
        None => Ok((name, shape)),
        Some(rest) => refuse(format!("{name} is followed by {rest:?}")),
    }
}

/// The browser's decoders, one per type the `answers` reach, each `decoder.ts`'s combinator for
/// its type's former: `declarations` as ts-rs's `decl`, and `answers` the names a route answers.
/// The text is a function of the two sets, whatever order either came in: each decoder after
/// what it names, by name, so `d.lazy` marks a cycle and nothing else.
///
/// # Errors
/// A declaration [`declaration`] cannot read, an answer or a name the declarations lack, or a
/// type with no decoder: a field that may be missing (a missing field is a break) or a tagged sum
/// with no name to give `d.sum`.
pub fn decoders<'a>(
    declarations: impl IntoIterator<Item = &'a str>,
    answers: &[String],
) -> Result<String, ShapeError> {
    let declared = declarations
        .into_iter()
        .map(declaration)
        .collect::<Result<Vec<_>, _>>()?;
    let aliases: BTreeMap<&str, &Shape> = declared
        .iter()
        .map(|(name, shape)| (name.as_str(), shape))
        .collect();
    let mut fold = Fold {
        aliases,
        emitted: BTreeSet::new(),
    };
    let mut out = String::from(DECODERS_HEADER);
    let mut answers: Vec<&str> = answers.iter().map(String::as_str).collect();
    answers.sort_unstable();
    let mut seen = BTreeSet::new();
    for answer in answers {
        fold.emit(answer, &mut seen, &mut out)?;
    }
    out.push_str("}\nexport default payload\n");
    Ok(out)
}

/// Where `value`, read as the declared `name`, holds a map: at each path its keys are its data,
/// never fields, so a key left out or added is not a field missing or extra.
///
/// # Errors
/// A declaration [`declaration`] cannot read.
pub fn maps<'a>(
    declarations: impl IntoIterator<Item = &'a str>,
    name: &str,
    value: &serde_json::Value,
) -> Result<Vec<Vec<serde_json::Value>>, ShapeError> {
    let declared = declarations
        .into_iter()
        .map(declaration)
        .collect::<Result<Vec<_>, _>>()?;
    let aliases = declared
        .iter()
        .map(|(name, shape)| (name.as_str(), shape))
        .collect();
    let fold = Fold {
        aliases,
        emitted: BTreeSet::new(),
    };
    let mut out = Vec::new();
    fold.maps(
        &Shape::Named(name.to_owned()),
        value,
        &mut Vec::new(),
        &mut out,
    );
    Ok(out)
}

const DECODERS_HEADER: &str = "\
// GENERATED FROM app/api/src/wire.rs BY `cargo test -p suzatary-api`, beside wire.ts.
// DO NOT EDIT: each decoder is its type's fold (`suzatary_core::wire::shape`), so a wire
// change is one edit, in the Rust; tsc checks every decoder against the type it was read from.

import d from \"../decoder\"
import type * as w from \"./wire\"

export namespace payload {
";

/// Every alias a shape names, at any depth.
fn names<'a>(shape: &'a Shape, out: &mut Vec<&'a str>) {
    match shape {
        Shape::Named(name) => out.push(name),
        Shape::Array(inner) | Shape::Record(inner) => names(inner, out),
        Shape::Object(fields) => {
            for field in fields {
                names(&field.shape, out);
            }
        }
        Shape::Union(members) | Shape::Both(members) => {
            for member in members {
                names(member, out);
            }
        }
        Shape::Str | Shape::Num | Shape::Bool | Shape::Unknown | Shape::Null | Shape::Lit(_) => {}
    }
}

/// One arm of a tagged sum: its tag's value and its other fields.
type Arm<'a> = (&'a str, Vec<&'a Field>);

/// The algebra from a shape to its decoder's text, with the aliases it reads through.
struct Fold<'a> {
    aliases: BTreeMap<&'a str, &'a Shape>,
    emitted: BTreeSet<&'a str>,
}

impl<'a> Fold<'a> {
    /// `name`'s decoder, after those of the names it holds, each once.
    fn emit(
        &mut self,
        name: &'a str,
        seen: &mut BTreeSet<&'a str>,
        out: &mut String,
    ) -> Result<(), ShapeError> {
        let Some(shape) = self.aliases.get(name).copied() else {
            return refuse(format!("{name} is not declared"));
        };
        if !seen.insert(name) {
            return Ok(());
        }
        let mut held = Vec::new();
        names(shape, &mut held);
        held.sort_unstable();
        for inner in held {
            self.emit(inner, seen, out)?;
        }
        let decoder = self.decoder(shape, Some(name))?;
        let _ = writeln!(
            out,
            "  export const {name}: d.Decoder<w.{name}> = {decoder}"
        );
        self.emitted.insert(name);
        Ok(())
    }

    fn maps(
        &self,
        shape: &Shape,
        value: &serde_json::Value,
        at: &mut Vec<serde_json::Value>,
        out: &mut Vec<Vec<serde_json::Value>>,
    ) {
        use serde_json::Value;
        match (shape, value) {
            (Shape::Named(alias), _) => {
                if let Some(shape) = self.aliases.get(alias.as_str()) {
                    self.maps(shape, value, at, out);
                }
            }
            (Shape::Array(inner), Value::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    self.under(i.into(), inner, item, at, out);
                }
            }
            (Shape::Record(inner), Value::Object(entries)) if **inner != Shape::Unknown => {
                out.push(at.clone());
                for (key, item) in entries {
                    self.under(key.as_str().into(), inner, item, at, out);
                }
            }
            (Shape::Object(fields), Value::Object(entries)) => {
                for field in fields {
                    if let Some(item) = entries.get(&field.name) {
                        self.under(field.name.as_str().into(), &field.shape, item, at, out);
                    }
                }
            }
            (Shape::Both(members), _) => {
                for member in members {
                    self.maps(member, value, at, out);
                }
            }
            (Shape::Union(members), _) => {
                if let Some(member) = members.iter().find(|member| self.fits(member, value)) {
                    self.maps(member, value, at, out);
                }
            }
            _ => {}
        }
    }

    fn under(
        &self,
        key: serde_json::Value,
        shape: &Shape,
        value: &serde_json::Value,
        at: &mut Vec<serde_json::Value>,
        out: &mut Vec<Vec<serde_json::Value>>,
    ) {
        at.push(key);
        self.maps(shape, value, at, out);
        at.pop();
    }

    /// Whether `value` reads as `shape` at its top: the member of a union it is.
    fn fits(&self, shape: &Shape, value: &serde_json::Value) -> bool {
        use serde_json::Value;
        match (shape, value) {
            (Shape::Named(alias), _) => self
                .aliases
                .get(alias.as_str())
                .is_some_and(|shape| self.fits(shape, value)),
            (Shape::Lit(text), Value::String(s)) => text == s,
            (Shape::Object(fields), Value::Object(entries)) => fields.iter().all(|field| {
                entries
                    .get(&field.name)
                    .is_some_and(|item| literal(&field.shape).is_none_or(|text| item == text))
            }),
            (Shape::Both(members), _) => members.iter().all(|member| self.fits(member, value)),
            (Shape::Union(members), _) => members.iter().any(|member| self.fits(member, value)),
            (Shape::Null, Value::Null)
            | (Shape::Str, Value::String(_))
            | (Shape::Num, Value::Number(_))
            | (Shape::Bool, Value::Bool(_))
            | (Shape::Array(_), Value::Array(_))
            | (Shape::Record(_), Value::Object(_))
            | (Shape::Unknown, _) => true,
            _ => false,
        }
    }

    /// `name` is the alias being declared, which a combinator needs as its type argument.
    fn decoder(&self, shape: &'a Shape, name: Option<&str>) -> Result<String, ShapeError> {
        let typed = name.map_or_else(String::new, |name| format!("<w.{name}>"));
        let open = Shape::Record(Box::new(Shape::Unknown));
        Ok(match shape {
            Shape::Str => "d.str".into(),
            Shape::Num => "d.num".into(),
            Shape::Bool => "d.bool".into(),
            Shape::Unknown => "d.unknown".into(),
            Shape::Null => "d.lit(null)".into(),
            Shape::Lit(text) => format!("d.lit({})", quoted(text)),
            Shape::Named(alias) if self.emitted.contains(alias.as_str()) => alias.clone(),
            Shape::Named(alias) if self.aliases.contains_key(alias.as_str()) => {
                format!("d.lazy(() => {alias})")
            }
            Shape::Named(alias) => return refuse(format!("{alias} is not declared")),
            Shape::Array(inner) => format!("d.arr({})", self.decoder(inner, None)?),
            Shape::Record(inner) => format!("d.rec({})", self.decoder(inner, None)?),
            // A term's JSON: the named fields checked, every other kept whole.
            Shape::Both(members) if members.contains(&open) => {
                let closed = Shape::Both(members.iter().filter(|m| **m != open).cloned().collect());
                format!("d.open{typed}({})", self.object(&self.fields(&closed)?)?)
            }
            Shape::Object(_) | Shape::Both(_) => {
                format!("d.obj{typed}({})", self.object(&self.fields(shape)?)?)
            }
            Shape::Union(members) => self.union(&members.iter().collect::<Vec<_>>(), name)?,
        })
    }

    fn union(&self, members: &[&'a Shape], name: Option<&str>) -> Result<String, ShapeError> {
        let rest: Vec<&Shape> = members
            .iter()
            .copied()
            .filter(|m| **m != Shape::Null)
            .collect();
        let typed = name.map_or_else(String::new, |name| format!("<w.{name}>"));
        if rest.len() < members.len() {
            return Ok(format!("d.nullable({})", self.union(&rest, None)?));
        }
        if let [one] = rest.as_slice() {
            return self.decoder(one, None);
        }
        if let Some(texts) = rest.iter().map(|m| literal(m)).collect::<Option<Vec<_>>>() {
            // `oneOf`'s table is keyed by its type's members only when it is given that type.
            if name.is_none() {
                return refuse(format!(
                    "{texts:?} is a union of words with no name: give it an enum"
                ));
            }
            let keys: Vec<String> = texts
                .iter()
                .map(|t| format!("{}: null", quoted(t)))
                .collect();
            return Ok(format!("d.oneOf{typed}({{ {} }})", keys.join(", ")));
        }
        if let Some((tag, arms)) = self.tagged(&rest) {
            let Some(name) = name else {
                return refuse(format!("a sum tagged on {tag:?} has no name to give d.sum"));
            };
            let arms = arms
                .iter()
                .map(|(value, fields)| Ok(format!("{}: {}", key(value), self.object(fields)?)))
                .collect::<Result<Vec<_>, ShapeError>>()?;
            let tag = quoted(tag);
            return Ok(format!(
                "d.sum<w.{name}, {tag}>({tag}, {{ {} }})",
                arms.join(", ")
            ));
        }
        let mut decoders = rest.iter().rev().map(|member| self.decoder(member, None));
        let last = decoders
            .next()
            .unwrap_or_else(|| refuse("an empty union"))?;
        decoders.try_fold(last, |later, earlier| {
            Ok(format!("d.alt({}, {later})", earlier?))
        })
    }

    /// A union every member of which is an object with one field, the same in each, holding a
    /// string literal: serde's internally tagged enum, as its tag and each arm.
    fn tagged(&self, members: &[&'a Shape]) -> Option<(&'a str, Vec<Arm<'a>>)> {
        let objects = members
            .iter()
            .map(|m| self.fields(m).ok())
            .collect::<Option<Vec<_>>>()?;
        let tag = objects.first()?.first()?.name.as_str();
        let arms = objects.into_iter().map(|fields| {
            let value = literal(&fields.iter().find(|field| field.name == tag)?.shape)?;
            Some((
                value,
                fields
                    .into_iter()
                    .filter(|field| field.name != tag)
                    .collect(),
            ))
        });
        Some((tag, arms.collect::<Option<_>>()?))
    }

    /// An object type's fields, through the intersections and aliases ts-rs's flatten prints.
    fn fields(&self, shape: &'a Shape) -> Result<Vec<&'a Field>, ShapeError> {
        match shape {
            Shape::Object(fields) => Ok(fields.iter().collect()),
            Shape::Both(members) => members
                .iter()
                .map(|m| self.fields(m))
                .collect::<Result<Vec<_>, _>>()
                .map(|all| all.concat()),
            Shape::Named(alias) => match self.aliases.get(alias.as_str()) {
                Some(shape) => self.fields(shape),
                None => refuse(format!("{alias} is not declared")),
            },
            other => refuse(format!("{other:?} is not an object type")),
        }
    }

    fn object(&self, fields: &[&'a Field]) -> Result<String, ShapeError> {
        if let Some(field) = fields.iter().find(|field| field.optional) {
            return refuse(format!(
                "{} may be missing, and a missing field is a break",
                field.name
            ));
        }
        let fields = fields
            .iter()
            .map(|field| {
                Ok(format!(
                    "{}: {}",
                    key(&field.name),
                    self.decoder(&field.shape, None)?
                ))
            })
            .collect::<Result<Vec<_>, ShapeError>>()?;
        Ok(if fields.is_empty() {
            "{}".into()
        } else {
            format!("{{ {} }}", fields.join(", "))
        })
    }
}

fn literal(shape: &Shape) -> Option<&str> {
    match shape {
        Shape::Lit(text) => Some(text),
        _ => None,
    }
}

fn quoted(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_default()
}

fn key(name: &str) -> String {
    let mut chars = name.chars();
    let bare = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if bare { name.to_owned() } else { quoted(name) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(declarations: &[&str], answers: &[&str]) -> Result<String, ShapeError> {
        let answers: Vec<String> = answers.iter().map(|name| (*name).to_owned()).collect();
        decoders(declarations.iter().copied(), &answers)
    }

    fn line<'t>(text: &'t str, name: &str) -> &'t str {
        let start = format!("  export const {name}: d.Decoder<w.{name}> = ");
        text.lines()
            .find_map(|line| line.strip_prefix(start.as_str()))
            .unwrap_or_else(|| panic!("{name} has no decoder in {text}"))
    }

    /// Every former ts-rs prints reads as its shape, comments and trailing commas left out.
    #[test]
    fn a_declaration_reads_as_its_shape() {
        let (name, shape) = declaration(
            "type T = { /** a doc */ a: string | null, \"b\": Array<number>, \
             c: Record<string, boolean[]>, f?: U, }",
        )
        .expect("reads");
        assert_eq!(name, "T");
        let field = |name: &str, shape, optional| Field {
            name: name.into(),
            shape,
            optional,
        };
        let boxed = Box::new;
        assert_eq!(
            shape,
            Shape::Object(vec![
                field("a", Shape::Union(vec![Shape::Str, Shape::Null]), false),
                field("b", Shape::Array(boxed(Shape::Num)), false),
                field(
                    "c",
                    Shape::Record(boxed(Shape::Array(boxed(Shape::Bool)))),
                    false
                ),
                field("f", Shape::Named("U".into()), true),
            ])
        );
        assert!(
            declaration("type P = [string, number];").is_err(),
            "a tuple is refused by name"
        );
        assert!(
            declaration("type G<T> = { a: T };").is_err(),
            "a generic type is refused"
        );
        assert!(
            declaration("type M = Map<string, number>;").is_err(),
            "an undeclared former is refused"
        );
    }

    /// A sum tagged inline or by a flattened alias is `d.sum` over each arm's other fields; any
    /// other union is `d.alt`, `null` is `d.nullable`, and literals alone are `d.oneOf`.
    #[test]
    fn each_former_folds_to_its_combinator() {
        let text = decoded(
            &[
                "type Inner = { x: number, };",
                "type Tag = { \"kind\": \"a\", y: string, } | { \"kind\": \"b\" } & Inner | { \"kind\": \"c\" };",
                "type Ext = { \"value\": number } | \"absent\";",
                "type Kind = \"\" | \"open\";",
                "type Node = { kind: string } & Record<string, unknown>;",
                "type Top = { tag: Tag | null, ext: Ext, kind: Kind, node: Node, };",
            ],
            &["Top"],
        )
        .expect("folds");
        assert_eq!(
            line(&text, "Tag"),
            "d.sum<w.Tag, \"kind\">(\"kind\", { a: { y: d.str }, b: { x: d.num }, c: {} })"
        );
        assert_eq!(
            line(&text, "Ext"),
            "d.alt(d.obj({ value: d.num }), d.lit(\"absent\"))"
        );
        assert_eq!(
            line(&text, "Kind"),
            "d.oneOf<w.Kind>({ \"\": null, \"open\": null })"
        );
        assert_eq!(line(&text, "Node"), "d.open<w.Node>({ kind: d.str })");
        assert_eq!(
            line(&text, "Top"),
            "d.obj<w.Top>({ tag: d.nullable(Tag), ext: Ext, kind: Kind, node: Node })"
        );
    }

    /// A sum flattened into a record, as ts-rs prints it (`{ shared } & (arm | arm)`), is read as
    /// the sum of each arm with the shared fields, so it decodes by its tag like any other sum.
    #[test]
    fn a_flattened_sum_is_a_sum_of_its_arms_with_the_shared_fields() {
        let text = decoded(
            &[
                "type Row = { index: number, } & ({ \"who\": \"person\", queued: boolean, } | { \"who\": \"model\" });",
                "type Top = { rows: Array<Row>, };",
            ],
            &["Top"],
        )
        .expect("folds");
        assert_eq!(
            line(&text, "Row"),
            "d.sum<w.Row, \"who\">(\"who\", { person: { queued: d.bool, index: d.num }, model: { index: d.num } })"
        );
    }

    /// Only what an answer reaches has a decoder, each after what it names, so only a cycle is
    /// read lazily, and the text is the same whatever order the declarations and answers came in.
    #[test]
    fn the_answers_reach_their_decoders_and_only_a_cycle_is_lazy() {
        let declarations = [
            "type Tree = { kids: Array<Tree>, leaf: Leaf, };",
            "type Leaf = { n: number, };",
            "type Body = { id?: string, };",
            "type Top = { tree: Tree, leaf: Leaf, };",
        ];
        let text = decoded(&declarations, &["Top", "Tree"]).expect("folds");
        assert_eq!(
            line(&text, "Tree"),
            "d.obj<w.Tree>({ kids: d.arr(d.lazy(() => Tree)), leaf: Leaf })"
        );
        assert_eq!(line(&text, "Leaf"), "d.obj<w.Leaf>({ n: d.num })");
        assert!(
            !text.contains("Body"),
            "a body no answer reaches has no decoder"
        );
        let mut reversed = declarations;
        reversed.reverse();
        assert_eq!(decoded(&reversed, &["Tree", "Top"]).expect("folds"), text);
    }

    /// A union of words is decoded only under its own name, so a member the decoder dropped
    /// would not compile.
    #[test]
    fn a_union_of_words_with_no_name_is_refused() {
        let refused =
            decoded(&["type A = { kind: \"x\" | \"y\", };"], &["A"]).expect_err("refused");
        assert!(refused.to_string().contains("give it an enum"), "{refused}");
    }

    /// A map is found through arrays, fields and the arm of a sum the value is, and the open
    /// record of a term's JSON is no map.
    #[test]
    fn a_value_names_its_maps() {
        let declarations = [
            "type Node = { kind: string } & Record<string, unknown>;",
            "type Row = { by: Record<string, string[]>, spec: Node, };",
            "type Arm = { \"kind\": \"a\", rows: Array<Row>, } | { \"kind\": \"b\", other: Record<string, number>, };",
        ];
        let value = serde_json::json!({
            "kind": "a",
            "rows": [{ "by": { "state": ["x"] }, "spec": { "kind": "flat", "value": 1 } }],
        });
        let found = maps(declarations, "Arm", &value).expect("reads");
        assert_eq!(
            found,
            vec![vec![
                serde_json::json!("rows"),
                serde_json::json!(0),
                serde_json::json!("by")
            ]]
        );
    }

    /// A field that may be missing has no decoder: a missing field is a break.
    #[test]
    fn an_answer_with_an_optional_field_is_refused() {
        let refused = decoded(&["type A = { id?: string, };"], &["A"]).expect_err("refused");
        assert!(
            refused.to_string().contains("id may be missing"),
            "{refused}"
        );
        assert!(
            decoded(&["type A = { b: B, };"], &["A"]).is_err(),
            "an undeclared name is refused"
        );
    }
}
