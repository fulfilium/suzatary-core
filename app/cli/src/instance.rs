//! `instance.toml`, parsed: the one place this binary learns about a box, so
//! `suzatary-core` and `suzatary-io` never see a path. It holds nothing secret.

mod chain_dir;
mod connections;
mod edits;
mod file;
mod local;
mod plugins;
mod reserve;
mod roots;

pub use chain_dir::{ChainDir, ChainDirs};
pub use edits::{InstanceEdits, editing};
pub(crate) use file::Signatures;
pub(crate) use file::{Chain, File, Web};
pub use file::{Mail, Secretary, Serving, Triage};
pub use local::Local;
pub use plugins::Plugins;
use plugins::plugins_of;
pub use reserve::{BadReserve, Reserve};
use roots::resolve_under;
pub use roots::{CodeRoot, DataRoot, Roots};

use crate::paths::{Env, Paths};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use prodrome_core::event::Actor;
use prodrome_core::sign::PublicKey;
use suzatary_core::chains::Chains;
use suzatary_core::connect::Channel;
use suzatary_core::deployment::Deployment;
use suzatary_core::nonempty::NonEmpty;
use suzatary_core::settings::Settables;
use suzatary_core::store::Kind;

/// The environment variable that names an instance file.
pub const ENV: &str = "SUZATARY_INSTANCE";

/// The variable a deployment's wrapper sets to the app's store path, over the file's `repo`.
pub const CODE_ENV: &str = "SUZATARY_REPO";

/// Each actor a chain registered on adopting, with the key that signs its writes.
pub type Registered = Vec<(Actor, PublicKey)>;

/// What [`Instance::adopt`] did: each chain that adopted, and why each of the others did not.
#[derive(Debug)]
pub struct Adopted<'a> {
    /// Each chain that adopted, with the actors it registered now.
    pub chains: Vec<(&'a ChainDir, Registered)>,
    /// Each chain that did not, and a revocation not completed, one sentence each.
    pub failed: Vec<String>,
}

impl<'a> Adopted<'a> {
    /// Every chain, or the sentences of those that did not adopt.
    ///
    /// # Errors
    /// Any chain that did not adopt, all of them named.
    pub fn whole(self) -> anyhow::Result<Vec<(&'a ChainDir, Registered)>> {
        if self.failed.is_empty() {
            Ok(self.chains)
        } else {
            bail!("not every chain has adopted:\n{}", self.failed.join("\n"))
        }
    }
}

/// Chain names the server's own routes would shadow.
///
/// `/api/act/{*verb}` is a catch-all, and `/api/briefings/{date}` and `/api/todo/{id}` would read
/// a chain's verb as a parameter; other routes cannot collide because matchit backtracks.
pub const RESERVED: &[&str] = &["act", "briefings", "todo"];

/// Everything the binary learned from the file.
#[derive(Debug, Clone)]
pub struct Instance {
    pub path: PathBuf,
    /// Every path derived outside the instance file.
    pub paths: Paths,
    pub roots: Roots,
    /// Every chain in the file's order; `deployment.chains` holds the same policies.
    pub chains: ChainDirs,
    pub deployment: Deployment,
    pub serving: Serving,
    pub secretary: Secretary,
    /// What the triage agent is handed.
    pub agent: Triage,
    /// Every repository the box may reach: `[[secretary.repos]]` and `[coding] repo`, one account.
    pub repos: suzatary_core::repos::Declared,
    /// The closed list of secret names a secret card mints its
    /// [`suzatary_core::settings::Settable`] against.
    pub settables: Settables,
    /// Those names, for a file under [`Paths::secretspec`]: reading the instance writes nothing;
    /// the first store call that needs the file writes it.
    pub declarations: suzatary_io::secrets::Declarations,
    /// The user repository's script the environment named.
    run: Option<String>,
    pub mail: Mail,
    pub persona: suzatary_core::persona::Persona,
    /// The box-local tier: never pushed, carried by the vault.
    pub local: Local,
    /// The channels `[notify]` names. `None` is no push, and no push secret is resolved.
    pub notify: Option<NonEmpty<Channel>>,
    /// What the box answers the Android portal at pairing.
    pub portal: suzatary_core::portal::Portal,
    pub plugins: Plugins,
}

impl Instance {
    #[must_use]
    pub fn code(&self) -> &Path {
        self.roots.code.as_path()
    }

    /// Typst for a model's answer blocks, its cache under `[secretary] renders`.
    #[must_use]
    pub fn answers(&self) -> suzatary_io::render::Typst {
        suzatary_io::render::Typst::new(
            self.secretary.renders.join("blocks"),
            suzatary_io::typeset::Closed::this_binary(),
        )
    }

