//! Every secretary verb, one type each, and what several of them share.

use super::SecretarySurface;
use super::filed::{Filed, Filing};
use super::gate::Gate;
use super::proposal::{Proposal, Reasons};
use super::proposer::Proposer;
use super::responded::Withdrawal;
use super::verb_name::Binding;
use crate::agent::verbs;
use crate::conversation_id::ConversationId;
use crate::mail_error::MailError;
use crate::reference::{Reference, TodoRef};
use crate::refusal::{Refusal, shown};
use crate::secretary::Secretary;
use crate::secretary::{conversations, transcript};

mod add_topics;
mod attachments;
mod briefing;
mod briefing_delta;
mod calendar;
mod consolidate;
mod context;
mod decisions;
mod draft_note;
mod drive_list;
mod drive_read;
mod filing;
mod forget;
mod junk_digest;
mod junk_show;
mod propose_act;
mod propose_changes;
mod propose_coding_session;
mod propose_decision;
mod propose_email;
mod propose_forward;
mod propose_patch;
mod recall;
mod remember;
mod repo_grep;
mod repo_read;
mod repo_tree;
mod repos_list;
mod revise_topics;
mod search;
mod search_older;
mod sync_mail;
mod triage_cards;
mod triage_withdraw;
mod upload_read;
mod watch_mail;
mod weather;
mod web;
mod withdraw_proposal;

pub(crate) use add_topics::AddTopics;
pub(crate) use attachments::Attachments;
pub(crate) use revise_topics::ReviseTopics;

/// The verbs the triage agent holds too, over the agent's part of the secretary's record.
pub type GitDiffSummary = verbs::GitDiffSummary<SecretarySurface>;
/// A todo's events.
pub type History = verbs::History<SecretarySurface, verbs::HistoryArgs>;
/// One chain's open todos.
pub type ListOpenTodos = verbs::ListOpenTodos<SecretarySurface>;
/// A todo's lifecycle event, as the trusted assistant.
pub type Record = verbs::Record<SecretarySurface>;
/// Author a todo, as the trusted assistant.
pub type TodoAdd = verbs::TodoAdd<SecretarySurface>;
/// Change fields on a todo, as the trusted assistant.
pub type TodoUpdate = verbs::TodoUpdate<SecretarySurface>;
/// One message in full, never paged.
pub type Show = verbs::Show<SecretarySurface, verbs::WholeArgs>;
/// One todo as the chain holds it.
pub type TodoShow = verbs::TodoShow<SecretarySurface>;
/// A part of the record opened, every part read in words.
pub type Zoom = verbs::Zoom<SecretarySurface>;
pub use briefing::Briefing;
pub(crate) use briefing_delta::BriefingDelta;
pub use calendar::Calendar;
pub use consolidate::Consolidate;
pub use context::Context;
pub use decisions::Decisions;
pub use draft_note::DraftNote;
pub(crate) use drive_list::DriveList;
pub use drive_read::DriveRead;
pub(crate) use filing::{FilingArgs, IfCarded, Reasoned};
pub use forget::Forget;
pub use junk_digest::JunkDigest;
pub(crate) use junk_show::JunkShow;
pub(crate) use propose_act::ProposeAct;
pub(crate) use propose_changes::ProposeChanges;
pub(crate) use propose_changes::check;
pub(crate) use propose_coding_session::ProposeCodingSession;
pub(crate) use propose_decision::ProposeDecision;
pub(crate) use propose_email::ProposeEmail;
pub(crate) use propose_forward::ProposeForward;
pub(crate) use propose_patch::ProposePatch;
pub(crate) use recall::Recall;
pub use remember::Remember;
pub(crate) use repo_grep::RepoGrep;
pub(crate) use repo_read::RepoRead;
pub(crate) use repo_tree::RepoTree;
pub(crate) use repos_list::ReposList;
pub use search::Search;
pub(crate) use search_older::SearchOlder;
pub(crate) use sync_mail::SyncMail;
pub use triage_cards::{TriageCards, answered, listed, standing};
pub(crate) use triage_withdraw::TriageWithdraw;
pub(crate) use upload_read::UploadRead;
pub(crate) use watch_mail::WatchMail;
pub use weather::Weather;
pub use web::Web;
pub(crate) use withdraw_proposal::WithdrawProposal;

