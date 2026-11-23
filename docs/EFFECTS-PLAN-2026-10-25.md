# Effects: a pure store, its inputs and its outputs (2026-10-25)

Status: open under roadmap task `localwrites`, whose stage 4 performs a tapped
output through §9.3's sink and appends §9.5's confirmation
(`docs/LOCALWRITES-PLAN-2026-11-01.md` §4.5, §5.2). The input stages (2, 4, 5,
the calendar half of 6, and 7) are not built and no open roadmap task asks for
them; their design is this file's text at `eea3216`.

## 0. The shape

| Part | What it is | Its law |
|---|---|---|
| **the store** | prodromes and the box's other stores, and the readings, which are folds over them | a reading is a function of the set of objects it reads |
| **the agent** | the secretary, the triage agent and the box's rules: they read readings and propose | it holds no `Tap`, so it performs no output |
| **inputs** | sources, one port (`Source`): each pull adds objects or messages to the store | monotone: what is learned is added, so a pull needs no lock |
| **outputs** | sinks, one port (`Sink`): each performs one tapped proposal on the world | non-monotone, so it is performed once, under the inbox's lock, after a tap |

The agent asking for a pull (`sync_mail`, `search_older`, an allowed read)
performs nothing on the world: a pull only adds to the store. The secretary's
direct chain write as `assistant` is a write the store's trust policy admits,
not an output.

## 6. Stages

- Stage 1 (one health register): landed in 2.1.31
  (`suzatary_health::health::SourceKey`, `suzatary_io_local::source_health`,
  `Connection::sources` in `app/connect/src/connect/connection.rs`).
  `Endpoint`, the sum of `In(SourceKey)` and `Out(SinkKey)` with one print,
  waits for stage 3, which adds the first `SinkKey`.
- Stage 2 (one source port and one runner): not built; no open roadmap task.
- Stage 3 (the split as a type, one sink port and one performer): open, below.
- Stage 4 (one Settings line, one card rule, one failing todo): not built; no
  open roadmap task. The rule it would generalise is `failuresaretodos`.
- Stage 5 (the writer, `Query::mail`, uploads as messages): not built; no open
  roadmap task.
- Stage 6 (calendars as messages, and the loop closed): the calendar half is not
  built and no open roadmap task asks for it. The confirmer half is open, below.
- Stage 7 (Drive as a source): deferred by the owner (2026-10-25).

### Stage 3 (outputs): the split as a type, one sink port and one performer

- **General piece:** `Proposal = Write | Output` (§9.2); one `Sink` port and
  a table of native sinks (§9.3); one performer, `inbox::submit` calling
  the store's writer for a `Write` and the sink for an `Output`.
  `Unperformed` is the one failure (`NotSent` or `MayHaveGone`, each with
  its `Failure` class), and every sink's outcome joins the stage-1 register
  under `Out(SinkKey)`.
- **The stored literals do not move.** Each act prints with the
  constructor it prints with today, so no inbox object changes and no
  migration runs. `app/inbox/tests/act_literal.rs` reads every arm back from
  its print, and is the pin.
- **Removes:** the act performer's per-act arms, each repeating a
  not-in-this-build, a real and a recording case (`Acting` in
  `app/cli/src/commands/secretary.rs`): a build without a sink has no table
  entry, and the fixture's recording is a table of its own. The output arms of
  `performed` (`app/server/src/secretary/routes.rs`) become one
  `Prepared::Output` arm.
- **The phone:** no change. Every card reads and taps as it does now.

### Stage 6, the confirmer half: `Confirmed` and the loop closed

- **General piece:** the inbox's `Confirmed` response and the one confirmer
  that runs after each pull (§9.5). For mail it needs no input stage: the
  message the box minted is found by `id:<mid>` in the account's Sent.
- **Migration:** `Confirmed` is a new inbox constructor, an addition the
  schema's laws cover (`app/secretary/tests/inbox_schema.rs`); no stored
  object changes.
