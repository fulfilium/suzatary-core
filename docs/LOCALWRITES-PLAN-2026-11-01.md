# Local writes: every page write is an object (2026-11-01)

The owner, 2026-11-01: "the whole point of the crdt is eventual consistency,
meaning we don't need to wait for anything, as long as applying an effect has
guaranteed idempotence we're good no? Even the confirmation that something got
sent for instance can be a state update and that's how we can represent that
something was done, and all of it can be parallelized."

This is the design pass for the todo `localwrites`. Nothing is built except
the measurement in §1.2 (`web/e2e/writewait.bench.ts`). The stages in §7
complete the todo; this document does not.

**The target.** Every page write (say a message, rename or tag a topic, tap a
card, answer a setup step, trust a device) is a signed, content-addressed
object. The tab appends it to its own replica and draws it at once. The tab
sends it to the box by the same object sync that the reads use. The box's
acceptance, its refusal, and each effect's completion come back as objects in
a store the tab already syncs. Each effect runs at most once per request at
its sink. Independent effects run in parallel. No page write waits on a round
trip.

Measured on master at `c83b24d` (2.1.43). The plan was then read against
2.1.44 (`f8a3988`), which added the operator's `call` (§5.3). Its changes to the write
routes are in the gate, the guards and the seat a stamp carries; the work the
timings measure (the appends and the reads) did not change. 2.1.45 (`388ece4`)
added one write route, the setup conversation's, which §1.1 and §3 include. Paths are relative to the repository root.

## 0. The shape in five rules

1. **A write is an object.** The tab seals it (a Prodrome `Change` signed by
   the device key) into the row of the store it changes, and draws the reading
   of that row at once. The object's name, the SHA-256 of its literal, is the
   REQUEST's name. A retry posts the same bytes, so it is the same request.
2. **Acceptance is the box holding the object.** The box takes a posted object
   under that store's admission rules (`sync::receive` and its per-schema
   rules, `Trust::admits`). A store's next pull answer names it in its
   frontier, and that is the acceptance. No second object says so.
3. **A refusal is an object.** The box writes `Refused { request, reason,
   kind }` into the ASKS store (§2.4), signed by the box, which every device
   of the actor syncs. The tab drops the refused object, and everything that
   rests on it, from every reading, so the drawing returns to the box's state.
   The refused object stays in the row and is named, as today (§2.5).
4. **An effect is a reading of what is owed, and its completion is an
   object.** A sink (the conversation worker, the inbox's performer, the
   device registry) reads the requests that no outcome answers yet. It claims
   one by appending a claim object, performs it, and appends the outcome
   object. A claim holds a lock only for the time of one append. Besides the
   claims, only the pairing code and the instance file need a lock (§4).
5. **The page never awaits a write.** A page posts nothing itself. The
   exchange that already pushes the replica's unsent objects before each pull
   (`web/src/prodrome/sync.ts:504-547`) posts them. A page draws pending,
   accepted, refused and done as readings of its replicas (§2.5).

## 1. The write routes today

### 1.1 Inventory

Every POST of the route table (`app/cli/src/commands/serve/table.rs`, 25
posts on 2.1.45). "Waits" says whether the page draws the result only after the box's
answer. The server facts are from the handlers; the page facts are from the
callers in `web/src/`.

| Route | What it writes | Effect, and when the answer comes | Page waits? | What the owner sees meanwhile |
|---|---|---|---|---|
| `POST /api/secretary/conversations/{c}/say` (`secretary::say`, `app/core/src/secretary/routes.rs:454`) | lines: `Entry::Said { turn, by: Stamp }`, authored, sealed and stamped by the box | nudges the conversation worker (`conversation_worker.rs:237-247`); answers 202 before the round runs | yes: `chat/source.ts:303-306` draws the question only after the answer, a pull of every store and a settle (`intentsync.ts:167`) | the draft empties (`composing.ts:134`); nothing else until the line is pulled |
| `…/edit`, `…/regenerate` (`secretary/mod.rs:752`, `:736`) | lines: a `Said` that sets later rounds aside; or a new round | as say | yes, as say | nothing |
| `…/stop` (`conversation_worker.rs:266`) | lines: the halt's note | waits up to `STOP_WAIT` for the round to end | yes | nothing; a toast on failure |
| `POST /api/secretary/conversations` (`secretary::open`, `routes.rs:274`) | lines: `Opened` under an id the box mints | none | yes, then a say | nothing |
| `POST /api/secretary/setup/{template}` (`secretary::setup`, `routes.rs:312`, 2.1.45) | lines: `Opened` under an id the box mints, titled for the template, and the box's note with the template's opening | none | yes (`pages/settings.ts:94`): Settings navigates to the conversation after the answer | Settings, unchanged |
| `…/title` (`secretary/title.rs:123`) | lines: `Titled` | `heard` | yes (`pages/conversation.ts:97`); the field stays open | the old title |
| `…/erase` (`routes.rs:341`) | lines: `Erased`; memory sweep; upload reindex | none | yes (`chat/erase.ts:55`) | a placeholder |
| `…/proposals/{n}/tap` (`proposals.rs:128` → `routes.rs:952`) | inbox: `Responded` after the effect, under `submit.lock` | the effect itself (mail, chain append, secret, instance edit, Google, web fetch, coding session) runs before the answer | yes (`inbox/proposal.ts:158`) | a placeholder in place of the buttons |
| `…/dismiss`, `…/acknowledge`, `…/drop` | inbox `Responded::Dismissed` / `Checked`; lines (`coding::dropped`) | `heard` (dismiss, drop) | yes | a placeholder |
| `POST /api/stores/{topics,topic-aliases}/objects` (`stores::take`) | the topic stores: the tab's own `Change` objects, unsigned, taken as the request's proven actor | none | half: kept and drawn first (`storeset.ts:209`), but the sheet closes only after the post's answer (`pages/secretary.ts:93,114,142`) | the topic already in the bar; the sheet open |
| `POST /api/{chain}/objects` (`chain::take_objects`) | a chain: the tab's sealed, signed acts | lifecycle notes become act lines and `heard` | no: drawn first (`sync.ts:414`), "sending" on the row (`todo/row.ts:187-190`) | the act, marked sending |
| `POST /api/{chain}/author` (`chain::author`) | nothing; returns the event's print for the tab to seal | none | yes (`todo/addsheet.ts:125`, `editform.ts:40,94,136`) | the form, open |
| `POST /todo/{id}/build` (`routes.rs:2096`) | lines and inbox: a coding-session card | none until tapped | yes (`todo/todoview.ts:336`) | a disabled button |
| `POST /api/secretary/memory/{id}/forget` (`routes.rs:2230`) | memory: `Forgotten` | none | yes (`chat/memory.ts:27`) | nothing |
| `POST /api/secretary/record/{node}/retract` (`routes.rs:2267`) | digests: `Retracted` | none | yes (`chat/memory.ts:23`) | nothing |
| `POST /api/mail/watches/{id}/cancel` (`mail/watch.rs:430`) | watches log: `Cancelled` | none | yes (`app/pages/src/mail.rs`, `Msg::Cancel`) | a disabled button |
| `POST /api/secretary/attachments` (`secretary::upload`) | uploads store, or the Drive vault when private | a Drive write when private | yes (`chat/composer.ts:148,206`) | a pending thumbnail; Send refuses meanwhile |
| `POST /api/act/device` (`core/devices.rs:69`) | push tokens: `Push(Device)` | none | no: background (`app/pushtoken.ts:53-59`) | nothing |
| `POST /api/act/key` (`routes.rs:1468`) | lines and inbox: a `Device` card (at most 3 waiting) | none until tapped | yes (`pages/settings.ts:103`) | Settings, redrawn after |
| `POST /api/act/pair` (`the_box.rs:186`) | the pairing file, then `KeyAdded` on every chain | none | yes (`app/pairing.ts:43-44`) | a disabled Trust button |
| `POST /api/held/reveal` (`held.rs:76`) | nothing (a read on a tap, recorded) | reads mail or a file | yes | a placeholder |
| `POST /api/client-errors` | one log line | none | no | nothing |