/// Keep a file for the model, in the box's words when it cannot be.
fn kept(caps: &Secretary<'_>, bytes: &[u8]) -> Result<crate::attachment::AttachmentId, String> {
    let id = crate::attachment::AttachmentId::of(bytes);
    caps.kept
        .set(&id, &bytes.to_vec())
        .map_err(|failed| failed.to_string())?;
    Ok(id)
}

/// A missing message or attachment is a 404; anything else is [`Refusal::internal`].
pub(crate) fn mail_refusal(failed: &MailError) -> Refusal {
    match failed {
        MailError::Absent { .. } => Refusal::not_found(failed.to_string()),
        MailError::Query(_) | MailError::Unreachable(_) => Refusal::internal(failed.to_string()),
    }
}

// Every `propose_*` verb files its act in the inbox as `assistant`; `serves` is kept as data.

/// File `proposal` in the inbox, name it in the conversation, and supersede what it revises.
///
/// The object is filed before the line names it, and an unsupersedable `revises` is refused first;
/// a coding session serving a todo is counted with each todo read open in `chains`.
///
/// # Errors
/// A `revises` naming no open proposal, or a store's failure to append.
pub(crate) fn propose(
    caps: &Secretary<'_>,
    chains: &[super::Bound<'_>],
    id: &ConversationId,
    proposal: Proposal,
    reasons: &Reasons,
    serves: Option<TodoRef>,
    revises: Option<usize>,
) -> Result<usize, Refusal> {
    let stored = conversations::read(caps.conversations, id)?;
    if let Some(old) = revises {
        withdrawable(caps, &stored, old)?;
    }
    // The box names the round a card was asked in; the model knows no conversation's id.
    let reasons = match add_topics::round_answered(&stored) {
        Some(round) => reasons.clone().citing(Reference::Target(round)),
        None => reasons.clone(),
    };
    let (filed, _) = super::filed::file(
        caps.conversations,
        caps.inbox,
        chains,
        caps.clock.now(),
        Filing {
            conversation: id,
            proposer: Proposer::Secretary,
            proposal,
            reasons,
            serves,
            revises,
        },
    )?;
    // The same act asked again answers the card already here, withdrawing nothing.
    let Filed::Drawn(index) = filed else {
        return Ok(filed.index());
    };
    if let Some(old) = revises {
        let stored = conversations::read(caps.conversations, id)?;
        let standing = withdrawable(caps, &stored, old)?;
        super::inbox::withdraw(
            caps.inbox,
            &standing.object.id,
            &transcript::writer(),
            caps.clock.now(),
            Withdrawal::Superseded,
        )?;
        // A remember card files with no `revises`, under the memory's hold, so this never runs
        // beneath it.
        caps.memories.sweep_after_write();
    }
    Ok(index)
}

/// Proposal `index`, if nothing has completed it yet so the secretary may withdraw or revise it.
fn withdrawable(
    caps: &Secretary<'_>,
    stored: &super::store::Stored,
    index: usize,
) -> Result<super::store::Standing, Refusal> {
    let standing = stored.standing(index, &super::inbox::fold(caps.inbox)?)?;
    if standing.object.reading.performs() {
        Ok(standing)
    } else {
        Err(super::store::refused(
            index,
            &standing.object.reading,
            caps.clock.now(),
        ))
    }
}

/// A `path` argument, parsed.
fn drive_path(path: &str) -> Result<crate::drive_path::DrivePath, Refusal> {
    path.parse()
        .map_err(|failed: crate::drive_path::DrivePathError| {
            Refusal::bad_request(failed.to_string())
        })
}

/// A missing file is a 404, a refused one a 400; anything else is the box's.
pub(crate) fn drive_refusal(failed: &crate::drive::DriveFailure) -> Refusal {
    use crate::drive::DriveFailure;
    match failed {
        DriveFailure::Missing(_) => Refusal::not_found(failed.to_string()),
        DriveFailure::Refused(_) => Refusal::bad_request(failed.to_string()),
        DriveFailure::Unreachable(_) | DriveFailure::SignedOut(_) => {
            Refusal::internal(failed.to_string())
        }
    }
}

fn not_a_junk_account(account: &str, known: &[String]) -> Refusal {
    Refusal::bad_request(format!(
        "{} is not an account whose Junk this box reads; those are: {}",
        shown(account),
        known.join(", ")
    ))
}

/// The `sources` argument: text forms, comma-separated, each through [`Reference`]'s parser.
fn references(list: &str, field: &str) -> Result<Vec<Reference>, Refusal> {
    list.split(',')
        .map(str::trim)
        .filter(|one| !one.is_empty())
        .map(|one| {
            one.parse::<Reference>()
                .map_err(|failed| Refusal::bad_request(format!("{field}: {failed}")))
        })
        .collect()
}

/// A fact written as a card the owner taps, in a round that has read something not his own:
/// its words kept under the memory's hold and named by the card.
///
/// # Errors
/// [`Refusal::bad_request`] for missing reasons; the stores' own.
fn memory_card(
    caps: &crate::secretary::Secretary<'_>,
    id: &ConversationId,
    carded: &IfCarded,
    (fact, kind, replaces): (
        &super::memory::Fact,
        super::memory::Kind,
        super::memory::Replaces,
    ),
) -> Result<usize, Refusal> {
    let reasons = carded.reasons("the fact")?;
    // Under the memory's hold, so no sweep decides about these words between keeping them and
    // the card that names them being filed.
    let held = caps.memories.hold()?;
    let kept = held.keep_card(fact)?;
    let proposal = Proposal::Remember {
        text: kept.name().clone(),
        kind,
        replaces,
    };
    propose(caps, &[], id, proposal, &reasons, None, None).or_else(|refused| {
        held.unkeep(&kept)?;
        Err(refused)
    })
}

/// The round's opening turn, for a memory write that has no card: a forgetting, which an email
/// must not be able to make, writes only where `remember` would.
///
/// # Errors
/// [`Refusal::forbidden`] for a round the box opened, or one that has read something not the
/// owner's own words, saying `instead`, what the owner can do.
fn clean_round(call: &super::Call<'_>, instead: &str) -> Result<super::memory::Origin, Refusal> {
    match Gate::of(
        Binding::Remembers,
        &conversations::read(call.caps.conversations, call.conversation)?,
    ) {
        Gate::Refused(why) => Err(Refusal::forbidden(why)),
        Gate::Card => Err(Refusal::forbidden(format!(
            "this round has read something that is not the owner's own words, so it changes \
             nothing in the memory; {instead}"
        ))),
        Gate::Binds(said) => Ok(said),
    }
}

/// A read's failure, in the words the model can act on.
pub(crate) fn refused_by(failure: &crate::repositories::RepoFailure) -> Refusal {
    use crate::repositories::RepoFailure;
    match failure {
        RepoFailure::Unknown(_) | RepoFailure::Missing(_) => {
            Refusal::not_found(failure.to_string())
        }
        RepoFailure::Conflict(_) | RepoFailure::TooLarge { .. } => {
            Refusal::bad_request(failure.to_string())
        }
        RepoFailure::Public(_) => Refusal::forbidden(failure.to_string()),
        RepoFailure::Unavailable(_) => Refusal::internal(failure.to_string()),
    }
}

/// `repo`, parsed, and the commit `at` names (a branch, a tag or a commit), or the default
/// branch's tip.
fn located(
    caps: &Secretary<'_>,
    repo: &str,
    at: Option<&str>,
) -> Result<(crate::repo_ids::RepoName, crate::repo_ids::CommitId), Refusal> {
    let bad = |failed: crate::repo_ids::RepoNameError| Refusal::bad_request(failed.to_string());
    let repo: crate::repo_ids::RepoName = repo.parse().map_err(bad)?;
    let at = at
        .map(str::trim)
        .filter(|at| !at.is_empty())
        .map(|at| crate::repo_ids::Rev::try_from(at.to_owned()).map_err(bad))
        .transpose()?;
    let commit = caps
        .repositories
        .resolve(&repo, at.as_ref())
        .map_err(|failed| refused_by(&failed))?;
    Ok((repo, commit))
}

/// A repository path; absent is the top.
fn path_of(path: Option<String>) -> Result<crate::repo_ids::RepoPath, Refusal> {
    crate::repo_ids::RepoPath::try_from(path.unwrap_or_default())
        .map_err(|failed| Refusal::bad_request(failed.to_string()))
}
