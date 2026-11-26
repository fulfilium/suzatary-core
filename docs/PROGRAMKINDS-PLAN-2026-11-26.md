# Subscriptions and two program kinds for the view framework (2026-11-26)

Status: open. Roadmap `programkinds`. This note is stage 1: a design, with no change of
behaviour. The code lands in the stages of §6.

The framework in `app/viewtree` is the Elm Architecture (model, view, update), with effects as
data and subscriptions as a function of the model. A `Program` has `init`, `update` and `view`.
This note adds `subscriptions` to it and names two program kinds, `element` and `application`,
as its public face. It was written from the code of 2.1.273, and the sources are named in each
section.

## 0. What exists today

An `Effect` (`app/pages/src/runtime.rs`) is data, and the TypeScript host
(`web/src/pages/program.ts`) performs each one. Most effects are a single request: `Get`,
`Post`, `Act`, `Core`, `Timer`, `Copy`, `Focus`. Four effects are different. `Hold`, `Watch`,
`Listen` and `Keys` start a listener that stays until something stops it.

The host keeps these listeners in one map, `held_`, keyed by the effect's `ask` (a name the page
chooses). The rule is in `hold`: a listener started under a name that is already held stops the
earlier one first. Nothing else stops a listener except the page's `destroy`. So a page has no
way to say "stop listening to this".

Two pages already do by hand what a runtime should do once:

- `todos.rs` (`wanted_keys`, `declare`) compares the keys it wants with the keys it last sent,
  and emits `Effect::Keys` only when they differ.
- `settings.rs` and `inbox.rs` add a `Watch` for each card as the card appears (`Hold::Rows`).
  Nothing removes the `Watch` when the card goes.

Counted in `app/pages/src`: 26 `Effect::Watch`, 10 `Effect::Hold`, 1 `Effect::Listen` and
1 `Effect::Keys` construction sites. All but about seven are issued once, in `init`, and never
change (§1.5).

## 1. Subscriptions

### 1.1 What a subscription means

`subscriptions` says which event sources a page listens to when its model has a given value:

```
subscriptions : Model -> (Slot -> Source)
```

A `Source` is data: a path to hold, a local reading to watch, a stream to open, a set of keys,
an instant, a frame clock. It holds no closure. A source produces the same inputs the page
already receives (`Input::Held`, `Heard`, `Opened`, `Lost`, `Answered`), each tagged with the
page's own slot name. The page never maps a source's output, so `Sub` needs no `map`.

A `Slot` is the name the page gives a source. Two sources never share a slot, because the value
is a map. If a page builds two sources under one slot, the last one wins, as `Map.fromList`
does. The function is total, and no state is invalid.

### 1.2 Types

In `viewtree`, generic over a page's types and naming nothing of this repository:

```rust
pub trait Program {
    // type Model, Msg, Effect, Flags as now
    type Slot: Ord + Clone;
    type Sub: Clone + PartialEq;
    fn subscriptions(model: &Self::Model) -> Subs<Self::Slot, Self::Sub>;
}

pub struct Subs<K, V>(BTreeMap<K, V>);

pub enum SubPatch<K, V> {
    Start { slot: K, source: V },
    Stop { slot: K },
}
```

`Start` on a slot that is active replaces it: the host stops the earlier listener first. There
is no separate "retarget" case. This is today's `hold` rule, written as data.

`Subs<K, V>` is generic over a schema in the sense of AGENTS.md rule 11 only as a collection
(`diff` over two ordered maps). It names no store, so it stays in `viewtree`, which is this
repository's web framework (decision `upstreamplacement`). No part of it belongs in Prodrome.

In `app/pages`, `Effect<H, A, S>` splits. The one-shot effects stay `Effect<A>`. The four
listening effects, and two new sources, become one enum:

```rust
pub enum Sub<H, S> {
    Hold   { path: String },            // slot: Held(H)
    Watch  { local: Local },            // slot: Held(H)
    Stream { path: String },            // slot: Streamed(S)
    Keys   { keys: BTreeSet<KeyKind> }, // a set, so reordering moves nothing
    At     { instant: i64 },            // epoch ms, absolute
    Frames,                             // every animation frame while present
}
```

`Effect` already derives `PartialEq`, so equality of sources costs nothing new.

### 1.3 The law

After every step, the runtime's active listeners equal `subscriptions(model)`.

- A listener starts when its slot appears.
- A listener stops when its slot disappears.
- A listener is replaced when its slot stays and its source changes.
- A listener is kept, and not restarted, when its slot stays and its source is equal.

The runtime holds the denotation, `active: Subs`, as a field. After `update` it computes
`next = P::subscriptions(&model)` and sends `diff(&active, &next)` to the host in the frame.
This is how the painter treats a view: it keeps the last tree and answers the smallest list of
patches. A subscription list is a keyed set, so the diff is simpler than the tree's:

1. Every slot in `active` and not in `next` gives `Stop`, in slot order.
2. Every slot in `next` whose source differs from `active`'s, or is new, gives `Start`, in slot
   order.
3. `active := next`.

