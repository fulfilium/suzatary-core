//! The composer. Its words are the tab's draft for the conversation (`Local::Draft`); the box
//! empties when the draft leaves.

use crate::part::Mark;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use suzatary_api::wire::{Uploaded, Uploading};
use suzatary_base::reference::Reference;
use suzatary_topics::wire::topics::{AttachmentRow, TurnRow};
use ts_rs::TS;
use viewtree::Attr::{
    Accept, AriaLabel, Class, Data, Disabled, Hidden, Multiple, Placeholder, Rows, Title, Type,
    Value,
};
use viewtree::{Key, Node, Picked, view};

use crate::chats::readonly;
use crate::chrome::mark;
use crate::places::Place;
use crate::runtime::{Answer, Keys, Local, Reach, Register, answered, read};

/// The most files one turn holds, as the box takes it.
const MAX_FILES: usize = suzatary_lines::secretary::lines_rule::MAX_ATTACHMENTS_PER_TURN;

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

/// What a Send asks: a question, or the question whose line the person chose to edit asked again
/// in the box's words, named as they chose it, never as "the last one".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(rename = "ComposerMode")]
pub enum Mode {
    Asking,
    Editing { question: String },
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
    /// Each file the box answered an upload with, in `files`' order: what a tab seals into its turn.
    pub(crate) attachments: Vec<serde_json::Value>,
    pub(crate) mode: Mode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "of")]
