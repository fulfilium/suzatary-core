//! A Typst template: `.typ` files called with one value of a Rust type, the value read through
//! the type's generated schema ([`schema`]), and every output kept under the name of the code and
//! the data that made it.
//!
//! A template's function is named by every byte that produced its output: the template's files,
//! the schema generated from its type, the typesetter `app/Cargo.lock` links, and the code that
//! turns a compiled document into what is kept. Its argument is named by the value's JSON. An edit
//! to a template misses every output it made, and nothing else moves a key.

pub mod example;
pub mod schema;

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::OnceLock;

use prodrome_core::event::Hash;
use prodrome_core::memo::{self, Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The library a compile sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Library {
    /// typst's library without `html` and `plugin`: for a template that evaluates markup a model
    /// or a writer typed.
    Closed,
    /// typst's whole library: for a template whose every datum is data.
    Open,
}

/// One compile: the files of its world by key, the one compiled, and the value it reads as
/// `/state.json`. A world holds these files and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    /// The library.
    pub library: Library,
    /// The key of the file compiled.
    pub main: String,
    /// Every file, by key.
    pub files: BTreeMap<String, String>,
    /// The value, as JSON.
    pub state: String,
}

/// The key a lone text is compiled under.
pub(crate) const TEXT: &str = "/main.typ";

impl Job {
    /// One text alone in the closed world: what the markup door compiles.
    #[must_use]
    pub fn text(source: &str) -> Self {
        Self {
            library: Library::Closed,
            main: TEXT.to_owned(),
            files: BTreeMap::from([(TEXT.to_owned(), source.to_owned())]),
            state: "{}".to_owned(),
        }
    }

    /// The text of the file compiled.
    #[must_use]
    pub fn main_text(&self) -> &str {
        self.files.get(&self.main).map_or("", String::as_str)
    }
}

/// What is kept of a compiled document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// Its body, past the tripwire ([`crate::narrowing`]).
    Body,
    /// The whole document, past the tripwire.
    Document,
}

/// Where an output is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    /// In the process and on the box's disk, and on a device: a body or an answer.
    Disk,
    /// In the process only: a page printed from values that move with the clock.
    Process,
    /// Nowhere: a page written once to a file.
    Nowhere,
}

/// A template called with values of `T`.
#[derive(Debug)]
pub struct Template<T> {
    main: &'static str,
    files: &'static [(&'static str, &'static str)],
    library: Library,
    output: Output,
    kept: Kept,
    also: &'static [&'static str],
    schema: OnceLock<schema::Schema>,
    function: OnceLock<Hash>,
    value: PhantomData<fn(&T)>,
}

/// Printing a template's value as JSON failed: a type serde cannot print as JSON.
#[derive(Debug, Clone, thiserror::Error)]
#[error("the template's value does not print as JSON: {0}")]
pub struct Unprintable(String);

impl<T: Serialize + JsonSchema> Template<T> {
    /// `files` by key (`/answer.typ`), `main` among them. `also` is any other code the output is
    /// a function of, named with the files.
    #[must_use]
    pub const fn new(
        main: &'static str,
        files: &'static [(&'static str, &'static str)],
        library: Library,
        output: Output,
        kept: Kept,
        also: &'static [&'static str],
    ) -> Self {
        Self {
            main,
            files,
            library,
            output,
            kept,
            also,
            schema: OnceLock::new(),
            function: OnceLock::new(),
            value: PhantomData,
        }
    }

    /// The schema generated from `T`.
    pub fn schema(&self) -> &schema::Schema {
        self.schema.get_or_init(schema::of::<T>)
    }

    /// Every file of the template's world but the value: its own and its schema's.
    #[must_use]
    pub fn world(&self) -> BTreeMap<String, String> {
        self.files
            .iter()
            .map(|(key, text)| ((*key).to_owned(), (*text).to_owned()))
            .chain(self.schema().modules.clone())
            .collect()
    }

    /// The function's name: every file of its world, the typesetter, what is kept of a
    /// document, and `also`.
    pub fn function(&self) -> &Hash {
        self.function.get_or_init(|| {
            let files: Vec<Hash> = self
                .world()
                .iter()
                .map(|(key, text)| named(&format!("{key}\n{text}")))
                .collect();
            let kept = match self.output {
                Output::Body => &*crate::rendering::NARROWING,
                Output::Document => &*crate::rendering::TRIPWIRE,
            };
            let also: Vec<Hash> = self.also.iter().map(|text| named(text)).collect();
            memo::name(
                self.main.as_bytes(),
                [&*crate::rendering::TYPESETTER, kept]
                    .into_iter()
                    .chain(&files)
                    .chain(&also),
            )
        })
    }

    /// The template called with `value`.
    ///
    /// # Errors
    /// `value` does not print as JSON.
    pub fn call(&self, value: &T) -> Result<Called, Unprintable> {
        let state = state(value)?;
        Ok(Called {
            key: self.keyed(&state),
            job: Job {
                library: self.library,
                main: self.main.to_owned(),
                files: self.world(),
                state,
            },
            output: self.output,
            kept: self.kept,
        })
    }

    /// The key `value`'s output is kept under.
    ///
    /// # Errors
    /// `value` does not print as JSON.
    pub fn key_of(&self, value: &T) -> Result<Key, Unprintable> {
        Ok(self.keyed(&state(value)?))
    }

    /// The key `value`'s output is kept under, printed as the box sends it beside its HTML.
    ///
    /// # Errors
    /// `value` does not print as JSON.
    pub fn key(&self, value: &T) -> Result<String, Unprintable> {
        self.key_of(value).map(|key| printed(&key))
    }

    fn keyed(&self, state: &str) -> Key {
        Key {
            function: self.function().clone(),
            argument: named(state),
        }
    }
}

