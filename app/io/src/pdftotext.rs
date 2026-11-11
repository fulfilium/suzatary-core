//! A PDF's text through `pdftotext`, and its form through `pdfinfo`: bytes in on stdin, text
//! out on stdout, so an attachment is never written to disk, and a bad PDF is an `Err`.

use std::path::PathBuf;
use std::time::Duration;

use suzatary_core::file_read::{Form, Layer};

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

    /// A PDF's text, laid out as it reads on the page, and whether it holds a form, or why not.
    ///
    /// # Errors
    /// A sentence when `pdftotext` or `pdfinfo` cannot run, times out or refuses the bytes.
    pub fn read(&self, bytes: &[u8]) -> Result<Layer, String> {
        self.layer(bytes).map_err(|failed| match failed {
            Unlayered::File(why) | Unlayered::Box(why) => why,
        })
    }

    /// A PDF's text and whether it holds a form, which `pdftotext` does not print the values of:
    /// `pdfinfo` names its form, and a PDF it names none for holds none.
    pub(crate) fn layer(&self, bytes: &[u8]) -> Result<Layer, Unlayered> {
        let text = self.run("pdftotext", &["-layout", "-", "-"], bytes)?;
        let info = self.run("pdfinfo", &["-"], bytes)?;
        let form = match info.lines().find_map(|line| line.strip_prefix("Form:")) {
            Some(named) if named.trim() == "none" => Form::Absent,
            Some(_) | None => Form::Present,
        };
        Ok(Layer::read(text, form))
    }

    fn run(&self, tool: &str, args: &[&str], bytes: &[u8]) -> Result<String, Unlayered> {
        let cmd = crate::tool::tool(&self.repo, tool)
            .args(args)
            .stdin(bytes)
            .timeout(self.timeout);
        match succeed(&cmd) {
            Ok(ran) => Ok(ran.out()),
            Err(RunError::Exited { stderr, .. }) => Err(Unlayered::File(stderr.to_string())),
            Err(RunError::TimedOut { .. }) => Err(Unlayered::File(format!(
                "{tool} did not finish within {:?}",
                self.timeout
            ))),
            Err(RunError::Spawn { source, .. }) => {
                Err(Unlayered::Box(format!("{tool} could not be run: {source}")))
            }
            Err(failed @ (RunError::Halted { .. } | RunError::Wait { .. })) => {
                Err(Unlayered::Box(failed.to_string()))
            }
        }
    }
}

/// Why a PDF has no text layer: the file, which `pdftotext` refused or did not finish in time, so
/// a second read finds the same; or the box, whose `pdftotext` did not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unlayered {
    File(String),
    Box(String),
}

impl suzatary_core::pdf_text::PdfText for Pdftotext {
    fn layer(&self, pdf: &[u8]) -> Result<Layer, String> {
        self.read(pdf)
    }
}

/// A one-page PDF that says `text`, which must hold no `(`, `)` or `\`.
#[cfg(test)]
#[must_use]
pub(crate) fn minimal_pdf(text: &str) -> Vec<u8> {
    pdf(text, "", "", &[])
}

/// [`minimal_pdf`] with one text field filled with `value`, as a filled W-9 is: the value is in
/// the form, not on the page.
#[cfg(test)]
#[must_use]
pub(crate) fn filled_form_pdf(text: &str, value: &str) -> Vec<u8> {
    let field = format!(
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (ssn) /V ({value}) /Rect [20 20 180 40] \
         /P 3 0 R >>"
    );
    pdf(
        text,
        "/AcroForm << /Fields [6 0 R] >> ",
        "/Annots [6 0 R] ",
        &[field],
    )
}

#[cfg(test)]
fn pdf(text: &str, catalog: &str, page: &str, more: &[String]) -> Vec<u8> {
    let content = format!("BT /F1 24 Tf 20 100 Td ({text}) Tj ET");
    let stream = format!(
        "<< /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    );
    let mut objects = vec![
        format!("<< /Type /Catalog /Pages 2 0 R {catalog}>>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        format!(
            "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> \
             /MediaBox [0 0 200 200] /Contents 5 0 R {page}>>"
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        stream,
    ];
    objects.extend_from_slice(more);
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
