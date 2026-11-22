//! What a reader derives from the lines' words and keeps beside them, one value per type of the
//! reader's own, so the lines name no reader's type and forgetting the words drops every value.

use std::any::Any;

/// Each reader's value, at most one of each type.
#[derive(Default)]
pub struct Derived(Vec<Box<dyn Any + Send>>);

impl Derived {
    /// `read` of the value of type `T` kept here, a default one kept first where none is.
    pub fn with<T: Default + Send + 'static, R>(&mut self, read: impl FnOnce(&mut T) -> R) -> R {
        if let Some(kept) = self.0.iter_mut().find_map(|kept| kept.downcast_mut::<T>()) {
            read(kept)
        } else {
            let mut fresh = T::default();
            let read = read(&mut fresh);
            self.0.push(Box::new(fresh));
            read
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}
