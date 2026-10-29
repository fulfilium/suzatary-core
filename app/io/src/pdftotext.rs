//! A PDF's text through `pdftotext`: bytes in on stdin, text out on stdout,
//! so an attachment is never written to disk, and a bad PDF is an `Err`.

use std::path::PathBuf;
use std::time::Duration;

use crate::run::{RunError, succeed};

/// Mail is attacker-controlled, and a PDF can wedge Poppler.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// `pdftotext`, with the checkout that is its nix fallback.
#[derive(Debug, Clone)]
pub struct Pdftotext {
    repo: PathBuf,
    timeout: Duration,
}

impl Pdftotext {
    /// `repo` is consulted only when `pdftotext` is not on `PATH`.
    #[must_use]
    pub fn new(repo: impl Into<PathBuf>) -> Self {
        Self {
            repo: repo.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// A PDF's text, laid out as it reads on the page, or why not.
    ///
    /// # Errors
    /// A sentence when `pdftotext` cannot run, times out or refuses the bytes.
    pub fn text(&self, bytes: &[u8]) -> Result<String, String> {
        let cmd = crate::tool::tool(&self.repo, "pdftotext")
            .args(["-layout", "-", "-"])
            .stdin(bytes)
            .timeout(self.timeout);
        match succeed(&cmd) {
            Ok(ran) => Ok(ran.out()),
            Err(RunError::Exited { stderr, .. }) => Err(stderr.to_string()),
            Err(RunError::Spawn { source, .. }) => {
                Err(format!("pdftotext could not be run: {source}"))
            }
            Err(RunError::TimedOut { .. }) => Err(format!(
                "pdftotext did not finish within {:?}",
                self.timeout
            )),
            Err(failed) => Err(failed.to_string()),
        }
    }
}

impl suzatary_core::capability::PdfText for Pdftotext {
    fn text(&self, pdf: &[u8]) -> Result<String, String> {
        Pdftotext::text(self, pdf)
    }
}

/// A one-page PDF that says `text`, which must hold no `(`, `)` or `\`.
#[cfg(test)]
#[must_use]
pub(crate) fn minimal_pdf(text: &str) -> Vec<u8> {
    let content = format!("BT /F1 24 Tf 20 100 Td ({text}) Tj ET");
    let stream = format!(
        "<< /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    );
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> \
         /MediaBox [0 0 200 200] /Contents 5 0 R >>"
            .to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        stream,
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref_offset = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}
