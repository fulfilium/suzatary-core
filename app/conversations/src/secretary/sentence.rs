//! The words the box writes when a round ends short of an answer, as data.
//!
//! A [`Sentence`] is the box's own: each variant holds a count, a duration, a verb the surface
//! lists or a limit, and nothing a store, a model, a verb or the model CLI wrote, so no value of
//! it can quote a line. [`Outside`] holds the words that came from outside. [`Closing::said`] is
//! the one function that prints either, and prints them escaped, since no markup is meant.

use std::time::Duration;

use super::taint::Taint;
use super::tool_name::ToolName;
use super::verb_name::VerbName;
use suzatary_base::escape::escape_markup;
use suzatary_health::health::Limited;

/// What a closing names as the last to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    Verb(VerbName),
    /// Something the person pointed at, loaded before the round.
    Reference,
}

/// What a round's calls had been sent when it writes a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Built {
    /// No call was built yet.
    #[default]
    Nothing,
    /// The join of every call built, each joined as it was built, before it was sent.
    Joined(Taint),
}

impl Built {
    /// After one more call built carrying `carries`.
    #[must_use]
    pub fn joined(self, carries: Taint) -> Self {
        Built::Joined(self.carries().join(carries))
    }

    /// What the calls built carried: clean before the first.
    #[must_use]
    pub fn carries(self) -> Taint {
        match self {
            Built::Nothing => Taint::Clean,
            Built::Joined(carries) => carries,
        }
    }
}

/// The box's own words for a round that ended short of an answer.
///
/// It is `Copy` so that no variant can hold a `String`, the one thing that could quote a line;
/// a clean closing is made from nothing else.
///
/// ```compile_fail,E0308
/// use suzatary_conversations::secretary::sentence::Closing;
/// let _ = Closing::Own(String::new());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sentence {
    /// No round answered in time.
    Unanswered,
    /// The model asked for verbs this many times without answering.
    Bound(usize),
    Silent,
    SilentAfter(Named),
    TooLong,
    TooLongAfter(Named),
    TimedOut(Duration),
    Limited(Limited),
    Overloaded,
    Halted,
}

const _: fn() = || {
    fn copy<T: Copy>() {}
    copy::<Sentence>();
};

/// Words a store, a model, a verb or the model CLI wrote, quoted in what the box says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outside {
    /// A refusal's message.
    Refused(String),
    /// A verb name no surface lists, which only a model chose.
    SilentAfter(String),
    TooLongAfter(String),
    Unreachable(String),
    /// The model CLI's words for a sign-in it refused.
    SignIn(String),
    Failed(String),
    /// Who pulled the halt.
    Stopped(String),
}

/// One closing's words: the box's alone, or with others' quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Closing {
    Own(Sentence),
    Outside(Outside),
}

impl Named {
    fn said(self) -> &'static str {
        match self {
            Named::Verb(verb) => verb.name(),
            Named::Reference => "reference",
        }
    }
}

impl Closing {
    /// The silence of a model after `after` answered, if any did.
    #[must_use]
    pub fn silent(after: Option<&ToolName>) -> Self {
        match after.map(named) {
            None => Closing::Own(Sentence::Silent),
            Some(Ok(named)) => Closing::Own(Sentence::SilentAfter(named)),
            Some(Err(name)) => Closing::Outside(Outside::SilentAfter(name.to_owned())),
        }
    }

    /// What a model that read too much after `after` answered, if any did.
    #[must_use]
    pub fn too_long(after: Option<&ToolName>) -> Self {
        match after.map(named) {
            None => Closing::Own(Sentence::TooLong),
            Some(Ok(named)) => Closing::Own(Sentence::TooLongAfter(named)),
            Some(Err(name)) => Closing::Outside(Outside::TooLongAfter(name.to_owned())),
        }
    }

    /// The words as plain prose: what a caller logs.
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            Closing::Own(sentence) => sentence.said(),
            Closing::Outside(outside) => outside.said(),
        }
    }

    /// The words as typst content, which is how a turn stores them, since they are no markup.
    #[must_use]
    pub fn escaped(&self) -> String {
        escape_markup(&self.said()).as_str().to_owned()
    }
}

/// A verb the surface lists or a reference; else the name a model chose.
fn named(tool: &ToolName) -> Result<Named, &str> {
    match tool {
        ToolName::Verb(verb) => Ok(Named::Verb(*verb)),
        ToolName::Reference => Ok(Named::Reference),
        ToolName::Unlisted(_) => Err(tool.as_str()),
    }
}

