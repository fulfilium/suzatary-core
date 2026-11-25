//! The design note's worked cases (§2, §5 and §8), with its numbers.

use tiles::{
    Act, Address, Arranged, At, Axis, Fit, Id, Leaf, Node, Part, Policy, Px, Rect, Share, Tiles,
    Tree, Viewport, act, arrange, follow, parse, print,
};

use super::page::{Kind, MEASURES as M, Pg, pg};

fn at(w: u32, h: u32) -> Viewport {
    Viewport { w: Px(w), h: Px(h) }
}

fn rect(x: u32, y: u32, w: u32, h: u32) -> Rect {
    Rect {
        x: Px(x),
        y: Px(y),
        w: Px(w),
        h: Px(h),
    }
}

fn leaf(p: Pg) -> Tree<Pg> {
    Tree::Leaf(p)
}

fn part(share: Share, tree: Tree<Pg>) -> Part<Pg> {
    Part::new(share, tree)
}

fn node(axis: Axis, before: Vec<Part<Pg>>, focus: Part<Pg>, after: Vec<Part<Pg>>) -> Node<Pg> {
    Node {
        axis,
        fit: Fit::Fits,
        before,
        focus: Box::new(focus),
        after,
    }
}

/// The root of eight workspaces, the first of which is `first`; the others are lists.
fn root(first: Tree<Pg>) -> Node<Pg> {
    let rest = (1..8)
        .map(|n| part(Share::ONE, leaf(pg(Kind::List, n))))
        .collect();
    Node {
        axis: Axis::Across,
        fit: Fit::Never,
        before: Vec::new(),
        focus: Box::new(part(Share::ONE, first)),
        after: rest,
    }
}

fn id(t: &Tiles<Pg>, p: &Pg) -> Id {
    t.root()
        .parts()
        .flat_map(|q| q.tree.leaves())
        .find(|l| l.page() == p)
        .map(Leaf::id)
        .expect("the page is in the tree")
}

fn tile(a: &Arranged<Pg>, id: Id) -> Option<Rect> {
    a.tiles.iter().find(|t| t.id == id).map(|t| t.rect)
}

fn new(home: Node<Pg>) -> Tiles<Pg> {
    Tiles::new(home, Policy::default()).expect("the case's home is well formed")
}

/// The root's row: the bar.
fn bar(a: &Arranged<Pg>) -> &tiles::Row<Pg> {
    a.rows
        .iter()
        .find(|r| r.node.is_empty())
        .expect("the root draws its row")
}

/// The rows below the root's, as crumbs: where each child stands, and the leaf it names.
fn crumbs(a: &Arranged<Pg>) -> Vec<Vec<(At, Id)>> {
    a.rows
        .iter()
        .filter(|r| !r.node.is_empty())
        .map(|r| {
            r.shown
                .iter()
                .chain(&r.more)
                .map(|c| (c.at, c.to))
                .collect()
        })
        .collect()
}

fn list() -> Pg {
    pg(Kind::List, 0)
}
fn todo() -> Pg {
    pg(Kind::Column, 0)
}
fn side() -> Pg {
    pg(Kind::Side, 0)
}

/// The Todos workspace: the list (One), a todo (Two) and the pinned side page (One), focused on
/// the todo.
fn three() -> Tiles<Pg> {
    let ws = node(
        Axis::Across,
        vec![part(Share::ONE, leaf(list()))],
        part(Share::TWO, leaf(todo())),
        vec![part(Share::ONE, leaf(side()))],
    );
    new(root(Tree::Node(ws)))
}