    /// Where a Google account's calendars are kept, in the cache tier.
    #[must_use]
    pub fn google_calendar(&self, account: &suzatary_core::mail::account::MailAccount) -> PathBuf {
        self.plugins
            .cache
            .join("google")
            .join(&account.subdir)
            .join("store")
    }

    /// Every store the calendar reads: each declared plugin's, and each Google account's.
    #[must_use]
    pub fn calendar_stores(&self) -> Vec<PathBuf> {
        let google = self
            .mail
            .accounts
            .accounts()
            .iter()
            .filter(|account| matches!(account.sync, suzatary_core::mail::account::Sync::Google))
            .map(|account| self.google_calendar(account));
        self.plugins.stores().into_iter().chain(google).collect()
    }

    /// The conversations, an erasure emptying the answer blocks' cache on disk.
    #[must_use]
    pub fn conversation_stores(&self) -> suzatary_io::conversations::ConversationStores {
        self.secretary
            .conversation_stores(std::sync::Arc::new(self.answers()))
    }

    /// How the person reaches the app from a terminal: the script [`crate::paths::RUN_ENV`]
    /// named, else the binary on `PATH`.
    #[must_use]
    pub fn reach(&self) -> suzatary_core::connect::Reach {
        match &self.run {
            Some(script) => suzatary_core::connect::Reach::Script(script.clone()),
            None => suzatary_core::connect::Reach::Installed,
        }
    }

    /// The tables connections are read from.
    #[must_use]
    pub fn declared(&self) -> suzatary_core::connect::Declared {
        suzatary_core::connect::Declared {
            accounts: self.mail.accounts.accounts().to_vec(),
            plugins: self.plugins.manifests.clone(),
            drive: self.secretary.drive_allow.clone(),
            channels: NonEmpty::listed(self.notify.as_ref()).to_vec(),
        }
    }

    /// The secret store, reading the names this instance declares.
    #[must_use]
    pub fn secrets(&self) -> suzatary_io::secrets::Secretspec {
        self.declarations.store()
    }

    #[must_use]
    pub fn data(&self) -> &Path {
        self.roots.data.as_path()
    }

    /// The first chain: what a verb with no `--chain` and the bare `/api/…` paths mean.
    #[must_use]
    pub fn first(&self) -> &ChainDir {
        self.chains.first()
    }

