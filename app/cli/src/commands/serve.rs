//! `suzatary serve`: the server's effects chosen from the instance file and its flags, handed to
//! `serve` as a built [`State`].

mod checks;
mod world;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;

use suzatary_base::refusal::shown;
use suzatary_digest::secretary::compact::Compact;
use suzatary_digest::secretary::compacting::Compacting;
use suzatary_google::google::Endpoints;
use suzatary_io_chains::objects::Objects;
use suzatary_io_local::model_health::Answered;
use suzatary_io_local::requests::Requests;
use suzatary_server::gate::{self, Door};

use crate::commands::secretary::{Acting, ActingStores, Postman};
use crate::instance::{Instance, InstanceEdits, Mounts};
use crate::interpreters::{self, Brain, CompactorModel, Peers};
use crate::mail_cache::MailCache;
use suzatary_connect::settings::Verify;
use suzatary_io_mail::google::Google;
use suzatary_server::serve::compaction::Compactor;
use suzatary_server::serve::live::{self, Live};
use suzatary_server::serve::models::Models;
use suzatary_server::serve::sign_ins;
use suzatary_server::serve::{
    self, Build, Chained, Performers, Project, Projects, Settings, Signed, Stamp, State, Workers,
    google, listening,
};
use world::World;

pub struct Serving {
    pub listening: listening::Listening,
    /// `app_url`'s host at the `listening` port.
    pub door: Door,
    pub dist: PathBuf,
    pub live: live::Shared,
    /// Names a peer, loopback included.
    pub peers: Peers,
    pub brain: Brain,
    pub titler: Brain,
    /// Off under every fixture.
    pub compacting: Compacting,
    pub postman: Postman,
    pub acting: Performing,
    /// Shared with the performer for a tap's announcements.
    pub workers: Arc<Workers>,
    pub stamp: Stamp,
    pub signing: Arc<sign_ins::SignIns>,
    pub verify: Arc<dyn Verify + Send + Sync>,
    /// Rings `moved` when a reading moves.
    pub answered: Answered,
    /// Rung when a model starts or stops answering.
    pub moved: Arc<tokio::sync::Notify>,
    /// Google's endpoints, or a fake.
    pub google: Endpoints,
}

/// Performs a tapped act through the pages' handles.
pub type Performing = Box<dyn FnOnce(ActingStores, Arc<Workers>) -> Acting + Send>;