#[ts(rename = "ComposerAsk")]
pub enum Asking {
    /// Each by the number its file was picked with.
    Upload(u32),
    Keep(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ev {
    Typed(String),
    Picked(Vec<Picked>),
    Detached(String),
    Unfiled(String),
    KeepPrivate(u32),
    Drop(u32),
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
    /// The upload's answer for each file, by its id ([`Sent::attachments`]).
    kept: BTreeMap<String, serde_json::Value>,
    mode: Mode,
    picks: u32,
    uploading: Vec<(u32, Picked)>,
    /// By the number each was picked with, so an answer reaches only its own file.
    held: BTreeMap<u32, Held>,
    others: Vec<String>,
    /// A question sent and not sealed yet: its words stay in the box until the tab has kept its
    /// line, so a page closed in between loses nothing ([`Composer::sealed`]).
    sealing: Option<String>,
    drafted: Drafted,
}

/// Whether the draft the device keeps has been read into the box: until it is, an empty box is no
/// empty composer, since the draft lands over whatever is put there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drafted {
    Unread,
    Read,
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
            // A todo named by its genesis is no thing the secretary reads: it reads a chain's.
            let seeds = r
                .parse::<Reference>()
                .is_ok_and(|r| !matches!(r, Reference::Global(_)));
            if seeds && !references.contains(r) {
                references.push(r.clone());
            }
        }
        (
            Composer {
                draft: draft.to_owned(),
                text: String::new(),
                references,
                files: Vec::new(),
                kept: BTreeMap::new(),
                mode: Mode::Asking,
                picks: 0,
                uploading: Vec::new(),
                held: BTreeMap::new(),
                others: Vec::new(),
                sealing: None,
                drafted: Drafted::Unread,
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

    /// Whether words handed back would land in an empty box: its draft read, and nothing typed,
    /// attached or on its way, so they overwrite nothing the person did since.
    #[must_use]
    pub(crate) fn takes_back(&self) -> bool {
        self.drafted == Drafted::Read
            && self.empty()
            && self.references.is_empty()
            && self.sealing.is_none()
    }

    /// A refused question placed in the box where it takes words back ([`Composer::takes_back`]),
    /// as it was sent; none where it does not. Nothing in flight (`sealing`) is touched.
    pub(crate) fn place(&mut self, sent: &Sent) -> Option<Vec<Out>> {
        if !self.takes_back() {
            return None;
        }
        self.references.clone_from(&sent.references);
        self.files.clone_from(&sent.files);
        self.kept = sent
            .attachments
            .iter()
            .filter_map(|a| Some((a.get("id")?.as_str()?.to_owned(), a.clone())))
            .collect();
        self.mode = sent.mode.clone();
        Some(self.typed(sent.text.clone()))
    }

    #[must_use]
    pub(crate) fn editing(&self) -> bool {
        matches!(self.mode, Mode::Editing { .. })
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
        match read::<Read>(r) {
            viewtree::kit::Reading::Val { v } => {
                self.text = v.text;
                self.others = v.others;
                self.drafted = Drafted::Read;
            }
            viewtree::kit::Reading::Error { .. } => self.drafted = Drafted::Read,
            viewtree::kit::Reading::Loading => {}
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

    /// `question`, the turn of the line named `line`, in the box to be edited.
    pub(crate) fn edit(&mut self, question: &TurnRow, line: String) -> Vec<Out> {
        let suzatary_topics::wire::topics::Speaker::Person {
            references,
            attachments,
            ..
        } = &question.who
        else {
            return Vec::new();
        };
        self.mode = Mode::Editing { question: line };
        self.references = references.iter().map(ToString::to_string).collect();
        self.files.clone_from(attachments);
        let mut out = self.typed(question.text.clone());
        out.push(focus());
        out
    }

    /// The question sent is the device's: the box empties, or, where the person typed past it
    /// meanwhile, keeps what they typed and lets the chips and files go with the question.
    pub(crate) fn sealed(&mut self) -> Vec<Out> {
        match self.sealing.take() {
            Some(sent) if self.text.trim() == sent => self.emptied(),
            Some(_) => {
                self.references.clear();
                self.files.clear();
                self.kept.clear();
                self.mode = Mode::Asking;
                vec![self.keep("Settled")]
            }
            None => Vec::new(),
        }
    }

    /// The question sent was refused before it was kept: its words are still in the box.
    pub(crate) fn released(&mut self) {
        self.sealing = None;
    }

    pub(crate) fn failed(&mut self, sent: Sent) -> Vec<Out> {
        self.sealing = None;
        if !self.empty() || !self.references.is_empty() {
            return Vec::new();
        }
        self.references = sent.references;
        self.kept = sent
            .attachments
            .into_iter()
            .filter_map(|a| Some((a.get("id")?.as_str()?.to_owned(), a)))
            .collect();
        self.files = sent.files;
        self.mode = sent.mode;
        self.typed(sent.text)
    }

    pub(crate) fn update(&mut self, ev: Ev) -> Vec<Out> {
        match ev {
            Ev::Typed(text) => self.typed(text),
            Ev::Picked(files) => {
                // The box refuses a turn past `MAX_FILES`, so no more are taken here.
                let room = MAX_FILES
                    .saturating_sub(self.files.len() + self.uploading.len() + self.held.len());
                let over = files.len() > room;
                let mut out: Vec<Out> = files
                    .into_iter()
                    .take(room)
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
                    .collect();
                if over {
                    out.push(Out::Toast(format!(
                        "At most {MAX_FILES} files in one turn."
                    )));
                }
                out
            }
            Ev::Detached(r) => {
                self.references.retain(|x| *x != r);
                Vec::new()
            }
            Ev::Unfiled(id) => {
                self.files.retain(|f| f.id != id);
                self.kept.remove(&id);
                Vec::new()
            }
            Ev::KeepPrivate(id) => match self.held.get_mut(&id) {
                Some(h @ Held::Asking(_)) => {
                    let w = h.w().clone();
                    let out = Out::Upload(
                        Asking::Keep(id),
                        Uploading {
                            file: w.base64.clone(),
                            name: w.name.clone(),
                            private: true,
                        },
                    );
                    *h = Held::Keeping(w);
                    vec![out]
                }
                Some(Held::Keeping(_)) | None => Vec::new(),
            },
            Ev::Drop(id) => {
                self.held.remove(&id);
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

    /// The chips go with the question, not the box, and so do the files held back or on their way.
    fn emptied(&mut self) -> Vec<Out> {
        self.references.clear();
        self.files.clear();
        self.kept.clear();
        self.uploading.clear();
        self.held.clear();
        self.mode = Mode::Asking;
        self.typed(String::new())
    }

    fn send(&mut self) -> Vec<Out> {
        if self.text.trim().is_empty() && self.files.is_empty() || self.sealing.is_some() {
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
            attachments: self
                .files
                .iter()
                .filter_map(|f| self.kept.get(&f.id).cloned())
                .collect(),
            mode: self.mode.clone(),
        };
        // A question and an edit are sealed in the tab: the words stay until the line is kept.
        self.sealing = Some(sent.text.clone());
        vec![Out::Send(sent)]
    }

    pub(crate) fn answered(&mut self, ask: Asking, a: Answer) -> Vec<Out> {
        match ask {
            Asking::Upload(id) => {
                let picked = self
                    .uploading
                    .iter()
                    .position(|(u, _)| *u == id)
                    .map(|at| self.uploading.remove(at).1);
                // A pick no longer on its way went with a cancelled question: nothing of it stays.
                let Some(picked) = picked else {
                    return Vec::new();
                };
                match answered::<Uploaded>(a) {
                    // The box keeps a file's bytes, not its name: the turn names it as it was picked.
                    Ok(Uploaded::Kept { file, attachment }) => self.filed(
                        AttachmentRow {
                            name: picked.name,
                            ..file
                        },
                        &attachment,
                    ),
                    Ok(Uploaded::Sensitive { why, folder }) => {
                        self.held.insert(
                            id,
                            Held::Asking(Withheld {
                                base64: picked.base64,
                                name: picked.name,
                                why,
                                folder,
                            }),
                        );
                        Vec::new()
                    }
                    Err(e) => vec![Out::Toast(e)],
                }
            }
            // A file dropped, or whose question was cancelled, is no longer held: it attaches nothing.
            Asking::Keep(id) => match (self.held.remove(&id), answered::<Uploaded>(a)) {
                (Some(_), Ok(Uploaded::Kept { file, attachment })) => self.filed(file, &attachment),
                (None, _) | (Some(_), Ok(Uploaded::Sensitive { .. })) => Vec::new(),
                (Some(Held::Asking(w) | Held::Keeping(w)), Err(e)) => {
                    self.held.insert(id, Held::Asking(w));
                    vec![Out::Toast(e)]
                }
            },
        }
    }

    fn filed(&mut self, file: AttachmentRow, attachment: &impl Serialize) -> Vec<Out> {
        if !self.files.iter().any(|f| f.id == file.id) {
            if let Ok(attachment) = serde_json::to_value(attachment) {
                self.kept.insert(file.id.clone(), attachment);
            }
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
    let chip = |icon: Mark, name: &str, title: String| {
        view! { Span "filechip" {Title => title} [mark(icon), view! { Span "filename" [name] }] }
    };
    if !f.private.is_empty() {
        let name = if f.name.is_empty() {
            "private file"
        } else {
            &f.name
        };
        chip(
            Mark::Lock,
            name,
            format!("Kept private in your Drive at {}", f.private),
        )
    } else if f.media_type == "application/pdf" {
        let name = if f.name.is_empty() { "PDF" } else { &f.name };
        chip(Mark::Doc, name, name.to_owned())
    } else {
        view! { Img "turnphoto" {viewtree::Attr::Src => Place::Attachment(&f.id).href(), viewtree::Attr::Alt => "photo", viewtree::Attr::Loading => "lazy"} }
    }
}

/// Ctrl/⌘-Enter sends on every device, a bare Enter where there is a keyboard too; Escape stops;
/// ↑ in an empty box edits.
#[derive(Debug, Clone, Copy)]
struct Pressing {
    keys: Keys,
    empty: bool,
}

fn key(
    k: Pressing,
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
        "Enter" if k.keys == Keys::Physical && !shift => Some(Ev::Send),
        _ => None,
    }
}

pub(crate) fn view(c: &Composer, reach: &Reach, keys: Keys) -> Node<Ev> {
    match reach {
        Reach::Writes => {}
        Reach::Waits => return view! { Div "compose" },
        Reach::Reads { .. } => return view! { Div "compose" [readonly(reach)] },
    }
    let quiet = c.text.trim().is_empty() && c.files.is_empty();
    let held: Vec<Node<Ev>> = c
        .held
        .iter()
        .map(|(&n, h)| {
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
    let pressing = Pressing {
        keys,
        empty: c.empty(),
    };
    let textbox = viewtree::node::el(viewtree::Tag::Textarea)
        .set(Class, "composebox")
        .set(Rows, "1")
        .set(Placeholder, "Ask, or say what to do")
        .set(Value, &c.text)
        .set(Data("field"), "compose")
        .input(Ev::Typed)
        .keydown_of(pressing, key)
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
                mark(Mark::Clip),
                view! { Input {Type => "file", Accept => "image/*,application/pdf", Multiple => true, Hidden => true} @files(Ev::Picked) },
            ] },
            textbox,
            view! { Button "composesend" {Type => "button", Title => "Send", AriaLabel => "Send"} @click(Ev::Send) [mark(Mark::Send)] },
        ] },
    ] }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The composer is seeded with the references the secretary reads, never a todo named by
    /// its genesis, nor a string that is no reference.
    #[test]
    fn a_seed_by_genesis_is_not_taken() {
        let seeds = [
            "todo:life/x".to_owned(),
            "todo:+3f2a9c1b2d4e/x".to_owned(),
            "not a reference".to_owned(),
        ];
        let (composer, _) = Composer::of("d", &seeds);
        assert_eq!(composer.references, vec!["todo:life/x".to_owned()]);
    }

    fn k(name: &str, ctrl: bool, shift: bool) -> Key {
        Key {
            name: name.to_owned(),
            ctrl,
            meta: false,
            shift,
            words: String::new(),
        }
    }

    /// Bare Enter sends only where there is a keyboard; Shift-Enter is a new line; ↑ edits only an
    /// empty box.
    #[test]
    fn what_each_key_does() {
        let (phone, wide, written) = (
            Pressing {
                keys: Keys::Touch,
                empty: true,
            },
            Pressing {
                keys: Keys::Physical,
                empty: true,
            },
            Pressing {
                keys: Keys::Physical,
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
        c.sealed();
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

    /// A question's words stay in the box until the tab has kept its line, so a page closed just
    /// after Send loses nothing; a second Send meanwhile sends nothing, and what is typed past the
    /// question meanwhile stays when it is kept.
    #[test]
    fn a_question_stays_in_the_box_until_it_is_kept() {
        let sends = |outs: Vec<Out>| outs.iter().filter(|o| matches!(o, Out::Send(_))).count();
        let (mut c, _) = Composer::of("c", &[]);
        c.update(Ev::Typed("first".to_owned()));
        assert_eq!(sends(c.update(Ev::Send)), 1);
        assert_eq!(c.text, "first", "not kept yet");
        assert_eq!(sends(c.update(Ev::Send)), 0, "one question, sent once");
        c.sealed();
        assert!(c.empty(), "kept, the box empties");

        c.update(Ev::Typed("second".to_owned()));
        assert_eq!(sends(c.update(Ev::Send)), 1);
        c.update(Ev::Typed("second, and a third".to_owned()));
        c.sealed();
        assert_eq!(c.text, "second, and a third", "what was typed since stays");
    }
}
