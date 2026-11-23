# One layout for every page: one tree of tiles (2026-11-22)

Status: open. Roadmap `tiledlayout`. It changes `lensrestore`, `streamcomposer` and
`toastinview` (§7) and puts one module in the page framework of `frameworksplit` (§8). No code
lands with this note.

The idea is a tiling window manager's. The bar switches workspaces, and a workspace holds what
was opened in it. On a wide screen a todo opens beside the list. On a phone the same act shows
the todo over the list, and Back returns to the list. One model gives both, and a function of
that model and the viewport decides what is drawn where. The model is one tree, and the bar's
workspaces are that tree's top level, not a second structure.

Each section gives its decision and the alternative it refuses. §10 counts the concepts a reader
must hold, today and in this model. §11 says what each piece of prior art gave and what was left
out. The figures were measured on this tree (2.1.219) as §12 describes.

## 0. What the layout replaces, measured

Layout today is done by hand, page by page, each case with its own code:

| Special case | Where | Lines |
| --- | --- | --- |
| The todo list's pane: a `todo::Model` inside the list's model, with its holds, asks and messages wrapped and routed by hand, and a write answered after the pane moved on handled apart (`unshown`) | `app/pages/src/todos.rs` | 132 |
| The todo page's own Back, for when it was pushed over the list | `app/pages/src/todo.rs` | 9 |
| The conversations column beside a conversation on a wide screen | `app/pages/src/conversation.rs` | 28 |
| The secretary panel: a second program mounted outside the page, with its own chrome, mount and unmount | `web/src/chat/panel.ts`, `web/src/chat/aside.ts` (`placement`) | 84 |
| The route resolved against the width (`Screen`, `screenFor`, `WIDE_MIN_PX`) | `web/src/shell/layout.ts` | 20 |
| The Typst list's open todo, drawn a second time as a pane and chosen by a container query | `web/typst/list.typ`, `web/src/typst/tree.ts`, `web/static/style.css` | 31 |
| `.split`, `.detailpane`, `.todosplit`, `.secretarypanel`, `.chatswide` and their 62rem rules | `web/static/style.css` | 199 |

That is 503 non-blank lines: 169 of Rust, 106 of TypeScript, 1 of Typst and 227 of CSS. On top of
them, 34 lines carry `wide` from `matchMedia` through `App`, `main.ts`, `Context` and `absorb`
(`app.ts`, `main.ts`, `program.ts`, `frame.ts`, `page.ts`, `table.ts`, `chrome.ts`). Stage b (§9)
deletes all of it. The brief named two special cases. The conversations column and the Typst
list's pane are two more instances of the same thing: a list with a page beside it.

## 1. The model

**Decision.** The model is one tree with one kind of node. It holds what was opened and where,
and no sizes.

```rust
/// A tile's identity: fresh for each page opened, never reused, so the host never feeds a
/// program that was closed.
pub struct Id(u32);

pub enum Tree<P> { Leaf(Leaf<P>), Node(Node<P>) }

pub struct Leaf<P> { id: Id, page: P }

/// The children of a node, as a zipper: the focused child is the hole, so a focus that names
/// no child cannot be written.
pub struct Node<P> {
    axis: Axis,
    fit: Fit,
    before: Vec<Part<P>>, focus: Box<Part<P>>, after: Vec<Part<P>>,
}
pub struct Part<P> { share: Share, tree: Tree<P> }

pub enum Axis { Across, Down }          // side by side; one above the other
pub enum Share { One, Two, Three }      // a child's weight in its node (§4)

/// Whether a node lays its children out side by side where they fit, or always shows one.
pub enum Fit { Fits, Never }
```

- **One node kind, with its fit declared.** A `Fits` node draws as many of its children side by
  side as fit, and collapses the rest into a stack (§2). A `Never` node always draws exactly one
  child, the focused one, and names the others in its crumb row. i3 has three container kinds:
  split, tabbed and stacked. Here they are one kind. i3's split is a `Fits` node on a wide
  enough viewport. Its tabbed container is a `Never` node across, and its stacked container is a
  `Never` node down. A `Fits` node on a phone draws as a stack, so the stack is the arrangement's
  answer for a narrow viewport, not a second kind.
- **The workspaces are the root's children.** The root is a `Never` node across whose children
  are the bar's destinations: Todos, Inbox, Secretary, Briefing, Mail, Calendar, Settings and
  About. Each child starts as its destination's page alone. "Switch workspace" is `Focus` at the
  root, and the bar is the root's crumb row (§2). No `Workspace` or `Desk` type is left. The
  workspaces not on screen are the root's unfocused children, held in the same tree.
- **Nodes have any number of children, and two is the common case.** A binary split is a node
  of two children. A binary tree would need nested splits for the root's eight destinations and
  for a row of columns. Reading such a chain as one row needs a law that reassociating it changes
  nothing, and the shares break that law. So the node holds its children as a list.
- **The focus is the path through each node's focused child.** Each node remembers its focused
  child, as i3's containers do. An unfocused subtree therefore knows which of its leaves to show
  when it collapses, which §2 needs.
- **A leaf holds a page's address.** `P` is the page's address: today's `Route`. The page's
  state lives in its running program, which the host keeps by `Id` (§6). The model holds no
  page state. A model that held every tile's state would be one Elm program over all tiles.
  The view is rebuilt and diffed whole on each step, so every keystroke in one tile would
  rebuild every tile. The todo list's pane is that design built by hand, and it is 132 lines of
  routing (§0).
- **What a page declares about itself is data on its leaf, read through `Page` (§8)**: its kind,
  its shape (a least width, and a ratio for a page typeset to a fixed size), and whether it is
  pinned. A **pinned** leaf stays where it stands when the pages beside it are replaced (§3).
  The secretary beside a page is a pinned leaf, and that is all that distinguishes it. It has no
  type of its own. What it is about follows the focused tile's reference, which the secretary's
  page derives itself, as `aside.placement` derives it today.
- **Not in the model**: pixel sizes, the viewport, whether the device is a phone, and which
  tiles are drawn. `arrange` derives all of these (§2). No act takes a viewport, so a change of
  viewport cannot change the tree. The types guarantee it, so no test has to.

**What it replaces.** `Width::{Column, Wide}` (`chrome.rs`) goes: a page draws its column, and
its tile's width is the arrangement's. `Context.wide` goes. The todo list's `Pane`, the todo
page's `pushed`, the shell's panel and `aside.placement`, the conversations column, the Typst
list's pane and the bar's list of destinations all become nodes of the tree. `Screen` and
`screenFor` go, because nothing resolves against a width. `Route` stays as a leaf's address (§5).

Two readers of `wide` were not about layout:

- The composer sends on Enter where there is a keyboard (`composer.rs`). That reading becomes
  `Context.keys: Keys::{Physical, Touch}`, read from `(pointer: fine)`.
