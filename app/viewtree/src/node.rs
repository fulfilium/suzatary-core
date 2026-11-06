//! The view's vocabulary: closed sets of tags and attributes, text, keyed children, foreign parts.
//! A view builds `Node<M>`; the painter keeps the same type as `Node<M, u32, Never>`, with DOM ids.

use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

/// Closed: no `script`, `iframe` or `style`. The wire number is the index in `ALL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Tag {
    Div,
    Section,
    H2,
    H3,
    P,
    A,
    Button,
    Input,
    Details,
    Summary,
    Code,
    Span,
    H1,
    Ul,
    Li,
    Dl,
    Dt,
    Dd,
    Strong,
    Form,
    Pre,
    Textarea,
    Nav,
    Article,
    Img,
    Label,
    Aside,
    /// `ol`.
    Ol,
}

impl Tag {
    pub const ALL: [Tag; 28] = [
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
        Tag::Pre,
        Tag::Textarea,
        Tag::Nav,
        Tag::Article,
        Tag::Img,
        Tag::Label,
        Tag::Aside,
        Tag::Ol,
    ];

    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub fn of(code: u8) -> Option<Tag> {
        Self::ALL.get(usize::from(code)).copied()
    }

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
            Tag::Pre => "pre",
            Tag::Textarea => "textarea",
            Tag::Nav => "nav",
            Tag::Article => "article",
            Tag::Img => "img",
            Tag::Label => "label",
            Tag::Aside => "aside",
            Tag::Ol => "ol",
        }
    }
}

/// Closed: no handler (`onclick`) and no `style`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Attr {
    Class,
    /// `#…` inside the app, or `https:`/`http:`/`mailto:`, opened in a new tab with no handle
    /// back on this one; the painter drops any other.
    Href,
    Title,
    Role,
    AriaCurrent,
    EnterKeyHint,
    /// A person opening a `details` changes the DOM and not the view, so the painter sets it
    /// only when the view's own value moves.
    Open,
    Hidden,
    Type,
    Placeholder,
    AriaLabel,
    Autocomplete,
    Autocapitalize,
    Spellcheck,
    Disabled,
    /// Set as the property, and only when the field holds another.
    Value,
    AriaPressed,
    AriaExpanded,
    Rows,
    Accept,
    Multiple,
    /// A path on the box, or the painter drops it.
    Src,
    Alt,
    Loading,
    Maxlength,
    Min,
    Max,
    Step,
    Id,
    AriaControls,
    Tabindex,
    /// A checkbox's `checked`: the painter sets the property, so a box a person ticked is drawn
    /// as the view says.
    Checked,
    /// The field a label names by its `id`.
    For,
    Data(&'static str),
}

impl Attr {
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
            Attr::AriaPressed => "aria-pressed".to_owned(),
            Attr::AriaExpanded => "aria-expanded".to_owned(),
            Attr::Rows => "rows".to_owned(),
            Attr::Accept => "accept".to_owned(),
            Attr::Multiple => "multiple".to_owned(),
            Attr::Src => "src".to_owned(),
            Attr::Alt => "alt".to_owned(),
            Attr::Loading => "loading".to_owned(),
            Attr::Maxlength => "maxlength".to_owned(),
            Attr::Min => "min".to_owned(),
            Attr::Max => "max".to_owned(),
            Attr::Step => "step".to_owned(),
            Attr::Checked => "checked".to_owned(),
            Attr::Id => "id".to_owned(),
            Attr::AriaControls => "aria-controls".to_owned(),
            Attr::Tabindex => "tabindex".to_owned(),
            Attr::For => "for".to_owned(),
            Attr::Data(suffix) => format!("data-{suffix}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum On {
    Click,
    /// Reports the field's value.
    Input,
    /// Performs nothing and reports no words.
    Submit,
    /// Reports `Key::print`; the key's own effect is kept only when the handler answers no
    /// message.
    Keydown,
    /// Reports `"1"` for open, `""` for closed.
    Toggle,
    /// Reports `Picked::print`.
    Files,
    /// A click on an element inside this one that carries `data-intent`; the painter reports the
    /// nearest such element's intent, as written, and reports nothing for a click on none.
    Intent,
}

impl On {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            On::Click => 0,
            On::Input => 1,
            On::Submit => 2,
            On::Keydown => 3,
            On::Toggle => 4,
            On::Files => 5,
            On::Intent => 6,
        }
    }

    #[must_use]
    pub const fn of(code: u8) -> Option<On> {
        match code {
            0 => Some(On::Click),
            1 => Some(On::Input),
            2 => Some(On::Submit),
            3 => Some(On::Keydown),
            4 => Some(On::Toggle),
            5 => Some(On::Files),
            6 => Some(On::Intent),
            _ => None,
        }
    }
}

