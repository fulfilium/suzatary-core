// The page's side of the stores, which the core's worker holds (`storeset`): every reading of them asked of the worker,
// what the box's one stream says of each store's heads passed on, and what the worker says heard. The page holds no
// store and does nothing that grows with one.

import type { Window } from "./core"
import { CoreError } from "./core"
import remote from "./remote"
import type questions from "./questions"
import type linesync from "./linesync"
import lanes from "./lanes"
import type storeset from "./storeset"
import { askHeld } from "./sync"
import heads from "./heads"
import excluded from "./excluded"
import handover from "./handover"
import readings from "./readings"
import progress from "./progress"
import replicasync from "./replicasync"
import stores from "./stores"
import type topics from "./topics"
import type time from "../app/time"
import type grouped from "../chat/grouped"
import app from "../app/app"
import inflight from "../app/inflight"
import once from "../algebra/once"
import sources from "../algebra/source"
import stream from "../algebra/stream"
import d from "../wire/decoder"
import payload from "../wire/generated/payload"
import type { ConversationsPayload, InboxProposalsPayload, MemoryPayload, SearchPayload } from "../wire/generated/wire"

export namespace storesync {
  const bars = sources.create<progress.T, progress.Event>(progress.NONE, progress.step)
  // What the worker last said moved, and how many times it said so: what a page reading the stores wakes on. A move of
  // the lines names the conversations it touched; the worker says a move of any other store touches every one.
  type Moves = { readonly n: number; readonly moved: linesync.Moved }
  const moves = sources.create<Moves, linesync.Moved>({ n: 0, moved: "All" }, (m, moved) => ({ n: m.n + 1, moved }))
  const both = (a: linesync.Moved, b: linesync.Moved): linesync.Moved => (a === "All" || b === "All" ? "All" : [...new Set([...a, ...b])])

  // Whether this page is shown, told the worker at each change: a hidden page's prefetch sends nothing. What the tab
  // tells the worker goes in the first lane, so it is taken when said: an opened stream taken late pulls every store.
  const shown = (): boolean => typeof document === "undefined" || document.visibilityState === "visible"
  const showing = once.of(async (): Promise<void> => {
    const say = (): void => void remote.ask({ type: "Shown", on: shown() }, lanes.FIRST)
    if (typeof document !== "undefined") document.addEventListener("visibilitychange", say)
    say()
  })

  // Every store pulled; whether every pull ended whole.
  const all = (): Promise<boolean> => (void showing.get(), remote.ask({ type: "Pull" }, "Page"))

  export const sync = (): Promise<void> => all().then(() => undefined)

  // A sync that must have reached the box, or says it did not.
  export const asked = (): Promise<void> => all().then(ok => (ok ? undefined : Promise.reject(new Error("the box did not answer"))))

  // The Inbox's rows at `at` and the chains they were priced through, read from the inbox alone: it never pulls the lines.
  export type Proposals = { readonly payload: InboxProposalsPayload; readonly held: readonly string[] }
  export const proposals = async (at: string, memory: MemoryPayload | null, lane: lanes.Lane): Promise<Proposals> => {
    const { held, answer } = await askHeld(() => ({ type: "Proposals", at, memory }), lane)
    return { payload: answer, held: held.held.map(c => c.name) }
  }

  // Asked of the worker, which answers once every store it reads is ready and every line it reads is held.
  const over = async <K extends questions.Kind>(question: questions.Of<K>, whole: boolean, lane: lanes.Lane): Promise<questions.Answer<K>> => {
    void showing.get()
    const answer = await remote.ask(question, lane)
    // Asked behind the answer, so it says what the reading left out and the answer waits for none of it.
    if (whole) void remote.ask({ type: "Left" }, "Shell").then(left => readings.told("stores", excluded.left(left)))
    return answer
  }

  // A reading of every line, said in words where the device lacks lines it reads: how many, and why.
  const held = <A>(read: questions.Whole<A>): A => {
    if (read.type === "NotHere") throw new Error(notHere(read.lines, "the conversations", read.why))
    return read.v
  }

  // The whole of both stores: the list and a search read every conversation, and say what they left out.
  export const list = (lane: lanes.Lane = lanes.FIRST): Promise<ConversationsPayload> => over({ type: "Conversations" }, true, lane).then(held)
  export const search = (q: string, lane: lanes.Lane): Promise<SearchPayload> => over({ type: "Search", q }, true, lane).then(held)

  // The stream a filter shows and the topics in use, read in the worker from one holding of the stores.
  export type Streamed = questions.Answer<"Stream"> extends questions.Whole<infer A> ? A : never
  export const streamed = (chosen: readonly string[], pages: number, lane: lanes.Lane): Promise<Streamed> => over({ type: "Stream", chosen, pages }, true, lane).then(held)

