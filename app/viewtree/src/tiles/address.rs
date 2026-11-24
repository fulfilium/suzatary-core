//! The address: the tree's print, which `parse` reads back as the same tree.
//!
//! ```text
//! address := route [ "~" node ]
//! node    := axis fit "(" part ("," part)* ")"
//! part    := [share] [focus] tree
//! tree    := "*" | leaf | node | ""
//! axis    := "|" (across) | "-" (down)
//! fit     := "" (fits) | "!" (never)
//! share   := "1" | "2" | "3"          (absent: one)
//! focus   := "'"                      (a node's focused child, off the path to "*")
//! leaf    := the page's print, escaped (`escape`)
//! ```
//!
//! `*` is the focused leaf, whose print is the head; it appears once. On its path a node's focused
//! child is the part that holds it; off it, the part marked `'`, or the first. A root child
//! equal to its declared home prints as `""`. Below the root a node has two parts or more.

use super::Tiles;
use super::act::{Act, act};
use super::escape::{escape, unescape};
use super::form;
use super::leaf::Leaf;
use super::node::{Axis, Fit, Node, Part, Share};
use super::open::MOST;
use super::page::Page;
use super::tree::Tree;

/// The bare head when `parse` reads it back as these tiles: a workspace's home, or a home with
/// one page opened from it. The whole tree otherwise.
#[must_use]
pub fn print<P: Page>(tiles: &Tiles<P>) -> String {
    let head = tiles.focused().page().print();
    if same(&read(&head, tiles), tiles) {
        return head;
    }
    let root = tiles.root.map(&Leaf::page);
    let home = tiles.home.map(&|p| p);
    format!("{head}~{}", node(&root, Path::On, Some(&home)))
}

/// Total, and the inverse of `print`; it answers only well-formed tiles (`Tiles::check`). A tree
/// that is malformed, or not well formed, reads as the whole string's page if the page reads it,
/// else as its head alone; a head the page cannot read gives the declared home. A head alone is
/// its page's workspace focused, with the page opened from the workspace's home when it is not
/// that home. Every id is minted fresh, after those of `tiles`: a host that keeps a live tile
/// across an address matches the pages itself.
#[must_use]
pub fn parse<P: Page>(s: &str, tiles: &Tiles<P>) -> Tiles<P> {
    read(s, tiles)
}

fn read<P: Page>(s: &str, lineage: &Tiles<P>) -> Tiles<P> {
    let (head, tail) = match s.rfind('~') {
        Some(k) => (&s[..k], Some(&s[k + 1..])),
        None => (s, None),
    };
    let tree = tail.and_then(|tail| {
        let page = P::parse(head)?;
        let mut r = Reader {
            s: tail.as_bytes(),
            i: 0,
            slots: 0,
        };
        let (root, stars) = r.node(&page, Within::Root(&lineage.home))?;
        let read_whole = r.i == r.s.len() && stars == 1;
        let formed = form::check(&root, &lineage.home, &Page::pin).is_ok();
        (read_whole && formed).then_some(root)
    });
    // A tail that starts as a node does is a tree, damaged or not; any other `~` is the page's own.
    let a_tree = tail.is_some_and(|t| t.starts_with(['|', '-']));
    match tree {
        Some(root) => lineage.grow(&root),
        None if !a_tree && P::parse(s).is_some() => bare(s, lineage),
        None => bare(head, lineage),
    }
}

fn bare<P: Page>(head: &str, lineage: &Tiles<P>) -> Tiles<P> {
    let tiles = lineage.grow(&lineage.home);
    let Some(page) = P::parse(head) else {
        return tiles;
    };
    let home = page.home();
    let at = tiles
        .root
        .parts()
        .position(|p| matches!(&p.tree, Tree::Leaf(l) if *l.page() == home));
    let tiles = match at {
        Some(i) => Tiles {
            root: tiles.root.select(i),
            ..tiles
        },
        None => tiles,
    };
    act(tiles, Act::Open(page))
}

