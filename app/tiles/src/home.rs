//! A page's chain: the pages it is opened from, up to a workspace's home, and the open that opens
//! them first.

use super::Tiles;
use super::act::{one, opened, origin, shown};
use super::page::Home;

/// The most steps a chain is followed: a host's `home` that never reaches a page answering itself
/// stops here, whatever the policy.
const GUARD: usize = 64;

/// `Act::Open` of `page` where the focused workspace does not show it already, with each page of
/// its `home` chain after the focused leaf opened first, by the same rule: an open from a
/// workspace's home of a page two steps from it inserts two tiles, the tree the page's bare
/// address reads as. A page shown already takes the focus, as `Act::Open` gives it. Each open is
/// bounded by the policy as `Act::Open` is.
#[must_use]
pub fn follow<P: Home>(tiles: Tiles<P>, page: P) -> Tiles<P> {
    if shown(&tiles, &page).is_some() {
        return opened(tiles, page);
    }
    let tiles = origin(tiles);
    let chain = homes(&page);
    match chain.iter().position(|c| c == tiles.focused().page()) {
        Some(k) => chain[..k]
            .iter()
            .rev()
            .cloned()
            .chain(std::iter::once(page))
            .fold(tiles, one),
        None => one(tiles, page),
    }
}

/// The pages `page` is opened from, nearest first, up to a workspace's home: the first page that
/// answers itself, or repeats a page already on the chain, or the `GUARD`th.
pub(crate) fn homes<P: Home>(page: &P) -> Vec<P> {
    let mut chain: Vec<P> = Vec::new();
    let mut at = page.clone();
    while chain.len() < GUARD {
        let home = at.home();
        if home == at || home == *page || chain.contains(&home) {
            break;
        }
        chain.push(home.clone());
        at = home;
    }
    chain
}
