# Where the pages' work runs (2026-11-22)

Status: open. Roadmap `streamfirstcompile` and `wasmsize`; it touches `coreskeleton` and
`wirecodec`. The stages in section 3 are the follow-up work.

The browser client has two wasm modules. The store's module (`prodrome_bg.wasm`) runs in a
worker and holds the stores' replicas. The views' module (`views_bg.wasm`) runs on the page's
thread and turns a page's model and its inputs into patches. This note asks which work belongs
on which thread, and answers by measurement. It compares three designs:

- **(A)** Keep the two modules where they are, and move or shrink what the page's thread
  decodes.
- **(B)** Run one module in the worker that holds the replicas and the pages' runtime. The page's
  thread paints the patches it receives and sends inputs back.
- **(C)** Build one module file, and instantiate it twice: once in the worker and once on the
  page.

## 1. The answers

### 1.1 What runs on the page's thread that does not need to

The views are not the cause. Section 4 lists every task over 50 ms. At 1x on an idle machine the
first page has none over 100 ms of CPU. At a CPU throttle of 4 the longest task per visit has a
median of 132 ms cold and 121 ms warm, and at 10x it is 226 to 840 ms. Most of those tasks hold
the open chain's replica, decoded and read on the page's thread, and the core in the worker
reads the same objects again. Across a whole visit at 1x, the page's thread runs a median of
553 ms of sampled JavaScript cold and 330 ms warm. The chain's replica code (`replica.ts`,
`objects.ts`, `remote.ts`, `idb.ts` and what they call) is 132 ms of it cold and 135 ms warm,
41% of a warm visit. At a throttle of 4 it is 470 ms of 1,761 ms cold and of 1,125 ms warm.
These run on the page's thread and need not:

- **The chain's replica, decoded on the page.** A warm visit reads the chain's row from
  IndexedDB and decodes it (`objects.ts`, `decoder.ts`). It then scans every object's literal with
  regular expressions to find its parents and genesis (`replica.ts`: `prodromeOf`, `restsOn`,
  `parentsOf`, `headsOf`). Every sync writes the whole row back (`idb.ts`). A cold visit fetches
  the chain's objects from the box on the page's thread (`objectsSince` through `http.ts` and
  `decoder.ts`) and joins them (`replica.ts`). Measured CPU per task: up to 135 ms at a throttle
  of 4 on the 1x store; at 10x up to 200 ms idle and up to 1,018 ms at a throttle of 4.
- **The same objects sent to the worker.** `remote.hold` computes the change since the last
  holding and copies the chain's objects to the worker by structured clone: 1.09 MB in five
  messages per visit at 1x. The worker prints them as JSON for the core, which parses them again.
- **The box's fold of the open chain, decoded on the page.** `sync.boxFolds` and `ensureFold`
  fetch `/api/fold` and decode the `FoldPayload` through `http.get` on the page's thread, for the
  `Decisions` reading and the todo list's first paint: up to 134 ms of CPU per task at a
  throttle of 4, and 83 to 137 ms when idle at 10x.
- **The fold of the open chain, cloned as objects.** The worker answers `List` with a
  `FoldPayload` decoded into 7,293 JavaScript objects (505 KB at 1x), and the page deserializes
  that tree.
- **The stores' head frames, decoded on the page.** `storesync.ts` decodes each frame of the
  box's event stream and sends the store's heads to the worker as `Heard` (88 KB in five
  messages at 1x): up to 308 ms of CPU per task at 10x and a throttle of 4.
- **The shell's due pass over the open chain** (`app/due.ts` over `todo/todos.ts` and
  `todo/term.ts`): up to 58 ms of CPU per task at a throttle of 4, and up to 309 ms at 10x.
- **The views module's code cache, read on the page's thread** (`v8.wasm.compileConsume`,
  `wasm.Deserialize` and its digest): up to 100 ms per task at a throttle of 4.

The views' own steps are short. The longest step per visit has a median of 7.4 ms at 1x and
27 ms at a throttle of 4 (50 ms at most); at 10x, 15 ms and 69 ms. A frame that applies a slice
of the stream and lays it out costs up to 74 ms of CPU at a throttle of 4 on the 1x store, and
up to 129 ms at 10x. No design in this note changes that cost.

