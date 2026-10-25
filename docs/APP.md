# The app — what it is for, and what it should become

The owner, 2026-09-12: "It's worth speccing the app fully, what's useful for day to
day, what's not, what nice features should exist, how things connect. It's nice
to have good high level design for all of this."

This is that document. `docs/ARCHITECTURE.md` says how the system is built and
`docs/AUDIT-UX-2026-09-12.md` says what is rough about the screens as they are;
this says what the app is FOR, grades every destination by how often a person
actually needs it, states the flows between them in one place, and names what
should exist, what should go, and what must not move. The queue is the
`suzatary` chain; this is the lens the queue is priced against.

## 0. One sentence

The app is where the owner sees what is owed, decides what his secretary has
proposed, and asks — on a phone, backed by a chain that proves every number.

Three clauses, three centres of gravity: the LIST, the INBOX, the
CONVERSATION. Everything else is either a way of reaching one of those or
evidence that they are telling the truth.

## 1. The person and the loop

One person, mostly on a phone, often between other things. Three writers feed
what he sees — himself, a trusted secretary, and an untrusted triage agent
whose work is reviewed before it lands — and the design goal is that he never
has to know which one wrote a thing beyond the `(unconfirmed)` marker.

**The day, as it actually runs.** A push at 07:03 is the morning brief: a
secretary conversation that opens with the weather, says what moved, summarises
what it settled for him overnight, lists what is waiting on his tap, and offers
two or three moves as proposals he can confirm from the phone. Through the day,
pushes arrive as todos land — from mail the agent read, from things he shared
into the app, from the secretary writing directly. He opens a todo from a push,
reads it, does the thing, completes it. When the Inbox badge shows a number,
something is waiting on him and only him: a batch the secretary held because it
touched a legal matter or his money, an email the secretary wants to send, a
changeset it was not sure of.

**What the loop is not.** It is not a mail client — he has three. It is not a
place to browse; the list is sorted and the top of it is the answer. It is not a
project tracker with columns and drag; the order is a fold and nothing reorders
it but a repricing. And it is not somewhere a chat replaces the todo: a
conversation ends in proposals, and a proposal ends in a tap or a no.

## 2. The map, graded

Four grades. DAILY means the app opens for it. WEEKLY means it is reached for.
EVIDENCE means it exists so a claim elsewhere can be checked, and its job is to
be right rather than to be visited. RETIRE means its reason has gone.