    /// The chain a flag named, or the first one when it named none.
    ///
    /// # Errors
    /// A name this deployment does not serve, with the real ones in the sentence.
    pub fn chain(&self, named: Option<&str>) -> anyhow::Result<&ChainDir> {
        let Some(name) = named else {
            return Ok(self.first());
        };
        self.chains
            .iter()
            .find(|chain| chain.policy.name.as_str() == name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no chain named {name:?} in {} — it serves {}",
                    self.path.display(),
                    self.deployment.chains.names().join(", ")
                )
            })
    }

    /// A root resolved against the instance file's directory, not the working directory.
    fn beside(file: &Path, named: &Path) -> PathBuf {
        let here = match file.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let resolved = resolve_under(&here, named);
        std::fs::canonicalize(&resolved).unwrap_or(resolved)
    }

    pub(crate) fn repo_beside(file: &Path, repo: &Path) -> PathBuf {
        Self::beside(file, repo)
    }

    pub(crate) fn data_beside(file: &Path, data: &Path) -> PathBuf {
        Self::beside(file, data)
    }

    /// The performed requests' records, `requests/` in the data tree, pushed with the data.
    #[must_use]
    pub fn requests(&self) -> PathBuf {
        self.roots.data.under(Path::new("requests"))
    }

    /// The recorded decisions' store, `decisions/` in the data tree, pushed with the data.
    #[must_use]
    pub fn decisions(&self) -> PathBuf {
        self.roots.data.under(Path::new("decisions"))
    }

    /// Where each store of `kind` lives, by the name the fsck reports it under.
    #[must_use]
    pub fn directories(&self, kind: Kind) -> Vec<(String, PathBuf)> {
        let once = |path: PathBuf| vec![(kind.name().to_owned(), path)];
        let chains = |path: fn(&ChainDir) -> &PathBuf| {
            self.chains
                .iter()
                .map(|chain| {
                    let name = format!("{} {}", chain.policy.name, kind.name());
                    (name, path(chain).clone())
                })
                .collect()
        };
        let secretary = &self.secretary;
        match kind {
            Kind::Chain => chains(|chain| &chain.events),
            Kind::Inbox => once(secretary.inbox.clone()),
            Kind::Lines => once(secretary.line_store()),
            Kind::LineKeys => once(secretary.line_keys()),
            Kind::Memory => once(secretary.memory.clone()),
            Kind::MemoryTexts => once(secretary.memory_texts()),
            Kind::Digests => once(secretary.digests.clone()),
            Kind::DigestTexts => once(secretary.digest_texts()),
            Kind::Decisions => once(self.decisions()),
            Kind::Topics => once(secretary.topics.memberships.clone()),
            Kind::TopicAliases => once(secretary.topics.aliases.clone()),
            Kind::Plugin => self
                .plugins
                .manifests
                .iter()
                .map(|manifest| {
                    let name = format!("{} {}", kind.name(), manifest.name());
                    (name, self.plugins.store(manifest.name()))
                })
                .collect(),
            Kind::Triage => once(self.mail.state.clone()),
            Kind::Local => once(self.local.path().to_path_buf()),
            Kind::Uploads => once(secretary.uploads()),
            Kind::Kept => once(secretary.kept()),
            Kind::Requests => once(self.requests()),
        }
    }

    /// The root this deployment declares, `[signatures] root`.
    #[must_use]
    pub fn root(&self) -> &PublicKey {
        &self.chains.first().policy.root
    }

    /// What the box signs with: both keys from secretspec, checked against the root, or why not.
    /// Never an error, so a box whose secrets are locked still starts and reads; every write
    /// refuses with that sentence instead. It only reads; adopting is [`Instance::adopt`].
    #[must_use]
    pub fn signers(&self) -> suzatary_core::signatures::Signers {
        use suzatary_core::signatures::{AGENT_KEY, BOX_KEY, BoxKeys, Signers};
        let root = self.root();
        let secrets = self.secrets();
        let seed = |name: &str| {
            secrets
                .get(name, "the box signs what it writes")
                .map_err(|locked| self.locked(&locked))?
                .ok_or_else(|| self.unresolved(name))
        };
        let keys = seed(BOX_KEY).and_then(|own| {
            let agent = seed(AGENT_KEY)?;
            BoxKeys::of(root, &own, &agent).map_err(|e| e.to_string())
        });
        match keys {
            Ok(keys) => Signers::Held(keys),
            Err(why) => {
                tracing::warn!("every chain write refuses until the keys resolve: {why}");
                Signers::Missing(why)
            }
        }
    }

    /// Adoption, asked of every chain (idempotent): each registers the box's and the agent's keys
    /// and takes its epoch, and a revocation that reached only some chains is completed. Under a
    /// lock beside each chain, so two processes adopt once. Every chain is tried.
    #[must_use]
    pub fn adopt(&self, signers: &suzatary_core::signatures::Signers) -> Adopted<'_> {
        let (adopted, failed): (Vec<_>, Vec<_>) = self
            .chains
            .iter()
            .map(|chain| (chain, Self::adopted(chain, signers)))
            .partition(|(_, answer)| answer.is_ok());
        let mut failed: Vec<String> = failed
            .into_iter()
            .filter_map(|(chain, answer)| {
                answer
                    .err()
                    .map(|why| format!("{}: {why:#}", chain.policy.name))
            })
            .collect();
        if let Some(keys) = signers.held() {
            let stores: Vec<_> = self.chains.iter().map(crate::commands::store_of).collect();
            let stores: Vec<&dyn suzatary_core::store::Prodrome<suzatary_core::record::TodoEvent>> =
                stores.iter().map(|store| store as _).collect();
            if let Err(why) = suzatary_core::signatures::complete_revocations(&stores, keys.root())
            {
                failed.push(format!("completing a revocation across chains: {why}"));
            }
        }
        Adopted {
            chains: adopted
                .into_iter()
                .filter_map(|(chain, answer)| answer.ok().map(|registered| (chain, registered)))
                .collect(),
            failed,
        }
    }

    /// One chain adopted: answers each actor registered now, with its key.
    ///
    /// # Errors
    /// The lock not taken, a store failing, or writes since the epoch that no key signed.
    fn adopted(
        chain: &ChainDir,
        signers: &suzatary_core::signatures::Signers,
    ) -> anyhow::Result<Registered> {
        let Some(keys) = signers.held() else {
            return Ok(Vec::new());
        };
        let _held = adoption_lock(&chain.events)?;
        let adoption = suzatary_core::signatures::adopt(
            &crate::commands::store_of(chain),
            keys,
            &chain.policy,
        )?;
        Ok(adoption.registered)
    }

    fn locked(&self, locked: &suzatary_io::secrets::SecretError) -> String {
        format!(
            "{}: [signatures] declares a root and {locked} — {}; reads work meanwhile",
            self.path.display(),
            locked.remedy()
        )
    }

    fn unresolved(&self, name: &str) -> String {
        format!(
            "{}: [signatures] declares a root and the secret store holds no {name} — run \
             `suzatary device init` on the box; reads work meanwhile",
            self.path.display()
        )
    }

    /// The store one chain is written through as `actor`: every append signed by `actor`'s key,
    /// or refused when the key is not at hand.
    #[must_use]
    pub fn writer_of(
        chain: &ChainDir,
        signers: &suzatary_core::signatures::Signers,
        actor: &prodrome_core::event::Actor,
    ) -> suzatary_core::store::Signing<
        suzatary_io::objects::Objects<suzatary_core::record::TodoEvent>,
    > {
        suzatary_core::store::Signing::new(
            crate::commands::store_of(chain),
            signers.for_actor(actor, &chain.policy.untrusted),
        )
    }

    /// The inbox's store, its one opener.
    #[must_use]
    pub fn inbox_store(
        &self,
    ) -> suzatary_io::objects::Objects<suzatary_core::secretary::inbox_event::InboxEvent> {
        suzatary_io::objects::Objects::open(self.secretary.inbox.clone())
    }

    /// The memory's history, its one opener.
    #[must_use]
    pub fn memory_store(
        &self,
    ) -> suzatary_io::objects::Objects<suzatary_core::secretary::memory_event::MemoryEvent> {
        suzatary_io::objects::Objects::open(self.secretary.memory.clone())
    }

    /// The digest's history, its one opener.
    #[must_use]
    pub fn digest_store(
        &self,
    ) -> suzatary_io::objects::Objects<suzatary_core::secretary::digest_event::DigestEvent> {
        suzatary_io::objects::Objects::digests(
            self.secretary.digests.clone(),
            self.deployment.writers(),
        )
    }

    /// The instance file at `path`, read in this process's environment.
    ///
    /// # Errors
    /// The environment's, then [`Instance::read`]'s.
    pub fn open(path: &Path) -> anyhow::Result<Instance> {
        Self::read(path, &Env::read()?)
    }

    /// Read an instance file in `env` and check it, at startup, against itself.
    ///
    /// # Errors
    /// The file is missing or malformed, or names dead or contradictory config.
    pub fn read(path: &Path, env: &Env) -> anyhow::Result<Instance> {
        Self::of(path, File::read(path)?, env)
    }

    /// The file's sections checked against each other and rooted.
    pub(crate) fn of(path: &Path, file: File, env: &Env) -> anyhow::Result<Instance> {
        let code = CodeRoot(Self::code_root_in(
            Self::repo_beside(path, &file.repo),
            env.code(),
        ));
        let data = DataRoot(Self::data_beside(path, &file.data));
        let chains = ChainDirs::of(path, &data, file.chains, &file.signatures.root)?;
        // Derived from `chains`, so the policies and the stores cannot name different chains.
        let policies = Chains::of(chains.iter().map(|chain| chain.policy.clone()).collect())
            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        let coding = file.coding;
        let (notify, sender) = file.notify.map_or((None, None), |section| {
            (NonEmpty::of(section.channels), section.fcm_sender)
        });
        let portal = suzatary_core::portal::Portal::of(notify.as_ref(), sender);
        let plugins = plugins_of(path, &data, file.cache.as_deref(), file.plugins)?;
        let secretary = file.secretary.rooted(&code, &data);
        let repos = match &coding {
            Some(coding) => secretary
                .repos
                .clone()
                .with(coding.repo.clone())
                .map_err(|e| anyhow::anyhow!("{}: [coding] repo: {e}", path.display()))?,
            None => secretary.repos.clone(),
        };
        let local = Local(data.under(Path::new(Local::DIR)));
        let persona = suzatary_core::persona::Persona {
            prose: local.prose()?,
            ..file.persona
        };
        let mail = file.mail.rooted(&data);
        let settables = Settables::of_deployment(
            suzatary_io::model::TOKEN,
            NonEmpty::listed(notify.as_ref()).iter().copied(),
            coding.as_ref(),
            (&plugins.manifests, mail.accounts.accounts()),
            &repos,
        );
        let deployment = Deployment {
            viewers: file.web.viewers.into_iter().collect(),
            actors: file.web.act_as,
            chains: policies,
            categories: file.categories,
            app_url: file.web.app_url.trim_end_matches('/').to_owned(),
            coding,
            zone: file.persona.zone,
        };
        writers_checked(path, &deployment)?;
        let paths = Paths::of(&data, secretary.drive_cli.as_deref(), env);
        // Declared on a store's first call, never here: the code root may be read-only.
        let declarations = suzatary_io::secrets::Declarations::new(
            code.as_path().join("secretspec.toml"),
            paths.secretspec.clone(),
            settables.clone(),
        );
        Ok(Instance {
            settables,
            declarations,
            repos,
            run: env.run().map(str::to_owned),
            path: path.to_path_buf(),
            chains,
            paths,
            secretary,
            agent: file.agent,
            serving: file.web.serving.rooted(&code, &data),
            mail,
            deployment,
            persona,
            local,
            notify,
            portal,
            plugins,
            roots: Roots { code, data },
        })
    }

    /// An adopted code root, else the file's `repo` resolved beside it.
    #[must_use]
    pub fn code_root_in(beside: PathBuf, adopted: Option<&Path>) -> PathBuf {
        adopted.map_or(beside, Path::to_path_buf)
    }

    /// Where the instance file is, by [`Self::locate_in`] over this process's environment.
    #[must_use]
    pub fn locate(flag: Option<&Path>) -> PathBuf {
        let exported = std::env::var(ENV).ok();
        Self::locate_in(flag, exported.as_deref())
    }

    /// The flag, else a non-empty environment value, else `instance.toml` in the working directory.
    #[must_use]
    pub fn locate_in(flag: Option<&Path>, exported: Option<&str>) -> PathBuf {
        if let Some(named) = flag {
            return named.to_path_buf();
        }
        if let Some(named) = exported
            && !named.is_empty()
        {
            return PathBuf::from(named);
        }
        PathBuf::from("instance.toml")
    }
}

