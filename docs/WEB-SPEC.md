# WEB-SPEC — the web app's own contract

The Prodrome's `SPEC.md`, for `web/` (`docs/WEBALGEBRA-PLAN-2026-09-16.md`):
the types, the three doors untyped data enters by, the laws (each naming its
test), and the vectors the Rust writes that the TypeScript must agree with.
A sentence with no test is not a law; a test with no sentence is an example.
Checked by `scripts/web-test.sh` (node, no DOM) or `scripts/web-e2e.sh` (a
real browser); each section says which.

## 1. The types

- **`Route`** (`web/src/shell/route.ts`): the address, a closed sum, one arm per
  page. `parseRoute` reads the hash, `hrefOf` prints it; `unknown` carries
  the hash it could not read.
- **`Screen`** (`web/src/shell/layout.ts`): a `Route` resolved by `screenFor`
  against whether the viewport is wide (`isWide`, at `WIDE_MIN_PX`). A todo is
  a pane beside the list at 62rem and up, a page below. Every other route is
  its own screen.
- **`app.App`** (`web/src/app/app.ts`): the shell's whole state — the route
  and whether the viewport is wide (so the screen is `app.screen`), the chain
  the tab reads (a list route pins it), who it is (`Asking | Unmet |
  Box | Reads(login) | Writes(login, actor, key)`, parsed once from `/api/me`
  by `login.of`, `web/src/app/login.ts`; a writer's `key` is
  `Unchecked | Trusted | Untrusted | Revoked | Unsupported(e)`, read from the
  replica by `signingHere` after `/api/me` and after each change the replica
  takes), the chains the box serves, the Inbox count, whether the
  More sheet is open, the event stream (the heads it last heard and the live
  dot's `Link`: `Quiet | Connected | Named | Off`) and the
  build's line (`shell.Line`). One pure `app.step` over the closed `app.Event`
  sum; the tab's own is `app.tab`, dispatched by the shell (`main.ts`) and
  read by every other module (`app.now`, `app.hear` for one derived value).
  Its derived values: `screen`, `writes`, `actor`, `reach`, `live`,
  `apiBase` (the one place a chain becomes a URL). Left out, each its own
  value with its own laws: the fold (`onscreen.Fold`), the replica and its sync, a
  page's own state, the panel's and the palette's openness, the theme.
- **`chrome.Chrome`** (`web/src/shell/chrome.ts`): what the shell draws
  around the page, as plain data — `nav.View` (`web/src/shell/nav.ts`), the
  panel's `Placement`, the live dot, `stale.Banner`
  (`web/src/shell/stale.ts`), what the readings left out (`excluded.T`,
  `web/src/prodrome/excluded.ts`) and the build's line (`shell.Line`) — made by
  `chrome.of(app, seen)`, where `seen` is the fold the banner dates and the
  join of what each reading left out, and painted part by part by `chrome.painter`, each part only when
  its value moved.
- **`pages.table`** (`web/src/pages/table.ts`): one mount per `Screen` tag,
  typed by that tag's arm; the router looks a screen up there, so a new
  screen is a compile error at the table until it is routed.
- **The wire** (`web/src/wire/generated/`, from `app/core/src/wire.rs`): every
  payload the app reads, declared once. `wire.ts` is its types and `payload.ts`
  a decoder per answer, folded from the same text (`suzatary_core::wire::shape`)
  into `d`'s combinators (`web/src/wire/decoder.ts`), each annotated with its
  type, so tsc refuses a decoder that misread it. `web/src/wire/api.ts` names
  each route once, over `web/src/wire/http.ts`.
- **`onscreen.Fold`** (`web/src/app/store.ts`): the fold on screen,
  `Unloaded | Shown{entries, at, by}`, where `by` says who read the rows:
  `Box{heads, fetchedAt, next, fresh}`, `Local{heads, next, fresh}` (the tab's
  own core over its replica) or `Cached{fetchedAt}` (the device's copy of an
  earlier box fold, which knows no `next` and is never fresh). One pure
  `onscreen.step`; `store` is it as a source, and every page reads the fold
  through `onscreen.entries`, `at`, `entry`, `next`, `fresh` and `loaded`.
- **`Replica`** (`web/src/prodrome/replica.ts`) and **`heads.Heads`**
  (`web/src/prodrome/heads.ts`): the objects this tab holds, the heads they end
  at, the box's heads as last heard, what the box is known to hold, what it
  refused and the prodromes it dropped. Replicas are the free join-semilattice
  on the box's objects, and `join` is their one join (§3.18). An act is never sealed here: `sync.appendAct` hands the event to the
  core's `Todos.append`, whose `Change` rests on its todo's heads in this
  replica's fold, and the replica takes the objects it answers. A head set is
  sorted and compared whole (`heads.same`); nothing picks one head, so
  the receipt, `/events` and the banner all speak sets. A pull names what the
  replica holds by `held.frontier` (2026-10-25, ARCHITECTURE §5): the token
  the box's last answer named, never a digest the tab computes, joined as
  the later by (era, count, digest), its era moved by an answer that did not
  know the token asked from (`replica.answered`), so `since=` is 64
  characters for any replica (`web/test/frontier.test.ts`). A piece of a
  store's pull is kept standing where the question it answers stood, so its
  era is read against that frontier, and a row whose frontier alone moved is
  a step (`stores.same`), since the next pull asks from it.
- **A store's row** (`web/src/prodrome/locked.ts`): every store the tab holds
  (the inbox, the lines and the two topic stores) is one `locked.Row` in the
  core's worker, its `Replica` and the keys of its words (none for a store
  that locks no words), each held by one engine (`engine.ts`, the set of
  them `storeset.ts`). A `Held` event joins by the row's join (§3.18); one that
  adds nothing (the same objects, heads and policy, `replica.same`) returns the
  same value, and one that changes only the policy is a step
  (`web/test/stores.test.ts`). The page asks every
  reading of the core's worker (`questions.ts`), which holds the stores by
  what changed and reads the lines through a memoised fold: the memoised
  reading is the plain one (`app/core/tests/held_lines.rs`
  `the_memoised_reading_is_the_plain_one`), and one new line computes its
  leaf, its conversation and the store
  (`a_new_line_costs_its_leaf_its_conversation_and_the_store`).
  `chat/dressed.ts` dresses a conversation's reading with what only the box
  says, each asked by content.
- **`lo.T<V>`** (`web/src/algebra/lo.ts`): a value on its way — what a page knows
  around its first load, and any payload a page asks for — a closed sum
  tagged on `type`: `loading`, `val(v)`, `error(e)`, with its algebra:
  `fold`, `map`, `bind`, `flatten`, `combine` (a monoid; the first error
  outranks loading, which outranks values), `withDefault` and `of` (a
  promise settled into one). A value with phases of its own nests a second
  sum in `V`, never a boolean beside this one.
- **`source.Source<S, E>`** (`web/src/algebra/source.ts`): `state()`,
  `subscribe(fn)`, `dispatch(event)` over a pure `step`, built by
  `source.create(initial, step)`. The listener reads `(state, event)`,
  because a page may redraw per event kind (`chat/transcript.ts`
  sticks the scroll only on `window`/`newest`, animates only on `delta`).
  The effects that turn the world into events sit beside a source and are
  a page's only impure code. `store.ts` and `chat/source.ts` are
  both a source plus effects (`store.ts`'s methods dispatch a closed
  `StoreEvent`; the conversation's `start`/`refresh`/`earlier`/`say`/
  `destroy` call `fetch` and the stream).
- **`frame.Page<S, E, K>`** (`web/src/view/frame.ts`): a page declared once, a
  value — what it `reads` (a `reading.Reading<S>`: a replica the tab holds,
  a constant, code that arrives once, or questions `asked` holds), how its
  own events move the value (`step`), what of its own state outlives a new
  reading (`keeps`, by default nothing), how it looks (`view(v, dispatch,
  again, at)`, a function
  of the value), what stands above it drawn once (`now(dispatch)`), what
  stands around it drawn once (`around`, a column beside it) and a route
  change it takes on itself (`absorb`) — mounted by `frame.mount` (or
  `frame.open`, the same mount returned synchronously, for a pane that swaps
  one page for another) over one `source.create(held, frame.step(ownStep,
  keeps))`, `held` being what its readings hold when it opens, so a page read
  before draws its value first; what its listener dispatches while it opens
  is folded in and drawn once. `frame.Event<S, E>` is the closed sum the
  frame folds: `read(l)`, a new reading, and `own(event)`. `frame.step` is
  the one transition; `paint` is idempotent. A page loads nothing: what the
  tab asks the box it holds no replica of is `asked` (`web/src/app/asked.ts`),
  one register per question whose answers are ordered by the ask that brought
  each, so an older answer never paints over a newer one by the register's
  order, not by a guard (§3.8). A read of the box is cancelled at thirty
  seconds (`http.WAIT_MS`), never the tab's own work; what is
  shown is the reading's `behind` when an ask failed after it, or one has
  been on its way over a second and a half, said under the heading with when
  the shown answer landed ("As of …", "Could not refresh: … at …", with a
  Retry), on the status line alone, never the body. Drawing runs no handler: a form keeps its draft in a register
  (`form.drafts`, a tab register keyed by its row), mounts on it and follows
  it, so a body drawn again draws what was typed; a text draft typed outside a
  form is carried into its counterpart (`web/src/view/typed.ts`, the field of
  the same kind in the row of the same `data-key`, never the one at the same
  place, with no event); a form off the page lets go of its draft. —
  `typed.test.ts` "law: carrying what was typed into a body drawn again
  dispatches nothing…" and "law: the field holding the keyboard hands it to
  its counterpart, typed into or not", `form.test.ts` "law: a form drawn
  again mounts on what was typed into the one shown…", "law: a form no
  longer shown writes nothing to the draft…", "law: a clearing form gives
  up what it sends as it sends it…" and "law: whatever is drawn, left and put back, every keystroke into the form
  shown is its draft, at a bounded cost", `islands.test.ts` "law: a
  change to what is behind
  alone draws the status line…", `writes.test.ts` "ticking a subtodo moves
  the chip" (one write, it stays ticked), `e2e/inbox.test.ts` "a
  reason being typed stays in its own card while the card above it goes".
- **A page waiting says nothing.** Until its first value, and while a
  value it read still waits on more (`waits`: the Inbox while the box is
  asked for its folds, the list while the chain folds, a search on its
  way), the frame draws one placeholder (`web/src/view/placeholder.ts`): the
  loading mark (`loadmark.ts`, one swappable piece) over grey bars in the
  shape the page names (`Lines`, `Rows`, `Cards`, `Turns`), with no words,
  shown by the stylesheet only 300 ms after it is placed. It is the same
  node from the page's wait through the value's, however often the status
  line is drawn, so a redraw or a handoff never starts its 300 ms again;
  a part waiting on its own (the thirty days, the conversations beside a
  conversation) keeps one node for its wait the same way. A wait beside a
  control the person pressed (a card's tap, an erasure) is the mark alone,
  shown at once; the card or turn is drawn again when its ask is taken,
  and the mark's fill starts again with it. The column's state class is
  `pagewaiting`, never the card's `.waiting`. A todo's or a conversation's title is a grey bar while
  the page waits and never its id, and no bar stands beside a failure. The
  mark holds still with reduced motion and draws in each theme from the
  token roles. — `islands.test.ts` "law: while a page waits it draws its
  placeholder…" and "law: a value that still waits keeps the placeholder…";
  `e2e/waiting.test.ts` "a page on its way shows its placeholder after
  300 ms with no words, still with reduced motion, in both themes" and "a
  page waiting on a search keeps its whole column…".
  A page may
  declare named **islands** (`frame.Island<S>`: a `node`, `update(value)`,
  `destroy`) beside its body: built once when the page mounts, handed each
  new value, destroyed with the page, and placed by the view (`at.<name>`)
  where the body redraw never takes their nodes out. An island keeps only
  what the DOM owns (scroll, focus and cursor, an unsent draft, a half-typed
  edit, a streaming transcript's nodes); anything the app decides on is in the
  value and goes through the step. A `masthead` may be a node drawn from the
  value in place of the title's heading. Instances:
  about, briefings, briefing, chain, changed, conversation, doc, inbox,
  mail, memory, message, secretary, settings, thread, the todo page
  (`web/src/todo/todoview.ts`, also the pane beside the list), the todos
  list, usage and the not-found page.
- **`act.T`** (`web/src/view/act.ts`): an act button — a label, a class and
  one effect answering `act.Outcome` (`Again | Spent`) — drawn by the one
  `act.view`, disabled while its effect runs. `act.Gate` (`Writes | Reads`)
  is who this tab writes as (`App.who`), and `act.offered` is the
  only way a page chooses between its acts and what a reader is told instead.
- **`form.T<F>`** (`web/src/view/form.ts`): a form as a value — its named
  fields, each a `form.Field` (`Line | Lines | Secret | Day | Choice | Pick |
  Toggle | Price`, tagged on `type`, holding what the owner typed as `v`), and
  what was `said` of the last submit (`Quiet | Refused`). `form.step` is its
  one transition over `Typed(name, v) | Refused | Sent | Reset`; `Sent`
  clears every `Secret`. `form.mount` builds each control once and paints it
  from every value the form's source commits in which its field moved (so a
  value autofill set without an event is left alone, and pulled before a
  submit); the caller lays out the fields,
  the refusal line and the button. What a form submits is its `read` of the
  fields, a **`parsed.T<B>`** (`web/src/algebra/parsed.ts`: `Ok(v) |
  Refused(reason)`, a monad), typed as the request it becomes (`NewTodo`,
  `UpdateTodo`, a settings body). A refusal, the form's own reading or the
  box's answer, is kept in `said`, drawn beside the button until the next
  submit and said once in the toast. Instances: the add sheet
  (`todo/draft.ts`, `todo/addsheet.ts`), the todo page's edit and defer forms
  (`todo/edit.ts`, `todo/editform.ts`), and the settings page's backend
  set-up and model adder behind Change (`pages/models.ts`).
- **`chat.Chat`** (`web/src/chat/chat.ts`): one conversation on screen — its
  source, its transcript in a given scroll box and its composer — built by
  `chat.mount` for the conversation page and the 62rem panel.
- **`kv.Kv`** (`web/src/app/kv.ts`) and **`time`** (`web/src/app/time.ts`):
  the one door to the browser's storage (a throwing accessor reads as absent
  and loses the write) and the one read of the clock, with the wire's instant
  printers and parser.
