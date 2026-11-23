//! A length divided among children by their shares, each clamped up to its least, in whole pixels
//! that sum to the length exactly.

use super::px::Px;

/// When `length` holds every least, each child gets its share of it; a child whose share is under
/// its least gets its least, and the shortfall comes from the others in proportion to what they
/// have above their least. When it does not, the length is divided in proportion to the leasts
/// (or evenly, when every least is zero). Fractions are rounded down and the pixels left go one
/// each to the largest remainders, the earlier child first on a tie.
pub(crate) fn apportion(length: Px, children: &[(Px, u32)]) -> Vec<Px> {
    let total = u128::from(length.0);
    let least: Vec<u128> = children.iter().map(|&(l, _)| u128::from(l.0)).collect();
    let share: Vec<u128> = children.iter().map(|&(_, s)| u128::from(s)).collect();
    let (leasts, shares): (u128, u128) = (least.iter().sum(), share.iter().sum());
    if total <= leasts || shares == 0 {
        let by: Vec<u128> = if leasts > 0 {
            least
        } else {
            vec![1; children.len()]
        };
        let sum = by.iter().sum();
        return round(
            length,
            &by.iter().map(|w| total * w).collect::<Vec<_>>(),
            sum,
        );
    }
    // In units of 1 / shares: each child's share of the length, and its least.
    let ideal: Vec<u128> = share.iter().map(|s| total * s).collect();
    let floor: Vec<u128> = least.iter().map(|l| l * shares).collect();
    let short = |k: usize| ideal[k] < floor[k];
    let deficit: u128 = (0..children.len())
        .filter(|&k| short(k))
        .map(|k| floor[k] - ideal[k])
        .sum();
    let excess: u128 = (0..children.len())
        .filter(|&k| !short(k))
        .map(|k| ideal[k] - floor[k])
        .sum();
    if deficit == 0 {
        return round(length, &ideal, shares);
    }
    // In units of 1 / (shares * excess); `excess > deficit` because the length holds every least,
    // and each child above its least keeps `ideal * (excess - deficit) + least * deficit`.
    let numerators: Vec<u128> = (0..children.len())
        .map(|k| {
            if short(k) {
                floor[k] * excess
            } else {
                ideal[k] * (excess - deficit) + floor[k] * deficit
            }
        })
        .collect();
    round(length, &numerators, shares * excess)
}

/// Whole pixels from `numerators / denominator`, which sum to `length`.
fn round(length: Px, numerators: &[u128], denominator: u128) -> Vec<Px> {
    let mut whole: Vec<u128> = numerators.iter().map(|n| n / denominator).collect();
    let mut left = u128::from(length.0) - whole.iter().sum::<u128>();
    let mut order: Vec<usize> = (0..numerators.len()).collect();
    order.sort_by_key(|&k| (std::cmp::Reverse(numerators[k] % denominator), k));
    for k in order {
        if left == 0 {
            break;
        }
        whole[k] += 1;
        left -= 1;
    }
    whole.into_iter().map(|w| Px::part(w, length)).collect()
}
