//! The search for the merge that turned a half red.

use crate::{Outcome, Reading};

/// Prefixes of a train's merges, `lo` known green and `hi` known red; the first `n` merges are
/// prefix `n`, and prefix 0 is master.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Search {
    pub lo: usize,
    pub hi: usize,
}

/// Why a search stops before its answer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stuck {
    /// The probe of this prefix gave no verdict.
    Unjudged(usize),
    /// The table holds both green and red for this prefix's tree.
    Finding(usize),
}

impl Search {
    /// The probes a search over `n` merges takes at most: ⌈log₂ n⌉.
    #[must_use]
    pub fn most(n: usize) -> u32 {
        n.next_power_of_two().trailing_zeros()
    }

    /// The least red prefix, assuming red stays red as merges are added. `read` gives the table's
    /// reading of a prefix in `on`; `probe` gates it, and is asked only for a prefix the table has not
    /// judged. The error is `probe`'s, or where the search is stuck.
    pub fn run<C, E: From<Stuck>>(
        mut self,
        on: &mut C,
        read: impl Fn(&mut C, usize) -> Result<Reading, E>,
        probe: impl Fn(&mut C, usize) -> Result<(), E>,
    ) -> Result<Search, E> {
        while self.hi - self.lo > 1 {
            let mid = usize::midpoint(self.lo, self.hi);
            let reading = match read(on, mid)? {
                Reading::Unjudged => {
                    probe(on, mid)?;
                    read(on, mid)?
                }
                reading => reading,
            };
            match reading {
                Reading::Agreed(Outcome::Green) => self.lo = mid,
                Reading::Agreed(Outcome::Red) => self.hi = mid,
                Reading::Unjudged => return Err(Stuck::Unjudged(mid).into()),
                Reading::Finding => return Err(Stuck::Finding(mid).into()),
            }
        }
        Ok(self)
    }
}
