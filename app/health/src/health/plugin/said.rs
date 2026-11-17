//! What a plugin said, as the host keeps it: its words, bounded.

use std::fmt;

/// The most bytes a [`Said`] holds, its marker included.
pub const MAX_SAID: usize = 512;

/// Text a plugin put before the host (an error, a URL, a log line), at most [`MAX_SAID`] bytes.
///
/// [`Said::clip`] is the one way to make one, so whatever the host keeps or prints of a plugin's
/// words is bounded by type. Text that does not fit is cut on a character and marked `…`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Said(String);

impl Said {
    /// `text`, whole when it fits and cut when it does not.
    #[must_use]
    pub fn clip(text: impl Into<String>) -> Self {
        let mut text = text.into();
        if text.len() > MAX_SAID {
            text.truncate(text.floor_char_boundary(MAX_SAID - '…'.len_utf8()));
            text.push('…');
        }
        Self(text)
    }

    /// The words as kept.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Said {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// As the text's own: a refusal quoting a plugin's words with `{:?}` escapes them as a string.
impl fmt::Debug for Said {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        /// Whatever a plugin says, what is kept fits the bound, and text that fits is kept whole.
        #[test]
        fn what_is_kept_is_bounded_and_a_prefix(text in "\\PC{0,700}") {
            let said = Said::clip(text.clone());
            prop_assert!(said.as_str().len() <= MAX_SAID);
            let kept = said.as_str().strip_suffix('…').filter(|_| text.len() > MAX_SAID);
            prop_assert!(text.starts_with(kept.unwrap_or(said.as_str())));
        }
    }
}