pub async fn listen(instance: &Instance, serving: Serving) -> anyhow::Result<()> {
    use suzatary_io_local::{epochs, frontiers};
    let listener = serve::bound(&serving.listening)?;
    // Declared first, so a server that cannot write them does not start.
    instance
        .declarations
        .opened()
        .context("declaring this instance's secret names")?;
    // Missing keys leave the server read-only.
    let signers = instance.signers();
    if let Err(failed) = instance.adopt(&signers).whole() {
        tracing::warn!("{failed:#}");
    }
    let world: &'static World = Box::leak(Box::new(World::assemble(
        instance,
        signers,
        serving.peers,
        serving.brain,
    )));
    if let Err(failed) = world.begin_written() {
        tracing::warn!("serve: the stores a tab writes are not begun: {failed}");
    }
    let mut bound = world.chains.iter().map(|stores| Chained {
        caps: Box::leak(Box::new(world.server(stores))),
        policy: Arc::new(stores.policy.clone()),
    });
    let first = bound.next().context("an instance names a chain")?;
    let rest: Vec<Chained> = bound.collect();
    let stores = world.acting_stores();
    let workers = serving.workers;
    let state = State::over(
        first.clone(),
        rest.clone(),
        Arc::new(instance.deployment.clone()),
        serving.door,
        Signed {
            requests: Arc::new(Requests::open(
                instance.requests(),
                instance.paths.answers.clone(),
            )),
            asks: Arc::new(Objects::open(instance.asks())),
            pairings: Arc::new(suzatary_io_local::pairing::Pairings::at(
                instance.paths.pairing.clone(),
            )),
        },
        Performers {
            postman: Arc::new(serving.postman),
            acting: Arc::new((serving.acting)(stores, Arc::clone(&workers))),
        },
        workers,
        Settings {
            // Never `secretspec.toml`, which names unsettable secrets.
            live: Arc::clone(&serving.live),
            models: Arc::new(models_of(instance)),
            secrets: Arc::new(instance.secrets()),
            declares: Arc::new(instance.declarations.clone()),
            edits: Arc::new(InstanceEdits::new(instance.path.clone())),
            forget: Arc::new(MailCache::of(instance)),
            stamp: serving.stamp,
            data: Arc::new(instance.data().to_path_buf()),
        },
    )
    .with_uploads(
        interpreters::upload_store(instance),
        Arc::new(interpreters::drive_vault(instance)),
    )
    .with_titler(Arc::new(serving.titler))
    .with_keys(world.signers.held().cloned())
    .with_plugins(instance.plugins.clone())
    .with_reach(instance.reach())
    .with_projects(projects(instance.mounts()))
    .with_repos(instance.repos.clone())
    .with_portal(instance.portal.clone())
    .with_reveal(Arc::new(crate::commands::mail::notmuch(instance)))
    .with_checks(Arc::new(checks::Synced::of(instance, &serving.google)))
    .with_answered(Arc::new(serving.answered))
    .with_books(
        Arc::new(epochs::open(instance.paths.epochs.clone())),
        Arc::new(frontiers::open(instance.paths.frontiers.clone())),
    )
    .with_sign_ins(serving.signing, serving.verify)
    .with_google(google::Consenting::new(
        Google::new(serving.google),
        Arc::new(instance.secrets()),
        Arc::new(InstanceEdits::new(instance.path.clone())),
        &instance.deployment.app_url,
    ));
    serve::started_up(&first, &rest).await;
    // Its first round folds the conversations, which a start step would wait for.
    let state = compacting(state, instance, serving.compacting);
    files_watched(&state, instance, &serving.moved);
    serve::after_started(&state, serving.moved);
    watch_from_now_on(instance, stores, &state, &first, rest);
    serve::serve_on(
        serving.listening,
        listener,
        serve::app(state, &serving.dist),
    )
    .await
}

/// `state` with the compactor's task started over its first chain's stores when `compacting`
/// runs one; off is the kill switch, with no task, and the lines wait.
fn compacting(state: State, instance: &Instance, compacting: Compacting) -> State {
    match compacting {
        Compacting::Off => state,
        Compacting::On {
            ladder,
            budget,
            workers,
        } => {
            // One model per rung for the life of the server, as `caps` is.
            let ladder = ladder.map(|model| -> &'static dyn Compact {
                Box::leak(Box::new(CompactorModel::new(instance, model.clone())))
            });
            let caps = state.first.caps;
            state.with_compactor(Compactor::spawn(caps, ladder, budget, workers))
        }
    }
}

/// The flags `suzatary serve` takes, each an override of the instance file.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// The address to bind, overriding the instance file's.
    #[arg(long)]
    host: Option<String>,
    /// The port.
    #[arg(long)]
    port: Option<u16>,
    /// A TLS certificate (PEM). With `--key`, serve https.
    #[arg(long, requires = "key")]
    cert: Option<PathBuf>,
    /// A TLS key (PEM).
    #[arg(long, requires = "cert")]
    key: Option<PathBuf>,
    /// The built frontend directory, served in place of the instance file's `dist`.
    #[arg(long)]
    dist: Option<PathBuf>,
    /// Test only, compiled out of the shipped binary: answer every whois with this login.
    #[cfg(feature = "fixture")]
    #[arg(long, value_name = "LOGIN")]
    whois_fixture: Option<String>,
    /// Test only: answer every model turn from this script.
    #[cfg(feature = "fixture")]
    #[arg(long, value_name = "FILE")]
    model_fixture: Option<PathBuf>,
    /// Test only: record what a tap would have sent into this file, and send nothing.
    #[cfg(feature = "fixture")]
    #[arg(long, value_name = "FILE")]
    smtp_fixture: Option<PathBuf>,
    /// Test only: answer `GET /api/about`'s build with this file, read per request.
    #[cfg(feature = "fixture")]
    #[arg(long, value_name = "FILE")]
    build_fixture: Option<PathBuf>,
    /// Test only: ask the fake Google at this base URL instead of Google's own.
    #[cfg(feature = "fixture")]
    #[arg(long, value_name = "URL")]
    google_fixture: Option<String>,
}