"Answer a setup step" has no route of its own. The box files the setup cards
(Secret, Connect, SignIn, GoogleClient, UnlockStore) in the `setup`
conversation (`app/core/src/secretary/noticed.rs:30-32,160`), and the owner
answers each one through the tap route.

The Android app is not a client of any write route. Its Java code reads `GET
/api/portal` (`android/java/com/bmabsout/suzatary/Pairing.java:38`) and `GET
api/fold` (`Widget.java:89,386`). It hands the push token to the page
(`MainActivity.java:253-254`), and the page posts it. So the APK keeps working
through every stage, and only the page's bundle changes.

Two mechanisms already exist and this plan generalises them:

- **The chains are already local-first.** A todo act is sealed, signed and
  drawn before the exchange posts it (`sync.ts:388-418`). A refused object
  stays in the row, named, and leaves every reading with everything resting on
  it (`replica.ts:437-465`, the `phonereplica` decision).
- **The page already tracks each ask** in an in-memory grow-only set
  (`prodrome/intents.ts`, `intentsync.ts`, `asks.ts`, 314 lines): Pending,
  Taken, Failed, Absent. It lives in memory, so a reload forgets it, and its
  answers come from HTTP replies rather than from stores.

### 1.2 What the box makes the page wait, measured

`web/e2e/writewait.bench.ts` signs each request as the page does, with a
device key it trusts at the terminal, and times the box's answer. It ran
against the lines' server of `scripts/web-e2e.sh` (the `bigstore` shape:
13,000 generated lines over 100 conversations with 1,500 reseals, about
14,500 line objects; 400 proposals, about 730 inbox objects; 900 topic
objects; 600 todos). The server was the `suzatary-dev` binary in a release
build, in a 4-core container, with a scripted model. It ran twice, each time
on a fixture built afresh, and each write ran 7 times a run. The table gives
the median and the slowest of each run, in milliseconds.

