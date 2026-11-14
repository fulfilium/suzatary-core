//! The inert device log: test support, never production.

use suzatary_base::capability::Nothing;
use suzatary_base::refusal::Refusal;
use suzatary_store::store::{Decide, Edit, Log, StoreError};

use crate::devices::Device;

/// No phone is registered, and a registration is refused.
impl Log<String, Device> for Nothing {
    fn keys(&self) -> Result<Vec<String>, StoreError> {
        Ok(Vec::new())
    }
    fn lines(&self, _actor: &String) -> Result<Vec<Device>, StoreError> {
        Ok(Vec::new())
    }
    fn start(&self, _actor: &String, _first: &Device) -> Result<(), StoreError> {
        Err(StoreError::unwritable(
            "device store",
            "nothing is registered",
        ))
    }
    fn push(&self, _actor: &String, _line: &Device) -> Result<(), StoreError> {
        Err(StoreError::unwritable(
            "device store",
            "nothing is registered",
        ))
    }
    fn edit(
        &self,
        _actor: &String,
        _decide: &mut Decide<'_, Device>,
    ) -> Result<Edit<Device>, Refusal> {
        Err(StoreError::unwritable("device store", "nothing is registered").into())
    }
}
