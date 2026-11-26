//! The size loop: ask, and while the answer is no line of at most `NODE_BYTES`, tell the
//! compactor how its answers did not fit and ask again, at most [`TRIES`] times.
//!
//! A summary is never an answer cut short and stored as though it had been said: a line cut at
//! its end can drop what settled or corrected what came before it. Each call is recorded
//! durably before it is made, so a call that then fails was still counted.

use crate::secretary::compact::Compact;
use crate::secretary::compact_ask::{Ask, Unfit};
use crate::secretary::digest_line::{Line, NODE_BYTES, NotALine};
use suzatary_base::call_log::{Call, Journal};
use suzatary_base::refusal::Refusal;
use suzatary_conversations::secretary::model::ModelError;

/// The most calls one attempt makes.
pub const TRIES: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A line that fits.
    Fits { line: Line },
    Unfit {
        /// The shortest answer's length, its spaces joined; 0 when the model answered nothing.
        shortest: u32,
    },
}

/// What a failed call says about its node.
enum Failed {
    /// The question is longer than the model reads: every try of it fails the same way.
    TheQuestion,
    /// Anything else (an outage, a limit, a timeout, a credential, a stop): the node waits, and
    /// nothing is written of it.
    Delays(ModelError),
}

impl From<ModelError> for Failed {
    fn from(error: ModelError) -> Self {
        match error {
            ModelError::TooLong(_) => Failed::TheQuestion,
            ModelError::Unreachable(_)
            | ModelError::Failed(_)
            | ModelError::Refused(_)
            | ModelError::TimedOut(_)
            | ModelError::Limited(_)
            | ModelError::Overloaded(_)
            | ModelError::Halted => Failed::Delays(error),
        }
    }
}

