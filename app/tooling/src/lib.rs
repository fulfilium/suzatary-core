//! The merge train's typed program. `scripts/train.sh` calls it for each verb it has handed over:
//! the verdict table (`train table read|write`) and publication (`train publish`, `publishable`,
//! `push-gate`, `published`).
#![forbid(unsafe_code)]

pub mod admission;
pub mod admitted;
pub mod command;
pub mod core_rows;
pub mod error;
pub mod fresh;
pub mod gate;
pub mod gate_branch;
pub mod git;
pub mod half;
pub mod instant;
pub mod known;
pub mod land_order;
pub mod leak_check;
pub mod matcher;
pub mod oid;
pub mod outcome;
pub mod plan;
pub mod process;
pub mod public_tree;
pub mod publication;
pub mod publishable;
pub mod reading;
pub mod refusal;
pub mod repo_path;
pub mod row;
pub mod scratch;
pub mod table;

pub use command::Command;
pub use error::Error;
pub use half::Half;
pub use land_order::LandOrder;
pub use outcome::{Ended, Outcome};
pub use reading::Reading;
pub use row::Row;
pub use table::{Scope, Seen, Table};