  // `w` written by the worker into its store as the actor this tab writes as, kept on the device and handed to the box.
  // Offline, it stays kept and goes with the next sync; a refusal is thrown in the box's words.
  export const write = async (w: topics.Write, at: string): Promise<void> => {
    const actor = app.actor(app.now())
    if (actor === null) throw new Error("this device only reads, so it writes no topic")
    // Held as the page's own writes are, so a reload waits for the worker to keep it and the box to answer.
    await inflight.held(remote.ask({ type: "Topic", write: w, at, actor }, "Page"))
  }

  // One conversation over its own lines, read as the whole store reads it; absent where they hold none of it.
  export type One = questions.One

  export const conversation = (id: string, window: Window, memory: MemoryPayload | null, lane: lanes.Lane = lanes.FIRST): Promise<One> =>
    over({ type: "Conversation", id, window, memory }, false, lane)

  // How a page says lines of `of` are not on this device.
  export const notHere = (lines: number, of: string, why: string): string => `Not on this device yet: ${lines} ${lines === 1 ? "line" : "lines"} of ${of} (${why}).`

  // The same, read back to the turn it must reach, grouped into rounds on the owner's days in `zone`.
  export const window = (id: string, window: grouped.Asked, memory: MemoryPayload | null, zone: time.Zone, lane: lanes.Lane): Promise<questions.Grouped> =>
    over({ type: "Window", id, window, memory, zone }, false, lane)

  // A conversation the device holds no line of, while the box has not said whether it holds one.
  export const unheard = "This conversation is not on this device yet, and the box has not answered for it."

  // A conversation read where the box holds none of it, in the words the core refuses one with.
  export const absent = (id: string): never => {
    throw new CoreError(`no conversation named ${id}`)
  }

  // The store a reading's row names, or none for a chain's.
  export const named = (key: string): stores.Name | null => stores.NAMES.find(name => replicasync.kept(name) === key) ?? null

  // A store's row that does not read, written over by a reset and taken again from the box; a reset that fails is said.
  export const reset = async (name: stores.Name, said: (why: string) => void): Promise<void> =>
    void (await remote.ask({ type: "Reset", store: name }, "Page")).forEach(said)

  // What the worker says of the stores; a move is heard once a frame however many it says, so a page reads again once,
  // and a hidden tab, whose frames do not run, within a tenth of a second.
  let moving: linesync.Moved | null = null
  const MOVES_MS = 100
  const flush = (): void => {
    if (moving === null) return
    const moved = moving
    moving = null
    moves.dispatch(moved)
  }
  export const heardWorker = (s: storeset.Said): void => {
    switch (s.type) {
      case "Told":
        return readings.told(s.reader, s.left)
      case "Bars":
        return bars.dispatch(s.e)
      case "Refused":
        return handover.told(s.why)
      case "Moved": {
        const was = moving
        moving = was === null ? s.conversations : both(was, s.conversations)
        if (was !== null) return
        requestAnimationFrame(flush)
        return void setTimeout(flush, MOVES_MS)
      }
    }
  }

  // Who hears what the worker says of the box and of itself: a revoked key, an older build, a core that did not load.
  export type Heard = { readonly gone: () => void; readonly stale: () => void; readonly unloaded: (why: string) => void }

  // Everything the worker says unasked, each to what hears it.
  const heardFromWorker = (s: questions.Said, fx: Heard): void => {
    switch (s.type) {
      case "Gone":
        return fx.gone()
      case "Stale":
        return fx.stale()
      case "Unloaded":
        return fx.unloaded(s.why)
      default:
        return heardWorker(s)
    }
  }

  export const hearWorker = (fx: Heard): void => remote.hear(said => heardFromWorker(said, fx))

  // Returns its own removal.
  export const watch = (listener: () => void): (() => void) => moves.subscribe(() => listener())

  // Wakes on a move that touched conversation `id`.
  export const watchConversation = (id: string, listener: () => void): (() => void) =>
    moves.subscribe(() => {
      const moved = moves.state().moved
      if (moved === "All" || moved.includes(id)) listener()
    })

  // Wakes on every move of a store's sync, with how far each has come.
  export const watchProgress = (listener: (p: progress.T) => void): (() => void) => bars.subscribe(() => listener(bars.state()))

  const frame = d.json(payload.StoreFrame)

  // One stream for every store, opened once by the shell. The worker pulls a store a reading wants when it lacks one of
  // a frame's heads, so a cut pull is taken up again.
  export const listen = (moved: (name: stores.Name) => void): (() => void) =>
    stream.open(stream.of("/api/stores/events"), {
      opened: () => void remote.ask({ type: "Opened" }, lanes.FIRST),
      frame: data => {
        const f = frame(data)
        if (f.type !== "Val") return
        moved(f.v.store)
        void remote.ask({ type: "Heard", store: f.v.store, heads: heads.of(f.v.heads) }, lanes.FIRST)
      },
    })
}
export default storesync
