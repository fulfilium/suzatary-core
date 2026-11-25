//! `suzatary device init|pair|trust|revoke|list`: the box's terminal door to who may write (SPEC
//! law 42).
//!
//! Trusted keys are history: `trust` writes a `KeyAdded` and `revoke` a `KeyRevoked` into every
//! private chain, both signed by the root key.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, bail};
use clap::Subcommand;
use prodrome_data::event::Actor;
use prodrome_data::sign::PublicKey;
use suzatary_mail::mail::dedup_key::DEDUP_KEY;
use suzatary_signatures::signatures::{AGENT_KEY, BOX_KEY, BoxKeys, Keeping, Keyed, Trust};

use super::store_of;
use crate::instance::{ChainDir, Instance};
use suzatary_base::capability;
use suzatary_signatures::signatures;
use suzatary_signatures::signatures::MINUTES;
use suzatary_store::store::Reads as _;

#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// Mint the box's and the agent's keys and the mail's dedup key into secretspec (keeping
    /// any already there), check them against the declared root, and adopt now, refusing while a
    /// write since the epoch carries no signature, and repairing a box adopted before custody:
    /// each actor left speaking with the key that signs its writes, the box's key revoked for
    /// the agent's actor. Says each act; run again, it does nothing.
    Init,
    /// List every write since the epoch that carries no signature (its name, chain, actor and
    /// instant), and with `--yes` sign exactly those, each by the key its actor owns: the repair
    /// for a crash between an append and its signature.
    SignUnsigned {
        /// Sign what is listed; without it, nothing is written.
        #[arg(long)]
        yes: bool,
    },
    /// Print a short code that trusts the one device which types it on its trust card: it works
    /// once, for ten minutes, and dies after five wrong tries. A new code replaces the last.
    Pair {
        /// Whom the device speaks for; the first chain's owner when absent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Trust a device's public key for an actor on every private chain, as a tapped Inbox card
    /// does, and withdraw its card in the Inbox.
    Trust {
        /// The device's public key, 64 lowercase hex, as its card shows it.
        key: String,
        /// Whom it speaks for; the first chain's owner when absent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Revoke a key on every private chain, all or none: a chain that fails is named and the
    /// revoke is run again until every one holds it (and the next writer to start completes it).
    /// What it signed until now stays proven, nothing after.
    Revoke {
        /// The key, 64 lowercase hex.
        key: String,
        /// The one actor it stops speaking for, every other keeping it; absent, every actor it
        /// speaks for, as for a stolen device.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Every registered key on every private chain, and its standing.
    List,
}

pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    match &args.action {
        Action::Init => init(&instance),
        Action::Pair { actor } => pair(&instance, actor.as_deref()),
        Action::Trust { key, actor } => {
            let keyed = keyed(&instance)?;
            let (keys, key, actor) = asked(&instance, key, actor.as_deref())?;
            for chain in keyed.iter() {
                signatures::trust(&store_of(chain), keys.root(), &actor, &key)
                    .with_context(|| format!("registering the key on {}", chain.policy.name))?;
            }
            println!(
                "trusted {} for {actor} on every private chain",
                key.as_str()
            );
            let withdrawn = suzatary_server::secretary::routes::withdraw_trusted(
                &instance.inbox_store(),
                &store_of(keyed.first()),
                &keys.root().public(),
                capability::Clock::now(&suzatary_io_local::clock::SystemClock),
            )
            .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
            if !withdrawn.is_empty() {
                println!("withdrew its card in the Inbox");
            }
            Ok(())
        }
        Action::Revoke { key, actor } => revoke(&instance, key, actor.as_deref()),
        Action::List => list(&instance),
        Action::SignUnsigned { yes } => sign_unsigned(&instance, *yes),
    }
}

/// The key revoked for the actor named, or for every actor it speaks for on any private chain,
/// on every private chain, all or none.
fn revoke(instance: &Instance, key: &str, actor: Option<&str>) -> anyhow::Result<()> {
    let keyed = keyed(instance)?;
    let (keys, key, actors) = if let Some(named) = actor {
        let (keys, key, actor) = asked(instance, key, Some(named))?;
        (keys, key, BTreeSet::from([actor]))
    } else {
        let keys = held_keys(instance)?;
        let key = PublicKey::new(key).map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut actors = BTreeSet::new();
        for chain in keyed.iter() {
            actors.extend(
                signatures::trusted_for(&store_of(chain), &keys.root().public(), &key)
                    .with_context(|| format!("reading {}", chain.policy.name))?,
            );
        }
        if actors.is_empty() {
            bail!("{} speaks for no actor on any chain", key.as_str());
        }
        (keys, key, actors)
    };
    // Every chain is tried, whatever one answers, so a failure leaves the fewest behind.
    let failed: Vec<String> = keyed
        .iter()
        .flat_map(|chain| actors.iter().map(move |actor| (chain, actor)))
        .filter_map(|(chain, actor)| {
            signatures::revoke(&store_of(chain), keys.root(), actor, &key)
                .err()
                .map(|why| format!("{} ({actor}): {why}", chain.policy.name))
        })
        .collect();
    if !failed.is_empty() {
        bail!(
            "the key is not yet revoked on every private chain — run this again until it is:\n{}",
            failed.join("\n")
        );
    }
    for actor in &actors {
        println!(
            "revoked {} for {actor} on every private chain",
            key.as_str()
        );
    }
    // A revoked key's device is told to wipe what it holds, and nothing more: the login it
    // reads as is the tailnet's, which this box cannot take back.
    println!(
        "its device deletes what it holds the next time it asks; it still reads as its Tailscale \
         login until it is removed from the tailnet, which is the real cut-off"
    );
    Ok(())
}

/// The unsigned writes listed, private chain by private chain, and signed only when asked.
fn sign_unsigned(instance: &Instance, yes: bool) -> anyhow::Result<()> {
    use prodrome_data::instant::{instant_of, iso};
    let signers = signatures::Signers::Held(held_keys(instance)?);
    let root = instance.root();
    let mut listed = 0;
    for chain in instance.keeping().iter() {
        let _held = chain_lock(chain)?;
        let store = store_of(chain);
        let writes = signatures::unsigned_writes(&store, root)
            .with_context(|| format!("reading {}", chain.policy.name))?;
        for write in &writes {
            println!(
                "{}\t{}\t{}\t{}",
                write.name.as_str(),
                chain.policy.name,
                write.actor,
                iso(instant_of(write.at))
            );
        }
        listed += writes.len();
        if yes {
            let signed =
                signatures::sign_unsigned(&store, &signers, &chain.policy.untrusted, &writes)
                    .with_context(|| format!("signing on {}", chain.policy.name))?;
            println!("{}: signed {} write(s)", chain.policy.name, signed.len());
        }
    }
    if !yes && listed > 0 {
        bail!("{listed} unsigned write(s) listed above; nothing was signed — run again with --yes");
    }
    Ok(())
}

fn held_keys(instance: &Instance) -> anyhow::Result<BoxKeys> {
    match instance.signers() {
        signatures::Signers::Held(keys) => Ok(keys),
        signatures::Signers::Missing(why) => bail!("{why}"),
    }
}

/// Mint a code for `actor`, keep its hash in place of any earlier code, and print it: the one
/// place a code is ever shown.
fn pair(instance: &Instance, actor: Option<&str>) -> anyhow::Result<()> {
    use suzatary_signatures::signatures::Pairings as _;
    // The server registers the key with the root, so a box that cannot sign pairs nothing.
    held_keys(instance)?;
    keyed(instance)?;
    let actor = on_every_roster(instance, actor)?;
    let (code, salt) = suzatary_io_local::pairing::fresh().map_err(|e| anyhow::anyhow!(e))?;
    let now = chrono::Utc::now();
    let minted = signatures::Pairing::minted(&code, salt, &actor, now);
    suzatary_io_local::pairing::Pairings::at(instance.paths.pairing.clone())
        .rewrite(&mut |_| Some(minted.clone()))?;
    let until = now + chrono::TimeDelta::minutes(MINUTES);
    println!("Pairing code for {actor}: {}", code.shown());
    println!();
    println!(
        "On the new device, open Settings, then This device, tap \"Enter the code from the box\" \
         and type the code."
    );
    println!(
        "It trusts one device, for {} minutes, until {} UTC. After {} wrong tries it stops \
         working. A new code replaces this one.",
        MINUTES,
        until.format("%H:%M"),
        signatures::TRIES,
    );
    Ok(())
}

fn keyed(instance: &Instance) -> anyhow::Result<Keyed<&ChainDir>> {
    match instance.keeping() {
        Keeping::Keyed(keyed) => Ok(keyed),
        Keeping::Keyless => bail!("every chain is public, and a public chain keeps no key"),
    }
}

fn asked(
    instance: &Instance,
    key: &str,
    actor: Option<&str>,
) -> anyhow::Result<(BoxKeys, PublicKey, Actor)> {
    let keys = held_keys(instance)?;
    let key = PublicKey::new(key).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok((keys, key, on_every_roster(instance, actor)?))
}

/// The actor named, or the first chain's owner, refused unless every private chain's roster
/// holds it.
fn on_every_roster(instance: &Instance, actor: Option<&str>) -> anyhow::Result<Actor> {
    let actor = match actor {
        Some(named) => Actor::new(named).map_err(|e| anyhow::anyhow!("{e}"))?,
        None => instance.chains.first().owner.clone(),
    };
    if !instance
        .keeping()
        .iter()
        .all(|chain| chain.policy.roster.contains(&actor))
    {
        bail!("{actor} is not on every private chain's roster");
    }
    Ok(actor)
}

/// The box's keys and the mail's dedup key, each seed kept where the store holds one and minted
/// only where it says it holds none: a store that could not answer may hold the box's key, and
/// minting over it would orphan every signature it made.
pub(crate) fn kept_or_minted(
    get: impl Fn(&str) -> anyhow::Result<Option<capability::Secret>>,
    set: impl Fn(&str, &capability::Secret) -> anyhow::Result<()>,
) -> anyhow::Result<BoxKeys> {
    let seed = |name: &str| -> anyhow::Result<capability::Secret> {
        if let Some(held) =
            get(name).with_context(|| format!("reading {name}; nothing was minted"))?
        {
            return Ok(held);
        }
        let fresh = suzatary_io_access::secrets::fresh_seed().map_err(|e| anyhow::anyhow!(e))?;
        set(name, &fresh).with_context(|| format!("storing {name}"))?;
        Ok(fresh)
    };
    // In this order, so a store that fails on the agent's key has kept or minted the box's.
    let (boxed, agent, dedup) = (seed(BOX_KEY)?, seed(AGENT_KEY)?, seed(DEDUP_KEY)?);
    suzatary_mail::mail::dedup_key::DedupKey::of(&dedup)?;
    let root = prodrome_dag::sign::Secret::new(boxed.expose())
        .map_err(|_| anyhow::anyhow!("{BOX_KEY} is not a key's seed"))?
        .public();
    Ok(BoxKeys::of(&root, &boxed, &agent)?)
}

pub(crate) fn kept_or_minted_in(
    secrets: &suzatary_io_access::secrets::Secretspec,
) -> anyhow::Result<BoxKeys> {
    let reason = "init: the box's own keys";
    kept_or_minted(
        |name| Ok(secrets.get(name, reason)?),
        |name, seed| Ok(secrets.set(name, seed, reason)?),
    )
}

fn init(instance: &Instance) -> anyhow::Result<()> {
    let keys = kept_or_minted_in(&instance.secrets())?;
    let root = keys.root().public();
    let declared = instance.root();
    if *declared != root {
        bail!(
            "the instance declares the root {declared} and {BOX_KEY} is {root}: one of them is \
             not this box's"
        );
    }
    let signers = signatures::Signers::Held(keys.clone());
    // Every chain is tried, whatever one answers, as a revoke's are: each that adopted is
    // repaired, and every failure of either step is said together.
    let adopted = instance.adopt(&signers);
    let mut failed = adopted.failed;
    for (chain, registered) in adopted.chains {
        let name = &chain.policy.name;
        for (actor, key) in registered {
            println!("{name}: registered {} for {actor}", key.as_str());
        }
        if let Err(why) = repair(chain, &keys) {
            failed.push(format!("{name}: {why:#}"));
        }
    }
    if !failed.is_empty() {
        bail!(
            "not every chain has adopted and been repaired — run `suzatary device init` again \
             until it has:\n{}",
            failed.join("\n")
        );
    }
    println!(
        "Every private chain has adopted the root {}, each actor registered with the key that \
         signs its writes.",
        root.as_str()
    );
    Ok(())
}

/// Revoke each box key registered for an actor whose writes the other box key signs, so each
/// actor speaks only with the key that signs its writes. Idempotent.
fn repair(chain: &ChainDir, keys: &BoxKeys) -> anyhow::Result<()> {
    let name = &chain.policy.name;
    let _held = chain_lock(chain)?;
    for (actor, key) in signatures::revoke_misplaced(&store_of(chain), keys, &chain.policy)? {
        println!("{name}: revoked {} for {actor}", key.as_str());
    }
    Ok(())
}

/// One line per registered key per private chain.
fn list(instance: &Instance) -> anyhow::Result<()> {
    let registrar = instance.chains.first().policy.registrar();
    for chain in instance.keeping().iter() {
        let dag: prodrome_dag::dag::Dag<suzatary_record::record::TodoEvent> = store_of(chain)
            .objects()
            .with_context(|| format!("reading {}", chain.policy.name))?
            .into_iter()
            .collect();
        let trust = Trust::of(&dag, &registrar);
        for (_, actor, key, standing) in trust.keys() {
            println!(
                "{}\t{actor}\t{}\t{}",
                chain.policy.name,
                key.as_str(),
                standing.as_str()
            );
        }
    }
    Ok(())
}

/// The adoption lock, so a repair and a starting writer do not interleave.
fn chain_lock(chain: &ChainDir) -> anyhow::Result<suzatary_os::durable::Held> {
    crate::instance::adoption_lock(&chain.events)
}