### 1.2 The choice

**Choose (A).** The worker becomes the one owner of every replica, the chains included. This is
stage 6 of `docs/ONEOWNER-PLAN-2026-11-02.md`, which this note now has measurements for. The
answers that cross to the page become bytes that the page parses once. The views stay on the
page's thread. Then compile the views module in the worker and post the compiled module to the
page.

**Reject (B).** A keystroke's step is as slow in a worker as on the page, because the step costs
the same CPU on either thread. (B) adds three costs, each measured in section 5:

- The reply to an input is an ordinary task, and the page's thread runs it after its other
  queued work. A browser runs an input event before ordinary tasks, so (A) keeps that priority.
- The store's work and the input's step then share one thread. A keystroke waits behind a
  store task: about one second at the median in the experiment.
- The composer's text and the scroll position then live on two threads, which needs a
  sequence number on every field echo and a layout read on the page.

**Reject (C) as a merge of the two crates.** One module saves 0.80 MB of the 9.03 MB that the two
modules ship (176 KB of 1.93 MB after brotli). It saves little compile time, and it does not
remove a single decode. The page's instance of a larger module reads a larger code cache on the
page's thread, unless the worker compiles it. If the worker compiles each module and posts it,
the page compiles nothing in either case, and the two modules can stay separate.

**Does (A) reach "no task over 100 ms on the first page, on a loaded phone"?**

- At a CPU throttle of 4 on the page's thread, stages 1 to 4 below remove every long task
  except the slice frames' layout and paint. On the 1x store those frames reach 105 ms of wall
  time at most, and at 10x 129 to 197 ms. To reach the bar, the stream must also draw a round in
  smaller pieces than it does today. That is stage 5.
- When the whole browser has two thirds of one core (`SUZATARY_E2E_BROWSER_CPU=0.67`), no
  design reaches the bar. The page's tasks use 2 to 45 ms of CPU, but they wait for the core
  while the worker uses it, so their wall time is 100 to 400 ms. On such a device only less
  total work helps. (A) removes work: the page's second decode of every chain object. (B) and
  (C) only move work.

### 1.3 The interface between the threads, and what the page's thread keeps

The page asks the worker questions, and the worker answers each one once. This protocol
exists today (`web/src/prodrome/questions.ts`, `remote.ts`). (A) changes what crosses, not the
shape:

```ts
// Page to worker: a question, in the lane of whoever asks it.
type Asked<K extends Kind> = { readonly id: number; readonly lane: Lane; readonly question: Question<K> }

// Worker to page: the answer to question `id`, or what the worker says unasked.
type Answered<K extends Kind> = { readonly id: number; readonly answer: Settled<Text<Answer<K>>> }
type Out = Answered<Kind> | { readonly said: Said }

// Bytes of JSON of a value of `T`, printed once by the core. The worker transfers the buffer,
// so the page does not copy it, and only the views module or `d.json` parses it.
type Text<T> = { readonly bytes: Uint8Array }

// `Hold` goes. The page holds no replica, so it has no objects to hand over.
// `List`, `Price` and the other answers that are object trees today become `Text`.
```

The page's thread keeps these, because each one must be read or written in the same task as the
DOM event or the patches:

- **The DOM and the painter.** Patches must be applied in the task that has the DOM.
- **The views' runtime.** One step and the inputs that its effects answer at once are one
  instant (`web/test/mailbox.test.ts`). With the views on the page's thread, an instant stays one
  task.
- **The composer's text, caret, selection and IME composition.** The browser draws each typed
  character itself. The model's echo of the text never races a keystroke on one thread.
- **The scroll position and everything read from layout.** A step that keeps the reader's place
  reads the scroll box's height and position just before its patches (`run.ts`, `scroll.ts`).
- **A keydown's decision to prevent the browser's default.** A keydown handler returns its
  message or nothing (`viewtree::Listener::Keydown`), and the painter must call
  `preventDefault` inside the event's dispatch.
- **Focus and the secret fields.** `Focus` acts on the DOM after the patches, and a `Secret`
  field is read from its input element at the moment of the act.