- The todo list either opens a tapped row in place or goes to the todo. On a phone it opens the
  row in place, and that is the phone's first screen, which must stay (§7). That reading
  becomes the tile's own `Context.opens: Opens::{Beside, Over}`. `arrange` computes it as the
  answer to one question: would a page opened from this tile be drawn beside it? It is a fact
  about the arrangement, so a page reads it and never computes it.

## 2. The arrangement is a function

```rust
pub struct Viewport { pub w: Px, pub h: Px }
pub fn arrange<P: Page>(tiles: &Tiles<P>, at: Viewport, m: &Measures) -> Arranged
pub struct Arranged { pub tiles: Vec<Placed>, pub dividers: Vec<Divider>, pub rows: Vec<Row> }
pub struct Placed { pub id: Id, pub rect: Rect, pub page: Rect, pub scroll: Scroller, pub opens: Opens }
pub enum Scroller { Document, Tile }
```

**Least sizes.** Each kind of page declares the least width it reads well at, as data. Every page
shares one least height, 20rem (320 px). These values put today's 62rem breakpoint back where it
is:

| Page | Least width |
| --- | --- |
| A list (todos, inbox, mail, conversations, briefings) | 20rem (320 px) |
| A page read as a column (a todo, a message, a thread, a briefing, a conversation, settings) | 40rem (640 px) |
| The secretary beside another page | 22rem (352 px), today's panel |

A divider is `d` = 1.5rem (24 px), today's gap. Children side by side need their least widths
summed, plus `d` between each pair, and the largest of their least heights. Children one above
the other need their least heights summed, plus `d`, and the largest least width. A list beside
a todo then needs 320 + 24 + 640 = 984 px, under today's 992 px edge. The list, a todo and the
secretary need 1,360 px.

**The rule.** It holds for every tree, splits down included. The focused leaf is always drawn.
Every other leaf is drawn or behind.

1. Start the drawn region at the focused leaf.
2. Walk up its ancestors, nearest first.
   - At a `Never` node, add nothing. Its other children are behind.
   - At a `Fits` node, look at the children after the region, then the children before it, each
     side nearest first. Add a child if the region with it still fits the viewport along the
     node's axis, and across it. A child that fits only in part is added by its edge nearest the
     region, if that child is a node on the same axis. Its own children are taken nearest first,
     by the same test. A child of the other axis, or a leaf, is added whole or not at all.
3. Once a side of a node has a child that is not added whole, nothing more is added on that side,
   at that node or above it. The walk goes on for the other sides.
4. Each node drawn whole divides its length along its axis by its children's shares, clamped so
   each child gets at least its least size. The shortfall comes from the others in proportion to
   what they have above their least. A node drawn in part gives all of its length to the children
   that are drawn. If the viewport is smaller than the focused leaf's least size, the leaf is drawn
   at the viewport's size, so a 320 px phone still gets a page.
5. A `Fits` node that leaves a child behind draws a crumb row, so its walk is tried again with the
   row's height taken before anything is added. Every drawn tile therefore has at least its least
   width and height, but a focused leaf drawn alone.

The drawn region is a rectangle, and what is behind lies at its edges, never between two drawn
tiles. That is a law of §3.

**The crumb row is one function, and the bar is the root's crumb row.** `rows` holds one row for
each node that has children behind it. A row holds a crumb for every child of its node, in the node's
order, each marked `Before`, `Drawn` or `After`, and the host draws the ones behind as `‹ Todos`
before the region and `Secretary ›` after it for a node across, and `˄ Inbox` and `Mail ˅` for a
node down. A row is drawn at the top edge of the part of the region that its node covers. Each
crumb is a 44 px target, and a tap focuses that child. The root's row is the bar. On a phone it
is drawn at the foot, as the bar is today, and on a wide screen across the top. A row draws as
many crumbs as fit its width, in its node's order, and the rest go into an overflow sheet, the
More sheet. So a phone shows four destinations and More, as today, and a wide screen shows all
eight. A crumb shows its page's badge, such as the Inbox's count, so the bar's count is no special
case either. A `Never` node with one child has nothing behind it and draws no row, so a page with
no bar needs no flag. Swipes are refused. An edge swipe is the browser's own Back on both phone
platforms, and the Android portal's WebView passes that gesture through.

A todo over the list is a node of two leaves drawn as one tile, and its crumb row replaces the todo
page's own Back row: `‹ Todos` stands where `todo.rs` draws its Back today, so a phone screen
gains no height.

When the region is a single tile, on any viewport, that tile scrolls the document
(`Scroller::Document`), so a phone's browser hides its address bar as it does today. Tiles drawn
side by side each scroll themselves, as `.detailpane` and `.panelscroll` do today. The host
already reads the scroll box through `box()` at every effect, so a tile whose scroller changes
needs no new protocol. The host moves the position from one box to the other.

**Five cases.** The first three share one tree: the Todos workspace as a node across holding the
list (share One), a todo (Two) and the secretary (One, pinned), focused on the todo.

- **360 px.** The focused todo needs 640 px, already more than the viewport, so it is drawn alone
  at 360 px and nothing is added. The Todos row reads `‹ Todos · Secretary ›`. Tapping `‹ Todos`
  focuses the list, and its row then reads `Todo › · Secretary ›`. The bar is at the foot.
- **900 px.** Looking after the todo first: the secretary needs 640 + 24 + 352 = 1,016 px, more
  than 900. Before it, the list needs 984 px, also more. The todo is drawn alone, at most 50rem
  wide in its column as today, with the same crumbs as at 360 px. Today's 900 px screen is also
  the one-page layout, so nothing at this width changes.
- **1440 px.** After the todo, the secretary fits (1,016 px). Before it, the list fits too
  (1,360 px). All three are drawn and the Todos row is empty. The 1,392 px left after the two
  dividers split 1 : 2 : 1, giving 348, 696 and 348 px. The secretary is clamped up to its least,
  352 px, and the 4 px come from the others, leaving the list about 347 px and the todo about
  693 px.
- **Three lists one above the other, on a portrait tablet of 800 by 1280 px.** The person made
  them with `Below` (§3): a node down holding the inbox, the mail and the todos, focused on the
  mail. Below the mail, the todos fit: 320 + 24 + 320 = 664 px of height. Above, the inbox fits
  too: 1,008 px of 1,280. All three are drawn, 800 px wide and 396 px tall each (1,280 less the bar
  and two dividers). On a phone held in landscape, 740 by 360 px, only the mail is drawn, 272 px tall
  under its row and over the bar, and its row reads `˄ Inbox · Todos ˅`.
  The longer-side rule of dwindle (§11) would have made this shape on its own on a portrait
  tablet. Here it exists only because the person asked for it.
- **A split down inside a split across, at 900 by 800 px.** The workspace is a node across
  holding the list and a node down, and the node down holds a todo over a message. The focus is
  on the todo. At the node down, the message below fits (664 px of 800), so it is added whole and
  the region is 640 by 664 px. At the node across, the list before needs 320 + 24 + 640 = 984 px,
  more than 900, so it is behind. The todo is drawn over the message, 344 px each, and the row reads
  `‹ Todos`. At 1,440 px the list fits as well and all three are drawn: the list 472 px wide and the
  node down 944 px, at shares One and Two.