- **`briefing.Day`** (`web/src/pages/briefing.ts`): `brief | saved | none`,
  what `briefing.day(date, fetch)` answers. **`usage.credits.Credits`**
  (`web/src/pages/usage.ts`): `disabled | spent | unspent`, what
  `usage.credits.of(window, refusal)` answers before anything draws it.
- **`ViewReference`** (`web/src/chat/inview.ts`), **`ComposeDecision`**
  (`web/src/chat/compose.ts`), **`Token`** (`web/src/chat/refs.ts`), **`Op`**
  (`web/src/algebra/reconcile.ts`), **`Round`** (`web/src/chat/rounds.ts`): the smaller
  sums decisions are spelled in. A decision is a named value, never a
  boolean.

## 2. The doors

Three; the build enforces the count:

1. **`web/src/view/dom.ts`** — `el` and text nodes. Markup is never parsed from
   a string; `scripts/build-web.sh` refuses a source assigning `innerHTML`.
2. **`web/src/rich.ts`** — the one `innerHTML`: typst's HTML, and only
   HTML the box compiled, after the tripwire (`suzatary_core::tripwire`)
   refused any script, handler or executable URL, or a view the
   tab compiled (`rich.checked`, below). Five kinds come through it:
   - a trusted actor's body and detail, compiled in the closed world (the
     Prodrome's `Project::restricted()`: no `html`, no `plugin`); an untrusted
     record arrives as source and is drawn as text;
   - a secretary's answer, block by block, compiled in the same closed world
     (a block that does not compile there is text);
   - a page the box printed (`suzatary_core::reading`): a todo's reading part
     with "Why this number", every datum a Typst string literal;
   - the Inbox's decision prose, and a day's briefing;
   - a Typst view the tab compiled with typst-wasm (`#/typst`, stages 0 and 4
     of `docs/APP-AS-READING-PLAN-2026-10-02.md`): the kit (`web/typst/`)
     over the nodes of one state value whose every text is a JSON string,
     compiled a wave of nodes at a time, each wave checked by the box's own
     tripwire through the core (`suspicious`, no TypeScript copy); a wave
     that trips is compiled again node by node, and a node that trips is
     refused alone, said in its place. Its controls carry `data-intent`,
     parsed by one listener against a closed vocabulary (§3.13);
   - the Typst editor's preview (§3.14): the text being typed, compiled in a
     worker by the same restricted project the box admits a text by, through
     the same tripwire and `rich.checked`.

   The page never sends text to be rendered: an answer block is asked for by
   its sha, a reading page by todo id, a briefing by date; bodies and Inbox
   prose arrive rendered beside their source. A reading page lives in the
   box's bounded memo, never on disk (it prints the clock); bodies and answer
   blocks are cached on disk by content. Offline, a page never fetched is its
   plain text.
3. **The generated decoders** (`web/src/wire/generated/payload.ts`), through
   `http.get`/`http.act` or `d.json` — the only way wire bytes (and the wasm's
   answers, in `core.ts`) become a typed value. A missing field is a break,
   refused naming it; an extra field is not, and is dropped
   (`web/test/decoder.test.ts` for `d`; `web/test/payload.test.ts` over
   `app/core/tests/web/answers.json`, a sample of every type a route answers,
   each naming its maps, whose keys are data and not fields).

## 3. The laws

Properties run over `web/test/gen.ts`, a seeded generator; the failing seed
is in the assertion.

### 3.1 The address

- **Round trip.** `parseRoute(hrefOf(r)) = r` for every route. —
  `laws.test.ts` "a route survives print and parse".
- **Totality.** Every hash parses; an unreadable one is `unknown` carrying
  the hash. — `laws.test.ts` "parsing is total".

### 3.2 The screen

- **Totality and the pane rule.** `screenFor` answers for every route, wide
  or not; a `todo` route is `{todos, selected}` at 62rem and up and `todo`
  below; a narrow list selects nothing. — `laws.test.ts` "screenFor is
  total".
- **Determinism.** Same route, same width, same screen. — `laws.test.ts`
  "the same route at the same width".

### 3.3 The reconciler

- **Identity.** `reconcile(k, k) = []`. — `laws.test.ts` "reconciling a
  list with itself is no work".
- **Correctness.** Applying the plan to the old order yields exactly the new
  order. — `laws.test.ts` "applying the plan yields exactly the new order";
  `reconcile.test.ts` (examples and minimality).
- **Well-formedness.** No key named twice; inserts are new keys, moves and
  removes old ones. — `laws.test.ts` "a plan never names a key twice".

### 3.4 The fold

Which fold is shown is decided by who read it and what it read, never by
which answered last.

- **The tab's own reading is the truth while it holds objects.** A box fold
  never replaces a local one, and a local one replaces a box fold, so a box
  answer asked before a push and landing after the local fold cannot hide the
  act; the tab pulls what the box named and folds again. — `store.test.ts`
  "law: the box's fold never replaces the tab's own".
- **Of two folds by one reader, the later in the causal order stays,** in
  either arrival order: each fold is handed in with the order on head sets
  read off the replica held then (`replica.ancestry`), so a straight line
  `{h1}` below `{h2}` is an order, never a tie. Of two concurrent ones the
  one shown stays; where the tab holds none of what they name, the later
  answer is taken, since one reader's answers are asked one at a time. —
  `store.test.ts` "law: of two folds by one reader", "law: of two
  concurrent folds".
- **A cached fold only boots an empty tab,** and is never fresh. —
  `store.test.ts` "law: a cached fold only boots an empty tab".
- **Stale and reset.** `Stale` keeps the rows and drops only the claim that
  they are current; `Reset` is `Unloaded`; both idempotent. — `store.test.ts`
  "law: stale keeps the rows…".
- **Total.** Rows exactly while shown, one per todo, each found by its id. —
  `store.test.ts` "law: a fold is total".

### 3.5 The conversation

- **A conversation is a reading of the replicas.** Its window, the list, the
  Inbox and a search are the module's readings of the objects the tab holds,
  the same functions the box applies to its own stores: equal objects read
  equal bytes, in any order they arrived. — `replicas.test.ts` (four laws,
  seeded); `app/cli/tests/stores.rs`.
- **A turn whose words are gone is drawn by its shape.** Who, when, the verb
  and the references stand, and its text is the box's bracketed sentence
  (`TurnRow.gone`: asked, forgot or missing), muted, with no Reply and no
  Erase; never an empty bubble. A key a tab still holds opens nothing an
  erasure names. — `replicas.test.ts` "an erased slot reads as erased
  whatever key a tab still holds…"; `e2e/lines.test.ts`.
- **A conversation on its way is never empty.** Until its first window is
  read, the transcript draws the turn-shaped placeholder above the composer,
  at the foot. — `e2e/waiting.test.ts` "a conversation on its way shows
  turns above the composer…".
- **A question the box holds is thinking.** From the moment the window says
  the conversation is queued or running, the answer's place reads "Thinking ·
  N s" beside Stop, its count drawn again each second, and a person's turn
  carries no queued mark. — `answer.test.ts` "a question the box holds is
  thinking from the moment it is seen…".