**The laws of the page under (A).**

- "One instant is one task" still holds: the views run where the DOM is.
- The draw stays idempotent. Readings arrive from the worker as inputs, as they do today, and the
  page's own runtime makes every patch.
- A second tab has its own worker and core. The two converge through the stores' log, as
  `docs/ONEOWNER-PLAN-2026-11-02.md` section 2.2 states. (A) changes nothing here; it removes the
  copy of the chains that each tab's page holds today.

### 1.4 What it settles on the roadmap

- **`streamfirstcompile`**: changed. Its remaining long tasks are the chain's decode on the page
  (stage 1) and the module's code cache read on the page (stage 4). The rehearsal's eight missed
  functions under load are outside this note. The task completes when stages 1 and 4 have
  landed and the bench shows no task over 100 ms at 1x under the e2e load.
- **`wasmsize`**: settled on the question of one module or two. Keep two modules. One module
  saves 9% of the bytes and nothing else that the bench measures.
- **`coreskeleton`**: changed. Stage 1 puts the chains' sync in the worker's queue, ahead of or
  beside the stream's question. Stage 1 must hold stage 3's timing target (the stream answered
  under 1 s from asked) and add the chains to its measure.
- **`wirecodec`**: untouched in scope. Stage 2 makes every answer bytes. The pages then decode
  every answer through one door, and `wirecodec` can replace that door's decoders without a
  derive per type.

### 1.5 What argues against (A)

**The strongest case against it.** On a phone whose cores are busy, the worker and the page
share the same CPU. Stage 1 takes about 135 ms of CPU per 1x load off the page's thread. Some of
that work must still happen in the worker, and the worker is already the busiest thread in a
load: it pulls and reads the lines. Under the whole-browser cap the page's thread waits for the
worker, so its long tasks can stay long. The gain then comes only from the part that stage 1
deletes (the second decode and the regular expression scans), not from the part it moves. Also, (A)
leaves the views' step on the page's thread. A keystroke in a large conversation costs two steps
of 23 ms each at a throttle of 4. A page whose step grows will hold the page's thread while the
owner types, and (B) would at least keep the page's thread free for scrolling.

The answer to the second point: the browser scrolls on its compositor thread, not on the page's
thread, and (B) makes the typed state slower and less safe.

**What result of stage 1 stops this plan.** Stop and plan again if any of these holds after
stage 1:

- The first page at a throttle of 4 still has a task over 100 ms on the page's thread, and
  more than 10% of that task's samples are in `web/src/prodrome` or `web/src/wire`.
- The stream is answered later from asked, by more than 10% at the median of ten warm loads
  under the whole-browser cap, because the chains' work now waits in the worker's queue in front
  of it.