/// A writer the gate would refuse (not a viewer), or whose actor is missing
/// from a chain's roster (a write goes to one store), stops startup.
fn writers_checked(path: &Path, deployment: &Deployment) -> anyhow::Result<()> {
    let dead = deployment.dead_writers();
    if !dead.is_empty() {
        let names: Vec<&str> = dead.iter().map(|login| login.as_str()).collect();
        bail!(
            "{}: {names:?} may write but is not a viewer — a writer the gate refuses is dead config",
            path.display()
        );
    }
    let unrostered = deployment.unrostered_writers();
    if !unrostered.is_empty() {
        let named: Vec<String> = unrostered
            .iter()
            .map(|(chain, actor)| format!("{}/{chain}", actor.as_str()))
            .collect();
        bail!(
            "{}: {named:?} is named as a writer's actor and is not on that chain's roster",
            path.display()
        );
    }
    Ok(())
}

/// The cross-process file lock adoption is taken under: any writer that starts may be first.
pub(crate) fn adoption_lock(events: &Path) -> anyhow::Result<suzatary_io::durable::Held> {
    suzatary_io::durable::hold(&events.join("adopt.lock"))
        .with_context(|| format!("locking {}/adopt.lock", events.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use prodrome_core::event::Actor;
    use suzatary_core::capability::Login;
    use suzatary_core::settings::Edits as _;

    const GOOD: &str = r#"
repo = "/tmp/suzatary-test"
data = "/tmp/suzatary-test"
[signatures]
root = "836884e1723c7b17f8b87e76a0272f012de6cff08f134eb9dcdb2670695b1818"

[[chains]]
name = "life"
events = "events"
untrusted = ["triage"]
actors = ["ada", "triage"]
owner = "ada"
[[chains]]
name = "suzatary"
events = "events-suzatary"
untrusted = ["triage"]
actors = ["ada", "triage"]
owner = "ada"
[web]
viewers = ["her@example.test"]
app_url = "https://example.test/"
[web.act_as]
"her@example.test" = "ada"
[web.serving]
host = "127.0.0.1"
port = 8787
tls_dir = "/var/lib/suzatary/tls"
dist = "web/dist"
briefings = "briefings"
[categories]
home = 3
[mail]
root = "/home/ada/Mail"
state = "state.json"
notmuch = "notmuch"
mbsyncrc = "mbsyncrc"
heartbeat = "/tmp/suzatary-instance-test-triage-heartbeat"
[[mail.accounts]]
email = "her@example.test"
subdir = "her"
sync = "ms-imap"
authority = "organizations"
inbox_folders = ["Inbox"]
sent_folder = "Sent Items"
"#;

    fn env() -> Env {
        Env::under("/srv/ada-test/home", "/srv/ada-test/run")
    }

    fn write(text: &str, name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "suzatary-instance-{name}-{}-{:?}.toml",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, text).expect("a writable temp file");
        path
    }

    #[test]
    fn an_adopted_code_root_wins_over_the_files_repo() {
        let beside = PathBuf::from("/srv/checkout");
        assert_eq!(
            Instance::code_root_in(beside.clone(), Some(Path::new("/nix/store/app"))),
            PathBuf::from("/nix/store/app")
        );
        assert_eq!(Instance::code_root_in(beside.clone(), None), beside);
        let path = write(GOOD, "adopted-code");
        let exported = env().running(Some(PathBuf::from("/nix/store/app")));
        assert_eq!(
            Instance::read(&path, &exported).expect("reads").code(),
            Path::new("/nix/store/app"),
            "the environment the reader is handed decides, never the process's"
        );
        assert_eq!(
            Instance::read(&path, &env()).expect("reads").code(),
            Path::new("/tmp/suzatary-test")
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_good_file_becomes_a_deployment_and_its_paths_are_resolved() {
        let path = write(GOOD, "good");
        let instance = Instance::read(&path, &env()).expect("a deployment");
        assert_eq!(instance.deployment.chains.names(), vec!["life", "suzatary"]);
        assert_eq!(instance.first().policy.name.as_str(), "life");
        assert_eq!(
            instance.first().events,
            PathBuf::from("/tmp/suzatary-test/events")
        );
        assert_eq!(
            instance
                .chain(Some("suzatary"))
                .expect("a second chain")
                .events,
            PathBuf::from("/tmp/suzatary-test/events-suzatary")
        );
        assert_eq!(
            instance.chain(None).expect("the first chain").events,
            instance.first().events
        );
        let missing = instance.chain(Some("nowhere")).expect_err("no such chain");
        assert!(
            format!("{missing:#}").contains("life, suzatary"),
            "{missing}"
        );
        assert_eq!(
            instance.serving.dist,
            PathBuf::from("/tmp/suzatary-test/web/dist")
        );
        assert_eq!(
            instance.serving.tls_dir,
            PathBuf::from("/var/lib/suzatary/tls")
        );
        assert_eq!(instance.mail.root, PathBuf::from("/home/ada/Mail"));
        assert_eq!(instance.deployment.app_url, "https://example.test");
        assert_eq!(
            instance
                .deployment
                .actor_of(&Login::from_whois("her@example.test"))
                .map(Actor::as_str),
            Some("ada")
        );
        assert_eq!(instance.mail.accounts.accounts().len(), 1);
        assert_eq!(instance.notify, None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_committed_sample_parses_with_notify_absent() {
        let sample = repo_root().join("instance.sample.toml");
        let instance = Instance::read(&sample, &env()).expect("the committed sample parses");
        assert_eq!(instance.notify, None);
    }

    /// The home is a file, so even root cannot write under it.
    #[test]
    fn reading_an_instance_writes_nothing_even_where_nothing_can_be_written() {
        let scratch = tempfile::tempdir().expect("a scratch directory");
        let home = scratch.path().join("home");
        std::fs::write(&home, "").expect("a file where the home would be");
        let instance = Instance::read(
            &repo_root().join("instance.sample.toml"),
            &Env::under(&home, scratch.path().join("run")),
        )
        .expect("the sample reads");
        assert_eq!(
            std::fs::read_dir(scratch.path())
                .expect("the scratch")
                .count(),
            1,
            "nothing but the home's file"
        );
        let asked = instance
            .secrets()
            .get("SUZATARY_TEST_SECRET_NOBODY_SETS", "test");
        assert!(
            matches!(
                asked,
                Err(suzatary_io::secrets::SecretError::Undeclared { .. })
            ),
            "{asked:?}"
        );
    }

    /// Skipped on a checkout with no `instance.toml` of its own.
    #[test]
    fn this_boxs_own_file_parses_with_notify_present() {
        let path = repo_root().join("instance.toml");
        let Ok(instance) = Instance::read(&path, &env()) else {
            return;
        };
        assert_eq!(
            NonEmpty::listed(instance.notify.as_ref()),
            [Channel::Fcm, Channel::Ntfy]
        );
    }

    /// `CARGO_MANIFEST_DIR` is `app/cli`, two levels below the repository root.
    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("app/cli/../.. is the repository root")
            .to_path_buf()
    }

    #[test]
    fn the_code_and_data_roots_resolve_from_the_file() {
        let base = std::env::temp_dir().join("suzatary-instance-roots");
        let elsewhere = std::env::temp_dir().join("suzatary-instance-roots-elsewhere");
        for dir in [&base, &elsewhere] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let here = std::fs::canonicalize(&base).unwrap();
        let there = std::fs::canonicalize(&elsewhere).unwrap();
        let text = |repo: &str, data: Option<&str>| {
            let data = data.map_or(String::new(), |data| format!("data = {data:?}\n"));
            GOOD.replace("data = \"/tmp/suzatary-test\"\n", &data)
                .replace("/tmp/suzatary-test", repo)
        };
        let file = base.join("instance.toml");
        for (repo, data, code, chains) in [
            (".", Some("."), here.clone(), here.clone()),
            ("/a", Some("/b"), PathBuf::from("/a"), PathBuf::from("/b")),
            (".", None, here.clone(), here.clone()),
            (
                ".",
                Some("../suzatary-instance-roots-elsewhere"),
                here,
                there,
            ),
        ] {
            std::fs::write(&file, text(repo, data)).unwrap();
            let instance = Instance::read(&file, &env()).unwrap();
            assert_eq!(instance.code(), code, "{repo} {data:?}");
            assert_eq!(instance.first().events, chains.join("events"));
            assert_eq!(
                instance
                    .chain(Some("suzatary"))
                    .expect("the second chain")
                    .events,
                chains.join("events-suzatary")
            );
            assert_eq!(instance.serving.dist, code.join("web/dist"));
        }
    }

    #[test]
    fn a_file_the_server_would_misread_stops_it_starting_and_says_why() {
        for (tag, text, said) in [
            (
                "ghost",
                GOOD.replace(
                    r#""her@example.test" = "ada""#,
                    "\"her@example.test\" = \"ada\"\n\"ghost@example.test\" = \"assistant\"",
                ),
                &["dead config"][..],
            ),
            (
                "unrostered",
                GOOD.replace(r#"= "ada""#, r#"= "nobody""#),
                &["roster"],
            ),
            (
                "slug",
                GOOD.replacen(r#"name = "suzatary""#, r#"name = "../etc""#, 1),
                &["chain name"],
            ),
            (
                "a-provider",
                format!(
                    "{GOOD}\n[[providers]]\nname = \"phala\"\nbase_url = \"https://example.test/v1\"\n\
                     key = \"X\"\nattested = false\n"
                ),
                &["unknown field `providers`"],
            ),
            (
                "an-order",
                format!("{GOOD}\n[models]\nreview = [\"claude-cli/claude-opus-5-5\"]\n"),
                &["unknown field `models`"],
            ),
            (
                "bad-reserve",
                format!("{GOOD}\n[secretary]\nreserve = 1.5\n"),
                &["reserve"],
            ),
        ] {
            let path = write(&text, tag);
            let refused = format!("{:#}", Instance::read(&path, &env()).expect_err(tag));
            for word in said {
                assert!(refused.contains(word), "{tag}: {refused}");
            }
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn a_trust_policy_naming_an_actor_the_chains_roster_does_not_is_refused() {
        let text = GOOD.replacen(
            r#"untrusted = ["triage"]"#,
            r#"untrusted = ["stranger"]"#,
            1,
        );
        let path = write(&text, "trust");
        let refused =
            Instance::read(&path, &env()).expect_err("an unknown untrusted actor is refused");
        let said = format!("{refused:#}");
        assert!(
            said.contains("untrusted") && said.contains("\"life\""),
            "{said}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_repeated_chain_name_and_an_empty_list_are_both_refused() {
        let text = GOOD.replacen(r#"name = "suzatary""#, r#"name = "life""#, 1);
        let path = write(&text, "twice");
        let refused = Instance::read(&path, &env()).expect_err("a repeated name is refused");
        assert!(format!("{refused:#}").contains("two chains"), "{refused}");
        let _ = std::fs::remove_file(&path);

        let bare = GOOD.replace("[[chains]]", "[[nothing]]");
        let path = write(&bare, "none");
        assert!(
            Instance::read(&path, &env()).is_err(),
            "a file with no chains"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_reserved_chain_name_stops_the_server_starting() {
        for reserved in RESERVED {
            let text = GOOD.replacen(r#"name = "suzatary""#, &format!("name = {reserved:?}"), 1);
            let path = write(&text, "reserved");
            let refused = Instance::read(&path, &env()).expect_err("a reserved name is refused");
            assert!(
                format!("{refused:#}").contains("reserved chain name"),
                "{reserved}: {refused}"
            );
            let _ = std::fs::remove_file(&path);
        }
    }

    /// A file with an operator's comment, which a serde round trip would drop.
    fn commented_instance() -> String {
        format!(
            "{GOOD}\n\
             # a comment an operator wrote by hand, years ago\n\
             [persona]\n\
             city = \"Springfield\"\n"
        )
    }

    #[test]
    fn secretary_repos_read_with_their_check() {
        let read: Secretary = toml::from_str(
            "[[repos]]\nname = \"bmabsout/Suzatary\"\n\n\
             [[repos]]\nname = \"bmabsout/resume_typst\"\ncheck = \"typst compile cv.typ\"\n",
        )
        .expect("parses");
        let target = |name: &str| {
            read.repos
                .target(&name.parse().expect("a name"))
                .cloned()
                .expect("declared")
        };
        assert_eq!(target("bmabsout/Suzatary").check, None);
        assert_eq!(
            target("bmabsout/resume_typst").check.as_deref(),
            Some("typst compile cv.typ")
        );
        assert!(toml::from_str::<Secretary>("[[repos]]\nname = \"nope\"\n").is_err());
        assert!(
            toml::from_str::<Secretary>(
                "[[repos]]\nname = \"bmabsout/cv\"\n\n[[repos]]\nname = \"someorg/cv\"\n"
            )
            .is_err(),
            "another account's repository is refused when the file is read"
        );
    }

    #[test]
    fn allow_web_adds_a_rule_once_and_the_file_still_reads() {
        let before = commented_instance();
        let path = write(&before, "edits-allow-web");
        let rule: suzatary_core::web::AllowRule = "*.gov.uk".parse().expect("a rule");
        let edits = InstanceEdits::new(path.clone());
        edits.allow_web(&rule).expect("allowed");
        edits
            .allow_web(&rule)
            .expect("allowing twice is the same answer");

        let after = std::fs::read_to_string(&path).expect("the file still exists");
        for line in before
            .lines()
            .filter(|line| line.trim_start().starts_with('#'))
        {
            assert!(after.contains(line), "a comment was lost: {line}");
        }
        assert_eq!(after.matches("*.gov.uk").count(), 1, "{after}");
        let instance = Instance::read(&path, &env()).expect("the edited file still parses");
        assert_eq!(instance.secretary.web_allow, vec![rule]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn allow_drive_adds_a_folder_once_and_the_file_still_reads() {
        let before = commented_instance();
        let path = write(&before, "edits-allow-drive");
        let folder: suzatary_core::drive::DrivePath =
            "/shared-with-me/Ada Documents".parse().expect("a folder");
        let edits = InstanceEdits::new(path.clone());
        edits.allow_drive(&folder).expect("allowed");
        edits
            .allow_drive(&folder)
            .expect("allowing twice is the same answer");
        edits
            .allow_web(&"*.gov.uk".parse().expect("a rule"))
            .expect("a web rule beside it");

        let after = std::fs::read_to_string(&path).expect("the file still exists");
        for line in before
            .lines()
            .filter(|line| line.trim_start().starts_with('#'))
        {
            assert!(after.contains(line), "a comment was lost: {line}");
        }
        assert_eq!(
            after.matches("/shared-with-me/Ada Documents").count(),
            1,
            "{after}"
        );
        let instance = Instance::read(&path, &env()).expect("the edited file still parses");
        assert_eq!(instance.secretary.drive_allow, vec![folder]);
        assert_eq!(instance.secretary.web_allow.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn each_instance_keeps_its_own_epochs() {
        let env = env();
        let epochs = |data: &str| Paths::of(&DataRoot(PathBuf::from(data)), None, &env).epochs;
        assert_eq!(epochs("/data"), epochs("/data"));
        assert_ne!(epochs("/data"), epochs("/scratch/data"));
        assert_eq!(
            epochs("/data").parent(),
            epochs("/scratch/data").parent(),
            "under the one state directory"
        );
    }

    #[test]
    fn drive_allow_is_checked_and_drive_cli_is_resolved() {
        assert!(toml::from_str::<Secretary>("drive_allow = [\"my-files/../trash\"]\n").is_err());
        let read: Secretary =
            toml::from_str("drive_allow = [\"/my-files/Docs/\"]\ndrive_cli = \"bin/pd\"\n")
                .expect("parses");
        let secretary = read.rooted(
            &CodeRoot(PathBuf::from("/code")),
            &DataRoot(PathBuf::from("/data")),
        );
        assert_eq!(
            secretary.drive_allow,
            vec![
                "/my-files/Docs"
                    .parse::<suzatary_core::drive::DrivePath>()
                    .expect("a folder")
            ]
        );
        assert_eq!(secretary.drive_cli, Some(PathBuf::from("/data/bin/pd")));
    }
}