- **One field above the conversations.** The search, with New topic beside
  it, which opens a draft the box holds nothing of: its first message opens
  the conversation under the name typed over the composer, so no
  conversation stands that nobody spoke in (`e2e/topic.test.ts` "a new topic
  is named, asked and answered…"); a topic with no name reads "New
  topic" until the box names it, and a last turn is its minute today,
  "yesterday", or its day, read against an instant the list holds and
  moves when the owner's day turns. No placeholder is cut at 390 or 1440
  px. — `chatlist.test.ts` "the list's instant moves…";
  `e2e/waiting.test.ts` "the conversations have one field above them, and
  no placeholder is cut, at 390 and 1440 px".
- **A section draws a page of its newest rows.** Topics and reviews each
  draw their newest 30, with "Show older · N" beneath, which draws 30 more;
  another filter starts again at one page, and a listing keeps how far the
  reader went. A store of thousands of conversations is never drawn whole,
  and the search still reads every one. — `chatlist.test.ts` "a section
  shows a page of its newest rows…".
- **A conversation draws in slices and is placed once.** A redraw reads its
  state once and draws the rounds newest first, at least one a slice and
  for at most 4 ms, from where the last slice stopped; when the last is
  drawn, the answer's place is placed in the next frame, which lays the
  drawn rounds out once (never a task reading the layout ahead of it), and
  the page marks `transcript:placed` (`chat/marks.ts`). A hidden tab, whose tasks
  the browser throttles, draws and places in one pass, and a tab hidden
  mid-way draws the rest at once. — `e2e/bigstore.test.ts`;
  `slices.test.ts` "a hidden tab draws the pass in one pass…".
- **The worker groups; the page holds one answer.** A page asks for every
  turn it holds, from its first on, and an older window for those before
  them too (`source.asking`); the core's worker reads back to them
  (`grouped.covering`) and answers grouped into rounds, each by the
  indices of its turns and cards, with its day and label
  (`chat/grouped.ts`). The page holds that answer whole and draws the rounds
  as given, never a join of windows, so a round is never split however long
  it runs. — `grouped.test.ts` "law: whatever a page reads, it holds every
  turn from its first to the newest, never fewer, grouped".
- **A write's answer is never drawn.** A person's turn, Stop, Regenerate, an
  edit and a card's answer are intents (§3.15); the page reads the replicas
  once they hold them. — `source.test.ts` "say reads the window again once
  the box took the turn, never its answer" and "stop asks the box once and
  reads the window again".

- **Rounds.** A round ends at a model turn with no calls; a proposal
  belongs to the last round begun by the turn it follows (the first when
  none was), so to the round holding that turn, and is drawn once; an empty
  answer is a round with no answer to draw. — `rounds.test.ts`, "law: every
  card is drawn once…"; the boundary is pinned
  against the Rust's over `app/core/tests/about-fixture.json`.
- **Draw is keyed.** A new round appends; earlier rounds' nodes are the
  same nodes. — `web/e2e/secretary.test.ts` (node identity across an
  answer).
- **One door to the secretary on each width.** On a phone the Secretary tab
  and a page's own "Ask the secretary"; at 62rem and up the panel, whose
  toggle carries what the page shows, and no page link. — `chrome.test.ts`
  "law: one door to the secretary on each width…".
- **A frame scrolls its own box.** Mounting a panel leaves the
  document's scroll where it was. — `web/e2e/secretary.test.ts` (the panel
  scene's scroll assertion).
- **The panel exists only where it is shown.** Below 62rem it is out of
  the document with no source open, and it is never beside the secretary's
  own pages (wherever `nav.activeFor` is `secretary`: the list, a
  conversation, the memory). — `web/e2e/secretary.test.ts` (the panel scene's last
  assertion; the desktop conversation page scene).
- **One grid at 62rem and up.** The header spans the window, so the brand
  stands in the same place on every page; the page starts at the brand's
  edge and runs to the open panel or the window's edge; a todo beside the
  list is the wider column at every wide width; the panel opens by itself
  only on a page about one thing (a todo, a mail, a thread, a briefing)
  whose conversation holds talk about it, a page of many things keeps it
  closed, and a press, open or close, holds for that page alone in this
  tab, a page of many things keyed by its destination; nothing about the
  panel is kept on the device (`chat/aside.ts`, one rule over the route and
  the conversation). Where the panel stands, a page's own "Ask the
  secretary" is not drawn, read from the panel and not from a width. — `aside.test.ts`; `web/e2e/desktop.test.ts` "at 1440 px
  the header holds still…" and "at 1100 px with the panel open…".
- **The panel's toggle has one place.** Open or closed, the toggle's box is
  the same, and a closed panel takes no width; the Todos page at 62rem and
  up fits the window, so only its panes scroll. — `web/e2e/desktop.test.ts`
  "the panel's toggle keeps its place…".
- **A composer is on screen after a send, at every width.** On the
  conversation page it sticks to the foot of the document it scrolls, above
  the phone's tab bar (`--tabbar`). — `web/e2e/secretary.test.ts` "a writer
  sends from the conversation page at desktop and phone width…" and the
  panel scene's sends.
- **A tab that may not write says so where the composer would be:** one
  status line (`app.reach`) naming what this device may do and where to
  write from, with a link to Settings for a key the chain does not trust or
  has revoked, and nothing before `/api/me` answers. — `app.test.ts` "a tab
  that may not write says so…".
- **A conditional rule never loses the cascade.** A property an `@media`,
  `@supports` or `@container` block sets is not set again, by itself or by
  a shorthand of it, for the same selector by a later base rule. —
  `cascade.test.ts`.
- **Block sources concatenate to the answer**, and a block is named by the
  sha256 of its source. — `app/core/src/secretary/answer.rs` (proptest);
  `answer.test.ts` over `app/core/tests/web/blocks.json`.
- **`answer.step` reaches only legal states:** blocks close in order, and
  the closed sources plus the tail are exactly what was streamed; a block
  frame that does not name the tail's prefix changes nothing. —
  `answer.test.ts` "law: answer.step reaches only legal states", over
  `gen.ts`; "the page agrees" streams `blocks.json` in random fragments.
- **Nothing that grows moves the box.** A send holds its question at the
  top; a new answer under a drifted-up box is the pill, never a scroll. —
  `anchor.test.ts` "law: nothing that grows moves the box…";
  `web/e2e/secretary.test.ts` "streaming never scrolls a drifted-up box".
- **What the reader did since the tap stands.** A question is asked at the
  tap and its turn known when the box answers; a send holds its question
  only while it is still being asked. — `anchor.test.ts` "law: a send whose
  turn is known after the reader scrolled away holds nothing".
- **The reader's scroll settles the hold, whatever held it; the page's
  settles nothing.** Who moved the box is read from the reader's input
  (wheel, touch, a scroll key, a press on the scrollbar), never from the
  scroll: the box's own placing, a clamp as the room under a question goes
  and the browser anchoring the box against content drawn above are the
  page's. — `anchor.test.ts` "law: the reader's scroll settles the hold,
  whatever held it; the page's settles nothing"; `web/e2e/secretary.test.ts`
  "content drawn above a held question keeps it held, and the reader's
  scroll up drifts for good".
- **A set-aside turn is drawn once, in its round's earlier attempt.** —
  `rounds.test.ts` "law: every turn is drawn once…".
- **A draft reads back what was written, per conversation; a store that
  throws loses the draft, never the page.** — `anchor.test.ts`.

### 3.6 Compose and keys

- **Enter's meaning is the width's.** Ctrl/⌘-Enter sends everywhere; bare
  Enter sends at 62rem and up and breaks a line below; Shift-Enter breaks a
  line. — `compose.test.ts`, every row of the table.
- **A field keeps the keyboard.** An input, textarea or select yields
  nothing to the table but Escape. — `keys.test.ts`.
- **The box empties when a draft leaves, not when the box answers.** Send
  takes the box's words, chips and files as one draft and empties the box at
  once; what is typed while the send is in flight is the next draft, and a
  failed send hands its draft back (and its Editing mode) only into an empty
  box. The sent words are never a saved draft. — `composer.test.ts` "law: a
  sent draft leaves the box at the click…" and "law: a failed send comes
  back…"; `web/e2e/secretary.test.ts` "the next question typed before a send
  settles stays in the box, and both are sent".
- **The composer is a value.** `composing.T` (`web/src/chat/composing.ts`)
  holds the draft, the mode, the uploads on their way (a set of pick ids)
  and the files held back, stepped only by `composing.step` over the closed
  `composing.Event`; the composer draws it idempotently, each part only when
  its slice is a new value, and writes its words to the draft register once
  per change, also after it is gone, so a send failing then hands its words
  back to the conversation's draft. Its chips are a set by spelling and its
  files by id; the uploads on their way are exactly the picks no answer has
  named; keeping or dropping a held file takes that file alone, and a file
  being kept stays so until its own answer; a send that is not ready, or a
  step that moves nothing, is the same value. — `composing.test.ts`, its
  laws over any sequence of events; `composer.test.ts` "law: a send that
  fails after the composer is gone…".

### 3.7 The source

- **A source is the fold of its events.** After dispatching any sequence,
  the state is `events.reduce(step, initial)`. — `sourcelaws.test.ts`
  "law: a source is the fold of its events".
- **Listeners see each committed state once, in order,** and nothing after
  unsubscribing. — `sourcelaws.test.ts` "law: a listener sees every
  committed state once, in order, and none after unsubscribing".
- **A no-op event commits nothing.** A step returning the same value wakes
  no listener. — `sourcelaws.test.ts` "law: an event that changes
  nothing wakes nobody".

### 3.8 The page frame

`lo`:

- **A functor.** `map` preserves identity and composition. — `lo.test.ts`
  "law: map preserves identity and composition".
- **A monad.** `bind` has `val` as unit on both sides and is associative;
  `flatten` undoes `map(val)` and `bind` is `flatten` after `map`. —
  `lo.test.ts` "law: bind has val as unit on both sides and is
  associative", "law: flatten undoes map(val), and bind is flatten after
  map".
- **An arm without a value is not rebuilt.** `map` and `bind` over
  `loading` or an `error` return the same object, so a source wakes
  nobody. — `lo.test.ts` "law: an arm without a value passes through map
  and bind as the same object".
- **The fold is the sum.** Folding with the constructors rebuilds the value.
  — `lo.test.ts` "law: folding with the constructors rebuilds the value".
- **`withDefault` takes back what `val` put in, natural in `map`.**
  `withDefault(val(n), d) = n`; `withDefault(map(l, f), null)` is `f` of
  `withDefault(l, null)`, `null` staying `null`. — `lo.test.ts` "law:
  withDefault takes back what val put in, and is natural in map".
- **`combine` is a monoid.** `combine()` is `val([])`, `combine(a)` is
  `map(a, x => [x])`, and combining two combines is combining the
  concatenation, flattened. — `lo.test.ts` "law: combine lifts one value,
  is a homomorphism from concatenation, and combine() = val([])".
- **`combine` decides by precedence.** The first error, else loading if
  any is, else every value in order. — `lo.test.ts` "law: combine is the
  first error, else loading if any is, else every value in order".

`frame`:

- **A value read replaces whatever was shown.** `step(s, read(val(v))) =
  val(v)`, after a failure included. — `pageframe.test.ts` "law: a value read
  replaces whatever was shown".
- **A failed or waiting reading never takes a value off the screen.** —
  `pageframe.test.ts` "law: a failed or waiting reading never takes a value
  off the screen".
- **An own event moves only a value.** `step(s, own(e)) = map(s, v =>
  ownStep(v, e))`, and before a value it is the same object, so nobody
  wakes. — `pageframe.test.ts` "law: an own event before a value returns
  the same object, so nobody wakes; after one it moves the value".
- **`mapOwn` is a functor, and `step` is natural in it.** —
  `pageframe.test.ts` "law: mapOwn is a functor, and step is natural in it".
- **The fold is the sum.** Folding an event with the constructors rebuilds
  it. — `pageframe.test.ts` "law: folding an event with the constructors
  rebuilds it".
- **A failed reading and an own event commute.** — `pageframe.test.ts`
  "law: a failed reading and an own event commute".
- **A page keeps what it says it keeps.** With `keeps`, a value read over a
  value is `keeps(was, now)`; with nothing shown it is the value read. —
  `pageframe.test.ts` "law: a page keeps what it says it keeps…".
- **An own event that changes nothing wakes nobody:** the step returns the
  same object. — `pageframe.test.ts` "law: an own event that leaves the
  value as it was returns the same object, so nobody wakes".
- **An island lives as long as its page.** It is mounted once when the page
  mounts, whatever the page's events, is handed every value the page commits,
  in order, and is destroyed with the page, after which it is handed nothing.
  — `islands.test.ts` "law: an island is mounted once, handed every value in
  order, and destroyed with the page".
- **Nothing heard before the first value is lost.** Own events dispatched
  before a page has a value are folded into the first one, in order, when it
  lands. — `islands.test.ts` "law: own events heard before the first value
  are folded into it when it lands, in order".
- **A body redraw leaves an island alone.** Its node is never taken out of
  its place, and what the DOM holds in it (a draft) is what it was. —
  `islands.test.ts` "law: a body redraw never takes an island's node out of
  the page, and leaves what the DOM holds in it".

`reading` (`web/src/algebra/reading.ts`):

- **A functor and an applicative** on what is read now; `both` is
  `lo.combine`'s monoid on two types. — `heard.test.ts` "law: map is a
  functor on what is read now", "law: both reads each part now, as
  lo.combine's monoid".
- **A reading that did not move hands back the same object,** so the frame,
  which takes only a new reading, draws nothing for it. — `heard.test.ts`
  "law: a reading that did not move hands back the same object…".
- **`once` arrives once,** however often it is heard. — `heard.test.ts`.

`asked` (`web/src/app/asked.ts`):

- **The reading is the answer of the greatest ask answered,** whatever order
  the answers came in, and a late answer to an earlier ask never replaces a
  newer one. — `asked.test.ts` "law: the reading is the answer of the
  greatest ask answered…", "law: a late answer to an earlier ask never
  replaces a newer one".
- **A failure never takes an answer away.** — `asked.test.ts`.
- **Registers join** commutatively, associatively and idempotently. —
  `asked.test.ts`.
- **A question is asked when first read and when the box moves,** one
  register per name; one nobody reads is never asked, and asks made while one
  is in flight are one ask, made when it lands. — `asked.test.ts` "a question
  is asked when first read…".
- **What is held is behind exactly when the newest ask failed after an
  answer stood, or one is slow, dated by that answer.** — `asked.test.ts`
  "law: what is held is behind exactly when…".
- **An ask the box never answers is said slow, then fails at the limit, and
  what stood still stands.** — `asked.test.ts` "an ask long on its way is
  said slow, and one that fails leaves what stood…".
- **An answer equal to the one standing wakes nobody**, by the register's
  equality, which is the data's own (`web/src/algebra/data.ts`, field by
  field) unless the register names one. — `asked.test.ts` "an answer equal to
  the one standing wakes nobody…"; `data.test.ts` "data: an answer is the same
  as itself asked again…".
- **A shape keeps at most `KEPT` registers nobody reads,** never one being
  read. — `asked.test.ts` "a family keeps at most KEPT registers…".

`act`:

- **The gate follows who writes.** A reading login is offered what it is
  told instead, a writing one the acts; `app.hear(app.actor, …)` hears a
  change of actor once. — `act.test.ts` "law: the gate follows who
  writes…", "who writes is heard once per change of actor…".

The pages' sums:

- **`briefing.Day`'s fold is the sum.** — `briefing.test.ts` "law: folding
  a day with the constructors rebuilds it".
- **The brief wins.** A day the secretary briefed is its first brief for
  that date, wherever it stands in the list, whatever the saved files say.
- **A 404 is an answer; nothing else is.** No brief reads the saved file,
  a 404 for it is `none`, any other failure is rethrown for the frame's
  Retry. — `briefing.test.ts` "a 404 is no briefing", "any other failure is a failure, so the frame
  offers Retry".

### 3.9 The Todos tree

`docs/TODOTREE-PLAN-2026-09-25.md` and `docs/REDESIGN-PLAN-2026-09-25.md`.
The browser builds the same tree as `app/core/src/tree.rs`. Each node is a
term, and the Prodrome wasm links and evaluates it.

`fulfillment`:

- **The state is monotone in the value.** — `fulfillment.test.ts` "law:
  stateOf is monotone in the value".
- **The boundaries are 0.5 and 0.7.** — `fulfillment.test.ts` "law: 0.5 is
  the first Watch and 0.7 the first Fine".
- **`worst` is a monoid with `fine` as identity.** — `fulfillment.test.ts`
  "law: worst is a monoid with fine as its identity".
- **The colour is the design system's one scale, continuous, bad to good**:
  typst-design's `fulfillment`, crimson at 0 to teal at 1 through copper,
  amber and green, its stops printed from `tokens.typ`, linear in each OKLCH
  channel between two stops, the hue increasing; and the kit's colour of a
  value is the tab's. — `fulfillment.test.ts` "law: at each of the scale's
  stops the colour is that stop's, the ends included", "law: the colour is
  continuous and linear in each OKLCH channel between two stops, its hue
  increasing"; `typstview.test.ts` "law: the colour the kit gives a value is
  the one the tab's own pies are drawn in, the package's scale".
- **A pie's filled share is its value,** from twelve o'clock, in the value's
  colour. — `fulfillment.test.ts` "law: the pie's filled share is the value,
  in the value's colour".

`curve` (`docs/DESIGN-DECISIONS-2026-09-25.md` §F): a todo's thirty days, 15
back and 15 ahead, as the core's knots, and the one line that draws them. A
row and a group draw only today's value, as a pie; nothing on the list samples
a window.

- **The window is honoured:** the knots split at now, the first from now on
  is now's, and the scale maps the window's start and end to the frame's
  edges. — `curve.test.ts` "law: the window is honoured: split at now, drawn
  from its start to its end, now's sample first ahead".
- **The line is drawn from the core's knots:** every knot is a vertex, in
  order, and the faded past runs up to now's value, where the rest begins. —
  `curve.test.ts` "law: the line is drawn from the core's knots, every one a
  vertex in order, the halves meeting at now's value".
- **The stroke is its own value's colour:** the gradient's stop at each height
  is `colour` of the value there, red at 0 and green at 1, and now's dot is
  now's colour. — `curve.test.ts` "law: the stroke's colour at a height is the
  colour of the value there, red at 0 and green at 1".
- **The dates are short and never touch:** each is the locale's short day,
  and at phone width no two labels' boxes overlap. — `web/e2e/list.test.ts` "at
  phone width the thirty days' dates are short and no two overlap".
- **The first crossing below a threshold is at the first sample that is
  worse,** met on the straight line from the sample before it (the line's
  label). — `curve.test.ts` "law: the first crossing below a threshold is at
  the first sample that is worse, met from the one before".

`term` (a term's JSON as a tree, `web/src/todo/term.ts`: a child is a term,
a list of them, or a row's, as `TermF::children` reads it; every walk over a
term is one of its folds):

- **A Piecewise's children are its head and every piece's term.** —
  `term.test.ts` "a Piecewise's children are its head and every piece's
  term; a curve's rows hold none"; the walk it fixed, `grouping.test.ts`
  "a reference inside a deferral's piece is a child, as
  tree::refs reads every piece".
- **`nodes` is the cata that lists, root first.** — `term.test.ts` "law:
  nodes is the cata that lists, root first, and every node is reached
  through children".
- **`find` is the first of `nodes` its function answers for.** —
  `term.test.ts` "law: find answers the first node, root first, that its
  function answers for".
- **`refs` are exactly the reference nodes.** — `term.test.ts` "law: refs
  are exactly the references among the nodes, a piece's included".

`tree` and `grouping`:

- **`map` is a functor, and `scan` keeps the cata at every node.** —
  `grouping.test.ts` "law: tree's map is a functor, and scan keeps the cata
  at every node".