A frame carries its subscription patches before its effects, so a command that names a
listener started in the same step (`Again { path }` after a `Hold`) finds it. `Runtime::start`
begins with `active` empty, so the first frame holds a `Start` for every source of the first
model.

The host (`program.ts`) keeps `Map<slot, stop>`, which is `held_` today. A `Start` is the
existing body of the `Hold`, `Watch`, `Listen` and `Keys` cases. A `Stop` calls the stored
function. A source's first answer is delivered after the frame's last patch is applied, as a
message like any other, so a start never re-enters the frame it came in.

### 1.4 Laws and where they are tested

Laws that stage 2 adds, as `viewtree` tests over a generated program and generated message walks,
with a fake host that applies `SubPatch`es to a map the way `Dom` applies patches:

1. `the_active_listeners_are_the_subscriptions`: after every step, the fake host's map equals
   `subscriptions(model)`.
2. `a_step_that_moves_no_subscription_patches_nothing`: equal lists give an empty patch list.
3. `an_unchanged_source_is_never_restarted`: over a walk, a slot's `Start` count is at most one
   more than the number of steps in which its source changed.

One law belongs to the page crate, once, over each page's generated walks:
`a_repeated_idle_input_moves_no_subscription`. Delivering the same `Context` or the same `Held`
twice gives no patch the second time. It catches a source that holds a counter or the clock,
which would restart every step.

One law belongs to the TypeScript host, in `node:test` with fake sources: the host's live
listeners equal the slots patched in. A `Leaving` page stops every subscription at once (§2.4).

### 1.5 Every page that listens today

"Today" is `init` and `update` in `app/pages/src`; the host is `program.ts`. A listener stops
only at the page's `destroy` unless a row says otherwise.

| Page | It listens to | How it starts and stops now | Its `subscriptions` |
| --- | --- | --- | --- |
| `todos` | Watch `Fold`, `Unsent`, `Waiting`, `Route`, one `Kept` per register (7); Hold `Briefings`, `Overview`; the keys it wants | All in `init`. `Keys` is re-sent when `declare` finds a different list | Eleven constant sources, plus `Keys { wanted_keys }`, a function of the model |
| `todo` | Hold `Page { path }`; Watch `Fold`, `Unsent`, `Proposals` | `init` | Four constant sources |
| `inbox` | Watch `Rows(w)` for each chain; `Intents`, `Titles`; a card's reading per shown card | `init`, then one `Watch` per new card; none is stopped | `Rows` and card sources derived from the model's readings, so a card that goes is stopped |
| `needsyou` | Watch `Rows(w)` for each chain; `Route` | `init` | Derived from the readings, plus `Route` |
| `settings` | Hold `Overview`; Watch `Device`, `Intents`; `Rows(w)` per card the box files | `init`, then one `Watch` per new card; none is stopped | Constants, plus `Rows` derived from the model |
| `mail` | Hold `First { path }`, `Watches` | `init` | Two constant sources |
| `memory` | Hold `Memory`, `Record` | `init` | Two constant sources |
| `about` | Hold, twice | `init` | Two constant sources |
| `reading::One<V>` (message, thread, calendar, briefings, briefing) | Hold `Page { path }` | `init` | One constant source |
| `draft` | Watch `Draft`, `Refused` | `init` | Two constant sources |
| `typstlist` | Watch `Fold`, `Route` | `init` | Two constant sources |
| `conversations` | Watch `Chats`, `Route` | `init` | Two constant sources |
| `secretary` | Watch `Stream { chosen, pages }`, `Scroll`, `Found { q }`; a timer to the next day; the animation frame while slices wait | `Stream` re-issued under one name when the topics or page count change (replace). `Found` issued when a debounce answers, never stopped. The timer is re-armed each time it answers. `Frame` re-issued after each answer while slices wait | `Stream` of the model's filter; `Scroll`; `Found` while a search is on; `At { next_day }`; `Frames` while `shown` has a backlog |
| `conversation` | Watch `Window { id, window }`, `Intents`, `Titles`, `Scroll`, `Draft`; Listen stream `/conversations/<id>/stream`; Hold `Block(sha)` per streaming block; the animation frame while slices wait | `Window` re-issued under one name whenever `asked` changes. `Block(sha)` issued per block seen, never stopped, though `blocks.clear()` empties the model's copy. `Frame` as in `secretary` | `Window` of the model; `Stream { path }`, constant for the page; a `Hold` per block still streaming; `Frames` while slices wait |
| `chats::Searching` (used by `secretary`, `conversations`) | A one-shot `Timer` for the debounce; Watch `Found { q }` | The timer per keystroke; `Found` when the timer answers | The timer stays an effect (§2.3). `Found { q }` is a subscription while a search is on |
| `card` (`Out::Watch`) | One Watch per card shown, on demand | As `inbox` | Derived from the cards the model shows |

Listeners that are not a page's, and stay in the TypeScript shell (§2.3): the context a host
pushes to every page (`app.hear`, `http.heardClock`, `placed.hear`); the address and history
(`address.ts`); `presence.ts`; `main.ts`'s wake handlers, `bfcache.ts`, the `storage` and
`pageshow` events; the stream's reconnect machine (`stream.ts`); the painter's scroll and resize
observers (`scroll.ts`); the elapsed-seconds ticker in `parts.ts`.

