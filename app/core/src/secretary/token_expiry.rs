//! A token that expires is a todo on the system's chain, due on its expiry day. Its id names the
//! token and the day, so rereading an expiry adds nothing; a later expiry completes the earlier todo.

use prodrome_core::event::Hash;

use crate::capability::Datetime;
use crate::chains::ChainPolicy;
use crate::deployment::Deployment;
use crate::refusal::Refusal;
use crate::secretary::changes::{self, Act, Changeset, Provenance};
use crate::settings::{Help, Settable};
use crate::todo::{Deadline, Price, TodoFields};

/// The todo id for `name` expiring on `day`.
#[must_use]
pub fn todo(name: &Settable, day: chrono::NaiveDate) -> String {
    format!("{}{}", prefix(name), day.format("%Y%m%d"))
}

fn prefix(name: &Settable) -> String {
    format!("token{}", name.handle())
}

/// The acts that bring `nodes` up to `expiring`. `settings` is the Settings page's address.
///
/// # Errors
/// The open list does not fold.
pub fn acts(
    nodes: &[crate::record::Node],
    at: Datetime,
    untrusted: &prodrome_core::policy::Untrusted,
    expiring: &[(Settable, crate::health::Instant)],
    (settings, category): (&str, &str),
) -> Result<Vec<Act>, Refusal> {
    let open = crate::todos::open_todos(nodes, at, untrusted)?;
    let mut acts = Vec::new();
    for (name, expires) in expiring {
        let day = expires.date();
        let id = todo(name, day);
        let label = Help::label(name.name());
        acts.extend(
            open.iter()
                .filter(|todo| todo.id.starts_with(&prefix(name)) && todo.id < id)
                .map(|todo| Act::Complete {
                    todo: todo.id.clone(),
                    note: format!("GitHub now says {label} expires on {day}, so it was replaced."),
                }),
        );
        if crate::todos::current(nodes, at, &id).is_some() {
            continue;
        }
        acts.push(Act::Add {
            todo: id,
            fields: TodoFields {
                body: format!("Replace {label} before it expires on {day}"),
                rationale: "A lapsed token stops the secretary reaching the repositories until it \
                            is replaced, a small cost it recovers from, so it floors at 70%."
                    .to_owned(),
                kind: "todo".to_owned(),
                category: category.to_owned(),
                price: Price::Deadline(Deadline {
                    end_date: day,
                    start_percent: 85,
                    end_percent: 70,
                    lead_up: Some(chrono::Duration::weeks(2)),
                    start_date: None,
                }),
                detail: format!(
                    "GitHub says {label} expires on {day}. It fills the secret `{}`. Make a new \
                     token on GitHub, then #link(\"{settings}\")[replace it in Settings]. The box \
                     completes this todo once GitHub names a later expiry.",
                    name.name()
                ),
                waiting_on: String::new(),
            },
            from: Provenance::default(),
        });
    }
    Ok(acts)
}

/// Write [`acts`] to the chain as the secretary.
///
/// # Errors
/// As [`acts`], and `gate::apply`'s refusals.
pub fn write(
    caps: &crate::secretary::Secretary<'_>,
    policy: &ChainPolicy,
    deployment: &Deployment,
    expiring: &[(Settable, crate::health::Instant)],
) -> Result<Vec<Hash>, Refusal> {
    let at = caps.clock.now();
    let nodes = caps.chain.nodes()?;
    let settings = format!("{}/#/settings", deployment.app_url.trim_end_matches('/'));
    // The least important category.
    let category = deployment
        .categories
        .iter()
        .max_by_key(|(_, rank)| **rank)
        .map_or("", |(name, _)| name.as_str());
    let acts = acts(
        &nodes,
        at,
        &policy.untrusted,
        expiring,
        (&settings, category),
    )?;
    let Ok(changeset) = Changeset::new(policy.name.as_str(), acts) else {
        return Ok(Vec::new());
    };
    let known: Vec<&str> = deployment.categories.keys().map(String::as_str).collect();
    crate::secretary::gate::apply(
        caps,
        policy,
        &known,
        deployment.zone,
        &changeset,
        crate::secretary::WRITES_AS,
    )
    .map_err(|unapplied| match unapplied {
        changes::Unapplied::Refused(refusal) | changes::Unapplied::Appending(refusal) => refusal,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::believed::Preview;
    use crate::capability::Parses;
    use crate::secretary::changes::Building;

    /// One todo per token and expiry: the same expiry read again writes nothing, and a later one
    /// completes the earlier day's todo and writes its own.
    #[test]
    fn one_todo_per_expiry_and_a_later_one_completes_the_last() {
        let settables = crate::settings::Settables::declared(
            crate::repos::TOKEN,
            crate::settings::Declaration::Repos,
        );
        let name = Settable::mint(&settables, crate::repos::TOKEN).expect("declared");
        let untrusted = prodrome_core::policy::Untrusted::none();
        let at = Datetime::new(2026, 11, 4, 12, 0, 0, 0).expect("an instant");
        let building = Building {
            now: crate::zone::Moment {
                at,
                zone: crate::zone::Zone::UTC,
            },
            by: crate::secretary::WRITES_AS,
            categories: &[],
            untrusted: &untrusted,
            typeset: &Parses,
        };
        let day = |d| chrono::NaiveDate::from_ymd_opt(2026, 12, d).expect("a day");
        let mut chain = Preview::chain(&[]);
        let read = |chain: &mut Preview, expires: chrono::NaiveDate| -> Vec<Act> {
            let expiring = [(name.clone(), expires.and_hms_opt(8, 0, 0).expect("a time"))];
            let acts = acts(
                chain.nodes(),
                at,
                &untrusted,
                &expiring,
                ("https://box.example.test/#/settings", ""),
            )
            .expect("the chain folds");
            if let Ok(changeset) = Changeset::new("suzatary", acts.clone()) {
                for event in changes::events_of(&changeset, chain, &building).expect("builds") {
                    chain.ahead(event).expect("lands");
                }
            }
            acts
        };
        let first = read(&mut chain, day(1));
        assert!(
            matches!(first.as_slice(), [Act::Add { todo: id, .. }] if *id == todo(&name, day(1))),
            "{first:?}"
        );
        assert!(
            read(&mut chain, day(1)).is_empty(),
            "the same expiry writes nothing"
        );
        let later = read(&mut chain, day(20));
        assert!(
            matches!(later.as_slice(), [Act::Complete { todo: old, .. }, Act::Add { todo: new, .. }]
                if *old == todo(&name, day(1)) && *new == todo(&name, day(20))),
            "{later:?}"
        );
        let open = crate::todos::open_todos(chain.nodes(), at, &untrusted).expect("folds");
        let ids: Vec<&str> = open.iter().map(|todo| todo.id.as_str()).collect();
        assert_eq!(ids, [todo(&name, day(20))]);
    }
}