- **Paths are a monoid, and a segment twice is once.** — `grouping.test.ts`
  "law: paths form a monoid, and a segment twice is once"; sets of paths:
  "law: sets of paths form a monoid under the pairwise combine".
- **A group of one is its member.** — `grouping.test.ts` "law: a group of
  one is its member's term, and so its value".
- **Grouping twice is grouping once.** — `grouping.test.ts` "law: grouping
  twice by the same key is grouping once".
- **No view averages.** Every node is worth the fulfillment of its linked
  term. — `grouping.test.ts` "law: every node is worth the fulfillment of
  its linked term, and a todo its own function"; the server agrees to 1e-9
  in every grouping (the e2e scene over `/api/<chain>/tree`).
- **Urgency orders every level.** — `grouping.test.ts` "law: children ascend
  in value at every level, a term that does not link first".
- **Nothing is dropped.** — `grouping.test.ts` "law: every member is placed,
  under each of its parents in every grouping".
- **A ∅ that waits on a chain is not yet priced.** A node whose value is
  `∅` only because its term refers, directly or through its chain's own
  references, into a chain the tab does not hold reads "not yet priced",
  after `∅`. — `grouping.test.ts` "a ∅ that refers into a chain not held is
  not yet priced, through the chain's own references, and nothing else is".
- **The rows draw from the box's fold at once.** Before any reading the
  basis is the box's rows: a todo is its row's value, and a group is priced
  by the core in its worker from those values, so nothing waits on a fold of
  the objects the tab holds. — `todos.state.test.ts` "the rows draw from the
  box's fold before any reading…"; `e2e/firstrows.test.ts`.
- **The basis is the latest reading's.** Each basis is one reading, the
  open chain and every other read at one instant and linked through one
  `Linker` the core's worker keeps by the set and the instant, asked through
  `latest` (§3.19), so one landing after a later one is dropped; another
  chain arriving reprices, and a reading's basis clears a reading's problem
  and no other. A page mounted again over the same chains at the same
  instant folds nothing. — `latest.test.ts` "law: an answer landing after a
  later-asked one is dropped…"; `todos.state.test.ts` "law: a reading's
  basis clears a problem from a reading, and none about no reading";
  `worker.test.ts` "a remount does not fold…".
- **Prices are the worker's answers, as events.** The tree names the terms
  its basis has no price of (`wants`: `None`, `Wanting` or `Asking`). The
  page's effect (`frame.Page.effects`, `todosState.ask`) asks once for what
  is wanted, marking it `Asked` first, and the answer comes back as
  `Priced`, dropped when the basis has moved on. A basis is named by the
  rows' instant or by the worker's reading (chain, chains held, instant), so
  two chains read at one instant are two readings. Until the answer, the
  tree drawn before stands, so a reading landing moves no row that its
  numbers do not move; a failed ask prices what it asked as refused, so the
  rows still draw, a group that does not link marked, and the page says
  why. — `todos.state.test.ts` "a reading landing keeps the rows drawn…",
  "an answer about a basis since replaced is dropped", "a reading of another
  chain at the same instant…", "a failed worker prices what it was asked as
  refused…", "the effect asks once…", and the walk law "growing again moves
  nothing"; `worker.test.ts` "two chains held and read at one instant…".
- **The worker is the core.** Every answer by message is the in-thread
  core's for the same objects, which cross to the worker once. —
  `worker.test.ts` "law: an answer by message is the in-thread core's…",
  "a holding sends each object once…".
- **A chain the set holds as a failure is one line** above the list (and
  above the Inbox), naming the chain and why, from the fold's `failed`.

`row`:

- **Folds follow need.** An untouched fold is open exactly when not Fine;
  a touched one is the viewer's. — `row.test.ts` "law: a fold nobody
  touched is open exactly when its value is not Fine; a touched one is the
  viewer's".
- **Keys visit each line once.** — `row.test.ts` "law: the stops are each
  card's head then its visible lines, every key once".
- **One control searches, filters and groups:** the magnifier in the title
  row, shut until asked for, saying what narrows the list while shut and
  nothing for the default. — `todos.state.test.ts` "the search, the filters
  and the grouping start shut…"; `web/e2e/list.test.ts` "on a phone
  the first line starts in the top third…".

### 3.10 The Inbox

A proposal's object in the inbox prodrome (the tab's replica of it, read by
`Proposals.inbox`) is the Inbox's row, beside each open decision on a chain (`waiting.ts` is the one
algebra over both), and the card is how the owner answers it. A triage card is such a proposal,
asked by the triage agent and tagged so.

- **One order over two readings of one set.** `waiting.all` mixes a
  proposal priced on the device (`Proposals.inbox` through the module's
  `Chains`, the chains this tab holds after `pullServed`) with a decision
  priced on the box (`/api/<chain>/fold`, through the box's `ChainSet`).
  Both are `chain_set`'s one function, so where the device holds what the
  box holds the two agree; where it lags, a decision reads the box's instant
  and a proposal the device's. A proposal serving a todo on a chain the
  device holds nothing of is NOT YET PRICED, never `∅`: it sorts after every
  priced row and after `∅`, and its card says so. — `waiting.test.ts` "law:
  a proposal on a chain the reading did not hold is not yet priced, and
  comes after every priced one and ∅".

- **No open proposal without something to press.** A proposal is open until
  a completion or a dismissal is recorded on its object — sent, applied,
  acted or dismissed — and a send that may have gone is open until the owner
  has checked Sent. While it is open the card offers Dismiss and the box's
  yes where one is safe: the answers while it stands, Try again after a tap
  that sent nothing (`failed`), and only "I checked Sent" after one that may
  have gone (`unknown`, then `checked`). The one open state with nothing to
  press is an act's running flow (`started`), which ends itself by its own
  expiry, or at a restart. — `app/core/src/secretary/store/tests.rs`
  "a_proposal_waits_exactly_until_its_completion_is_recorded" (every
  `Outcome` arm);
  `vectors.test.ts` "law: every open proposal's card offers an answer, runs
  its own act or asks for his check, and no closed one does" over
  `app/core/tests/web/proposals.json`; `stage.test.ts` "law: whatever may be
  answered is open, a send that may have gone waits for his check, and
  settled states are neither".
- **A submit performs at most once.** A tap runs the effect only when the
  object's fold has no completion, under the inbox's one lock; otherwise it
  answers with the recorded response. A send that sent nothing records
  nothing that completes, so a retry runs again; a send that may have gone
  is a completion and is never sent again, before or after his check; the
  same email proposed twice is one object and sends once. —
  `app/core/tests/inbox.rs`
  "a_submit_performs_at_most_once_and_answers_what_was_recorded" (proptest,
  concurrent rounds, acknowledgements included); `store.rs`
  "a_failed_proposal_is_retried_only_when_nothing_went";
  `app/cli/tests/secretary.rs`
  "a_send_that_may_have_gone_waits_until_acknowledged_and_never_sends_again"
  and "the_same_email_proposed_twice_is_one_object_and_sends_once";
  `web/e2e/inbox.test.ts` scenes 6 and 7.

- **A diff opens on its summary, and nothing decided from is folded.** A
  changeset's rows are under its card's Details, collapsed by default, under
  one line that is a function of the rows alone: every act of the diff
  appears in it, grouped in the order the acts first appear, every row
  counted. A changeset that no longer applies and a price at or under the
  review's 45% are said in that overview, never only in the rows, and the
  answers stay outside the fold. — `digest.test.ts` "law: `of` is a monoid homomorphism" and "law:
  every act and every named todo of the rows is in the line" (seeded);
  `inbox.state.test.ts` "expanded: …"; `web/e2e/inbox.test.ts` scene 8 and
  `secretary.test.ts` "a changeset card shows the diff, and Confirm applies
  it"; `web/e2e/inbox.test.ts` "a triage card appears in the Inbox, tagged as
  the agent's and drawn as its diff".
- **A card is drawn closed, and a yes is asked with the card open.** A card
  shows its head, who asked when it was not the secretary (a tag whose tip
  says why), its reason, what it would do, a changeset's one line, its page
  link and its answers; its sources, fields, body, files and rows open under
  Details. Asking a yes opens Details first, so the question is asked of
  exactly what is on screen. Every card says no as Dismiss; the yes keeps its
  own verb. A source is named in words, a todo by its title when the device
  holds it. — `cardui.test.ts` "a card is drawn with its details hidden, and
  asking any yes shows them…"; `web/e2e/inbox.test.ts` "a card is drawn closed, and asking Send opens it
  whole before the question"; `secretary.test.ts` "a changeset card shows the
  diff, and Confirm applies it" (Dismiss, not Deny).
- **The setup cards are one row, after the work.** A secret the box asks for
  in its setup conversation (`Asker` `setup`) is drawn in one closed Setup row
  after every other card, opened in place to those cards and a link to
  Settings. The box's other cards (`Asker` `box`: the store to unlock, a
  sign-in, an origin to allow, what was said around a forgotten fact) stand
  among the work in price order, with their conversation's title. A link
  that names a setup card unfolds the row once, in the page's state; from
  then on it is the reader's. — `inbox.test.ts` "law: only the setup cards
  go after the work…"; `inbox.state.test.ts` "a link to a setup card unfolds
  the Setup row once…".
- **A card is a function of its value.** It names its sources from the
  titles its page hands it (`ProposalFrame.titles`), never from the fold on
  screen. The Inbox reads those titles as one of its inputs
  (`inbox/onscreen.ts`), so a page opened before the todos arrived names its
  sources once they have. — `onscreen.test.ts` "a cold link reads no titles,
  then the todos' titles once the fold arrives…".

### 3.11 The shell's state

`App` (`web/src/app/app.ts`) and what the shell draws of it
(`web/src/shell/chrome.ts`), checked by `web-test.sh`.

- **`step` is pure.** It reads only its arguments, never changes them, and
  answers the same twice. — `app.test.ts` "law: step is pure…".
- **Every event the shell hears is one of the sum's arms,** and `step` is
  total over each: the arms are listed once, in a generator keyed by the tag
  (an arm with no generator is a type error), and every step keeps the count
  whole and the chain a slug. — `app.test.ts` "law: every event the shell
  hears is one of the sum's arms…".
- **Said twice is said once.** Every arm is idempotent, and a repeat returns
  the same `App`, so nobody wakes. —
  `app.test.ts` "law: said twice is said once…".
- **A list pins its chain, a new place closes the sheet, and only the
  route register's arms (`Went`, `Replaced`, `Moved`, `Kept`, `Resized`)
  move the log;** only a new place or More moves the sheet. — `app.test.ts`
  "law: a list pins its chain…", "law: only a route change or More moves
  the sheet…".
- **The stream is given up and comes back** by the one policy every stream
  keeps (§3.19): given up, it turns the live dot off, and its next open turns
  it on; a chain switch starts a new connection's heads and keeps what the
  live dot said. — `stream.test.ts` "law: a stream is given up…", "law: a
  given-up stream opens anew…"; `app.test.ts` "a stream given up turns the
  live dot off…", "law: a chain switch…".
- **A drawn shell is a function of `App`** (and of the fold the banner
  dates): `chrome.of` is plain data, the same for equal inputs, whatever the
  tab's own `App` does meanwhile, and a part is painted only when its value
  moved. — `chrome.test.ts` "law: a drawn shell is a function of App…", "a
  part is painted only when its value moved"; the banner's rule: "the banner
  is down while the fold is loaded and heard, and always for this browser's
  own fold while the stream is up". An act on its way raises no banner and
  no count. Its own row says where it stands, a sum per todo read off the
  replica (`replica.unsentTodos`: Sending, AwaitingTrust or Refused, the
  furthest from going through winning): "done · sending", "waits for this
  device to be trusted" or "refused: open it", and it stays on the list until
  the box takes it, whatever the filter. The shell says once, on any page and
  naming no object, when an act is refused, waits for trust, or is still on
  its way after five seconds (`shell/outbox.ts`). — `chrome.test.ts` "an act
  on its way raises no banner, and no banner names an object";
  `replica.test.ts` "a refused act, and every act resting on it, says
  refused…"; `todos.state.test.ts` "a todo done here…" and "a refused act and
  one waiting for this device's trust stay on the list…"; `outbox.test.ts`;
  `web/e2e/writes.test.ts` "Done marks only its line as sending…".

### 3.12 The app shell, the beacon and the kept renders

`docs/VISION-2.0.md` §8b, the `localfirst-2026-09-27` report.

`shell` (`web/src/app/shell.ts`, the loader `web/src/boot.ts` over
`web/src/app/loader.ts`, the worker `web/src/sw.ts` over
`web/src/app/worker.ts`, the page's half `web/src/app/newbuild.ts`):

- **The page holds nothing a build reads.** `index.html` is the same file
  for every build (a worker serves its own copy to whichever build boots),
  so it holds only the head and the loader; the build draws its own body
  and every host element (`web/src/shell/page.ts`) and links its own
  stylesheets beside its `app.js`. — `shell.test.ts` "law: index.html, the
  same file for every build, holds only the loader…"; in a browser,
  `web/e2e/shell.test.ts` "a cold start runs a new build that draws a host
  the old page lacks, under the old build's worker and page".
- **A build's files are named by it.** The page and its loader (`index.html`,
  `boot.js`) carry no build; every other shell file is `b/<build>/<file>`,
  and everything the app loads resolves against its own URL, the Typst
  editor's worker included. The box serves `b/<build>/` only for the build it
  serves, names that build at `b/current`, and refuses another build's file
  and a build's file under no build's name (`app/cli/src/commands/serve/
  versioned.rs`), so what arrives under a build's name is that build's file
  or a refusal. — `shell.test.ts` "law: a build's file is routed under the
  build its name says…", "a build's file under no build's name is no file of
  the shell's…", "law: a page's or a worker's own file names its build…";
  `versioned.rs`'s tests.
