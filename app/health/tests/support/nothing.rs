//! The inert health register: test support, never production.

use suzatary_base::capability::Nothing;

impl crate::health::Register<crate::health::SourceKey> for Nothing {
    fn healths(
        &self,
    ) -> Result<crate::health::Healths<crate::health::SourceKey>, suzatary_store::store::StoreError>
    {
        Ok(crate::health::Healths::default())
    }
}
