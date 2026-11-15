//! The inert upload index and PDF reader: test support, never production.

use suzatary_base::capability::Nothing;

impl crate::upload_index::UploadIndex for Nothing {
    fn refresh(
        &self,
        named: &std::collections::BTreeSet<crate::attachment::AttachmentId>,
    ) -> Result<
        std::collections::BTreeMap<
            crate::attachment::AttachmentId,
            crate::secretary::upload_standing::UploadStanding,
        >,
        crate::upload_index::IndexError,
    > {
        Ok(named
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    crate::secretary::upload_standing::UploadStanding::Erased,
                )
            })
            .collect())
    }
    fn open(
        &self,
        _id: &crate::attachment::AttachmentId,
    ) -> Result<Option<crate::file_read::FileRead>, crate::upload_index::IndexError> {
        Ok(None)
    }
}

impl crate::pdf_text::PdfText for Nothing {
    fn layer(&self, _: &[u8]) -> Result<crate::file_read::Layer, String> {
        Err("this test reads no PDF".to_owned())
    }
}