- **The phone:** a sent message whose copy reaches Sent, or a calendar event
  that comes back from the calendar, reads "Confirmed" on its card, and a send
  that may have gone stops asking the owner to check Sent once its copy is
  found.

## 9. Outputs

### 9.1 Two kinds of tapped proposal

Every tapped proposal (`Proposal` in `app/inbox/src/secretary/proposal.rs`,
`changes::Act`, `acts::Act` in `app/inbox/src/secretary/acts.rs`) is one of two
kinds:

| Kind | Proposals | Where its effect lands |
|---|---|---|
| **write to a store** | the changeset's acts (`Add` .. `Reprice`) | a todo chain (`changes::events`) |
| | `Remember`, `Decide`, `Erase`, `Retract`, `Afresh`, `Noise`, `Restore` | the memory, the decisions, the conversations, the triage queue, the inbox |
| | `Device` | the signatures' history (`KeyAdded`) |
| | `Connect`, `Disconnect`, `Grant` | the instance file, by compare-and-swap under its `flock` |
| | `Secret`, `GoogleClient` | the secret store (the value travels in the tap's body alone) |
| **output to the world** | `Email`, `Forward` | a mail server |
| | `Patch`, `CodingSession` | a repository, the cloud routine |
| | `CalendarEvent` | a Google calendar |
| | `Web`, `DriveRead` | a web server, the Drive: a read whose request itself leaves the box, so it waits for a tap |
| | `Auth`, `AuthSend`, `GoogleSignIn`, `SignIn`, `SignInCode`, `UnlockStore` | an identity provider or the secret store's agent: a consent or an exchange whose result the box then keeps |

The instance file is a register with compare-and-swap, so its edits are
writes to a store. A write is performed by its store's writer; an output by a
sink. An arm added later is classified by the same rule.

### 9.2 The split as a type

```rust
pub enum Proposal { Write(Write), Output(Output) }

pub enum Write {
    Chain(Changeset),            // Add .. Reprice
    Remember { .. }, Decide(Draft), Erase { .. }, // and the other memory and queue acts
    Device(Enrolment),
    Instance(InstanceEdit),      // Connect, Disconnect, Grant
    Secret(SecretWrite),         // Secret, GoogleClient
}

pub enum Output {
    Mail(Mail),                  // Email, Forward
    Patch { .. }, CodingSession { .. },
    CalendarEvent { .. },
    Read(Read),                  // Web, DriveRead
    SignIn(SignIn),              // Auth, AuthSend, GoogleSignIn, SignIn, SignInCode, UnlockStore
}
```

Each arm prints with the constructor it prints with today, so the inbox's
stored objects do not move and no migration runs. A `Write` is performed by
its store's writer under that store's hold, as a changeset is now. An
`Output` is performed by its sink.

### 9.3 One sink port

```rust
/// A sink: something outside the box that a tapped output changes.
pub trait Sink {
    fn perform(&self, output: &Output, consent: Consent) -> Result<Receipt, Unperformed>;
}

/// What a sink said it did, and the input that will show it.
pub struct Receipt { pub confirms: Confirms, pub said: String }

/// Why it did not, by what its transport knew.
pub enum Unperformed {
    NotSent { class: Failure, said: String },
    MayHaveGone { class: Failure, said: String },
}
```

- `Consent` is today's, minted only by `Consent::given_by_tap` from a `Tap`
  (`app/mail/src/mail/oauth.rs`; `Tap::of` in
  `app/signatures/src/signatures/tap.rs` is its only constructor), so the
  agent, which holds no `Tap`, cannot perform an output: the same proof as
  today, moved to one port.
- **Native sinks**, trusted: the mail transport, the Google calendar insert,
  the publisher of a patch, the coding routine, the web fetcher, the Drive
  (read and keep), and the sign-in flows. Each is one table entry. A build
  without one has no entry, so "not performed by this build" is said once.
- **The plugin actor world is not a sink.** An actor returns proposals and
  never performs, so it is part of the agent. A sandboxed sink would be a
  world of its own: one `perform(output) -> receipt` export, importing a fetch
  that may write only to its manifest's origins, called only by the performer
  after a tap. Nothing needs one now, and the plugin fetch is GET-only.
- **What is shared with the sources, and nothing more:** the `Endpoint` key
  (a mail account reads and sends, a Google account reads and writes its
  calendar, the Drive reads and keeps), the one health register, the failing
  rule (§9.6) and the Settings line. **What is not shared:** the cursor, the
  runner and `Deliver` belong to inputs; consent, at-most-once and
  confirmation belong to outputs. There is no `Effect` port with both `pull`
  and `perform`. An output has no ledger of its own: the inbox's state
  register records each try and its settlement per proposal, and a sink's
  health is the join of those outcomes.

### 9.4 Once, by the object's name

The output is the inbox's `Asked`, named by its content (`ProposalId`,
`app/inbox/src/secretary/inbox.rs`). `inbox::submit` asks the inbox's one
non-monotone question under `submit.lock` ("does it still stand, so I may
perform?"), performs only while nothing settled the proposal, and otherwise
returns the recorded response. A try that sent nothing records `NotSent`, so a
resubmit runs again. `app/secretary/tests/inbox.rs` is the law: any
interleaving of submits, resubmits, dismissals and acknowledgements performs
each effect at most once. The sink port changes none of this; it is the one
call the performer makes inside that rule.

### 9.5 Closing the loop: an output is confirmed by an input

Today the owner closes the loop by hand: a send that may have gone is marked
"check Sent" until he taps acknowledge, which appends `Checked`. A sink's
"sent" is the sink's claim, not an observation. Every `Receipt` names the
input that will show the effect:

| Output | `Confirms` | How it arrives |
|---|---|---|
| `Email`, `Forward` | the message with the Message-ID the box minted (`commands::mail::deliver`) in that account's Sent | the mail source's next pull; `id:<mid>` in notmuch |
| `CalendarEvent` | the event, by the id Google answered, in that account's calendar | the Google calendar source |
| `CodingSession` | its report on a `claude/` branch naming its URL | the coding poll, which already works this way (`app/cli/src/commands/coding.rs`) |
| `Patch` | its branch on the repository's mirror | a listing of that mirror's branches, which the coding poll already reads for `claude/` |
| `Auth`, `AuthSend`, `GoogleSignIn`, `SignIn`, `SignInCode` | the grant or token in its store, and the next success of the endpoint it serves | stage 1's health register |
| `Web`, `DriveRead`, `UnlockStore` | its own answer | the answer itself, recorded as today |

The inbox gains one response, `Confirmed { proposal, by }`, whose `by` is a
reference in its one text form (`mail:<hash>`, `proposal:…`). In the phase
order it sits above `Sent`, `MayHaveGone` and `Checked`, since all three say
"it went" and `Confirmed` is the observation that it did. It is incomparable
with `Dismissed`, so a send that the owner dismissed and that turns up in Sent
is a conflict, shown and not tiebroken. The confirmer is a reading: from the
inbox and the mail index, the set of `Confirmed` responses owed. It runs after
each pull and only appends, so it takes no lock. A may-have-gone send whose
copy arrives stops asking the owner; one whose copy never arrives still asks,
and `Checked` stays for it.

Denotation: an output means an observation, not a sink's reply:
`confirms(o) : S → Bool`, monotone (once confirmed, it stays confirmed).
Without the confirmer, `Sent` means "the sink said so".

### 9.6 Failing outputs

A sink's `Unperformed` carries a `Failure` class, parsed where the failure is
observed, as a source's `Unpulled` does (`app/mail/src/mail/puller.rs`). Its
outcome joins the stage-1 register under `Out(SinkKey)`. The rule is
`failuresaretodos`: an endpoint failing for a day proposes one todo on the
upkeep chain, once per episode, priced by a deadline. A card is filed only for
what the owner alone can do: a lapsed send consent is the `AuthSend` card,
which the todo names in `waiting_on`. A sending endpoint is priced as its
account's mail is (45% falling to 35% over 7 days): mail that cannot go out is
a real consequence within weeks.
