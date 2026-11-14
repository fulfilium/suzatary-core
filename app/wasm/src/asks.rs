//! The box's refusals of what the tab posted, as a replica the tab pulls and never writes; each
//! store the tab writes reads its own refusals out of it (`Store::refused_in`).

use suzatary_core::asks::AskEvent;

crate::object_rows::objects_store!(Asks = AskEvent, unread);
