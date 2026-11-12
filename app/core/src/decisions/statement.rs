use suzatary_record::record::MarkupSource;

use super::NotADecision;

/// What a decision says: a title, its body as Typst markup, and why. Its value is the whole
/// statement, so a revision restates it rather than patching one field.
///
/// The title and the rationale are DATA (escaped wherever they are drawn); the body is markup
/// admitted through the one door when it was written, or read back from where it was stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    title: String,
    body: MarkupSource,
    rationale: String,
}

impl Statement {
    /// The longest title, in characters.
    pub(crate) const TITLE: usize = 120;

    /// Check and build one, the title and rationale with no space around them: one spelling,
    /// so what is stored reprints as itself.
    ///
    /// # Errors
    /// `NotADecision::Title` for a title that is not one trimmed line of 1-120 characters, and
    /// `NotADecision::Empty` for an empty body or an empty or untrimmed rationale.
    pub fn new(title: &str, body: MarkupSource, rationale: &str) -> Result<Self, NotADecision> {
        if title.is_empty()
            || title != title.trim()
            || title.chars().count() > Self::TITLE
            || title.contains(['\n', '\r'])
        {
            return Err(NotADecision::Title);
        }
        if body.as_str().trim().is_empty() {
            return Err(NotADecision::Empty("body"));
        }
        if rationale.is_empty() || rationale != rationale.trim() {
            return Err(NotADecision::Empty("rationale"));
        }
        Ok(Self {
            title: title.to_owned(),
            body,
            rationale: rationale.to_owned(),
        })
    }

    /// The title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The body, Typst markup.
    #[must_use]
    pub fn body(&self) -> &MarkupSource {
        &self.body
    }

    /// Why.
    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }
}
