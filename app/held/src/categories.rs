//! The three `General_Category` sets the redaction and the leak guard's folding read, as tables of
//! Unicode 16.0.0, so what they match does not depend on the Unicode data a build links.

mod table;

/// A set of characters, as sorted, disjoint, inclusive ranges of code points.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Set(&'static [(u32, u32)]);

/// Cf: the format characters.
pub(crate) const FORMAT: Set = Set(&table::FORMAT);

/// Pd: the dash punctuation.
pub(crate) const DASH: Set = Set(&table::DASH);

/// Nd: the decimal digits.
pub(crate) const DECIMAL: Set = Set(&table::DECIMAL);

impl Set {
    #[must_use]
    pub(crate) fn contains(self, c: char) -> bool {
        let c = u32::from(c);
        self.0
            .binary_search_by(|&(first, last)| {
                if last < c {
                    std::cmp::Ordering::Less
                } else if first > c {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use unicode_general_category::{GeneralCategory, get_general_category};

    use std::fmt::Write as _;

    use sha2::Digest as _;

    use super::{DASH, DECIMAL, FORMAT, Set};

    const SETS: [(&str, Set, GeneralCategory); 3] = [
        ("Cf", FORMAT, GeneralCategory::Format),
        ("Pd", DASH, GeneralCategory::DashPunctuation),
        ("Nd", DECIMAL, GeneralCategory::DecimalNumber),
    ];

    /// Every scalar value is in a set exactly when version 1.1.0 of `unicode-general-category`,
    /// which holds Unicode 16.0.0, gives it the set's category.
    #[test]
    fn each_set_is_its_category_in_unicode_16() {
        let mut differ = Vec::new();
        for c in (0..=0x0010_FFFF).filter_map(char::from_u32) {
            for (name, set, category) in SETS {
                if set.contains(c) != (get_general_category(c) == category) {
                    differ.push(format!("{name} U+{:04X}", u32::from(c)));
                }
            }
        }
        assert!(differ.is_empty(), "{}", differ.join("\n"));
    }

    /// The SHA-256 of `set`, in lowercase hex, over one line for each range, `FIRST..LAST` and a
    /// newline, each code point in uppercase hex of at least four digits: the Prodrome's form for
    /// its printable set.
    fn digest(set: Set) -> String {
        let mut text = String::new();
        for (first, last) in set.0 {
            let _ = writeln!(text, "{first:04X}..{last:04X}");
        }
        hex::encode(sha2::Sha256::digest(text.as_bytes()))
    }

    #[test]
    fn each_set_has_its_pinned_digest() {
        let digests: Vec<String> = SETS.iter().map(|(_, set, _)| digest(*set)).collect();
        assert_eq!(
            digests,
            [
                "35aac2c4f78cc0b4434337a5e4257dc762205269f404bc44325ca13767aa45eb",
                "fe6ae33dda2922a44286d6b24a04407bccbeb8ca324260da0b1cb6659f3fcbc9",
                "71120d2a45a4ff768bd598d98da60c08a7da5afbf8b808e16df5fe18eaaa8487",
            ]
        );
    }
}
