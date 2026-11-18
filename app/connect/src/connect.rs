//! What is connected, and how: a [`Connection`] is a [`Template`] applied to parameters.

mod catalog;
mod check;
mod connection;
mod declared;
mod group;
mod params;
mod reach;
mod routine;
mod seen;
mod standing;
mod step;
mod template;

pub(crate) use catalog::secret_words;
pub use catalog::{CATALOG, offered, plugin_called};
pub use check::Check;
pub use connection::{CONNECTION, Connection, Key};
pub use declared::Declared;
pub use group::Group;
pub use params::{Params, Shape};
pub use reach::{PLAIN, Reach, Run};
pub use routine::{Coding, NotARoutine, ROUTINE_TOKEN, RoutineId};
pub use seen::{Checked, Seen};
pub use standing::Standing;
pub use step::{SecretName, Step, StepShape};
pub use template::{AccountKind, Channel, Fragment, Table, Template, TemplateId, channel_secrets};