| Write | Run 1: median, slowest | Run 2: median, slowest | Note |
|---|---|---|---|
| say (the 202, before the round runs) | 791, 917 | 928, 1,028 | the line's append rereads the lines store |
| rename a conversation | 800, 1,987 | 871, 1,957 | |
| open a conversation | 830, 878 | 876, 1,100 | |
| tap an email card (the fixture records the mail) | 379, 391 | 457, 595 | |
| dismiss a card | 1,855, 2,497 | 2,456, 3,155 | reads the 400-card conversation twice ("opened" phases in the server's timings log) and wakes a round |
| author a todo (`/api/life/author`) | 57, 64 | 57, 65 | then the page seals and posts |
| rename a topic (`topic-aliases` objects) | 13, 16 | 14, 16 | |
| push token | 13, 14 | 9, 11 | |
| ask to trust a key | (refused) | 434, 1,168 | at most 3 cards wait, so run 1 was answered 429 after the debug run's 3 asks, and run 2 took 3 asks and refused 4 |
| a lines skeleton pull with nothing new | 525, 647 | 596, 694 | what each "then the page pulls" adds |
| the inbox's whole pull | 15, 25 | 17, 27 | |

So the say, the title and the open each make the page wait 0.8 to 0.9 s for
the answer, then at least 0.5 s for the lines pull before the line is drawn.
A dismissal waits 1.9 to 2.5 s. A tap on a card whose effect is slow (a 10 s
web fetch, a coding session) waits for the whole effect, and holds the
inbox's one `submit.lock` for that time, so a second tap on another card
waits behind it. The debug build of the same binary, which the e2e suite
runs, measured about 1.2 s for say, title and open, and 10.8 s for a
dismissal. The page's own time after the answer (the settle and the draw)
is not in these numbers.

## 2. The objects

### 2.1 Three kinds of write

| Kind | Writes | The object goes to | Needs a sink? |
|---|---|---|---|
| **store write** | rename or tag a topic, a todo act, a conversation's title, an erasure | the store whose reading it changes | no: acceptance is the whole write |
| **request to an effect** | say, edit, regenerate, stop; a tap, dismiss, acknowledge or drop of a card; build a todo's coding card | the store whose reading shows the effect (lines for a round, the inbox for a card) | yes: the conversation worker, the inbox's performer |
| **ask of a box-held store** | forget a fact, retract a summary, cancel a watch, register a push token | the asks store (§2.4), since the tab does not hold the target store | yes: the box applies it to its own store |

A pairing code and a request to trust a new key cannot be any of these,
because the device that makes them is not trusted yet (§4.1). Revealing a held value is a read, and
stays one (§5.6).

### 2.2 What a hash covers

Each object is a Prodrome `Change(genesis, deps, event)` inside `Signed`, so
its name covers:

- the store (its `genesis`), and the heads of the register it writes (`deps`):
  a write that rests on a different state is a different request;
- the event: its kind and fields, the writer's `at` and the `actor`;
- the device key and its signature, which `Signed` carries.

Two cases must be the same request, and they are:

- **A retry.** The tab seals once, into the row, in one IndexedDB transaction
  (`objects.holdReplica`). Every later post sends the row's bytes, so the box
  answers `already` (`ReceiptPayload.already`). A post whose answer was lost
  is posted again by the next exchange and costs nothing.
- **A second tab of the same device.** Both tabs read one row, so both post
  the same bytes. A tab never seals an act that another tab sealed: the act is
  sealed by the tap that made it, in the tab that was tapped.

`deps` also replaces the say route's `after` field ("how many turns the asker
saw", `wire.rs:1044-1047`). A said line rests on the conversation's heads in
the tab's replica, so a question written before the tab saw an answer is
concurrent with that answer, and the transcript's causal order places it.

An object whose store the box no longer serves (a migration moved its
genesis) is never accepted. It stays in the row, named as stranded, and is
counted in what a reset loses, as an act in a dropped store is today (WEB-SPEC
§3.18).

Two devices tapping the same card make two requests, with two names. For an
effect, the request's name is not the effect's identity. The **effect key** is
what the sink performs at most once, and the request names it:

| Request | Effect key |
|---|---|
| a tap, dismiss, acknowledge or drop of a card | the proposal's `ProposalId` (the content name of the filed proposal, `inbox.rs:268-286`) |
| a said line (say, edit) | the said line's own name |
| regenerate | the round it asks again, and the request's own name (each regenerate is a new ask) |
| stop | the round it stops |
| forget, retract, cancel | the fact, the node, the watch |
| push token | the device key and the platform: a register whose latest value wins by causality |
| key ask | the key |

### 2.3 The tab seals lines

A said line and a title are words, and the lines store seals every slot's
words under a key of its own (`secretary::locked_text`, ChaCha20-Poly1305,
one key per text), after the redaction door (`line_event.rs:392`, `seal`).
The seal is a function of the core, so the tab runs the same function in the
core's worker (`suzatary-wasm`), with the browser's entropy as the `Entropy`
port. The tab posts the slot's key beside the object, as a pull answer
carries keys beside the nodes today (`routes::words`). Then:

- **Erasure is unchanged.** The box keeps the key in the lines' key store, and
  an erasure deletes it there and in every device's row (§4.3).
- **The scrub is checked at acceptance.** The admission rule opens the slot
  with the posted key, scrubs the words under the current rules, and refuses
  the line if the scrub changes them or the slot claims other rules. A tab of
  an older bundle seals under older rules. Its line is refused with a new
  `RefusalKind`, `Outdated`: the tab keeps the line, offers Reload as it does
  for `CLIENT_MIN`, and the new bundle seals the words again.
  So no unscrubbed word reaches the store, and the box stays the one door.
- **The stamp is checked at acceptance.** A said line carries its stamp
  (`Stamp::Actor(Speaker { actor, seat })`, `app/core/src/seat.rs`). The rule computes the
  stamp from the signer: the actor the history trusts the key for, and the
  seat (`Device` for a device key, `Operator` for the root key). A mismatch is
  a final refusal. So a tab cannot claim a seat or another actor.

### 2.4 The asks store

One new Prodrome store, `secretary-asks/`, at a schema of its own:

```rust
/// What a device asked of a store the box alone holds, or what the box said of a request.
pub enum AskEvent {
    Asked { ask: Ask, at: Datetime, actor: Actor },
    Refused { request: Hash, store: StoreName, reason: String, kind: RefusalKind, at: Datetime },
    Answered { ask: Hash, outcome: Outcome, at: Datetime },
}

pub enum Ask {
    Forget { fact: FactId },
    Retract { node: NodeId },
    CancelWatch { watch: WatchId },
    Push { platform: Platform, token: String },
}
```

- The register of an ask is its outcome, inflationary as the inbox's phase is
  (`app/core/src/secretary/phase.rs`): `Asked < Answered`, and `Refused`
  incomparable with `Answered`. Two outcomes for one ask are a conflict, shown
  whole.
- `Refused` names a request of ANY store, by its name and its store, so it is
  the one home of every refusal (rule 3). The box writes one only for a
  request signed by a key it trusts for an actor in `web.act_as`, so an
  untrusted poster cannot fill the store. An unsigned or untrusted post is
  refused in the HTTP answer alone, as today (`RefusalKind::Trust`), and the
  tab keeps it in its row.
- The box writes `Refused` and `Answered`; the tab writes `Asked`. The store's
  admission rule enforces this split, as the chains' `BoxOnly` rule does for
  `KeyAdded` (`sync.rs:221-314`).
- The tab syncs it whole. It holds a few objects per ask and per refusal, so
  it stays small; it is not a lazysync store.

### 2.5 Pending, accepted, refused, done

Every drawing is a reading of the tab's replicas, which are joined with every
other tab's through the device's rows. No page holds an answer of its own.

| State | Reading | Drawn as |
|---|---|---|
| pending | an own object the box's word does not hold yet (`replica.ts`, `toPush`) | the effect, marked "sending" (as a todo row is today, `todo/row.ts:187-190`); after 5 s the shell's outbox line (`shell/outbox.ts:15`) |
| accepted | the box's word holds it | the effect, unmarked; for a request to an effect, "queued" |
| working | a claim object answers it (§4) | "working", with the claim's `at` |
| done | an outcome object answers it | the outcome: the answer line, "Sent", "Applied", "Dismissed" |
| refused | a `Refused` names it, or the post's answer refused it | the reading WITHOUT it and everything resting on it; the refusal named in the shell's line and on the card |

**The rollback law.** For every replica `R` and every set `F` of its own
objects that the box refuses, the reading of `R` with `F` refused equals the
reading of `R` minus `F` and minus every object resting on an object of `F`.
`replica.standing` already reads this way for chains and topics. The stages
pin it once, generically, over generated stores (`web/test/gen.ts`) for every
store the tab writes.

The intents set (`intents.ts`, `intentsync.ts`, `asks.ts`, 314 lines) becomes
this reading. Its four states are the table's, and its answers come from the
stores, so a reload, a second tab and a second device all draw the same
state.

## 3. Each write

| Write | Object | Store | Receipt | Pending drawing | Refused drawing |
|---|---|---|---|---|---|
| tag or untag a topic | `Added` / `Removed` (`MembershipEvent`) | `secretary-topics` | acceptance | the chip in the bar at once | the chip goes; the shell names the refusal |
| rename a topic | `Aliased` / `Unaliased` (`AliasEvent`) | `secretary-topic-aliases` | acceptance | the new name at once | the old name again |
| a todo act (done, cancel, reopen, tend, add, update, confirm) | the chain's events, as today | the chain | acceptance | as today | as today |
| rename a conversation | a `Titled` line, sealed in the tab | lines | acceptance | the new title | the old title |
| open a conversation | none of its own: the first `Said` of a new conversation id, which the tab mints as the first 32 hex of the said line's name | lines | acceptance | the new conversation with its question | the conversation is gone; the draft returns to the composer |
| begin a setup conversation from Settings | a `SetUp { template }` line, the first line of a new conversation id the tab mints as above | lines | acceptance, then the box's opening note, which the box writes as a reading of a `SetUp` line with no note after it | the conversation, titled for the template, at once | the conversation is gone; Settings again |
| say, edit | a `Said` line, sealed in the tab | lines | acceptance, then `Began` (the claim, §4.4), then the answer lines | the question at once; "queued", then "working" | the question goes; the words return to the composer |
| regenerate, stop | a `Regenerate { round }` / `Stop { round }` line | lines | the worker's next line | "asked again" / "stopping" | the round as the box holds it |
| tap a card | `Tapped { answer, endorsement, sealed_value }` on the proposal | inbox | `Trying` (the claim), then `Sent`, `Applied`, `Acted`, `MayHaveGone` or `NotSent` | the card reads "sending" or "applying" | the card stands again, with the refusal |
| dismiss, acknowledge, drop | `Dismissed { reason }` / `Checked` / `Dropped` on the proposal | inbox (drop: lines) | acceptance | the card reads the answer | the card stands again |
| erase | an `Erased { keys }` line | lines | acceptance; the box deletes the keys | the turns drawn as erased at once; the device deletes the keys once the box holds the line | the turns again, since the device kept the keys |
| forget, retract, cancel a watch | `Asked { ask }` | asks | `Answered` | the fact, node or watch marked "forgetting", "retracting", "cancelling" | unmarked |
| push token | `Asked { Push }` | asks | `Answered` | nothing (background) | named in the shell |
| ask to trust this device | none: `{ key }` to the untrusted device's route (§4.1) | a `Device` card in the owner's inbox | the card, then `KeyAdded` | "asked; tap the card on a trusted device" | named |
| build a todo's coding card | a `Build { todo }` line in the `build` conversation | lines | the card, filed by the box | "filing the card" | named |
| upload a file | its bytes, `PUT /api/uploads/<sha256>` (§5.7) | uploads | the store holds the hash | the thumbnail from the device's own bytes | the thumbnail marked, Send refuses |
| answer a setup step | a tap, as above; a secret's value is sealed to the card's key (§4.6) | inbox | as a tap | as a tap | as a tap |

## 4. The effects that need coordination

Most of the system is monotone: a pull, an acceptance, a fold, an erasure, a
revocation only add to a set. Monotone work needs no coordination (CALM), and
the page never waits for it. The table names every non-monotone question that
remains, where its one decision is made, and why the page still does not wait.

Every claim below is a box-local hold, a file lock on the fetcher. That is
correct only because exactly one box performs effects: `suzatary up` refuses
on any host but the one `<data>/FETCHER` names, and a replica box
(`suzatary replica`) syncs objects and performs nothing. If two boxes ever
performed, the claim would need a consensus, and this plan does not provide
one.

| Effect | The non-monotone question | The one decision | Why the page does not wait |
|---|---|---|---|
| pairing code (§4.1) | is this code unused, unexpired, within 5 tries? | the pairing file under its lock (`app/io/src/pairing.rs`, `signatures::Pairing`) | the page draws "trusting", and switches when its key's `KeyAdded` reaches a chain it pulls |
| key revocation (§4.2) | is this key trusted now? | the terminal (`suzatary device revoke`), applied at each store's acceptance | the revoked device is answered with the wipe (410); no page of another device waits |
| erasure (§4.3) | none: an erasure is a tombstone | none needed | the device deletes its keys at once |
| a model call (§4.4) | has this said line's round begun, and is the run of its first attempt gone? | the conversation worker's hold, by appending `Began { said, attempt, run }` | the question is drawn at once; the answer streams in |
| a coding session, a patch, a calendar event, a web or Drive read, a sign-in (§4.5) | does the card still stand? | the inbox's per-proposal claim (`Trying`), as `inbox::submit` | the card reads "working" from its claim, then its outcome |
| sending mail (§4.5) | as above, and: did the owner sign this, on a device? | the inbox's admission rule (signer), then the per-proposal claim | as above |
| a secret's value (§4.6) | none beyond the tap's | the per-proposal claim | as a tap |
| an instance edit (Connect, Grant, Configure) | does the file still say what the card read? | the instance file's compare-and-swap under its `flock` (`settings/configure.rs:368`) | as a tap |

### 4.1 Pairing codes

A device with no trusted key cannot write a store: every store refuses its
objects with `RefusalKind::Trust`, and the tab keeps them and posts them again
once it is trusted (`wire.rs:1259`). That is already local-first. What the
device needs is trust, and the pairing code is the one way to get it without
another device.

The code is a one-time secret: ten minutes, one use, five tries, kept only as
a keyed hash (`signatures::Pairing`). Its check is not monotone, so it has one
decision point, the pairing file under its lock. The code must never enter a
synced store, and the untrusted device cannot write one. So pairing keeps one
request route for an untrusted device, `POST /api/devices/ask`, which takes
`{ code }` (today `/api/act/pair`) or `{ key }` (today `/api/act/key`, which
files a `Device` card for a trusted device to tap). Both prove only that the
asker holds its key (the `Asker` witness). It is the one write route left
after the last stage, and the page still does not wait on it:

- the page seals nothing; it posts `{ code }` signed by its key and draws
  "trusting this device";
- the answer, if it arrives, refuses a wrong code at once ("4 tries left");
- the decision's effect is `KeyAdded` on every chain, root-signed. The device
  reads it with its next pull of a chain (reads are open to the tailnet
  login), and the page switches to "trusted" from that reading, whatever
  became of the answer. Then the exchange posts every object the device
  sealed while untrusted.

A lost answer is safe once stage 6 adds one rule: the pairing file answers a
used code as "trusted" when the asker's key is the key that code trusted, and
as "used" for any other key. Then the device may post the same code again,
and the request is idempotent by its key. Today a second post of a used code
is refused.

### 4.2 Key revocation on every chain

A key is revoked at the terminal alone, and `suzatary device revoke` writes
`KeyRevoked` on each chain in turn. Between two of those appends, one chain
trusts the key and another does not. Today that window is short and harmless,
because every write route asks `Trust::admits` on every chain the route can
write (`route.rs:169-172`).

Once every write is an object, the window matters more, since an object
travels without its request. So `Trust::admits` reads the UNION of the
revocations of every chain and store the box holds: a key revoked anywhere is
revoked everywhere. Revocation is a two-phase set (once removed, never
re-added), so the union is monotone, and the decision is the terminal's
first `KeyRevoked`.

- **When it takes effect.** At each store's next acceptance after the box
  holds the first `KeyRevoked`. Acceptance runs under that store's lock, so
  no object is admitted by a stale trust read within one store.
- **What it refuses.** Every later object signed by the key, final. An object
  the device sealed before the revocation and posted after is refused too:
  order is causal and time is data, so the box cannot honour the device's
  `at`. Objects already accepted stay; they are history.
- **The device.** Its next request is answered with the wipe (410), as today.

### 4.3 Erasure reaching every device

An erasure is an `Erased { keys }` line. Keys form a two-phase set: the
lines' reading never sends a key that an erasure names (`routes::words`), and
a device deletes every erased key it holds at its next landing (the purge,
`LAZYSYNC` §7). So erasure needs no single decision. The box deletes the keys
from its key store at acceptance. The tab that erases draws the turns as
erased at once and deletes its own keys when the box's word holds the line, so
a refused erasure leaves the words readable as the box holds them. The memory's sweep
(`sweep_after_write`) and the upload index's reindex run after acceptance, as
readings of the new line, and each is idempotent.

The limit, stated: a device that never syncs again keeps its keys. A device
that is offline deletes them when it next lands the lines. A revoked device
is wiped, which deletes its database, and removing it from the tailnet is the
real cut-off (CLAUDE.md, Signatures).

### 4.4 Spending: a model call

A said line asks for one round. The worker for a conversation already runs
rounds one at a time, and rounds of different conversations in parallel
(`conversation_worker.rs`). The plan makes each start of a round an object,
and allows one automatic retry after a crash (the owner's answer 4, §8):

1. **Owed.** A reading of the lines. A said line is owed attempt 1 when no
   `Began` line names it and no later round set it aside. It is owed attempt
   2 when its only `Began` is attempt 1, no outcome answers it, and attempt
   1's run is gone (below).
2. **Claim.** The worker takes the conversation's hold, reads `owed` again,
   appends `Began { said, attempt, run, taint }`, and releases the hold. Only
   then does it call the model. `attempt` is 1 or 2; `run` names the server
   process that claimed it.
3. **Outcome.** The answer lines, as today, or a `Failed { said, class }`
   line. A round's error inside a live process is always written as `Failed`
   (the worker catches it), so a `Began` with no outcome means that its
   process died.

**When attempt 1's run is gone.** Each server process draws a random RUN id
at start and holds an exclusive `flock` on `runs/<run>.lock` in box-local
state for its whole life. The operating system releases a `flock` when its
process exits, however it exits. A worker claims attempt 2 only when both
hold, read under the conversation's hold:

- attempt 1's `run` is not the worker's own run, so a worker never retries a
  round its own process may still be running; and
- the worker can take attempt 1's run lock itself (a non-blocking `flock`
  that succeeds), which proves that the process that claimed attempt 1 has
  exited. It then releases that lock and deletes the file.

The first rule alone is not enough: two server processes on the box could
overlap for a moment (a restart whose new process starts before the old one
has exited), and the old one may still be calling the model. The second rule
is the proof of death, and it is sound on one host because a `flock` is held
until its process exits and only the kernel releases it. Claims are only
made on the fetcher (§4), so one host is the whole system. A worker of a
later run may only claim attempt 2 after attempt 1's `Began`, since it reads
that `Began` under the hold, so the two attempts never run at once.

**The law.** Under any interleaving of acceptances, process restarts and two
workers, a said line causes at most two model calls, and never two at the
same time. If attempt 2 is also interrupted, no third claim is owed: the
reading draws the round as "interrupted", and only Regenerate, a new request
with a name of its own, spends again.

`Began` also carries the round's taint, decided at each claim (§5.4). The
usage budget is checked at the same claim, so a round over the budget is
answered with `Failed { class: Budget }` and spends nothing.

### 4.5 Outputs: a coding session, a patch, mail

A tap is a `Tapped` object on a proposal in the inbox. The tap's consent is
the object's signature by a device key that the history trusts for the
owner's actor. That is the door's proof today (`Tap::of` from
`signatures::Trusted`), carried in the object, so it can be checked again
later by anyone who holds the store.

The inbox's performer reads every tapped proposal with no claim, and for each:

1. takes the inbox's hold, reads the proposal's state, and appends `Trying {
   attempt }` only while the proposal stands: not dismissed, not in
   conflict, and either never tried or last settled as `NotSent` before this
   tap (the tap's `deps` hold that `NotSent`, so a second try needs a second
   tap, as a resubmit does today). It releases the hold at once;
2. performs the output through its sink (EFFECTS-PLAN §9.3);
3. appends the outcome: `Sent`, `Applied`, `Acted`, `NotSent { class }` or
   `MayHaveGone { class }`.

This changes two things in `inbox::submit` (`inbox.rs:823`):

- **The hold covers the claim, not the effect.** Today `submit.lock` is held
  while the effect runs, so every tap waits for every other tap's effect. A
  10 s web fetch holds every other card for 10 s. With the claim, the hold
  lasts one append, and taps on different cards perform in parallel. The
  effect key (the proposal) is what the claim protects, so two taps on one
  card still perform once.
- **A crash cannot send twice.** Today a box that dies after the transport
  took the mail and before `Responded` is appended has recorded nothing, so
  the card can be tapped again. With the claim recorded first, the card
  reads `Trying` with no outcome, which the reading shows as "may have gone",
  and only the owner's `Checked` or a confirmation (EFFECTS-PLAN §9.5) settles
  it.

**Mail at most once, owner-signed.** The inbox's admission rule refuses a
`Tapped` on a card that sends mail (`Sending::of`, `app/core/src/secretary/sending.rs`: `Email`,
`Forward`, `AuthSend`) unless a device key signs it. The operator's root key
is refused there, by type, as `Tap::performs` refuses it today. The tap
names the proposal by its content name, not by an index, so the signature
covers exactly the message that was shown (the `secretaryproposes` rule, made
stronger: the tap still carries no body). A dismissal accepted after the
claim is recorded, and the reading shows the card as sent and dismissed, a
conflict shown whole, as EFFECTS-PLAN §9.5 does for `Confirmed` against
`Dismissed`.

### 4.6 A secret's value

The app may set a secret and never read one back (the `settings` and
`secretask` rules). A tap's object is synced to every device and kept in the
vault, so the value cannot travel in it in the clear, and the tap must not
need a second route. So each Secret or GoogleClient card carries a one-time
X25519 public key that the box drew when it filed the card. The box keeps the
private half in box-local state that the daily vault does not back up; stage
4 names the directory and a guard test that the vault's list excludes it. The
tab seals the value to that key
in the `Tapped` object. The performer opens it, writes the secret store, and
deletes the private half in the same claim. After that, no one can open the
ciphertext, which is the lines store's erasure by key deletion applied to a
secret.

### 4.7 Parallel, and where order is still one

- Rounds of different conversations run in parallel (already).
- Outputs of different cards run in parallel (§4.5).
- Acceptance of objects is per store, under that store's lock, and stores
  accept in parallel. An append rereads the whole store (about 250 ms for a
  chain at the live size, the `onewritermodel` limit; 0.8 s for the lines at
  the `bigstore` shape, §1.2). The page no longer waits for it.
- The instance file stays one register under one `flock`. Its edits are
  short, local and rare.

## 5. How it fits what exists

### 5.1 The Prodrome's sealed objects and frontier sync

Every write is a `Change` over its register's heads, signed (`Signed`) by the
device key, posted by the push-first exchange and taken by `sync::receive`'s
rules. The topic stores take unsigned changes today, on the strength of the
request's signature (`sync.rs:394-452`). Stage 1 makes them signed objects,
so an object's proof travels with it and every store reads trust one way.

Generic pieces, which belong in the Prodrome and hold no Suzatary type:

- **the receipt register**: `Refused { request, store, reason, kind }` and
  `Answered { ask, outcome }` over an outcome type a schema names, with its
  inflationary order and its laws. It would join `prodrome_core` as
  `receipt`, beside `store::frontier`. Suzatary's `Ask` and `Outcome` are the
  payload.
- **the owed reading and the claim**: "the requests of a store that no claim
  answers", and "append a claim under the store's hold only while the request
  is owed", generic over a schema with a request kind and a claim kind. It
  would join `prodrome_core::store` as `claim`. The worker (§4.4) and the
  performer (§4.5) are its two instances in Suzatary.
- **the rollback reading** (§2.5) is already the Prodrome replica's
  `standing`; the law joins its tests.

### 5.2 The effects plan

`docs/EFFECTS-PLAN-2026-10-25.md` splits every tapped proposal into `Write |
Output` (§9.2), performs outputs through one `Sink` port at most once by the
object's name (§9.4), and confirms an output by the input that brings it back
(§9.5). This plan keeps all of it and moves only who waits:

- a `Write` the owner makes (a topic, a title, a todo act) needs no proposal
  and no tap: it is a store write (§2.1). A `Write` the secretary proposes is
  performed when its tap is accepted, by its store's writer.
- an `Output` is performed by its sink after the claim (§4.5). The sink's
  `Receipt` (EFFECTS-PLAN §9.3) is what the performer appends as the
  outcome object.
- the chain of objects for one output is `Tapped`, `Trying`, then `Sent`,
  then `Confirmed` (EFFECTS-PLAN §9.5). Each is a state update in the inbox,
  and the page draws each one. This is the owner's "the confirmation that
  something got sent can be a state update".

### 5.3 The operator CLI's `call` (2.1.44)

`suzatary call get|post <route>` asks any route of the server through the
same gate, and `secretary ask` and `tap` are names over it. Under this plan:

- `secretary ask` seals a `Said` line with the root key (seat `Operator`) and
  posts it to the lines; it follows the lines for the round's outcome, as it
  does today with `Operator::pulled`;
- `tap` seals a `Tapped` with the root key; the inbox's admission rule
  refuses it on a sending card (§4.5), which moves `Sending::of` from
  `Tap::performs` to the rule;
- `call post` reaches the objects routes and the untrusted device's route only, since
  the other write routes are gone (stage 8). `call get` is unchanged.

So the operator's writes are objects signed by the root key, recorded in the
stores they write, rather than requests recorded in `requests/`.

### 5.4 The taint and stamp rules

- **The stamp** is checked at acceptance against the signer (§2.3). A line is
  clean as the owner's words only when the key that signed it is trusted for
  the owner's actor, the same rule as the gate's stamp today, now checkable
  from the store alone.
- **`Stamp::Unconfirmed`** marks a line that points at a todo no trusted
  actor confirmed (`stamp.rs`, decided where the box writes the line). The
  tab writes the line now, and its replica of the chains can be behind. So
  the stored stamp of a tab-written line is `Actor` or `Operator` alone, and
  the worker decides "points at an unconfirmed todo" when it claims the
  round, from the chains it holds then, and records it in `Began { taint }`
  (§4.4). `taint` stays a pure read of stored objects: of the stamp for old
  lines, and of `Began` for new rounds. The owner decided this (§8, answer 2).

### 5.5 The lazysync stages

LAZYSYNC §11 says the box alone writes the lines, so an IndexedDB upgrade may
drop the lines' row. That stops being true at stage 2. So:

- **Never drop an unsent object.** Every reset and upgrade of `store/lines`
  and `store/inbox` keeps the rule the chains have (LAZYSYNC §11, stage 5):
  post first, and drop a row only once nothing in it is unsent. A refused
  object is carried, named.
- **Own nodes.** The tab computes the skeleton node of each object it seals
  with the core's header function, the same `Step` the box computes
  (`secretary::step`). The law: the node the tab computes for its own object
  equals the node the box serves for it. Its content goes into the `content`
  store at once, so it is never fetched.
- **Eviction** (`keep.plan`, LAZYSYNC §15) keeps every own object until the
  box's word holds it.
- **The warm snapshot** (`warmsnapshot`, LAZYSYNC §16) is deleted by every
  row write, so each sealed line deletes it, and the store writes it again
  after 30 s of quiet. A conversation of several messages rewrites the
  snapshot (6 MB at the `bigstore` shape) once after it ends. Stage 3
  measures that cost.
- **The core's skeleton** (`coreskeleton`, the next stage of LAZYSYNC §16):
  once the core holds the skeleton, a seal is one call to the core
  (`Lines.append_own`) that returns the node, and the core can update its
  snapshot by that node instead of printing it again. Stage 3 does not
  depend on it.

### 5.6 What stays a request

- **The untrusted device's route** (§4.1): the device is not trusted yet,
  and a pairing code is a secret.
- **Revealing a held value** (`POST /api/held/reveal`): a read that answers a
  value the tab must not keep, recorded as `NotKept`. It is not a write, and
  the page shows a placeholder until the value arrives, as any read does. It
  becomes a signed `GET` in stage 8, so the write table holds writes only.
- **Client errors**: a beacon, one log line, no store.
- **Google's consent redirect**: the GoogleSignIn tap's outcome is the
  consent URL, an object in the inbox for the tapping actor. The page opens it
  when the outcome arrives. That wait is for an output the owner asked to
  see, not for a write.

### 5.7 Uploads

An upload is bytes, named by their SHA-256. The tab keeps them in the device's
content store and draws the thumbnail from them at once. It sends them with
`PUT /api/uploads/<sha256>`, which stores them once whatever the number of
puts and refuses bytes whose hash differs. The said line names the upload by
its hash, and the lines' admission rule refuses a line naming an upload the
box does not hold yet with `RefusalKind::Trust` (posted again later), so the
exchange puts the bytes first. "Keep it private" is an output (the Drive
vault) whose effect key is the hash, so keeping twice writes one file.

### 5.8 The decisions this revises

- **`phonereplica`**: "nothing optimistic, nothing rolled back, no queue".
  The first and third stay true: the reading is of what the device holds, and
  the unsent objects are a set, not a queue. "Nothing rolled back" becomes
  "a refused object leaves the reading with what rests on it", which
  `standing` already does for chains. The decision is extended to every store
  the tab writes.
- **`secretaryproposes`**: only `POST …/tap` turns a proposal into mail. It
  becomes: only a `Tapped` object signed by a device key the history trusts
  for the owner turns a proposal into mail, and it names the proposal by its
  content name. Exactly one file still names the transport.
- **`localnode`** and **`onewritermodel`**: unchanged.

### 5.9 What one shape replaces

Today each write has its own route, its own handler, its own wire body, its
own `api.ts` function and its own pending state in `asks.ts`. After stage 8,
each write is an object of its store's schema, and three generic pieces do the
rest: the exchange (already one), the receipt register and the claim (§5.1).
What goes, measured with `wc -l` on master at `c83b24d` (whole files; the
stages measure their own diffs with `scripts/size.sh`):

| Goes | Lines |
|---|---|
| `app/cli/src/commands/serve/proposals.rs` (tap, dismiss, acknowledge, drop) | 296 |
| the write handlers of `serve/secretary.rs` (open, title, erase, say, stop, regenerate, edit, upload, forget, retract) | most of 496 |
| the act handlers of `serve/the_box.rs` (device, key, pair become one route) | part of 240 |
| `web/src/prodrome/intents.ts`, `intentsync.ts`, `asks.ts` | 314 |
| the write functions of `web/src/wire/api.ts` (22 calls of `http.act`; the objects posts stay) | part of 279 |

The per-route handlers in `app/core/src/secretary/routes.rs` (2,948 lines)
move into admission rules and the two sinks rather than going, so the stages
report their net.

## 6. Does the page ever wait?

After stage 8, a page waits on the network in these places, and only these:

- a read it needs to draw (a content fetch, a conversation not on the device);
- revealing a held value, which is a read;
- the Google consent URL, which is an output's outcome the owner asked for;
- a pairing code's answer, drawn as "trusting" meanwhile.

No page awaits `http.act`. `scripts/web-static.sh` gains one check: `http.act`
is called only by the exchange and the untrusted device's module
(`app/pairing.ts`).

## 7. Stages

Each stage is one PR, a series of commits, each buildable and tested. Each
names the law it adds and what it measures. Each moves `gate::CLIENT_MIN`
when it removes or changes a route a page calls, so an old page is told to
reload. `web/e2e/writewait.bench.ts` loses a row each time a route goes.

1. **Topics drawn at once, and the asks store.** The topic sheet closes at
   the tap; the bar draws from the replica. Topic objects become `Signed` by
   the device key. The asks store exists with its one box-written event so
   far, `Refused`, written by the topic stores' acceptance. The tab syncs it
   and draws a refused topic as gone, named.
   *Law:* the rollback law (§2.5) over generated topic stores, with refusals
   drawn from the asks store. *Measure:* tap to the bar drawn, before and
   after, in the browser (`bigstore`'s server); expected no round trip.
2. **The tab seals lines: a title, a new conversation and a setup
   conversation.** The core's seal
   in the worker, keys posted beside the object, the scrub and stamp checked
   at acceptance (§2.3), own nodes (§5.5), and the lines' row never dropping
   an unsent object. Rename, open and setup stop calling their routes; the
   routes are deleted. *Laws:* a line the tab seals and the line the box seals from
   the same words read alike; the admission rule refuses a slot whose scrub
   changes its words and a stamp the signer does not prove; the node the tab
   computes equals the box's. *Measure:* rename drawn with no round trip;
   the box's acceptance time.
3. **Say, edit, regenerate and stop as lines; rounds claimed by `Began`.** The
   owed reading and the claim (§4.4), generic in the Prodrome; the run id and
   its lifetime `flock`; one automatic retry. *Law:* at most two model calls
   per said line, and never two at once, under any interleaving of
   acceptances, process restarts and two workers; a line interrupted twice
   reads "interrupted" and is owed nothing. *Measure:* Send to the question drawn (no round
   trip); Send to the first answer delta; the warm snapshot's rewrite cost
   after a conversation.
4. **Taps as inbox objects, claimed per proposal.** `Tapped`, `Dismissed`,
   `Checked`, `Dropped`; the performer's claim (§4.5); the sending rule at
   admission; secret values sealed to the card's key (§4.6). The four
   proposal routes are deleted. *Laws:* `app/core/tests/inbox.rs` extended:
   any interleaving of taps from two devices, dismissals and performer
   restarts performs each output at most once, and a crash after the claim
   reads as may-have-gone; a `Tapped` on a sending card signed by the root key
   is refused; a sealed value does not open after its card key is deleted.
   *Measure:* tap to "sending" drawn; two slow taps (the fixture's web read
   delayed) perform in parallel.
5. **The asks: forget, retract, cancel a watch, push token, build.** `Asked`
   and `Answered`; the box applies each ask to its own store. Erase becomes
   an `Erased` line. Their routes are deleted. *Law:* the asks register only
   climbs, and each ask is answered at most once.
6. **Devices.** `POST /api/devices/ask` in place of `/api/act/pair` and
   `/api/act/key`
   (§4.1); revocation read as the union over every store (§4.2). *Laws:* a
   key revoked on one chain is refused on every store at the next
   acceptance; a pairing code posted twice by the key it trusted is answered
   as trusted, and by another key as used.
7. **Authoring in the tab, and uploads by hash.** The core's authoring
   (`authoring::build_todo`, `update_todo`, the term from a price) runs in
   the worker; the markup's closed-world check moves to the chain's admission
   rule, and the row draws the source until the box renders it (as an
   unconfirmed todo's source is drawn today). `PUT /api/uploads/<sha256>`
   (§5.7). `POST …/author` and `POST /api/secretary/attachments` are deleted.
   *Laws:* a todo authored in the tab and by the box from the same fields
   prints the same event; a put stores bytes once and refuses a wrong hash.
8. **The RPC write routes deleted.** What answers a POST is the objects
   routes (each chain, each tab-written store, the asks), the untrusted
   device's route
   and the client-errors beacon; the upload is a PUT; the reveal is a signed
   GET. `app/cli/tests/writes.rs` pins that list. `intents.ts`,
   `intentsync.ts` and `asks.ts` are replaced by the reading of §2.5; the
   `requests/` record keeps only pairing and reveal. `call post` reaches the
   remaining routes. *Measure:* `writewait.bench.ts` reports the objects
   posts alone, and no page awaits `http.act` (`web-static.sh`).

Stages 1 to 3 can land before EFFECTS-PLAN stage 3 (one sink port). Stage 4
builds on that sink port if it has landed, and otherwise calls the act
performer as today.

## 8. The owner's answers (2026-11-01)

1. **Refusals live in one asks store** (§2.4), as recommended, rather than a
   refusal register per store.
2. **"Points at an unconfirmed todo" is decided when the round is claimed**
   and recorded in `Began` (§5.4), as recommended. The box never refuses the
   owner's line for a stamp that a race made out of date.
3. **A secret's value is sealed to a one-time card key** that the box deletes
   after use (§4.6), as recommended, so no second route carries it.
4. **An interrupted round is retried once, automatically**, against the
   recommendation to never retry it. §4.4 and stage 3 hold the design: a
   `Began` carries its attempt and its run, attempt 2 is claimed only once
   attempt 1's process is proven gone, a said line causes at most two model
   calls and never two at once, and a round interrupted twice waits for
   Regenerate.
