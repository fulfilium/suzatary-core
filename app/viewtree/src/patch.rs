//! What the painter is told to do, and the bytes that tell it.
//!
//! The format, read by `web/src/painter/patches.ts`: a sequence of patches, each an opcode byte
//! and its fields. An id is a LEB128 `u32`; `before` is the id of the node to insert before plus
//! one, or `0` for the end; a string is its UTF-8 length as a LEB128 `u32`, then its bytes. Id
//! `0` is the root the painter was given.

use crate::node::{On, Tag};

/// The root the painter was given.
pub const ROOT: u32 = 0;

/// One instruction to the painter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Patch {
    /// Make an element and insert it.
    Element {
        /// Its new id.
        id: u32,
        /// The element it goes in.
        parent: u32,
        /// The node it goes before; `None` for the end.
        before: Option<u32>,
        /// What it is.
        tag: Tag,
    },
    /// Make a text node and insert it.
    Text {
        /// Its new id.
        id: u32,
        /// The element it goes in.
        parent: u32,
        /// The node it goes before; `None` for the end.
        before: Option<u32>,
        /// Its text.
        text: String,
    },
    /// Ask the host for a foreign part and insert it.
    Foreign {
        /// Its new id.
        id: u32,
        /// The element it goes in.
        parent: u32,
        /// The node it goes before; `None` for the end.
        before: Option<u32>,
        /// Which of the host's parts.
        kind: String,
        /// Which one.
        key: String,
        /// What it is drawn from.
        props: String,
    },
    /// Set an attribute.
    Set {
        /// The element.
        id: u32,
        /// Its name.
        name: String,
        /// Its value.
        value: String,
    },
    /// Remove an attribute.
    Unset {
        /// The element.
        id: u32,
        /// Its name.
        name: String,
    },
    /// Set a text node's text.
    Retext {
        /// The text node.
        id: u32,
        /// Its new text.
        text: String,
    },
    /// Tell a foreign part its new props.
    Reprop {
        /// The part.
        id: u32,
        /// What it is now drawn from.
        props: String,
    },
    /// Move a node, keeping it (and its focus, scroll and children).
    Move {
        /// The node.
        id: u32,
        /// The element it goes in.
        parent: u32,
        /// The node it goes before; `None` for the end.
        before: Option<u32>,
    },
    /// Remove a node and everything in it.
    Remove {
        /// The node.
        id: u32,
    },
    /// Start reporting an event on an element.
    Listen {
        /// The element.
        id: u32,
        /// The event.
        on: On,
    },
    /// Stop reporting an event on an element.
    Unlisten {
        /// The element.
        id: u32,
        /// The event.
        on: On,
    },
}

const ELEMENT: u8 = 0;
const TEXT: u8 = 1;
const FOREIGN: u8 = 2;
const SET: u8 = 3;
const UNSET: u8 = 4;
const RETEXT: u8 = 5;
const REPROP: u8 = 6;
const MOVE: u8 = 7;
const REMOVE: u8 = 8;
const LISTEN: u8 = 9;
const UNLISTEN: u8 = 10;

fn varint(out: &mut Vec<u8>, mut n: u32) {
    while n >= 0x80 {
        out.push(n.to_le_bytes()[0] | 0x80);
        n >>= 7;
    }
    out.push(n.to_le_bytes()[0]);
}

fn string(out: &mut Vec<u8>, s: &str) {
    varint(out, u32::try_from(s.len()).unwrap_or(u32::MAX));
    out.extend_from_slice(s.as_bytes());
}

fn place(out: &mut Vec<u8>, id: u32, parent: u32, before: Option<u32>) {
    varint(out, id);
    varint(out, parent);
    varint(out, before.map_or(0, |b| b + 1));
}

