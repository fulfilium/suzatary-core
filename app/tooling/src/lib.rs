//! The merge train's typed program. `scripts/train.sh` calls it for each verb it has handed over;
//! today that is the verdict table (`train table read|write`).
#![forbid(unsafe_code)]

pub mod command;
pub mod error;
pub mod gate;
pub mod git;
pub mod half;
pub mod instant;
pub mod land_order;
pub mod oid;
pub mod outcome;
pub mod reading;
pub mod row;
pub mod table;

pub use command::Command;
pub use error::Error;
pub use half::Half;
pub use land_order::LandOrder;
pub use outcome::{Ended, Outcome};
pub use reading::Reading;
pub use row::Row;
pub use table::{Scope, Seen, Table};
