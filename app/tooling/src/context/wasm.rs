//! `web/wasm-budget` against the merge base: a line rises by the allowance or a `Wasm-growth:`.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, bail};

use super::tree::git;

pub(crate) const BUDGET: &str = "web/wasm-budget";

/// The bytes a line may rise by against the merge base without a word, per module and system.
const ALLOWANCE: u64 = 16_000;

type Lines = BTreeMap<(String, String), u64>;

/// Each line by its module and system: `<module>.wasm <system> <bytes>`.
fn lines(text: &str) -> anyhow::Result<Lines> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.split_whitespace().collect::<Vec<_>>()[..] {
            [name, system, bytes] => Ok((
                (name.to_owned(), system.to_owned()),
                bytes
                    .parse()
                    .with_context(|| format!("{BUDGET}: `{bytes}` is not a number of bytes"))?,
            )),
            _ => bail!("{BUDGET}: `{l}` is not `<module>.wasm <system> <bytes>`"),
        })
        .collect()
}

/// What the trailers' values declare each module may grow by, summed, and each value not in the
/// form, refused, so a declaration is never read as something it does not say.
fn grants(values: &[String]) -> (BTreeMap<String, u64>, Vec<String>) {
    let (mut grants, mut malformed) = (BTreeMap::new(), Vec::new());
    for said in values {
        let parsed = said.split_once(':').and_then(|(head, why)| {
            let (name, bytes) = head.trim().split_once(" +")?;
            let bytes: u64 = bytes.trim().parse().ok()?;
            (!why.trim().is_empty()).then(|| (name.trim().to_owned(), bytes))
        });
        match parsed {
            Some((name, bytes)) => *grants.entry(name).or_insert(0) += bytes,
            None => malformed.push(format!(
                "`Wasm-growth: {said}` is not `Wasm-growth: <module> +<bytes>: <reason>`"
            )),
        }
    }
    (grants, malformed)
}

/// Each line raised past the allowance and past what the grants declare for its module.
fn raised(was: &Lines, now: &Lines, grants: &BTreeMap<String, u64>) -> Vec<String> {
    was.iter()
        .filter_map(|((name, system), &before)| {
            let after = *now.get(&(name.clone(), system.clone()))?;
            let granted = grants.get(name).copied().unwrap_or(0);
            (after > before + ALLOWANCE.max(granted)).then(|| {
                format!(
                    "{BUDGET}: {name} {system} rises {before} → {after}, past the \
                     {ALLOWANCE}-byte allowance and the {granted} bytes declared (a `Wasm-growth: \
                     <module> +<bytes>: <reason>` line in the pull request's description, or in a \
                     commit)"
                )
            })
        })
        .collect()
}

/// Each refused growth of the checkout at `root` against its merge base; none without a merge
/// base, or where the merge base held no budget.
///
/// # Errors
/// When git or the budget cannot be read.
pub(crate) fn refused(root: &Path, base: &str) -> anyhow::Result<Vec<String>> {
    let Ok(was) = git(root, &["show", &format!("{base}:{BUDGET}")]) else {
        return Ok(Vec::new());
    };
    let now =
        std::fs::read_to_string(root.join(BUDGET)).with_context(|| format!("reading {BUDGET}"))?;
    let values = crate::git::declared(root, &format!("{base}..HEAD"), "Wasm-growth")
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let (grants, mut refused) = grants(&values);
    refused.extend(raised(&lines(&was)?, &lines(&now)?, &grants));
    Ok(refused)
}

#[cfg(test)]
mod tests {
    use super::{ALLOWANCE, grants, raised};
    use std::collections::BTreeMap;

    /// A line rises by the allowance, or by what is declared for its module; a declaration not in
    /// its form is refused, and so is a growth past both.
    #[test]
    fn a_line_rises_by_the_allowance_or_by_what_is_declared() {
        let line = |name: &str, bytes| ((name.to_owned(), "x86_64-linux".to_owned()), bytes);
        let was = BTreeMap::from([line("views_bg.wasm", 490_000)]);
        let at = |bytes| BTreeMap::from([line("views_bg.wasm", bytes)]);
        assert!(raised(&was, &at(490_000 + ALLOWANCE), &BTreeMap::new()).is_empty());
        assert_eq!(
            raised(&was, &at(490_001 + ALLOWANCE), &BTreeMap::new()).len(),
            1
        );
        let (said, malformed) = grants(&["views_bg.wasm +19000: the topic bar".to_owned()]);
        assert!(malformed.is_empty() && raised(&was, &at(509_000), &said).is_empty());
        assert_eq!(raised(&was, &at(509_001), &said).len(), 1);
        let both = |views, prodrome| {
            BTreeMap::from([
                line("views_bg.wasm", views),
                line("prodrome_bg.wasm", prodrome),
            ])
        };
        let was = both(490_000, 1_100_000);
        assert!(
            raised(&was, &both(490_000, 1_090_000), &BTreeMap::new()).is_empty(),
            "a fall"
        );
        assert_eq!(
            raised(&was, &both(509_000, 1_119_000), &said).len(),
            1,
            "a declaration is its module's alone"
        );
        let (_, malformed) = grants(&["views_bg.wasm +19000:".to_owned()]);
        assert_eq!(malformed.len(), 1, "no reason");
        let (_, malformed) = grants(&["views_bg.wasm 19000 bytes: why".to_owned()]);
        assert_eq!(malformed.len(), 1, "no + before the bytes");
        let prose =
            "a change\n\nIts lines ride its merge beside\nWasm-growth:. Then the build runs.\n";
        assert!(
            crate::git::trailers("Wasm-growth", prose)
                .expect("git reads")
                .is_empty(),
            "a line of prose that begins with the key is no declaration"
        );
    }
}
