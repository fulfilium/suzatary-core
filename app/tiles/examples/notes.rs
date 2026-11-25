//! A notes reader in three panes: the list of notes, the note opened from it, and an outline that
//! stays beside whatever note is open. It has no address, so it implements `Page` alone. Run it
//! with `cargo run -p tiles --example notes`; `cargo test -p tiles` runs it too.

use tiles::{
    Act, Arranged, At, Axis, Fit, Measures, Node, Page, Part, Pin, Policy, Px, RootRow, Shape,
    Share, Tiles, Tree, Viewport, act, arrange,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Pane {
    Notes,
    Note(u32),
    Outline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Notes,
    Note,
    Outline,
}

impl Page for Pane {
    type Kind = Kind;

    fn kind(&self) -> Kind {
        match self {
            Pane::Notes => Kind::Notes,
            Pane::Note(_) => Kind::Note,
            Pane::Outline => Kind::Outline,
        }
    }

    fn shape(&self) -> Shape {
        let least = match self {
            Pane::Notes => 280,
            Pane::Note(_) => 560,
            Pane::Outline => 240,
        };
        Shape::Flows { least: Px(least) }
    }

    fn pin(&self) -> Pin {
        match self {
            Pane::Outline => Pin::Pinned,
            Pane::Notes | Pane::Note(_) => Pin::Free,
        }
    }

    fn share(&self) -> Share {
        match self {
            Pane::Note(_) => Share::Two,
            Pane::Notes | Pane::Outline => Share::One,
        }
    }

    fn opens(&self) -> Shape {
        Pane::Note(0).shape()
    }
}

fn main() {
    // One workspace: a root of one child has nothing behind it, so it draws no crumb row.
    let home = Node::of(
        Axis::Across,
        Fit::Never,
        Part::new(Share::One, Tree::Leaf(Pane::Notes)),
    );
    let tiles = Tiles::new(home, Policy::default()).expect("one list is a well-formed home");
    let tiles = act(tiles, Act::Open(Pane::Note(1)));
    let tiles = act(tiles, Act::Hold(Pane::Outline));
    // A note opened from a note replaces it: pages of one kind open in place.
    let tiles = act(tiles, Act::Open(Pane::Note(2)));
    let m = Measures {
        divider: Px(16),
        row: Px(40),
        crumb: Px(0),
        tall: Px(0),
        root_row: RootRow::Top,
    };
    let wide = arrange(&tiles, viewport(1280, 800), &m);
    let narrow = arrange(&tiles, viewport(390, 844), &m);
    show(&wide);
    show(&narrow);

    // Wide: the list, the note and the outline side by side, with no crumb row.
    let pages: Vec<&Pane> = wide.tiles.iter().map(|t| &t.page).collect();
    assert_eq!(pages, [&Pane::Notes, &Pane::Note(2), &Pane::Outline]);
    assert!(wide.rows.is_empty());
    // Narrow: the focused note alone, with the list before it and the outline after it as crumbs.
    let pages: Vec<&Pane> = narrow.tiles.iter().map(|t| &t.page).collect();
    assert_eq!(pages, [&Pane::Note(2)]);
    let crumbs: Vec<(&Pane, At)> = narrow
        .rows
        .iter()
        .flat_map(|r| r.shown.iter().chain(&r.more))
        .map(|c| (&c.page, c.at))
        .collect();
    assert_eq!(
        crumbs,
        [
            (&Pane::Notes, At::Before),
            (&Pane::Note(2), At::Drawn),
            (&Pane::Outline, At::After)
        ]
    );
}

fn viewport(w: u32, h: u32) -> Viewport {
    Viewport { w: Px(w), h: Px(h) }
}

#[test]
fn the_three_panes_arrange_as_main_says() {
    main();
}

fn show(a: &Arranged<Pane>) {
    println!("{} tile(s), {} crumb row(s):", a.tiles.len(), a.rows.len());
    for t in &a.tiles {
        let r = t.rect;
        println!(
            "  {:?} at ({}, {}), {} by {}, opens {:?}",
            t.page, r.x.0, r.y.0, r.w.0, r.h.0, t.opens
        );
    }
    for d in &a.dividers {
        println!("  a divider at x {}, {} wide", d.rect.x.0, d.rect.w.0);
    }
    for row in &a.rows {
        let crumbs: Vec<String> = row
            .shown
            .iter()
            .chain(&row.more)
            .map(|c| format!("{:?} {:?}", c.page, c.at))
            .collect();
        println!("  crumbs: {}", crumbs.join(", "));
    }
}
