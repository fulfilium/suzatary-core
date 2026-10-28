//! The connection algebra's laws, over generated instances and connections: a connection is read
//! back out of the tables it writes, removing it leaves what was there, a duplicate is refused,
//! the reading is a function of the tables as sets, every secret a step asks for is a name the
//! deployment may set, and meeting a step never moves a standing back.

use prodrome_core::literal::codec::Literal as _;
use proptest::prelude::*;
use suzatary_core::capability::SecretState;
use suzatary_core::connect::{
    CATALOG, Checked, Connection, Declared, PLAIN, Params, Refused, Seen, StepShape, Table,
    TemplateId,
};
use suzatary_core::health::{Health, Outcome};
use suzatary_core::mail::account::Authority;
use suzatary_core::mail::address::EmailAddress;
use suzatary_core::plugin::Manifest;
use suzatary_core::settings::{Page, Settable, Settables};

fn email() -> impl Strategy<Value = EmailAddress> {
    "[a-c]{1,3}@example\\.test".prop_map(|text| EmailAddress::new(&text).expect("an address"))
}

fn connection() -> impl Strategy<Value = Connection> {
    let made = |id: TemplateId, params: Params| Connection::new(id, params).expect("its shape");
    prop_oneof![
        Just(made(TemplateId::GitRemote, Params::Nothing)),
        "/[a-c]{1,3}(/[a-c]{1,3})?".prop_map(move |folder| made(
            TemplateId::DriveCli,
            Params::Folder(folder.parse().expect("a folder"))
        )),
        "[A-Za-z]{1,6}( [a-z]{1,3})?".prop_map(move |label| made(
            TemplateId::Ics,
            Params::Label(suzatary_core::connect::Label::parse(&label).expect("a label"))
        )),
        email().prop_map(move |email| made(TemplateId::Gmail, Params::Address(email))),
        email().prop_map(move |email| made(TemplateId::Proton, Params::Address(email))),
        (
            email(),
            prop_oneof![Just(Authority::Organizations), Just(Authority::Consumers)]
        )
            .prop_map(move |(email, tenant)| made(
                TemplateId::MsImap,
                Params::Microsoft { email, tenant }
            )),
        Just(made(TemplateId::Model, Params::Nothing)),
        Just(made(TemplateId::Ntfy, Params::Nothing)),
        Just(made(TemplateId::Fcm, Params::Nothing)),
    ]
}

/// A plugin the operator declared by hand, so a generated instance holds manifests no template
/// would write: an `ics` with another component and a widened origin, and another plugin.
fn by_hand() -> impl Strategy<Value = Vec<Manifest>> {
    let ics = Manifest::declare(
        "ics",
        "plugins/mine/ics.wasm",
        "plugins/mine/ics.schema",
        &["https://calendar.example.test"],
    )
    .expect("a manifest");
    let other = Manifest::declare(
        "dueday",
        "dueday.wasm",
        "dueday.schema",
        &["https://dueday.example.test"],
    )
    .expect("a manifest");
    let bare = match TemplateId::Ics.template().table {
        Table::Link(fragment) => fragment.manifest().expect("its manifest"),
        _ => unreachable!("the calendar's template writes a link"),
    };
    prop_oneof![
        Just(Vec::new()),
        Just(vec![ics.clone()]),
        Just(vec![bare.clone()]),
        Just(vec![other.clone(), bare]),
        Just(vec![other.clone(), ics]),
        Just(vec![other]),
    ]
}

/// An instance: tables written by hand, then by connections, refusals ignored.
fn declared() -> impl Strategy<Value = Declared> {
    (by_hand(), proptest::collection::vec(connection(), 0..8)).prop_map(|(plugins, connections)| {
        let start = Declared {
            plugins,
            ..Declared::default()
        };
        connections.iter().fold(start, |declared, connection| {
            declared.with(connection).unwrap_or(declared)
        })
    })
}

fn flip<T: Clone>(list: &[T]) -> Vec<T> {
    list.iter().rev().cloned().collect()
}

fn reversed(declared: &Declared) -> Declared {
    Declared {
        accounts: flip(&declared.accounts),
        plugins: flip(&declared.plugins),
        drive: flip(&declared.drive),
        channels: flip(&declared.channels),
    }
}

fn health(results: &[(u32, bool)]) -> Option<Health> {
    results
        .iter()
        .map(|(minute, ok)| {
            Health::of(
                chrono::NaiveDate::from_ymd_opt(2026, 10, 9)
                    .and_then(|day| day.and_hms_opt(9, minute % 60, 0))
                    .expect("an instant"),
                if *ok {
                    Outcome::Succeeded
                } else {
                    Outcome::Failed(suzatary_core::connect::Failure::Unclassified)
                },
                "the sync said no",
            )
        })
        .reduce(|joined, one| joined.join(&one))
}