/// One attempt at `ask`, each call of [`Ask::bytes`] first handed to `before`. An answer's runs
/// of space, line breaks included, are joined into one space before it is read as a line. A
/// question longer than the model reads is unfit for that model, so the ladder's next is asked.
///
/// # Errors
/// `before`'s refusal, which makes no call; [`Refusal::unprocessable`] for a call that failed
/// for any other cause, which `before` has already counted and which writes nothing.
pub fn compact(
    compactor: &dyn Compact,
    journal: &dyn Journal,
    ask: Ask,
    before: &mut dyn FnMut(u64) -> Result<(), Refusal>,
) -> Result<Outcome, Refusal> {
    let mut ask = ask;
    let mut shortest = None::<usize>;
    let unfit = |shortest: Option<usize>| Outcome::Unfit {
        shortest: u32::try_from(shortest.unwrap_or(0)).unwrap_or(u32::MAX),
    };
    for _ in 0..TRIES {
        let bytes = ask.bytes();
        before(bytes)?;
        let words = match compactor.compact(&ask).map_err(Failed::from) {
            Ok(plain) => {
                journal.record(
                    Call::Compaction,
                    compactor.model().as_str(),
                    plain.usage,
                    bytes,
                );
                plain.text.split_whitespace().collect::<Vec<_>>().join(" ")
            }
            Err(Failed::TheQuestion) => return Ok(unfit(shortest)),
            Err(Failed::Delays(failed)) => {
                return Err(Refusal::unprocessable(format!("the compactor: {failed}")));
            }
        };
        shortest = Some(shortest.map_or(words.len(), |was| was.min(words.len())));
        let why = match Line::new(&words) {
            Ok(line) => return Ok(Outcome::Fits { line }),
            Err(NotALine::TooLong(_)) => Unfit::Long {
                bytes: words.len(),
                cut: words[..words.floor_char_boundary(NODE_BYTES)].to_owned(),
            },
            Err(NotALine::Empty | NotALine::LineBreak | NotALine::Unscrubbable(_)) => {
                Unfit::NotALine
            }
        };
        ask = ask.tried(why);
    }
    Ok(unfit(shortest))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::model_name::ModelName;
    use suzatary_base::call_log::TokenUsage;
    use suzatary_base::capability::{Nothing, Recording};
    use suzatary_conversations::secretary::model::Plain;

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

        fn compact(&self, ask: &Ask) -> Result<Plain, ModelError> {
            self.asked.lock().expect("held").push(ask.clone());
            let length = self
                .answers
                .lock()
                .expect("held")
                .pop()
                .ok_or_else(|| ModelError::Failed("no answer left".to_owned()))?;
            Ok(Plain {
                text: "w".repeat(length),
                usage: None,
            })
        }
    }

    fn ask() -> Ask {
        Ask::new(Vec::new(), vec!["Owner: hi".to_owned()])
    }

    fn counted(compactor: &Lengths) -> (Result<Outcome, Refusal>, Vec<u64>) {
        let mut sent = Vec::new();
        let outcome = compact(compactor, &Nothing, ask(), &mut |bytes| {
            sent.push(bytes);
            Ok(())
        });
        (outcome, sent)
    }

    /// Answers a line that fits, reporting that it cost the given counts.
    struct Costing(ModelName, TokenUsage);

    impl Compact for Costing {
        fn model(&self) -> &ModelName {
            &self.0
        }

        fn compact(&self, _: &Ask) -> Result<Plain, ModelError> {
            Ok(Plain {
                text: "w".repeat(40),
                usage: Some(self.1),
            })
        }
    }

    /// Each call that returns is told to the journal with the counts it reported and the bytes
    /// it sent, and a call that fails is not.
    #[test]
    fn a_call_is_journaled_with_its_counts_and_a_failed_call_is_not() {
        let spent = TokenUsage {
            prompt_tokens: 1,
            completion_tokens: 2,
            cached_prompt_tokens: 3,
            cache_written_prompt_tokens: 4,
        };
        let journal = Recording::default();
        let costing = Costing(ModelName::parse("costing").expect("a model"), spent);
        compact(&costing, &journal, ask(), &mut |_| Ok(())).expect("fits");
        let calls = journal.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].conversation, None);
        assert_eq!(calls[0].model, "costing");
        assert_eq!(calls[0].usage, Some(spent));
        assert_eq!(calls[0].bytes, ask().bytes());
        let failing = Failing(ModelName::parse("failing").expect("a model"), || {
            ModelError::Failed("exit 1".to_owned())
        });
        compact(&failing, &journal, ask(), &mut |_| Ok(())).expect_err("fails");
        assert_eq!(journal.calls().len(), 1);
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
        assert_eq!(asked[0].last(), None);
        assert_eq!(
            asked[2].last(),
            Some(&Unfit::Long {
                bytes: 900,
                cut: "w".repeat(NODE_BYTES)
            }),
            "a retry shows the last answer cut"
        );
        assert_eq!(
            asked[2].text().len() - asked[1].text().len(),
            "; 900 bytes".len(),
            "and grows by the size of each earlier one alone"
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
        let refused = compact(&Lengths::new(&[1]), &Nothing, ask(), &mut |_| {
            Err(Refusal::conflict("held".to_owned()))
        });
        assert!(refused.is_err(), "a refused call is not made");
    }

    /// Fails every call with the error it is made with.
    struct Failing(ModelName, fn() -> ModelError);

    impl Compact for Failing {
        fn model(&self) -> &ModelName {
            &self.0
        }

        fn compact(&self, _: &Ask) -> Result<Plain, ModelError> {
            Err((self.1)())
        }
    }

    #[test]
    fn a_question_too_long_for_the_model_is_unfit_and_an_outage_only_delays() {
        let name = || ModelName::parse("failing").expect("a model");
        let mut calls = 0;
        let too_long = Failing(name(), || ModelError::TooLong("over".to_owned()));
        let outcome = compact(&too_long, &Nothing, ask(), &mut |_| {
            calls += 1;
            Ok(())
        });
        assert_eq!(outcome, Ok(Outcome::Unfit { shortest: 0 }));
        assert_eq!(calls, 1, "the same question is not asked again");
        let outages: [fn() -> ModelError; 4] = [
            || ModelError::Unreachable("down".to_owned()),
            || ModelError::Overloaded("busy".to_owned()),
            || ModelError::TimedOut(std::time::Duration::from_secs(1)),
            || ModelError::Failed("exit 1".to_owned()),
        ];
        for outage in outages {
            let outcome = compact(&Failing(name(), outage), &Nothing, ask(), &mut |_| Ok(()));
            assert!(outcome.is_err(), "{outcome:?} writes nothing");
        }
    }
}