## 2. Where subscriptions make things worse

Each case below was looked for in the code. A case is either absorbed by the design, or it stays
outside `subscriptions`.

### 2.1 A start that costs, or holds state

- **A stream with a resume point.** No page holds a cursor today. The browser's `EventSource`
  resumes within one socket by itself. After a stream is given up (`Lost`), `stream.ts` opens a
  new socket, and the page recovers with a re-read: `conversation::connected` asks for the window
  again and sends `Sync` when a lost stream opens again. The cursor, if a page ever needs one, is
  model state and travels as a message on `Opened`, as that re-read does now. It must never be a
  field of `Sub::Stream`. A cursor in the source changes on every frame, so every frame would be a
  `Replace`, and the socket would reopen on every frame. `Sub::Stream` has a `path` and no
  cursor field, so that mistake does not compile. If a server later needs the cursor in the URL
  at connect time, that one stream stays an effect with its own key. This is the one case where I
  keep an escape: nothing in the tree needs it today, and I would not build for it.
- **The stream's reconnect machine.** `stream.ts` keeps `Health` (`Trying`, `Open`, `GivenUp`)
  and a 30-second tick, none of it in the model. This is the host's implementation of a
  `Stream` source, and stays there. The model sees only `Opened`, `Heard` and `Lost`.
- **A `Watch` whose start is work.** `Stream { chosen, pages }` and `Search { q }` run in the
  core's worker. Replacing one costs the same as today's replace. The slot is the page's name
  (`Hold::Stream`), so a page that pages forward replaces one source and starts no second one.
  The code keys the memo by content (`asked.family`), so reading again an unchanged value is
  free.
- **A `Hold` that fetches.** A path is one register per path (`held`), so stopping and starting
  the same path in one step costs no request. A new path costs one, as today.

### 2.2 A listener that is hard to key

A source's identity is its value, and the slot is the page's own `ask`. Today's `hold` already
uses that pair. Three values need care:

- `Keys` is a set of kinds. `todos.rs` compares an ordered `Vec`, which is correct only because
  one function builds it. The source holds a `BTreeSet`, so no order can restart it.
- `Local` and `Window` hold a `serde_json::Value` in places. Equality over them is structural
  and stays exact.
- A source must not hold a counter, a version or the clock. This is the one way to write a
  restart loop: a source that differs on every step starts again on every step. Today the same
  mistake is an effect issued on every step, as easy to write, and equally visible in a test.
  `a_repeated_idle_input_moves_no_subscription` (§1.4) pins it for every page.

### 2.3 A timer that must not reset, and what stays outside

- The day boundary (`secretary::day`) is `Timer { after }`, computed from `now` at the step
  that arms it. That has a defect today: `setTimeout` counts only awake time, so a device that
  sleeps wakes late. `At { instant }` is an absolute deadline. While the model's `now` stays in
  the same day, the instant is equal and the timer is kept. When it fires, the page learns the
  new `now`, the instant moves a day, and the source is replaced. The host re-arms from the
  instant with `clock.wake` (`web/src/algebra/clock.ts`), which already caps a wait at 2^31 ms and
  is told to arm again after sleep. This is better than today.
- The search debounce stays an effect. It is a delay that starts at a keystroke, and no model
  field holds the keystroke's time. A source of `After { ms }` would have to start when the
  runtime saw it, which is not a function of the model, and the denotation would not be pure. The
  `Timer` effect keeps the page's staleness check (`typings`).
- The elapsed-seconds counter (`parts.ts`, `setInterval` 1000) stays a part the painter owns. As
  a subscription it would step the model and diff the view every second for a number the model
  does not use. The part also keeps its start time by key across rewording. Elm's `Time.every`
  does the other thing, and re-renders each second. We do not copy that.
- Every one-shot effect (`Get`, `Post`, `Act`, `Core`, `Endorse`, `NewKey`, `Timer`, `Offer`,
  `Sync`, `Reset`, `Rendered`, `Keep`, `Again`, `Copy`, `Focus`, `Scroll`, `Toast`, `Mark`) is a
  question asked once. None becomes a subscription.
- The context a host pushes to every page is not declared by any page, and `needsyou` ignores
  it. It stays an input of the host (`Input::Context`), and not a source.
- The scroll and resize observers, the address and history, presence, wake and the storage events
  belong to the shell. They have no page model to depend on.

### 2.4 What the TypeScript shell keeps that the model cannot

- The live listener handles (`stop` functions) and the sockets. The model holds the value of a
  source, the host holds what the browser gives back.
- Whether a page is `Drawn`, `Hidden` or `Leaving` (`drain.ts`). The tile decides it, not the
  document, so the page never sees it. Subscriptions improve this host code. `drain.ts` has two
  tables of a verdict for each effect kind, and `LEAVING` drops the six listening kinds (`Hold`, `Watch`, `Listen`, `Keys`, `Timer`, `Frame`) one by one. One
  law replaces those rows: a `Leaving` program's active subscriptions are empty. `Keys`
  is the one source a `Hidden` page keeps stopped (`UNDRAWN` says `Waits` for it today); a
  source kind then carries its own verdict in the same table.