/// The patches as the bytes the painter reads.
#[must_use]
pub fn encode(patches: &[Patch]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in patches {
        match p {
            Patch::Element {
                id,
                parent,
                before,
                tag,
            } => {
                out.push(ELEMENT);
                place(&mut out, *id, *parent, *before);
                out.push(tag.code());
            }
            Patch::Text {
                id,
                parent,
                before,
                text,
            } => {
                out.push(TEXT);
                place(&mut out, *id, *parent, *before);
                string(&mut out, text);
            }
            Patch::Foreign {
                id,
                parent,
                before,
                kind,
                key,
                props,
            } => {
                out.push(FOREIGN);
                place(&mut out, *id, *parent, *before);
                string(&mut out, kind);
                string(&mut out, key);
                string(&mut out, props);
            }
            Patch::Set { id, name, value } => {
                out.push(SET);
                varint(&mut out, *id);
                string(&mut out, name);
                string(&mut out, value);
            }
            Patch::Unset { id, name } => {
                out.push(UNSET);
                varint(&mut out, *id);
                string(&mut out, name);
            }
            Patch::Retext { id, text } => {
                out.push(RETEXT);
                varint(&mut out, *id);
                string(&mut out, text);
            }
            Patch::Reprop { id, props } => {
                out.push(REPROP);
                varint(&mut out, *id);
                string(&mut out, props);
            }
            Patch::Move { id, parent, before } => {
                out.push(MOVE);
                place(&mut out, *id, *parent, *before);
            }
            Patch::Remove { id } => {
                out.push(REMOVE);
                varint(&mut out, *id);
            }
            Patch::Listen { id, on } => {
                out.push(LISTEN);
                varint(&mut out, *id);
                out.push(on.code());
            }
            Patch::Unlisten { id, on } => {
                out.push(UNLISTEN);
                varint(&mut out, *id);
                out.push(on.code());
            }
        }
    }
    out
}

/// Why bytes are not patches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Malformed(pub usize);

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, Malformed> {
        let b = *self.bytes.get(self.at).ok_or(Malformed(self.at))?;
        self.at += 1;
        Ok(b)
    }

    fn varint(&mut self) -> Result<u32, Malformed> {
        let mut n: u32 = 0;
        for shift in (0..35).step_by(7) {
            let b = self.byte()?;
            n |= u32::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(n);
            }
        }
        Err(Malformed(self.at))
    }

    fn string(&mut self) -> Result<String, Malformed> {
        let n = self.varint()? as usize;
        let end = self.at.checked_add(n).ok_or(Malformed(self.at))?;
        let s = self.bytes.get(self.at..end).ok_or(Malformed(self.at))?;
        self.at = end;
        String::from_utf8(s.to_vec()).map_err(|_| Malformed(self.at))
    }

    fn place(&mut self) -> Result<(u32, u32, Option<u32>), Malformed> {
        Ok((
            self.varint()?,
            self.varint()?,
            self.varint()?.checked_sub(1),
        ))
    }

    fn tag(&mut self) -> Result<Tag, Malformed> {
        const TAGS: [Tag; 12] = [
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
        ];
        let at = self.at;
        TAGS.get(usize::from(self.byte()?))
            .copied()
            .ok_or(Malformed(at))
    }

    fn on(&mut self) -> Result<On, Malformed> {
        let at = self.at;
        On::of(self.byte()?).ok_or(Malformed(at))
    }
}

/// The bytes read back as patches: the painter's reading, here so its round trip is a law.
///
/// # Errors
/// [`Malformed`], at the byte where the bytes stop being patches.
pub fn decode(bytes: &[u8]) -> Result<Vec<Patch>, Malformed> {
    let mut r = Reader { bytes, at: 0 };
    let mut out = Vec::new();
    while r.at < bytes.len() {
        let op = r.byte()?;
        out.push(match op {
            ELEMENT => {
                let (id, parent, before) = r.place()?;
                Patch::Element {
                    id,
                    parent,
                    before,
                    tag: r.tag()?,
                }
            }
            TEXT => {
                let (id, parent, before) = r.place()?;
                Patch::Text {
                    id,
                    parent,
                    before,
                    text: r.string()?,
                }
            }
            FOREIGN => {
                let (id, parent, before) = r.place()?;
                Patch::Foreign {
                    id,
                    parent,
                    before,
                    kind: r.string()?,
                    key: r.string()?,
                    props: r.string()?,
                }
            }
            SET => Patch::Set {
                id: r.varint()?,
                name: r.string()?,
                value: r.string()?,
            },
            UNSET => Patch::Unset {
                id: r.varint()?,
                name: r.string()?,
            },
            RETEXT => Patch::Retext {
                id: r.varint()?,
                text: r.string()?,
            },
            REPROP => Patch::Reprop {
                id: r.varint()?,
                props: r.string()?,
            },
            MOVE => {
                let (id, parent, before) = r.place()?;
                Patch::Move { id, parent, before }
            }
            REMOVE => Patch::Remove { id: r.varint()? },
            LISTEN => Patch::Listen {
                id: r.varint()?,
                on: r.on()?,
            },
            UNLISTEN => Patch::Unlisten {
                id: r.varint()?,
                on: r.on()?,
            },
            _ => return Err(Malformed(r.at - 1)),
        });
    }
    Ok(out)
}
