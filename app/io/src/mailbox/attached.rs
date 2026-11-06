//! The kinds of held value each PDF attachment's text layer names, kept by the PDF's SHA-256, so
//! a listing reads a PDF once. A record holds kinds alone, never a digit of a value.

use std::path::PathBuf;

use sha2::{Digest as _, Sha256};
use suzatary_core::capability::MailError;
use suzatary_core::held_value::{Kinds, MAX_OPENED, Values};
use suzatary_core::store::{Doc as _, Gets as _};

use crate::docs::{DocCodec, Docs};
use crate::pdftotext::{Pdftotext, Unlayered};

#[derive(Debug, Clone)]
pub(crate) struct Attached(Docs<String, Kinds>);

static CODEC: DocCodec<String, Kinds> = DocCodec::JSON;

impl Attached {
    pub(crate) fn open(dir: PathBuf) -> Self {
        Self(Docs::sharded("attachment kinds", dir, &CODEC))
    }

    /// The kinds `pdf`'s text layer names: none for a layer `pdftotext` refuses or a file over
    /// [`MAX_OPENED`]. A box whose `pdftotext` does not run records nothing and fails.
    pub(crate) fn kinds(&self, pdf: &[u8], pdftotext: &Pdftotext) -> Result<Kinds, MailError> {
        let digest = hex::encode(Sha256::digest(pdf));
        let unreachable =
            |failed: suzatary_core::store::StoreError| MailError::Unreachable(failed.to_string());
        if let Some(kinds) = self.0.get(&digest).map_err(unreachable)? {
            return Ok(kinds);
        }
        let kinds = if pdf.len() as u64 > MAX_OPENED {
            Kinds::empty()
        } else {
            match pdftotext.layer(pdf) {
                Ok(text) => Values::of([text.as_str()]).kinds(),
                Err(Unlayered::File(_)) => Kinds::empty(),
                Err(Unlayered::Box(why)) => return Err(MailError::Unreachable(why)),
            }
        };
        self.0.set(&digest, &kinds).map_err(unreachable)?;
        Ok(kinds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_core::redact::Kind;

    /// A synthetic number no person holds: SSN area 000.
    const SSN: &str = "000-12-3456";

    /// The boundary a search row rests on: a record holds no digit of a value.
    #[test]
    fn a_pdf_s_layer_is_recorded_as_its_kinds_and_never_its_number() {
        let dir = tempfile::tempdir().expect("scratch");
        let attached = Attached::open(dir.path().to_owned());
        let pdftotext = Pdftotext::new(".");
        let named = crate::pdftotext::minimal_pdf(&format!("SSN {SSN}"));
        let refused = b"%PDF-1.4 no objects".to_vec();
        assert_eq!(
            attached.kinds(&named, &pdftotext).expect("read"),
            Kinds::one(Kind::Ssn)
        );
        assert_eq!(
            attached.kinds(&refused, &pdftotext).expect("read"),
            Kinds::empty()
        );
        assert_eq!(
            attached
                .kinds(&crate::pdftotext::minimal_pdf("Invoice 42"), &pdftotext)
                .expect("read"),
            Kinds::empty()
        );
        let records: Vec<String> = walk(dir.path());
        assert_eq!(records.len(), 3, "{records:?}");
        assert!(
            records.iter().all(|record| !record.contains("3456")),
            "{records:?}"
        );
    }

    fn walk(dir: &std::path::Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .expect("a directory")
            .flat_map(|entry| {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    walk(&path)
                } else {
                    vec![std::fs::read_to_string(&path).expect("a record")]
                }
            })
            .collect()
    }
}