fn state<T: Serialize>(value: &T) -> Result<String, Unprintable> {
    serde_json::to_string(value).map_err(|e| Unprintable(e.to_string()))
}

/// A template called with a value: what to compile, what to keep of it, and the key it is kept
/// under. Only [`Template::call`] makes one, so a route cannot hand a renderer Typst it assembled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Called {
    key: Key,
    job: Job,
    output: Output,
    kept: Kept,
}

impl Called {
    /// The key its output is kept under.
    #[must_use]
    pub fn key(&self) -> &Key {
        &self.key
    }

    /// What to compile.
    #[must_use]
    pub fn job(&self) -> &Job {
        &self.job
    }

    /// What is kept of the document.
    #[must_use]
    pub fn output(&self) -> Output {
        self.output
    }

    /// Where its output is kept.
    #[must_use]
    pub fn kept(&self) -> Kept {
        self.kept
    }
}

/// A key as both tables print one: the function's name, a space, the argument's.
#[must_use]
pub fn printed(key: &Key) -> String {
    format!("{} {}", key.function.as_str(), key.argument.as_str())
}

/// A text named by its bytes, as a memo node is.
#[must_use]
pub fn named(text: &str) -> Hash {
    memo::name(text.as_bytes(), [])
}

/// A template whatever its value's type: what the build checks of every one.
pub trait Checked: Sync {
    /// The key of its main file.
    fn main(&self) -> &'static str;

    /// It called with each example of its type (`schema::Schema::examples`).
    fn examples(&self) -> Vec<Job>;
}

impl<T: Serialize + JsonSchema> Checked for Template<T> {
    fn main(&self) -> &'static str {
        self.main
    }

    fn examples(&self) -> Vec<Job> {
        self.schema()
            .examples
            .iter()
            .map(|example| Job {
                library: self.library,
                main: self.main.to_owned(),
                files: self.world(),
                state: example.to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[derive(Serialize, JsonSchema)]
    struct Said {
        words: String,
    }

    fn template(text: &str) -> Template<Said> {
        let text: &'static str = Box::leak(text.to_owned().into_boxed_str());
        let files: &'static [(&'static str, &'static str)] =
            Box::leak(vec![("/said.typ", text)].into_boxed_slice());
        Template::new(
            "/said.typ",
            files,
            Library::Closed,
            Output::Body,
            Kept::Disk,
            &[],
        )
    }

    fn key(template: &Template<Said>, words: &str) -> Key {
        template
            .key_of(&Said {
                words: words.to_owned(),
            })
            .expect("prints")
    }

    proptest! {
        /// A template's text names every key it makes, and the value names the rest: changing
        /// the text changes every key's function, and nothing else does.
        #[test]
        fn changing_a_templates_text_changes_every_key_and_nothing_else_does(
            a in ".{0,40}",
            b in ".{0,40}",
            x in ".{0,20}",
            y in ".{0,20}",
        ) {
            let (one, other, again) = (template(&a), template(&b), template(&a));
            prop_assert_eq!(key(&one, &x), key(&again, &x));
            prop_assert_eq!(key(&one, &x).function == key(&other, &y).function, a == b);
            prop_assert_eq!(key(&one, &x).argument == key(&other, &y).argument, x == y);
        }
    }
}