- **The API is never the worker's.** Every `/api/…` path, `/events` and every
  non-GET passes through. — the same route law.
- **A build survives the worker's script URL, and `dev` is never a build.** —
  `shell.test.ts` "law: a build survives the worker's script URL…".
- **No page runs two builds' files, whichever worker serves it,** so an old
  page is safe to run for as long as it is open. The worker keeps no record of
  who runs what: a file is served from the cache of the build its name says,
  or fetched under that name and kept there. A worker takes over as soon as
  it has kept its build whole. Only the sweep lets a cache go, never a failed
  install: a build's cache goes once no open page answers that it runs that
  build (the worker asks each page, `which`) and no worker installing or
  waiting is that build's; nothing goes while a page does not answer. — `shell.test.ts` "law: a build's cache goes only once no open page
  runs it…"; in a browser, `web/e2e/shell.test.ts` "a tab on an old build
  loads only that build's files, though the box moved and its worker was
  stopped before its first Typst view".
- **A start runs the build last heard the box serves, and waits on the
  network for nothing before its app runs.** The loader asks `b/current`;
  the worker answers it from its cache (`shell.HEARD`, kept at install and
  each time the page hears `/api/about`, a Reload's included) and asks the
  box again behind the answer, so the next start runs a build the box moved
  to, and this one offers it on the line. With nothing heard the box's answer
  is the answer (bounded, a few seconds). It loads that build, and when the
  build does not load, the build of the worker serving the page, so a page
  runs offline. Beside the app the loader asks for the build's two
  stylesheets and the chunks `app.js` imports (`modulepreload`, the names
  `scripts/build-web.sh` read from the bundle), and starts the list's fold,
  which the app's first ask of that path takes (`app/primed.ts`), bounded
  as every read is, its timeout the same error as the app's own read. That is the
  whole automatic update: no logic decides whether a reload is safe. —
  `boot.test.ts` "a warm start runs its app with the network never
  answering…", "the box's build heard behind one start is the build the next
  start runs", "the loader tries the box's build, then the worker's, each
  once…"; `revalidate.test.ts` "the loader's first read is the app's first
  answer of its path, taken once"; in a browser, `web/e2e/shell.test.ts` "a
  start runs the build last heard with nothing asked of the box first,
  offers the box's newer one, and the next start runs it".
- **A page is a chunk of its build, loaded as it is first drawn.** The Typst
  list, the chain, usage and settings are each an `import()` in
  `web/src/pages/table.ts` (the rest share most of their code with the shell);
  a page's chunk and the code it shares are named by their content beside
  `app.js`, kept with the
  build's shell at install, and refused under an old build's name once the
  box moved, while a tab on that build draws the page from its cache. —
  `boot.test.ts` "a build's chunks are kept with its shell at install…", "a
  newer build's worker serves a tab still on an older build that build's
  chunk…" (a chunk is known by its shape, `shell.chunk`, so any worker routes
  any build's chunk to that build's cache); in a browser, `web/e2e/shell.test.ts` "a page first drawn loads its own chunk…",
  "an old build's chunk is the box's 404 once it moved, and a tab on that
  build still draws the page from its cache", "a tab whose build's worker a
  newer tab's worker took over still draws a page it never opened…".
- **A running page never reloads without a tap.** It asks the box which build
  it serves as it starts, on a reconnect, as it comes back from the
  background and when an answer is misread or a write refused; a newer build
  is offered on the line across the top, "Suzatary <version> is ready", with
  Reload and Dismiss. The line is one value with a pure step,
  `newbuild.State` (`None | Updated | Offered | Dismissed`): a dismissed build
  stays dismissed and a newer one is offered, a build taken back is no longer
  offered, and an answer that names no build is no answer. Reload waits for
  the writes on their way (`app/inflight.ts`) and reloads; the composer's
  draft is kept as every draft is. A write the box refuses as older than its
  contract (426) is a deploy the page has not heard of: it asks again and
  offers the box's build, and the act says it was not sent and can be sent
  again after Reload; an act sealed into the replica stays held for the next
  build to send. —
  `newbuild.test.ts` "law: a running page never reloads without a tap,
  whatever it hears", "a newer build is offered, and a dismissed one stays
  dismissed…", "an answer that names no build is no answer", "Reload waits
  for the writes on their way"; in a browser, `web/e2e/shell.test.ts` (a
  reconnect and a resume offer the build and reload nothing; Reload keeps the
  composer's draft) and `web/e2e/writes.test.ts` "a page whose write the box
  refuses as older is never reloaded under the reader, and the act it sealed
  lands after a reload" (the push answered 426, the page still standing, the
  act on the box after the next load).
- **`/api/about` is never misread,** so a misread answer is one more ask of
  the box's build, never a loop. — `newbuild.test.ts` "/api/about is read as
  two strings, so it is never misread…".
- **A page says which build it started on once per build:** "Updated to
  Suzatary <version> (<revision>)", with Dismiss, when the box names that
  build too; never on a device's first build or a dev build, and never again
  on a plain reload or a second tab (`registers.announced`, the one thing this
  persists). — `shell.test.ts` "law: a move to a new build is news once…";
  `newbuild.test.ts` "law: a page says which build it started on once per
  build…"; in a browser, `web/e2e/shell.test.ts` (said once after Reload and
  after a cold start, dismissed for good; the line at phone and desktop
  width).
- **The shell keeps every vendored font.** The two small faces are kept at
  install; the two Source Serif files, 250 KB of the 330, as a page first
  draws in them (`font-display: swap`, so no text waits on them). —
  `shell.test.ts` "the shell keeps every vendored font the stylesheet can ask
  for", "the two Source Serif files are kept as a page first draws in them…".
- **A read carries a validator.** Every JSON answer to a GET has a weak
  `ETag`, the digest of its bytes, never of the heads the route read (a fold
  prices at the instant asked, so the same heads give new bytes an hour on);
  a request naming it is a 304 with no body. Answers stay `no-store`: the
  page keeps the last answer of the paths it read most recently and sends its
  tag (`wire/http.ts`). The one exception is an object's literal asked for by
  its name (`…/content?hashes=`), which is a function of the URL and is sent
  `private, immutable`; no page asks for one yet
  (`docs/LAZYSYNC-PLAN-2026-10-21.md`). — `validated.rs` "the tag changes exactly when the
  bytes do"; `app/cli/tests/routes.rs` "a read answers 304 to its own tag and
  a new tag once the chain moves"; `revalidate.test.ts`.

`beacon` (`web/src/app/beacon.ts`; the route is `suzatary_core::clienterror`):