#[test]
fn at_360_px_the_todo_is_drawn_alone_with_both_neighbours_as_crumbs() {
    let t = three();
    let a = arrange(&t, at(360, 740), &M);
    assert_eq!(a.tiles.len(), 1);
    assert_eq!(tile(&a, id(&t, &todo())), Some(rect(0, 44, 360, 652)));
    assert_eq!(
        crumbs(&a),
        vec![vec![
            (At::Before, id(&t, &list())),
            (At::Drawn, id(&t, &todo())),
            (At::After, id(&t, &side()))
        ]]
    );
    // The bar: at the foot, four destinations and the overflow.
    assert_eq!(bar(&a).rect, rect(0, 696, 360, 44));
    assert_eq!((bar(&a).shown.len(), bar(&a).more.len()), (4, 4));
    // Tapping the list's crumb focuses it, and the row then names the todo after it.
    let back = act(t.clone(), Act::Focus(id(&t, &list())));
    let b = arrange(&back, at(360, 740), &M);
    assert_eq!(
        crumbs(&b),
        vec![vec![
            (At::Drawn, id(&t, &list())),
            (At::After, id(&t, &todo())),
            (At::After, id(&t, &side()))
        ]]
    );
}

#[test]
fn at_900_px_the_todo_is_still_drawn_alone() {
    let t = three();
    let a = arrange(&t, at(900, 800), &M);
    assert_eq!(a.tiles.len(), 1);
    assert_eq!(tile(&a, id(&t, &todo())), Some(rect(0, 44, 900, 712)));
    assert_eq!(crumbs(&a)[0].len(), 3);
}

#[test]
fn at_1440_px_all_three_are_drawn_and_the_side_page_is_clamped_to_its_least() {
    let t = three();
    let a = arrange(&t, at(1440, 800), &M);
    assert_eq!(a.rows.len(), 1, "the bar alone: the Todos row is empty");
    assert_eq!(bar(&a).rect, rect(0, 0, 1440, 44));
    assert_eq!(bar(&a).shown.len(), 8);
    assert_eq!(tile(&a, id(&t, &list())), Some(rect(0, 44, 347, 756)));
    assert_eq!(tile(&a, id(&t, &todo())), Some(rect(371, 44, 693, 756)));
    assert_eq!(tile(&a, id(&t, &side())), Some(rect(1088, 44, 352, 756)));
    assert_eq!(a.dividers.len(), 2);
}

/// Three lists one above the other, made with `Below`, focused on the middle one.
fn stacked() -> (Tiles<Pg>, [Pg; 3]) {
    let [inbox, mail, todos] = [pg(Kind::List, 10), pg(Kind::List, 11), pg(Kind::List, 12)];
    let ws = node(
        Axis::Down,
        vec![part(Share::ONE, leaf(inbox.clone()))],
        part(Share::ONE, leaf(mail.clone())),
        vec![part(Share::ONE, leaf(todos.clone()))],
    );
    (new(root(Tree::Node(ws))), [inbox, mail, todos])
}

#[test]
fn three_lists_stack_on_a_portrait_tablet_and_collapse_on_a_landscape_phone() {
    let (t, [inbox, mail, todos]) = stacked();
    let a = arrange(&t, at(800, 1280), &M);
    // 1,280 less the bar (44) and two dividers (48) leaves 1,188: 396 each.
    assert_eq!(tile(&a, id(&t, &inbox)), Some(rect(0, 0, 800, 396)));
    assert_eq!(tile(&a, id(&t, &mail)), Some(rect(0, 420, 800, 396)));
    assert_eq!(tile(&a, id(&t, &todos)), Some(rect(0, 840, 800, 396)));
    let b = arrange(&t, at(740, 360), &M);
    assert_eq!(b.tiles.len(), 1);
    assert_eq!(tile(&b, id(&t, &mail)), Some(rect(0, 44, 740, 272)));
    assert_eq!(
        crumbs(&b),
        vec![vec![
            (At::Before, id(&t, &inbox)),
            (At::Drawn, id(&t, &mail)),
            (At::After, id(&t, &todos))
        ]]
    );
}

