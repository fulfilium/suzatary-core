//! What is connected, and how a connection is made: one algebra of templates and connections.
//!
//! A [`Template`] is data: its group, the table of the instance file it writes, the steps a
//! person takes and how the connection proves it works. A [`Connection`] is a template applied
//! to the parameters a person chose, so its tables, steps and check are functions of it. No
//! table is added for it: the connections a deployment has are read back out of the tables it
//! already writes ([`Declared::connections`]), and adding one is writing its table
//! ([`Declared::with`]). Whether one works is a reading too, [`Standing`], of the instance, the
//! secrets that resolve and its check's results, never a flag kept beside them.

mod catalog;
mod check;
mod connection;
mod declared;
mod failure;
mod group;
mod label;
mod params;
mod reach;
mod refused;
mod seen;
mod standing;
mod step;
mod template;

pub use catalog::{CATALOG, plugin_called, secret_words};
pub use check::Check;
pub use connection::{CONNECTION, Connection, Key};
pub use declared::Declared;
pub use failure::{Failure, Remedy, Revocation};
pub use group::Group;
pub use label::Label;
pub use params::{Params, Shape};
pub use reach::{PLAIN, Reach, Run};
pub use refused::Refused;
pub use seen::{Checked, Seen};
pub use standing::Standing;
pub(crate) use standing::stamp;
pub use step::{SecretName, Step, StepShape};
pub use template::{AccountKind, Channel, Fragment, Table, Template, TemplateId};
