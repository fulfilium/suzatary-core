use super::{
    Agent, Briefings, Calendar, Clock, Corpus, Drive, Junk, MailSync, Mailbox, Manuals, Model,
    OlderMail, Render, Repositories, Repository, Secrets, Weather, Web,
};
use crate::record::TodoEvent;
use crate::store::Prodrome;

/// What the secretary may use: the agent's effects over the chain itself, plus a model,
/// its conversations and memory, and the owner's world read-only. Nothing here sends: its
/// only outward act is filing a proposal in the inbox, which a person taps.
///
/// ```compile_fail,E0609
/// fn push(secretary: suzatary_core::capability::Secretary<'_>) { let _ = secretary.notify; }
/// ```
/// ```compile_fail,E0609
/// fn peer(secretary: suzatary_core::capability::Secretary<'_>) { let _ = secretary.whois; }
/// ```
///
/// Its Drive reads and never writes: the record's port is a read port, and writing is
/// `drive::WritePort`, held by the vault alone.
///
/// ```compile_fail,E0599
/// use suzatary_core::drive::WritePort as _;
/// fn upload(secretary: suzatary_core::capability::Secretary<'_>, at: &suzatary_core::drive::DrivePath) {
///     let _ = secretary.drive.port().upload(at, "x", b"x");
/// }
/// ```
/// ```compile_fail,E0599
/// use suzatary_core::drive::WritePort as _;
/// fn keep(secretary: suzatary_core::capability::Secretary<'_>, at: &suzatary_core::drive::DrivePath) {
///     let _ = secretary.drive.port().keep(at, "x", b"x");
/// }
/// ```
/// ```compile_fail,E0599
/// use suzatary_core::drive::WritePort as _;
/// fn behind(secretary: suzatary_core::capability::Secretary<'_>, at: &suzatary_core::drive::DrivePath) {
///     let _ = secretary.drive.fenced().create_folder(at, "x");
/// }
/// ```
#[derive(Clone, Copy)]
pub struct Secretary<'a> {
    /// The chain it writes to as a trusted actor.
    pub chain: &'a dyn Prodrome<TodoEvent>,
    /// The triage queue.
    pub corpus: &'a dyn Corpus,
    /// The mail index, read-only.
    pub mail: &'a dyn Mailbox,
    /// The repository's gates.
    pub repository: &'a dyn Repository,
    /// The manuals.
    pub manuals: &'a dyn Manuals,
    /// Typst, for confirmed markup.
    pub render: &'a dyn Render,
    /// The instant every write is stamped with.
    pub clock: &'a dyn Clock,
    /// The model it asks.
    pub model: &'a dyn Model,
    /// Where each exchange is written down.
    pub conversations: crate::secretary::conversations::Conversations<'a>,
    /// Where a proposal is filed for a person to tap.
    pub inbox: &'a dyn crate::store::Prodrome<crate::secretary::inbox_event::InboxEvent>,
    /// Its memory, shared by every conversation; the agent has none, so an injection
    /// cannot become durable.
    pub memories: crate::secretary::memories::Memories<'a>,
    /// The recorded decisions: a verb reads them; only the owner's tap on a proposal writes.
    pub decisions: crate::decisions::Decisions<'a>,
    /// The topics on its rounds and notes, box-local: `add_topics` writes the round it answers.
    pub topics: crate::secretary::topics::stores::TopicStores<'a>,
    /// The saved briefings.
    pub briefings: &'a dyn Briefings,
    /// The day's forecast.
    pub weather: &'a dyn Weather,
    /// The owner's calendars, read-only, in the box's clock: what a plugin learned, whose
    /// titles are other people's words. Meetings are never todos.
    pub calendar: &'a dyn Calendar,
    /// Whether each declared secret resolves, never a value.
    pub secrets: &'a dyn Secrets,
    /// The web, within the allow rules.
    pub web: &'a dyn Web,
    /// The owner's repositories, read-only.
    pub repositories: &'a dyn Repositories,
    /// The owner's Drive, within the allowed folders.
    pub drive: &'a dyn Drive,
    /// The Microsoft accounts' Junk folders, read-only.
    pub junk: &'a dyn Junk,
    /// The timer's own mail sync, started early; it reaches nothing the timer does not.
    pub sync: &'a dyn MailSync,
    /// Every source's health: each mail account's syncs, each plugin's runs and links, each
    /// Google account's calendars and the Drive's probes.
    pub health: &'a dyn crate::health::Register<crate::source_key::SourceKey>,
    /// The mail servers, searched for mail older than the routine sync's window.
    pub older: &'a dyn OlderMail,
    /// The owner's uploaded photos and PDFs, read-only.
    pub uploads: &'a dyn crate::store::Gets<crate::attachment::AttachmentId, crate::upload::Upload>,
    /// The words of the uploads, for its search.
    pub upload_index: &'a dyn super::UploadIndex,
    /// The files a verb keeps for the model.
    pub kept: &'a dyn crate::store::Doc<crate::attachment::AttachmentId, Vec<u8>>,
    /// A PDF's text layer, which a held file's numbers are read back from.
    pub pdf: &'a dyn super::PdfText,
    /// The files the owner tapped Read on outside the allowed folders, as each tap read it.
    pub tapped: &'a dyn crate::store::Gets<crate::held_value::HeldFile, crate::secretary::attached::Attached>,
    /// The tables the instance file declares, read-only: what a Connect card is checked against.
    pub configuration: &'a dyn crate::settings::Configuration,
    /// The watches on mail the owner expects; a watch's only act is the sync's push.
    pub watches: &'a crate::mail::watch::Watches,
}

impl<'a> Secretary<'a> {
    /// The agent's share of these effects.
    #[must_use]
    pub fn agent(&self) -> Agent<'a> {
        Agent {
            chain: self.chain,
            corpus: self.corpus,
            mail: self.mail,
            repository: self.repository,
            manuals: self.manuals,
            render: self.render,
            clock: self.clock,
            memory: crate::secretary::recollection::Recollection::of(
                self.memories.reads(),
                crate::secretary::digest_view::VIEW_BYTES,
            ),
        }
    }
}
