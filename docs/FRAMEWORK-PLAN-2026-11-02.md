# The framework split: pages as Rust programs, a small painter, Typst for content (2026-11-02)

This is the design pass for the todo `frameworksplit` (2.3). It holds a plan and one measured
prototype: the Settings page converted to the new shape behind a flag. The stages in §7 complete
the todo; this document does not. Stage 1 removed the flag (§7.1). Paths are relative to the repository root, read at `10c7f82`
(2.1.55); review round 1 rewrote the prototype and its numbers on 2.1.56 (`87584c3`).

**The owner's decisions (2026-11-02), which this plan takes as given:**

1. Three languages, each owning its own files. **Rust** holds the model, the pure `update` and the
   view as data (`view(model) -> a typed view tree`), compiled to wasm on the device and natively
   on the box. **Typst** holds content only (a round's answer, a todo's body, a brief, reports),
   as template files called with typed values, cached by content hash. **TypeScript** holds a
   small generic painter and effects runtime and no app logic or app state, about 2,000 to 3,000
   production lines in place of today's 21,188.
2. No source of one language embedded in another. A Rust view vocabulary of data constructors is
   fine. Every boundary is typed and generated: Rust types to TypeScript (the wire, which exists)
   and Rust types to Typst data schemas (new).
3. Not Gleam or Unison, but their ideas: effects as data and server components from Lustre;
   caches keyed by the hash of the code, effects as handlers, and moving work to the data from
   Unison.
4. React Native's payoff without its shape: the painter is replaceable. Today a DOM painter, later
   native painters reading the same view tree only if the WebView limits us.

**The answer, in short.** Go, decided by the owner in review round 2 (2026-11-03). A page's size
is measured in characters without whitespace, which no formatter's line width moves, and lines
are kept as a secondary figure. In characters the Rust Settings page is 0.88 times the TypeScript
it replaces (13,614 against 15,526, counting `overview.ts`, whose words it now carries). It draws
at phone width under a 0.67-core cap as fast as today's page (521 and 599 ms against 586 and 588
ms, medians of two runs), and costs 0.3 ms and a 23-byte patch per keystroke. Its module is 461 KB,
121 KB in brotli, since the device no longer carries the time-zone database. In lines it is 587 at
the repository's rustfmt, 1.6 times the 360 to 380 lines of comparable TypeScript.

## 1. The view tree and the patch protocol

### 1.1 The view tree (built: `app/viewtree/src/node.rs`)

A view is a value of `Node<M>`, where `M` is the page's message type:

```rust
enum Node<M, I = ()> {
    Text(I, String),
    Element(Box<Element<M, I>>),
    Foreign(I, Foreign),            // a part another painter draws, by kind, key and props
}
struct Element<M, I = ()> { id: I, tag: Tag, attrs: Vec<(Attr, String)>, on: Vec<Listener<M>>, kids: Kids<M, I> }
enum Kids<M, I = ()> { Plain(Vec<Node<M, I>>), Keyed(Vec<(String, Node<M, I>)>) }
enum Listener<M> { Click(M), Input(fn(String) -> M) }
```

- **`Tag` and `Attr` are closed sums.** A view cannot make a `script`, an `iframe` or a `style`,
  and cannot set `onclick` or `style`. `Attr::Data(&'static str)` covers `data-*`. The painter
  sets `href` only for an address inside the app (one that starts with `#`).
- **A handler is a message, never a closure.** `Click(M)` carries the message a click sends.
  `Input(fn(String) -> M)` is a plain function pointer, so it cannot capture state either. This
  is Elm's rule, and it is what makes the view a function of the model alone.
- **The id is a type parameter.** A view builds `Node<M>` with no ids. The painter keeps the same
  tree as `Node<M, u32>`, each node with the id the DOM knows it by. The tree a view answers and
  the tree the painter remembers are one type, and nothing translates between them.
- **Keyed children** keep a row's node, with its focus, scroll and selection, when rows move. The
  builder keeps the first node of a key given twice, so a view cannot name one key twice.
- **The vocabulary** is `view!{}`, a `macro_rules!` that expands to `el(tag)` and `El`'s methods
  (`set`, `click`, `input`, `child`, `keyed`, `node`) and nothing else:
  `view! { Section "settingsline" {Data("key") => &l.key} @click(Msg::Open) [heading, detail] }`,
  or `view! { Div "rows" => pairs }` for keyed children. A child is anything `Child`: a node,
  words, an `Option` or a `Vec` of either; an attribute's value is words, or a `bool` for a
  present-or-absent one. `viewtree::kit` holds the parts every page draws alike (a paragraph, a
  button that is disabled while its act runs, a disclosure) and `Reading<T>`, the web's `lo`
  printed the same way (`Loading | Error { e } | Val { v }`).

### 1.2 The diff (built: `app/viewtree/src/paint.rs`)

`Painter::paint(view) -> Vec<Patch>` compares the new view with the tree it last painted:

- A node whose kind and tag stay is kept and patched in place: attributes set or removed,
  listeners added or removed, text changed.
- A node whose kind or tag changes is made anew before the old one and the old one is removed.
- Plain children are matched by position.
- Keyed children are matched by key. Rows that left are removed; rows already in relative order
  (a longest increasing subsequence of their old places, as `web/src/algebra/reconcile.ts` does
  today) stay where they are; the rest move. Rows are placed from the end backwards, so the node a
  patch inserts before is always in place already.
