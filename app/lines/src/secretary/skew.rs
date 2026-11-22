//! How far a device's clock may be from the box's, and what the lines' rule says of a line dated
//! further off: the bounds and the words the rule refuses or closes by, which a page says before
//! Send in the same words.

use crate::secretary::transcript::ANSWERED_WITHIN;

/// How far ahead of the box's clock a tab's line may be dated.
pub const MAX_AHEAD_SECONDS: i64 = 300;

/// A device's clock outside the bounds the box takes its lines by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skew {
    /// More than [`MAX_AHEAD_SECONDS`] ahead: a line is refused for now, and posted again once
    /// the box's clock reaches it.
    Ahead { seconds: i64 },
    /// Behind by more than [`ANSWERED_WITHIN`]: a question is taken already past the hour the box
    /// answers within, so the box closes it unanswered.
    Behind { seconds: i64 },
}

impl Skew {
    /// A clock `ahead` seconds ahead of the box's (behind where negative); none within the bounds.
    #[must_use]
    pub fn of(ahead: i64) -> Option<Self> {
        if ahead > MAX_AHEAD_SECONDS {
            Some(Skew::Ahead { seconds: ahead })
        } else if -ahead > ANSWERED_WITHIN.num_seconds() {
            Some(Skew::Behind { seconds: -ahead })
        } else {
            None
        }
    }

    /// The rule's words for a line dated by this clock.
    #[must_use]
    pub fn said(self) -> String {
        match self {
            Skew::Ahead { seconds } => format!(
                "it is dated {seconds} s ahead of the box's clock, more than {MAX_AHEAD_SECONDS} s: it is posted again then"
            ),
            Skew::Behind { seconds } => format!(
                "it is dated {seconds} s behind the box's clock, more than the {} min the box answers a question within: the box closes it unanswered",
                ANSWERED_WITHIN.num_minutes()
            ),
        }
    }
}