fn same<P: Page>(a: &Tiles<P>, b: &Tiles<P>) -> bool {
    a.root.map(&Leaf::page) == b.root.map(&Leaf::page)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Path {
    On,
    Off,
}

fn node<P: Page>(n: &Node<&P>, path: Path, home: Option<&Node<&P>>) -> String {
    let mut s = String::from(match n.axis {
        Axis::Across => "|",
        Axis::Down => "-",
    });
    if n.fit == Fit::Never {
        s.push('!');
    }
    s.push('(');
    for (i, part) in n.parts().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(match part.share {
            Share::One => "",
            Share::Two => "2",
            Share::Three => "3",
        });
        let focused = i == n.index();
        match (focused, path, &part.tree) {
            (true, Path::On, Tree::Leaf(_)) => s.push('*'),
            (true, Path::On, Tree::Node(c)) => s.push_str(&node(c, Path::On, None)),
            (true, Path::Off, t) | (false, _, t) => {
                if focused && i > 0 {
                    s.push('\'');
                }
                let text = tree(t);
                let elided = home
                    .and_then(|h| h.parts().nth(i))
                    .is_some_and(|h| tree(&h.tree) == text);
                if !elided {
                    s.push_str(&text);
                }
            }
        }
    }
    s.push(')');
    s
}

fn tree<P: Page>(t: &Tree<&P>) -> String {
    match t {
        Tree::Leaf(p) => escape(&p.print()),
        Tree::Node(n) => node(n, Path::Off, None),
    }
}

/// A reader of the tree's print. `slots` is the least number of leaves the workspace being read
/// will hold: each node below the root holds two children or more, so entering one adds a leaf,
/// and so does each child after its second. A read stops the moment it would pass `MOST`, so the
/// nesting it reads is at most `MOST - 1` nodes deep, whatever the string.
struct Reader<'s> {
    s: &'s [u8],
    i: usize,
    slots: usize,
}

/// Where a node stands: the root, which keeps the home's children, or below it.
enum Within<'h, P> {
    Root(&'h Node<P>),
    Workspace,
}

impl<P> Clone for Within<'_, P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P> Copy for Within<'_, P> {}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn take(&mut self, b: u8) -> bool {
        let took = self.peek() == Some(b);
        if took {
            self.i += 1;
        }
        took
    }

    /// One more leaf the workspace will hold; `None` past `MOST`.
    fn slot(&mut self) -> Option<()> {
        self.slots += 1;
        (self.slots <= MOST).then_some(())
    }

    /// A node, and how many `*` it holds.
    fn node<P: Page>(&mut self, head: &P, within: Within<'_, P>) -> Option<(Node<P>, usize)> {
        let axis = match self.peek()? {
            b'|' => Axis::Across,
            b'-' => Axis::Down,
            _ => return None,
        };
        self.i += 1;
        let fit = if self.take(b'!') {
            Fit::Never
        } else {
            Fit::Fits
        };
        if !self.take(b'(') {
            return None;
        }
        if let Within::Workspace = within {
            self.slot()?;
        }
        let mut parts = Vec::new();
        let mut starred = Vec::new();
        let mut marked = Vec::new();
        loop {
            let i = parts.len();
            match within {
                Within::Root(home) if i >= home.len() => return None,
                Within::Root(_) => self.slots = 1,
                Within::Workspace if i >= 2 => self.slot()?,
                Within::Workspace => {}
            }
            let share = match self.peek() {
                Some(b'2') => Share::Two,
                Some(b'3') => Share::Three,
                Some(_) | None => Share::One,
            };
            if matches!(self.peek(), Some(b'1' | b'2' | b'3')) {
                self.i += 1;
            }
            if self.take(b'\'') {
                marked.push(i);
            }
            let (tree, stars) = match (self.peek()?, within) {
                (b'*', _) => {
                    self.i += 1;
                    (Tree::Leaf(head.clone()), 1)
                }
                (b'|' | b'-', _) => {
                    let (n, stars) = self.node(head, Within::Workspace)?;
                    (Tree::Node(n), stars)
                }
                (b',' | b')', Within::Root(home)) => (home.parts().nth(i)?.tree.clone(), 0),
                (b',' | b')', Within::Workspace) => return None,
                _ => {
                    let start = self.i;
                    while !matches!(self.peek(), None | Some(b',' | b')' | b'(')) {
                        self.i += 1;
                    }
                    let text = std::str::from_utf8(&self.s[start..self.i]).ok()?;
                    (Tree::Leaf(P::parse(&unescape(text)?)?), 0)
                }
            };
            if stars > 0 {
                starred.push(i);
            }
            parts.push((Part { share, tree }, stars));
            if self.take(b')') {
                break;
            }
            if !self.take(b',') {
                return None;
            }
        }
        let stars: usize = parts.iter().map(|(_, s)| s).sum();
        let focus = match (starred.as_slice(), marked.as_slice()) {
            ([], []) => 0,
            ([s], []) => *s,
            ([], [m]) => *m,
            _ => return None,
        };
        let mut parts = parts.into_iter().map(|(p, _)| p);
        let first = parts.next()?;
        Some((Node::new(axis, fit, first, parts.collect(), focus), stars))
    }
}
