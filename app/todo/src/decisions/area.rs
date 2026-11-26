use std::fmt;

use super::NotADecision;

/// A part of the tree that a decision governs: a relative path such as `app/tiles`, read as the
/// directory or file at that path and everything below it. A path has no leading or trailing
/// `/`, no empty segment and no `.` or `..` segment.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, prodrome_data::literal::codec::Literal,
)]
#[literal(crate = prodrome_data, new = Area::stored)]
pub struct Area(String);

impl Area {
    pub fn new(text: impl Into<String>) -> Result<Self, NotADecision> {
        let text = text.into();
        let text = text.trim();
        let segments_fit = text
            .split('/')
            .all(|segment| !matches!(segment, "" | "." | ".."));
        let characters_fit = text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/'));
        if segments_fit && characters_fit && text.len() <= 120 {
            Ok(Self(text.to_owned()))
        } else {
            Err(NotADecision::Area(text.chars().take(80).collect()))
        }
    }

    fn stored(text: String) -> Result<Self, prodrome_data::literal::ProdromeError> {
        Self::new(text)
            .map_err(|failed| prodrome_data::literal::ProdromeError::invalid(failed.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// How many directories deep it is: `app` is 1 and `app/tiles` is 2.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.0.split('/').count()
    }

    /// Whether `path`, a relative path in the tree, is this area or lies below it.
    #[must_use]
    pub fn covers(&self, path: &str) -> bool {
        path.strip_prefix(self.0.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    }
}

impl fmt::Display for Area {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_area_covers_itself_and_what_lies_below_it_and_no_sibling() {
        let area = Area::new("app/tiles").expect("an area");
        assert!(area.covers("app/tiles"));
        assert!(area.covers("app/tiles/src/lib.rs"));
        assert!(!area.covers("app/tiles2"));
        assert!(!area.covers("app"));
    }

    #[test]
    fn an_area_is_a_clean_relative_path() {
        for bad in [
            "",
            "/app",
            "app/",
            "app//tiles",
            "app/../x",
            "./app",
            "app tiles",
            "app'",
        ] {
            assert!(Area::new(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            Area::new(" app/tiles ").expect("trimmed").as_str(),
            "app/tiles"
        );
    }
}