- The history entries' marks and what the view kept of each place (scroll and focus). Only the
  DOM knows the second (§3).

### 2.5 The cost of calling `subscriptions` after every step

A step already builds the whole `view` tree and diffs it. `subscriptions` builds a list of about
fifteen small values at most, and the diff walks two small ordered maps. The costliest case
is `conversation` while a model answer streams: one step per delta frame, and the `Block`
sources are derived from the blocks in flight, which is small beside the view of the same step.

I could not measure this here. Stage 2 measures it before pages move: a `page:subs` User Timing
measure beside `page:draw`, read with the browser bench of `docs/BENCH.md`. If it is over a
tenth of a step on any page, the runtime skips the call for a step whose message cannot move a
source (a `Msg` the page marks as view-only). I do not add that skip now, because I have no
figure that asks for it.

### 2.6 What is worse, and the verdict

Honest costs:

1. Today `init` is the one place that lists a page's listeners. After the change, `subscriptions`
   is. The dynamic cases are few: about seven of the 38 sites. For a page whose listeners never
   change, the new function is the old list moved, with no gain except the law. The gain is in
   the cases that were wrong.
2. A restart loop is silent where a missing `Stop` is quiet. §2.2 and the law of §1.4 answer it,
   and a test is the only fence; I do not add a runtime guard for a state the laws can pin.
3. During stage 2 a page can issue a `Watch` effect by habit, so both paths exist. Stage 3
   deletes the four effect variants, so the old path stops compiling. No shim stays.

What improves:

- `Hold::Block(sha)` and the `Rows`/card `Watch`es leak today. Nothing stops them before the
  page ends. With subscriptions, they stop when the model stops asking.
- `todos.rs`'s hand-written `declare` (about 12 lines) goes, and so do the `new`-watches
  bookkeeping of `settings` and `inbox`.
