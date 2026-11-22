//! A reading as a land weighs it.

use crate::reading::Judged;
use crate::{Half, Outcome, Reading};

/// The chain a land reads, green < unjudged < red < finding. A train's verdict is the greatest of
/// its halves' readings in it, so one half's green never hides the other half's silence.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum LandOrder {
    Green,
    Unjudged,
    Red,
    Finding,
}

impl LandOrder {
    #[must_use]
    pub fn of(reading: Reading) -> LandOrder {
        match reading {
            Reading::Agreed(Outcome::Green) => LandOrder::Green,
            Reading::Unjudged => LandOrder::Unjudged,
            Reading::Agreed(Outcome::Red) => LandOrder::Red,
            Reading::Finding => LandOrder::Finding,
        }
    }

    /// The verdict on a train: the reading of its two halves that weighs more.
    #[must_use]
    pub fn verdict([tree, live]: [Reading; 2]) -> Reading {
        if LandOrder::of(live) > LandOrder::of(tree) {
            live
        } else {
            tree
        }
    }
}

pub struct Land {
    pub tree: Judged,
    pub live: Judged,
}

impl Land {
    #[must_use]
    pub fn verdict(&self) -> Reading {
        LandOrder::verdict([self.tree.reading, self.live.reading])
    }

    #[must_use]
    pub fn record(&self) -> Vec<String> {
        let mut said = Vec::new();
        for (half, judged) in [(Half::Tree, &self.tree), (Half::Live, &self.live)] {
            for aside in &judged.set_aside {
                said.push(format!(
                    "the {} half's red of {} at {} ({}) was set aside at {} because {}: {}",
                    half.name(),
                    aside.commit.short(),
                    aside.red_at,
                    aside.beside,
                    aside.at,
                    aside.task,
                    aside.words
                ));
            }
        }
        for red in &self.live.lapsed {
            said.push(format!(
                "the live half's red of {} at {} ({}) stands, and a green came after the stores moved",
                red.commit.short(),
                red.at,
                red.beside
            ));
        }
        said
    }

    #[must_use]
    pub fn refusal(&self, short: &str) -> Option<String> {
        let missing: Vec<String> = [(Half::Tree, &self.tree), (Half::Live, &self.live)]
            .into_iter()
            .filter(|(_, judged)| judged.reading != Reading::Agreed(Outcome::Green))
            .map(|(half, judged)| format!("{}: {}", half.name(), judged.reading))
            .collect();
        let missing = missing.join(", ");
        let aside = "a red whose cause is outside the tree is set aside at a terminal: \
                     scripts/train.sh set-aside <half> --because <task> \"<words>\"";
        match LandOrder::of(self.verdict()) {
            LandOrder::Green => None,
            LandOrder::Unjudged => Some(format!(
                "the gate has not passed the train's HEAD ({short}) in both halves ({missing}): \
                 scripts/train.sh gate first. Nothing landed."
            )),
            LandOrder::Red => Some(format!(
                "the gate failed on the train's HEAD ({short}) ({missing}): scripts/train.sh \
                 bisect, build without the merge it names, gate again; or {aside}. Nothing landed."
            )),
            LandOrder::Finding => Some(format!(
                "the gate both passed and failed the train's tree ({short}) at the same stores \
                 ({missing}), and no person set the red aside, so neither verdict lands: it read \
                 something beyond the tree; {aside}. Nothing landed."
            )),
        }
    }
}