| destination | grade | what it is for | what it should not become |
|---|---|---|---|
| **Todos** | DAILY | The list, ascending in fulfillment: what is owed, most urgent first. The home screen. | A dashboard. One list, one order, filters that get out of the way. |
| **Todo** (detail) | DAILY | One todo: what it says, what to do, why that number, what it rests on, what happened to it. Every act the chain allows. | A page about the evaluator. The arithmetic is below the fold, under a disclosure, in prose first. |
| **Inbox** | DAILY | Everything waiting on his tap and nothing else: held batches, proposed emails, proposed changesets. Badge = count. Empty most days now that the secretary reviews. | A feed. If it is not waiting on HIM it does not belong here. |
| **Secretary** | DAILY | Conversations, the morning brief at the top of them, and Ask one tap away from anywhere. Proposals are made here and answered here or in the Inbox — same tap. | A chat that replaces todos. Every conversation should be trying to end in a proposal or a todo. |
| **Project** | WEEKLY (his) / DAILY (the agents') | The second chain, same shape as Todos: the roadmap and the issues the system files about itself. | A different UI. It is Todos pointed at another store, and `#/project` calls `setChain`. |
| **Mail** | REFERENCE | The corpus the agent read, reachable FROM a todo's source and a secretary's reference. Read-only by design. | A mail client. Search and threads stay; composing never arrives, because sending is a tap on a proposal. |
| **Chain** | EVIDENCE | Every object rehashed in the tab, causal order, heads. The proof. | Anything but correct. |
| **Briefings** | RETIRED (2026-09-13) | Saved daily files from the era when the assistant session wrote them. The mechanism retired 2026-09-11; the entry left the sheet on the 13th. | A destination. The files and `#/briefings` stay served, linked from About and from Secretary as the archive. |
| **About** | RARE | What the app claims about itself — one evaluator, on two machines — verbatim from the footer that stood under every page; the theme control; the archive link. | A settings page. Theme is its only control and the rest is prose that had to live somewhere. |
| **Usage** (not built) | WEEKLY | How much of the Claude allowance this box has spent, in the windows the limits are measured in, split by who spent it, and whether it has crossed into credits — because the quota is the constraint and the Claude app is bad at showing it. | A price. The subscription is flat up to the allowance and CREDITS are real money past it; the page separates those two and shows the billed quantity, but the price per credit is not on this box and a made-up one is worse than none. |

Some rows are several pages (`message` and `thread` are Mail's,
`conversation` is Secretary's, `briefing` is Briefings'). None of them is
wrong; what the grades say is where attention should go. Three rows are DAILY
and one is RETIRE, and that is the shape of the next few weeks.

The Graph, History and Links pages were removed on 2026-09-26 (the owner: "The
fulfillment graph and links pages I never use, history for everything I never
use as well"). A todo's own links and history stay on its page.

## 3. How things connect

Stated once, so the next feature is judged against the flows rather than
against a page.

**The frontend has a contract.** `docs/WEB-SPEC.md` names its types, its
three doors and its laws, each pinned by a node test over a seeded
generator; a page is a closed state with a pure step and an idempotent draw
over one source (`web/src/algebra/source.ts`), and the Rust writes vectors the pure
modules must agree with (`webalgebra`, 2026-09-16). A feature that adds a
closure flag, a second copy of state or a cast is judged against that
document before it is judged against a page.

**Mail becomes a todo through a card.** The agent reads the corpus every five
minutes and files what it would change as cards in the Inbox, one per todo,
each with its reason and the mail it rests on. The secretary reviews them
against a written rule and withdraws duplicates and noise; everything else
waits for his tap, and nothing reaches a chain until he taps it.

**The person writes three ways and they are one path.** The phone seals an act
locally and syncs it (`POST /api/objects`); a share from another app arrives as
`#/add?text=…`; the CLI appends directly. All three go through
`authoring::build_todo` for an add and `confirm_todo` for a confirm, so the
rules cannot fork — what differs between the callers is the actor and only the
actor. An act the box refuses stays local and is NAMED; it is never retried and
never silently dropped.

**The secretary proposes and he taps.** A conversation is where the secretary
reads, writes directly for routine evidenced work, and PROPOSES for anything it
is unsure of — an email, or a changeset of chain acts. A proposal is stored
whole and shown whole, in the conversation and in the Inbox. The tap carries an
index and never a body, needs the gate's witness that it was him on his own
device, and is the ONE path from this app to an outward act. The answer no is
a dismissal with a reason the secretary reads next round.

**The brief is a conversation with a push on it.** A timer runs
`secretary brief`, which opens a conversation, answers it in the shape
`SECRETARY.md` prescribes, and pages the phone with a deep link into it. Every
proposal in the brief is an ordinary proposal; every reference in it is an
ordinary reference. There is no briefing format any more — there is a
conversation that happens to arrive at 07:03.

**There is ONE conversation, and it remembers** (the owner, 2026-09-13: "I like
the format of conversation that we have here with continuous memory, I want
the chat in the suzatary to be similar … by far most of the time I want to have
1 default chat where the agent knows everything"). BUILT 2026-09-14 (chain
item `onechat`, `docs/ONECHAT-PLAN-2026-09-13.md`, `docs/ARCHITECTURE.md`
§10.3); until then a conversation was a transcript re-sent whole to a
stateless `claude -p` on every turn, the page disabled Send while a turn ran,
and each conversation started knowing only what its system prompt carried.
The design, as built:

- *The default conversation.* Secretary opens into it; the brief lands in it
  (the morning is its top, not a separate page); `start` still makes another
  for a side topic, and that is the exception. It is the same conversation
  tomorrow, and the secretary in it has read everything it has ever read there.
- *A session, not a re-send.* The default conversation is a long-lived Claude
  Code session (`--resume`, or a process kept open on `stream-json` input), so
  a turn sends only what is new, the harness compacts the context itself, and
  the prompt cache does what it does for a long chat (this session runs at 98%
  hits; the stateless rounds at 84%, with 61% of runs starting cold). The
  session id is state the box holds beside the transcript; losing it costs one
  cold re-send of the transcript, nothing else. `Model` grows a session-shaped
  turn beside the stateless one; the stateless one stays for triage and review.
- *A queue, not a disabled button.* Send always works. A message sent while a
  turn runs is appended as a THE OWNER line at once, shown as queued, and reaches
  the model at its next step (the harness surfaces mid-turn messages the way
  this chat does) or, if the runner cannot take it mid-turn, as the head of the
  next turn together with anything else that queued. One worker per
  conversation; the page streams the answer over SSE as it is written rather
  than waiting for the whole of it.
- *Answers are Typst, rendered as HTML, in blocks.* (`docs/CHAT-PLAN-2026-09-25.md`
  §4.5–4.6, `docs/TYPST-FIRST-PLAN-2026-09-25.md`.) The secretary writes
  Typst — every todo body is Typst prose, references are
  `#link("todo:<id>")` — so an answer is cut at top-level blank lines into
  blocks, each rendered as typst's own HTML (MathML for equations) through
  the render boundary, cached on disk by content, and parsed by `rich.ts`, so
  it reflows, selects and copies like text. A block naming the `html` module
  or `eval` is drawn as text, as is one that does not compile; a streaming
  block is asked for by its sha, never by its text. A day's briefing is the
  same blocks. A PROPOSAL stays text, verbatim: what he confirms is exactly
  what is sent, and a rendering of it would be a second reading of that rule.
  References are drawn from the tokeniser over the turn's text and are live
  links into the chain. The first version drew a whole turn as an SVG image
  per theme (chain item `typstanswers`); that route is gone.

Chain items: `onechat` (the default conversation, the session, the queue,
streaming), `typstanswers` (the rendered answer, since replaced by HTML
blocks) and `chatshape` (rounds,
cards in place, a windowed dated transcript, acts that wake the model, a push
when unwatched, the about-slice, the sheet and the panel; plan
`docs/CHATSHAPE-PLAN-2026-09-15.md`). `askfromanywhere` was the first door
into the conversation and the sheet is its generalisation;
`briefwherethemorningstarts` is settled by `onechat`.

**A todo points at its source, and the source points back.** A todo carries
`source.hash`; the Mail message page lists the todos it produced. A todo's
prose can name another todo (`#link("todo:<id>")`), which is two lists on the
todo view, one on each side of the link. A secretary turn can name a todo, a
message, a thread, a conversation or a proposal in one text form, and the app
tokenises it into links. Nothing here is a stored relation: every edge is
derived from content.

**Every page is a fold, and the browser folds too.** The server folds the
chain; the tab holds the objects and can fold them itself with the same core
compiled to WebAssembly. So a page offline draws THIS MOMENT's numbers from
what it holds and says the chain may have moved, rather than yesterday's numbers
with no provenance. The staleness banner is the honest sentence about that, and
it belongs on every page because it is a claim about the whole fold.

**Pushes land on the thing they are about.** A new todo → its page. The brief →
its conversation. The review down for an hour → the Inbox. Each is a deep link
the Android portal opens in place, and none of them carries content — a title,
a count, a URL.

## 4. What should exist and does not

Ordered by how often the absence is felt. Each one is stated with the semantic
it maps onto, because a feature that needed a new stored kind would be a
feature the chain does not have — and two of the tempting ones are refused
below for exactly that reason.

1. **Complete from the list — ALREADY THERE, and this item was wrong**
   (corrected 2026-09-13). The card's footer carries a Done button for a
   login that may act (`card.ts::foot`, `actButton("done", …)` behind
   `mayAct()`); the gallery this section was written from ran as a VIEWER,
   which draws no act controls, so the picture showed a footer with only the
   slug. The keyboard's `c` and the card's Done are the same seal. What
   remains true is the audit's finding 2: the footer row costs every card a
   line even when it holds nothing but the slug and a chevron, and that is the
   fold item below, not a missing act. Kept here rather than deleted so the
   next reader of a viewer's gallery does not file it again.

2. **Edit a todo in the app — ALREADY THERE** (built, project chain
   `editandtick`). `POST /api/author` gains an `update` arm
   (`routes::author`'s `Authoring::Update`, built through `form::update` and
   `authoring::update_todo` exactly as `add` goes through `form::add` and
   `build_todo`) that builds and appends nothing, and the todo page
   (`web/src/todo/editform.ts`'s `editPanel`) gains a form that sends only the
   fields that changed. The tab seals what comes back through the same door
   `add` uses.

3. **Tick a subtodo — ALREADY THERE** (built alongside item 2). Subtodos are
   tappable checkboxes (`web/src/todo/editform.ts`'s `subtodoChecklist`, native
   `<input type="checkbox">`, so keyboard- and screen-reader-operable for
   free) rather than a read-only list; ticking one sends the WHOLE checklist
   back through the same `update` arm — a register holds its frontier, not a
   diff — and the "1 of 2 done" chip moves.

4. **Defer, as a curve and never as a flag — BUILT** (project chain
   `deferascurve`). "Not before Friday" is a real need and a snooze field is
   the wrong answer to it: state outside the fulfillment curve is the
   dual-write class §10.1 killed. The right answer is `Piecewise(Flat(1.0),
   [(D, term)])` with `term` UNSHIFTED — nothing owed before `D`, and from `D`
   the todo is exactly what it was — which `authoring::defer` builds and
   `TodoEdit.defer_to` carries through the same `update` arm items 2-3 use,
   with the rationale generated ("deferred to `D`") rather than typed. It is
   deliberately NOT a `Shift` (which would move the deadline the postponed
   work was against) and NOT a fresh `Decay` from `D` (which would invent
   one): a deferral is refused, in the box's own words, for a date not after
   today or for one past a `Decay` the term already carries — postponing past
   a deadline is a REPRICE, and this says so rather than quietly moving it.
   Deferring an already-deferred todo REPLACES the outer deferral instead of
   nesting one `Piecewise` inside another's single piece. The web page's "Defer
   to…" control (`web/src/todo/editform.ts`'s `deferControl`, beside the edit panel
   on an open todo) sends `{ todo, defer_to }` and the todo's thirty days then
   show the deferral as fulfilled until the day and as it was after — which is
   what a deferral IS.

5. **Two more filters on the list: waiting, and dated.** `waiting_on` is on
   every record and on every card; "what am I blocked on" is one predicate
   over it. "What has a date in the next fortnight" is one predicate over the
   spec — any term with a `Decay` end inside the window. Both are pure
   functions in `todos.ts` beside the existing filters, node-tested.

6. **Search everything from ⌘K.** The palette matches destinations and todos.
   Mail has a search route and conversations have a list; the palette should
   reach both, so one field finds a todo, a message or a conversation without
   knowing which it is.

7. **The brief where the morning starts.** Today's brief is a conversation in a
   list. On the morning it arrives it should be the first thing on the
   Secretary page, and a one-line card at the top of Todos ("Your brief is
   ready — 3 proposals") until it is opened. After that it is a conversation
   like any other.

8. **Ask from anywhere.** "Ask the secretary" exists on a todo, a message, a
   thread and a briefing, and attaches the reference. It should also be a
   keyboard verb and a palette entry with no reference, because half the
   questions are not about a thing on screen.

9. **Show the Claude usage this box is spending.** The owner, 2026-09-12: "the
   app should also show me my current usage and info, cause for some reason
   the Claude app is bad at that." Quota, not money, is the constraint: the
   weekly limit killed a build mid-series and stopped the secretary's review
   for a day, and the subscription is flat, so the only number that matters is
   how much of the allowance is gone.

   **What this box can know.** Every request Claude Code makes is written to
   `~/.claude/projects/**/*.jsonl` with its model, its timestamp and its
   usage — fresh input, cache write, cache read, output — keyed by a request
   id (the same line is written more than once, so the id is the dedup; the
   2026-09-12 measurement found 36% duplicates). And when a limit refuses a
   request, the refusal is written too, with the limit's own metadata: which
   window (`seven_day`), when it resets, whether overage was allowed. So the
   page can show, honestly: consumption in the **current 5-hour block** and
   the **current 7-day window**, because those are the windows the limits are
   measured in; the split **by model** and **by who** — the main session, the
   triage timer, the secretary — read off which project directory the
   transcript sits in; the **cache hit rate**, which is what a cold start
   costs; and the **last refusal**, with its window and its reset time, which
   is the one fact the Claude app is worst at.

   **Credits are real money, and the page must say where they start.**
   The owner, 2026-09-12: "after a certain amount of usage there's also usage
   credits, which are real money." The subscription is flat UP TO the
   allowance; with extra usage enabled, crossing it does not refuse — it
   bills. So there are two kinds of consumption and one page must not blur
   them. What this box holds: every limit refusal carries `isUsingOverage`,
   `overageStatus` and `overageDisabledReason` (measured 2026-09-12: 13
   refusals, overage org-disabled throughout, so nothing here has ever run on
   credits and successful requests carry no overage field). What follows is
   the honest derivation: consumption AFTER the last allowance refusal in a
   window, while the last-reported overage status is enabled, is the quantity
   credits are charged on. The page shows that quantity under its own heading,
   the overage status the limits last reported, and NO price — the rate per
   credit is not on this box, and a number invented to look complete is the
   ccusage mistake of the same morning, where API list prices read as a bill
   of thousands against a plan of two hundred.

   **What this box cannot know, and how to be honest about it.** The
   allowance itself is not written anywhere local. Nothing here can say "62%
   of your week". What it CAN say is empirical: the last time the weekly
   limit refused a request, this much had been consumed in the seven days
   before it — so that consumption is a measured ceiling, and "you are at
   this fraction of the last ceiling" is a true sentence where a percentage
   of an unknown would be a guess. If Claude Code ever exposes the allowance
   or the credit rate through something this box can read, each is one more
   field; until then the page says which kind of number each number is.

   **The boundary, stated once.** The transcripts QUOTE MAIL and everything
   else that passed through a session. The reader extracts numbers, model
   names, timestamps and request ids and NOTHING ELSE — no text of any
   message reaches the wire — and it is a capability on `ServerCapabilities`
   ALONE. Not the agent's bundle: an unattended process reading its own
   transcripts back would be the durable memory §Containment rejected. Not
   the secretary's: it has no reason to know what it costs, and a model
   reasoning about its own quota is a model with a reason to do less.
   `#/usage` lives in the More sheet, graded WEEKLY: reached for when the box
   is working hard, which is when the number matters.

10. **Author a recurring price in the app — BUILT** (2026-09-26). Since
    0.16.0 `authoring::Price` is a closed choice of FOUR — `Flat`, `Deadline`,
    `Recur(Cycle)` and `Periodic(Cycle)` — and the CLI, the verbs and a
    changeset could already author the last two; only the add sheet
    (`web/src/todo/addsheet.ts`) and the edit form (`web/src/todo/editform.ts`) still
    offered a flat percentage or a deadline. The price control is now the
    same closed choice: "Flat", "Deadline", "After a pass" (`Recur` — the
    cycle restarts at the next `Tend`, "Did it") and "On the calendar"
    (`Periodic` — it restarts on schedule whatever is done), each of the last
    two asking for days, a from-% and a to-% (`web/src/todo/price.ts`'s `Cycle`).
    The form is a TAGGED SUM (`docs/TS-STYLE.md`) — `price.Form`, one variant
    per kind, no optional field and no boolean deciding which controls show —
    and switching kind keeps a cycle's own numbers when the other kind
    sharing its shape is picked. The wire grew two arms to match
    (`wire::Priority::Recur`/`Periodic`, tagged by `recurDays`/`periodicDays`
    rather than by field presence alone, since the two are otherwise the same
    shape), parsed in the one place `form::priority` already parsed the other
    two. The edit form opens a recurring todo with its cycle already filled
    in — read off `Authored.spec`, the FPL print every other reader of a
    todo's price already gets (`web/src/todo/recur.ts`'s `priceOf`, the reverse of
    `authoring::Cycle::ramp`) — rather than a fresh default, because the
    cycle was already on the chain and nothing needed adding to the wire for
    that half. Once added, a recurring todo behaves exactly as the CLI's
    already did: its row offers "Did it" instead of "Done" (`todos.finish`)
    and tapping it leaves the row open, restarted.

## 5. What should go, or must not arrive

Written down so the rejection is a fact and not a memory.

- **The Briefings destination.** Retired with the mechanism on 2026-09-11. The
  files and `/api/briefings*` keep serving what is on disk; the sheet entry
  goes, and the old days are an archive link under Secretary.
- **A compose box.** Sending is a tap on a proposal the secretary wrote and he
  read whole. A compose box would be a second path to the transport, which is
  the one path this repository keeps at one.
- **Snooze as state.** Refused in item 4 above. The curve is the state.
- **Drag to reorder.** The order is the fold. A hand-placed order would be a
  number nobody computed.
- **A settings page.** Theme goes to About (audit finding 6). Nothing else here
  is a preference: the deployment is `instance.toml`, and the phone has no
  configuration of its own beyond the replica it holds.
- **A second chain switcher.** `Project` is a destination and the segmented
  control in the sheet is for a third chain. What is still to decide is what
  the control reads while Project is active (audit finding 5) — not whether
  there should be two.
- **A service worker for data.** Rejected on 2026-09-04 and still: a cached
  shell serving an old fold silently would put a number on screen with no
  provenance, and naming the staleness is the design. Since 2026-09-27 a worker
  keeps the SHELL only (`web/src/sw.ts`), one build's files under a cache named
  by that build, and offers a newer one (`web/src/app/newbuild.ts`); the API is
  never its business.

## 6. What does not move

The design rules a change is judged against. Each is enforced somewhere; this
list is so a reader knows where.

- **One evaluator.** The browser computes no fulfillment of its own. It runs the
  same core, checks the server's numbers, and draws a gap rather than
  interpolating where it has no knot. (`web/src/prodrome/core.ts`, `series.ts`;
  `web/e2e/core.test.ts`.)
- **Everything is a fold of the chain**, and the phone is a replica of it. There
  is no client state that is not derivable from objects it holds.
  (`store.ts`, `replica.ts`; SPEC §6.2's law.)
- **Local first, and staleness is named.** The last fold is drawn before the
  fetch, and a page that cannot confirm it says so with a date. (`cache.ts`,
  `stale.ts`.)
- **The tap carries an index, never a body.** What is performed is what was
  stored and shown. (`secretary::tap`; `app/cli/tests/writes.rs`.)
- **`dom.ts` never parses markup; `rich.ts` is the one door**, and the build
  refuses a second. (`scripts/build-web.sh`.)
- **Five slots on the bar**, and which four is the rule "what a phone opens the
  app FOR". (`nav.ts`; node-tested.)
- **The types are the server's.** Every payload is generated from `wire.rs` and
  pinned; `api.ts` keeps the runtime guards because a generated type is erased.
  (`web/src/wire.ts`.)
- **The app shows its working.** Every number can be explained and every claim
  about the chain can be checked from the tab. Moving that below the fold is
  fine; removing it is not.

## 7. Order

The audit's six findings first, in its order — they are rearrangements of what
is already rendered and they are felt on every visit. Then, from §4: complete
from the list (1), then **usage (9)** — out of numerical order because it is
about the constraint that stopped work twice on 2026-09-11 and the owner asked for
it by name, and because it is a new capability with a privacy boundary that is
better landed alone than beside a UI change — then edit and tick together
(2, 3) since they share the `update` arm, then the brief card (7) and Ask from anywhere (8), which are small
and change the morning. Then defer (4) and the two filters (5), which touch
`todos.ts` and the graph and want their own tests. Search everything (6) last,
because it is the one that grows the palette's contract. Briefings' retirement
(§5) rides with the About destination, since one sheet edit does both.

Every step keeps the gate — `web-test.sh`, `web-e2e.sh`, and the gallery
re-taken and LOOKED AT, because a design pass is judged by a person looking at
it and not by the tests passing.
