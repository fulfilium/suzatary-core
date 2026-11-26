//! The recogniser's laws, over generated addresses of every form mixed into generated prose.

use std::cell::Cell;

use proptest::prelude::*;
use proptest::test_runner::TestRunner;

use crate::{Address, Join, addresses, holds_address, replace_addresses};

/// Every way a sender can write an address that the recogniser reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    Plain,
    Symbols,
    NonAsciiLocal,
    NonAsciiDomain,
    SpaceBeside,
    ZeroWidthBeside,
    FullWidthAt,
    FullWidthDot,
    Quoted,
    IpLiteral,
}

const FORMS: [Form; 10] = [
    Form::Plain,
    Form::Symbols,
    Form::NonAsciiLocal,
    Form::NonAsciiDomain,
    Form::SpaceBeside,
    Form::ZeroWidthBeside,
    Form::FullWidthAt,
    Form::FullWidthDot,
    Form::Quoted,
    Form::IpLiteral,
];

/// The addresses in one generated text.
const PER_TEXT: u32 = 3;

fn pick<'a>(from: &[&'a str], at: usize) -> &'a str {
    from[at % from.len()]
}

/// The address of `form` made from the invented name `name`; `at` picks among its variants.
fn spell(form: Form, name: &str, at: usize) -> (String, Join) {
    let invisible = pick(
        &[
            "\u{200b}", "\u{200c}", "\u{200d}", "\u{2060}", "\u{feff}", "\u{ad}",
        ],
        at,
    );
    let spaced = || {
        let (left, right) = [
            (" ", ""),
            ("", " "),
            (" ", " "),
            ("\t", "\u{a0}"),
            ("\n", "\u{3000}"),
        ][at % 5];
        format!("{name}{left}@{right}example.test")
    };
    let text = match form {
        Form::Plain => format!("{name}@{}example.test", pick(&["", "mail."], at)),
        Form::Symbols => format!(
            "o{}{name}@example.test",
            pick(&["'", "+", "_", "!", "%", "~"], at)
        ),
        Form::NonAsciiLocal => format!(
            "{}{name}@example.test",
            pick(&["josé", "иван", "ада", "李", "ørjan", "straße"], at)
        ),
        Form::NonAsciiDomain => format!(
            "{name}@{}",
            pick(
                &[
                    "exämple.test",
                    "пример.test",
                    "例え.test",
                    "example.рф",
                    "example.中国"
                ],
                at
            )
        ),
        Form::SpaceBeside => spaced(),
        Form::ZeroWidthBeside => [
            format!("{name}{invisible}@example.test"),
            format!("{name}@{invisible}example.test"),
            format!("{name}@exam{invisible}ple.test"),
        ][at % 3]
            .clone(),
        Form::FullWidthAt => format!("{name}{}example.test", pick(&["＠", "﹫"], at)),
        Form::FullWidthDot => format!("{name}@example{}test", pick(&["．", "。", "｡"], at)),
        Form::Quoted => match at % 3 {
            0 => format!("\"{name} {name}\"@example.test"),
            1 => format!("\"{name}@{name}\"@example.test"),
            _ => format!("\"{name}\\\"{name}\"@example.test"),
        },
        Form::IpLiteral => format!(
            "{name}@{}",
            pick(&["[192.0.2.1]", "[IPv6:2001:db8::1]"], at)
        ),
    };
    let join = if form == Form::SpaceBeside {
        Join::Spaced
    } else {
        Join::Tight
    };
    (text, join)
}

/// Words that are not an address: letters of several scripts, every punctuation, quotes, format
/// characters and line breaks, and no at sign.
fn prose() -> impl Strategy<Value = String> {
    proptest::collection::vec("[a-zA-Zéж李 .,;:!?'\"()\\[\\]{}<>/=+_*#%&~^|`-]{1,12}|at|dot|example\\.test|a\u{200b}b|\\n", 0..6)
        .prop_map(|words| words.join(" "))
}

