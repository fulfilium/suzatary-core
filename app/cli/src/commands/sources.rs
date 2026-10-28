//! `suzatary sources migrate-health`: the three health registers the sources' one replaces,
//! joined into it.
//!
//! The mail health (`mail-health.json`, keyed by address) and the Drive's (`drive-health.json`,
//! keyed `drive`) lie beside the heartbeat; each plugin ledger's `links` is keyed by credential.
//! Each result is joined into `source-health.json` under its [`SourceKey`]. The join is a
//! semilattice's, so a run twice, a run cut short and run again, and syncs that joined their own
//! results first all read as one run: a failure's episode keeps the instant it began. The old
//! files are left as they are, for the old binary, which reads nothing else.
//!
//! A card is keyed by its episode, so the new binary must never read a failing source before its
//! episode is carried: run the verb with the new binary before the deploy, and again after it to
//! carry what the old binary recorded in between. This module is the one reader of the old forms
//! and goes with the verb.

use std::path::Path;

use anyhow::Context as _;
use clap::{Args as ClapArgs, Subcommand};
use serde_json::{Map, Value};
use suzatary_core::health::{Health, Healths, Register as _};
use suzatary_core::mail::account_email::AccountEmail;
use suzatary_core::mail::address::EmailAddress;
use suzatary_core::source_key::SourceKey;
use suzatary_io::present::Present as _;

use crate::instance::Instance;

/// `suzatary sources …`.
#[derive(Debug, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Join the mail and Drive health registers and each plugin ledger's links into the sources'
    /// health register; run on the box before the deploy and after it, then deleted.
    MigrateHealth,
}

/// Run it.
///
/// # Errors
/// An unreadable instance, register or ledger, or a key that names no source.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let Command::MigrateHealth = args.command;
    let instance = Instance::open(path)?;
    let register = suzatary_io::source_health::beside(&instance.mail.heartbeat);
    let joined = register.join(&carried(&instance)?)?;
    println!(
        "sources migrate-health: {} sources",
        joined.readings().count()
    );
    Ok(())
}

/// The sources whose failure the old registers hold and the register does not, so that a
/// card for it would be filed again under a new episode: what `verify` refuses while any is
/// left, since the new binary must not read the register before the move.
///
/// # Errors
/// As [`run`].
pub fn uncarried(instance: &Instance) -> anyhow::Result<Vec<SourceKey>> {
    let held = suzatary_io::source_health::beside(&instance.mail.heartbeat).healths()?;
    let joined = held.join(&carried(instance)?);
    let episode =
        |healths: &Healths<SourceKey>, key| healths.health(key).and_then(Health::remedial);
    Ok(joined
        .readings()
        .map(|(key, _)| key)
        .filter(|key| {
            episode(&joined, key).is_some() && episode(&held, key) != episode(&joined, key)
        })
        .cloned()
        .collect())
}

/// What the three old registers hold, each result under its source.
fn carried(instance: &Instance) -> anyhow::Result<Healths<SourceKey>> {
    let old = |file: &str| object(&instance.mail.heartbeat.with_file_name(file));
    let mut carried = rekeyed(old("mail-health.json")?, mail)?
        .join(&rekeyed(old("drive-health.json")?, |text| {
            Ok(SourceKey::parse(text)?)
        })?);
    for manifest in &instance.plugins.manifests {
        let file = instance.plugins.beside(manifest.name()).join("ledger.json");
        if let Some(Value::Object(links)) = object(&file)?.remove("links") {
            let name = manifest.name();
            let link = |text: &str| Ok(SourceKey::parse(&format!("link:{name}/{text}"))?);
            carried = carried.join(&rekeyed(links, link)?);
        }
    }
    Ok(carried)
}

/// The JSON object at `file`, empty where there is no file.
fn object(file: &Path) -> anyhow::Result<Map<String, Value>> {
    let Some(bytes) = std::fs::read(file).present()? else {
        return Ok(Map::new());
    };
    serde_json::from_slice(&bytes).with_context(|| format!("reading {}", file.display()))
}

/// The mail register's key as its source: written as the address was before addresses were
/// folded, so `Ada@Example.test` is the account `ada@example.test`.
fn mail(text: &str) -> anyhow::Result<SourceKey> {
    Ok(SourceKey::Mail(AccountEmail::folded(&EmailAddress::new(
        text,
    )?)))
}

/// A register keyed by text, each entry under the source `key` names it, joined: two texts
/// naming one source join as two of its results.
fn rekeyed(
    register: Map<String, Value>,
    key: impl Fn(&str) -> anyhow::Result<SourceKey>,
) -> anyhow::Result<Healths<SourceKey>> {
    register
        .into_iter()
        .try_fold(Healths::default(), |sum, (text, health)| {
            let source = key(&text).with_context(|| format!("the key {text:?}"))?;
            let one = Map::from_iter([(source.to_string(), health)]);
            Ok(sum.join(&serde_json::from_value(Value::Object(one))?))
        })
}

#[cfg(test)]
mod tests {
    use suzatary_core::health::{Outcome, Reading};

    use super::*;

    fn at(hour: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 1)
            .and_then(|day| day.and_hms_opt(hour, 0, 0))
            .expect("an instant")
    }

    /// An old register's entry: a success at `ok`, then a failure at `failed`.
    fn entry(ok: u32, failed: u32) -> Value {
        let failure = serde_json::json!({
            "at": at(failed), "class": {"kind": "needs_login"}, "words": "lapsed"
        });
        serde_json::json!({"ok": at(ok), "failed": failure, "classified": failure, "first": at(0)})
    }

    /// A failing account's episode keeps its beginning across the move, whatever the new
    /// register already holds: a failure the new binary's sync joined first, and the same move
    /// run twice, read as the one move. Its key written in capitals, as the register kept it
    /// before addresses were folded, and in lower case are one account.
    #[test]
    fn a_failures_episode_survives_the_move_and_a_second_run() {
        let old = Map::from_iter([
            ("Ada@Example.test".to_owned(), entry(1, 3)),
            ("ada@example.test".to_owned(), entry(2, 4)),
        ]);
        let key = SourceKey::Mail(AccountEmail::new("ada@example.test").expect("an address"));
        let since = |healths: &Healths<SourceKey>| match healths.reading(&key) {
            Some(Reading::Failing { since, .. }) => since,
            other => panic!("not failing: {other:?}"),
        };
        let lapsed = Outcome::Failed(suzatary_core::connect::Failure::NeedsLogin);
        let synced_first = Healths::one(key.clone(), at(5), lapsed, "lapsed");
        assert_eq!(since(&synced_first), at(5));
        let moved = synced_first.join(&rekeyed(old.clone(), mail).expect("moved"));
        assert_eq!(moved.readings().count(), 1, "two spellings, one account");
        assert_eq!(since(&moved), at(2));
        assert_eq!(moved.join(&rekeyed(old, mail).expect("moved again")), moved);
    }
}
