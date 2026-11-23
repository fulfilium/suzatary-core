//! A PDF's text through `pdftotext`, and its form through `pdfinfo`: bytes in on stdin, text
//! out on stdout, so an attachment is never written to disk, and a bad PDF is an `Err`.

use std::path::PathBuf;
use std::time::Duration;

use suzatary_held::file_read::{Form, Layer};

use suzatary_io_access::run::{RunError, succeed};

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
    pub fn read(&self, bytes: &[u8]) -> Result<Layer, String> {
        self.layer(bytes).map_err(|failed| match failed {
            Unlayered::File(why) | Unlayered::Box(why) => why,
        })
    }

    /// A PDF's text and whether it holds a form, which `pdftotext` does not print the values of:
    /// `pdfinfo` names its form, and a PDF it names none for holds none.
    pub fn layer(&self, bytes: &[u8]) -> Result<Layer, Unlayered> {
        let text = self.run("pdftotext", &["-layout", "-", "-"], bytes)?;
        let info = self.run("pdfinfo", &["-"], bytes)?;
        let form = match info.lines().find_map(|line| line.strip_prefix("Form:")) {
            Some(named) if named.trim() == "none" => Form::Absent,
            Some(_) | None => Form::Present,
        };
        Ok(Layer::read(text, form))
    }

    fn run(&self, tool: &str, args: &[&str], bytes: &[u8]) -> Result<String, Unlayered> {
        let cmd = suzatary_io_access::tool::tool(&self.repo, tool)
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
pub enum Unlayered {
    File(String),
    Box(String),
}

impl suzatary_held::pdf_text::PdfText for Pdftotext {
    fn layer(&self, pdf: &[u8]) -> Result<Layer, String> {
        self.read(pdf)
    }
}
