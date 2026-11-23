//! The merge train's typed program. `scripts/train.sh` calls it for each verb it has handed over:
//! the verdict table (`train table read|write`) and publication (`train publish`, `publishable`,
//! `push-gate`, `published`).
#![forbid(unsafe_code)]

pub mod admission;
pub mod admitted;
pub mod bisect;
pub mod command;
pub mod context;
pub mod core_rows;
pub mod data;
pub mod error;
pub mod fresh;
pub mod gate;
pub mod gate_branch;
pub mod gate_run;
pub mod gating;
pub mod git;
pub mod half;
pub mod instant;
pub mod known;
pub mod land_order;
pub mod leak_check;
pub mod lock;
pub mod matcher;
pub mod moved;
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
pub mod supervisor;
pub mod table;
pub mod verbs;
pub mod watch;

pub use command::Command;
pub use error::Error;
pub use half::Half;
pub use land_order::LandOrder;
pub use outcome::{Ended, Outcome};
pub use reading::Reading;
pub use row::Row;
pub use table::{Scope, Seen, Table};