#[test]
fn a_split_down_inside_a_split_across() {
    let message = pg(Kind::Column, 1);
    let down = node(
        Axis::Down,
        Vec::new(),
        part(Share::ONE, leaf(todo())),
        vec![part(Share::ONE, leaf(message.clone()))],
    );
    let ws = node(
        Axis::Across,
        vec![part(Share::ONE, leaf(list()))],
        part(Share::TWO, Tree::Node(down)),
        Vec::new(),
    );
    let t = new(root(Tree::Node(ws)));
    let a = arrange(&t, at(900, 800), &M);
    assert_eq!(tile(&a, id(&t, &todo())), Some(rect(0, 44, 900, 344)));
    assert_eq!(tile(&a, id(&t, &message)), Some(rect(0, 412, 900, 344)));
    assert_eq!(tile(&a, id(&t, &list())), None);
    assert_eq!(
        crumbs(&a),
        vec![vec![
            (At::Before, id(&t, &list())),
            (At::Drawn, id(&t, &todo()))
        ]]
    );
    let b = arrange(&t, at(1440, 800), &M);
    assert_eq!(tile(&b, id(&t, &list())), Some(rect(0, 44, 472, 756)));
    assert_eq!(tile(&b, id(&t, &todo())), Some(rect(496, 44, 944, 366)));
    assert_eq!(tile(&b, id(&t, &message)), Some(rect(496, 434, 944, 366)));
}

/// The home: eight workspaces of one list each.
fn home() -> Tiles<Pg> {
    new(root(leaf(list())))
}

#[test]
fn a_bare_address_is_the_home_with_its_page_opened_from_it() {
    let t = parse(&todo().print(), &home());
    assert_eq!(t.root().index(), 0);
    assert_eq!(t.focused().page(), &todo());
    assert_eq!(print(&t), todo().print());
    let phone = arrange(&t, at(360, 740), &M);
    assert_eq!(
        crumbs(&phone),
        vec![vec![
            (At::Before, id(&t, &list())),
            (At::Drawn, id(&t, &todo()))
        ]]
    );
    let desk = arrange(&t, at(1440, 800), &M);
    assert_eq!(desk.tiles.len(), 2);
    // A workspace's home prints as its page's address alone.
    let inbox = act(home(), Act::Focus(id(&home(), &pg(Kind::List, 3))));
    assert_eq!(print(&inbox), "list/3");
}

/// The leaves of the focused workspace, in order.
fn shown(t: &Tiles<Pg>) -> Vec<Pg> {
    t.root()
        .focus
        .tree
        .leaves()
        .into_iter()
        .map(|l| l.page().clone())
        .collect()
}

#[test]
fn an_open_from_a_page_on_the_chain_opens_the_rest_of_the_chain_first() {
    let slide = pg(Kind::Slide, 1);
    // From the list, the slide's todo opens beside it and the slide after: the tree its bare address reads as.
    let opened = follow(home(), slide.clone());
    assert_eq!(
        shown(&opened),
        vec![list(), pg(Kind::Column, 1), slide.clone()]
    );
    assert_eq!(shown(&parse(&slide.print(), &home())), shown(&opened));
    assert_eq!(print(&opened), slide.print());
    // From another todo, which is on no chain of the slide's, the slide opens beside it.
    let other = follow(home(), pg(Kind::Column, 2));
    let from_todo = follow(other, slide.clone());
    assert_eq!(
        shown(&from_todo),
        vec![list(), pg(Kind::Column, 2), slide.clone()]
    );
    assert_ne!(print(&from_todo), slide.print());
}

#[test]
fn the_1440_px_tree_prints_with_its_other_workspaces_left_out() {
    let t = follow(home(), todo());
    let t = follow(t, side());
    let t = act(t.clone(), Act::Focus(id(&t, &todo())));
    assert_eq!(
        print(&t),
        "0todo-(~*)~|!(|(list%2F0,2*,side%270%2C%5E%7C%21%3A),,,,,,,)"
    );
}

