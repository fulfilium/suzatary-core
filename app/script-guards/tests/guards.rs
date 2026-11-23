//! The guards over the repository's scripts that need nothing of the application: each runs a
//! script (the deploy, the merge gate, the browser lane's servers, the signature adoption, the
//! vault backup, the credential helper) against stub programs in a throwaway repository and asserts
//! on what it did. They build in seconds and read no member's code but this crate's and the
//! testkit's, so the gate keeps their pass across a change to the application. The guards that run
//! the binary or read the sources are the CLI's (`app/cli/tests/guards.rs`).

#[path = "guards/adopt_signatures.rs"]
mod adopt_signatures;
#[path = "guards/credential.rs"]
mod credential;
#[path = "guards/deploy.rs"]
mod deploy;
#[path = "guards/e2e_servers.rs"]
mod e2e_servers;
#[path = "guards/gate_cache.rs"]
mod gate_cache;
#[path = "support/gate_list.rs"]
#[expect(dead_code, reason = "the CLI's guards read a step's lane and command")]
mod gate_list;
#[path = "guards/gate_runs.rs"]
mod gate_runs;
#[path = "support/process.rs"]
mod process;
#[path = "support/scratch.rs"]
mod scratch;
#[path = "guards/vault.rs"]
mod vault;

use scratch::{Scratch, repo};
