//! The deployment's mail roster, asked of the files themselves (`instance.toml` and
//! `mbsyncrc`), since literals in a test would stay green while the deployment drifted.
//!
//! `mbsync`'s `Patterns` decides what lands on disk and `inbox_folders` what gets tagged
//! `+inbox`; a folder on one list and not the other is synced but silently invisible to
//! triage.

use suzatary_mail::mail::account::Sync;

/// The roster passes `Roster::new`'s checks: no duplicate address or maildir folder.
#[test]
fn the_instance_file_reads_and_its_roster_is_one() {
    let Some(instance) = crate::this_box::instance() else {
        eprintln!("skipped: this checkout has no instance.toml");
        return;
    };
    assert!(
        !instance.mail.accounts.accounts().is_empty(),
        "a deployment with no mail accounts syncs nothing"
    );
    // Every account names a strategy this program knows; the parse refuses any other, and an
    // address that is not one.
    for account in instance.mail.accounts.accounts() {
        assert!(!account.subdir.is_empty(), "{account:?}");
    }
}

/// mbsync spells the inbox `INBOX` and the maildir leaf is `Inbox`; every other folder maps
/// verbatim.
#[test]
fn every_proton_inbox_folder_is_actually_synced() {
    let Some(instance) = crate::this_box::instance() else {
        eprintln!("skipped: this checkout has no instance.toml");
        return;
    };
    let Ok(rc) = std::fs::read_to_string(&instance.mail.mbsyncrc) else {
        eprintln!(
            "skipped: no mbsyncrc at {}",
            instance.mail.mbsyncrc.display()
        );
        return;
    };
    let patterns: Vec<&str> = rc
        .lines()
        .filter(|line| line.starts_with("Patterns "))
        .collect();
    assert!(!patterns.is_empty(), "mbsyncrc names no Patterns line");

    for account in instance.mail.accounts.accounts() {
        let Sync::Proton(_) = &account.sync else {
            continue;
        };
        for folder in account.inbox_folders() {
            let spelled = if folder == "Inbox" { "INBOX" } else { folder };
            assert!(
                patterns.iter().any(|line| line.contains(spelled)),
                "{folder} is in inbox_folders and in no Patterns line — it \
                 would be tagged +inbox and never land on disk"
            );
        }
    }
}

/// The channel the syncer runs is the group that covers sent, or sent coverage silently stops;
/// the sent channel is `Sync Pull`, so it can never mutate the upstream mailbox.
#[test]
fn the_proton_channel_is_the_group_covering_a_pull_only_sent() {
    let Some(instance) = crate::this_box::instance() else {
        eprintln!("skipped: this checkout has no instance.toml");
        return;
    };
    let Ok(rc) = std::fs::read_to_string(&instance.mail.mbsyncrc) else {
        eprintln!(
            "skipped: no mbsyncrc at {}",
            instance.mail.mbsyncrc.display()
        );
        return;
    };
    for account in instance.mail.accounts.accounts() {
        let Sync::Proton(proton) = &account.sync else {
            continue;
        };
        let group = rc
            .lines()
            .find(|line| line.starts_with(&format!("Group {}", proton.channel)))
            .unwrap_or_else(|| {
                panic!(
                    "mbsyncrc names no `Group {}` — the syncer runs a channel \
                     that does not exist",
                    proton.channel
                )
            });
        let sent = group
            .split_whitespace()
            .skip(2)
            .find(|channel| channel.contains("sent"))
            .expect("the group covers no sent channel — sent coverage would stop silently");

        let block = rc
            .split(&format!("Channel {sent}"))
            .nth(1)
            .expect("the sent channel the group names has no block")
            .split("\nGroup ")
            .next()
            .expect("a block ends at the next Group or at the file");
        assert!(
            block.contains("Sync Pull"),
            "the {sent} channel is not `Sync Pull` — a local change could \
             propagate upstream through it"
        );
    }
}

/// Provenance is derived from where a message lives, so a relative mail root would depend on
/// the sync's cwd.
#[test]
fn the_mail_root_is_absolute_and_the_state_file_is_in_the_data_tree() {
    let Some(instance) = crate::this_box::instance() else {
        eprintln!("skipped: this checkout has no instance.toml");
        return;
    };
    assert!(
        instance.mail.root.is_absolute(),
        "the maildir root is relative: {}",
        instance.mail.root.display()
    );
    assert!(
        instance.mail.state.starts_with(instance.data()),
        "state.json is the one mail state that is COMMITTED, so it lives in the \
         data tree: {}",
        instance.mail.state.display()
    );
    assert!(
        instance.mail.mbsyncrc.starts_with(instance.data()),
        "mbsyncrc is pinned to the data tree's tracked copy on purpose — the \
         home directory's was untracked and held its password in plaintext: {}",
        instance.mail.mbsyncrc.display()
    );
}
