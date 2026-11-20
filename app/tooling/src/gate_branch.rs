//! The name of a branch the train may push with force.

use crate::Error;
use std::fmt;

/// A branch under `gate/`, the only kind `push-gate` force-pushes: a mistyped name can never
/// rewrite core's master or any branch a deployment reads.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GateBranch(String);

impl GateBranch {
    pub fn parse(name: &str) -> Result<GateBranch, Error> {
        let rest = name.strip_prefix("gate/").unwrap_or_default();
        let plain = |c: char| c.is_ascii_alphanumeric() || "-_./".contains(c);
        if rest.is_empty() || !rest.chars().all(plain) || rest.contains("..") || rest.ends_with('/')
        {
            return Err(Error::Usage(format!(
                "{name:?} is no gate branch: gate/<letters, digits, - _ . />"
            )));
        }
        Ok(GateBranch(name.to_owned()))
    }
}

impl fmt::Display for GateBranch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
