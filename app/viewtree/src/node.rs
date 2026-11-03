//! The view's vocabulary: elements of a closed set of tags, attributes of a closed set of names,
//! text, keyed children, and foreign parts another painter draws.
//!
//! A node is generic over its id: a view builds `Node<M>` (no ids), and the painter keeps the
//! same tree as `Node<M, u32>`, each node with the id the DOM knows it by. So the tree a view
//! answers and the tree the painter remembers are one type, and nothing translates between them.

use std::collections::BTreeSet;

/// What an element may be. Closed, so a view cannot make a `script`, an `iframe` or a `style`.
/// Its number on the wire is its place here, the index of the painter's table of tag names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Tag {
    /// `div`.
    Div,
    /// `section`.
    Section,
    /// `h2`.
    H2,
    /// `h3`.
    H3,
    /// `p`.
    P,
    /// `a`, whose `href` the painter checks.
    A,
    /// `button`.
    Button,
    /// `input`.
    Input,
    /// `details`.
    Details,
    /// `summary`.
    Summary,
    /// `code`.
    Code,
    /// `span`.
    Span,
    /// `h1`.
    H1,
    /// `ul`.
    Ul,
    /// `li`.
    Li,
    /// `dl`.
    Dl,
    /// `dt`.
    Dt,
    /// `dd`.
    Dd,
    /// `strong`.
    Strong,
    /// `form`, whose submit the painter reports and never performs.
    Form,
}

impl Tag {
    /// Every tag, in the order of their numbers.
    pub const ALL: [Tag; 20] = [
        Tag::Div,
        Tag::Section,
        Tag::H2,
        Tag::H3,
        Tag::P,
        Tag::A,
        Tag::Button,
        Tag::Input,
        Tag::Details,
        Tag::Summary,
        Tag::Code,
        Tag::Span,
        Tag::H1,
        Tag::Ul,
        Tag::Li,
        Tag::Dl,
        Tag::Dt,
        Tag::Dd,
        Tag::Strong,
        Tag::Form,
    ];

    /// Its number on the wire.
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// The tag its number names, if any.
    #[must_use]
    pub fn of(code: u8) -> Option<Tag> {
        Self::ALL.get(usize::from(code)).copied()
    }

    /// Its name in HTML.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Tag::Div => "div",
            Tag::Section => "section",
            Tag::H2 => "h2",
            Tag::H3 => "h3",
            Tag::P => "p",
            Tag::A => "a",
            Tag::Button => "button",
            Tag::Input => "input",
            Tag::Details => "details",
            Tag::Summary => "summary",
            Tag::Code => "code",
            Tag::Span => "span",
            Tag::H1 => "h1",
            Tag::Ul => "ul",
            Tag::Li => "li",
            Tag::Dl => "dl",
            Tag::Dt => "dt",
            Tag::Dd => "dd",
            Tag::Strong => "strong",
            Tag::Form => "form",
        }
    }
}

/// An attribute's name. Closed, so a view cannot set a handler (`onclick`) or a `style`; a
/// `data-` attribute names its suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Attr {
    /// `class`.
    Class,
    /// `href`: an address inside the app (`#…`), or an `https:`, `http:` or `mailto:` one, which
    /// the painter opens in a new tab with no handle back on this one; any other it drops.
    Href,
    /// `title`, the words a pointer shows.
    Title,
    /// `role`.
    Role,
    /// `aria-current`.
    AriaCurrent,
    /// `enterkeyhint`.
    EnterKeyHint,
    /// `open`, present or absent: a `details` drawn open. A person opening one changes the DOM
    /// and not the view, so the painter sets it only when the view's own value moves.
    Open,
    /// `hidden`, present or absent.
    Hidden,
    /// `type`.
    Type,
    /// `placeholder`.
    Placeholder,
    /// `aria-label`.
    AriaLabel,
    /// `autocomplete`.
    Autocomplete,
    /// `autocapitalize`.
    Autocapitalize,
    /// `spellcheck`.
    Spellcheck,
    /// `disabled`, present or absent.
    Disabled,
    /// An input's value: the painter sets the property, and only when the field holds another.
    Value,
    /// `data-<suffix>`.
    Data(&'static str),
}