fn tls(
    cert: Option<PathBuf>,
    key: Option<PathBuf>,
    tls_dir: &std::path::Path,
) -> Option<(PathBuf, PathBuf)> {
    if let (Some(cert), Some(key)) = (cert, key) {
        return Some((cert, key));
    }
    let (cert, key) = (tls_dir.join("cert.pem"), tls_dir.join("key.pem"));
    (cert.is_file() && key.is_file()).then_some((cert, key))
}

/// Refused for plain http anywhere but loopback, and for a fixture flag anywhere but loopback.
fn serving(instance: &Instance, args: Args) -> anyhow::Result<Serving> {
    #[cfg(feature = "fixture")]
    let (whois_fixture, model_fixture, smtp_fixture, build_fixture, google_fixture) = (
        args.whois_fixture,
        args.model_fixture,
        args.smtp_fixture,
        args.build_fixture,
        args.google_fixture,
    );
    let tls = tls(args.cert, args.key, &instance.serving.tls_dir);
    let (listening, door) = listening_of(instance, args.host, args.port, tls)?;
    // A fixture runs only where no other machine reaches it: loopback, over http or https.
    #[cfg(feature = "fixture")]
    let reachable = listening.reachable();
    #[cfg(feature = "fixture")]
    let stamp = stamp_of(build_fixture, reachable)?;
    #[cfg(not(feature = "fixture"))]
    let stamp = crate::stamp::stamped(Build::Compiled(crate::stamp::build()));
    #[cfg(feature = "fixture")]
    let google = match google_fixture {
        None => Endpoints::google(),
        Some(_) if reachable => {
            anyhow::bail!(
                "--google-fixture is test-only and refuses to run where another machine reaches it"
            )
        }
        Some(base) => Endpoints::at(&base),
    };
    #[cfg(not(feature = "fixture"))]
    let google = Endpoints::google();
    let live = live_of(instance).shared();
    let moved = Arc::new(tokio::sync::Notify::new());
    let answered = suzatary_io_local::model_health::beside(&instance.mail.heartbeat).ringing({
        let moved = Arc::clone(&moved);
        move || moved.notify_one()
    });

    #[cfg(feature = "fixture")]
    let any_fixture = whois_fixture.is_some();
    #[cfg(feature = "fixture")]
    let peers = match whois_fixture {
        None => daemon_peers(&instance.paths.whois_socket),
        Some(login) => {
            if reachable {
                anyhow::bail!(
                    "--whois-fixture is test-only and refuses to run where another machine reaches it"
                );
            }
            tracing::warn!("serve: --whois-fixture {login} — every peer is this login");
            Peers::Fixture(suzatary_base::capability::Login::from_whois(login))
        }
    };
    #[cfg(not(feature = "fixture"))]
    let peers = daemon_peers(&instance.paths.whois_socket);

    // Records taps too, never reaching Microsoft.
    #[cfg(feature = "fixture")]
    let scripted = model_fixture.is_some();
    #[cfg(feature = "fixture")]
    let titler = match &model_fixture {
        Some(path) => Brain::Scripted(crate::commands::fixture::Scripted::reading(path)?),
        None => real_brain(instance, &answered),
    };
    #[cfg(not(feature = "fixture"))]
    let titler = real_brain(instance, &answered);
    #[cfg(feature = "fixture")]
    let (brain, postman) = fixture_talking(
        instance,
        &answered,
        (model_fixture, smtp_fixture),
        reachable,
    )?;
    #[cfg(not(feature = "fixture"))]
    let (brain, postman) = (real_brain(instance, &answered), real_postman(instance));
    let (acting, signing, verify) = performers(
        instance,
        &answered,
        &moved,
        #[cfg(feature = "fixture")]
        (scripted, &google),
    );
    #[cfg(feature = "fixture")]
    let fixture = any_fixture || scripted;
    #[cfg(not(feature = "fixture"))]
    let fixture = false;
    let compacting = quiet(instance, fixture);

    Ok(Serving {
        listening,
        door,
        dist: args.dist.unwrap_or_else(|| instance.serving.dist.clone()),
        live,
        peers,
        brain,
        titler,
        compacting,
        postman,
        acting,
        workers: Arc::new(Workers::new()),
        stamp,
        signing,
        verify,
        answered,
        moved,
        google,
    })
}

