//! Generates the Junk churn seed's `domains.txt` and `sources.txt` from maintained public
//! lists' raw text; `flake.nix`'s `suzatary-junk-seed` derivation is the one caller.
//!
//! Every source goes through [`suzatary_triage::mail::junk::normalize_seed_file`], so all are
//! normalised alike. `domains.txt` is their sorted union; `sources.txt` names each source, its
//! rev and its count before the union, so a source going quiet stays visible.
//!
//! Usage: `junk_seed <out-dir> <name> <rev> <file> [<name> <rev> <file> ...]`

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::{env, fs};

use suzatary_triage::mail::junk::{Domain, normalize_seed_file};

fn main() {
    let mut args = env::args().skip(1);
    let out_dir = args
        .next()
        .unwrap_or_else(|| panic!("usage: junk_seed <out-dir> <name> <rev> <file>..."));

    let mut all: BTreeSet<Domain> = BTreeSet::new();
    let mut sources = String::new();
    loop {
        let name = args.next();
        let (Some(name), Some(rev), Some(file)) = (name, args.next(), args.next()) else {
            break;
        };
        let text = fs::read_to_string(&file).unwrap_or_else(|failed| panic!("{file}: {failed}"));
        let domains = normalize_seed_file(&text);
        writeln!(sources, "{name} {rev} {}", domains.len()).expect("String writes never fail");
        all.extend(domains);
    }
    assert!(!sources.is_empty(), "junk_seed: no sources named");

    let mut printed = String::new();
    for domain in &all {
        printed.push_str(domain.as_str());
        printed.push('\n');
    }
    fs::write(format!("{out_dir}/domains.txt"), &printed).expect("domains.txt is writable");
    fs::write(format!("{out_dir}/sources.txt"), &sources).expect("sources.txt is writable");
    println!(
        "domains.txt: {} domains from {} sources",
        all.len(),
        sources.lines().count()
    );
}