- The painter also keeps each element's listeners by id, so an event the DOM reports as `(id,
  event, words)` becomes the message the view put on that node (`Painter::message`). An event for
  a node that is gone, or that no longer listens, answers no message.

### 1.3 The patch protocol (built: `app/viewtree/src/patch.rs`, `web/src/painter/painter.ts`)

A patch is one of eleven instructions: make an element, a text node or a foreign part and insert
it; set or remove an attribute; set a text; give a foreign part new props; move a node; remove a
node; start or stop reporting an event. The bytes are an opcode followed by its fields: ids and
string lengths as LEB128 numbers, strings as UTF-8. Id 0 is the root the painter was given, and
`before` is an id plus one, or 0 for the end. One `Uint8Array` crosses the wasm boundary per step;
the painter reads it with no allocation per patch except the strings.

**What crosses per keystroke, measured (§6.3):** in, the element's id, the event's code and the
field's words (a string of a few bytes, copied by wasm-bindgen); out, a patch of 23 bytes (12 to
34) and the effects as a two-byte JSON array `[]`. The step, from the input event to the patched
DOM, takes 0.3 to 0.9 ms (the medians of four runs) at 0.67 of a core. The whole view is rebuilt and diffed on every step.
That cost grows with the page, so the Todos list (about 2,000
rows at the live shape) needs a lazy node: `Node::Lazy(key, hash of its inputs, fn)`, which the
diff skips when the hash is unchanged, as Elm's `Html.Lazy` does. It is stage 4's first piece
(§7).

## 2. The painter's contract and the effects runtime

### 2.1 The painter (built: `web/src/painter/painter.ts`, 158 lines)

The painter may:

- create elements of the tags in its table, text nodes, and foreign parts the host draws;
- set and remove attributes. `value` on an input sets the property, and only when the field holds
  something else, so typing is never overwritten by its own echo. `href` is set only for `#…`;
- insert, move and remove nodes it made;
- listen for the events a patch names, and report each as `(id, event code, words)`, where
  `words` is the field's value for an input.

It may not decide anything. It holds a map from id to node and the listeners it added, and both
are a cache of the DOM: dropping the painter and painting the view again from nothing gives the
same DOM (law 1, §2.4). It never reads the DOM to decide what to draw.