impl Attr {
    /// Its name on the wire, as the DOM spells it.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Attr::Class => "class".to_owned(),
            Attr::Href => "href".to_owned(),
            Attr::Title => "title".to_owned(),
            Attr::Role => "role".to_owned(),
            Attr::AriaCurrent => "aria-current".to_owned(),
            Attr::EnterKeyHint => "enterkeyhint".to_owned(),
            Attr::Open => "open".to_owned(),
            Attr::Hidden => "hidden".to_owned(),
            Attr::Type => "type".to_owned(),
            Attr::Placeholder => "placeholder".to_owned(),
            Attr::AriaLabel => "aria-label".to_owned(),
            Attr::Autocomplete => "autocomplete".to_owned(),
            Attr::Autocapitalize => "autocapitalize".to_owned(),
            Attr::Spellcheck => "spellcheck".to_owned(),
            Attr::Disabled => "disabled".to_owned(),
            Attr::Value => "value".to_owned(),
            Attr::Data(suffix) => format!("data-{suffix}"),
        }
    }
}

/// What the painter listens for on an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum On {
    /// A click or a tap.
    Click,
    /// A field's words changed; the painter reports the field's value.
    Input,
    /// A form was submitted (Enter in its field); the painter performs nothing and reports no words.
    Submit,
}

impl On {
    /// Its number on the wire.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            On::Click => 0,
            On::Input => 1,
            On::Submit => 2,
        }
    }

    /// The event its number names, if any.
    #[must_use]
    pub const fn of(code: u8) -> Option<On> {
        match code {
            0 => Some(On::Click),
            1 => Some(On::Input),
            2 => Some(On::Submit),
            _ => None,
        }
    }
}

/// A handler as data: the message a click or a submit sends, or the function from a field's words
/// to one.
#[derive(Debug, Clone)]
pub enum Listener<M> {
    /// A click sends this message.
    Click(M),
    /// A form's submit sends this message.
    Submit(M),
    /// The field's words, as typed, make this message.
    Input(fn(String) -> M),
}

impl<M> Listener<M> {
    /// What it listens for.
    #[must_use]
    pub const fn on(&self) -> On {
        match self {
            Listener::Click(_) => On::Click,
            Listener::Submit(_) => On::Submit,
            Listener::Input(_) => On::Input,
        }
    }
}

/// A part another painter draws (a host's card, today), named by a kind and a key; `props` is
/// all it is told, and a change of `props` is the only change it is told about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Foreign {
    /// Which of the host's parts.
    pub kind: &'static str,
    /// Which one, stable across draws.
    pub key: String,
    /// What it is drawn from.
    pub props: String,
}

/// An element: its tag, its attributes, its handlers, and its children.
#[derive(Debug, Clone)]
pub struct Element<M, I = ()> {
    /// The id the DOM knows it by; `()` in a view.
    pub id: I,
    /// What it is.
    pub tag: Tag,
    /// Its attributes, each name once (the builder keeps the last).
    pub attrs: Vec<(Attr, String)>,
    /// Its handlers, each event once (the builder keeps the last).
    pub on: Vec<Listener<M>>,
    /// Its children.
    pub kids: Kids<M, I>,
}

/// An element's children: by position, or by key, so a list keeps each row's node (and its
/// focus and scroll) when rows move.
#[derive(Debug, Clone)]
pub enum Kids<M, I = ()> {
    /// Matched by position.
    Plain(Vec<Node<M, I>>),
    /// Matched by key; every key once.
    Keyed(Vec<(String, Node<M, I>)>),
}

/// One node of the view.
#[derive(Debug, Clone)]
pub enum Node<M, I = ()> {
    /// Text, set as text and never parsed.
    Text(I, String),
    /// An element.
    Element(Box<Element<M, I>>),
    /// A part another painter draws.
    Foreign(I, Foreign),
}

/// Text.
pub fn text<M>(s: impl Into<String>) -> Node<M> {
    Node::Text((), s.into())
}

/// A foreign part.
pub fn foreign<M>(kind: &'static str, key: impl Into<String>, props: impl Into<String>) -> Node<M> {
    Node::Foreign(
        (),
        Foreign {
            kind,
            key: key.into(),
            props: props.into(),
        },
    )
}

/// An element of `tag`, built by the methods below and finished by [`El::node`].
#[must_use]
pub fn el<M>(tag: Tag) -> El<M> {
    El(Element {
        id: (),
        tag,
        attrs: Vec::new(),
        on: Vec::new(),
        kids: Kids::Plain(Vec::new()),
    })
}

/// An element being built.
#[derive(Debug)]
pub struct El<M>(Element<M>);

/// What an attribute is set to: words, or for a present-or-absent attribute (`disabled`) a bool,
/// whose `false` leaves it out.
pub trait AttrValue {
    /// The attribute's text, or `None` to leave it out.
    fn text(self) -> Option<String>;
}

impl AttrValue for &str {
    fn text(self) -> Option<String> {
        Some(self.to_owned())
    }
}

impl AttrValue for String {
    fn text(self) -> Option<String> {
        Some(self)
    }
}

impl AttrValue for &String {
    fn text(self) -> Option<String> {
        Some(self.clone())
    }
}