fn listening_of(
    instance: &Instance,
    host: Option<String>,
    port: Option<u16>,
    tls: Option<(PathBuf, PathBuf)>,
) -> anyhow::Result<(listening::Listening, Door)> {
    let host = host.unwrap_or_else(|| instance.serving.host.clone());
    let at = SocketAddr::new(
        host.parse()
            .with_context(|| format!("{} is not an address", shown(&host)))?,
        port.unwrap_or(instance.serving.port),
    );
    let door = gate::door_at(&instance.deployment.app_url, at)?;
    let listening = listening::Listening::new(at, tls).with_context(|| {
        format!(
            "no readable {}/{{cert,key}}.pem",
            instance.serving.tls_dir.display()
        )
    })?;
    Ok((listening, door))
}

fn quiet(instance: &Instance, fixture: bool) -> Compacting {
    if fixture {
        Compacting::Off
    } else {
        instance.secretary.compacting()
    }
}

/// Fixtures only; never where another machine reaches them.
#[cfg(feature = "fixture")]
fn fixture_talking(
    instance: &Instance,
    answered: &Answered,
    (model_fixture, smtp_fixture): (Option<PathBuf>, Option<PathBuf>),
    reachable: bool,
) -> anyhow::Result<(Brain, Postman)> {
    Ok(match (model_fixture, smtp_fixture) {
        (None, None) => (real_brain(instance, answered), real_postman(instance)),
        (script, recording) => {
            if reachable {
                anyhow::bail!(
                    "the model and smtp fixtures refuse to run where another machine reaches them"
                );
            }
            (
                match script {
                    None => real_brain(instance, answered),
                    Some(path) => {
                        tracing::warn!(
                            "serve: --model-fixture {} — scripted replies",
                            path.display()
                        );
                        Brain::Scripted(crate::commands::fixture::Scripted::reading(&path)?)
                    }
                },
                match recording {
                    None => real_postman(instance),
                    Some(path) => {
                        tracing::warn!(
                            "serve: --smtp-fixture {} — nothing is sent",
                            path.display()
                        );
                        Postman::Recording(path)
                    }
                },
            )
        }
    })
}

fn performers(
    instance: &Instance,
    answered: &Answered,
    moved: &Arc<tokio::sync::Notify>,
    #[cfg(feature = "fixture")] (scripted, google): (bool, &Endpoints),
) -> (
    Performing,
    Arc<sign_ins::SignIns>,
    Arc<dyn Verify + Send + Sync>,
) {
    let signing = Arc::new(sign_ins::SignIns::new("claude").ringing({
        let moved = Arc::clone(moved);
        move || moved.notify_one()
    }));
    let answered = || answered.clone();
    #[cfg(feature = "fixture")]
    if scripted {
        return (
            recording_acting(instance, &signing, google),
            Arc::clone(&signing),
            Arc::new(interpreters::Verifying::Fixture {
                instance: Arc::new(instance.clone()),
                answered: answered(),
            }),
        );
    }
    (
        real_acting(instance, &signing, moved),
        Arc::clone(&signing),
        Arc::new(interpreters::Verifying::Real {
            instance: Arc::new(instance.clone()),
            answered: answered(),
        }),
    )
}

/// `--build-fixture`; refused where another machine reaches it.
#[cfg(feature = "fixture")]
fn stamp_of(build_fixture: Option<PathBuf>, reachable: bool) -> anyhow::Result<Stamp> {
    let compiled = crate::stamp::build();
    let Some(path) = build_fixture else {
        return Ok(crate::stamp::stamped(Build::Compiled(compiled)));
    };
    if reachable {
        anyhow::bail!(
            "--build-fixture is test-only and refuses to run where another machine reaches it"
        );
    }
    tracing::warn!(
        "serve: --build-fixture {} — the build is that file's",
        path.display()
    );
    Ok(crate::stamp::stamped(Build::File {
        path: Arc::new(path),
        compiled,
    }))
}

