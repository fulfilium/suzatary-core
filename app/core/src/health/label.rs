//! The name a person gives a connection.

use std::fmt;

use super::Refused;

/// The longest label.
const MAX: usize = 32;

/// `[A-Za-z0-9 ]{1,32}`, a space only between two words, kept as the credential suffix it names (`Work` is `WORK`, `home cal`
/// is `HOME_CAL`), so the label written and the label read back out of the instance are one
/// value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Label(String);

impl Label {
    /// Parse one.
    ///
    /// # Errors
    /// [`Refused::Param`] for an empty label, a longer one, or another character.
    pub fn parse(text: &str) -> Result<Self, Refused> {
        let shaped = !text.is_empty()
            && text.len() <= MAX
            && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b' ')
            // One spelling per suffix: no space at either end, and none doubled.
            && !text.starts_with(' ')
            && !text.ends_with(' ')
            && !text.contains("  ");
        if !shaped {
            return Err(Refused::Param {
                param: "label",
                said: format!(
                    "{text:?} is not a label: 1 to {MAX} letters, digits and single spaces \
                     between words"
                ),
            });
        }
        Ok(Self(
            text.bytes()
                .map(|b| match b {
                    b' ' => '_',
                    other => char::from(other.to_ascii_uppercase()),
                })
                .collect(),
        ))
    }

    /// The label a credential's suffix names: `HOME_CAL` is `Home cal`.
    ///
    /// # Errors
    /// [`Refused::Param`] for a suffix no label names.
    pub fn of_suffix(suffix: &str) -> Result<Self, Refused> {
        Self::parse(&suffix.replace('_', " "))
    }

    /// The suffix it names, `WORK`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// As a person reads it, `Work`.
    #[must_use]
    pub fn shown(&self) -> String {
        let lower = self.0.replace('_', " ").to_lowercase();
        let mut chars = lower.chars();
        chars
            .next()
            .map(|first| first.to_uppercase().chain(chars).collect())
            .unwrap_or_default()
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_is_the_suffix_it_names() {
        let work = Label::parse("Work").expect("a label");
        assert_eq!(work.as_str(), "WORK");
        assert_eq!(work.shown(), "Work");
        assert_eq!(
            Label::parse("home cal").expect("a label").as_str(),
            "HOME_CAL"
        );
        for bad in ["", "a/b", "a_b", "é", " a", "a ", "a  b", &"x".repeat(33)] {
            assert!(Label::parse(bad).is_err(), "{bad:?}");
        }
    }

    proptest::proptest! {
        /// What a label shows reads back as the same label, and so does the suffix it names.
        #[test]
        fn a_label_reads_back_as_itself(text in "[A-Za-z0-9]{1,10}( [A-Za-z0-9]{1,10}){0,2}") {
            let label = Label::parse(&text).expect("a shaped label");
            proptest::prop_assert_eq!(Label::parse(&label.shown()), Ok(label.clone()));
            proptest::prop_assert_eq!(Label::of_suffix(label.as_str()), Ok(label));
        }
    }
}