- **A tab says each failure once, and at most `MOST` in all.** — `beacon.test.ts`
  "law: the reports sent are the distinct ones, in order, and never more than
  MOST"; the box's budget, bounds and one-line log are the core's tests and
  `app/cli/tests/clienterrors.rs`; in a browser, `web/e2e/shell.test.ts` (a
  rejection and a failed core, each in the box's log).

`table` (`web/src/algebra/table.ts`), the one tabulation (the Prodrome's
`memo::Cache`, design §6.2): the fold's caches, the namer's, and the box's
HTML kept on this device. In `table.test.ts`, over `gen.ts`:

- **A join is union,** commutative, associative and idempotent. — "law: a
  join is union…".
- **A key reads as the values it holds:** one as a hit, two or more as a
  finding, never as either. — "law: a key reads as the values it holds…".
- **A finding is reported exactly where it is made,** by an insert or a join.
  — "law: a finding is reported exactly where it is made…".
- **An entry may go and reads as a miss; a bounded table holds its bound.** —
  "law: an entry may go…", "law: a bounded table holds at most its bound…".

`rendered` and `richkeep` (`web/src/app/rendered.ts`, `web/src/app/richkeep.ts`),
the box's tabulation of Typst HTML on this device, keyed by the box's function
and the source's sha:

- **Joining the box's pairs is union;** a second value at a key is a finding,
  read as nothing, told to the box through the beacon and dropped from the
  device. — `rendered.test.ts` "law: joining the box's pairs is union…";
  `through` asks once and keeps the answer — "through answers what the device
  holds…".
- **Eviction keeps the newest that fit.** After it the kept fit the budget,
  nothing kept is older than anything evicted, evicting twice is evicting
  once, and within budget nothing goes. — `rendered.test.ts`.
- **Usage is a monoid** (`combine`, identity `none`), and a concatenation's
  usage is the combine. — `rendered.test.ts`.
- **Kept and read back, a fold is itself;** an unconfirmed row is never
  filled, whatever this device holds for its source; a device that cannot
  hash or store is a miss. — `richkeep.test.ts`, over the Rust's `fold.json`.

### 3.12 The form

`form` (`web/src/view/form.ts`), in `form.test.ts` over a small fake DOM
(attributes, a control's properties, listeners and children; no jsdom),
over `gen.ts`:

- **`step` is pure.** The same state and event give the same state, and
  neither is touched. — "law: step is pure — the same state and event give
  the same state, and neither is touched".
- **A typed value lands in its field alone,** and typing what is there
  returns the same object, so nobody wakes. — "law: a typed value lands in its
  field alone, and typing what is there wakes nobody".
- **A secret is gone once sent.** `Sent` leaves no `Secret` holding text and
  quiets a refusal. — "law: sending leaves no secret in the value, and quiets
  a refusal".
- **A drawn form is a function of its value.** A form drawn through any
  sequence of events is, node for node, the form drawn fresh from where they
  ended. — "law: a drawn form is a function of its value, whatever it was
  drawn from before".
- **A submit sends exactly the typed value,** a sent secret is not drawn
  again, and a refusal sends nothing and is shown beside the button and said
  once; so is the box's. — "law: a submit sends exactly the typed value, and
  a refusal sends nothing and is shown beside the button", "a box's refusal
  is kept beside the button and said once"; what each form sends is
  `web/e2e/writes.test.ts` and `overview.test.ts`, on the network.
- **A clearing form gives up what it sends as it sends it,** so what is
  typed while the box answers stays, whatever it answers and whenever a
  reading lands; a refusal hands what was sent back, its secret excepted, to
  fields still as the send left them, on the form shown. — "law: a clearing
  form gives up what it sends as it sends it…", "law: a refusal hands what was
  sent back…".
- **A half-typed value survives a redraw,** in the same node. — "law: a
  half-typed value survives a redraw, in the same node"; the todo page's edit
  form survives a sync because it is an island (§3.8).
- **`parsed` is a monad,** and a refusal passes through `map` as itself. —
  "law: parsed is a monad, and a refusal passes through untouched".
- **A refusal is said beside what it is about:** on the line of the field
  it names when the form placed one (`saidBy`), else on the form's line, and
  the box's refusal is about the whole. — "a refusal is said on the line of
  the field it is about…". The add sheet's reading names the field of each
  refusal, and no refusal lands under the tab bar.
- **An act is a change over its todo's heads,** named by the sha256 of its
  print, and a twin writes nothing; several heads are no reason to refuse. —
  `append.test.ts` "an act is a Change over its todo's heads…", "a twin of a
  held event writes nothing…", "several heads are no reason to refuse…".
- **Two tabs writing apart converge by adoption in either order:** one set of
  heads, one reading, and the next act the same bytes on either side. —
  `append.test.ts` "two tabs writing apart converge by adoption in either
  order"; the box's half is `sync.rs`'s "two tabs writing changes apart
  converge whichever reaches the box first".
- **A head set forgets order and repetition.** — `heads.test.ts` "law: of is
  idempotent and order-free, and same is its equality".

### 3.13 A Typst view

`viewstate`, `intent` and the kit (`web/src/typst/`, `web/typst/`), in
`viewstate.test.ts`, `intent.test.ts` and `typstview.test.ts` (the kit
compiled by the tab's own typst-wasm under node), over `gen.ts`:

- **The state is plain data,** and prints and reads back as itself. — "the
  state is plain data: it prints and reads back as itself".
- **Rendered means confirmed, in the state's type:** every text is a
  `viewstate.Text`, `Prose` or `Markup`. Escaped markup is read back into its
  prose by the core; a person's markup is `Markup`, kept byte for byte,
  exactly where the box rendered that field (`rich` non-null, which
  `view::renders` gives only a trusted actor's record), and is otherwise
  `Prose`, printed as its source. Only `Markup` is evaluated, by the kit's
  `shown`, in markup mode in the compile's closed world; inside a control a
  link keeps its words and drops its anchor. — "where the box rendered
  nothing, a title is the body's text as written", "escaped markup reaches
  the state as the prose it was escaped from", "a text is markup only where
  the box rendered it and it is not escaped prose", "confirmed markup is read
  as markup", "the same text as prose is printed as its source"; in a
  browser, `typstview.test.ts`'s "rendered means confirmed".
- **What markup makes still meets the tripwire:** a page whose confirmed
  markup makes a script, a handler or a `javascript:` link is refused whole.
  — "confirmed markup that makes a script, a handler or a javascript: link is
  refused by the box's tripwire", over the tab's own typst-wasm under node.
- **Every page compiles and passes the box's tripwire,** and a hostile title
  is printed as its text with no element made of it. — "every generated state
  compiles, and the box's tripwire passes the page", "a hostile title is
  printed as its text, never read as markup or HTML"; in a browser,
  `typstview.test.ts`'s "a crafted title renders as its literal text".
- **print and parse are inverses, and nothing else parses;** every control a
  compiled page declares parses. — "parse reads back what print wrote, for
  every intent", "anything outside the vocabulary is refused", "every control
  a compiled page declares is an intent the shell parses".
- **No overflow at 390 or 1280 px, and nothing above a tapped row moves** —
  `web/e2e/typstview.test.ts`.
- **A long title takes at most three lines in the list, beside a mark no
  wider than 40 px, and the open todo shows it whole;** the clamp hides
  nothing from the row's text or its control's name. — `typstview.test.ts`'s
  "a long title shows three lines in the list".

**The draw is one memoised fold** (stage 4; the Prodrome's `memo`, design
§6.2), for a page drawn by a Typst view (`web/src/typst/page.ts`; the list
is the one such page, the list's placeholder until Typst has drawn it): a view
names its kit module and its state's tree of `draw.Part`s, named by the
core's `memo_name`; `memo.fold` folds the kit's algebra (`draw.algebra`)
over it, tabulated in a `table` by (the kit's name, the node's name), a key
holding two values computed and reported as a `Finding` visit; a part is
drawn by the kit or is a run of its children; `place.into` puts the drawing
in keyed DOM, and
each redraw's `typst:draw` measure names its page and what it computed, node
by node and by depth. In `memo.test.ts`, `viewtree.test.ts` and
`typstview.test.ts`, over `gen.ts`:

- **Memoised equals plain,** whatever the cache dropped, at random or by a
  small bound. — "the memoised fold is the fold, whatever the cache dropped".
- **A key holding two values is computed, never chosen,** and reported where
  the fold met it. — "a key holding two values is computed, never chosen…".
- **An edit computes exactly its spine,** each node once, and hits at the
  top of every unchanged subtree, entering none; a name seen twice is
  computed once. — "an edit computes exactly its spine…", "with subtrees
  shared, a fold computes exactly the names it has not seen, each once".
- **A wave is the nodes whose children are known,** one `apply` each, and
  one compile of a wave is each node compiled alone. — "a wave holds only
  nodes whose children are known…", "a wave of nodes compiles to each node
  compiled alone, in order".
- **Naming is tabulated too,** and names as the namer does whatever its
  cache dropped; a node's own element is tabulated by its own content, so a
  node renamed only by its children compiles nothing. — "a namer read
  through a cache names as the namer does…"; in a browser, the
  `compiled` counts below.
- **A balanced sequence is the sequence,** its shape a function of its
  elements, an edit's spine logarithmic; cut by identity, an element's new
  content renames exactly its path. — "a balanced sequence folds back to the
  sequence", "the shape is the elements…", "one edit to a balanced sequence
  computes a spine logarithmic in its length", "cut by identity…".
- **The view's tree is a function of the rows,** not of the fold's order;
  a row's change renames exactly its path to the root, and opening a todo
  its row's path, what it shows in place under it, and the pane. —
  `viewtree.test.ts`.
- **A one-row change computes and places exactly that row's spine,**
  compiles one node, makes one element and keeps every other row's;
  **an Open intent computes its row's spine, the in-place block and the
  pane, and compiles those two; scroll and focus survive a redraw,** and a
  control replaced under the focus hands it to the control with the same
  intent. — `web/e2e/typstview.test.ts`'s "a one-row change computes and
  places…" and "scroll and focus survive a redraw…".

### 3.14 The Typst editor

`edit` and `editor` (`web/src/typst/`), the worker (`worker.ts`), and the
add sheet's and edit form's `form.markup` fields; in `typstedit.test.ts`
(the tab's typst-wasm under node, over `gen.ts`) and
`web/e2e/writes.test.ts`:

- **The shaded runs partition the source,** whatever the spans and marks: the
  twin under the textarea holds exactly the text the textarea does. — "the
  runs partition the source, whatever the spans and marks".
- **An answer about another text changes nothing:** a shade, a check, an
  offer or a hover names the source it is about, and is dropped once the
  field holds another. — "an answer about a text the editor no longer holds
  changes nothing".
- **Nothing moves under the cursor:** a text that does not compile keeps the
  last page that did, marked stale, and the field grows with its text instead
  of scrolling. — "a failed compile keeps the last page that compiled, marked
  stale"; in a browser, the editor scene's stale preview.
- **One rule, box and tab:** the tab's restricted project admits and refuses
  exactly the texts the box's does (`app/core/tests/web/closed-world.json`,
  read by both). — "the tab's restricted project admits exactly what the
  box's does".
- **A title stored as escaped prose opens as that prose,** and a diff is
  measured against what the field opened with, so opening and saving
  unchanged sends nothing. — "an escaped title opens in its prose, and markup
  a writer wrote opens unchanged".
- **A loop is a refused text:** a compile past two seconds terminates the
  worker and is answered with a refusal; the box's own child is killed after
  five (`app/cli/tests/typeset.rs`).
- **The box refuses, the editor says so sooner:** saving a text that does not
  compile is refused by `POST /api/author` with its line and column, shown
  under the field. — the editor scene in `web/e2e/writes.test.ts`.

### 3.15 Intents

Every act a page takes is an `asks.Ask` in one set (`intents.ts`), kept in
memory: what was asked, by its canonical print, and what the history has since
subsumed, both only growing.

- **A semilattice that only climbs.** `join` is idempotent, commutative and
  associative with `EMPTY` its unit, and every step leaves the set or climbs
  above it. — `intents.test.ts` "law: join is …" and "law: every step climbs"
  (seeded).
- **Asking twice is asking once.** An act pending, taken or held is not sent
  again, and its key is its content whatever order its fields were spelled
  in; the box answers a repeated tap with the recorded response and a
  repeated question with its first line. — `intents.test.ts` "law: asking
  twice …" and "law: the key is the act's content …";
  `app/cli/tests/secretary.rs` "a_question_asked_twice_is_asked_once".
- **Retraction is by subsumption, for good.** Settling against a reading
  retracts exactly what it holds, is idempotent and commutes with `join`. —
  `intents.test.ts` "law: settling retracts …".
- **A card's state is derived.** What a card draws is its row, then the open
  intent about it the row does not hold (asking, taken, failed); nothing is
  kept. A lost answer leaves the intent in view: the next sync retracts it,
  or asking again is one more ask and a double tap none. —
  `intents.test.ts` "law: what a card shows …" and "law: a lost answer …"
  (the 2026-09-30 frozen send card); `web/e2e/secretary.test.ts` "a tap
  whose answer is lost shows what the box did, not a frozen card".

- **A card's own state is a register, never its nodes.** Which step a card's
  controls are on (a confirm question, a reason asked for), the words typed
  there and whether its Details show are one register per card
  (`inbox/cardui.ts`, `Tab` registers of `app/ui.ts`), keyed by the card's
  identity and kept in the tab's memory outside every drawing. A redraw, or the page drawn again from nothing, reads it,
  so neither the step nor a half-typed reason is lost; a secret's value is
  never in one. — `cardui.test.ts` (seeded); `web/e2e/coding.test.ts` (the
  drop step and its reason across a redraw) and `web/e2e/inbox.test.ts` (the
  confirm step and its note, a dismissal's reason).

### 3.16 What a reading left out

A replica's reading is total over what it holds (Prodrome SPEC law 44): a print
that is not an object at its store's schema is left out with everything resting
on it, and what was left out is a function of the prints, answered beside every
reading as `excluded`. A chain that does not read at all is the other half of
the same fact: `excluded.Left` is both, objects and chains.

- **Two sets by name, joined by union.** `excluded.join` and `joinLeft` are
  idempotent, commutative and associative with `NONE` and `NOTHING` their
  units; a reader's later reading replaces only its own. Each held chain is
  told by the one set that holds it (`setOf`, from `Chains.hold`'s answer)
  and cleared when the set no longer holds it; the box's fold and the Inbox
  tell the chains they were priced without. — `excluded.test.ts` "law: join
  is …", "law: what one reading left out …" and "law: a reader's step …"
  (seeded); `total.test.ts` "law: a chain held over an unreadable print …".
- **Leaving out changes nothing else.** An unparseable print, and what rests
  on it, changes no reading of the rest: the secretary's readings answer the
  box's own words, and what was left out is exactly the print, in the parse's
  words, with what rests on it. — `total.test.ts` (the built core).
- **Never silent, never a failed page, one line.** The page draws what it
  could read, and the shell's one line, the only one, says "N objects on this
  device could not be read and were left out (M resting on them)" and names
  each chain that could not be read, opened to each name and why. —
  `web/e2e/excluded.test.ts`.

### 3.17 The ui as registers

The tab's own state is registers of the ui tier (the Prodrome's
`design-register-types.md` §6.0). Every register in the tab, a store's and
the ui's, is read by ONE reading, `register.of`: its frontier's values (the
writes no other write descends from) in the discrete order's completion,
one value or every distinct value as a conflict, never a winner by clock.
What differs is how the order is represented. A store's frontier comes
from the core, its writes named by object (`core.readingOf`,
`Entry.conflicts`); a ui register kept in this browser carries its order as
version vectors (`web/src/algebra/register.ts`: one writer per tab makes
each replica's writes a chain, so a past is a vector), joined by union read
back to the frontier. One reading, two representations of the order.
`app/ui.ts` is the effectful half; `app/registers.ts` declares every
register once with its tier, which is the decision of whether it syncs:

| Register | Tier | Why |
|---|---|---|
| the route (App's log, `shell/routelog.ts`) and each place's scroll and focus | Tab | a place is this tab's |
| a narrow list's open todo and its add sheet, by chain; a card's step, words and Details; the left-out line opened; a transcript's hold and scroll | Tab | what this tab has open |
| the theme, the chain read when none is named, the panel, a list's filter, grouping and folds, the build last announced | Device | how this device reads; joined across its tabs |
| a draft to the secretary, by conversation | Owner | the owner's words follow the owner |

A `Tab` register has one writer, so it is a plain value in memory, with no
vectors. A `Device` one lives in this browser's storage alone, stored cut
to its frontier's replicas. An `Owner` one is stored whole, and its writes
are what another device is handed (`ui.receive`); today that reaches this
browser's tabs. **An Owner register never enters a pushed store:** unsent
words never reach git. If drafts ever travel between devices, it is
through a box-local tier that is never pushed and that keeps each
register coalesced to its frontier. An overlay that is open (the palette,
the shortcut sheet) is an island (`frame.Island`) and closes on a new
place. The storage keys the registers replaced are swept once at start
(`app/sweep.ts`): an unsent draft moves into its register, the rest go.

- **A semilattice.** `join` is idempotent, commutative and associative with
  `EMPTY` its unit, and a register holds the frontier of every write it has
  seen, so reading commutes with union. — `register.test.ts` "law: join is
  …", "law: a register holds the frontier …" (seeded).
- **A write sees what is held.** It is the value whatever was there. —
  `register.test.ts` "law: a write sees everything held …".
- **A write rests on what its writer saw,** never on what storage holds
  at the moment it lands: tabs interleaving at random never lose a write,
  and a stale box's keystroke stands beside the newer words as a
  conflict. — `register.test.ts` "law: tabs interleaving at random never
  lose a write …" (seeded).
- **One reading, two representations.** The reading the tab shares,
  applied to a store's values, agrees register by register with the
  store's own fold of the same history: a row's state and content read
  as a conflict exactly where `Entry.conflicts` names them, and a
  proposal's registers read as the box's fold counts them; a register is a
  conflict exactly when that reading says so. — `candidates.test.ts` "the
  one reading of a row's candidate values …"; `proposals.test.ts` "a
  proposal's registers read by the tab's one reading …".
- **A cut changes no verdict.** A Device register stored cut reads the
  same, and so does its join with any write resting on it; its stored past
  names only its frontier's replicas however many tabs wrote it. —
  `register.test.ts` "law: cutting a frontier …", "a Device register's
  stored past …".
- **A disclosure drawn again is as the reader left it.** A `<details>`
  is drawn from its register (`disclosure.details`), written by the
  summary's tap before the browser toggles it, so a reading that lands in
  the next task redraws it open. — `e2e/excluded.test.ts` "a store's copy
  … the line stays open while a reset moves it".
- **A form drawn again is the form left,** what is typed and what the box
  said of it, until it is sent again. — `form.test.ts` "law: a form drawn
  again says what was said of the one shown …".
- **The old keys are swept once.** — `sweep.test.ts`.
- **A synced draft from two devices reads as a conflict, not a loss,** on
  both once they meet, and the next write settles it; the composer shows
  the other words to take. — `register.test.ts` "law: two replicas writing
  …", "law: a synced draft typed on two devices …".
- **Stored bytes are input.** A write set that does not parse, or a value
  its register's parser refuses, reads as nothing written. —
  `register.test.ts`, `registers.test.ts`.
- **A route printed and read back is the same route,** and the log holds a
  route by its print. — `laws.test.ts` "a route survives print and parse";
  `routelog.test.ts` "law: a route printed and read back …".
- **Back after going is the identity on the view:** the place and what was
  kept of it (scroll, focus); forward after back is the place gone to;
  going where the reader is writes nothing, and going drops what lay
  ahead. — `routelog.test.ts` (seeded); `web/e2e/history.test.ts` "back
  after going to a todo is the list as it was left …".
- **A draw is a function of (readings, ui registers):** the list drawn
  again from nothing after back is the same DOM, byte for byte, scrolled and
  focused where it was. — `web/e2e/history.test.ts` (the same scene).
- **One writer of the address.** Only `shell/address.ts` writes `location`
  or `history`; each entry it writes carries its place in the log, so the
  browser's back is the log's, a same-document link is a write through it,
  a hash typed by hand is a going, and a place with nothing behind it goes
  back to its fallback in its stead. A route that only forwards is never a
  place. — `scripts/web-static.sh` "the address has one writer";
  `web/e2e/history.test.ts` "a hash typed by hand is a place in the log, and
  a page that only forwards is never one".

### 3.18 The replica

The tab's replicas, in memory and on the device, are the free
join-semilattice on the box's objects (the Prodrome's
`design-register-types.md` §1 and §6.1). A replica means what it holds
(`replica.Held`): its generation, every object, the names the box is known
to hold, its refusals by name, and the words the box said, each its epoch,
heads, the policy and root they are read under, when, and the prodromes it
held. The epoch is the box's own clock over what a store serves
(`suzatary_core::epoch`, kept box-local under the state directory and
never in the vault): it moves whenever an answer does not extend the last
(other prodromes, or a head answered last no longer held) and at nothing
else, read in one snapshot with the answer's objects, so a migration, a
drop and a rollback, of the stores or within one, which can hold less, are
each a later word. Everything else is a reading of that set, made only by `replicaOf`:
the box's word is of the latest epoch, and within it the one whose heads
reach most of the objects (the later contact decides only between words of
equal reach); a prodrome that word does not list is forgotten with all
resting on it, and the heads are derived from what is left. The box's word
is read whole, as one value (`Replica.box`, `Unheard` or `Heard` with that
word), so its heads are never read beside another word's epoch or policy;
a chain the core holds is that replica whole (`HeldChain`), so its key, its
objects and its policy come from one value.
`replica.join` is the one join: within a generation, the plain union of
what each holds; a later generation is kept whole (a chain first, so still
a semilattice). The words are a register: the union keeps the latest
epoch's (a max), and of those drops a word whose heads rest strictly under
another's: what a held object rests on never changes, so such a word stays
under in every larger union and never wins.

- **The join is a semilattice.** Idempotent, commutative and associative,
  `NOTHING` its unit, over generated replicas that span a migration (a store
  added, the old one dropped, a rollback serving both again) and resets. —
  `join.test.ts` "law: join is idempotent, commutative and associative".
- **It only climbs, in a causal order.** Within a generation every object of
  either side is held after, each after what it rests on. — `join.test.ts`
  "law: a join only climbs".
- **The word read is one held, and none held is under another.** —
  `join.test.ts` "law: the word read is one held"; `replica.test.ts` "the
  box's heads, epoch, policy and root are one word, …".
- **The latest epoch's word is the box's, a drop and a rollback included.**
  — `join.test.ts` "law: the box's word is its latest epoch's", "a drop,
  heard after …", "a rollback within the same store …", "a rollback to the
  old store …"; `epoch.rs` "a store set change bumps the epoch and nothing
  else does", "a rollback of the same stores is a later epoch".
- **A box whose epoch went back is named.** Only a lost state directory
  does it; the later word is still read, and the one line says why. —
  `replica.test.ts` "an answer of an earlier epoch …".
- **Within an epoch, the word that has seen more wins.** — `join.test.ts`
  "law: the box's word that has seen more wins".
- **A dropped prodrome is a reading, which a later word lifts.** What a store
  the box no longer serves held stays held and reads as forgotten; if the box
  serves it again (a rollback to an earlier checkpoint), it reads again,
  fetched from nowhere. An unsent act written in it is named in the shell's
  one line, and counted in what a reset loses. — `join.test.ts` "law: a
  prodrome the box dropped reads as forgotten …".
- **Memory only climbs, and only from what the device kept.** An act, a pull
  or a receipt is written to the row first (`objects.holdReplica`, one
  read-write transaction, so two tabs offline at once both keep what they
  sealed), and memory moves only by joining the row the write left
  (`stepSync`'s `committed` is a join), so no exchange posts what the device
  did not keep. — `objects.test.ts` "two tabs offline"; `syncstep.test.ts`
  "committed: joins the row".
- **A reset stands, and says everything it loses.** The one way down is a
  reset, an empty replica of the generation after the row's, read in the
  transaction, so neither a copy from before it nor a concurrent reset brings
  anything back. It is handed every unsent act the reader was shown (refused,
  waiting on a key, waiting on the network, or stranded in a dropped store),
  and a row holding one more refuses it. A save that comes back on a later
  generation landed nowhere (`replica.landed`): an act sealed in a tab before
  another tab's reset is sealed once more onto the reset, filled from the box
  first, and reported lost only if a second reset overtakes that too, never
  shown and then dropped unsaid, and never sent twice. — `join.test.ts` "law:
  a later generation is kept whole"; `objects.test.ts` "a reset says
  everything it loses", "two resets in turn take two generations";
  `e2e/sync.test.ts` "an act overtaken by another tab's reset …".
- **A row that does not read is said, with its own way out.** It is never
  taken for an absent row (`idb.Row`: absent, held, a row that does not read
  and why, or a database that does not open and why; `idb.test.ts`),
  never written over by a save, and named in the shell's one line with a
  reset that says it writes over that copy, the one write that may; a
  store's row (the inbox's, the conversations') the same way, and only when
  a read of it says so, never for a failed save over a row that reads
  (`objects.keep`). —
  `objects.test.ts` "a row that does not read is said", "a save that fails
  over a row that reads …", "a reset told it
  writes over a copy that does not read …"; `e2e/excluded.test.ts` "a
  chain's copy that does not read is named …", "a store's copy that does
  not read …".

- **A store's row holds its keys, and a key an erasure names is owed.** A
  store's row is its replica and the keys of its words (`locked.Row`),
  written in one transaction, and its join is a pair of unions, the free
  semilattice on both, within one generation and one epoch of the box's
  word. A key the objects name as erased (the core's reading,
  `transcript::dead`) opens nothing, whatever copy brought it, and is OWED:
  whichever tab reads it, a pull or the device's row (one another tab wrote
  and never purged included), forgets the blocks its words were rendered to,
  then drops it, in memory and on the device (`purge.forgotten`,
  `objects.alterRow`), before the row is drawn. A forget that fails keeps the
  key, says so in the shell's line and is asked again at the next sync; it
  never stops a reading, since the words read as erased meanwhile. The
  core's worker is the one reader and writer of the device's rendered blocks
  (`rendered.device`, made by `questions.serve` alone) as of the lines' row,
  and a key's bytes live in its key row alone, so a forget is one delete
  there, and a mark on the device every tab's worker reads, so no tab writes
  an erased block back; an answer carrying keys is never kept with the reads
  (`http.Keeping`); the page holds blocks in view (`rendered.viaWorker`), asks the
  worker for the rest and hands it the box's answers, and a join the page
  sent before it heard of an erasure writes nothing back. A key an erasure
  names is let go only once its blocks are forgotten, wherever it is held,
  the row or a pull's pieces. — `onewriter.test.ts` "law: once the box erases
  words, no row, message or page holds them or their key…". A store's row in the shape of a
  build before the keys (a bare replica) is deleted under a reset's rule
  before it is read and taken again from the box; the plaintext row of the
  store the lines replaced is deleted under the same rule once the lines'
  row holds objects, the rendered blocks cleared first; a wipe deletes both
  databases. — `linekeys.test.ts` "law: the row's join is a semilattice…",
  "law: a key an erasure names opens nothing…", "law: after a purge nothing
  the row or the rendered blocks hold decodes to the erased words", "a key a
  tab that never purged left on the device is forgotten…", "a purge that
  cannot forget a block keeps the key…"; `objects.test.ts` "a row a build no
  longer reads is deleted…"; `wipe.test.ts` "the lines' row…";
  `e2e/lines.test.ts`.
- **A store's later epoch drops the copy in the join that brings it.** A
  store is a copy of the box's alone, so its rows are ordered by generation
  and then by the epoch of the box's word (`locked.join`), and the later row
  is kept whole: the join that takes in a later epoch's word is the one that
  drops every object and key of the epochs before, so no row holds that word
  beside an object the box no longer holds, in whatever order a pull, a piece
  kept from before, another tab's pull and a reset land. A pull answered in a
  later epoch than either part of what the tab holds heard, the row or the
  pieces kept beside it (`stores.overtaken`, asked of each part, since a
  piece of the later epoch beside a row of the one before joins to the piece
  alone), still starts over (`startOver`): its piece was asked from heads of
  the epoch before and is not a set the box could have sent, so the row is
  reset and the store asked for again from nothing. — `epochdrop.test.ts`
  "law: in every order, the device never holds the box's later word beside
  an object it no longer holds"; `e2e/epoch.test.ts`.
- **A store is pulled by pieces, and only for a page that reads it.** The
  boot never pulls the lines: the core's worker (`storeset`) restores and
  pulls a store the first time a question reads it (every reading and `Pull`
  all of them, `Proposals` the inbox alone), and a store this device never landed is waited for
  until it is whole, so a page never reads part of it, nor reads nothing as
  empty. The Inbox badge, on every page the Inbox's included, is the box's
  count (`GET /api/waiting`), and the "Needs you" popup reads the proposals
  from the inbox alone. A pull is a run of pieces (`pieces.pull`): the box
  answers what the tab's heads lack, parents first, cut to a megabyte of
  literals (`routes::Cut::PIECE`), with how many objects are `rest` and the
  name of every key an erasure in the store names (`erased`). Each piece is
  kept beside the row pages read, in a row of its own, so keeping one costs
  that piece, and the next is asked from the heads they hold together; the
  pieces land in one step when the box says nothing is left, so a page reads
  the store once per pull, never once per piece; until a store first lands,
  a page reading it draws its placeholder. How a pull ended is a value
  (`pieces.Ended`: `Whole`, `Stalled`, `Cut`). A pull that
  did not end whole is asked again after a wait that doubles to a minute,
  and sooner by a stream frame naming an object the tab does not hold,
  landed or not (`replica.behind`). A piece's read waits while bytes come
  (`http.get` with a `Silence` bound), since one object may be larger than a
  piece: a verb's answer is stored at most 64 KiB, but a line written before
  that bound is not rewritten. A piece joined to what the tab holds holds everything it
  rests on, the genesis of its prodrome included, the pieces together are
  the whole answer, and no piece carries an erased key; and the erased names
  every piece carries are forgotten from what the tab holds, blocks first,
  as soon as it takes any piece, so a partial pull never shows an erased
  block. — `app/core/tests/pieces.rs` "law: the pieces' union is the
  whole…", "a fork then a line is a merge"; `pieces.test.ts` "a pull
  interrupted after k pieces resumes at piece k+1", "law: every piece taken
  leaves the tab holding the ancestors…", "another tab's reset that lands
  while a piece is on its way…", "a many-piece pull lands once and writes
  each object once…", "a frame naming what the tab does not hold takes a cut
  pull up again…"; `stores.test.ts` "a reset is a step even of a row that
  holds nothing…"; `linekeys.test.ts` "a key the box's pull says is erased is
  forgotten…"; `e2e/secretary.test.ts` "a new request pops up in an open
  app…"; `e2e/lines.test.ts` "the list draws and the badge is
  counted with no request for the lines…".
- **The lines are a skeleton, and their content is fetched by name.** The
  core's worker pulls, lands and reads them (`linesync.ts`, the lines'
  instance of `engine.ts`, whose content is named rather than carried); the page says
  when it reads them and hears what moved: which conversations a landing
  touched (a line come or gone, a key of their words given or taken), or all
  of them after a reset or a later epoch, heard once a frame, so an open
  conversation reads again only when its own lines moved (`touched.test.ts`). The lines' row holds the skeleton
  (`GET /api/stores/lines/skeleton`, pulled by the same pieces), kept one
  node and one key per row in the database's `lines` store beside a head row
  (`noderows.ts`, which keeps every store so, each in its own object store), so a landing writes only what arrived. Each store's head row records the format its rows were written in (`noderows.Format`); a restore that finds another drops what those rows found (the lines: their rendered blocks), empties the store to a row of the generation after and takes it again from the box, while the content by name stays, so a change of a node's form moves the store's format and never the database's version (`noderows.test.ts` "rows of another format are dropped at restore…"); the lines' format 2 is the nodes that say what their lines retire; version 8 drops every earlier
  skeleton, whose nodes carry no step, with its keys, clears the rendered
  blocks those keys could find (`RENDERED_OWED`), and pulls the skeleton
  again. It is a replica of nodes: each object's name, what it rests
  on, its size and its header, never its literal (`replica.Obj`). Each
  literal is kept by its name in the database's `content` store
  (`content.ts`), taken only when its bytes hash to the name asked for in
  the same place, so the device holds the box's bytes and nothing else. A
  reading is whole or it is not made (`engine.over`): the list and a search
  read every line, some conversations (a window's one, the stream's drawn
  ones) read their own lines and what they rest on, and a reading whose lines are not all held fetches them (`GET
  /api/stores/lines/content?hashes=`, at most 64 names and a megabyte an
  answer, four answers out at once over HTTP/2 and one over HTTP/1.1, where a
  browser opens six connections to a host and a page holds three for its
  streams (`content.atOnce`), an opened conversation's names taken out
  of the whole store's queue, which goes newest first) and reads only once
  every one is held. Each name is read once a load, by the first reading
  that needs it: the names the device keeps are read in one transaction over
  one opened database, and every other reading that needs them waits on that
  read or on the box's answer. So every reading a tab
  answers is the reading of the whole pull at the same skeleton. A purge
  reads the largest down-set whose content is held, since words were only
  rendered from a conversation read whole, and first fetches the content of
  the conversations naming the keys it owes, keeping them owed if it cannot;
  it runs at each landing and once every line's content is held. Each
  node's step says what its line retires (`Step::Erased`'s keys, or the
  seal `Step::Resealed` was sealed again from), so the purge
  reads every dead key from the nodes the device holds, by the box's own
  rule (`linesync.retiredBy`, `transcript::erased` and
  `transcript::superseded`): in a conversation, every key an erasure names
  and every key of a reseal of a slot it names, and every seal a reseal was
  sealed from whose own key the device holds or a later reseal supersedes in
  turn. No answer names a dead key, and none sends one's bytes. The purge
  treats a replaced key as an erased one: it fetches the lines naming it,
  forgets the blocks its old words fed, and then drops it, so the device
  holds the box's keys whatever its readings read, and whatever pull it
  heard them in. A pull that lacks nothing is sent no node, no key and not
  the heads it holds (`wire::Heads::Held`). A slot whose seal a reseal
  supersedes is read from the reseal whether or not its old key is still
  held. The keys a row holds are the only ones its purge counts as held, so
  a key given before the row started over supersedes nothing. — `retired.test.ts` "law: the keys the held nodes retire leave a
  device the box's keys", "a key given before the row started over
  supersedes nothing…", over the box's vector `retired.json`; e2e
  `rescrub.test.ts`; the core's `known_pull` "a pull that lacks nothing
  sends nothing and reads no object".
  Content no node names is dropped after a landing. — `lazylines.test.ts` "law: a reading of part-fetched
  lines is the whole pull's reading at the same skeleton, or names what it
  fetches", "law: content is taken only where its bytes hash to the name
  asked for, in the place asked", "an opened conversation's names go out
  before the whole store's batches still queued…", "over HTTP/1.1 one
  content answer is out at a time…", "concurrent whole readings read each
  object the device keeps once…"; `linekeys.test.ts` "a
  purge that cannot find the blocks…"; the core's `held_lines`
  `a_replica_holding_the_replaced_keys_reads_the_reseals_and_scrubs_nothing`;
  `e2e/rescrub.test.ts`; `e2e/lines.test.ts` "a conversation is
  read from the lines…", "the list draws and the badge is counted with no
  request for the lines…"; `e2e/epoch.test.ts`.
- **The device keeps the newest conversations, and says what it lacks.**
  Once a page has read the lines, the core's worker (`linesync`, at each
  landing) prefetches the content of the
  newest conversations, each whole, newest first, up to a byte budget
  (`keep.plan`): a tenth of the origin's quota from
  `navigator.storage.estimate()`, at most 256 MiB, and 64 MiB where the
  browser does not say (`keep.budget`). A prefetch goes out one answer at a
  time, only when no other content answer is out and a page is shown (as the
  page tells the worker, `LinesShown`), and
  its first failure stops it until the next landing; what it
  fetches goes to the device's `content` store, not to the tab's memory. After
  a landing the tab keeps only the plan's content and the content of every
  node a reading of this page read; it drops the rest, which a reading fetches
  again by name. Eviction changes only whether a reading can be made, never
  what it says, since a reading is whole or not made. A conversation whose
  lines are not held and cannot be fetched is answered `NotHere` by the
  worker (`questions.One`), with how many lines it lacks, and draws "Not on
  this device yet: N lines of this conversation", with why, never an empty
  transcript or a
  placeholder that does not end. — `lazylines.test.ts` "law: the plan keeps
  the newest conversations whole, in order, within the budget", "a prefetch
  goes out one answer at a time…"; `e2e/lines.test.ts` "a conversation the
  device prefetched reads with no network, and one it lacks says so".
- **A sync says how far it has come, in bytes.** Every piece of a store's
  pull carries `size`, the bytes the replica lacked when it asked, so the
  first piece says what the whole pull brings, and `weight`, its own bytes;
  a content fetch says its whole size, by the sizes the skeleton gives, when
  it starts. The line under the header (`shell/syncing.ts`) draws each
  store syncing as bytes come of bytes needed, a neutral pie (§3.20) before
  the same in words ("Syncing conversations · 0.4 of 1.1 MB"), from
  `progress.T`, and is hidden once no store syncs.

### 3.19 Runs, streams and the device's rows

Each pattern written once, in `web/src/algebra/once.ts`, `latest.ts`,
`stream.ts` and `web/src/app/idb.ts`, checked by `web-test.sh` over
`gen.ts`:

- **`once` keeps a success and forgets a failure:** a kept success answers
  every later call and runs nothing, a failure is forgotten so the next call
  runs again, and `forget` drops what is kept. — `once.test.ts` "law: once
  keeps a success and forgets a failure".
- **`shared` joins a run in flight:** a call during a run gets that run, and
  a run starts only for a call made while none is in flight. —
  `once.test.ts` "law: shared joins a run in flight".
- **`latest` keeps answers in the order asked:** one landing after a
  later-asked one's is dropped and let go, a failure included, and after
  `close` nothing lands. — `latest.test.ts` "law: an answer landing after a
  later-asked one is dropped and let go; what lands climbs".
- **Streams have one give-up policy, and one way back:** every stream from
  the box (the chain's, each store's, a conversation's) gives its socket up
  after `GIVE_UP` failures with no open between them, or at once when
  closed, and opens a new one when the network may be back (the browser
  online again, the tab shown again, or a slow tick, `TICK_MS`, for a tab
  that stays shown while the box restarts); a live stream is left alone, so
  the tick is no poll. — `stream.test.ts` "law: a stream is given up…",
  "law: a given-up stream opens anew when the network may be back…", "law: a
  given-up stream on a tab that stays shown and online opens anew on the
  slow tick, and only then".
- **A document in the back/forward cache holds nothing open:** hidden, it
  closes its streams and its page once; back from the cache, it opens them
  once again and mounts the page it was on; a show that is no return from
  the cache opens nothing. — `bfcache.test.ts` "law: hidden closes once, a
  return from the cache opens once, and a show that is no return opens
  nothing".
- **An unreadable signer row never mints a key:** a device that keeps no
  key mints one; a row that does not read is `Unreadable`, said, signs
  nothing, and is offered "Use a new key on this device", the one write
  over it; a store that does not open (`idb.Row`'s `Unavailable`) is
  `Unsupported`, offered nothing, since no key could be kept there, and
  asked again next time, since a browser resumed from sleep may open it; and a
  key held again, a renewed one's included, moves the tab back to writing
  (`App`'s `Signs`). — `signer.test.ts` "law: an unreadable signer row never mints a
  key"; `device.test.ts` "an unreadable key row is offered a new key…";
  `app.test.ts` "a renewed or newly trusted key writes again without a
  reload".
- **A replica row is read in one form:** the database's upgrade drops every
  row an earlier version kept (version 2's and version 1's alike, and
  version 4's lines, kept as literals, with their pieces), the tab
  pulls each chain and store again from the box, and the reader holds no
  shim. What a dropped row held that the box had not taken is lost. —
  `objects.test.ts` "a row version 2 kept is dropped…", "a row version 1
  kept is dropped…", "version 5 drops the lines' row and its pieces…"; e2e `upgrade8.test.ts`; `replica.test.ts` "a row in a form an earlier build
  kept does not read…"; e2e `upgrade.test.ts` "a device holding a version 2
  replica drops it on its first load…".
- **A verdict says what it found as a sum.** Each object the walk reached
  carries its depth and one it never reached says so (`verify.Walk`), and a
  chain's check is `Reached`, or `Unreached` with every problem, never an
  empty list beside a flag (`verify.Found`); the core's flag-and-depth and
  flag-and-list are read into those once, at its decoder, which refuses a
  pair that contradicts itself. — `verify.test.ts` "a parent that names
  nothing breaks the walk…".

### 3.20 A fraction is a pie

Every fraction the app shows is one mark, `web/src/view/pie.ts`: typst-design's
`pie`, the one a Typst page draws, at its geometry (filled clockwise from
twelve o'clock on a track of its own colour at 22%, outlined at 6.7% of its
size), 14 px beside a line of text and larger only beside a headline. No
`<progress>`, no bar drawn as a width. Its colour says what kind of fraction it
is (`pie.Kind`), and nothing else does:

- a **judgement**, which says how well something stands: a todo's
  fulfillment, the allowance left on the usage page, a price being typed.
  It wears the fulfillment scale at its own value;
- anything else is **neutral**: a sync's or a fetch's progress, a share of a
  whole (the usage page's tokens by who and by model), the loading mark. It
  wears the one accent, `tokens.typ`'s `accent` (the blue ramp, off the
  scale's crimson-to-teal path), so a pull at 10% never looks urgent.

Every pie is `role="img"` and named by its fraction in words ("36% synced",
"42% fulfilled", "58% of the allowance left"), and the same number stands
beside it as text, so neither its colour nor its shape is the only carrier.
The accent is the blue ramp at 35% on the light ground and its dark stop on
the dark one, so its outline holds a contrast above 3:1 in both themes.

- **A fraction is drawn as the pie, named in words, in its kind's colour.** —
  `fulfillment.test.ts` "law: the pie's filled share is the value, in the
  value's colour"; in a browser, `web/e2e/lines.test.ts` "a store's sync is
  drawn as a neutral pie, named in words".

### 3.21 The topic stream

The secretary page (`web/src/pages/secretary.ts`) is one `secretaryState.T`
(`secretary.state.ts`): the filter, how many rounds are asked for, the last
reading, the sheet and the head's search, moved only by `secretaryState.step`.
Its one effect, `secretaryState.ask`, reads the stream again whenever what the
page wants (the filter, Earlier, a move of the stores) is newer than what it
last asked. The core reads the stream and the bar from the tab's replicas, so
the page holds no filter of its own and no second reading of the topics.

- **A combination shows exactly the rounds that carry every chosen topic.** No
  topic chosen shows every round, a combination shows what each of its topics
  shows, and an old name filters as the topic it stands for. —
  `app/core/tests/topics.rs`
  `a_filter_shows_exactly_the_rounds_holding_every_chosen_topic`.
- **The stream reads only the rounds it draws.** Each skeleton node of a
  line carries its step (`secretary::step::Step`: it opens the conversation,
  asks, answers, rewinds, proposes, records a coding session, names the
  conversation, seals a slot again (`Resealed { of }`), erases slots
  (`Erased { keys }`), or moves no turn), which the box takes from the
  line's shape. The core's worker holds the skeleton's line nodes beside the
  prints (`secretary::outlines::Outlines`) and folds each conversation's
  steps, in the causal order its own reading uses, to its outline
  (`secretary::outline::of_steps`): each live round's opening turn and when,
  the line of each of its turns, the line that titles the conversation, the
  reseals of each slot and the erasures naming each key, kept until a line
  of that conversation comes or goes. The stream chooses and counts its
  rounds from the outlines (`stream::choose`). A drawn round reads its title
  line, its turns' lines, every reseal of their slots and every erasure
  naming a key of those (`Rounds::objects`), and nothing they rest on: a
  slot reads from its own seal, its reseals and its erasures alone, so these
  lines open as the whole conversation opens them. The worker fetches those
  objects (`lines.Scope` `Objects`), holds them without hashing them again
  (`content.Admitted`, `Lines.hold_admitted`), and draws each round from them
  (`HeldLines::round`), memoised by the names of its lines' leaves, which
  cover each line, its reseals and whether each slot is erased and its key
  held. A conversation whose earlier rounds are not drawn costs nothing. The
  bar reads the topic stores alone. What a reading left out (`Left`) is
  asked after its answer, of the lines held. — `app/core/tests/transcript.rs`
  `the_skeleton_outlines_a_conversation_as_its_reading_does` (the outline of
  the box's nodes opens the conversation's rounds, and both refuse the same
  lines); `app/core/tests/held_lines.rs`
  `the_stream_reads_only_the_rounds_it_draws` (the stream drawn round by
  round, from every line or from the drawn rounds' lines alone, is the
  stream of the conversations read whole, whatever was erased, resealed or
  swept; holding those lines decodes exactly their prints and hashes none,
  opening them decrypts no slot outside them, and a second stream decodes
  and decrypts nothing),
  `the_first_stream_at_the_live_shape_reads_only_its_rounds`,
  `a_print_that_does_not_decode_leaves_out_its_round_and_draws_the_rest`;
  `e2e/bigstore.test.ts` "a warm load of the conversations draws them from
  the device…" (the worker decodes at most the drawn conversations' nodes
  and hashes none).
- **The filter is the address.** `#/secretary?topics=a,b` parses to the route
  that prints it. — `laws.test.ts` "a route survives print and parse" (the
  generator gives the secretary route topics).
- **In a browser:** two chips tapped show the one round that carries both,
  Back and Forward move the filter, a name the type refuses is said before
  anything is written, and a topic added by tap reaches the box's topic store.
  — `web/e2e/secretary.test.ts` "two topics chosen show the rounds holding
  both, and a topic added by tap lands on the round".

## 4. The vectors

What the Rust writes and the TypeScript must agree with, never refresh:

- **The wire.** `web/src/wire/generated/*.ts`, regenerated from `wire.rs` by
  `scripts/wire.sh`: the types and a decoder for each answer.
- **Fulfillment.** `web/test/wasm.test.ts` drives `prodrome-wasm` under
  node against the flake input's seeded vectors and this chain (`events/`,
  `app/conformance/`).
- **Rounds.** `app/core/tests/about-fixture.json`: one transcript both
  sides group identically.
- **The fold, the chain and the conversation** (`webalgebra` §5 commit 6).
  `app/core/tests/web_vectors.rs` builds a `FoldPayload`, `ChainPayload`
  and `ConversationPayload` over the six-todo fixture chain at a fixed
  instant and, only under `SUZATARY_WRITE_VECTORS=1`, writes each plus the
  Rust's answer to the TypeScript's question under `app/core/tests/web/`
  (`{fold,chain,conversation}.json` and each `.answer.json`); otherwise it
  recomputes and asserts equal to the committed bytes. `proposals.json` is
  a card at every stage, and `proposals.answer.json` whether its decision
  is open (§3.10).
  `web/test/vectors.test.ts` passes each payload through its decoder
  and answers with the modules a page uses — `todos.ts`'s ordering (fold),
  `verify.ts`, the fallback verifier (chain), `rounds.ts` (conversation) —
  asserting equality with the Rust's answer.

- **The replicas.** The same test file writes `replicas.json`: the inbox's
  and the conversations' objects as a tab is handed them, and the box's
  answers for them (the Inbox, the list, six windows, three searches), read
  off the box's own stores, with the keys the pull carries; one turn is
  erased after a tab was given its key (`stale`), and `erased` is what such
  a tab forgets. `web/test/replicas.test.ts` holds the module's
  readings to those strings byte for byte, in any arrival order and with
  objects sent twice, at every cut of a sync, and for two tabs that took the
  objects in different rounds; `web/test/join.test.ts` holds the replicas'
  `join` to the semilattice laws over the same objects.

## 5. What is not here

The DOM is specified only by the doors and the keyed-draw law; `dom.ts`
draws it, browser scenes check it; no jsdom, ever. Styling
(`web/static/style.css`) is not a law.