- The day timer survives sleep.
- `Leaving` becomes one law in place of six table rows.
- A test asserts `subscriptions(&model)` where it asserts a vector of effects today (for example
  `conversation.rs`'s `matches!(e, Effect::Frame { ask: Ask::Slice })`), which is a statement of
  what a page listens to, not of what one step happened to emit.

Verdict: adopt subscriptions, with the two carve-outs of §2.3 (the debounce and the
elapsed ticker) and the stream-with-a-URL-cursor exception of §2.1. If stage 2's measure shows a
cost I did not expect, the verdict reverses for that page only: it keeps its listeners as
effects and the kind stays valid, because a page's `subscriptions` may return an empty map.

## 3. Program kinds

Both kinds are a `Program`. They differ in what the host gives them.

### 3.1 `element`

An element is mounted into a node the host gives it, and takes over nothing else. It receives no
address, no link click and no document event. It may send requests outward as effects (`Go`,
`Replace`, `Back`, `Leave`, `Release`), and its host answers them. For a page, the host is its
tile, which is the desk. All 17 pages are elements, and a program with no effects and no
subscriptions (`Effect = Never`, `Sub = Never`; `Never` exists in `runtime.rs`) is the smallest
one. There is no third `sandbox` kind: no code asks for one, and `Never` already says it.

An element may read the part of the address it was given (`Watch { local: Route }`, used by
`todos`, `typstlist`, `conversations` and `needsyou`), and it may not write it.

### 3.2 `application`

An application is an element that also owns the URL. It adds:

```rust
pub trait Application: Program {
    type Url: Clone + PartialEq;
    fn url_changed(url: Self::Url) -> Self::Msg;
    fn url_requested(request: Requested<Self::Url>) -> Self::Msg;
}
```

- The first `Url` arrives in `Flags`, parsed by the host. This repository's hash is total
  (every string reads as a route), so a parse that can refuse has no place.
- `url_changed` is sent for every change of the address, the application's own pushes included.
  There is one path for "the address moved", whatever moved it. This is Elm's rule, and it is
  what lets a model hold one address and trust it.
- `url_requested` is sent for every followed link and every `suzatary:open` event inside the
  application's root. Neither changes the address until the application sends a navigation
  effect.
- Navigation effects are `Push`, `Replace`, `Back { fallback }` and `Leave`. The type of the
  effect, and not a `Key` capability, is the capability: an element's `Effect` holds the
  requests of §3.1, which its host answers, and only an application's host turns them into
  history writes. `address.ts` stays the one module that writes `location` and `history`.

### 3.3 Where ours differs from Elm's `Browser.application`

| Elm | Here | Why |
| --- | --- | --- |
| `UrlRequest` is `Internal` or `External` | `Requested { from, to }`: the place and the tile or part that asked | The painter drops every `href` that is not `#…` or an `https:`, `http:` or `mailto:` link opened in a new tab with no handle back. The app never sees an external click. The desk's one rule (open beside, or over) needs the origin |
| `Browser.Navigation.Key` in the model | None. The effect type is the capability | A `Key` cannot be made in a test, which pushes applications to carry `Maybe Key`. A value that cannot be built is a cost, with no gain here |
| A page may not see the URL | An element may read its own route (`Local::Route`), never write it | The tiles already work this way, and `pages.absorbs` depends on it |
| `Sub` and `Cmd` are opaque, built from closures | `Sub` and `Effect` are data. A test asserts them | AGENTS.md rule 5. §4 |
| `Sub.batch`, a list; the manager diffs it | A keyed map with a diff in the runtime | A listener's identity is explicit, and a duplicate cannot be written |
| `onVisibilityChange` is a source a program declares | The tile's `Shown` is the host's, and pages never see it | A tile can be hidden while the document is visible |
| `document` kind controls title and body | Not added | Nothing asks for it |
| `Browser.Events` reaches `window` and `document` for any program | A closed `Source` of what the shell can name: replicas, registers, streams, keys, an instant, the frame clock | A new source is one new arm and one host function. A page cannot listen to a DOM event the shell has not named, which is the framework's rule: the TypeScript holds no page logic |

What Elm gets right, and we keep: effects at the edge and run by the runtime; subscriptions a
function of the model; one runtime path for "the address changed"; a closed set of effect
managers, with ports as the way out; `Html.map` (`Node::map` exists); `Html.Lazy` (`Lazy`
exists).

### 3.4 Which kind to start with

A first-time author starts with `element`, always. Ask two questions:

1. Does the program own the document's address, writing it as well as reading it? If not, it is
   an element, even if it reads its own route.
2. Is it the one program that holds the whole window, and is no other program mounted by the
   host above it? Only then is it an application.

This repository has one application, the desk, and 17 elements.

### 3.5 How the desk and the router map onto them

**The desk** (`app/pages/src/desk.rs`) holds the tiles tree, the shares set at dividers and the
dock. Today it is a plain struct driven by `tiles.ts`: `go`, `open`, `resize`, `dock` mutate it,
and `drawing(w, h)` answers a `Drawing`. As an `Application`:

| Desk today | Application |
| --- | --- |
| `go(address)` after the browser moves | `url_changed(Url)` |
| `open(key, href)` for a link in a tile, a card's `suzatary:open`, a page's `Go` | `url_requested(Requested { from: Some(tile), to })` |
| `resize`, `dock` | Messages |
| `Pinned`, `behind`, `keep` of the `Drawing` | The view's keyed children: a leaf in the tree is a child, and a leaf that leaves is dropped by the host |

Two things differ from a page. The desk's output is a `Drawing` with rectangles, and the view
vocabulary is closed and has no `style` (`node.rs`: "Closed: no handler and no style"). Stage 4
decides between a keyed `Foreign` node per tile whose `props` carry the rectangle (`Foreign` is
"a part another painter draws from props alone", which `tiles.ts` already is), and a typed
position attribute. I lean to `Foreign`, because it adds no attribute. I do not decide it here.

**The router** is `Route` (`route.rs` and `web/src/shell/route.ts`), `routelog.ts`, `address.ts`
and the `Went`, `Replaced`, `Moved` events of `app.step`:

- `Url` is the parsed hash (`tiles::Address`); `parseAddress` has a Rust and a TypeScript copy
  now, pinned by vectors. The application's parse is the Rust one, and the TypeScript one goes
  where nothing else in the shell uses it. Stage 4 counts the lines.
- `Went` and `Replaced` become `Push` and `Replace`; `Moved` becomes `url_changed`.
- `routelog` (the entries, the pointer, and what the view kept of each place) stays in the host.
  Only the DOM knows the scroll and the focus, and the browser's own session history is the
  other half, which cannot be read ahead. The model holds the address and nothing of the log. The
  question "is there an entry behind" stays `Back { fallback }`, answered by the host as it is.

**What the desk does once, that Elm makes every application write by hand.** An Elm application
wraps each child page's message (`GotTodoMsg`), maps each child's commands and subscriptions
(`Cmd.map`, `Sub.map`), and maps each child's view (`Html.map`), once per page. Here:

- No wrapping is written. Each page is its own runtime and its own wasm class, so a child's
  messages never reach the desk. `pages.table` is one line per route and is total over `Route`,
  checked by its type.
- The desk gives every child the same context and the same `Placed` (`opens`, `shown`, `go`,
  `release`), so a page's `Go` is the same request wherever the page stands.
- The desk owns one lifetime rule for every child: a page runs while its leaf is in the tree,
  drawn or behind, up to `keep` (`Drawing.keep`).
- A `Leaving` child stops its subscriptions by one law (§2.4), with no per-page code.

The cost is that two runtimes cannot share a model. That is the question of §5.

## 4. What stays ours

Effects stay transparent data that a test can assert. Elm's `Cmd` and `Sub` are opaque and
cannot be compared or printed. We do not copy that:

- `Effect` and `Sub` are `Clone`, `PartialEq`, `Debug`, `Serialize`. A test asserts a page's
  subscriptions with `assert_eq!`.
- The host interprets each by a total match with no `_ =>` over our own sum. A new source is
  a compile error in every place that must handle it.
- A page's handlers in the view stay messages, never closures (`Listener<M>`).
- A source carries no callback. The message it makes is the fixed `Input` of §1.1, so there is
  nothing to map.

## 5. Open question: composing programs as state machines

A program is a state machine. Its state is the model. Its inputs are messages. Its outputs are
effects, and, with this note, the set of sources it wants. `init` gives the start state, and
`update` is the transition.

To compose two programs would mean to build one program from two:

- **Product**: the model is a pair, a message goes to one of two, and the effects and
  subscriptions are the two lists put together. Two panels that never talk are a product.
- **Sum**: the model is one of two, as when a page is a list or a detail. The inactive one has no
  subscriptions.
- **Nesting**: a parent holds a child's model, wraps the child's messages and maps its effects
  and subscriptions into its own. This is Elm's hand-written `GotTodoMsg`.

The desk already does a part of this by hand, outside the type: each tile is a product
component, mounted and kept by the host, and `pages.table` is its sum over `Route`. What a
composition would give is the same behaviour in Rust, as one combinator with laws: the child's
subscriptions end when its leaf leaves the tree by construction, not by the host's table, and
one test of the combinator covers all 17 pages.

What decides whether it is worth a stage:

1. A second consumer that needs two programs to share one model. Today no page does. The
   closest case is the todo pane that `todos.rs` once held (TILEDLAYOUT's §0), and the tile tree
   removed that need.
2. Whether `Sub::map` and `Effect::map` would have a caller. Without a caller, they are a
   wrapper nothing inspects (AGENTS.md rule 11), and I do not add them.
3. Whether stage 4 finds that the desk, as an application, must route a child's messages
   itself. If the desk's tiles can stay separate runtimes, composition is not needed.

I do not design it here.

## 6. Stages

Each stage is one pull request. A stage that moves a page moves every effect of that page in
one commit, with the commits split by page group (AGENTS.md; CLAUDE.md "A change lands as a
series").

1. **This note.** No code.
2. **The trait and the runtime, with laws.** `Program` gains `Slot`, `Sub` and `subscriptions`.
   `viewtree` gains `Subs`, `SubPatch`, `diff`, the runtime's `active` field, and the frame's
   `subs`. `Hosted` and `export!` gain `subs()`, as JSON like `effects()`. The laws of §1.4 and
   a fake host. The TypeScript host gains the patch interpreter, beside the effect interpreter,
   and the `Leaving` law. The pages' `subscriptions` default to an empty map, so no page moves
   and no behaviour changes. The `page:subs` measure lands here, and decides §2.5.
3. **The pages move over, in groups.** (a) The constant readers: `reading::One`, `about`,
   `memory`, `mail`, `draft`, `typstlist`, `conversations`, `needsyou`, `todo`. (b) The ones
   with derived or dynamic sources: `todos` (`Keys`), `inbox`, `settings`, `card`. (c) The
   streaming and timed ones: `secretary`, `conversation` (`Window`, `Stream`, `Block`, `At`,
   `Frames`). The last commit deletes `Effect::Hold`, `Watch`, `Listen` and `Keys`, the matching
   host cases, the `UNDRAWN` and `LEAVING` rows they had, and `todos::declare`. I expect it to
   remove about 150 lines of Rust and TypeScript, to be counted at the stage.
4. **The public face and README.** `Application` and `Requested`, `viewtree::export_element!`
   and `export_application!`, the README's "Use" naming the kinds and §3.4's two questions. The
   desk becomes the one application, with the router mapped as in §3.5, and the stage decides
   the `Foreign` question of §3.5 and counts what `route.ts` loses. It also decides the question
   of §5, with the evidence of stages 2 and 3.

The reading set grows by this note. The stages should delete as much as they add: the
estimate above is the reason for stage 3.

## 7. Performance: a frame of motion

Today every message runs `update`, then the whole `view`, then the painter's diff. A `Lazy`
node skips a subtree whose hash is unchanged (`paint.rs`, `lazy`). This section asks what a
frame of motion costs, in three layers.

### 7.1 Motion as data

An animation can be a value in the model: where it starts, where it ends, when it starts, how
long it lasts, and its easing. The browser's compositor plays it (a CSS transition, or the Web
Animations API). `update` and `view` then run only when the motion starts, ends or is
interrupted, and never for a frame in between.

Which motions the app has today:

- **16 CSS rules** in `web/static/style.css` hold a `transition` or `animation`, and two
  `@keyframes` (`placeholder-in`, `loadmark-fill`). Each is driven by a class or an attribute
  that the view already sets, so the compositor plays it and the model holds nothing. These do
  not change.
- **No JavaScript animates anything.** `scrollTo`, `scrollTop` and `scrollIntoView` in
  `web/src` all jump. The `Scroll` effect does the same.
- **Tile placement is not animated.** `tiles.ts` writes each tile's rectangle as a style, and
  a divider moves between discrete stops (§7.2). Moving a tile from one rectangle to the next
  is the one place a motion value fits: its end points are measured layout, which a CSS rule on
  a class cannot name.

How the painter would take one. A tile's rectangle is the `props` of a keyed `Foreign` node
(§3.5). Add a field `Move { from, duration, easing }` to those props. The painter reads it when
the node's `props` change, and calls `element.animate` on the compositor-owned properties
(`transform` and `opacity`). It writes the final style at once, so the page after the motion
is the page the view says, and an interrupted motion starts from the node's computed position.
The painter's laws (a patch applied equals a fresh draw) hold, because the final style is in
the patch and the motion only describes how the browser shows the way there. I add no motion
kind to the view vocabulary for any other purpose: no page asks for one.

Where it does not fit: a motion that depends on pointer position every frame (a drag that
follows the pointer) is per-frame logic, not a motion value (§7.2).

### 7.2 Per-frame logic: a divider drag

By the code, and as §7.4 measures, a divider drag runs no page's `update`, `view` or diff. The
pointer handler (`tiles.ts`) picks the nearest of the divider's discrete stops and calls
`snap` only when the stop changes. `snap` calls `Desk::resize`, then `Desk::drawing`, which
the host reads as JSON, and `draw` writes the tiles' styles where they differ. A page learns
its new size from a container query, in CSS. So the frame's cost scales with the desk's tree and
with the tiles' count, and not with a page's size.

That is the property the brief asks for, and it holds only because the drag is discrete and the
pages are separate runtimes. Two things would break it:

- A continuous drag (a stop for every pixel) would call `resize` and `drawing` on every
  pointer event. The desk's work is small, but `drawing` is a whole-tree JSON print each time.
- A page whose view depends on its own width (not by a container query) would need a `Context`
  message per frame. That page would then pay its whole `update`, `view` and diff for a change
  of one number.

The general answer is incremental recomputation keyed by the footprint of a change. A change
to a model is a value of a small type (a message), and each part of the view is a function of a
slice of the model, a lens known from the types. When a message changes only the slice of a
lens, only the views of that lens rerun, and the rest are the same nodes. Compared with today:

| | Runs after a message | Decided by |
| --- | --- | --- |
| Today: full view plus `Lazy` | The whole `view` function. A `Lazy` part is skipped when its hash equals the last | A hash a person writes for each lazy part, so a wrong hash is a stale screen |
| Footprint of a change | Only the views whose slice the message changed | The lens, a typed path into the model, known at compile time |

The footprint costs a type for each slice and a rule that a view reads only its slice. It does
not need runtime read tracking. I do not recommend building it now. The measure of §7.4 shows
whether a frame is spent in the view, and `Lazy` has not been shown to fall short. It is a
question for the stage after the kinds, and it depends on that measure.

### 7.3 Per-frame state never reaches the replicated store

Only the settled result of a motion or a drag is written. A divider's position is the desk's
state in this tab (`Desk.resized`), and the address does not carry it. A write to a chain or a
store is the owner's act or an effect (`Act`, `Keep`); a frame of motion is neither. A
`Frames` source exists only while the model says something moves: `Sub::Frames` is in the map
returned by `subscriptions` while a motion or a backlog is pending (`secretary` and
`conversation`'s slices), and the slot disappears, and the animation frame stops, when the
model says nothing moves. This is one more reason for the `Frames` source of §1.2: today each
answer re-issues the next `Frame` by hand.

### 7.4 Measured: one frame of dragging a divider

Measured on this container with the fixture build, by the existing bench
`web/e2e/resizedrag.bench.ts` (`scripts/web-e2e.sh resizedrag`): a todo open beside the list at
1,440 px by 900 px, the divider dragged by a mouse 8 px a frame, with the main thread 6 times
slower.

| Measure | Result |
| --- | --- |
| A mouse drag of the divider, 278 steps, 566 frames at 6x | Median 17 ms and p95 17 ms (the 60 Hz frame is 16.7 ms), slowest 33 ms, 4 frames over budget, 2 long animation frames of 53 ms |
| The window narrowed and widened 8 px a frame, 136 steps, 286 frames at 6x | Median 17 ms, p95 67 ms, slowest 150 ms, 133 frames over budget, 37 long animation frames of 50 to 141 ms |
| The desk's own work for one divider snap (`Desk::resize`, `Desk::drawing`, the JSON the host reads), a native release build, 200 snaps, a list with a todo beside it | 2 µs, 9 µs and 3 µs, and 779 bytes of JSON |

What the numbers say:

- The divider drag keeps the 60 Hz frame even at a 6x slower main thread. The desk's part is
  about 14 µs a snap, so the frame's time is the browser's: writing two tiles' styles, layout
  and paint.
- The window resize is where the time goes. Every step places the tiles again and lets the
  page's container queries lay out again, and it misses the frame at half of its frames at 6x.
  This is the layer that the footprint of §7.2 does not touch: the cost is the browser's layout
  of the page's DOM, and it scales with the page's size.
- A page's `update`, `view`, diff, patch bytes and painter apply are not in either path, so I
  have no figure for them and did not invent one. The code reads so (`snap` and `resized` call
  only the desk), and the bench's frame times agree with it. The bench does not split a frame
  into its parts. A split needs a `User Timing` measure around `update`, `view`, diff and apply,
  which stage 2 adds as `page:subs` and its neighbours; I did not add one in a stage that
  changes no code.
- The figure for a page's whole step on a large model, which is what a footprint would cut, is
  the one thing the brief asked for that I could not take here. I name it as the first
  measurement of stage 2.

## 8. Priorities: urgent and deferred work

React's concurrent rendering (`startTransition`, `useDeferredValue`) keeps an urgent update (a
keystroke, a drag) from waiting behind a non-urgent one (a large store reading that lands from a
sync). Does our runtime need urgent and deferred messages?

What the runtime already does:

- Everything runs on one thread, and a step is synchronous. The heavy reads run in the core's
  worker (`Effect::Core`, `Local::Tree`, `Local::Stream`), and their answers arrive as messages.
- The host coalesces moves of what a page watches: `heard` in `program.ts` tells a move in a
  task of its own, once per burst, and measures the wait as `page:heard`. Tiles a tab keeps never
  step together.
- A page that draws a long list draws it in slices: `slices.rs` steps a slice for each animation
  frame, newest first (`Effect::Frame`). This is deferred view work, written by hand in the page.

Messages must not be reordered. `update` is a pure function of the model and a message, and two
messages rarely commute. React solves that by keeping each update's priority and rebasing the
state. A pure `update` that moves an urgent message ahead of a deferred one gives a model no
sequence of messages produced, and the laws of §1.4 would have no statement to check.

So the design keeps one queue for `update`, in arrival order, and defers the work that has no
effect on the model: the `view`, the diff and the paint.

1. `update` runs for every message at once. It is cheap, because the heavy work is in the
   worker.
2. A message from a person's gesture (an input, a key, a pointer, a click) paints in the same
   task. This is today's behaviour.
3. A message that the host delivers (`Held`, `Heard`, `Context`) marks the view stale, and the
   runtime paints once, at the next animation frame. If another message arrives first, the stale
   view is not drawn, and the later one covers it. This is the "deferred view work that can be
   dropped when overtaken": the `view` function never started, so nothing is wasted.
4. A `view` that is already running cannot be stopped, because wasm has no preemption. A page
   whose `view` is too long for one frame splits it by slices, as `slices.rs` does now.

The runtime holds one value of a two-case type for this,
`Painted` or `Owed`, and no boolean beside optional fields. Priority is a property of a message's
source, as seen by the host, and not a field of `Msg`. A page does not name it, so `update`
stays one function of a message.

I do not recommend building this now. No measurement here shows a delivered message making a
gesture wait. A bench exists for it (`web/e2e/pagebench.bench.ts` and the `page:heard`
measure), and stage 2 can read it. If the measure shows a wait, the render coalescing above is
the smallest change that answers it, and it needs no change to `Program`.

## 9. Prior art: what decides which work runs on a change

| System | What runs when state changes | What decides it |
| --- | --- | --- |
| Elm (`Html`, `Html.Lazy`) | The whole `view`, then a virtual-tree diff. A lazy node is skipped when its arguments are reference-equal to the last call's | The programmer, by writing `lazy` and choosing its arguments |
| React | The component whose state changed, and its children, unless a child is memoized. The React Compiler adds memoization automatically | A compile-time analysis of the values a component reads. `useSyncExternalStore` with a selector rerenders a component only when its selected value changes. Motion libraries keep their values outside render and write styles directly |
| Fine-grained signals (Solid, Svelte 5 runes, Vue Vapor, the TC39 Signals proposal) | Only the computations and the DOM writes that read the signal that was written. There is no tree diff | Reads tracked at runtime: a computation records the signals it reads as it runs |
| This framework, today | The whole `view`, then the painter's keyed diff. A lazy node is skipped when its hash is the one drawn at its place | A hash the page's author writes, as in Elm |
| This framework, as §7.2 would extend it | Only the views whose slice of the model a message changed | A lens, a typed path into the model, known from the types |

Where ours sits. The model is a plain value, and `update` is pure. Nothing records a read, so no
view can depend on something it did not declare. A footprint is data: a test can print it, and a
law can say that a message outside a view's slice leaves that view's nodes as they were. The
cost is that someone writes each lens, and a view must take its slice and nothing else, which
the type of the view can enforce. A signal system pays no such cost and has no such fence, since
its reads are found by running. We choose the typed footprint over tracked reads for the
same reason as everywhere else in this note: a thing a test can assert is better than a thing a
runtime finds.

The footprint is stage 5, after the kinds, and only if the measure of §7.4 shows a frame spent
in the view.

## 10. What the new sections add to the stages

- Stage 2 takes the first measurement: a page's whole step (`update`, `view`, diff, patch bytes,
  apply) on the largest page, with User Timing measures, before any page moves (§2.5, §7.4).
- Stage 4 may add the `Move { from, duration, easing }` field to a tile's `props` (§7.1), if the
  desk becomes an application with keyed `Foreign` tiles.
- A fifth stage, the footprint of §7.2, and the render coalescing of §8, are decided by the
  measure of stage 2. This note recommends neither now.
