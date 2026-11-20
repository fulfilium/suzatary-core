//! The vocabulary a proposal's act is stored in: every constructor an act prints, from the
//! proposal down to its leaves' records, each declared beside the type it prints.

use crate::secretary::{acts, changes, proposal};
use prodrome_core::literal::codec::Constructor;
use suzatary_held::secretary::{attached, attachment_source};
use suzatary_record::todo::price;

/// Every constructor of a stored act, with its fields in printed order. None is a term's or an
/// inbox event's, so one walker reads an `Asked` whole.
pub const SIGNATURES: &[Constructor] = &[
    proposal::EMAIL,
    proposal::FORWARD,
    proposal::FORWARDED,
    proposal::REMEMBER,
    proposal::CONSOLIDATE,
    suzatary_todo::decisions::DECIDE,
    changes::CHANGES,
    changes::ADD,
    changes::ADD_FROM,
    <suzatary_record::record::Source as prodrome_core::literal::codec::Record>::CONSTRUCTOR,
    changes::UPDATE,
    changes::COMPLETE,
    changes::CANCEL,
    changes::REOPEN,
    changes::TEND,
    changes::REPRICE,
    price::FLAT_PRICE,
    price::DEADLINE_PRICE,
    price::RECUR_PRICE,
    price::PERIODIC_PRICE,
    suzatary_record::record::SUBTODO,
    acts::AUTH,
    acts::AUTH_SEND,
    acts::SECRET,
    acts::WEB,
    acts::PATCH,
    acts::DRIVE_READ,
    acts::CODING_SESSION,
    acts::DEVICE,
    acts::SIGN_IN,
    acts::SIGN_IN_CODE,
    acts::UNLOCK_STORE,
    acts::CONNECT,
    acts::DISCONNECT,
    acts::GRANT,
    suzatary_connect::connect::CONNECTION,
    acts::ERASE,
    acts::AFRESH,
    acts::RETRACT,
    acts::NOISE,
    acts::RESTORE,
    acts::GOOGLE_CLIENT,
    acts::GOOGLE_SIGN_IN,
    acts::CALENDAR_EVENT,
    suzatary_services::web::SEARCH,
    suzatary_services::web::FETCH,
    attached::ATTACHED,
    attachment_source::FROM_MAIL,
    attachment_source::FROM_DRIVE,
    attachment_source::FROM_UPLOAD,
    attachment_source::FROM_KEPT,
    attachment_source::FROM_REPO,
    suzatary_mail::mail::STORED_MESSAGE,
    suzatary_mail::mail::STORED_PART,
    suzatary_held::held_value::HELD,
];

#[cfg(test)]
mod tests {
    /// Every constructor the inbox stores, with its fields in print order: an act's, from the
    /// proposal down to its leaves' records, the inbox's events, and the identity a proposal's
    /// name hashes.
    #[test]
    fn the_stored_names_are_pinned() {
        let stored = [
            super::SIGNATURES,
            crate::secretary::inbox_event::SIGNATURES,
            &[crate::secretary::inbox::RECURRING],
        ]
        .concat();
        prodrome_core::literal::codec::assert_pinned(
            &stored,
            "
            Email(account, to, cc, subject, body, attachments)
            Forward(account, to, cc, subject, note, message)
            Forwarded(file, style, from, subject, date)
            Remember(text, kind)
            Consolidate(text, kind, from)
            Decide(id, title, body, rationale, supersedes)
            Changes(chain, acts)
            Add(todo, body, rationale, kind, category, priority, detail, waiting_on)
            AddFrom(todo, body, rationale, kind, category, priority, detail, waiting_on, source, created, parent)
            Source(sender, subject, date, hash, thread_id, message_id)
            Update(todo, body, detail, detail_append, rationale, priority, waiting_on, clear_waiting_on, category, subtodos, defer_to)
            Complete(todo, note)
            Cancel(todo, note)
            Reopen(todo, note)
            Tend(todo, note)
            Reprice(todo, priority, rationale)
            FlatPrice(percent)
            DeadlinePrice(end_date, start_percent, end_percent, lead_up_days, start_date)
            RecurPrice(days, from_percent, to_percent)
            PeriodicPrice(days, from_percent, to_percent)
            SubTodo(body, done)
            Auth(account)
            AuthSend(account)
            Secret(name, why)
            Web(request, rule)
            Patch(repo, visibility, base, patch, message)
            DriveRead(path, size)
            CodingSession(task)
            Device(actor, key)
            SignIn(since)
            SignInCode(url)
            UnlockStore(after)
            Connect(connection)
            Disconnect(connection)
            Grant(plugin, origin)
            Connection(template, params)
            Erase(fact, keys)
            Afresh(conversation, turn)
            Retract(node)
            Noise(sender, message)
            Restore(card)
            GoogleClient(redirect)
            GoogleSignIn(account, scopes)
            CalendarEvent(account, title, when, place)
            Search(query)
            Fetch(url)
            Attached(source, name, media, size, sha256)
            FromMail(message, name)
            FromDrive(path)
            FromUpload(id)
            FromKept(id)
            FromRepo(repo, path, at)
            StoredMessage(id, size, sha256, held, parts)
            StoredPart(name, media, size, sha256)
            Held(kind, hint)
            Asked(proposal, at, actor, act, why, sources, asked, serves, spec)
            Noticed(proposal, at, actor, act, why, sources, asked, serves, spec)
            Triaged(proposal, at, actor, act, why, sources, asked, serves, spec)
            Offered(proposal, at, actor, plugin, act, why, sources, asked, serves, spec)
            NotSent(proposal, at, actor, attempt, reason)
            Lacks(proposal, at, actor, attempt, lacking)
            Refused(proposal, at, actor, attempt, reason)
            Unfinished(proposal, at, actor, attempt, reason, link)
            Started(proposal, at, actor, attempt, began, expires_at)
            Sent(proposal, at, actor, recipients, message_id)
            Applied(proposal, at, actor, objects, chain)
            Acted(proposal, at, actor, what)
            MayHaveGone(proposal, at, actor, reason)
            Checked(proposal, at, actor)
            Dismissed(proposal, at, actor, reason)
            Superseded(proposal, at, actor)
            NoLongerNeeded(proposal, at, actor, why)
            DoneElsewhere(proposal, at, actor, what)
            Recurring(at, act)
            ",
        );
    }
}