## 3. The acts

```rust
pub enum Act<P> {
    Open(P),       // by the one rule below
    Below(P),      // the focused tile becomes a node down: the one act that makes one
    Close,         // the focused tile
    Focus(Id),     // the leaf a crumb, a tile or a workspace names
    Move(Dir),     // swap the focused tile's page with its neighbour's
    Resize(Share), // the focused child's share
}
pub fn act<P: Page>(tiles: Tiles<P>, act: Act<P>) -> Tiles<P>   // total
```

**Open has one rule.** It decides only where the page goes in the tree. Whether the page is
drawn beside or over is then `arrange`'s answer for the viewport.

1. If the opened page is of the same kind as the focused leaf, it opens **here**: the focused
   leaf's address is replaced. Examples are another day of the calendar, another briefing, a
   todo's parent from the todo, and another conversation.
2. Otherwise the page **replaces the unpinned children that stand after the focused tile** in its
   node, and those tiles close. A pinned child stays, and the page goes before it. A todo picked
   in the list replaces the todo beside it, and the secretary after both stays.
3. If nothing unpinned stood after the focused tile, the page is inserted right after it, with
   the share its page declares (`Page::share`: One for a list, Two otherwise), and it takes the
   focus.
4. A pinned page (the secretary) opens at the end of the focused workspace's node, or replaces
   the pinned child already there. That is rule 2 applied from the workspace's last child.
5. From a pinned leaf, a free page replaces the free child right before it, or is inserted before
   it, so a link in the secretary opens beside it and the secretary stays at the end.

The focused tile's node is its parent. If the parent is the root or another `Never` node (a tab
node), rule 3 first wraps the focused tile in a new node across. This is the
behaviour of Miller columns (the Finder's column view). Each page opens to the right of the page
it was opened from and replaces whatever stood there. "Beside where it fits, over where it does
not" is then a property of `arrange`, not a second rule. A workspace holds at most four tiles
(§6). An open that would make a fifth opens here, and Back returns to the page it replaced; from a
pinned leaf, such an open does nothing. Rule 2 gives the page the share of the first child it
replaces.

**Back, Close and the crumb to the left are not one act.** The crumb is `Focus`. It moves the
focus and keeps the tile it left alive, which is what lets the reader return to their place.
`Close` removes the focused tile and stops its program. Back is not an act on the tree at all. It
moves through the address history, a log of whole trees (§5). The three are related by laws, not
merged: Back after an open is the same tree as `Close` after that open, when the open inserted a
tile (rule 3).

**Laws.** Each is a property over generated trees of any shape (any depth, both axes, both fits,
pinned leaves anywhere), acts and viewports:

- `arrange` is total, and it draws every drawn leaf once. The rectangles do not overlap. Together
  with the dividers and the crumb rows, they cover the viewport exactly.
- The drawn region is a rectangle, and no leaf behind lies between two drawn leaves on any axis.
- The focused leaf is drawn on every viewport.
- Every leaf is reachable on every viewport: focusing any leaf, then arranging, draws it.
- A `Never` node draws exactly one child. A `Fits` node whose children all fit draws them all.
- `act` takes no viewport, so a change of viewport cannot change the tree. The checked law is
  that `arrange(t, v)` depends on `t` and `v` alone: the same inputs give the same rectangles,
  however many viewports were tried in between.
- Open, then close, is the identity when the open inserted a tile (rule 3). For every open, the
  address history gives back the tree from before it (§5). Rules 1 and 2 replace a page, so only
  Back can undo them.
- An open never removes a pinned leaf other than by rule 4, and never moves one.
- A workspace of one leaf arranges to exactly today's page: one rectangle covering the viewport
  under the bar, `Scroller::Document`, no divider and an empty row. Its address prints as exactly
  today's hash. So does a home with one page opened from it (§5), whose crumb row on a phone is
  today's Back row. Today's phone is this trivial case, so the migration can be staged.
- The address round-trips: `parse(print(t)) == t`, and `parse` is total over strings (§5).
- Close of a workspace's last tile is the identity. A workspace always holds a page.
- Every act keeps the tree **well formed**, and `Tiles::new` and `parse` answer only such trees
  (`Tiles::check`, the error `Unfit`): the root keeps the home's axis, fit and number of
  workspaces; every node below the root has two children or more; a workspace holds at most four
  tiles; and a workspace holds at most one pinned leaf, as its **end**, the last leaf reached
  through the last child of each node. So an open of a pinned page replaces a pinned end, or the
  end of a full workspace, or is added after the workspace's last child; `Below` of a pinned page
  opens it the same way; `Below` from a pinned leaf puts the page above it; and `Move` does nothing
  when either child it would swap holds a pinned leaf.
- Every drawn tile has its least size, but a focused leaf drawn alone (§2, rule 5).

## 4. Resizing with a finger and with a mouse

**Decision: each child has a share of one, two or three, and nothing in between.** Two children
of shares One and Two split a third to two thirds, and One and One split in halves. A divider is
drawn 1 px wide with a 44 px handle at its middle. Dragging the handle with a mouse or a finger
moves weight between the two children beside it, and snaps to the nearest pair of shares when it
is let go. Tapping the handle steps the share of the child before it through One, Two and Three.
A share that would give a child less than its least size is skipped. A node that cannot show its
children side by side is a stack, so a narrow screen has no divider to drag.

Free ratios are refused. A finger cannot place a 1 px line, a ratio of 0.4137 means nothing to
read back, and the address would have to carry it. Three shares cover the layouts a person asks
for: a list beside its page, two pages side by side, and a page with a companion.

**Keyboard on a desktop**: a `w` prefix, after vim's window commands and beside the shell's
existing `g` prefix (`shell/keys.ts`). `w h/j/k/l` focuses by direction, and `w H/J/K/L` moves
the tile. `w s` opens below (`Below`), `w q` closes, and `w =` steps the focused child's share.
The arrow keys and Alt are left to the browser: Alt+← is Back on Windows and Linux. Only the
focused tile's program holds the page keys (`Effect::Keys`). The host installs that program's key
target when its tile takes the focus, so the keys that move a cursor in the list never reach the
todo beside it.

## 5. The address

**Decision: the address is the tree's print, and a page's `Route` is a leaf's address.** Every
address of the app is a tree. `Route` remains a type, but it now names one page, not where the
app is. A bare hash is shorthand for a tree, so every hash written today, and every link in a
briefing or a push, keeps its meaning.

```
address := route [ "~" node ]
node    := axis fit "(" part ("," part)* ")"
part    := [share] [focus] tree
tree    := "*" | leaf | node | ""
axis    := "|" (across) | "-" (down)
fit     := "" (fits) | "!" (never)
share   := "1" | "2" | "3"         (absent: One)
focus   := "'"                     (this node's focused child, off the path to "*")
leaf    := the route's hash, without "#", every byte but a letter, a digit, "_" and "." as %XX,
           and a leading digit too
```

- **`*` marks the focused leaf**, whose route is the address's head. It appears exactly once. On
  the path to it, each node's focused child is the part holding `*`. Off that path, the focused
  child is the part marked `'`, or the first part when none is marked.
- **The root is printed whole.** A child equal to its declared default prints as empty (`""`). The
  shell declares the root's children and each one's default, its destination's home page. So the
  address carries only the workspaces that differ from their homes, and a reload restores every
  one of them. The first draft kept the other workspaces' trees in a tab register. That register
  is gone, because those trees are now in the tree.
- **A bare hash whose page is not its workspace's home reads as the home with that page opened from
  it.** `#/todo/x` is the root focused on Todos, holding a node across of the list and `todo x`,
  focused on the todo. That tree prints back as the bare hash, so the address still round-trips.
  On a phone it draws the todo with `‹ Todos` in its crumb row, where today's pushed todo page
  draws its Back. On a desktop it draws the list beside the todo, which is what `#/todo/x` draws
  at 62rem today.
- **The 1440 px tree of §2** prints as `#/todo/x~|!(|(%2F,2*,secretary%2Fbeside),,,,,,,)`. A pin
  is the page's own declaration, so the address carries no pin mark. The
  secretary beside a page has one constant address, `#/secretary/beside`, because what it is about
  is derived from the focused tile.
- **A tile's own link is its leaf's bare hash.** "Copy link" on a page copies its route alone, so a
  link a person shares carries one page, not their other workspaces.
- **`parse` is total**, as `parseRoute` is: it reads a malformed tree, or one that is not well formed
  (§3), as the whole string's route, or else as the head route alone, and a malformed head as
  today's `unknown`. It stops reading the moment a workspace would hold a fifth leaf, counting two
  leaves for each node it enters, so a string of thousands of nested nodes reads in a few steps.
- **A page that prints as nothing** is written `:`, which no escaped address holds, so it never reads
  as a root child's home.
- **`parse` mints every id fresh.** A host keeps a live tile across an address change itself, in
  stage d: it matches the new tree's leaves to the live tiles by page, within each workspace and in
  order, and gives each matched leaf the live tile's program (§6).

**Back is the address history on every screen.** The history is a log of whole trees, and
`routelog` stays that log, holding addresses. An open pushes an entry, and so does a focus that
changes a `Never` node's child: switching workspace, or a tab. Other focuses, moves and resizes
replace the current entry. Closing goes back when the history's previous entry is exactly the tree
without that tile (the open that made it), and replaces the current entry otherwise. So Back
undoes the last open. On a phone that pops the stack. On a desktop it closes the tile opened last,
which is what Back does today when it takes a todo out of the pane. Switching workspace today
pushes too, so the bar's Back is unchanged.

**Kept per device**: nothing new. The least sizes are code, and the shares are in the address.
**Not kept at all**: which tiles are drawn (`arrange` derives it), and the state of pages behind a
`Never` node beyond what each page already keeps in its own registers.

## 6. A tile is a running page

Today one page program runs at a time, plus the panel's conversation on a wide screen. With
tiles, two rules decide what runs, and both read the tree alone:

- **Behind a `Fits` node that collapsed, a tile is kept and fed.** That is the stack a phone
  returns to with Back. A tile is kept by its leaf's `Id`. An act keeps every id it does not
  remove; an address read by `parse` mints new ones, so after Back or a reload the host matches the
  new leaves to the live tiles by page (§5) before it mounts anything.
- **Behind a `Never` node, nothing runs.** The workspaces not on screen, and the unfocused tabs of
  a tab node, are addresses in the tree. Focusing one mounts its tiles again, as navigating to a
  page does today.

**What two tiles share for nothing.** One views module per tab (`program.module` is a `once`).
One worker, holding one replica of each store, which the one-owner plan already makes the only
owner of each fact. One `held` register per GET path (`program.ts`: "one register per path"), so
two tiles holding the same path fetch it once. One local register per `Watch`.

**What they do not share.** Each `Effect::Core` question is asked per tile and answered per
tile. The todo list and the todo beside it each ask for the chain's reading, so the worker
computes it twice. Each program keeps its own model and its own painter's last tree. A memo in
the worker keyed by the question would share the core's answers. It is not needed for stage b,
whose two tiles are the cases running today.

**The budget.** The first-page bar is no task on the page's thread over 100 ms
(`web/e2e/bigstore.test.ts`, and `docs/PAGETHREAD-PLAN-2026-11-22.md` §1.2, at a CPU throttle of
4). Each of a program's held readings tells its moves in a task of its own (`soon.run` in
`program.ts`), so two tiles' readings never share a task. Two places today step every program in
one task:

- a `Context` change, which `app.hear` delivers to every program in the same dispatch;
- a mount, which prints the flags, makes the class and runs the first step synchronously.

The longest view step per visit measured at a throttle of 4 has a median of 27 ms and a maximum
of 50 ms on the 1x store (PAGETHREAD §4.1). On the 10x store the median is 69 ms. In one task,
then, three tiles fit at the median and two at the maximum on the 1x store, and only one at 10x.
**So the host delivers each tile's `Context` and mount in a task of its own.** With that, the
100 ms bar bounds the cost of one tile's step and not the number of tiles. A program's own start
is small beside that: §12 measures it at 0.1 ms or less, natively, for every rehearsed page.

**The bound on live tiles is four per workspace.** At the least widths of §2, 1,440 px draws
three tiles, and a fourth would need 2,024 px or more. The cap therefore costs a desktop nothing
it would draw, and a phone keeps at most three tiles behind the one shown.

**A hidden tile is kept, fed and not drawn.** Its program keeps running and its readings keep
arriving, so it is current when the person returns, and that is what makes Back return to the
reader's place (§7). Its element stays in the document under `content-visibility: hidden` and
`inert`. That skips its style, layout and paint, and keeps its scroll offset. Its patches still
apply, at the cost of a step, which is measured above. A `Focus` or `Scroll` effect from a hidden
tile waits until the tile is drawn.

**Where a toast, a sheet and the composer belong: to their tile.** A toast is drawn at the foot
of its tile's rectangle. A toast from a hidden tile is drawn in the shell's toast place, so a
refusal or an Undo offer is never lost. The add sheet rises inside its tile. The More sheet is
the root crumb row's overflow. The composer stands at the foot of its conversation's tile. None
of the three is a tile: none has a program or an address of its own (§10, c).

**Closing a tile with a write in flight.** The intent set holds the write, not the program, so the
write lands. The refusal of a write whose program has gone becomes a shell toast. That one
generic rule replaces `unshown` in `todos.rs`, which handles the same case for the pane alone.

## 7. What it replaces, and what the phone must keep