/// A handler as data. A view writes each function as a plain `fn`, so it reads only the event.
#[derive(Debug, Clone)]
pub enum Listener<M> {
    Click(M),
    Submit(M),
    Input(Handler<String, M>),
    /// `None` leaves the key to the browser.
    Keydown(Handler<Key, Option<M>>),
    Toggle(Handler<bool, M>),
    Files(Handler<Vec<Picked>, M>),
    /// The intent a click inside it named makes this message.
    Intent(Handler<String, M>),
}

pub struct Handler<A, B>(Rc<dyn Fn(A) -> B>);

impl<A, B> Handler<A, B> {
    pub fn call(&self, a: A) -> B {
        (self.0)(a)
    }
}

impl<A: 'static, B: 'static> Handler<A, B> {
    #[must_use]
    pub fn of(f: fn(A) -> B) -> Self {
        Handler(Rc::new(f))
    }

    fn then<C: 'static>(self, g: Rc<dyn Fn(B) -> C>) -> Handler<A, C> {
        Handler(Rc::new(move |a| g((self.0)(a))))
    }
}

impl<A, B> Clone for Handler<A, B> {
    fn clone(&self) -> Self {
        Handler(Rc::clone(&self.0))
    }
}

impl<A, B> std::fmt::Debug for Handler<A, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Handler")
    }
}

impl<M: 'static> Listener<M> {
    fn map<N: 'static>(self, f: &Rc<dyn Fn(M) -> N>) -> Listener<N> {
        let g = Rc::clone(f);
        match self {
            Listener::Click(m) => Listener::Click(f(m)),
            Listener::Submit(m) => Listener::Submit(f(m)),
            Listener::Input(h) => Listener::Input(h.then(g)),
            Listener::Keydown(h) => {
                Listener::Keydown(h.then(Rc::new(move |m: Option<M>| m.map(&*g))))
            }
            Listener::Toggle(h) => Listener::Toggle(h.then(g)),
            Listener::Files(h) => Listener::Files(h.then(g)),
            Listener::Intent(h) => Listener::Intent(h.then(g)),
        }
    }
}

impl<M: 'static> Node<M> {
    /// Lifts a part's messages into its page's (Elm's `Html.map`). A lazy part inside is drawn
    /// now, since its hash does not name `f`.
    #[must_use]
    pub fn map<N: 'static>(self, f: impl Fn(M) -> N + 'static) -> Node<N> {
        self.mapped(&(Rc::new(f) as Rc<dyn Fn(M) -> N>))
    }

    fn mapped<N: 'static>(self, f: &Rc<dyn Fn(M) -> N>) -> Node<N> {
        match self {
            Node::Text((), t) => Node::Text((), t),
            Node::Foreign((), x) => Node::Foreign((), x),
            Node::Block((), b) => Node::Block((), b),
            Node::Lazy(l) => l.draw().mapped(f),
            Node::Element(e) => {
                let Element {
                    id: (),
                    tag,
                    attrs,
                    on,
                    kids,
                } = *e;
                Node::Element(Box::new(Element {
                    id: (),
                    tag,
                    attrs,
                    on: on.into_iter().map(|l| l.map(f)).collect(),
                    kids: match kids {
                        Kids::Plain(ns) => {
                            Kids::Plain(ns.into_iter().map(|n| n.mapped(f)).collect())
                        }
                        Kids::Keyed(ns) => {
                            Kids::Keyed(ns.into_iter().map(|(k, n)| (k, n.mapped(f))).collect())
                        }
                    },
                }))
            }
        }
    }
}

