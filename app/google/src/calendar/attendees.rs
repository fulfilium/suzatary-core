//! Who is in an occurrence: the names it keeps and how many people it counts without one.

use suzatary_address::{AttendeeName, NotAName};

/// How many attendees, by their place in the provider's list, may be named.
pub const MAX_ATTENDEES: usize = 50;

/// The names among the first [`MAX_ATTENDEES`] attendees, in the provider's order, and the count
/// of every other attendee: one with no name, a name that is not an [`AttendeeName`], and one
/// past the first [`MAX_ATTENDEES`]. The owner's own entry is an attendee like any other.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Attendees {
    named: Vec<AttendeeName>,
    unnamed: u32,
}

impl Attendees {
    /// The attendees a provider gave, in its order, each by its whole name or by `""` for none,
    /// beside `unnamed` others already counted. No name is cut: one too long is counted.
    /// LAW (`the_people_are_counted_and_no_name_is_an_address`): the names kept plus the count of
    /// the unnamed is `unnamed` plus the number of texts, and a text is kept exactly when it is among the first [`MAX_ATTENDEES`]
    /// and parses.
    pub fn of<'t>(names: impl IntoIterator<Item = &'t str>, unnamed: u32) -> Self {
        let mut attendees = Self {
            named: Vec::new(),
            unnamed,
        };
        for (place, text) in names.into_iter().enumerate() {
            match AttendeeName::parse(text) {
                Ok(name) if place < MAX_ATTENDEES => attendees.named.push(name),
                Ok(_) | Err(NotAName::Empty | NotAName::Address | NotAName::TooLong) => {
                    attendees.unnamed = attendees.unnamed.saturating_add(1);
                }
            }
        }
        attendees
    }

    #[must_use]
    pub fn named(&self) -> &[AttendeeName] {
        &self.named
    }

    #[must_use]
    pub fn unnamed(&self) -> u32 {
        self.unnamed
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use suzatary_address::holds_address;

    use super::*;
    use crate::calendar::MAX_NAME;

    fn given() -> impl Strategy<Value = String> {
        prop_oneof![
            Just(String::new()),
            Just("   ".to_owned()),
            "[A-Za-z ,.'-]{1,40}",
            "[a-z]{1,12}".prop_map(|local| format!("{local}@example.invalid")),
            "[A-Za-z ]{1,20}".prop_map(|name| format!("{name} <{}@example.test>", "ada")),
            "[A-Za-z]{1,8}".prop_map(|name| format!("{name}{}", "x".repeat(MAX_NAME))),
            "[a-z]{1,8}"
                .prop_map(|local| format!("{}{local}@example.invalid", "y".repeat(MAX_NAME))),
        ]
    }

    #[test]
    fn a_name_at_the_bound_is_kept_and_one_past_it_counted() {
        let names = [
            "a".repeat(MAX_NAME),
            "a".repeat(MAX_NAME + 1),
            "é".repeat(MAX_NAME),
            "é".repeat(MAX_NAME + 1),
        ];
        let attendees = Attendees::of(names.iter().map(String::as_str), 0);
        assert_eq!((attendees.named().len(), attendees.unnamed()), (2, 2));
    }

    proptest! {
        #[test]
        fn the_people_are_counted_and_no_name_is_an_address(
            given in proptest::collection::vec(given(), 0..120),
            already in 0u32..5,
        ) {
            let attendees = Attendees::of(given.iter().map(String::as_str), already);
            prop_assert_eq!(
                attendees.named().len() + attendees.unnamed() as usize,
                given.len() + already as usize
            );
            let kept: Vec<AttendeeName> = given
                .iter()
                .take(MAX_ATTENDEES)
                .filter_map(|text| AttendeeName::parse(text).ok())
                .collect();
            prop_assert_eq!(attendees.named(), kept.as_slice());
            for name in attendees.named() {
                prop_assert!(!holds_address(name.as_str()), "{:?}", name);
                prop_assert!(name.as_str().chars().count() <= MAX_NAME);
                prop_assert!(!name.as_str().trim().is_empty());
            }
        }
    }
}
