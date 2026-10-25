//! The size loop: ask, and while the answer is no line of at most [`NODE_BYTES`], show the
//! compactor its answer cut at the limit and ask again, at most [`TRIES`] times.
//!
//! The first answer that fits is kept, the shortest that fits since each retry asks for less. If
//! none fits, the attempt is [`Outcome::Unfit`] and says why: a summary is never an answer cut
//! short and stored as though it had been said. Each call is first handed to `before`, which
//! records it durably and may refuse it, so a call that then fails was still counted.

use crate::capability::Compact;
use crate::refusal::Refusal;
use crate::secretary::compact_ask::{Ask, Unfit};
use crate::secretary::digest_line::{Line, NODE_BYTES, NotALine};

/// The most calls one attempt makes.
pub const TRIES: u32 = 5;

/// What an attempt came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A line that fits.
    Fits {
        /// The summary.
        line: Line,
    },
    /// No answer fitted.
    Unfit {
        /// The shortest answer's length, its spaces joined.
        shortest: u32,
    },
}

/// One attempt at `ask`, each call of [`Ask::bytes`] first handed to `before`. An answer's runs
/// of space, line breaks included, are joined into one space before it is read as a line.
///
/// # Errors
/// `before`'s refusal, which makes no call; [`Refusal::unprocessable`] for a call that failed,
/// which `before` has already counted.
pub fn compact(
    compactor: &dyn Compact,
    ask: Ask,
    before: &mut dyn FnMut(u64) -> Result<(), Refusal>,
) -> Result<Outcome, Refusal> {
    let mut ask = ask;
    let mut shortest = usize::MAX;
    for _ in 0..TRIES {
        before(ask.bytes())?;
        let words = compactor
            .compact(&ask)
            .map_err(|failed| Refusal::unprocessable(format!("the compactor: {failed}")))?
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        shortest = shortest.min(words.len());
        let unfit = match Line::new(&words) {
            Ok(line) => return Ok(Outcome::Fits { line }),
            Err(NotALine::TooLong(_)) => Unfit::Long {
                bytes: words.len(),
                cut: words[..words.floor_char_boundary(NODE_BYTES)].to_owned(),
            },
            Err(NotALine::Empty | NotALine::LineBreak | NotALine::Unscrubbable(_)) => {
                Unfit::NotALine
            }
        };
        ask = ask.tried(words, unfit);
    }
    Ok(Outcome::Unfit {
        shortest: u32::try_from(shortest).unwrap_or(u32::MAX),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::capability::ModelError;
    use crate::model_name::ModelName;

    /// Answers of the given lengths in turn, and keeps every question.
    struct Lengths {
        name: ModelName,
        answers: Mutex<Vec<usize>>,
        asked: Mutex<Vec<Ask>>,
    }

    impl Lengths {
        fn new(lengths: &[usize]) -> Self {
            Self {
                name: ModelName::parse("lengths").expect("a model"),
                answers: Mutex::new(lengths.iter().rev().copied().collect()),
                asked: Mutex::new(Vec::new()),
            }
        }
    }

    impl Compact for Lengths {
        fn model(&self) -> &ModelName {
            &self.name
        }

        fn compact(&self, ask: &Ask) -> Result<String, ModelError> {
            self.asked.lock().expect("held").push(ask.clone());
            let length = self
                .answers
                .lock()
                .expect("held")
                .pop()
                .ok_or_else(|| ModelError::Failed("no answer left".to_owned()))?;
            Ok("w".repeat(length))
        }
    }

    fn ask() -> Ask {
        Ask::new(Vec::new(), vec!["Owner: hi".to_owned()])
    }

    /// Every call's size, as `before` was handed them.
    fn counted(compactor: &Lengths) -> (Result<Outcome, Refusal>, Vec<u64>) {
        let mut sent = Vec::new();
        let outcome = compact(compactor, ask(), &mut |bytes| {
            sent.push(bytes);
            Ok(())
        });
        (outcome, sent)
    }

    #[test]
    fn a_long_answer_is_shown_cut_and_asked_again_until_one_fits() {
        let compactor = Lengths::new(&[600, 900, 300]);
        let (outcome, sent) = counted(&compactor);
        let Ok(Outcome::Fits { line }) = outcome else {
            panic!("the third fits");
        };
        assert_eq!(line.as_str().len(), 300);
        assert_eq!(sent.len(), 3);
        let asked = compactor.asked.lock().expect("held");
        assert_eq!(asked[2].tries().len(), 2);
        assert_eq!(
            asked[1].tries()[0].unfit,
            Unfit::Long {
                bytes: 600,
                cut: "w".repeat(NODE_BYTES)
            }
        );
        let asked: Vec<u64> = asked.iter().map(Ask::bytes).collect();
        assert_eq!(
            sent, asked,
            "every call's whole question is counted before it is made"
        );
    }

    #[test]
    fn five_long_answers_are_unfit_and_nothing_is_cut_into_a_summary() {
        let compactor = Lengths::new(&[700, 650, 520, 900, 800, 1]);
        let (outcome, sent) = counted(&compactor);
        assert!(
            matches!(outcome, Ok(Outcome::Unfit { shortest: 520 })),
            "{outcome:?}"
        );
        assert_eq!(sent.len(), usize::try_from(TRIES).expect("small"));
    }

    #[test]
    fn an_empty_answer_is_asked_again_and_a_failed_call_was_counted_first() {
        let (outcome, sent) = counted(&Lengths::new(&[0, 40]));
        assert!(matches!(outcome, Ok(Outcome::Fits { .. })));
        assert_eq!(sent.len(), 2);
        let (outcome, sent) = counted(&Lengths::new(&[600]));
        assert!(outcome.is_err(), "the second call fails");
        assert_eq!(sent.len(), 2, "and it was counted before it was made");
        let refused = compact(&Lengths::new(&[1]), ask(), &mut |_| {
            Err(Refusal::conflict("held".to_owned()))
        });
        assert!(refused.is_err(), "a refused call is not made");
    }
}