/// Carries the field's words when the key went down: a field a script filled hears no input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    /// As `KeyboardEvent.key` spells it.
    pub name: String,
    pub ctrl: bool,
    pub meta: bool,
    pub shift: bool,
    pub words: String,
}

impl Key {
    /// `0`/`1` for each of ctrl, meta and shift, the name, a NUL (which no key's name holds), then
    /// the field's words.
    #[must_use]
    pub fn print(&self) -> String {
        let bit = |b: bool| if b { '1' } else { '0' };
        format!(
            "{}{}{}{}\0{}",
            bit(self.ctrl),
            bit(self.meta),
            bit(self.shift),
            self.name,
            self.words
        )
    }

    #[must_use]
    pub fn read(words: &str) -> Option<Key> {
        let mut bits = words.chars();
        let mut bit = || match bits.next() {
            Some('1') => Some(true),
            Some('0') => Some(false),
            _ => None,
        };
        let (ctrl, meta, shift) = (bit()?, bit()?, bit()?);
        let (name, field) = words.get(3..)?.split_once('\0')?;
        Some(Key {
            name: name.to_owned(),
            ctrl,
            meta,
            shift,
            words: field.to_owned(),
        })
    }
}

/// A photo is redrawn as a JPEG (its metadata gone); any other file is its own bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    pub media: String,
    pub base64: String,
    pub name: String,
}

impl Picked {
    /// One line per file, `media`, `base64` and `name` split by tabs: no media type or base64
    /// holds a tab, and the name is the rest of its line.
    #[must_use]
    pub fn print(files: &[Picked]) -> String {
        files
            .iter()
            .map(|f| {
                format!(
                    "{}\t{}\t{}",
                    f.media,
                    f.base64,
                    f.name.replace(['\n', '\r'], " ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[must_use]
    pub fn read(words: &str) -> Vec<Picked> {
        words
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\t');
                Some(Picked {
                    media: parts.next()?.to_owned(),
                    base64: parts.next()?.to_owned(),
                    name: parts.next()?.to_owned(),
                })
            })
            .collect()
    }
}

impl<M> Listener<M> {
    #[must_use]
    pub const fn on(&self) -> On {
        match self {
            Listener::Click(_) => On::Click,
            Listener::Submit(_) => On::Submit,
            Listener::Input(_) => On::Input,
            Listener::Keydown(_) => On::Keydown,
            Listener::Toggle(_) => On::Toggle,
            Listener::Files(_) => On::Files,
            Listener::Intent(_) => On::Intent,
        }
    }
}

/// A part another painter draws; `props` is all it is told, and a change of `props` is the
/// only change it is told about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Foreign {
    pub kind: &'static str,
    pub key: String,
    pub props: String,
}

/// The painter places the host's HTML for `key` through the host's one door from a string to
/// markup, showing `fallback` as text until it is held. The view never holds the HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Its template's name and its value's.
    pub key: String,
    pub fallback: String,
}

/// Redrawn only when `hash` (of the function and its value) differs from the part drawn at
/// the same place (Elm's `Html.Lazy`).
pub struct Lazy<M> {
    pub hash: u64,
    draw: Rc<dyn Fn() -> Node<M>>,
}

impl<M> Lazy<M> {
    #[must_use]
    pub fn draw(&self) -> Node<M> {
        (self.draw)()
    }
}

impl<M> Clone for Lazy<M> {
    fn clone(&self) -> Self {
        Self {
            hash: self.hash,
            draw: Rc::clone(&self.draw),
        }
    }
}

impl<M> std::fmt::Debug for Lazy<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Lazy({:016x})", self.hash)
    }
}

/// The painter keeps each lazy part as it drew it, so a painted tree holds none.
#[derive(Debug, Clone, Copy)]
pub enum Never {}

/// `draw` is a plain `fn`, so it reads nothing but `a`, and the hash of the two names the part.
pub fn lazy<M: 'static, A: Hash + 'static>(draw: fn(&A) -> Node<M>, a: A) -> Node<M> {
    let mut h = DefaultHasher::new();
    (draw as usize).hash(&mut h);
    a.hash(&mut h);
    Node::Lazy(Lazy {
        hash: h.finish(),
        draw: Rc::new(move || draw(&a)),
    })
}

