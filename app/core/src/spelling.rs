//! How text is spelled for a reader.

/// A size in bytes, in words: "812 B", "48.2 KB", "3.1 MB".
#[must_use]
pub fn bytes(size: u64) -> String {
    const KB: u64 = 1024;
    // Integer tenths; a document's size is never near u64's limit.
    let tenths = |unit: u64| {
        let scaled = size.saturating_mul(10) / unit;
        format!("{}.{}", scaled / 10, scaled % 10)
    };
    if size < KB {
        format!("{size} B")
    } else if size < KB * KB {
        format!("{} KB", tenths(KB))
    } else {
        format!("{} MB", tenths(KB * KB))
    }
}

/// `words` with its first letter in capitals, for a label that opens a line.
#[must_use]
pub fn capitalised(words: &str) -> String {
    let mut chars = words.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// `text` cut to at most `chars` characters, on a char boundary, never panicking.
#[must_use]
pub fn shorten(text: &str, chars: usize) -> &str {
    match text.char_indices().nth(chars) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

/// `text` cut to `cap` characters, and whether it was cut.
#[must_use]
pub fn clip(text: &str, cap: usize) -> (String, bool) {
    let short = shorten(text, cap);
    let truncated = short.len() != text.len();
    (short.to_owned(), truncated)
}

/// A rule's why as one sentence, capitalised and ended with one full stop.
#[must_use]
pub fn said(why: &str) -> String {
    let trimmed = why.trim().trim_end_matches('.');
    let mut chars = trimmed.chars();
    chars.next().map_or_else(String::new, |first| {
        format!("{}.", first.to_uppercase().chain(chars).collect::<String>())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// `shorten` never panics, never splits a `char` over arbitrary text, and answers at
        /// most `chars` characters.
        #[test]
        fn shorten_never_panics_and_never_splits_a_char(text: String, chars in 0usize..200) {
            let short = shorten(&text, chars);
            prop_assert!(text.is_char_boundary(short.len()));
            prop_assert!(short.chars().count() <= chars);
        }

        /// `clip`'s flag is exactly whether `shorten` cut anything.
        #[test]
        fn clip_flags_truncation_exactly_when_shorten_cuts(text: String, cap in 0usize..200) {
            let (clipped, truncated) = clip(&text, cap);
            prop_assert_eq!(truncated, clipped != text);
            prop_assert_eq!(clipped.as_str(), shorten(&text, cap));
        }
    }
}
