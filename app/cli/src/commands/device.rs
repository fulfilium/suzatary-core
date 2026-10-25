//! `suzatary device init|pair|trust|revoke|list`: the box's terminal door to who may write (SPEC
//! law 42).
//!
//! The trusted keys are history, not configuration: each `trust` writes a `KeyAdded` into every
//! chain and each `revoke` a `KeyRevoked`, both signed by the root, the box's own key. The
//! Inbox's device card is the same write behind a tap (`suzatary_core::signatures::trust`), and
//! `pair` prints a short code with which the server makes the same write for the one device
//! that types it (`suzatary_core::signatures::Pairing`).

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, bail};
use clap::Subcommand;
use prodrome_core::event::Actor;
use prodrome_core::sign::PublicKey;
use suzatary_core::mail::dedup_key::DEDUP_KEY;
use suzatary_core::signatures::{AGENT_KEY, BOX_KEY, BoxKeys, Trust};

use super::store_of;
use crate::instance::Instance;
use suzatary_core::store::Reads as _;

/// Which act.
#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
}

/// The acts.
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
    /// Trust a device's public key for an actor on every chain, as a tapped Inbox card does,
    /// and withdraw its card in the Inbox.
    Trust {
        /// The device's public key, 64 lowercase hex, as its card shows it.
        key: String,
        /// Whom it speaks for; the first chain's owner when absent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Revoke a key on every chain, all or none: a chain that fails is named and the revoke is
    /// run again until every chain holds it (and the next writer to start completes it). What
    /// it signed until now stays proven, nothing after.
    Revoke {
        /// The key, 64 lowercase hex.
        key: String,
        /// The one actor it stops speaking for, every other keeping it; absent, every actor it
        /// speaks for, as for a stolen device.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Every registered key on every chain, and its standing.
    List,
}

/// `suzatary device <act>`.
///
/// # Errors
/// A key or actor that is not one, a deployment with no keys, or a store failing.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    match &args.action {
        Action::Init => init(&instance),
        Action::Pair { actor } => pair(&instance, actor.as_deref()),
        Action::Trust { key, actor } => {
            let (keys, key, actor) = asked(&instance, key, actor.as_deref())?;
            for chain in &instance.chains {
                suzatary_core::signatures::trust(&store_of(chain), keys.root(), &actor, &key)
                    .with_context(|| format!("registering the key on {}", chain.policy.name))?;
            }
            println!("trusted {} for {actor} on every chain", key.as_str());
            let withdrawn = suzatary_core::secretary::routes::withdraw_trusted(
                &instance.inbox_store(),
                &store_of(instance.first()),
                &keys.root().public(),
                suzatary_core::capability::Clock::now(&suzatary_io::clock::SystemClock),
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

/// The key revoked for the actor named, or for every actor it speaks for on any chain, on every
/// chain, all or none.
fn revoke(instance: &Instance, key: &str, actor: Option<&str>) -> anyhow::Result<()> {
    let (keys, key, actors) = if let Some(named) = actor {
        let (keys, key, actor) = asked(instance, key, Some(named))?;
        (keys, key, BTreeSet::from([actor]))
    } else {
        let keys = held_keys(instance)?;
        let key = PublicKey::new(key).map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut actors = BTreeSet::new();
        for chain in &instance.chains {
            actors.extend(
                suzatary_core::signatures::trusted_for(
                    &store_of(chain),
                    &keys.root().public(),
                    &key,
                )
                .with_context(|| format!("reading {}", chain.policy.name))?,
            );
        }
        if actors.is_empty() {
            bail!("{} speaks for no actor on any chain", key.as_str());
        }
        (keys, key, actors)
    };
    // Every chain is tried, whatever one answers, so a failure leaves the fewest behind.
    let failed: Vec<String> = instance
        .chains
        .iter()
        .flat_map(|chain| actors.iter().map(move |actor| (chain, actor)))
        .filter_map(|(chain, actor)| {
            suzatary_core::signatures::revoke(&store_of(chain), keys.root(), actor, &key)
                .err()
                .map(|why| format!("{} ({actor}): {why}", chain.policy.name))
        })
        .collect();
    if !failed.is_empty() {
        bail!(
            "the key is not yet revoked on every chain — run this again until it is:\n{}",
            failed.join("\n")
        );
    }
    for actor in &actors {
        println!("revoked {} for {actor} on every chain", key.as_str());
    }
    // A revoked key's device is told to wipe what it holds, and nothing more: the login it
    // reads as is the tailnet's, which this box cannot take back.
    println!(
        "its device deletes what it holds the next time it asks; it still reads as its Tailscale \
         login until it is removed from the tailnet, which is the real cut-off"
    );
    Ok(())
}

/// The unsigned writes listed, chain by chain, and signed only when asked.
fn sign_unsigned(instance: &Instance, yes: bool) -> anyhow::Result<()> {
    use prodrome_core::fpl::{instant_of, iso};
    let signers = suzatary_core::signatures::Signers::Held(held_keys(instance)?);
    let root = instance.root();
    let mut listed = 0;
    for chain in &instance.chains {
        let _held = chain_lock(chain)?;
        let store = store_of(chain);
        let writes = suzatary_core::signatures::unsigned_writes(&store, root)
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
            let signed = suzatary_core::signatures::sign_unsigned(
                &store,
                &signers,
                &chain.policy.untrusted,
                &writes,
            )
            .with_context(|| format!("signing on {}", chain.policy.name))?;
            println!("{}: signed {} write(s)", chain.policy.name, signed.len());
        }
    }
    if !yes && listed > 0 {
        bail!("{listed} unsigned write(s) listed above; nothing was signed — run again with --yes");
    }
    Ok(())
}

/// The box's keys, or why this box cannot sign.
fn held_keys(instance: &Instance) -> anyhow::Result<BoxKeys> {
    match instance.signers() {
        suzatary_core::signatures::Signers::Held(keys) => Ok(keys),
        suzatary_core::signatures::Signers::Missing(why) => bail!("{why}"),
    }
}

/// Mint a code for `actor`, keep its hash in place of any earlier code, and print it: the one
/// place a code is ever shown.
fn pair(instance: &Instance, actor: Option<&str>) -> anyhow::Result<()> {
    use suzatary_core::signatures::Pairings as _;
    // The server registers the key with the root, so a box that cannot sign pairs nothing.
    held_keys(instance)?;
    let actor = on_every_roster(instance, actor)?;
    let (code, salt) = suzatary_io::pairing::fresh().map_err(|e| anyhow::anyhow!(e))?;
    let now = chrono::Utc::now();
    let minted = suzatary_core::signatures::Pairing::minted(&code, salt, &actor, now);
    suzatary_io::pairing::Pairings::at(instance.paths.pairing.clone())
        .rewrite(&mut |_| Some(minted.clone()))?;
    let until = now + chrono::TimeDelta::minutes(suzatary_core::signatures::MINUTES);
    println!("Pairing code for {actor}: {}", code.shown());
    println!();
    println!(
        "On the new device, open Settings, then This device, tap \"Enter the code from the box\" \
         and type the code."
    );
    println!(
        "It trusts one device, for {} minutes, until {} UTC. After {} wrong tries it stops \
         working. A new code replaces this one.",
        suzatary_core::signatures::MINUTES,
        until.format("%H:%M"),
        suzatary_core::signatures::TRIES,
    );
    Ok(())
}

/// The keys, the key named and the actor it is for, each checked.
fn asked(
    instance: &Instance,
    key: &str,
    actor: Option<&str>,
) -> anyhow::Result<(BoxKeys, PublicKey, Actor)> {
    let keys = held_keys(instance)?;
    let key = PublicKey::new(key).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok((keys, key, on_every_roster(instance, actor)?))
}

/// The actor named, or the first chain's owner, refused unless every chain's roster holds it.
fn on_every_roster(instance: &Instance, actor: Option<&str>) -> anyhow::Result<Actor> {
    let actor = match actor {
        Some(named) => Actor::new(named).map_err(|e| anyhow::anyhow!("{e}"))?,
        None => instance.chains.first().owner.clone(),
    };
    if !instance
        .chains
        .iter()
        .all(|chain| chain.policy.roster.contains(&actor))
    {
        bail!("{actor} is not on every chain's roster");
    }
    Ok(actor)
}

/// The box's keys and the mail's dedup key, each seed kept where the store holds one and minted
/// from the system's randomness, then stored, only where it says it holds none: a store that
/// could not answer may hold the box's key, and minting over it would orphan every signature it
/// made. `suzatary init` and `device init` alike.
///
/// # Errors
/// The store not answering or refusing a seed, or a seed that is not one.
pub(crate) fn kept_or_minted(
    get: impl Fn(&str) -> anyhow::Result<Option<suzatary_core::capability::Secret>>,
    set: impl Fn(&str, &suzatary_core::capability::Secret) -> anyhow::Result<()>,
) -> anyhow::Result<BoxKeys> {
    let seed = |name: &str| -> anyhow::Result<suzatary_core::capability::Secret> {
        if let Some(held) =
            get(name).with_context(|| format!("reading {name}; nothing was minted"))?
        {
            return Ok(held);
        }
        let fresh = suzatary_io::secrets::fresh_seed().map_err(|e| anyhow::anyhow!(e))?;
        set(name, &fresh).with_context(|| format!("storing {name}"))?;
        Ok(fresh)
    };
    // In this order, so a store that fails on the agent's key has kept or minted the box's.
    let (boxed, agent, dedup) = (seed(BOX_KEY)?, seed(AGENT_KEY)?, seed(DEDUP_KEY)?);
    suzatary_core::mail::dedup_key::DedupKey::of(&dedup)?;
    let root = prodrome_core::sign::Secret::new(boxed.expose())
        .map_err(|_| anyhow::anyhow!("{BOX_KEY} is not a key's seed"))?
        .public();
    Ok(BoxKeys::of(&root, &boxed, &agent)?)
}

/// The box's keys through secretspec, kept or minted.
///
/// # Errors
/// As [`kept_or_minted`].
pub(crate) fn kept_or_minted_in(
    secrets: &suzatary_io::secrets::Secretspec,
) -> anyhow::Result<BoxKeys> {
    let reason = "init: the box's own keys";
    kept_or_minted(
        |name| Ok(secrets.get(name, reason)?),
        |name, seed| Ok(secrets.set(name, seed, reason)?),
    )
}

/// Keep or mint each seed, check it against the declared root, and adopt now.
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
    let signers = suzatary_core::signatures::Signers::Held(keys.clone());
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
        "Every chain has adopted the root {}, each actor registered with the key that signs its \
         writes.",
        root.as_str()
    );
    Ok(())
}

/// THE REPAIR AFTER ADOPTION: a key of the box's registered for an actor whose writes the other
/// signs, revoked, so each actor of one chain is left speaking with the key that signs its writes
/// and no other of the box's. What a box adopted before custody needs; asked again, nothing. Each
/// act is said.
fn repair(chain: &crate::instance::ChainDir, keys: &BoxKeys) -> anyhow::Result<()> {
    let name = &chain.policy.name;
    let _held = chain_lock(chain)?;
    for (actor, key) in
        suzatary_core::signatures::revoke_misplaced(&store_of(chain), keys, &chain.policy)?
    {
        println!("{name}: revoked {} for {actor}", key.as_str());
    }
    Ok(())
}

/// One line per registered key per chain.
fn list(instance: &Instance) -> anyhow::Result<()> {
    let registrar = instance.chains.first().policy.registrar();
    for chain in &instance.chains {
        let dag: prodrome_core::dag::Dag<suzatary_core::record::TodoEvent> = store_of(chain)
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
fn chain_lock(chain: &crate::instance::ChainDir) -> anyhow::Result<suzatary_io::durable::Held> {
    crate::instance::adoption_lock(&chain.events)
}
