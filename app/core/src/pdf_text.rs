//! A PDF's text layer, for what reads a file the box holds back.

/// Reads a PDF: its text layer as `pdftotext -layout` lays it out, and whether it holds a form.
pub trait PdfText: Sync {
    /// What `pdf` holds.
    ///
    /// # Errors
    /// The reader's sentence, when it cannot run or refuses the bytes.
    fn layer(&self, pdf: &[u8]) -> Result<crate::file_read::Layer, String>;
}