proptest! {
    /// A connection written into an instance is read back out of it, whole.
    #[test]
    fn a_connection_is_read_back_out_of_its_tables(declared in declared(), connection in connection()) {
        if let Ok(written) = declared.with(&connection) {
            let read = written.connections();
            prop_assert_eq!(read.get(&connection.key()), Some(&connection));
        }
    }

    /// Removing what was added leaves the tables as they were but for a plugin's manifest the
    /// add declared, which stays, and adding twice is refused.
    #[test]
    fn removing_what_was_added_leaves_what_was_there(declared in declared(), connection in connection()) {
        if let Ok(written) = declared.with(&connection) {
            let mut kept = declared.clone();
            kept.plugins.extend(
                written
                    .plugins
                    .iter()
                    .filter(|held| declared.plugins.iter().all(|had| had.name() != held.name()))
                    .map(|added| added.without_credential(&connection.secrets()[0])),
            );
            prop_assert_eq!(written.without(&connection), Ok(kept));
            prop_assert_eq!(
                written.with(&connection),
                Err(Refused::Duplicate(connection.called()))
            );
        }
    }

    /// Only a template no table holds is refused on an instance without its key.
    #[test]
    fn a_new_connection_is_written_unless_no_table_holds_it(declared in declared(), connection in connection()) {
        let held = declared.connections().contains_key(&connection.key());
        match declared.with(&connection) {
            Ok(_) => prop_assert!(!held),
            Err(Refused::Duplicate(_)) => prop_assert!(held),
            Err(Refused::Unwritten { .. }) => prop_assert!(matches!(
                connection.id(),
                TemplateId::GitRemote | TemplateId::Model
            )),
            Err(other) => prop_assert!(false, "{other}"),
        }
    }

    /// The connections are a function of the tables as sets: no order of the file's entries
    /// moves them.
    #[test]
    fn the_reading_is_a_function_of_the_tables_as_sets(declared in declared()) {
        prop_assert_eq!(declared.connections(), reversed(&declared).connections());
    }

    /// Every secret a connection's steps ask for is a name its deployment declares, so its card
    /// mints: deriving the settable names is a homomorphism over the connections.
    #[test]
    fn every_secret_a_step_asks_for_is_settable(declared in declared()) {
        let settables = Settables::derive("MODEL_TOKEN", declared.channels.iter().copied(),
            None,
            &declared.plugins,
        );
        for connection in declared.connections().values() {
            for name in connection.secrets() {
                prop_assert!(Settable::mint(&settables, &name).is_ok(), "{name}");
            }
        }
    }

    /// A connection's text form and its stored literal read back as it.
    #[test]
    fn a_connection_reads_back_from_its_text_and_its_literal(connection in connection()) {
        prop_assert_eq!(
            Connection::parse(connection.id().as_str(), &connection.params().texts()),
            Ok(connection.clone())
        );
        prop_assert_eq!(Connection::from_value(&connection.to_value()), Ok(connection.clone()));
        let json = serde_json::to_string(&connection).expect("it prints");
        prop_assert_eq!(serde_json::from_str::<Connection>(&json).ok(), Some(connection));
    }

    /// Meeting a step never makes a standing ask more: more secrets resolving, the sign-in held
    /// or the check once succeeded leave it below where it was, in the join's order.
    #[test]
    fn meeting_a_step_never_makes_a_standing_ask_more(
        connection in connection(),
        resolved in proptest::collection::vec(any::<bool>(), 0..3),
        more in proptest::collection::vec(any::<bool>(), 0..3),
        signed in any::<bool>(),
        results in proptest::collection::vec((0u32..60, any::<bool>()), 0..4),
        later in proptest::collection::vec((0u32..60, any::<bool>()), 0..4),
    ) {
        let secrets = connection.secrets();
        let held = |on: &dyn Fn(usize) -> bool| -> Vec<String> {
            secrets.iter().enumerate().filter(|(at, _)| on(*at)).map(|(_, name)| name.clone()).collect()
        };
        let at = |list: &[bool], index: usize| list.get(index).copied().unwrap_or(false);
        let fewer = held(&|index| at(&resolved, index));
        let wider = held(&|index| at(&resolved, index) || at(&more, index));
        let standing = |set: &[String], signed_in: bool, health: Option<&Health>| {
            let secret = |name: &str| {
                if set.iter().any(|held| held == name) { SecretState::Set } else { SecretState::Missing }
            };
            connection.standing(&Seen {
                secret: &secret,
                signed_in: &|_: &EmailAddress| signed_in,
                may_send: &|_: &EmailAddress| signed_in,
                checked: Checked::of(health),
                reach: &PLAIN,
            })
        };
        let (before, joined) = (health(&results), health(&[results.clone(), later].concat()));
        let narrow = standing(&fewer, false, before.as_ref());
        for wide in [
            standing(&wider, false, before.as_ref()),
            standing(&fewer, signed, before.as_ref()),
            standing(&fewer, false, joined.as_ref()),
        ] {
            prop_assert_eq!(narrow.clone().join(wide), narrow.clone());
        }
    }
}

/// Every template is the catalog's under its own name, spelled once, and its help links are
/// https, so a card never draws another scheme as a button.
#[test]
fn the_catalog_is_one_template_per_name() {
    for (template, id) in CATALOG.iter().zip(TemplateId::ALL) {
        assert_eq!(template.id, id);
        assert_eq!(id.template(), template);
        assert_eq!(TemplateId::parse(id.as_str()), Ok(id));
        for step in template.steps {
            if let StepShape::Secret { help, .. } = step {
                assert!(
                    !matches!(help.page, Page::At(href) if !href.starts_with("https://")),
                    "{id:?}"
                );
            }
        }
    }
    let mut names: Vec<&str> = TemplateId::ALL.iter().map(|id| id.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), TemplateId::ALL.len());
}