**The phone's first screen stays what it is.** The Todos workspace starts as one leaf, the list.
Its tile's `opens` is `Over` on a phone, so a tap opens the row in place, and Open goes to the
todo, as today. The bar, the More sheet and every hash keep their meaning.

**What a phone gains:**

- **A todo opened from a briefing, then Back, returns to the briefing where the reader was.** The
  briefing's tile stays alive behind the todo (§6), so its scroll and its open folds are still
  in the document. Today the briefing page is destroyed, and only the window's scroll comes back
  through `routelog.Kept`.
- **The secretary over a todo.** "Ask the secretary" opens the conversation beside the todo
  (a pinned leaf, §3 rule 4). On a phone the conversation is drawn over the todo with `‹ Todo` in its crumb row,
  and Back returns to the todo as it was. Today it navigates away from the todo.

**The secretary's opening rule stays, and stops reading the width.** Today the panel opens by
itself on a page about one thing when the conversation holds talk about it, a press holds for
that page in this tab (`aside.shown`, `aside.pressed`), and below 62rem the panel does not exist.
With tiles, the rule decides whether the workspace holds the pinned secretary leaf. It reads the
page's reference, the talk and the presses, never the viewport. On a wide screen the leaf is drawn
beside, as today. On a phone it is the crumb `Secretary ›` in the todo's existing Back row: a sign
that the secretary has said something about this todo, at no extra height. The alternative, a
rule that adds the leaf only where it would be drawn, would make an act read the viewport and
break the law that a resize never changes the tree.

**What it does to the open interface tasks.** The roadmap's descriptions could not be read in
this session: the binary needs an instance and the roadmap's store is in another repository. So
each task is read here from its name and from this brief:

- **`lensrestore`** (Back returns to where the reader was): subsumed. The page being returned to
  is a live tile and is never rebuilt. `routelog.Kept`'s scroll and focus are then needed only for
  a page reached by a workspace switch, which mounts again.
- **`streamcomposer`** (the composer of the stream page): changed. The composer belongs to its
  tile and stands at the tile's foot, so it has no fixed position against the viewport.
- **`toastinview`** (a toast drawn where its page is): subsumed for pages. A toast is drawn in its
  tile (§6). The shell's own toasts stay where they are.

## 8. Where it lives

The layout module names no page of this application. It is generic over a thing with an
address, a kind, a shape (its least width, and a ratio for a page typeset to a fixed size), a pin
and a view. So it belongs with the page framework: `app/viewtree` today, and the framework's own
repository after `frameworksplit`. It is not Prodrome's, because it is generic over pages, not over
a store's schema. Its public face:

```rust
//! `tiles`: one tree of pages, arranged for a viewport.

pub trait Page: Clone + Eq {
    type Kind: Copy + Eq;
    fn kind(&self) -> Self::Kind;    // open here when equal (§3, rule 1)
    fn shape(&self) -> Shape;        // how it fills a rectangle (below)
    fn pin(&self) -> Pin;            // whether it stays when its neighbours are replaced
    fn print(&self) -> String;       // the leaf's address
    fn parse(s: &str) -> Option<Self>;
    fn share(&self) -> Share;        // the share an open gives it where it inserts it (§3)
    fn opens(&self) -> Shape;        // the shape of what it opens, which decides `Opens`
    fn home(&self) -> Self;          // the page a bare address of it is opened from (§5)
}

/// A flowing page takes its whole rectangle and reads well from `least` wide. A paged one is
/// typeset to a fixed page and is drawn at the largest size of its ratio inside its rectangle.
pub enum Shape { Flows { least: Px }, Paged { least: Px, ratio: Ratio } }
pub enum Pin { Free, Pinned }

pub struct Tiles<P>;   // the root node, the declared home, and the counter that mints ids
impl<P: Page> Tiles<P> { pub fn new(home: Node<P>) -> Result<Tiles<P>, Unfit>; pub fn check(&self) -> Result<(), Unfit>; }
pub struct Row { pub rect: Rect, pub node: Vec<usize>, pub axis: Axis, pub shown: Vec<Crumb>, pub more: Vec<Crumb> }
pub enum Tree<L>;  pub struct Node<L>;  pub struct Leaf<P>;  pub struct Id;
pub enum Axis;     pub enum Fit;        pub enum Share;
pub enum Act<P>;   pub fn act(tiles: Tiles<P>, act: Act<P>) -> Tiles<P>;
pub fn arrange<P: Page>(tiles: &Tiles<P>, at: Viewport, m: &Measures) -> Arranged;
pub fn print<P: Page>(tiles: &Tiles<P>) -> String;
pub fn parse<P: Page>(s: &str, tiles: &Tiles<P>) -> Tiles<P>;   // the home and the ids of `tiles`

/// The tree as a `viewtree::Program`: its model is the tree and the viewport, its view the
/// dividers, the crumb rows (the bar among them) and one foreign part per drawn tile, keyed by
/// `Id`, and its effects the address's `Go` and `Replace`.
pub struct Desked<P>;
```

The page framework already has every piece the host needs. A tile is a `Foreign` part
(`viewtree::node::Foreign`, "a part another painter draws"), keyed by `Id`, which the host
draws by mounting the tile's program, and whose stop destroys it. The tiles are siblings in
one keyed list ordered by `Id`, placed by their rectangles' styles. Rearranging the tree
therefore never moves or re-parents a tile's element, so no program loses its DOM, its focus or
its scroll. `pages.table` becomes the drawer of that part: it maps a leaf's address to
`program.mount`. The shell instantiates `P` with the route, which the Rust side must parse. Stage
d moves `parseRoute` into Rust beside `places.rs`, which already prints every address, so the
address has one parser and one printer.

**A second application, as the check that the module names no page of this one.** Take a
slide deck written in Typst on the same framework. Each slide is a typeset page. The model is the
deck and the place in it, and next and back are steps. A second device joined to the deck's store
moves the place, so a phone is the remote.

- **The presenter's view on a wide screen** is a tree the deck builds at start and offers no act
  to change: a node across holding the current slide (share Two) and a node down, which holds the
  slide's notes over the next slide. That is three tiles with fixed roles, and none needs a pin,
  because nothing is opened beside them. Their content follows the place in the store. That is
  the page's state, so the layout takes no part in it.
- **The audience's view** is a root `Never` node with one child, a leaf, full screen. A `Never`
  node with one child has nothing behind it and draws no crumb row (§2), so having no bar needs
  no flag and no second shell.
- **A tile that keeps an aspect ratio is a property of its leaf.** A slide is typeset to a fixed
  page size, and this is the one place where the typesetter's layout decides a tile's shape. It
  enters `arrange` as data, through `Shape::Paged`. The fit test of §2 still reads the least
  width, and a paged leaf's least height is `least / ratio`. `arrange` gives the tile its
  rectangle as for any page, and the tile draws the page at the largest size of its ratio inside
  it, centred. The margin left over stays the tile's and is not handed to its neighbours, so no
  law of §3 changes. The page is an SVG of fixed size scaled by its `viewBox`, so a resize
  recompiles no slide.

