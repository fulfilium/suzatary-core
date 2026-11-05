//! What the painter is told to do, and the bytes `web/src/painter/patches.ts` reads: each patch is
//! an opcode byte and its fields; an id is a LEB128 `u32` (`0` is the root); `before` is the id
//! plus one, `0` for the end; a string is its UTF-8 length as LEB128, then its bytes.

use crate::node::{On, Tag};

pub const ROOT: u32 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Patch {
    Element {
        id: u32,
        parent: u32,
        before: Option<u32>,
        tag: Tag,
    },
    Text {
        id: u32,
        parent: u32,
        before: Option<u32>,
        text: String,
    },
    Foreign {
        id: u32,
        parent: u32,
        before: Option<u32>,
        kind: String,
        key: String,
        props: String,
    },
    Set {
        id: u32,
        name: String,
        value: String,
    },
    Unset {
        id: u32,
        name: String,
    },
    Retext {
        id: u32,
        text: String,
    },
    Reprop {
        id: u32,
        props: String,
    },
    /// Keeps the node's focus, scroll and children.
    Move {
        id: u32,
        parent: u32,
        before: Option<u32>,
    },
    /// Removes everything in it too.
    Remove {
        id: u32,
    },
    Listen {
        id: u32,
        on: On,
    },
    Unlisten {
        id: u32,
        on: On,
    },
    /// Its plain text is shown until its HTML is held.
    Block {
        id: u32,
        parent: u32,
        before: Option<u32>,
        key: String,
        fallback: String,
    },
    Rekey {
        id: u32,
        key: String,
        fallback: String,
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
const BLOCK: u8 = 11;
const REKEY: u8 = 12;

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
            Patch::Block {
                id,
                parent,
                before,
                key,
                fallback,
            } => {
                out.push(BLOCK);
                place(&mut out, *id, *parent, *before);
                string(&mut out, key);
                string(&mut out, fallback);
            }
            Patch::Rekey { id, key, fallback } => {
                out.push(REKEY);
                varint(&mut out, *id);
                string(&mut out, key);
                string(&mut out, fallback);
            }
        }
    }
    out
}

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
        let at = self.at;
        Tag::of(self.byte()?).ok_or(Malformed(at))
    }

    fn on(&mut self) -> Result<On, Malformed> {
        let at = self.at;
        On::of(self.byte()?).ok_or(Malformed(at))
    }
}

/// The painter's reading, here so the round trip is a law.
///
/// # Errors
/// [`Malformed`], at the byte offset where the bytes stop being patches.
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
            BLOCK => {
                let (id, parent, before) = r.place()?;
                Patch::Block {
                    id,
                    parent,
                    before,
                    key: r.string()?,
                    fallback: r.string()?,
                }
            }
            REKEY => Patch::Rekey {
                id: r.varint()?,
                key: r.string()?,
                fallback: r.string()?,
            },
            _ => return Err(Malformed(r.at - 1)),
        });
    }
    Ok(out)
}