- The total CPU of a warm load (the page's thread and the worker together) does not fall.

## 2. How each figure was measured

Every figure in this note is measured on the machine below by the method below, unless the
sentence that gives it says "estimated".

**The machine.** A cloud container with 4 cores of an Intel Xeon at 2.10 GHz and 15 GB of
memory, Linux 6.18, and headless Chromium 154.0.8037.92. The fixture server is
`suzatary-dev fixture-chain`. A debug build served the 1x store and a release build served the
10x store, which it generates faster.

**The store.** "1x" is the `bigstore` fixture as `scripts/web-e2e.sh` makes it: 13,000 lines,
100 conversations, 1,500 reseals, 400 proposals, 900 topic objects and 600 todos. "10x" is ten
times each count.

**The bench.** A local copy of the `openprof` first-page phase, not committed. Each visit opens
the stream as the document's first page and ends as `longtasks.settle` ends a phase. Each visit
records a Chromium trace with the timeline, V8's samples, garbage collection and wasm events. A
cold visit first deletes every IndexedDB database except the device's keys, every cache and the
service worker, and clears the browser's cache. A warm visit loads the same page again. Each
condition has ten cold and ten warm visits, and every figure is a median of ten unless it says
otherwise.

**Load.** "Idle" is the browser alone. "Throttle 4" is `Emulation.setCPUThrottlingRate` at 4,
which slows the page's thread only (Chromium refuses it for a worker). "Cap" is
`SUZATARY_E2E_BROWSER_CPU=0.67`: one cgroup gives the whole browser two thirds of one core, the
worker included.

**Reading a trace.** A script outside the tree lists each task of the page's thread over 50 ms.
It gives the task's wall time and its thread's CPU time, the time of the trace events inside it
by kind, and V8's samples mapped through the bundle's source maps to the source files and their
callers. It skips the task in which the profiler starts. The counts of what crossed come from a
wrapper around `Worker` in the page. That wrapper adds work to the page's thread, so the task
tables for idle and a throttle of 4 come from runs without it.

**What was not measured.** At 10x this note gives no cap figures and no counts of what crossed:
a cold visit at 10x takes about three minutes, and those runs did not fit in the session.

## 3. The stages

Each stage is one pull request that lands alone, with its law and its measure.

1. **The chains in the worker** (`docs/ONEOWNER-PLAN-2026-11-02.md` stage 6). The worker holds
   each chain as a replica of the generic kind and makes the chain's requests to the box. The
   page asks for a chain's fold and its unsent objects, and draws the answers.
   - *Law*: no module of the page's bundle imports `prodrome/replica.ts` or the chain's row
     decoder. A test reads esbuild's metafile and checks the import graph from `main.ts`. The
     rollback law of the replicas moves over the core, as that plan says.
   - *Measure*: the first-page bench at 1x and at a throttle of 4, cold and warm. No task has
     more than 10 ms of samples in `web/src/prodrome` or `web/src/wire`, and the bytes sent to
     the worker for the first page fall from 1.09 MB to under 10 KB.
   - *Deletes*: the page's `Replica` and its state in `sync.ts`, `remote.hold` and its delta,
     the `chain` rows' decoders in `objects.ts`, and most of `replica.ts`. Estimated: about
     1,200 of the 1,629 lines of `sync.ts`, `replica.ts`, `objects.ts`, `remote.ts` and
     `handover.ts`.
2. **Every answer is bytes.** The questions whose answers are object trees today (`List`,
   `Price`, `Tree`, and the rest that `questions.ts` answers with a decoded value) answer
   `Text<T>`, and the worker transfers the buffer. The worker also fetches the box's fold of a
   chain (`/api/fold`), and the page asks for it as a question. That fold then has one owner, as
   `docs/ONEOWNER-PLAN-2026-11-02.md` stage 8 asks. The worker also opens the stores' event
   stream (`/api/stores/events`) itself, since a dedicated worker has `EventSource`, so the page
   decodes no head frame.
   - *Law*: a type-level test says that every `questions.Answer<K>` is `Text<…>`, `null` or a
     sum of fewer than eight fields.
   - *Measure*: the page's garbage collection time during the first page, and the bytes and
     objects that cross.
   - *Deletes*: the worker's decode of the core's JSON into objects for those questions, the
     page's `boxFolds`, and the page's decode of the stores' head frames in `storesync.ts`.
3. **The due pass in the core.** The shell's due pass (`app/due.ts`) becomes a question that
   the core answers from the chain it holds.
   - *Law*: the due list from the core equals the one `due.ts` computes, over generated chains,
     until `due.ts` is deleted in the same pull request.
   - *Measure*: no task over 50 ms with samples in `todo/term.ts` or `todo/todos.ts`.
4. **The views module compiled in the worker.** The rehearsal worker compiles the module and
   posts the `WebAssembly.Module` to the page, which only instantiates it. A posted module shares
   its compiled code between the two threads.
   - *Law*: the page's thread runs no `wasm.Deserialize` and no `v8.wasm.compileConsume` over
     50 ms (a bench assertion, as `bigstore` checks the functions the page compiles).
   - *Measure*: `views:load`, cold and warm, idle and at a throttle of 4.
   - *Deletes*: the page's `compileStreaming` call.
5. **A round drawn in pieces.** The stream draws a round's turns over several frames, so no
   frame lays out more than a set budget of words. This stage is needed to reach the bar at a
   throttle of 4. It belongs to the views' runtime and is outside the choice among (A), (B) and
   (C).

## 4. What is on the page's thread today, measured

### 4.1 The stream as the first page, 1x

Longest task per visit, wall time in ms, ten visits each. "0" means that the visit had no task
over 50 ms.

| Condition | Cold | Warm |
|---|---|---|
| Idle | 109, 0, 0, 0, 0, 0, 0, 0, 0, 51 (median 0) | 0, 0, 0, 0, 0, 0, 51, 0, 0, 0 (median 0) |
| Throttle 4 | 115, 185, 132, 163, 171, 110, 109, 117, 170, 159 (median 132) | 92, 103, 99, 137, 121, 136, 116, 139, 140, 123 (median 121) |
| Cap | 400, 289, 295, 304, 288, 195, 389, 197, 401, 294 (median 294) | 195, 105, 105, 107, 101, 103, 97, 105, 288, 113 (median 105) |

When idle, the four tasks over 50 ms used 13 to 20 ms of CPU and waited for the rest. The cap's
runs had the counting wrapper on (section 2); its tasks are almost all waiting, so the wrapper
does not change what they show.

Every task over 50 ms at a throttle of 4, by cause, ten cold and ten warm visits. "CPU" is the
page's thread's own time in the task.

| Cause | Tasks (cold / warm) | Wall median, cold / warm; max (ms) | CPU median, cold / warm; max (ms) | Over 100 ms wall |
|---|---|---|---|---|
| The chain's replica decoded, read, written back and sent (`replica.ts`, `objects.ts`, `idb.ts`, `remote.ts`) | 22 / 18 | 74 / 75; 185 | 64 / 67; 135 | 12 |
| The box's fold decoded through `http.get` (`/api/fold`) | 10 / 7 | 76 / 86; 163 | 68 / 76; 134 | 4 |
| The views module's code cache or compile | 11 / 11 | 80 / 76; 105 | 70 / 74; 104 | 2 |
| A slice frame: the painter's patches, style, layout and paint | 11 / 37 | 61 / 59; 105 | 47 / 54; 102 | 1 |
| The shell's due pass (`app/due.ts`) | 7 / 1 | 72 / 84; 96 | 43 / 62; 62 | 0 |
| Garbage collection | 6 / 3 | 72 / 78; 117 | 54 / 65; 116 | 4 |
| Native work that the sampler does not attribute | 13 / 19 | 88 / 85; 120 | 87 / 75; 120 | 5 |
| Waiting: wall time over three times the CPU time | 1 / 0 | 59; 59 | 18; 18 | 0 |

No task's samples are mostly the views module's step. The views' steps (`painter:step`): the
longest per visit has a median of 7.4 ms cold and 8.0 ms warm when idle, and 27 and 26 ms at a
throttle of 4 (50 ms at most). `views:load` (the module fetched, compiled, instantiated and
rehearsed): median 261 ms cold and 223 ms warm when idle, 408 and 502 ms at a throttle of 4, and
993 and 805 ms under the cap.

Under the cap, 646 cold and 419 warm tasks were over 50 ms, with a median CPU time of 2 to 6 ms.
Eleven cold tasks held the chain's replica (27 to 42 ms of CPU each, 112 to 400 ms of wall time),
and four held the views module's code cache (27 to 42 ms of CPU, 89 to 113 ms of wall time).

### 4.2 The stream as the first page, 10x

Longest task per visit, wall time in ms, ten visits each:

| Condition | Cold | Warm |
|---|---|---|
| Idle | 242, 226, 200, 238, 208, 245, 213, 264, 212, 296 (median 226) | 335, 309, 238, 424, 192, 312, 402, 190, 219, 221 (median 238) |
| Throttle 4 | 939, 714, 920, 878, 780, 1087, 658, 912, 710, 772 (median 780) | 680, 1176, 952, 840, 770, 908, 1050, 839, 867, 699 (median 840) |

Every task over 50 ms, by cause:

| Cause | Idle: tasks (cold / warm) | Idle: CPU median, cold / warm; max (ms) | Throttle 4: tasks (cold / warm) | Throttle 4: CPU median, cold / warm; max (ms) |
|---|---|---|---|---|
| The chain's replica decoded, read, written back and sent | 120 / 116 | 68 / 62; 252 | 197 / 227 | 239 / 170; 1,018 |
| The box's fold decoded through `http.get` | 20 / 10 | 93 / 83; 137 | 76 / 11 | 53 / 356; 707 |
| The stores' head frames (`storesync.ts`) | 8 / 5 | 38 / 37; 55 | 9 / 10 | 203 / 130; 308 |
| The shell's due pass | 9 / 1 | 42 / 44; 50 | 11 / 7 | 200 / 144; 309 |
| The views module's code cache or compile | 1 / 4 | 23 / 32; 43 | 11 / 12 | 78 / 73; 131 |
| A slice frame | 0 / 0 | none | 59 / 89 | 53 / 55; 129 |
| Garbage collection | 11 / 6 | 48 / 30; 100 | 41 / 7 | 74 / 161; 394 |
| Native work that the sampler does not attribute | 26 / 10 | 75 / 82; 96 | 48 / 31 | 317 / 86; 477 |
| Waiting | 12 / 10 | 23 / 28; 79 | 13 / 9 | 21 / 11; 32 |

The views' steps: the longest per visit has a median of 15 ms cold and 12 ms warm when idle, and
69 and 59 ms at a throttle of 4. One idle cold step reached 116 ms. `views:load`: median 277 ms
cold and 681 ms warm when idle, 443 and 560 ms at a throttle of 4. A cold visit at 10x takes
about three minutes, most of it the worker's pull of the lines.

The native work that the sampler does not attribute sits inside tasks that receive a large
message from the worker (`HandlePostMessage`) or between the replica's tasks. This note does
not divide it further.

### 4.3 What crosses from the worker, and how many times it is parsed

The stream as the first page at 1x, idle, mean per visit:

| Message | Direction | Count | Size | Form | Parsed |
|---|---|---|---|---|---|
| `Stream` answer | to the page | 3 cold, 2 warm | 2.38 MB cold, 1.69 MB warm | bytes of JSON (`text.Text`) | once, by the views module |
| `Hold` | to the worker | 5 | 1.09 MB, 2,040 objects | the chain's objects, structured clone | six times, below |
| `List` answer | to the page | 1 | 505 KB, 7,293 objects | a decoded `FoldPayload`, structured clone | three times, below |
| `Proposals` answer | to the page | 2 | 277 KB | bytes of JSON | once, by the views module |
| `KeepBlocks` | to the worker | 1 (cold) | 174 KB | rendered blocks | once |
| `Heard` | to the worker | 5 | 88 KB | a store's heads | once |
| Everything else | both | about 70 | under 5 KB in all | small sums | once |

A chain object is read six times before the core can use it: (1) IndexedDB's structured clone
reads the row; (2) the page's `d` decoders walk it; (3) `replica.ts` scans each literal with
regular expressions; (4) `remote.hold` copies it to the worker by structured clone; (5) the worker
prints it as JSON for the core (`objectsOf`); (6) the core parses that JSON. A cold visit
replaces (1) with the box's JSON, parsed and decoded on the page's thread.

The fold that `List` answers is read three times after the core prints it: the worker parses
the JSON and decodes it, the worker's structured clone writes it, and the page's structured
clone reads 7,293 objects.

Structured clone copies a `Uint8Array`'s bytes into a new buffer outside the JavaScript heap, so
a reading sent as bytes costs a copy and no garbage collection. Transferring the buffer avoids
the copy too. The page's garbage collection comes from the decoded object trees: the chain's
replica and the `List` answer.

## 5. Option (B): an input's round trip, measured

### 5.1 The inputs the pages take today

The views' runtime takes these inputs (`app/pages/src/runtime.rs`, `Input`): `Context`, `Held`,
`Answered`, `Heard`, `Opened` and `Lost`. They come from the host, and they can cross a thread
in either design. The painter reports DOM events by their listeners
(`app/viewtree/src/node.rs`, `On`, and `web/src/painter/painter.ts`):

| Input | What the painter sends | Can it cross a thread and come back within a frame? | What must stay on the page's thread |
|---|---|---|---|
| `Click` | nothing but the event | yes at 1x: the experiment's round trip adds 6 ms at the median | nothing |
| `Intent` (a click inside a `data-intent` element) | the intent's words | yes, as `Click` | finding the element under the click |
| `Input` (a field's value) | the whole value | the step's patches come back a frame later at 1x, and as late as the step at a throttle | the field's text, caret, selection and IME composition. The model's echo of the value must carry the input's sequence number, and the page must drop an echo older than the field's last input |
| `Keydown` | the key and the field's value | no: the handler decides in the event's dispatch whether the browser's default runs | the decision to call `preventDefault`. Under (B) each handler must declare its keys as data, which `viewtree` does not do today |
| `Submit` | nothing | yes | `preventDefault`, which the painter always calls |
| `Toggle` (a `details` opened) | open or closed | yes | the element's state, which the browser changes first |
| `Files` | the files, read whole | yes, as bytes | the read of the files |

The effects that read or write the DOM (`Effect` in `runtime.rs`) must also run on the page's
thread: `Focus`, `Scroll`, the scroll box's kept place, `Copy`, `Frame`, `Mark`, and a `Secret`
field read at the moment of the act. No page takes drag events or text selection as an input
today.

**The split interface under (B), as types.** This is what (B) would need. This note rejects (B).

```ts
// Page to worker.
type Up =
  | { readonly type: "Event"; readonly seq: number; readonly node: number; readonly on: On; readonly words: string }
  | { readonly type: "Answered"; readonly ask: number; readonly answer: Answer } // Frame, Timer, Copy: effects the page performs

// Worker to page: one step's patches and the effects only the page can perform.
type Down = {
  readonly type: "Stepped"
  readonly seen: number // the newest Event.seq that this step took
  readonly patches: ArrayBuffer // transferred
  readonly keeps: boolean // read the scroll box just before the patches, keep the reader's place after
  readonly effects: readonly PageEffect[] // Focus, Scroll, Copy, Go, Back, Toast, Mark, Tell
}

// The page applies a field's value from `patches` only when `seen` is at least the field's last
// Event.seq, and never while the field is composing (IME). A keydown handler is a set of keys.
type KeyHandler = { readonly keys: ReadonlySet<Key>; readonly node: number }
```

### 5.2 The experiment

A page and a worker outside the tree (not committed) stand for the two designs. The page has a
scroll box of 300 rows, five elements each, and a composer: a text area, a character count, a
send button and a preview line. The model holds the rows and the draft. Its step builds the
view tree, diffs it against the last one and encodes the patches as bytes. Fixed extra work
brings one step to the cost of the real keystroke's two steps. The real figure, from the views
module in a large conversation of the 1x store: one keystroke runs two steps (the event, then
the draft register read back), each 4.2 to 5.7 ms at the median at 1x and 23 ms at a throttle of
4, 46 ms at most. The experiment's step is 7.7 ms at 1x and 31 to 35 ms at a throttle of 4.

- **Same thread**: the input event's handler steps the model and applies the patches.
- **Worker**: the handler posts the input. The worker steps and posts the patches back with the
  buffer transferred. The page applies them.

Chromium does not throttle a worker, so the worker does its step's work as many times as the
throttle's factor. Playwright types 30 characters, 90 ms apart (30 ms apart for the echo test).
"To frame" is the time from the key event's timestamp to the end of the first frame that holds
the patches. The page measures it with a message posted from that frame's animation callback.
Each row is ten runs of 30 keystrokes, 300 in all. One core of the machine was generating the
10x store at the time.

| Condition | Same thread: to frame, median / 90th percentile (ms) | Worker: to frame (ms) |
|---|---|---|
| 1x, idle | 11.8 / 17.4 | 18.2 / 26.8 |
| Throttle 4, idle | 56.0 / 78.1 | 55.4 / 71.0 |
| Throttle 6, idle | 86.1 / 115.9 | 81.5 / 106.1 |
| Throttle 4, the page's thread busy: a task of 120 ms, then a pause of 120 ms, repeated | 60.7 / 197.5 | 313.3 / 371.7 |
| Throttle 4, the worker busy with store tasks of 400 ms, a new one every 300 ms, as during a load | 52.5 / 72.6 | 1,078 / 1,605 |

| Condition | Characters lost in 30 typed (ten runs) |
|---|---|
| Throttle 4, 30 ms between keys, the worker's echo applied as it arrives | 6, 4, 5, 13, 6, 11, 16, 9, 9, 9 |
| The same, with the echo dropped when older than the field's last input | 0 in every run |
| The same thread, the echo applied as it arrives | 0 in every run |

| Kept scroll, throttle 4: 20 rows added above the view ten times while the wheel scrolls | Rows that moved on screen | Largest move |
|---|---|---|
| Same thread | 0 of 100 | 0 px |
| Worker, the place read just before the patches | 0 of 100 | 0 px |
| Worker, the place read when the input was sent | 33 of 100 | 40 px |

**What a user would see.**

- At 1x, (B) shows the count and the send button one frame later. The typed character itself
  appears at once in both designs, because the browser draws it.
- At a throttle of 4 or 6 on an idle page, the two designs are the same.
- When the page's thread is busy, (B) is five times slower. The browser runs the key event
  before other tasks, but the worker's reply is an ordinary task and waits behind them.
- When the store's work is running in the same worker, the derived parts of the composer freeze
  for a second or more. The wait grows with the length of the store's work, and a cold load
  keeps the worker busy for seconds.
- With a naive echo, (B) loses typed characters. The guard fixes this, but the field's state
  now lives on two threads.
- The kept scroll works under (B) only if the page reads the scroll box in the task that
  applies the patches.

## 6. Option (C): one module file, measured

Both modules and a third that links both crates were built with names outside the tree:
`--profile wasm-release`, then `wasm-bindgen` and `wasm-opt -Os` as `flake.nix` does, once
without names for the size and once with `-g` for the function names.

| Module | Shipped bytes | Brotli bytes | Functions |
|---|---|---|---|
| The store's (`suzatary-wasm`) | 5,303,044 | 1,275,137 | 7,870 |
| The views' (`suzatary-pages`) | 3,725,717 | 658,874 | 4,486 |
| Both, as two files | 9,028,761 | 1,934,011 | 12,356 |
| Both crates in one module | 8,229,962 | 1,757,642 | 8,837 |

This local build of the store's module is 240 KB larger than the Nix build (5,063,517). The
likely cause is panic locations that name the local paths, which the Nix build remaps to a
short path. The views' module differs by 6 KB.

What the two share: 693 functions with the same name, 240 to 269 KB of code. The largest shares
by crate are `chrono` 38 KB, `core` 32 KB, `serde_json` 32 KB, `serde` 28 KB, `alloc` 25 KB and
`suzatary_topics` 19 KB. The one module is 0.80 MB smaller than the two. That is more than the
shared functions, because the linker also merges data (1,054 KB of data in two files, 830 KB in
one) and identical generic code under different names.

Compile time at 1x, median of ten: the page compiles the store's module in 20 ms, the views' in
12 ms and the one module in 29 ms (`WebAssembly.compile`; Chromium compiles wasm functions
lazily, at their first call). A worker compiles each one and posts the `WebAssembly.Module` to
the page in 42, 32 and 49 ms. The posted module arrives as a module that the page instantiates
without compiling.

(C) saves 176 KB on a first visit after brotli and about 3 ms of compile. It does not save a
decode. Its costs: every change to either crate rebuilds the one module, which the Nix build
takes minutes for, and the page's instance needs the whole module's code cache. Stage 4 removes
the page's compile and code cache read for the views' module without a merge.

## 7. The people this note was written as

- *An author of a UI runtime that runs its model off the main thread*: text entry, selection,
  IME and scroll stay with the DOM; others solve the echo with a sequence number and scroll on the
  compositor. Section 5 measures both.
- *A browser performance engineer*: structured clone copies; a transferred buffer moves; decoded
  object trees are what the collector pays for; a posted `WebAssembly.Module` shares its code.
  Sections 4.3 and 6.
- *An author of the Elm architecture*: one model and one update. Under (A) the update runs where
  the DOM is, so an instant stays one task.
- *A phone user typing on a slow device*: a keystroke that waits behind the store's work is
  worse than a first draw that takes longer. The last row of section 5.2's first table is the
  reason to reject (B).