impl Sentence {
    fn said(&self) -> String {
        match self {
            Sentence::Unanswered => "This was not answered in time; ask again.".to_owned(),
            Sentence::Bound(bound) => format!(
                "I asked for a verb {bound} times without finishing, so this exchange stopped \
                 here. Say something to continue it."
            ),
            Sentence::Silent => "The model replied with nothing to say. Ask again.".to_owned(),
            Sentence::SilentAfter(verb) => format!(
                "I stopped after {} answered, without an answer for you. Ask again, or ask me \
                 to narrow it.",
                verb.said()
            ),
            Sentence::TooLong => "I could not finish: this message, with what it brought, is \
                 longer than the model reads. Ask again in smaller parts. Nothing is cut: your \
                 next message is sent with this one whole, a long answer it brought by its \
                 marker, or without it when even that is too long."
                .to_owned(),
            Sentence::TooLongAfter(verb) => format!(
                "I could not finish: {} answered more than I can read. Ask me to narrow it; \
                 your next message is sent without that answer.",
                verb.said()
            ),
            Sentence::TimedOut(limit) => format!(
                "I stopped: the model did not answer within {} s. Ask again, or ask for less \
                 at once.",
                limit.as_secs()
            ),
            Sentence::Limited(limited) => {
                format!("I could not finish: {limited}. Ask again after then.")
            }
            Sentence::Overloaded => "I could not finish: the model's service is overloaded. Ask \
                 again in a few minutes."
                .to_owned(),
            Sentence::Halted => "I stopped: the model's run was ended. Ask again.".to_owned(),
        }
    }
}

impl Outside {
    fn said(&self) -> String {
        match self {
            Outside::Refused(words) => format!("I could not finish: {words}. Ask again."),
            Outside::SilentAfter(verb) => format!(
                "I stopped after {verb} answered, without an answer for you. Ask again, or ask \
                 me to narrow it."
            ),
            Outside::TooLongAfter(verb) => format!(
                "I could not finish: {verb} answered more than I can read. Ask me to narrow it; \
                 your next message is sent without that answer."
            ),
            Outside::Unreachable(words) => format!(
                "I could not finish: the model could not be reached ({words}). Ask again in a \
                 minute."
            ),
            Outside::SignIn(words) => format!(
                "I could not finish: the model refused its sign-in ({words}). Sign in again from \
                 Settings."
            ),
            Outside::Failed(words) => {
                format!("I could not finish: the model failed ({words}). Ask again.")
            }
            Outside::Stopped(by) => format!("{by} stopped this answer."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each sentence's own words, and what a closing that quotes prints escaped.
    #[test]
    fn each_sentence_prints_and_each_quote_is_escaped() {
        let verb = ToolName::from("show");
        let guess = ToolName::from("#run(\"x\")");
        let limited: Limited =
            serde_json::from_str(r#"{"limit":"rate","until":"2026-10-30T12:00:00"}"#)
                .expect("a limit");
        for (closing, expect) in [
            (Closing::Own(Sentence::Unanswered), "not answered in time"),
            (Closing::Own(Sentence::Bound(7)), "a verb 7 times"),
            (Closing::silent(None), "nothing to say"),
            (Closing::silent(Some(&verb)), "after show answered"),
            (Closing::too_long(None), "longer than the model reads"),
            (Closing::too_long(Some(&verb)), "show answered more"),
            (
                Closing::Own(Sentence::TimedOut(Duration::from_secs(90))),
                "within 90 s",
            ),
            (Closing::Own(Sentence::Limited(limited)), "rate-limited"),
            (Closing::Own(Sentence::Overloaded), "overloaded"),
            (Closing::Own(Sentence::Halted), "run was ended"),
            (Closing::Outside(Outside::Refused("r".into())), "finish: r."),
            (Closing::silent(Some(&guess)), r"after \#run("),
            (Closing::too_long(Some(&guess)), r"\#run("),
            (Closing::Outside(Outside::Unreachable("u".into())), "(u)"),
            (Closing::Outside(Outside::SignIn("s".into())), "(s)"),
            (Closing::Outside(Outside::Failed("f".into())), "(f)"),
            (
                Closing::Outside(Outside::Stopped("ada".into())),
                "ada stopped",
            ),
        ] {
            let said = closing.escaped();
            assert!(said.contains(expect), "{closing:?}: {said}");
        }
    }

    /// A verb name no surface lists is a model's words, so it is never part of an own sentence.
    #[test]
    fn an_unlisted_name_is_never_the_boxs_own() {
        for closing in [
            Closing::silent(Some(&ToolName::from("guessed"))),
            Closing::too_long(Some(&ToolName::from("guessed"))),
        ] {
            assert!(matches!(closing, Closing::Outside(_)), "{closing:?}");
        }
        for closing in [
            Closing::silent(Some(&ToolName::from("show"))),
            Closing::silent(Some(&ToolName::Reference)),
            Closing::too_long(Some(&ToolName::from("show"))),
        ] {
            assert!(matches!(closing, Closing::Own(_)), "{closing:?}");
        }
    }
}
