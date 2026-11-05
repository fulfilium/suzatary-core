//! The composer. Its words are the tab's draft for the conversation (`Local::Draft`); the box
//! empties when the draft leaves.

use serde::{Deserialize, Serialize};
use suzatary_core::reference::Reference;
use suzatary_core::wire::{AttachmentRow, TurnRow, Uploaded, Uploading};
use ts_rs::TS;
use viewtree::Attr::{
    Accept, AriaLabel, Class, Data, Disabled, Hidden, Multiple, Placeholder, Rows, Title, Type,
    Value,
};
use viewtree::{Key, Node, Picked, view};

use crate::chats::readonly;
use crate::chrome::mark;
use crate::places::Place;
use crate::runtime::{Answer, Local, Reach, Register, answered, read};

/// A file the box held back until the owner says to keep it private in their Drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Withheld {
    base64: String,
    name: String,
    why: String,
    folder: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Held {
    Asking(Withheld),
    Keeping(Withheld),
}

impl Held {
    fn w(&self) -> &Withheld {
        match self {
            Held::Asking(w) | Held::Keeping(w) => w,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mode {
    Asking,
    Editing,
}

/// `Local::Draft`: the box's words, and other words a conflict sets beside them.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "DraftRead")]
pub struct Read {
    pub(crate) text: String,
    pub(crate) others: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sent {
    pub(crate) text: String,
    pub(crate) references: Vec<String>,
    /// Kept on the box.
    pub(crate) files: Vec<AttachmentRow>,
    pub(crate) mode: Mode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "of")]
#[ts(rename = "ComposerAsk")]
pub enum Asking {
    /// By the number it was picked with.
    Upload(u32),
    /// By its place among the held-back files.
    Keep(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ev {
    Typed(String),
    Picked(Vec<Picked>),
    Detached(String),
    Unfiled(String),
    KeepPrivate(usize),
    Drop(usize),
    KeepMine,
    Take(String),
    Cancel,
    Send,
    /// Stops the running round when nothing is being edited.
    Stop,
    Up,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Then {
    Send(Sent),
    Stop,
    Up,
}

/// Uploads are asked as `ask` names them.
pub(crate) fn effects<H, A, S>(
    outs: Vec<Out>,
    ask: fn(Asking) -> A,
) -> (Vec<crate::runtime::Effect<H, A, S>>, Vec<Then>) {
    use crate::runtime::{Body, Effect};
    let mut effects = Vec::new();
    let mut then = Vec::new();
    for o in outs {
        match o {
            Out::Keep(register, value) => effects.push(Effect::Keep { register, value }),
            Out::Upload(a, body) => effects.push(Effect::Post {
                ask: ask(a),
                path: "/api/secretary/attachments".to_owned(),
                body: Body::Upload(body),
            }),
            Out::Focus => effects.push(Effect::Focus {
                field: "compose".to_owned(),
            }),
            Out::Toast(words) => effects.push(Effect::Toast { words }),
            Out::Send(sent) => then.push(Then::Send(sent)),
            Out::Stop => then.push(Then::Stop),
            Out::Up => then.push(Then::Up),
        }
    }
    (effects, then)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Out {
    Keep(Register, serde_json::Value),
    Upload(Asking, Uploading),
    Focus,
    Send(Sent),
    Stop,
    Up,
    Toast(String),
}

#[derive(Debug, Clone)]
pub(crate) struct Composer {
    draft: String,
    text: String,
    references: Vec<String>,
    files: Vec<AttachmentRow>,
    mode: Mode,
    picks: u32,
    uploading: Vec<(u32, Picked)>,
    held: Vec<Held>,
    others: Vec<String>,
}

fn register(draft: &str) -> Register {
    Register::Draft {
        id: draft.to_owned(),
    }
}

impl Composer {
    #[must_use]
    pub(crate) fn of(draft: &str, seeded: &[String]) -> (Composer, Local) {
        let mut references: Vec<String> = Vec::new();
        for r in seeded {
            if r.parse::<Reference>().is_ok() && !references.contains(r) {
                references.push(r.clone());
            }
        }
        (
            Composer {
                draft: draft.to_owned(),
                text: String::new(),
                references,
                files: Vec::new(),
                mode: Mode::Asking,
                picks: 0,
                uploading: Vec::new(),
                held: Vec::new(),
                others: Vec::new(),
            },
            Local::Draft {
                id: draft.to_owned(),
            },
        )
    }

    #[must_use]
    pub(crate) fn empty(&self) -> bool {
        self.text.is_empty() && self.files.is_empty()
    }

    #[must_use]
    pub(crate) fn editing(&self) -> bool {
        self.mode == Mode::Editing
    }

    fn typed(&mut self, text: String) -> Vec<Out> {
        if text == self.text {
            return Vec::new();
        }
        self.text = text;
        vec![self.keep("Typed")]
    }

    fn keep(&self, how: &str) -> Out {
        Out::Keep(
            register(&self.draft),
            serde_json::json!({ "type": how, "text": self.text }),
        )
    }

    pub(crate) fn held(&mut self, r: viewtree::kit::Reading<Answer>) {
        if let viewtree::kit::Reading::Val { v } = read::<Read>(r) {
            self.text = v.text;
            self.others = v.others;
        }
    }

    /// Attaching the same reference twice is one chip.
    pub(crate) fn attach(&mut self, reference: String) -> Vec<Out> {
        if !self.references.contains(&reference) {
            self.references.push(reference);
        }
        vec![focus()]
    }

    /// A quote, not a reference, since a turn is no place.
    pub(crate) fn quote(&mut self, said: &str) -> Vec<Out> {
        let cut: String = if said.chars().count() > 200 {
            format!("{}…", said.chars().take(200).collect::<String>())
        } else {
            said.to_owned()
        };
        let lead = if self.text.is_empty() {
            String::new()
        } else {
            format!("{}\n\n", self.text)
        };
        let text = format!("{lead}> {}\n\n", cut.replace('\n', "\n> "));
        let mut out = self.typed(text);
        out.push(focus());
        out
    }

    pub(crate) fn edit(&mut self, question: &TurnRow) -> Vec<Out> {
        let suzatary_core::wire::Speaker::Person {
            references,
            attachments,
            ..
        } = &question.who
        else {
            return Vec::new();
        };
        self.mode = Mode::Editing;
        self.references = references.iter().map(ToString::to_string).collect();
        self.files.clone_from(attachments);
        let mut out = self.typed(question.text.clone());
        out.push(focus());
        out
    }

    pub(crate) fn failed(&mut self, sent: Sent) -> Vec<Out> {
        if !self.empty() || !self.references.is_empty() {
            return Vec::new();
        }
        self.references = sent.references;
        self.files = sent.files;
        self.mode = sent.mode;
        self.typed(sent.text)
    }

    pub(crate) fn update(&mut self, ev: Ev) -> Vec<Out> {
        match ev {
            Ev::Typed(text) => self.typed(text),
            Ev::Picked(files) => files
                .into_iter()
                .map(|f| {
                    self.picks += 1;
                    let post = Out::Upload(
                        Asking::Upload(self.picks),
                        Uploading {
                            file: f.base64.clone(),
                            name: f.name.clone(),
                            private: false,
                        },
                    );
                    self.uploading.push((self.picks, f));
                    post
                })
                .collect(),
            Ev::Detached(r) => {
                self.references.retain(|x| *x != r);
                Vec::new()
            }
            Ev::Unfiled(id) => {
                self.files.retain(|f| f.id != id);
                Vec::new()
            }
            Ev::KeepPrivate(n) => match self.held.get(n).cloned() {
                Some(Held::Asking(w)) => {
                    let out = Out::Upload(
                        Asking::Keep(u32::try_from(n).unwrap_or(u32::MAX)),
                        Uploading {
                            file: w.base64.clone(),
                            name: w.name.clone(),
                            private: true,
                        },
                    );
                    self.held[n] = Held::Keeping(w);
                    vec![out]
                }
                Some(Held::Keeping(_)) | None => Vec::new(),
            },
            Ev::Drop(n) => {
                if n < self.held.len() {
                    self.held.remove(n);
                }
                Vec::new()
            }
            Ev::KeepMine => {
                self.others.clear();
                vec![self.keep("Settled")]
            }
            Ev::Take(text) => {
                self.others.clear();
                self.text = text;
                vec![self.keep("Settled")]
            }
            Ev::Cancel => self.emptied(),
            Ev::Send => self.send(),
            Ev::Stop if self.editing() => self.emptied(),
            Ev::Stop => vec![Out::Stop],
            Ev::Up => vec![Out::Up],
        }
    }

    /// The chips go with the question, not the box, and so do the files held back.
    fn emptied(&mut self) -> Vec<Out> {
        self.references.clear();
        self.files.clear();
        self.held.clear();
        self.mode = Mode::Asking;
        self.typed(String::new())
    }

    fn send(&mut self) -> Vec<Out> {
        if self.text.trim().is_empty() && self.files.is_empty() {
            return Vec::new();
        }
        if !self.uploading.is_empty() {
            return vec![Out::Toast("A file is still uploading.".to_owned())];
        }
        if !self.held.is_empty() {
            return vec![Out::Toast("Answer the file above first.".to_owned())];
        }
        let sent = Sent {
            text: self.text.trim().to_owned(),
            references: self.references.clone(),
            files: self.files.clone(),
            mode: self.mode.clone(),
        };
        let mut out = self.emptied();
        out.insert(0, Out::Send(sent));
        out
    }

    pub(crate) fn answered(&mut self, ask: Asking, a: Answer) -> Vec<Out> {
        match ask {
            Asking::Upload(id) => {
                let picked = self
                    .uploading
                    .iter()
                    .position(|(u, _)| *u == id)
                    .map(|at| self.uploading.remove(at).1);
                match (answered::<Uploaded>(a), picked) {
                    // The box keeps a file's bytes, not its name: the turn names it as it was picked.
                    (Ok(Uploaded::Kept { file }), picked) => self.filed(AttachmentRow {
                        name: picked.map_or(file.name.clone(), |f| f.name),
                        ..file
                    }),
                    (Ok(Uploaded::Sensitive { why, folder }), Some(f)) => {
                        self.held.push(Held::Asking(Withheld {
                            base64: f.base64,
                            name: f.name,
                            why,
                            folder,
                        }));
                        Vec::new()
                    }
                    (Ok(Uploaded::Sensitive { .. }), None) => Vec::new(),
                    (Err(e), _) => vec![Out::Toast(e)],
                }
            }
            Asking::Keep(n) => {
                let n = n as usize;
                match answered::<Uploaded>(a) {
                    Ok(Uploaded::Kept { file }) => {
                        if n < self.held.len() {
                            self.held.remove(n);
                        }
                        self.filed(file)
                    }
                    Ok(Uploaded::Sensitive { .. }) => {
                        if n < self.held.len() {
                            self.held.remove(n);
                        }
                        Vec::new()
                    }
                    Err(e) => {
                        if let Some(Held::Keeping(w)) = self.held.get(n).cloned() {
                            self.held[n] = Held::Asking(w);
                        }
                        vec![Out::Toast(e)]
                    }
                }
            }
        }
    }

    fn filed(&mut self, file: AttachmentRow) -> Vec<Out> {
        if !self.files.iter().any(|f| f.id == file.id) {
            self.files.push(file);
        }
        Vec::new()
    }
}

const fn focus() -> Out {
    Out::Focus
}

/// A photo as itself, a PDF by its name, a private file by its name and where it is kept.
#[must_use]
pub(crate) fn attachment<M>(f: &AttachmentRow) -> Node<M> {
    let chip = |icon: &'static str, name: &str, title: String| {
        view! { Span "filechip" {Title => title} [mark(icon), view! { Span "filename" [name] }] }
    };
    if !f.private.is_empty() {
        let name = if f.name.is_empty() {
            "private file"
        } else {
            &f.name
        };
        chip(
            "lock",
            name,
            format!("Kept private in your Drive at {}", f.private),
        )
    } else if f.media_type == "application/pdf" {
        let name = if f.name.is_empty() { "PDF" } else { &f.name };
        chip("doc", name, name.to_owned())
    } else {
        view! { Img "turnphoto" {viewtree::Attr::Src => Place::Attachment(&f.id).href(), viewtree::Attr::Alt => "photo", viewtree::Attr::Loading => "lazy"} }
    }
}

/// Ctrl/⌘-Enter sends at every width, a bare Enter on a wide screen too; Escape stops; ↑ in an
/// empty box edits.
#[derive(Debug, Clone, Copy)]
struct Keys {
    wide: bool,
    empty: bool,
}

fn key(
    k: Keys,
    Key {
        name,
        ctrl,
        meta,
        shift,
        ..
    }: Key,
) -> Option<Ev> {
    match name.as_str() {
        "Escape" => Some(Ev::Stop),
        "ArrowUp" if k.empty && !ctrl && !meta && !shift => Some(Ev::Up),
        "Enter" if ctrl || meta => Some(Ev::Send),
        "Enter" if k.wide && !shift => Some(Ev::Send),
        _ => None,
    }
}

pub(crate) fn view(c: &Composer, reach: &Reach, wide: bool) -> Node<Ev> {
    match reach {
        Reach::Writes => {}
        Reach::Waits => return view! { Div "compose" },
        Reach::Reads { .. } => return view! { Div "compose" [readonly(reach)] },
    }
    let quiet = c.text.trim().is_empty() && c.files.is_empty();
    let held: Vec<Node<Ev>> = c
        .held
        .iter()
        .enumerate()
        .map(|(n, h)| {
            let w = h.w();
            let name = if w.name.is_empty() { "This photo".to_owned() } else { w.name.clone() };
            view! { Div "composeheld" [
                view! { P [format!("{name}: {}.", w.why)] },
                view! { P [format!("Keep it in your Drive at {}? The secretary reads its text with those numbers blacked out, and reading it again or sending it asks you first.", w.folder)] },
                view! { Div "composeheldacts" [
                    view! { Button "composeheldkeep" {Type => "button", Disabled => matches!(h, Held::Keeping(_))} @click(Ev::KeepPrivate(n)) ["Keep it private"] },
                    view! { Button "composeheldrop" {Type => "button"} @click(Ev::Drop(n)) ["Don't send it"] },
                ] },
            ] }
        })
        .collect();
    let thumbs: Vec<Node<Ev>> = c
        .files
        .iter()
        .map(|f| view! { Span "composethumb" [attachment(f), view! { Button "chipdrop" {Type => "button", Title => "Remove"} @click(Ev::Unfiled(f.id.clone())) ["×"] }] })
        .collect();
    let chips: Vec<Node<Ev>> = c
        .references
        .iter()
        .map(|r| {
            let kind = r.split(':').next().unwrap_or("");
            view! { Span {Class => format!("composechip ref-{kind}")} [r.as_str(), view! { Button "chipdrop" {Type => "button", Title => "Remove"} @click(Ev::Detached(r.clone())) ["×"] }] }
        })
        .collect();
    let elsewhere = (!c.others.is_empty()).then(|| {
        view! { Div "composeelsewhere" [
            view! { P "legend" ["This draft was also typed elsewhere. Keep what is in the box, or take the other words:"] },
            view! { Button "composekeep" {Type => "button"} @click(Ev::KeepMine) ["Keep mine"] },
            c.others.iter().map(|o| view! { Button "composeuse" {Type => "button"} @click(Ev::Take(o.clone())) [if o.is_empty() { "(nothing)" } else { o.as_str() }] }).collect::<Vec<_>>(),
        ] }
    });
    let keys = Keys {
        wide,
        empty: c.empty(),
    };
    let textbox = viewtree::node::el(viewtree::Tag::Textarea)
        .set(Class, "composebox")
        .set(Rows, "1")
        .set(Placeholder, "Ask, or say what to do")
        .set(Value, &c.text)
        .set(Data("field"), "compose")
        .input(Ev::Typed)
        .keydown_of(keys, key)
        .node();
    view! { Div "compose" [
        view! { Div "composeediting" {Hidden => !c.editing()} [
            view! { Span ["Editing your question — Send asks it again"] },
            view! { Button "composecancel" {Type => "button"} @click(Ev::Cancel) ["Cancel"] },
        ] },
        elsewhere,
        view! { Div "composechips" [chips] },
        view! { Div "composethumbs" [held, thumbs, (!c.uploading.is_empty()).then(|| view! { Span "composethumb pending" ["…"] })] },
        view! { Div {Class => if quiet { "composefield unsaid" } else { "composefield" }} [
            view! { Label "composeattach" {Title => "Attach a photo or PDF", AriaLabel => "Attach a photo or PDF"} [
                mark("clip"),
                view! { Input {Type => "file", Accept => "image/*,application/pdf", Multiple => true, Hidden => true} @files(Ev::Picked) },
            ] },
            textbox,
            view! { Button "composesend" {Type => "button", Title => "Send", AriaLabel => "Send"} @click(Ev::Send) [mark("send")] },
        ] },
    ] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(name: &str, ctrl: bool, shift: bool) -> Key {
        Key {
            name: name.to_owned(),
            ctrl,
            meta: false,
            shift,
            words: String::new(),
        }
    }

    /// Bare Enter sends only on a wide screen; Shift-Enter is a new line; ↑ edits only an empty
    /// box.
    #[test]
    fn what_each_key_does() {
        let (phone, wide, written) = (
            Keys {
                wide: false,
                empty: true,
            },
            Keys {
                wide: true,
                empty: true,
            },
            Keys {
                wide: true,
                empty: false,
            },
        );
        let table = [
            (phone, k("Enter", true, false), Some(Ev::Send)),
            (wide, k("Enter", true, false), Some(Ev::Send)),
            (phone, k("Enter", false, false), None),
            (wide, k("Enter", false, false), Some(Ev::Send)),
            (wide, k("Enter", false, true), None),
            (phone, k("Escape", false, false), Some(Ev::Stop)),
            (wide, k("ArrowUp", false, false), Some(Ev::Up)),
            (written, k("ArrowUp", false, false), None),
            (wide, k("ArrowUp", false, true), None),
            (wide, k("a", false, false), None),
        ];
        for (keys, pressed, does) in table {
            assert_eq!(key(keys, pressed.clone()), does, "{pressed:?} at {keys:?}");
        }
    }

    /// Typing after a send is the next draft; a failed send comes back only into an empty box.
    #[test]
    fn a_failed_send_comes_back_only_into_an_empty_box() {
        let (mut c, _) = Composer::of("c", &[]);
        c.update(Ev::Typed("first".to_owned()));
        let sent = c
            .update(Ev::Send)
            .into_iter()
            .find_map(|o| if let Out::Send(s) = o { Some(s) } else { None });
        let sent = sent.expect("a written box sends");
        assert!(c.empty());
        c.update(Ev::Typed("next".to_owned()));
        c.failed(sent.clone());
        assert_eq!(
            c.text, "next",
            "a failure never overwrites what was typed since"
        );
        c.update(Ev::Typed(String::new()));
        c.failed(sent);
        assert_eq!(c.text, "first", "into an empty box it comes back");
    }
}