#[derive(Debug, Clone)]
pub struct Element<M, I = (), L = Lazy<M>> {
    pub id: I,
    pub tag: Tag,
    /// Each name once (the builder keeps the last).
    pub attrs: Vec<(Attr, String)>,
    /// Each event once (the builder keeps the last).
    pub on: Vec<Listener<M>>,
    pub kids: Kids<M, I, L>,
}

/// By key, a list keeps each row's node (and its focus and scroll) when rows move.
#[derive(Debug, Clone)]
pub enum Kids<M, I = (), L = Lazy<M>> {
    Plain(Vec<Node<M, I, L>>),
    /// Every key once.
    Keyed(Vec<(String, Node<M, I, L>)>),
}

#[derive(Debug, Clone)]
pub enum Node<M, I = (), L = Lazy<M>> {
    /// Set as text, never parsed.
    Text(I, String),
    Element(Box<Element<M, I, L>>),
    Foreign(I, Foreign),
    Block(I, Block),
    Lazy(L),
}

pub fn text<M>(s: impl Into<String>) -> Node<M> {
    Node::Text((), s.into())
}

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

pub fn block<M>(key: impl Into<String>, fallback: impl Into<String>) -> Node<M> {
    Node::Block(
        (),
        Block {
            key: key.into(),
            fallback: fallback.into(),
        },
    )
}

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

#[derive(Debug)]
pub struct El<M>(Element<M>);

/// Words, or a bool for a present-or-absent attribute (`disabled`), whose `false` leaves it out.
pub trait AttrValue {
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

pub trait Child<M> {
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

impl<M: 'static> El<M> {
    #[must_use]
    pub fn input(self, f: fn(String) -> M) -> Self {
        self.on(Listener::Input(Handler::of(f)))
    }

    #[must_use]
    pub fn keydown(self, f: fn(Key) -> Option<M>) -> Self {
        self.on(Listener::Keydown(Handler::of(f)))
    }

    /// `data` carries what the view was drawn from (whether the field holds words), as a click
    /// carries its message.
    #[must_use]
    pub fn keydown_of<C: Clone + 'static>(self, data: C, f: fn(C, Key) -> Option<M>) -> Self {
        self.on(Listener::Keydown(Handler(Rc::new(move |k| {
            f(data.clone(), k)
        }))))
    }

    #[must_use]
    pub fn toggle(self, f: fn(bool) -> M) -> Self {
        self.on(Listener::Toggle(Handler::of(f)))
    }

    #[must_use]
    pub fn files(self, f: fn(Vec<Picked>) -> M) -> Self {
        self.on(Listener::Files(Handler::of(f)))
    }

    /// A click on an intent inside it sends `f` of the intent's words.
    #[must_use]
    pub fn intent(self, f: fn(String) -> M) -> Self {
        self.on(Listener::Intent(Handler::of(f)))
    }
}

impl<M> El<M> {
    #[must_use]
    pub fn set(mut self, a: Attr, value: impl AttrValue) -> Self {
        self.0.attrs.retain(|(b, _)| *b != a);
        if let Some(v) = value.text() {
            self.0.attrs.push((a, v));
        }
        self
    }

    #[must_use]
    pub fn on(mut self, l: Listener<M>) -> Self {
        self.0.on.retain(|m| m.on() != l.on());
        self.0.on.push(l);
        self
    }

    #[must_use]
    pub fn click(self, m: M) -> Self {
        self.on(Listener::Click(m))
    }

    #[must_use]
    pub fn submit(self, m: M) -> Self {
        self.on(Listener::Submit(m))
    }

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

    /// A key given twice keeps its first node.
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

    #[must_use]
    pub fn node(self) -> Node<M> {
        Node::Element(Box::new(self.0))
    }
}

/// An element as data, expanding to `el` and `El`'s methods only:
///
/// ```text
/// view!{ Section "line" {Data("key") => k} @click(Msg::Open) [ view!{ H3 [label] }, detail ] }
/// view!{ Div "rows" => rows.iter().map(|r| (r.key.clone(), row(r))) }
/// ```
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