impl<V: AttrValue> AttrValue for Option<V> {
    fn text(self) -> Option<String> {
        self.and_then(AttrValue::text)
    }
}

impl AttrValue for bool {
    fn text(self) -> Option<String> {
        self.then(String::new)
    }
}

/// What may stand among an element's children: a node, words, or none, one or many of them.
pub trait Child<M> {
    /// Appends itself to `to`.
    fn add_to(self, to: &mut Vec<Node<M>>);
}

impl<M> Child<M> for Node<M> {
    fn add_to(self, to: &mut Vec<Node<M>>) {
        to.push(self);
    }
}

impl<M> Child<M> for &str {
    fn add_to(self, to: &mut Vec<Node<M>>) {
        to.push(text(self));
    }
}

impl<M> Child<M> for String {
    fn add_to(self, to: &mut Vec<Node<M>>) {
        to.push(text(self));
    }
}

impl<M, C: Child<M>> Child<M> for Option<C> {
    fn add_to(self, to: &mut Vec<Node<M>>) {
        if let Some(c) = self {
            c.add_to(to);
        }
    }
}

impl<M, C: Child<M>> Child<M> for Vec<C> {
    fn add_to(self, to: &mut Vec<Node<M>>) {
        for c in self {
            c.add_to(to);
        }
    }
}

impl<M> El<M> {
    /// Sets an attribute, replacing one of the same name; a `false` removes it.
    #[must_use]
    pub fn set(mut self, a: Attr, value: impl AttrValue) -> Self {
        self.0.attrs.retain(|(b, _)| *b != a);
        if let Some(v) = value.text() {
            self.0.attrs.push((a, v));
        }
        self
    }

    /// A handler, replacing one for the same event.
    #[must_use]
    pub fn on(mut self, l: Listener<M>) -> Self {
        self.0.on.retain(|m| m.on() != l.on());
        self.0.on.push(l);
        self
    }

    /// A click sends `m`.
    #[must_use]
    pub fn click(self, m: M) -> Self {
        self.on(Listener::Click(m))
    }

    /// A submit of the form sends `m`.
    #[must_use]
    pub fn submit(self, m: M) -> Self {
        self.on(Listener::Submit(m))
    }

    /// Each edit of the field sends `f` of its words.
    #[must_use]
    pub fn input(self, f: fn(String) -> M) -> Self {
        self.on(Listener::Input(f))
    }

    /// Children by position, after any already given.
    #[must_use]
    pub fn child(mut self, c: impl Child<M>) -> Self {
        let mut had = match self.0.kids {
            Kids::Plain(had) => had,
            Kids::Keyed(_) => Vec::new(),
        };
        c.add_to(&mut had);
        self.0.kids = Kids::Plain(had);
        self
    }

    /// Children by key, in order; a key given twice keeps its first node, so a view cannot
    /// name one key twice.
    #[must_use]
    pub fn keyed(mut self, xs: impl IntoIterator<Item = (String, Node<M>)>) -> Self {
        let mut seen = BTreeSet::new();
        self.0.kids = Kids::Keyed(
            xs.into_iter()
                .filter(|(k, _)| seen.insert(k.clone()))
                .collect(),
        );
        self
    }

    /// The element as a node.
    #[must_use]
    pub fn node(self) -> Node<M> {
        Node::Element(Box::new(self.0))
    }
}

/// An element written as data, expanding to [`el`] and [`El`]'s methods and nothing else:
///
/// ```text
/// view!{ Section "line" {Data("key") => k} @click(Msg::Open) [ view!{ H3 [label] }, detail ] }
/// view!{ Div "rows" => rows.iter().map(|r| (r.key.clone(), row(r))) }
/// ```
///
/// A tag, an optional class, `{attribute => value, ...}`, `@click(message)`, `@submit(message)` or
/// `@input(function)`, then the children in `[...]` (anything [`Child`]) or `=> keyed pairs`.
#[macro_export]
macro_rules! view {
    ($tag:ident $($class:literal)? $({ $($a:expr => $v:expr),* $(,)? })? $(@ $ev:ident ($l:expr))* => $keyed:expr) => {
        $crate::node::el($crate::Tag::$tag)
            $(.set($crate::Attr::Class, $class))? $($(.set($a, $v))*)? $(.$ev($l))* .keyed($keyed) .node()
    };
    ($tag:ident $($class:literal)? $({ $($a:expr => $v:expr),* $(,)? })? $(@ $ev:ident ($l:expr))* $([ $($kid:expr),* $(,)? ])?) => {
        $crate::node::el($crate::Tag::$tag)
            $(.set($crate::Attr::Class, $class))? $($(.set($a, $v))*)? $(.$ev($l))* $($(.child($kid))*)? .node()
    };
}