type Case = (Vec<(usize, String, usize)>, Vec<String>);

fn case() -> impl Strategy<Value = Case> {
    (
        proptest::collection::vec(
            (0..FORMS.len(), "[a-z]{1,8}", 0usize..30),
            PER_TEXT as usize,
        ),
        proptest::collection::vec(prose(), PER_TEXT as usize + 1),
    )
}

/// The text with each address between prose, and where each address stands in it.
fn mixed(
    addresses: &[(Form, String, Join)],
    prose: &[String],
) -> (String, Vec<std::ops::Range<usize>>) {
    let mut text = prose[0].clone();
    let mut at = Vec::new();
    for ((_, address, _), after) in addresses.iter().zip(&prose[1..]) {
        text.push(' ');
        at.push(text.len()..text.len() + address.len());
        text.push_str(address);
        text.push(' ');
        text.push_str(after);
    }
    (text, at)
}

fn expected(case: &Case) -> Vec<(Form, String, Join)> {
    case.0
        .iter()
        .map(|(form, name, at)| {
            let (text, join) = spell(FORMS[*form], name, *at);
            (FORMS[*form], text, join)
        })
        .collect()
}

fn check(case: &Case) -> Result<(), TestCaseError> {
    let made = expected(case);
    let (text, at) = mixed(&made, &case.1);
    let found: Vec<_> = addresses(&text).collect();
    prop_assert_eq!(
        found.iter().map(Address::range).collect::<Vec<_>>(),
        at.clone(),
        "{:?}",
        text
    );
    prop_assert_eq!(
        found.iter().map(Address::join).collect::<Vec<_>>(),
        made.iter().map(|(_, _, join)| *join).collect::<Vec<_>>()
    );
    let gone = replace_addresses(&text, "[a]");
    prop_assert_eq!(
        gone.matches("[a]").count() - text.matches("[a]").count(),
        made.len()
    );
    Ok(())
}

/// The recogniser finds each generated address, whole, between prose; and the run reaches each
/// form at least four standard deviations less often than its mean (the draws of a form are
/// binomial) would take it, so a form that the generator never writes fails the law.
#[test]
fn every_form_is_found_in_prose_and_every_form_is_drawn() {
    let config = ProptestConfig::default();
    let total = f64::from(config.cases * PER_TEXT);
    let reached: [Cell<u32>; FORMS.len()] = Default::default();
    TestRunner::new(config)
        .run(&case(), |case| {
            for (form, ..) in &case.0 {
                reached[*form].set(reached[*form].get() + 1);
            }
            check(&case)
        })
        .expect("every generated address is found");
    let share = 1.0 / f64::from(u8::try_from(FORMS.len()).expect("a few forms"));
    let floor = total * share - 4.0 * (total * share * (1.0 - share)).sqrt();
    for (form, count) in FORMS.iter().zip(&reached) {
        assert!(
            f64::from(count.get()) >= floor,
            "{form:?} drawn {} times, floor {floor:.1}",
            count.get()
        );
    }
}

proptest! {
    /// A text without an at sign holds no address, whatever else it holds.
    #[test]
    fn prose_without_an_address_holds_none(text in prose()) {
        prop_assert_eq!(addresses(&text).count(), 0);
        prop_assert!(!holds_address(&text));
        prop_assert!(matches!(replace_addresses(&text, "[a]"), std::borrow::Cow::Borrowed(_)));
    }

    /// The recogniser reads at least what the ASCII pattern before it read.
    #[test]
    fn an_ascii_address_is_still_read(text in "[a-zA-Z0-9._%+@ -]{0,60}") {
        let old = regex::Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9\-]+(?:\.[A-Za-z0-9\-]+)+").expect("a literal pattern");
        let now: Vec<_> = addresses(&text).map(|a| a.range()).collect();
        for found in old.find_iter(&text) {
            prop_assert!(now.iter().any(|r| r.start <= found.start() && found.end() <= r.end), "{:?}", text);
        }
    }
}
