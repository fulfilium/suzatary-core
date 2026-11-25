# tiles

`tiles` lays pages out as one tree of tiles, as a tiling window manager does. A host changes the
tree only by acts, and no act reads the screen. `arrange` is a function of the tree and a viewport:
it answers which tiles are drawn, where, and which stand behind a crumb. On a wide viewport a page
opens beside the one it came from; on a narrow one the same tree draws it alone, with a crumb back.
The crate depends on nothing but the standard library and draws nothing itself.

Run `cargo run -p tiles --example notes` to see three panes at a wide and a narrow viewport.

## Start

```rust
let home = Node::of(Axis::Across, Fit::Never, Part::new(Share::One, Tree::Leaf(Pane::Notes)));
let tiles = Tiles::new(home, Policy::default())?;
let tiles = act(tiles, Act::Open(Pane::Note(1)));
let drawn = arrange(&tiles, Viewport { w: Px(1280), h: Px(800) }, &measures);
```

`Pane` is the host's type and implements `Page`. The root's children are the workspaces; a root
of one child has one workspace and draws no crumb row of its own.

## The model

- **A page** implements `Page`: its kind (an open of a page of the focused page's kind replaces
  it), its `Shape` (a least width, and a ratio for a page typeset to a fixed size), its `Pin`, the
  `Share` an open gives it, and the shape of the pages it opens.
- **A tree** (`Tree`) is a leaf or a `Node`. A node has an `Axis` (`Across` or `Down`), a `Fit`
  and its children as a zipper: the focused child is the hole, so a focus that names no child
  cannot be written. Each child has a `Share` of one, two or three.
- **A fit** is declared, not computed. A `Fits` node draws as many children side by side as fit
  and stacks the rest behind; a `Never` node draws its focused child alone, as tabs do.
- **`Tiles`** holds the root, the home it started from, the `Policy`, and the counter that gives
  each opened page a fresh `Id`. A host keeps a page's running state by that `Id`.
- **A pinned leaf** stands at its workspace's end and stays when the pages beside it are replaced.
- **`Policy`** bounds the free tiles of a workspace (`most`) and says what an open beside does
  with the tiles after the focused one: `Beside::Replace` closes them, as Miller columns do, and
  `Beside::Insert` keeps them. At the bound, either rule replaces the focused leaf. A bound of one
  opens every page in place. `Policy::default()` is `Replace` at the bound this crate's first host
  uses.

## The acts

`act(tiles, Act) -> Tiles` is total and keeps the tree well formed (`Tiles::check`):

- `Open(page)`: a page shown already is focused; one of the focused leaf's kind replaces it; a
  pinned page docks at the end; any other opens beside the focused leaf, by the policy.
- `Below(page)`: the focused leaf becomes a node down, with the page below it.
- `Close`, `Focus(id)`, `Move(dir)`, `Resize { before, after, shares }`.
- `Hold(page)` and `Release`: a host docks a pinned page at the focused workspace's end, or lets it
  go, without moving the focus or changing any act of a person's.

## The arrangement

```text
arrange(tiles, viewport, measures) = place(walk(tiles.root, viewport, measures), viewport)
```

`walk` starts at the focused leaf and climbs its ancestors. At each `Fits` node it adds the
children after the drawn region, then the ones before, nearest first, while the region still fits
the viewport. `place` gives a node with children behind a crumb row and divides the rest by the
shares, each child at least its least size. The answer (`Arranged`) holds the drawn tiles
(`Placed`: a rectangle, a page and its `Opens`), the dividers with their `Stop`s, and the crumb
rows. Together they cover the viewport exactly. `Measures` holds the lengths a host chooses, and
`RootRow` says where the root's crumb row stands.

## Three traits

- `Page` is all that `act` and `arrange` need.
- `Home: Page` adds the page a page is opened from; `follow(tiles, page)` opens that chain first.
- `Address: Home` adds a printed form; `print` and `parse` then keep the whole tree in a string.

## The laws

The laws are properties over generated trees, policies, acts and viewports, in `tests/laws/`.
Each law holds for every policy and every measures. Two state where they apply in terms of the
bound: an open that inserts can be undone by `Close` only where the bound leaves room, and an open
along a chain closes back only where the bound holds the chain.

## What it does not do

No floating point, no floating windows, no drag targets, and no drawing of its own: a host draws
what `arrange` answers. Shares are one, two or three, and an open places a page beside the focused
leaf, never at a place a host names.

## Develop

`cargo test -p tiles` runs the laws and the example. The laws run over sequences of acts; after a
change, run them once with `PROPTEST_CASES=20000`.