/// A slide deck on the same module: the presenter's three tiles, and the audience's one.
#[test]
fn a_slide_deck_arranges_its_presenter_and_its_audience() {
    let (slide, notes, next) = (pg(Kind::Slide, 0), pg(Kind::List, 0), pg(Kind::Slide, 1));
    let column = node(
        Axis::Down,
        Vec::new(),
        part(Share::ONE, leaf(notes.clone())),
        vec![part(Share::ONE, leaf(next.clone()))],
    );
    let deck = node(
        Axis::Across,
        Vec::new(),
        part(Share::TWO, leaf(slide.clone())),
        vec![part(Share::ONE, Tree::Node(column))],
    );
    let presenter = new(Node::of(
        Axis::Across,
        Fit::Never,
        part(Share::ONE, Tree::Node(deck)),
    ));
    let a = arrange(&presenter, at(1920, 1080), &M);
    assert!(a.rows.is_empty(), "a root of one child draws no row");
    assert_eq!(a.dividers.len(), 2);
    let placed = |p: &Pg| {
        a.tiles
            .iter()
            .find(|t| t.id == id(&presenter, p))
            .map(|t| (t.rect, t.fill))
            .expect("drawn")
    };
    assert_eq!(
        placed(&slide),
        (rect(0, 0, 1264, 1080), rect(0, 184, 1264, 711))
    );
    assert_eq!(
        placed(&notes),
        (rect(1288, 0, 632, 528), rect(1288, 0, 632, 528))
    );
    assert_eq!(
        placed(&next),
        (rect(1288, 552, 632, 528), rect(1288, 638, 632, 355))
    );

    let audience = new(Node::of(
        Axis::Across,
        Fit::Never,
        part(Share::ONE, leaf(slide.clone())),
    ));
    let b = arrange(&audience, at(1600, 1200), &M);
    assert!(b.rows.is_empty() && b.dividers.is_empty());
    assert_eq!(
        (b.tiles[0].rect, b.tiles[0].fill),
        (rect(0, 0, 1600, 1200), rect(0, 150, 1600, 900))
    );
}

/// A weight prints as its decimal digits before the child, and one prints nothing, so every
/// address of a weight of one to three reads as it did. A weight of two is the todo's own here,
/// which the bare address reads, so it prints bare.
#[test]
fn a_weight_prints_in_decimal_and_one_prints_nothing() {
    let (list, todo) = (list(), todo());
    let t = act(home(), Act::Open(todo.clone()));
    let share = |w: u8| Share::of(w).expect("a weight from one");
    for (weight, printed) in [(1, ""), (3, "3"), (12, "12"), (255, "255")] {
        let resized = act(
            t.clone(),
            Act::Resize {
                before: id(&t, &list),
                after: id(&t, &todo),
                shares: [share(1), share(weight)],
            },
        );
        let s = print(&resized);
        assert!(
            s.ends_with(&format!("(list%2F0,{printed}*),,,,,,,)")),
            "{s}"
        );
        assert_eq!(
            parse(&s, &home()).root().map(&Leaf::page),
            resized.root().map(&Leaf::page)
        );
    }
    assert_eq!(print(&t), todo.print());
}

/// The weights the focused workspace's node gives its children, in the tree `address` reads.
fn weights(address: &str) -> Vec<u8> {
    match &parse(address, &home()).root().focus.tree {
        Tree::Node(n) => n.parts().map(|p| p.share.weight()).collect(),
        Tree::Leaf(_) => Vec::new(),
    }
}

/// A weight is the longest run of digits that starts with 1 to 9: 2 to 11 read and print as
/// written, and 256 and 300 are no share, so the address is no tree and reads as its head alone. A
/// share that starts with a zero is no share either, and the digits that follow it are no page's
/// print. A hand-written `21` is twenty-one, never two and then a one.
#[test]
fn a_weight_reads_as_its_decimal_run_and_a_run_past_255_or_after_a_zero_is_no_tree() {
    for weight in 2..=11 {
        let s = format!("list/1~|!(|(list%2F0,{weight}*),,,,,,,)");
        assert_eq!(weights(&s), [1, weight], "{s}");
        assert_eq!(print(&parse(&s, &home())), s);
    }
    assert_eq!(weights("list/1~|!(|(list%2F0,21*),,,,,,,)"), [1, 21]);
    let head = weights("list/1");
    for refused in ["256", "300", "0", "00", "01"] {
        let s = format!("list/1~|!(|(list%2F0,{refused}*),,,,,,,)");
        assert_eq!(weights(&s), head, "{s}");
    }
}