Nothing else in the deck needed a concept that this application's pages lack, so the module's
face above is generic as written.

### 8.1 Where the typesetter belongs, and where it does not

**Decision: Typst sets a tile's content, CSS sets the tiles.** A resize compiles nothing.

- **A tile's content is typeset as today, and needs no compile when its tile changes width.**
  The box compiles content with Typst's HTML export and keeps the body by its content
  (`suzatary_todo::rendering`, `suzatary_base::narrowing`). The tab compiles the Typst list's
  view with typst-wasm. What the export emits is elements with classes (`page.typ`'s `el`,
  `kit.typ`'s `el`). It has no page and no page width. The pie is an inline SVG sized `1em`,
  and the one fixed size is the chevron icon's `width="22" height="22"`, which an icon may have.
  No app template uses `html.frame`. The one in the tree, 400pt wide, draws the roadmap's
  picture, which no tile shows. The browser flows the content to whatever width the tile has.
- **What keys on the viewport's width moves to the tile's.** `style.css` holds 13 rules on the
  viewport's width: eleven at `min-width: 62rem`, one at `max-width: 61.99rem`, and one at 900px
  for the destination title's size. It also has two `100vh` heights, for the panes. Each becomes a container
  query on the tile (`container-type: inline-size` on the tile's element), and each `100vh`
  becomes the tile's height. The Typst list already works this way. `.typstframe` is a
  container, and its open todo is drawn twice, once in its row and once in a pane, and shown once
  by `@container (min-width: 56rem)`, "by the frame's own width, never the viewport's". That rule
  is the model for the others, and it shows the content needs no recompile to change layout.
- **The arrangement is not typeset.** It is §2's pure function, drawn as tile elements whose
  rectangles are styles. A drag between shares changes one custom property per frame on the
  split's element, and the tiles' content reflows in the browser.
- **The budget a resize must meet is one frame**: 16.7 ms at 60 Hz, and 8.3 ms at 120 Hz.
  One typeset of a page takes 6 ms on the box (`docs/FRAMEWORK-PLAN-2026-11-02.md` §2.6), before
  the round trip to the box and the swap of the tile's HTML. A frame of a drag across three
  tiles would then spend 18 ms compiling alone. Measured here, one HTML export of a page of 31
  paragraphs by the `typst` 0.15.1 of `.#triage` takes 10.9 ms at the median of 21 runs (8.6 to
  17.6 ms, the process's start included, §12). One tile's compile is already over a frame at
  120 Hz. The arrangement therefore cannot go through the typesetter.
- **What the design system should give the layout: the measures.** A page's least width (§2) and
  the divider `d` should come from the same named values the typography uses, so that the
  collapse rule and the type agree and no second definition of a width exists (CLAUDE.md, "The
  look is the owner's design system"). Today those values are not the design system's.
  `tokens.typ` names colour roles only. The spacing scale (`--s-6: 1.5rem`, today's gap) and the
  reading column's 50rem measure are written in `style.css`. So stage b first moves the spacing
  scale and the measures into `tokens.typ`, printed by `scripts/gen-tokens.mjs` for CSS and for
  the Rust least widths alike. Writing 20rem and 40rem into the layout module by hand would be a
  second definition.

A Typst layout of a whole workspace would be the right tool only for a printed or exported view
of it, where the page size is fixed and nothing moves.

## 9. Stages

Each stage lands alone and deletes something, except the first.

| Stage | What a person sees change | What is deleted | Law added |
| --- | --- | --- | --- |
| **a.** The general module and its laws, wired to nothing: one node kind with its fit, axis and shares, pinned leaves, shapes, `arrange` over every tree, the acts and the address | Nothing | Nothing | Every law of §3, over generated trees of any shape and generated viewports |
| **b.** The existing special cases become instances: the list's pane, the todo's Back, the conversations column, the secretary panel as a pinned leaf, the Typst list's pane, the width-resolved route, and the 62rem rules as container queries on the tile. First, the spacing scale and the measures move into `tokens.typ` (§8.1) | The panel is a tile beside the page instead of a fixed overlay, and the page gives it room through the arrangement instead of `--aside` | The 503 lines of §0 and the 34 lines carrying `wide` | A workspace of one leaf draws today's page: the e2e scenes of today's phone pass unchanged, and the desktop scene sees the same tiles at 1440 px |
| **c.** Open by the one rule for links between pages, `Below`, and the divider with its shares | A link in a todo to a message opens the message beside the todo on a desktop, and over it on a phone | The per-page open branches (`Tap` and `open_at` reading `wide` in `todos.rs`) | Open then close is the identity; an open never moves a pinned leaf |
| **d.** The tree in the address, Back, and the bar as the root's crumb row | A reload, a link or Back restores the tiles of every workspace. A wide screen shows all eight destinations in the bar | `parseRoute` in TypeScript; `routelog.Kept`'s use for a page Back returns to; the bar's own list and painting in `nav.ts`, whose destinations become the root's declared children | The address round-trips; Back after an open restores the tree |
| **e.** Workspaces a person adds and names, only if wanted | A new destination in the bar | Nothing to delete: a workspace is a child of the root, so this stage adds a name and an act | A named child round-trips through the address |

**What the general tree makes later, honestly.** Stage a is larger than a module for columns
would be. Its generators must make trees of any depth, with both axes and both fits, and its laws
need a checker of rectangles: that they cover the viewport, do not overlap and leave nothing
behind between two drawn tiles. The address carries the whole root with its defaults, so stage d
also moves the bar, which touches the chrome of every page. The bar therefore moves in stage d and
not in b, where it would make b's change to every page larger. `Below` and `Never` nodes inside a
workspace exist from stage a and are tested there. Nothing in the app makes one until stage c
gives `Below` a key, and no tab node is made until a person asks for one.

## 10. What argues against it, and the concepts it unifies

**The case against, as strongly as it can be put.** This is one person's secretary, not a
window manager. A layout built for power users can make a simple app harder. The person opens
the app on a phone for ten seconds to see what needs them, and every concept added (tiles,
crumbs, shares) is one more thing that can surprise them. Every tile is a live program on a
phone's battery: §6 keeps hidden tiles fed. A free-form tree invites states nobody designed,
such as three lists stacked top to bottom on a portrait tablet, and the collapse rule of §2 must
then answer for every one of them.

**The two things done every day that it makes better.** First, reading a todo beside the list,
and the secretary beside both on a wide screen. That works today through four hand-made special
cases, and it becomes one rule. Second, opening something from a briefing or a todo on a phone
and coming back to exactly where the reader was. That is broken today, and `lensrestore` exists
because of it.

**What the daily acts make, as a fact about use.** The open rule of §3 only inserts after the
focused tile or replaces what stands after it. So the trees that daily use makes are rows of
columns inside each workspace, with the secretary pinned at the end of the row. Only `Below`
makes a node down, and it is not a daily act. This is a fact about use, not a second structure:
a row of columns is a node across, the node the model already has. It is why the rule of §2 reads
the children after the focus first, and why the common cases cost nothing beyond the general rule.

**The answer to the case against.** Every state a free tree can reach is answered by one rule
that §2 shows on trees the daily acts never make, and the laws of §3 hold for every generated
tree. The states nobody designed therefore all get a drawing that the laws check. The phone
reaches only what a person asks for. Battery and live programs are bounded by §6: four tiles a
workspace, and nothing running behind a `Never` node.

**Which concepts are the same concept.** Each candidate was tested:

- **(a) The workspaces are the same tree: adopted.** They are the children of a root `Never`
  node. Switching workspace is `Focus`, the bar is the root's crumb row, and `Workspace` and `Desk`
  are gone. i3's split, tabbed and stacked kinds become one node kind with a declared fit (§1).
  "Never fits" is a value a node declares (the root, a tab node). A phone is not a node, so it is
  not a value: it is a viewport on which `Fits` nodes collapse.
- **(b) The crumb row and the bar are one function: adopted.** One function draws the crumb row
  of any node with children behind it. The bar is that row at the root, drawn at the foot on a
  phone. The More sheet is the row's overflow, and the Inbox's count is its page's badge (§2).
- **(c) A sheet, a toast and the composer: refused, because they are different.** A page pushed
  over another is a tile behind a collapsed node, and that is adopted. A sheet, a toast and the
  composer are not tiles. None has a program of its own or an address of its own, and a sheet is
  modal: it holds the input until it is dismissed. The add sheet's state and draft are the list's
  own model (`Kept::Sheet`, `Kept::Draft`). `#/add?text=` is the list's address with a field the
  list consumes, not the sheet's. The More sheet is part of the root's crumb row. Forcing them
  into tiles would give an address and a program to things that have neither.
- **(d) Back, Close and the crumb to the left: refused as one act.** The crumb is `Focus`, which
  keeps the tile it leaves alive. `Close` stops a program. Back is no act on the tree, but a step
  through the history of trees. Laws relate them (§3).
- **(e) The address is the tree's print: adopted.** `Route` remains a type, as a leaf's address,
  and stops being the app's address. A bare hash is shorthand for a tree (§5).
- **(f) The Typst list's pane: adopted as an instance, and the container query refused as one.**
  The open todo drawn a second time beside the Typst list is a page beside a list, so it becomes a
  todo leaf beside the list's leaf, and the second drawing goes (§0). The open todo in its row on
  a phone stays: that is page content, read through `Opens::Over`, as on the todo list. A
  container query inside a page's content stays CSS. Running `arrange` inside a page's content
  would make the layout module draw content, which belongs to Typst and the browser (§8.1).

**The concepts a reader must hold:**

| Model | Concepts | Count |
| --- | --- | --- |
| Today, by hand | `Width`, `Context.wide`, `Screen` and `screenFor`, the list's `Pane`, the todo's `pushed` Back, the panel's `Placement`, the conversations column, the Typst list's pane, the bar's destinations (`nav.Tab`), `Route`, the history (`routelog`) | 11 |
| The note's first draft | `Tree` with its `Split` and `Stop`, `Workspace`, `Desk`, `Aside`, `Columns`, the crumb row, the bar, `Route`, the history, `Opens`, `Shape` | 12 |
| This model | `Tree`, one node kind with its `Fit`; a leaf's declarations (`Shape`, `Pin`); the crumb row, the bar among them; `Route`, as a leaf's address; the history of trees; `Opens` | 6 |

**Recommendation: the general tree, as one concept.** The first draft recommended a smaller
model, columns with an aside. That model was two concepts, and each was a special case of a node:
a row of columns is a node across, and the aside is a pinned leaf at its end. The general tree has
fewer concepts than either today's code or the first draft, it drops no capability, and every
case the app has today is one of its instances.

## 11. Prior art: what each gave and what was left out

- **xmonad's `StackSet`.** Taken: the whole manager is a pure data structure with laws checked
  by generated tests; a layout is a pure function from a stack and a rectangle to rectangles; and
  a stack is a zipper. Here each node's children are that zipper, and the set of workspaces with
  one current is the root `Never` node. Left out: screens other than the one, floating windows,
  and a layout class chosen per workspace. One arrangement serves every node.
- **i3 and sway's container tree.** Taken: focus as a path, with each container remembering its
  focused child, and nodes of any number of children. Left out: three container kinds. Split,
  tabbed and stacked are one node kind here, and the declared fit tells them apart. A stack is
  also what a `Fits` node becomes on a narrow viewport, so it is the arrangement's answer there.
- **bspwm and Hyprland's dwindle.** Taken: the binary partition as the common case, and a new
  window splitting where the focus is. Left out: splitting along the longer side. A page is a
  column read from top to bottom, so on a portrait tablet the longer-side rule would stack pages
  into short scrolling boxes. A node down exists only where a person asks for one (`Below`).
  Left out also: binary nodes only, for the reason of §1.
- **Adaptive list-detail on phone platforms** (a split view that collapses to a navigation stack
  when narrow and expands again). Taken: one model, two projections. The detail shows when
  collapsed if it is focused, and Back pops it. Expanding again loses nothing, because the
  collapsed side was a live tile all along. Left out: a platform navigator holding its own back
  stack beside the page's. Here the address history is the only one.
- **Web docking libraries** (Golden Layout, Dockview, FlexLayout). Each owns the DOM of its panels
  and keeps its own mutable layout state, updated by its own event handlers. A view that is a
  pure function of a model would then have two owners of the layout, one of which no law can
  read, so none fits. The layout is a small pure module of this repository's own (§8).

## 12. How the figures were measured

- **The lines of §0** were counted with `sed -n` over the named ranges of each file on this tree,
  as non-blank lines, by a script outside the tree. The ranges are the declarations, arms and
  rules that exist only for the pane, the panel, the pushed Back, the conversations column or the
  Typst list's second drawing of its open todo.
  Lines shared with other behaviour, such as the conversation's `About …` header or the panel's
  opening rule in `aside.shown`, are not counted, because stage b keeps them.
- **The step costs of §6** are PAGETHREAD's (§4.1 and §4.2 of that note), measured in headless
  Chromium at a CPU throttle of 4 on the `bigstore` fixture. This note adds no browser figure.
- **One typeset** (§8.1) was timed with `date +%s%N` around `typst compile --features html
  --format html` of a page of a heading, 30 paragraphs with strong, emphasis and a link, and a
  list, 21 runs in `.#triage` on the same container, after the gate had finished.
- **A program's start** (its `init`, first view, diff and patch encoding) and its longest
  rehearsed step were timed natively in a release build. A test that was not committed ran each
  page's rehearsal (`app/pages/tests/rehearsal/`) 200 times and took the median, on a cloud
  container with 4 cores. The figures are in the table below. They show a program's own start
  against the readings it waits for, and are not browser figures.

| Page | Rehearsed inputs | Start (median) | Longest step (median) | Start and every input (median) |
| --- | --- | --- | --- | --- |
| Todos (flags only) | 0 | 0.099 ms | none | 0.099 ms |
| Secretary (the stream) | 28 | 0.060 ms | 0.337 ms | 2.327 ms |
| Conversation | 25 | 0.063 ms | 0.181 ms | 2.299 ms |
| Inbox | 9 | 0.018 ms | 0.050 ms | 0.276 ms |
| Needs you | 5 | 0.003 ms | 0.011 ms | 0.039 ms |
| Settings | 7 | 0.010 ms | 0.035 ms | 0.170 ms |

## 13. Stage b: what the face gave, and what it did not

Stage b instantiates `Page` with the app's route (`app/pages/src/route.rs`). It holds the tiles in a desk (`desk.rs`),
and a host (`web/src/shell/tiles.ts`) draws what `arrange` answers. The figures of §2 changed in three ways when they
met the window:

- **The arrangement is given the room, not the window.** The tiles stand in `main`, which is the window less the
  body's gutters (1rem each side) and less the secretary's panel when it is open (22rem). `arrange` is given that
  box, read in root-font units and floored as a media query compares, so no tile is drawn under its least width.
- **Two questions, no tie.** The window's chrome (the bar across the top, the header, the panel, the add sheet's dock)
  keeps its 62rem media queries: it is placed against the window and asks the window alone. The tiles ask the room.
  A column's least width is its own token, 30rem, what a page read as a column needs; a list's is 20rem. So two tiles
  stand together wherever the room holds 824 px: from a window of 856 px with the panel closed, under a phone's
  chrome up to 991 px, and from 1,208 px with the panel open. A first version tied the two (the column's least
  width defined from the edge, 38.5rem); the review undid it, because it cost the list beside a todo at every window
  from 992 to 1,343 px with the panel open. The law that replaces the tie: **no tile is ever under a bar, in either
  chrome**: tiles under a phone's chrome end above the bar and the add sheet's docked handle, which are the
  window's (`web/e2e/tiles.test.ts` sweeps 840 to 1,010 px, with the panel and without, at two text sizes).
- **The room never depends on the document's scrollbar.** `html` keeps a classic scrollbar's gutter
  (`scrollbar-gutter: stable`), so whether the document scrolls (one tile alone) or not (tiles together) does not
  move the room, and the arrangement is not bistable near the threshold.
- **Height takes no tile away** (`Measures::tall` is zero). Every tile scrolls itself, and a tablet's keyboard or
  docked developer tools must not unmount the list beside a todo. A node down (`Below`, stage c) brings a least
  height back.

The face was enough for every layout stage b draws. What it did not give, and what stage b did instead:

- **The page of a drawn tile.** `Placed` names an `Id`, and `Tiles::page` answers an `Option`. The desk joins the two
  with `filter_map`. A `Placed` that carried its leaf's page, `Arranged<P>`, would make the join total. Stage c should
  add it with a law: every placed page is its leaf's.
- **The bar's band.** The root's row is the bar, which the window's chrome draws until stage d. The desk arranges the
  room with the bar's band above it (`foot` zero) and cuts the band off every rectangle, so the host never sees it.
- **Keeping a live tile across an address.** `parse` mints fresh ids (§5), so the host matches each new leaf to a
  live tile of the same page. §5 places this in stage d, but stage b needs it already: the list beside a todo is not
  mounted again when the todo changes. The host also keeps a tile whose page takes the move on itself (the list a
  share opens, the Typst list's open todo).
- **A conversation's column** is a list, so the conversation declares that it opens pages a list wide. Its column
  stands where `opens` is `Beside`, as the list beside a todo does.

**The host is a function of the drawing.** It keeps one element per tile, crumb row (by `Row.node`) and divider (by
`Divider.node` and the tile before it), writes a style or a row's crumbs only where they differ, and never moves a
tile's element. When a kept tile's scroller changes (alone it scrolls the document, beside another it scrolls itself),
the host scrolls the new box so what the person sees stays where it stood on the screen: what they last pressed in the
tile while it is on screen (the row clicked), else the element crossing the top of what the tile shows. Not what has
the keyboard: Safari and a tablet's tap focus no button, and a click in the todo takes the keyboard from the list. A page draws into its tile's element, with no wrapper between. What CSS still decides is the window's: when
tiles stand together the body is a column one window tall, and `main`, where the tiles stand, takes what the chrome
(the header, the banners) leaves of it.

**The notice is the window's**: `#toast` stands in the flow under the header at a page's top and sticks to the window's top, over the header's line, once the page scrolls: in view however far a lone list scrolls, and never a change to the header's height, which the page reads.

**A page left with a write in flight drains** (`web/src/pages/drain.ts`): its program keeps running, undrawn and
holding nothing, until each write has answered and each Undo it offered has expired, and performs only the write's
effects and what says how it went. That is §6's rule for a closed tile, in the page framework, and it replaces
`unshown`. It has no bound, on purpose: a write waiting offline is held by the intent set until the box answers, and
its program (one page's model, nothing drawn, nothing held) waits with it, so the answer can still be said.

**Engines.** A tile is a query container. Engines before late 2024 made such an element the containing block of its
fixed children, which would pin the add sheet, its scrim and the topic sheet to the tile. The application supports
current Chromium, Firefox and Safari; the scenes prove the sheet docked to the window only in the suite's Chromium.
Moving those layers out of the tiles is stage c's.

What stage b did not reach, on purpose, and the stage each goes in:

- **The secretary panel as the pinned leaf: stage c, after hidden tiles are kept and fed.** The panel's opening rule
  needs the conversation's talk about the page, which only a running panel can tell. Without §6's kept-and-fed hidden
  tiles a closed panel would run nothing, so the panel stays the fixed overlay. Its rule reads the window's chrome
  (`tiles.room`, the same media query as the chrome) and nothing else, and it takes its width from the room.
- **Hidden tiles kept and fed (§6): stage c,** with the panel, which is their first consumer. The host mounts only
  drawn tiles; on a phone, Back mounts the list again and restores its scroll through `routelog.Kept`, as before.
- **The conversations column as a tile: stage c,** with the open rule for links between pages, since its column is a
  list a conversation opens from. Until then it is page content that follows the tile's `opens`.
- **The Typst list's pane as a tile: stage c,** for the same reason; until then it follows its frame's container query.
- **The add sheet rising inside its tile: stage c,** once a sheet belongs to its tile (§6). Until then the sheet is
  placed against the window, so its width rules stay the window's media queries at the edge.
- **`Route` and `places::Place`** print the same hashes twice in Rust. One should become a view of the other, in
  stage d, which also deletes `parseRoute` in TypeScript.

Every page is under exactly one mechanism: the address names a leaf, the desk arranges it, and the host mounts it in
a tile. The panel, the More sheet, the add sheet and the toast are the window's chrome, not tiles, and none of them is
placed by the arrangement.
