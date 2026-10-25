use serde_json::Value;

use crate::agent::reads;
use crate::capability::{Answer, Snapshot};
use crate::refusal::Refusal;
use crate::secretary::{Call, SecretarySurface};
use crate::settings::Settables;
use crate::verb::{NoArgs, Verb};

/// The manuals and the vocabularies, first.
pub struct Context;

/// Every declared secret that is missing, and every one the store failed on, names only, or
/// `""` when all are set; when the store is locked, that, since then no name is known to be unset.
fn missing_secrets(snapshot: &Snapshot, settables: &Settables) -> String {
    let Ok(answers) = snapshot.answers() else {
        return "the secret store is locked, so no declared secret could be read".to_owned();
    };
    let named = |wanted: Answer| -> Vec<String> {
        settables
            .iter()
            .filter(|settable| answers.get(settable) == Some(&wanted))
            .map(|settable| settable.name().to_owned())
            .collect()
    };
    [
        ("declared but unset", named(Answer::Missing)),
        ("the secret store could not read", named(Answer::Failed)),
    ]
    .into_iter()
    .filter(|(_, names)| !names.is_empty())
    .map(|(what, names)| format!("{what}: {}", names.join(", ")))
    .collect::<Vec<_>>()
    .join("; ")
}

impl Verb for Context {
    type Surface = SecretarySurface;
    type Args = NoArgs;
    type Answer = Value;
    const NAME: &'static str = "context";
    const DESCRIPTION: &'static str = "Read this first. The operator's manual and the exact vocabularies the \
        authoring verbs accept; `recall`, `calendar` and the other reads answer the \
        rest on demand. Your OWN \
        manual (SECRETARY.md) is already your system prompt on every turn and is \
        not repeated here.";

    fn run(call: &Call<'_>, _: NoArgs) -> Result<Value, Refusal> {
        // `declared_secrets` is here only, since the agent must never learn what is missing; the
        // calendar is `calendar`'s to answer, on demand.
        let mut answer = reads::manual(call.caps.manuals, call.deployment);
        if let Some(object) = answer.as_object_mut() {
            object.insert(
                "declared_secrets".to_owned(),
                Value::String(missing_secrets(
                    &Snapshot::read(call.caps.secrets, call.settables.iter()),
                    call.settables,
                )),
            );
        }
        Ok(answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{SecretState, Secrets};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fake that answers `secret_state` from a fixed set; a name it does not hold is
    /// `locked` when listed so, else missing.
    struct FakeSecrets {
        resolves: std::collections::BTreeSet<&'static str>,
        locked: std::collections::BTreeSet<&'static str>,
        failed: std::collections::BTreeSet<&'static str>,
        asked: AtomicUsize,
    }

    impl FakeSecrets {
        fn new(resolves: &[&'static str], locked: &[&'static str]) -> Self {
            Self {
                resolves: resolves.iter().copied().collect(),
                locked: locked.iter().copied().collect(),
                failed: std::collections::BTreeSet::new(),
                asked: AtomicUsize::new(0),
            }
        }

        fn missing(&self) -> String {
            missing_secrets(&Snapshot::read(self, declared().iter()), &declared())
        }
    }

    impl Secrets for FakeSecrets {
        fn secret_state(&self, settable: &crate::settings::Settable) -> SecretState {
            self.asked.fetch_add(1, Ordering::SeqCst);
            if self.resolves.contains(settable.name()) {
                SecretState::Set
            } else if self.locked.contains(settable.name()) {
                SecretState::Locked
            } else if self.failed.contains(settable.name()) {
                SecretState::Failed
            } else {
                SecretState::Missing
            }
        }
        fn mail_grants(
            &self,
            _account: &crate::mail::account::MailAccount,
        ) -> crate::capability::MailGrants {
            crate::capability::MailGrants::default()
        }

        fn google_grant(&self, _: &crate::mail::address::EmailAddress) -> crate::google::Held {
            crate::google::Held::NotSignedIn
        }
    }

    fn declared() -> Settables {
        Settables::derive(
            [crate::settings::DeclaredProvider {
                name: "phala".to_owned(),
                base_url: "https://phala.example.test/v1".to_owned(),
                key: "PHALA_API_KEY".to_owned(),
                attestation: crate::settings::Attestation::Unclaimed,
            }]
            .iter(),
            std::iter::empty(),
            None,
            &[],
        )
    }

    /// A locked store names no secret unset, and is asked once, not once per name and state.
    #[test]
    fn a_locked_store_is_asked_once_and_names_nothing_unset() {
        let caps = FakeSecrets::new(&[], &["ANTHROPIC_API_KEY", "PHALA_API_KEY"]);
        assert_eq!(
            caps.missing(),
            "the secret store is locked, so no declared secret could be read"
        );
        assert_eq!(caps.asked.load(Ordering::SeqCst), 1);
    }

    /// A value the store could not read is that name's: the others are named as they stand.
    #[test]
    fn a_name_the_store_failed_on_is_named_apart() {
        let mut caps = FakeSecrets::new(&["ANTHROPIC_API_KEY"], &[]);
        caps.failed.insert("PHALA_API_KEY");
        assert_eq!(
            caps.missing(),
            "declared but unset: CLAUDE_CODE_OAUTH_TOKEN; \
             the secret store could not read: PHALA_API_KEY"
        );
    }
}
