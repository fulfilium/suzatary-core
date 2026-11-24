# Taps and settings writes as local writes (2026-11-23)

The design that stage 4 of `docs/LOCALWRITES-PLAN-2026-11-01.md` builds from: what is left of
the roadmap task `localwrites` once a message, a rename, a topic and a todo act are objects the
tab seals. Nothing here is built. It revises that plan's §4.5, §4.6 and stages 4 to 6 where
they disagree, and says so. Paths are relative to the repository root, measured on 2.1.230.

## 1. What is left, counted

| Write | Route today | Its client | What makes it the owner's | What stops a replay |
|---|---|---|---|---|
| tap a card (and a card's fix) | `POST …/proposals/{n}/tap` (`app/server/src/serve/proposals.rs`) | `api.ts` `tapProposal`, `fixProposal` | a `Tap` minted by `Tap::of` from the request's device signature and the CSRF triple (`app/signatures/src/signatures/tap.rs`) | the request's name (`request_name`: method, target, `at`, nonce, body digest, key) answered from `requests/` within `WINDOW_SECONDS` (600 s) |
| dismiss, acknowledge, drop | `…/dismiss`, `…/acknowledge`, `…/drop` | `api.ts` | the same door | the same record |
| forget a fact, retract a summary, cancel a watch, erase, build a coding card | five routes under `/api/secretary`, `/api/mail`, `/todo/{id}/build` | `api.ts`, `pages/` | the door's signed request | the same record |
| push token | `POST /api/act/device` | the page inside the APK's WebView (`app/pushtoken.ts`); the APK's Java posts nothing | the page's device signature | the token is a register: posting it again changes nothing |
| ask to trust this device; a pairing code | `POST /api/act/key`, `POST /api/act/pair` | `pages/settings.ts`, `app/pairing.ts` | nothing yet: the device is untrusted, and the code or the owner's tap on the filed card proves it | the pairing file's lock: one use, five tries |

That is 12 POST routes, 7 request bodies in `app/api/src/wire.rs` (`Tapping`, `Endorsing`,
`Reasoning`, `KeyAsked`, `Pairing`, `Registering`, `Erasing`), 12 `http.act` calls in
`web/src/wire/api.ts`, 298 lines of in-memory pending state (`intents.ts`, `intentsync.ts`,
`asks.ts`) and one more record of what happened (`requests/`), beside the stores. A message, by
contrast, is one thing: a signed object in a store the tab replicates, drawn at once, whose
receipt is more objects (stage 3). The settings page writes nothing of its own: its writes are
the setup cards' taps and the two device asks.

The aim is that a tap, a message and a settings write are one kind of thing, and that an effect
is two kinds of object and no more:

1. **the tap**, the owner's signed object that says an effect is to take place on a card;
2. **the outcome**, an object the executor writes that composes with the tap to say the effect
   took place, or was refused, or failed, each in words. Only the deployment's own performer writes
   one: the inbox's admission rule takes an outcome from its key alone.

A card is a fold over the two. There is one executor, on the host `<data>/FETCHER` names
(`suzatary up` refuses elsewhere), so nothing arbitrates between executors, and a tap is performed
whenever the executor comes to hold it.

**Counted, for an effect on a card.** Today: 4 routes (tap, dismiss, acknowledge, drop) with 3
request bodies, the `requests/` record, a submit that holds `submit.lock` for the whole effect,
and 7 responses written into the inbox (`Sent`, `Applied`, `Acted`, `NotSent`, `MayHaveGone`,
`Checked`, `Dismissed`): 16 things. The first draft of this note kept the 7 responses and added a
replicated claim (`Trying`), an epoch rule and a stale-tap rule: 10 things, beside the objects.
After: 2 kinds of object (the tap, the outcome), the one executor, and its look at start: 4
things, beside the store objects every write already has (acceptance is the box's frontier
holding the object; a refusal at the store's door is a `Refused` in the asks store). What
stays a request is what cannot be an object: the untrusted device's ask (it can sign nothing the
box trusts), the held value's reveal (a read) and the client-error beacon.

## 2. A tap is an object

A tap is a change to the inbox, `Tapped { proposal, answer }`, sealed in the tab over the heads of
that proposal's register as the tab drew them, signed by the device key, kept in the device's row
and drawn at once. `answer` says what the owner asked: perform it (with today's `Tapping` without
the transport: the note, `always`, a Device card's endorsement, and for a Secret or GoogleClient
card the value sealed to the card's one-time key, LOCALWRITES §4.6), dismiss it with a reason, or
drop a coding session with a reason. A dismissal is a tap whose effect is that the card closes;
there is no acknowledge, since nothing is left for the owner to check (§4).

- **Why it is the owner's.** The inbox's admission rule takes a `Tapped` only under a `Signed` by
  a key the history trusts for an actor of `web.act_as`, at the seat its key speaks from, and
  refuses a `Tapped` of a card that sends mail (`Sending::of`) signed by the root key: the rule
  `Tap::performs` applies today, moved to the store's door. The actor is the poster's mapping,
  never a field the tab chose (the replica write rule of CLAUDE.md).
- **Why it is this card.** It names the proposal by its `ProposalId`, the content name of the
  filed proposal, so the signature covers the message that was shown, not an index into a
  transcript that a later line can move. Its `deps` are the proposal register's heads the page
  drew, so a tap of a card revised or answered since is concurrent with that answer, and the
  executor refuses it in words (§4).
- **Why a replay is no second tap.** The object's name is the SHA-256 of its literal, which holds
  the store's genesis, the deps, the event with its `at` and actor; its `Signed` holds the key. A
  retry posts the same bytes, so the box answers `already` with no time window: the request hash
  is the object's name, and the store is its record. A new tap needs a new signature, which only
  the device's non-extractable key makes. `requests/` keeps only the pairing ask and the reveal.
- **What a request hash no longer does.** It no longer stops a second effect: the request is the
  tap, and the effect key is the proposal (LOCALWRITES §2.2). Two devices tapping one card are
  two taps and one effect.

## 3. The card is a fold, and what is drawn when

Every state below is a reading of the tab's replicas of the inbox and the asks, so a reload, a
second tab and a second device draw the same thing. The tab never draws an outcome from its own
tap.

| The replicas hold | Drawn on the card | Why it is true |
|---|---|---|
| the own tap, not in the box's frontier | "Tapped 14:02 · not at the box yet"; after 5 s offline, the shell's outbox line | the device holds the tap; nothing has happened in the world |
| the tap in the box's frontier, no outcome | "Tapped 14:02 · at the box" | the executor holds it, and performs it or finds it done |
| an outcome that it took place | "Tapped 14:02 · sent 14:03" (or applied, or acted), and "found in Sent" where the executor found it rather than performed it | the executor's object, from the sink's answer or from the sink's input |
| an outcome that it failed | "Tapped 14:02 · not sent: <why>", Tap again | the sink said nothing left |
| an outcome that it was refused | the card as the box holds it, with the refusal in its words, until the person dismisses the notice | the executor performed nothing |
| a `Refused` of the tap at the store's door | the same, in the door's words | the box never took the tap |

A tap drawn as done before its effect ran would be a lie, so "sent" is drawn only from an outcome.
Each outcome is a climb of the inbox's `Phase`, so the drawing only moves forward, except to a
refusal, which returns the card to the box's state. "In flight" is not a state of its own: a tap
the box holds with no outcome is "at the box", which is all the owner can know.

## 4. The executor, and the objection

**The objection, as strongly as it can be put.** Convergence is a statement about states: two
replicas that hold the same objects read alike, whatever order they received them in. An effect
in the world is not a state. Sending a mail twice is not sending it once, and no join undoes the
second. Worse, a replicated tap is read by more than one process over time: the executor after a
restart, a box restored from the vault. Each sees "tapped, no outcome", and an executor that
performs every such tap would send again whatever a run that died had already sent. And a tap
delayed by a train journey is performed hours after the owner decided. The HTTP route avoids this
by making the tap and the effect one exchange the owner watches.

**What the design does about exactly that.** It does not make mail idempotent, which it cannot
be. It uses what the effects plan already holds: every sink is confirmed by its input (EFFECTS
§9.5), and names its effect by the proposal.

1. **One executor performs.** It reads the taps that no outcome answers, in the store's order. A
   tap accepted while this run of the executor lives is performed at once: no earlier run could
   have held it.
2. **A tap held at the executor's start is looked for first.** Such a tap may have been performed
   by a run that died before writing its outcome, or by a box whose outcomes a restore lost. So
   before performing it, the executor reads the sink's input for the effect the proposal names:
   - found: it writes the outcome (taken place, found, with the reference that found it) and
     performs nothing;
   - not found, by a read of that input that began at least the sink's filing bound `F` after
     the executor started: it performs the tap.
   The read must begin after the start so it cannot be a read from before the effect; it must
   wait `F` so that an effect handed to the sink just before the death has had time to show.
   Meanwhile the card reads as any tap the box holds with no outcome ("Tapped 14:02 · at the
   box"): the look is no state of its own. A read that fails (the sink's server away) is a read
   not made: the executor reads again with the input's own retry, performs nothing, and the card
   stays as it is, however long that takes.
3. **Each sink names its effect by the proposal, and files where it is read.** Stage 4 makes this
   true where it is not yet:
   - **mail** (`Email`, `Forward`, `AuthSend`): the Message-ID is minted from the `ProposalId`
     (today `commands/mail.rs` mints it from the clock and the process), and the account's Sent,
     which the server files, is read by a sync of that folder alone. `F` is 120 s.
   - **calendar event**: the event id is the box's, from the `ProposalId`; an insert of an id
     Google holds answers that it exists, so the insert is its own look. `F` is 0.
   - **patch**: the branch is named by the proposal, and a push of the same commit to it changes
     nothing. `F` is 0.
   - **coding session**: named by the proposal; its input is its report on a `claude/` branch.
   - **sign-ins** (`Auth`, `GoogleSignIn`, `SignIn`, `SignInCode`, `UnlockStore`): their effect
     is a consent the owner gives in a browser; performing again offers the consent again, and
     changes nothing until the owner answers.
   - **reads** (`Web`, `DriveRead`): no effect in the world beyond the request; performed again,
     the request goes again.
   - **writes to the box's own stores** (a todo change, a remember, a device, a secret, an
     instance edit): state updates under their store's hold and compare-and-swap, which the
     executor applies once by the tap's name in that store.
4. **The windows that remain, named.**
   - **mail filed late.** A message the server accepted from a run that died, and files into Sent
     more than `F` after the executor started, is sent again. The read waits `F` from the start,
     not from the death, so the window is the server's own filing delay past `F`.
   - **a coding session.** Its report arrives minutes to hours after it fires, so a session fired
     by a run that died is not seen by the look, and fires again. No `F` closes this; the window
     closes only if the routine answers a second fire of the same name with the first.
   - **reads** run again, as above.
**Taken out of the earlier draft, one sentence each.**
- **The replicated claim (`Trying`):** with one executor there is no other to exclude, and the
  look at start covers a run that stopped by reading the world rather than a record of intent.
- **"May have gone: check Sent" and `Checked`:** the look reads the same Sent the owner would
  check and writes the outcome itself, so nothing is left for the owner but the windows above.
- **The epoch rule for a restored box:** a restore leaves taps with no outcome, which the look
  covers as it covers a restart (point 5), with the same windows and no other.
- **The stale-tap rule:** a tap is performed when the executor holds it, and the card shows when
  it was tapped and when it was performed.

**The seam.** "Who performs a tap" is the one executor today. Should a second executor ever be
wanted, that sentence becomes "the holder of the tap's claim", a lock to be designed in the store
library then; no object stored under this design would change, since the tap and the outcome name
the proposal and not their executor.

5. **A restored box** holds taps whose outcomes the restore lost; they are taps held at the
   executor's start, so the same look covers them, with the same windows. Mail is a cache rebuilt
   from the server, so the Sent it reads is the server's, not the vault's.
6. **A late tap is performed.** A tap delayed by a train is performed when the executor holds it;
   the card says when it was tapped and when it was performed, so the owner sees both. The tap
   rests on the card as drawn, so a card revised or answered since is a different card (§2).
7. **The card that is gone.** A tap that arrives after its card was dismissed, withdrawn, revised
   or performed from another device is answered by an outcome that it was refused, in words
   ("dismissed on another device at 13:40"; "sent from another device at 13:41"). A dismissal and a
   tap held at once are a conflict shown whole, and the executor performs nothing in conflict
   (`inboxschema`).

## 5. On a train with no signal

The person taps Send. The card reads "Tapped 14:02 · not at the box yet" at once, and the shell
says the tab is offline. That is true: nothing has left the device. The tap is in the device's
row, so closing the tab, rebooting the phone or opening a second tab keeps it and draws it the
same. When the network returns, the exchange posts it once; the post is idempotent by name, so a
post whose answer was lost is posted again and costs nothing. If the card is gone by then, the
refusal says why (§4.7); if not, the executor performs it, and the card reads when it was tapped
and when it was sent. The person never reads "sent" for a mail that has not been sent.

## 6. The Android client

The APK's Java posts nothing: it reads `GET /api/portal` (`Pairing.java`) and `GET /api/fold`
(`Widget.java`) and hands the push token to the page. Every write comes from the box's bundle in
the WebView, which the box serves, so it moves with the box and a page of an older contract is
told to reload by `CLIENT_MIN`. So a client two releases old keeps working as long as two
contracts hold, and every stage pins them: `GET /api/portal` and `GET /api/fold?limit=` answer
the shapes the APK reads, and the CSRF check keeps taking a WebView request that carries no
`Sec-Fetch-Site` label (`csrf.rs`'s "the Android shell's own registration carries no bundle").
The push token becomes an `Asked { Push }` object the page signs, so the APK is untouched.

## 7. Which routes go, and when

- **Stage 4, taps.** The inbox becomes a store the tab writes (`Written`), with its admission
  rule (signer, seat, sending cards, the card's `ProposalId`). The tap and the outcome; the one
  executor with its look at start; each sink naming its effect by the proposal (the mail's
  Message-ID first); the secret sealed to the card's key. Goes: `…/tap`, `…/dismiss`,
  `…/acknowledge`, `…/drop`, `Tapping`, `Endorsing`, `Reasoning`, `proposals.rs` (258 lines),
  `submit.lock`'s hold over an effect, and `MayHaveGone` and `Checked` as things written
  (stored ones are read forever). The setup cards are taps, so the settings page's writes move
  here.
- **Stage 5, asks.** Forget, retract, cancel a watch, build, the push token: `Asked` and
  `Answered` in the asks store. Erase: an `Erased` line. Goes: their five routes,
  `POST /api/act/device`, `Registering`, `Erasing`, and `intents.ts`, `intentsync.ts`,
  `asks.ts`.
- **Stage 6, devices.** `/api/act/key` and `/api/act/pair` become one request route for the
  untrusted device, `POST /api/devices/ask`, answered idempotently by key (LOCALWRITES §4.1).

## 8. The laws to generate

Each over generated sequences of taps and outcomes, run at 20,000 cases once before the stage is
called ready:

1. **At most once per effect, outside the named windows.** Over any interleaving of two devices of
   two tabs each tapping and dismissing, posts lost and repeated, the executor dying at any point
   and starting again, and a restore to an earlier store: with a sink whose input shows each
   effect within `F`, each output is performed at most once per tap, and a tap that an outcome
   answers is never performed again.
2. **Every held tap gets one outcome.** Once the executor holds a tap and runs long enough, exactly
   one outcome answers it, whatever the interleaving; the fold reads that outcome.
3. **A retry is the same request.** Posting a tap k times leaves one object and k receipts that
   all read it taken.
4. **A refusal is shown until the person acts**, in every tab of the device and across reloads,
   as the lines' own refusals are (`releasable`).
5. **The card is a fold that never claims an outcome it does not hold.** For every replica, the
   state drawn for a card is a function of its taps and outcomes alone; as the replica grows it
   only moves forward, except to a refusal; and it reads done only where an outcome says so.
6. **Only the owner's device sends.** A tap on a sending card is taken only under a device key the
   history trusts for an actor of `web.act_as`; the root key and an untrusted key are refused at
   the store's door.
7. **A sealed secret dies with its card key.** Once the executor deletes the card's private key,
   the tap's value opens for no one.
