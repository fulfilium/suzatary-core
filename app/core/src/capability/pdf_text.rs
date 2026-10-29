//! A PDF's text layer, for what reads a file the box holds back.

/// Reads a PDF's text layer, as `pdftotext -layout` lays it out, a form feed ending each page.
pub trait PdfText: Sync {
    /// The text of `pdf`.
    ///
    /// # Errors
    /// The reader's sentence, when it cannot run or refuses the bytes.
    fn text(&self, pdf: &[u8]) -> Result<String, String>;
}