**Events it reports.** Built: `click` and `input`. To add with the pages that need them: `submit`
and `keydown` with the key's name (the composer), `toggle` (a `details` opened), `focusin` and
`focusout` (the shell's keyboard), and pointer gestures as one `Swipe { direction, distance }`
event, recognised by the runtime and never by a page.

**Focus and scroll.** Focus and scroll belong to the DOM node, and a kept node keeps both: this is
why the diff keeps nodes and moves keyed rows rather than remaking them. A page asks for focus or
for a scroll position as an effect (`Focus(key path)`, `ScrollTo(key path)`), which the runtime
performs after the patches of the same step are applied. The painter reports nothing about
scroll; a page that needs it (the conversation's "follow the end") asks for `Scrolled` events on
one element.

**A Typst block.** A Typst output is placed as a node of its own,
`Node::Block { hash, fallback }`: the painter asks the block cache for the HTML of `hash`, places
it through `web/src/rich.ts` (still the one door from a string to markup, with the tripwire), and
draws `fallback` (the plain text, a real state, TYPST-FIRST §4 "Offline") until the HTML is held.
The view never holds HTML; it holds the hash. In the prototype, the Inbox's cards are placed the
same way, as `Node::Foreign { kind: "proposal", key, props }`, drawn by the existing TypeScript
until the Inbox is converted. Foreign parts are the migration's seam and go when the last
TypeScript page goes; `Block` is the permanent form of the same idea.

### 2.2 The effects runtime (built in part: `web/src/painter/run.ts`, 30 lines)

`run.start(root, module, foreign, handle)` steps the module, applies its patches, takes the
effects it asked for, and hands each to `handle`, which performs it and answers an input. This is
Unison's "effects as handlers": the program never performs anything, and the runtime is one
handler of its effects. A test swaps in a pure handler (a fake store, a fake network), which the
Rust tests do today (`app/pages/tests/boundary.rs` steps the program with no handler at all and
reads the effects it asked for).

In the prototype the effect sum is the page's own (`settings::Effect`: `Begin`, `AskTrust`,
`Renew`, `Pair`, `Toast`, `Again`), and the host's handler calls the existing `api.ts`
functions. That is app knowledge in TypeScript, and it is the part the target removes. The
target's effect sum is the runtime's, closed and generic, and every page uses it:

```rust
enum Effect {
    Write { store: StoreName, object: Literal },  // LOCALWRITES: every page write is an object
    Ask { request: Request },                     // a GET the replica cannot answer yet (mail, usage)
    Navigate { route: Route },
    Focus { at: KeyPath }, ScrollTo { at: KeyPath },
    Toast { words: String },
    Timer { after_ms: u32, then: Tick },
    Copy { text: String },
}
```

and the inputs that come back are the runtime's too: `Answered(request, Response)`,
`Moved(store)` (the worker's model changed), `Ticked(tick)`, `Route(route)`, `Visible(bool)`.
With LOCALWRITES built, a tap on Settings is a `Write` to the inbox store, and nothing in
TypeScript knows what a setup conversation is.

### 2.3 One model, one update, the view a function of it

ONEOWNER §3.0 makes the core's worker one model (the replicas) with one pure update and effects a
runtime performs. LOCALWRITES §0 makes every write an object appended to that model. This plan is
the same shape one level up: a page is a view of the worker's model plus the page's own UI
registers (which field is open, what is typed, which acts are running).

There are two ways to place the page's program, and the plan takes the first:

1. **In the worker, beside the replicas.** The worker's model is `(replicas, the open page's UI
   registers)`, its update takes both kinds of message, and `view(model)` runs there; the
   patches cross to the page as one transferable `ArrayBuffer` per step. There is one model and
   one update on the device, and a reading the view needs is a function call, not a message.
2. **In the page's thread**, with its own instance of the module, fed the worker's readings as
   inputs. This is what the prototype does (it reads what today's Settings reads), and it is a
   second model.

The cost of the first is a `postMessage` per step, which a keystroke does not wait on: the
painter never overwrites a field's own value (§2.1), so the field shows what was typed whatever
the patch's latency. The first draw waits on the worker's start, which ONEOWNER stage 3 already
measures (the capped `bigstore` warm first draw, target under 2.5 s); the box's first paint (§2.5)
covers that wait. Stage 2 (§7) measures the round trip before any page moves into the worker.

**Laws, in WEB-SPEC's words.** The view is a pure function of the model: the same model gives the
same tree, and a model replayed from the same messages paints the same DOM. A patch applied to the
painter's DOM equals a fresh draw of the new view. A step whose model does not move patches
nothing. These are built as properties (§2.4).

### 2.4 The laws as built

`app/viewtree/tests/laws.rs`, over generated views (elements of four tags and three classes,
texts, foreign parts, children by position or by keys drawn from a small set so rows are kept,
moved and lost, 512 cases each) and a DOM kept as data that applies patches the way the painter
does:

1. **A patch applied equals a fresh draw**, over runs of up to five views:
   `a_patch_applied_equals_a_fresh_draw`. Breaking the move of a keyed row fails it at once.
2. **Painting a view again patches nothing**: `painting_a_view_again_patches_nothing`.
3. **The bytes read back as the patches**: `the_bytes_read_back_as_the_patches`.
4. **Reordering keyed rows makes and removes nothing**, and moves only the rows out of relative
   order: `reordering_keyed_rows_keeps_every_node`.
5. **An event is answered by the current view's handler**:
   `an_event_is_answered_by_the_current_view`.

And `a_keystroke_patches_what_it_changed` (one field in a page of 200 rows: two patches, under 24
bytes). `app/pages/tests/boundary.rs` holds the one law of the page itself that a type does not:
an act runs once until it is answered (a double tap is two writes otherwise). The same five laws
in a real browser, against `painter.ts`, are stage 1's first test (§7): generated patches applied
by the painter and compared with a fresh draw, node by node.

### 2.5 The box's first paint (Lustre's server components)

The box links the same `suzatary-pages` crate natively. For a first visit it can run
`Runtime::start(flags)` on the reading it already has and send the first frame's patch bytes
with the page, so the painter draws before any wasm is fetched or compiled. Because a fresh
painter numbers nodes in the order it makes them, the device's runtime started from the same
model produces the same bytes and the same ids, and it continues from the box's paint without a
second draw (hydration is "the two first frames are equal", a law the box and the device can
test against each other). The prototype does not build this; §6 shows it is not needed for
Settings, whose first draw waits on its readings and not on the module.

### 2.6 Where each reading runs (moving work to the data)

| Reading | Runs | Why |
| --- | --- | --- |
| A page's view of the replicas (lists, rows, counts) | device | the replica is there; offline |
| A todo's fulfilment, a chain's verification | device (core) | one evaluator, already wasm |
| The first frame of a page on a cold visit | box | no module yet on the device |
| Typst content (a body, an answer, a brief) | either: box renders, device caches by hash | the compiler is 6 ms a page on the box, and the tab already compiles some views |
| Mail search, usage, the overview's health | box | the data is there and never synced |
| Formatting an instant in the owner's zone | device, from the zone's offsets the host hands over | the whole database is 890 KB of wasm (§6.3) |

## 3. The generated boundaries and the cache keys

### 3.1 Rust to TypeScript (built for the prototype)

The wire already is (`scripts/wire.sh`, `app/core/src/wire.rs`). The prototype adds
`web/src/wire/generated/settings.ts`, one file per page, written by `app/pages/tests/boundary.rs`
from every type reachable from the page's `Input` and `Effect` (the overview's types included).
`scripts/wire.sh` runs it, and so does `.#suzatary-pages-ts`, which `.#suzatary-web-ts` joins with
the wire's own files, so `.#suzatary-wire-ts` stays built from the core alone. It has no decoders:
the page's module reads its own inputs with serde, once, at its edge. It caught a mistake on its
first build: the host spelt a card's state `"Standing"` where serde spells `"standing"`, and tsc
refused it.

To read the overview in Rust, `Overview`, `Offer`, `OverviewGroup`, `OverviewLine`,
`connect::Standing` and `connect::Group` gained `Deserialize`. The box still only prints them.

### 3.2 Rust to Typst (stage 3)

A template is a `.typ` file called with one value of a Rust type, say `RoundAnswer`. The build
generates, beside the template, `schema/round-answer.typ`:

```typst
// GENERATED from app/core/src/answers.rs by `cargo test -p suzatary-core`.
#let text(d) = d.at("text")
#let sources(d) = d.at("sources")
```

A template reads its data only through these accessors (`#import "schema/round-answer.typ" as
answer`, then `answer.text(d)`), so a misspelt or removed field is an unknown identifier and
fails the compile, whatever data the template is given. The build compiles every template once
against a generated example of its type (every field present, every sum's first arm), so a
template that reads a field of the wrong shape fails the build too. A template never holds code
in a string and Rust never holds Typst source: the template is a file, the data is a value, and
the schema is generated.

### 3.3 Cache keys by the hash of the code (stage 3)

- **A Typst output** is keyed by the hash of (the template file and every file it imports, the
  Typst compiler's version, the canonical bytes of its datum). A template edit misses every entry
  it made and no other; nothing is versioned by hand. Today's `rendered`/`blocks` stores key by
  content and a hand-raised kit version; the kit version goes.
- **A Rust reading** kept across loads (W2's kept readings, ONEOWNER §3.0) is keyed by the hash of
  the module that computed it: the device hashes the `.wasm` bytes it instantiated (the service
  worker has them), and the box hashes its own binary at start. A new build misses every kept
  reading, and a reload of the same build hits them all. This replaces the form numbers of
  ONEOWNER rule 1 for readings; the snapshot's form number stays, since the snapshot is data, not
  a reading. Function-level hashes (Unison's) come only if a measured rebuild proves the
  module-level miss too costly.

## 4. The framework's repository, and what stays here

The framework becomes a public repository (its name is the owner's to choose; this plan calls it
`fulfilium/viewtree`). It holds what names no Suzatary concept:

- **`viewtree`** (the crate built here, 1,084 production lines): the view vocabulary and `kit`,
  the diff, the patch format, `Program`, `Runtime`, the JSON host and `export!`. It depends on
  nothing but serde and ts-rs, behind a feature.
- **The page algebra**: today's `frame` (`web/src/view/frame.ts`: a reading's `Loading | Error |
  Val` and its laws in WEB-SPEC §3.8) becomes `Reading<S>` in Rust with the same laws as
  properties; `PageState`, `step` and `draw` are `Program`'s model, `update` and `view`.
- **The painter and the runtime** (`painter.ts`, `run.ts`, 188 lines today), with the generic
  effect handlers (fetch, IndexedDB, BroadcastChannel, focus, timers, gestures).
- **The laws as tests**: `laws.rs`, and the same laws in a browser against `painter.ts`.
- **An example app**: a counter, a keyed list with a field per row, and a Typst block, built and
  tested by the repository's own CI.

**Stays in Suzatary:** the pages (`app/pages`), the wire, the Suzatary-specific effects (the
replica writes of LOCALWRITES), the Typst templates and their schemas, the shell's routes and
chrome, `rich.ts` and the tripwire.

**Not Prodrome.** Nothing in this plan is generic over a store's schema: a view tree is not a
replica, a fold or a sync. The pieces of ONEOWNER §5 (the generic replica, the persistence form,
the memo by version) stay bound for `prodrome_core`; the framework depends on none of them, and
Suzatary joins the two.

## 5. Reconciling the decisions this revises

- **`webphone`** ("TypeScript, one esbuild bundle, no framework, no npm"): no npm stays, and one
  esbuild bundle stays for the shell. The framework is this project's own, so "no framework"
  becomes "no framework we do not own", and the page code moves to Rust.
- **`typstfirst`** ("what is read is Typst, what reacts is TypeScript"): what is read stays
  Typst; what reacts becomes Rust, painted by TypeScript.
- **`webalgebra`** and WEBALGEBRA §6 ("do not import a virtual DOM"): the keyed diff is a virtual
  DOM, written here, with its laws as properties. The page algebra keeps its laws and changes
  language.

The decision to record when stage 1 lands, `frameworksplit`: "A page is a Rust program (a model,
a pure update returning effects, a view as data); a generic TypeScript painter applies its
patches and a generic runtime performs its effects; Typst renders content by template and typed
value. Supersedes the parts of `webphone`, `typstfirst` and `webalgebra` named above."

## 6. The prototype: Settings behind a flag

### 6.1 What was built

- `app/viewtree`: the generic crate (§1, §2.4), with `view!` and `kit` (§1.1).
- `app/pages/src/settings.rs`: Settings as a `Program`. Its model is what the page reads
  (`Shown`), the pairing field (`Closed | Open { code }`) and the acts running. Every tap is a
  message (`Run(act)`, or the pairing field's new state), every act an `Effect::Run`, answered by
  `Input::Answered`, and the view is a function of the model. It draws everything
  `web/src/pages/settings.ts`, `overview.ts`'s line words, `app/device.ts` and `app/pairing.ts`
  draw; the Inbox's cards are foreign parts.
- **The zone as data.** The host hands over the owner's zone as offsets from UTC, each from the
  instant it starts, read from the browser's `Intl` by `time.offsets` (`web/src/app/time.ts`,
  generic: a day at a time, and each change found to the minute) over the two years before now,
  once per zone. The page names instants only from that span; one older would be read at the
  oldest offset. The page links no zone database.
- `web/src/painter/painter.ts` and `run.ts`: the generic painter and runtime.
- `web/src/pages/settingsview.ts`: the host. It reads what `settings.ts` reads (the frame's
  reading, unchanged), hands each value to the program, performs its acts with the existing API
  calls, and draws the cards.
- The flag: `localStorage["suzatary.viewtree"] = "on"` (`web/src/pages/table.ts`), and
  `SUZATARY_E2E_VIEWTREE=on` runs every e2e scene with it set (`web/e2e/harness.ts`).
- The module: `nix build .#suzatary-views-wasm`, built by the same function as the core's module
  (`wasmModule` in `flake.nix`, which replaced the core's own copy), copied by
  `scripts/build-web.sh` beside `prodrome.js`.

**Behaviour, checked on 2.1.56.** With the flag on, every Settings scene passes: `overview` (7),
`pairing` (2), `google`, `tapsecret`, `connections` (3) and `bootstrap`, 15 of 15; with it off,
`overview` and `pairing` pass, 9 of 9. Round 0 also ran `links` with the flag: one scene failed
the same way with the flag off, where the container's proxy answers the fixture's unreachable
origin.

### 6.2 How it was measured

`web/e2e/settingsbench.bench.ts`, run as `SUZATARY_E2E_BROWSER_CPU=0.67 scripts/web-e2e.sh
settingsbench`: headless Chromium at 390 by 844 px, the whole browser in a cgroup of two thirds of
a core, the read-only fixture, the two shapes alternating with the order flipped each run, ten
fresh documents each, the first of each reported as its cold visit and the other nine as warm (the
HTTP cache holds the app). A keystroke is measured in a browser whose key nobody trusted, so
Settings offers the pairing field: 24 input events, each timed from the event to its synchronous
end. The edit loop was timed by hand on the same container (4 cores, nothing else running). Lines
are `scripts/size.sh`'s rule; content is the same lines' characters, whitespace left out.

### 6.3 The numbers

**First draw, warm, median of nine (range), two runs on 2.1.56:**

| Run | Today's page | Rust page | Rust page's parts: readings drawn, module load beside them, first paint |
| --- | --- | --- | --- |
| 1 | 586 ms (503 to 696) | 521 ms (510 to 795) | 521 ms, 104 ms, 0.9 ms |
| 2 | 588 ms (506 to 605) | 599 ms (514 to 609) | 599 ms, 101 ms, 1.2 ms |

The Rust page draws within a millisecond or two of its readings, as today's page does: both are
bounded by the readings. Round 0, with the 1.3 MB module, measured the Rust page 18 to 83 ms later
over four runs (712 against 690 ms), because a load of about 200 ms shared the capped core with the
readings; the smaller module halved the load, and the gap went with it. Before the load moved to
the page's mount, the module loaded after the readings, and the page drew in 787 against 687 ms.

**A keystroke** in the pairing field: 0.3 ms median for the Rust page in both runs (the event into
wasm, `update`, `view`, the diff, 23 bytes of patch out and applied), and under the timer's 0.1 ms
resolution for today's page, whose listener sets one `disabled`.

**The module** (`views_bg.wasm`, wasm-release with LTO, then `wasm-opt -Os`):

| | Raw | Brotli |
| --- | --- | --- |
| Round 0, with `Zone::wall` | 1,321,622 B | 188,671 B |
| As built, the zone as offsets | 460,960 B | 121,443 B |
| The core's module, for scale | 3,982,953 B | 1,008,562 B |

890 KB of round 0's module was chrono-tz's database, which `Zone::wall` reaches for any zone; the
built module holds none of it. The pages' code shares serde, serde_json, chrono's formatting and
the standard library with the core, so in one module with the core (§2.3) each page costs only its
own code: round 0 measured the pages' code at 288 KB raw beside the core's.

**The edit-and-reload loop**, from a saved edit to a module or bundle ready to reload (the
reload itself and the page's draw are the same for both):

| Edit | Steps | Time |
| --- | --- | --- |
| A TypeScript page | `tsc --noEmit -p web` 0.8 to 0.94 s, esbuild 0.06 to 0.07 s | about 0.9 s |
| A Rust page, dev profile | cargo wasm32 1.8 to 2.0 s, wasm-bindgen 0.25 s | about 2.1 s |
| A Rust page, wasm-release (LTO) | cargo 4.9 to 5.2 s, wasm-bindgen 0.07 s, wasm-opt about 1.6 s | about 6.8 s |
| `suzatary-core` (a wire type), round 0 | cargo wasm32, dev profile | 49 s |

The core's 49 s is the workspace's `incremental = false` (`app/Cargo.toml`, chosen to keep the
test tree small); a dev loop that builds the module with `CARGO_INCREMENTAL=1` would cut it.

**Size**, in characters first (the owner's yardstick, §6.4) and lines second. Round 0's page was 775 lines of Rust, written with the builder's methods. Rewritten with
`view!` and `kit`, it is 587. Against it:

| | Lines | Content (characters) |
| --- | --- | --- |
| Today's Settings in TypeScript (`settings.ts`, `settings.state.ts`, `device.ts`, `pairing.ts`) | 305 | 11,487 |
| …with `overview.ts`, whose words for a line the Rust page now carries | 382 | 15,526 |
| The Rust page at the repository's rustfmt (100 columns) | 587 | 13,614 |
| The same file at rustfmt `max_width = 120`, `use_small_heuristics = "Max"` | 448 | 13,614 |
| The same file at the TypeScript's widths (`max_width = 160`) | 420 | 13,614 |

The comparable TypeScript is 360 to 380 lines: the five files less `overview.ts`'s two readings
for Todos and `device.ts`'s own reads, which stay in TypeScript. Where the 587 lines go: the types
160 (the boundary's 140 of them, which the TypeScript gets from the wire or declares in a line
each), `init`, `update` and the top of `view` 93, the words (titles, health, causes, a key's
standing, an instant on the owner's clock) 146, and the view's parts 188. Per line the Rust holds
23 characters against the TypeScript's 38: rustfmt puts each argument, field and arm on a line of
its own where the TypeScript runs to 206 columns.

Once, and not counted in the page: `app/viewtree` 1,084 lines of Rust, `web/src/painter` 188 of
TypeScript, `time.offsets` 17, and the host (`settingsview.ts`, 139), which goes when the acts
become the runtime's (§2.2).

### 6.4 Go, decided

The trade-off the owner wanted settled, on these numbers:

- **The draw and the keystroke** do not argue against the move: the first draw is the readings'
  in both shapes, and a keystroke is 0.3 ms and 23 bytes.
- **The module** is 121 KB of brotli, once per build, shared by every page; in the core's module,
  less.
- **The edit loop** is slower, 2.1 s against 0.9 s per page edit, and 49 s after a core edit.
- **The size**: the same content, laid out longer. At the repository's rustfmt the page is 1.6
  times the comparable TypeScript in lines; at 120 columns 1.2 times; in characters 0.88 to 1.19
  times.

**Go, decided by the owner in review round 2 (2026-11-03).** The yardstick for a page's size is
characters without whitespace (or tokens), which formatting cannot inflate; lines are reported
beside them as a secondary figure. On it the Rust Settings page is 0.88 times the TypeScript it
replaces. `scripts/size.sh` prints both for every change, against the merge-base, and the merge
gate prints it in every report. The conditions:

1. **No time-zone database on the device.** Built: the zone reaches the page as offsets.
2. **One module for the app.** The pages link into the core's module, so serde, chrono and the
   standard library are paid once and the model and its views share memory (§2.3). Stage 2.
3. **The module loads when the route changes**, beside the readings. Built.
4. **A page is measured in characters.** Decided: the repository's rustfmt stays as it is, and
   each stage reports the characters it removes and adds before its lines.

**The no-go, which the owner did not take**, would have converted no page and kept what pays on
its own:

- **Effects as data, in the TypeScript pages.** `frame`'s `step` already returns a value; it would
  return the effects too, performed by one runtime, so a page's tests swap in a pure handler.
- **The generated boundaries.** The wire stays generated; the Rust-to-Typst schemas of §3.2 and the
  caches keyed by the template's hash (§3.3) are worth building either way.
- **The zone as data.** `time.offsets` serves any reading that formats an instant from the core.
- **Not the patch protocol.** It pays only with a Rust view to produce the patches, or the box's
  first paint (§2.5), which Settings does not need: it would be deleted with the prototype.

## 7. Stages

Each stage is one PR, a series of commits, each buildable and tested, each deleting the old
page's TypeScript. Each reports its size from `scripts/size.sh`: the characters of production code
before and after, in Rust and in TypeScript, first, and the lines second, and the page's own
characters against the TypeScript it deletes, as §6.3 does for Settings. Each adds the laws "the view is a pure
function of the model" and "a patch applied equals a fresh draw" for its page (a property over
generated models, through the DOM kept as data), and passes the page's e2e scenes with no
flag. Pages go smallest first by their own TypeScript lines (`web/src/pages`, production lines).
The stages are 2.3's work, and each begins when the owner says.

1. **The framework in a browser, and Settings for good.** The painter's laws in a real browser;
   `Effect` and `Input` become the runtime's (§2.2) for what
   Settings uses (`Ask`, `Navigate`, `Toast`; `Write` waits for LOCALWRITES stage 4, so the taps
   stay requests until then); the flag goes; `settings.ts`, `settings.state.ts`, `device.ts`'s
   view and `pairing.ts` are deleted. *Measure:* the bench of §6 with no flag, where the module's load no longer shares the core
with the readings once it is in the service worker's cache and compiled. *Size:* −11,487 characters, −305 lines of TS.
2. **One module, in the worker.** The pages link into `suzatary-wasm`; the page program runs in
   the core's worker (§2.3), the patches cross as one transferable buffer per step. *Measure:* a
   keystroke's round trip through the worker; the capped first draw of Settings and of
   `bigstore`. *Law:* a model replayed from the same messages in the worker and in a test paints
   the same bytes.
3. **Typst by template and schema, caches by code hash.** The schema generator (§3.2), the block
   node (§2.1), the Typst caches keyed by the template's hash (§3.3); the brief and the round's
   answer move first. *Law:* every template compiles against its type's generated example; a
   missing field fails the build (a test that removes one and expects the failure).
4. **The small pages:** memory (25), watches (31), calendar (37), briefings (48), about (52),
   thread (60), briefing (62), releases (98), each with its `.state.ts` where it has one; the
   lazy node (§1.3) lands first. *Size:* about −15,000 characters, −413 lines of TS.
   *Built (2026-11-03), on stage 5's runtime:* the lazy node (`viewtree::node::lazy`; the painted
   tree is `Node<M, u32, Never>`, so no lazy node stands in it); memory, About (with the
   releases), the calendar, the briefings and a briefing (`app/pages/src/`); thread and watches
   went with stage 5. A page of one reading is a `reading::View` and `reading::One` its program,
   which the thread and the message now use too. *Size:* the five pages are 17,485 characters and
   753 lines of Rust against the 15,202 and 414 of TypeScript they replace (1.15 times in
   characters); `reading.rs` is 2,821 more, and took 2,161 out of the thread and the message,
   which brings stage 5's pages to 1.19 times.
5. **Mail and message** (148, 152, with `mailrow.ts`). *Built (2026-11-03):* Mail, a message and
   a thread (`thread.ts` read `mailrow.ts` too, so it moved with it, and `watches.ts` with Mail, its
   one reader) are `app/pages/src/{mail,message,thread}.rs`. They speak the runtime's sums
   (`runtime.rs`: `Effect<H, A>` holds a question, asks it again, gets or posts once, goes, goes
   back, toasts; `Input<H, A>` is the context, a held reading or an answer), performed by one host
   that knows no page (`web/src/pages/program.ts`), and draw their chrome as `frame.ts` does
   (`chrome.rs`) until stage 9. *Size:* the pages are 17,752 characters and 813 lines of Rust
   against 13,056 and 422 of TypeScript (1.36 times in characters); the generic pieces they added
   (the runtime, the zone, the addresses, the chrome) are 7,965 more, and the DOM as data
   (`viewtree::dom`, 4,648) moved out of the painter's tests so every page's laws share it. The
   TypeScript fell by 8,986 characters and 279 lines, with the host and its parts (4,801) counted.
6. **Conversation, secretary and the Inbox** (179, 238, 244, with `secretary.state.ts`,
   `inbox.state.ts`, `web/src/chat` and `web/src/inbox`): the foreign parts go with the Inbox's
   cards. *Size:* about −164,000 characters, −4,700 lines of TS. *Built (2026-11-03):* §7.2.
7. **Usage and chain** (275, 283). *Decided (2026-11-04):* deleted, not rewritten, with
   `GET /api/usage` and everything only they read (§7.3).
8. **Todos and the todo page** (416 + 223, with `web/src/todo`), after ONEOWNER stage 6 puts the
   chains in the core. *Measure:* the list's first draw at the live shape, against today's.
   *Built (2026-11-04), with stage 7:* §7.3.
9. **The shell.** Navigation, the palette, the panel and the keyboard become one program; `frame`,
   `dom.ts`'s `el`, `act.ts`, `source.ts` and `reconcile.ts` go; what remains in TypeScript is the
   painter, the runtime's handlers, `rich.ts`, the service worker, the boot loader and the
   worker's IndexedDB adapter. *Measure:* the TypeScript left (target 2,000 to 3,000 lines).
10. **The repository split.** `app/viewtree`, the painter, the runtime and the example app move to
    the public repository; Suzatary takes it by revision, as it takes the Prodrome.

Stages 4 to 8 can run in parallel once stage 2 has landed. Stage 10 can happen any time after
stage 3.

### 7.1 Stage 1, as built (on 2.1.60, after stage 5)

Stage 5 (#316) landed the runtime's sums first, so stage 1 built Settings on them rather than on a
second runtime.

- **The runtime's sums.** Settings uses `Hold` and `Again` for the overview, `Go` for a begun
  conversation, `Toast`, and three arms this stage adds, each generic: `Post` carries the route's
  request (`Body`, a closed sum of the core's request types), `NewKey` puts a new key in this
  device's place, and `Watch` holds a reading the tab keeps (`Local`: this device, the Inbox's
  cards; `web/src/pages/local.ts`), handed over as `Input::Held`, as the box's answers are.
  `Gate` gains `Views` (no actor), which Settings needs to hide Ask, and the host sends the
  context again when the actor or whether it writes changes. The next page adds a `Local` arm or a
  `Body` arm, never a handler; `Write` comes with LOCALWRITES stage 4.
- **The host.** Settings is mounted by `program.mount` as the mail pages are, and draws its head and
  status through `chrome.rs`. `settingsview.ts`, `settings.ts`, `settings.state.ts`, `pairing.ts`
  and `device.ts`'s view are gone. The Inbox's cards are parts in `parts.ts` until stage 6. When
  the watched cards change, the program asks the overview again, since a card the box files changes
  which line asks for what. Nothing in `web/src/painter` names a page.
- **The laws.** `settings_laws` in `app/pages/tests/laws/`, and in a browser
  `web/e2e/painter.test.ts` (WEB-SPEC §3.22) over Settings' program through `Runtime::fresh`; with
  the painter's move made a no-op the browser scene fails at its first seed.
- **Behaviour.** With no flag, `overview` (7), `pairing` (2), `google`, `tapsecret`, `connections`
  (3) and `bootstrap` pass, 15 of 15, and so do stage 5's `mail`, `waiting`, `keyboard` and
  `desktop`. The Inbox's card for this device no longer carries a code field: it links to
  Settings, where the code is typed.
- **Size** (`scripts/size.sh` against stage 5's branch, `8b1aafc`): Rust +2,801 characters (+130
  lines), TypeScript −15,115 characters (−375 lines).
- **The bench** (§6.2, the same container, two runs each, warm medians of nine):

  | | Run 1 | Run 2 |
  | --- | --- | --- |
  | Today's page, before (`50232d3`, no flag) | 599 ms (518 to 786) | 606 ms (591 to 681) |
  | The Rust page, before (`50232d3`, flag on) | 614 ms (595 to 784) | 696 ms (614 to 797) |
  | The Rust page, after (no flag) | 500 ms (410 to 692) | 491 ms (399 to 590) |

  The "before" rows read the end of the draw that first showed the lines (`page:draw`); the "after"
  row reads a mark set when the first line stands in the document, since that measure belonged to
  the host this stage deletes. The page draws sooner because the overview no longer waits on this
  device's reading beside it. The module's load is 95 ms on every warm visit, before and after: a
  fresh document compiles it again, so a load that no longer shares the core is not met yet. A
  keystroke is 0.4 to 0.5 ms and a 23-byte patch (12 to 34). The module, now four pages, is 737,922
  bytes after `wasm-opt`.

### 7.2 Stage 6, as built

- **The pages:** `inbox.rs`, `secretary.rs` (with `chats.rs`), `conversation.rs` (placed `Page`
  or `Panel`), `draft.rs` and `needsyou.rs`. Their shared parts are `card.rs` (with
  `digest.rs`), `waiting.rs`, `composer.rs`, `answer.rs`, `intents.rs` and `slices.rs` (a long
  list drawn two rounds a step, so no task draws it whole). Settings draws the Rust card;
  `parts.ts` keeps the marks, Typst's HTML, prose, a seconds clock and a message's todos.
- **The runtime** gained, all generic: the core's readings as `Local`s; `Act` (an intent whose
  typed value never reaches the page, told as answered or as held), `Sync`, `Endorse`, `Listen`,
  `Timer`, `Focus`, `Scroll`, `Copy`, `Replace`, `Mark`, `Tell`; the reach and width in
  `Context`. The host is `program.ts`, `local.ts`, `painter/scroll.ts` and `painter/files.ts`.
- **The view tree** gained the tags, the keydown, toggle and file events, an untaken event left
  to the browser, `Node::map` and `keydown_of`.
- **The laws** are in `app/pages/tests/laws/` for every page, with the vectors' open
  proposals in `app/pages/tests/vectors.rs`.
- **Stays in TypeScript** until stages 2 and 9: the readings' adapters (`dressed.ts`,
  `drafts.ts`, `draftbox.ts`), the worker's grouping (`grouped.ts`, `rounds.ts`, `days.ts`),
  `refs.ts`, and the panel's placement (`aside.ts`, `inview.ts`).
- **Next:** an `Effect::map` over held and asked names, as `Node::map` is over messages, would
  replace the hand lifts `card::effects`, `composer::effects` and `Searching::typed`.

### 7.3 Stages 7 and 8, as built (on 2.1.77)

- **The chain and usage pages are deleted**, on the owner's word (2026-11-04), rather than
  rewritten: `chain.ts` (the raw events), `usage.ts`, `view/rows.ts` and their scenes, their
  routes, their places in the bar and the More sheet, and the box's `GET /api/usage` with the
  core's usage report, its capability and the reader of the transcripts it priced. `GET
  …/chain` stays: the web vectors and the write scenes read it. *Size* (`scripts/size.sh`, that
  commit against 2.1.76): Rust −17,165 characters (−751 lines), web −20,333
  (−614): `usage.ts` 8,583, `chain.ts` 9,397, `rows.ts` 1,177, the rest the shared code that
  named them.
- **The pages.** The todo list (`todos.rs`), a todo on its page and in the list's pane
  (`todo.rs`), and the Typst list (`typstlist.rs`) are programs; what every page says of a todo
  (its acts and their words, its title, its chips, an act's two phases) is `todoparts.rs`, the
  fields every form draws (a line, a day, chips, Typst markup in the editor part, the price
  picker) are `form.rs`, and the one path of an edit (the box prints, the page seals) is
  `authoring.rs`. The list hands its members and its grouping to the core in the tab's worker,
  which grows the tree with `suzatary_core::tree` (`app/wasm/src/grow.rs`) and prices every node
  (`Core::Tree`), so `grouping.ts` and `row.ts` went with no copy and the pages' module links no
  term and no trie. `todos.ts`, `todos.state.ts`, `overview.ts` (the list's setup line, now
  `connect::Standing::asks`) and `typstlist.ts` are deleted, and so are the modules of
  `web/src/todo` only they read (the add sheet, the edit form, the filter bar, the grouping, the
  row, the pane, the draft, the reading, the recurring form, the price) and the old list's pieces
  of `todo/todos.ts`; what stays there is read by the shell, the palette, the Typst list's state
  and the message's todo card. A todo's fields are Typst blocks placed by key, as
  #317 asked: the page shows the source until the device holds the HTML, which the box's folds and
  its page of the todo (`Effect::Rendered`) bring.
- **The runtime,** each arm generic: `Effect::Core` (a question of the tab's core: a reading, a
  price, the list's tree, thirty days), `Offer` (a toast with a button, answered as its ask), `Reset`, `Keys` (the
  keys a page takes, heard as frames), `Rendered`; `Act::Seal` and `Act::Lifecycle` (an event the
  tab's core spells at the tap); `Local` gained the fold, what is unsent, what waits and a kept
  register; `Effect::map` lifts a part's effects, as `Node::map` lifts its messages. The view tree
  gained `Ol`, the form attributes, an intent's click (`On::Intent`) and a foreign part brought to
  new props in place. The painter waits for a block's HTML from a join that lands while it reads.
- **The laws.** `todo_laws`, `list_laws` and `typst_laws` in `app/pages/tests/laws/`; beside the
  code, that an answer to a replaced ask is not drawn and that every ask of the list reads back
  as itself, two defects the scenes found, and that each refusal of the add sheet names the
  field it is about (`addsheet.test.ts`'s law, which read the deleted TypeScript sheet).
- **Size** (`scripts/size.sh` against 2.1.76): Rust +84,023 characters (+3,697 lines), web
  −108,494 characters (−3,197 lines); without the deletions above, Rust +101,188 (+4,448) and web
  −88,161 (−2,583). The pages' own Rust (`todo`, `todoparts`, `form`, `authoring`, `held`,
  `todos`, `typstlist`) is 96,239 characters (4,194 lines) against the 96,496 (2,815) of
  TypeScript they replace, and the core's `grow` is 2,300.
- **The modules,** after `wasm-opt`, raw and with brotli as the box serves them. The views
  module is 3,446,635 bytes (679,930) on 2.1.76 and 4,080,520 (792,209) here: the three pages'
  code and the serde of the wire types they read, which replaces the TypeScript above. Growing
  the list's tree in the core's module took 83,294 bytes (24,932) out of the views module; the
  core's is 4,360,126 (1,106,115) on 2.1.76 and 4,421,820 (1,128,065) here. Both cost 134 kB more
  over the wire than on 2.1.76.
- **The cold stream** on `bigstore` (its longest task, three runs each, then three beside three
  other scene files): 68, 0 and 0 ms on 2.1.76 and 65, 0 and 0 here; under that load 110, 51
  and 52 on 2.1.76 and 0, 50 and 84 here. The task is the stream's first input into the views
  module on both, and what varies is the main thread's own work around it, where V8 compiles a
  wasm function at its first call: 11 to 69 ms of a 61 to 122 ms stretch on 2.1.76, 28 to 87 of
  78 to 123 here, sampled. The limit of 100 ms has no room for it on either.
- **A page draws for the newest navigation** (#351, 2.1.77). The list, a todo and the Typst
  list are `program.mount`'s `Draw` like every Rust page: the views module is awaited, then the
  page is drawn in one step only if its navigation is still the newest. Here the list waits on
  the module too, so #351's scene reads each page title drawn after the tap, and fails naming
  Briefings when `main.ts` draws a page whose navigation was overtaken.
- **What it does not do yet.** The add sheet's focus is not held inside it while it is open, and a
  todo's edit form keeps no draft across a remount (the list's sheet does). Stage 9 (the shell)
  owns both.
- **The crate's interface.** An item of `app/pages` is `pub` only where its tests or the wasm
  boundary name it; the types reachable through one stay `pub(crate)`, unnameable outside, which
  is what the crate root's `allow(private_interfaces)` says. The unit's use is 16.8k tokens
  against its 25.0k (22.0k before), most of it the runtime's effects.
- **A container's hang, not the branch's.** Three `shell` scenes that start a tab on a new build
  fail in the cloud container this was built in, on this branch and on stage 6's head alike:
  after a tab opens the Typst list on a build the box has left, `register()` and `update()` of
  the next build's worker never settle and no version of it is made. On GitHub's runners (train
  216, stage 6's head) the same scenes pass.

## 8. The plan read two ways

**As Evan Czaplicki would read it: one model, one update, views as pure functions, no hidden
state.**

- *One model?* On a converted page, yes: everything the page knows is `Model`, and the pairing
  field's words, which today live in a `cardui` register beside the page, are a field of it. On
  the device as a whole, not yet: the prototype's program runs in the page's thread, fed the
  worker's readings, which is a second model. §2.3 puts the program in the worker so the device
  has one, and stage 2 builds that.
- *One update?* Yes: every tap is a `Msg` and every answer an `Input`, stepped by one `update`
  that returns effects as data. The runtime performs them and never changes the model.
- *Is the view a pure function?* Yes, and the type enforces it: a handler is a message or a plain
  function pointer, so a view cannot capture state, and `view` takes `&Model`.
- *Hidden state?* Three pieces remain outside the model, each named: the painter's id map (a
  cache of the DOM, rebuilt by a fresh draw), the host's `Running` sum (whether the module is
  loaded, which is traffic), and the foreign cards, whose state is still the Inbox's TypeScript
  until stage 6.

**As a performance engineer would read it: what crosses the wasm boundary per keystroke, and
what the first draw waits on.**

- *Per keystroke:* the field's words in (a few bytes, one copy), 23 bytes of patch and two bytes of
  effects out, 0.3 ms at 0.67 of a core. The view is rebuilt and diffed whole each step, which is
  linear in the page; the lazy node is required before the Todos list moves (stage 4).
- *The first draw waits on:* the page's readings (the overview, the device, the Inbox's rows),
  exactly as today. It used to wait on the module after them, which cost 98 ms; loading at mount
  removed that, and with the module at 121 KB its load (about 100 ms) no longer delays the
  readings it runs beside. In the worker (stage 2) it also waits on the worker's start, which the box's first
  paint (§2.5) can cover if stage 2's measure says it must.
- *What it costs to fetch:* 121 KB of brotli for the module, paid once
  per build and shared by every page; inside the core's module, less.
- *What it costs to edit:* 2.1 s per page edit and 49 s per core edit, against 0.9 s today.