/// The local daemon names every peer, loopback included: an address is never an identity.
fn daemon_peers(socket: &Path) -> Peers {
    Peers::Daemon(suzatary_io_local::whois::Tailscaled::new(socket))
}

fn real_brain(instance: &Instance, answered: &Answered) -> Brain {
    interpreters::brain(instance, answered.clone())
}

fn real_postman(instance: &Instance) -> Postman {
    Postman::Real(Arc::new(instance.clone()))
}

/// Rings `moved` when a device flow ends.
fn real_acting(
    instance: &Instance,
    signing: &Arc<sign_ins::SignIns>,
    moved: &Arc<tokio::sync::Notify>,
) -> Performing {
    let (instance, signing, moved) = (instance.clone(), Arc::clone(signing), Arc::clone(moved));
    Box::new(move |stores, workers| {
        Acting::real(
            &instance,
            stores,
            workers,
            signing,
            Arc::new(move || moved.notify_one()),
        )
    })
}

/// Never dials Microsoft.
#[cfg(feature = "fixture")]
fn recording_acting(
    instance: &Instance,
    signing: &Arc<sign_ins::SignIns>,
    google: &Endpoints,
) -> Performing {
    let (instance, signing, google) = (instance.clone(), Arc::clone(signing), google.clone());
    Box::new(move |stores, workers| Acting::recording(&instance, stores, workers, signing, &google))
}

/// The plan store of `mounts`, read at each call.
#[must_use]
pub fn projects(mounts: Mounts) -> Projects {
    Arc::new(move || {
        Ok(mounts.project()?.map(|project| Project {
            chain: project.chain,
            plan: project.plan,
        }))
    })
}

/// The instance file and the sources' health file, each read again when it moves; a move of the
/// health file that changes what is owed rings `moved`.
fn files_watched(state: &State, instance: &Instance, moved: &Arc<tokio::sync::Notify>) {
    tokio::spawn(serve::instance_watched(
        state.clone(),
        instance.path.clone(),
    ));
    let moved = Arc::clone(moved);
    tokio::spawn(serve::health_watched(
        state.clone(),
        suzatary_io_local::source_health::file_beside(&instance.mail.heartbeat),
        move || moved.notify_one(),
    ));
}

/// Only the first chain pushes.
fn watch_from_now_on(
    instance: &Instance,
    stores: ActingStores,
    state: &State,
    first: &Chained,
    rest: Vec<Chained>,
) {
    crate::commands::coding::spawn_polling(instance, stores.conversations, &state.workers);
    let upkeep = crate::commands::doctor::upkeep_line(instance.deployment.upkeep.as_ref());
    if instance.deployment.upkeep.is_some() {
        tracing::info!("serve: upkeep {upkeep}");
    } else {
        tracing::warn!("serve: upkeep {upkeep}");
    }
    serve::watching(
        state,
        first,
        rest,
        interpreters::mirrors(instance),
        Arc::new(suzatary_io_access::github_tokens::GithubTokens::new(
            instance.paths.github_tokens.clone(),
            Box::new(instance.secrets()),
        )),
    );
}

pub fn run(path: &Path, args: Args) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    // Installed already where `RUST_LOG` asked `main` for it, with the same filter.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .try_init();
    let serving = serving(&instance, args)?;
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(listen(&instance, serving))
}

/// The settings a running server reads, as the instance file declares them now.
fn live_of(instance: &Instance) -> Live {
    Live {
        settables: instance.settables.clone(),
        channels: instance.notify.clone(),
        accounts: instance.mail.accounts.clone(),
        plugins: instance.plugins.manifests.clone(),
    }
}

/// The models `[secretary]` names.
fn models_of(instance: &Instance) -> Models {
    Models {
        chat: instance.secretary.model.clone(),
        compacting: instance.secretary.compacting(),
        reserve: instance.secretary.reserve,
    }
}
