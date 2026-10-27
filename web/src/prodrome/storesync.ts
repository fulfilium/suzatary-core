// The stores' effectful shell: the page's stores (the inbox and the topic stores) kept on the device and pulled from the
// box by heads (`pull`), every store told of a move by the one stream, and every reading asked of the core's worker
// (`questions`). The worker holds the lines: it pulls their skeleton, keeps it one node per row, fetches their content
// and reads them, so this thread does nothing that grows with them; it says when a page reads the lines, passes on the
// stream's words about them, and hears what the worker says. The worker is sent the inbox's objects by what changed.

import type { Window } from "./core"
import { CoreError } from "./core"
import remote from "./remote"
import type questions from "./questions"
import type linesync from "./linesync"
import { askHeld } from "./sync"
import heads from "./heads"
import excluded from "./excluded"
import readings from "./readings"
import locked from "./locked"
import progress from "./progress"
import pull from "./pull"
import { CHAIN, dropRows, heldRow, heldRows, holdRow, keepRow, storeRows } from "./objects"
import idb from "../app/idb"
import type time from "../app/time"
import type grouped from "../chat/grouped"
import type rendered from "../app/rendered"
import pageBlocks from "../app/blocks"
import { appended, losses, reset as emptied, standing } from "./replica"
import type { Replica } from "./replica"
import stores from "./stores"
import once from "../algebra/once"
import lo from "../algebra/lo"
import sources from "../algebra/source"
import stream from "../algebra/stream"
import d from "../wire/decoder"
import payload from "../wire/generated/payload"
import storereads from "../wire/storereads"
import { postStoreObjects } from "../wire/api"
import handover from "./handover"
import type topics from "./topics"
import app from "../app/app"
import type { ConversationsPayload, InboxProposalsPayload, MemoryPayload, SearchPayload, StoredObject } from "../wire/generated/wire"

export namespace storesync {
  const source = sources.create<stores.T, stores.Event>(stores.NONE, stores.step)
  const bars = sources.create<progress.T, progress.Event>(progress.NONE, progress.step)
  // What the worker last said moved of the lines, and how many times it said so: what a page reading them wakes on.
  type Moves = { readonly n: number; readonly moved: linesync.Moved }
  const moves = sources.create<Moves, linesync.Moved>({ n: 0, moved: "All" }, (m, moved) => ({ n: m.n + 1, moved }))
  const both = (a: linesync.Moved, b: linesync.Moved): linesync.Moved => (a === "All" || b === "All" ? "All" : [...new Set([...a, ...b])])

  const kept = pull.kept

  // A page store's row: its replica alone, since its objects lock no words, in the rows' store beside the chains', with
  // its pieces in rows of their own beside it. Without IndexedDB what is held is the only copy, as the chains' is.
  //
  // A build before this one kept the row under the store's name with the keys of its words, always none: that row is
  // moved here once, its replica kept whole (a topic this tab wrote and the box has not taken is in it), and deleted
  // with its pieces, which the box sends again; the inbox's row of a build before the keys, a bare replica, is deleted
  // the same way, since the box alone writes the inbox and a pull takes all of it again.
  const keyedRows = storeRows(payload.StoredObject, locked.NONE, losses)
  const rowPlace = (name: stores.Paged): pull.Place<Replica> => {
    const at = `pages/${name}`
    const piecesOf = `${at}/piece/`
    const pieceAt = (n: number): string => `${piecesOf}${String(n).padStart(12, "0")}`
    const moved = async (): Promise<lo.Settled<null>> => {
      const old = await heldRow(keyedRows, kept(name))
      if (old.type === "Held") {
        const into = await holdRow(CHAIN, at, { type: "Join", replica: old.v.replica })
        if (into.type === "Error") return lo.error(into.e)
      }
      // The row and its pieces, which a pull cut before its first landing kept with no row.
      return dropRows(kept(name))
    }
    const resetRow = async (): Promise<lo.Settled<Replica>> => {
      const row = await holdRow(CHAIN, at, { type: "Reset", shown: new Set(), floor: 0, over: "Unread" })
      if (row.type === "Error") return row
      const gone = await dropRows(piecesOf)
      return gone.type === "Error" ? lo.error(`the pieces of a pull were not deleted: ${gone.e}`) : row
    }
    return {
      restore: async () => {
        const move = await moved()
        const read = await heldRow(CHAIN, at)
        const left = await heldRows(CHAIN, piecesOf)
        const row: pull.Restored<Replica> = read.type === "Held" ? { type: "Held", row: read.v } : read.type === "Unreadable" ? { type: "Unreadable", why: read.why } : { type: "Absent" }
        const unread = move.type === "Error" ? [...left.unread, `the row a build before this one kept was not moved (${move.e})`] : left.unread
        return { row, pieces: left.rows, unread }
      },
      keepPiece: (n, piece) => (idb.available() ? holdRow(CHAIN, pieceAt(n), { type: "Join", replica: piece }).then(r => lo.map(r, () => null)) : Promise.resolve(lo.val(null))),
      land: now => (idb.available() ? keepRow(CHAIN, at, { type: "Join", replica: now }) : Promise.resolve({ row: lo.val(now), unread: null })),
      dropPieces: () => (idb.available() ? dropRows(piecesOf) : Promise.resolve(lo.val(null))),
      reset: held => (idb.available() ? resetRow() : Promise.resolve(lo.val(emptied(held)))),
    }
  }

  // A store pulled whole, its objects with their literals, which locks no words: the box sends no keys and erases
  // nothing of it, and none is taken in.
  const wholeStore = (name: stores.Paged): pull.Store<StoredObject, Replica> => ({
    name,
    ...pull.plain<StoredObject>(replica => source.dispatch({ type: "Held", name, replica })),
    held: () => source.state()[name],
    place: rowPlace(name),
    ask: since =>
      storereads.objects(name, since).then(a => ({ said: a, erased: [], piece: r => r, rest: a.rest, size: a.size, weight: a.weight })),
    landed: () => Promise.resolve(),
    restoring: () => Promise.resolve(),
  })

  const effects = (replaced: () => void): pull.Effects => ({
    told: readings.told,
    bars: bars.dispatch,
    replaced,
    again: () => heard(),
  })

  const syncs: { readonly [N in stores.Paged]: pull.Sync<Replica> } = {
    inbox: pull.of(
      wholeStore("inbox"),
      effects(() => {
        ready.forget()
        inboxReady.forget()
      }),
    ),
    topics: pull.of(wholeStore("topics"), effects(() => ready.forget())),
    "topic-aliases": pull.of(wholeStore("topic-aliases"), effects(() => ready.forget())),
  }

  const restore = (): Promise<void> => Promise.all(stores.PAGED.map(name => syncs[name].restored.get())).then(() => undefined)

  // Whether this page is shown, said to the worker once a page reads the lines and at every change after: its prefetch
  // sends nothing while the page is hidden.
  const shown = (): boolean => typeof document === "undefined" || document.visibilityState === "visible"
  const showing = once.of(async (): Promise<void> => {
    const say = (): void => void remote.ask({ type: "LinesShown", on: shown() })
    if (typeof document !== "undefined") document.addEventListener("visibilitychange", say)
    say()
  })

  // A page store pulled, or the lines, which the worker pulls once a page reads them.
  const pulled = (name: stores.Name): Promise<boolean> =>
    name === "lines" ? (void showing.get(), remote.ask({ type: "LinesWant" })) : syncs[name].pull()

  // Which stores a page has asked for so far. A stream's move or reconnect pulls those and nothing else, so before a page
  // asks it pulls nothing: the boot asks for none, and a visit that never reads a conversation never pulls the lines.
  type Wanted = "None" | "Inbox" | "All"
  let wanted: Wanted = "None"
  const heard = (): void => {
    switch (wanted) {
      case "None":
        return
      case "Inbox":
        return void inboxOnly()
      case "All":
        return void all()
    }
  }

  // The lines, then the inbox: the box files a proposal before the line that names it, so an inbox pulled after the
  // lines holds every proposal they name. The topic stores name rounds, never a proposal, so they are pulled beside them,
  // each after what this tab wrote in it is handed over; a refusal is said, and a push with no answer waits.
  const all = (): Promise<boolean> => {
    wanted = "All"
    const topicStores = Promise.all(
      TOPIC_STORES.map(name =>
        syncs[name].restored
          .get()
          .then(() => push(name))
          .then(pushed => (pushed.type === "Refused" ? handover.told(pushed.why) : undefined), () => undefined)
          .then(() => pulled(name)),
      ),
    )
    return pulled("lines").then(lines => pulled("inbox").then(inbox => topicStores.then(ts => lines && inbox && ts.every(t => t))))
  }

  // The inbox alone, for a reading of the proposals; a move of the stores pulls the rest too once a page asked for all.
  const inboxOnly = (): Promise<boolean> => {
    if (wanted === "None") wanted = "Inbox"
    return pulled("inbox")
  }

  export const sync = (): Promise<void> => all().then(() => undefined)

  // A sync that must have reached the box, or says it did not.
  export const asked = (): Promise<void> => all().then(ok => (ok ? undefined : Promise.reject(new Error("the box did not answer"))))

  // Local-first, store by store: a store this device has landed answers at once and is pulled behind; one it never
  // landed is waited for until it is whole, so a page never reads part of it and never reads nothing for empty. The
  // worker waits for the lines the same way before it reads them.
  const readyOf = (names: readonly stores.Paged[], pulling: () => Promise<boolean>): once.T<void> =>
    once.of(() =>
      Promise.all(names.map(name => syncs[name].restored.get())).then(() => {
        void pulling()
        return Promise.all(names.filter(name => !syncs[name].landed()).map(name => syncs[name].whole())).then(() => undefined)
      }),
    )
  const ready = readyOf(stores.PAGED, all)
  const inboxReady = readyOf(["inbox"], inboxOnly)

  // What the worker was last told of a page store's objects, right before a reading of them, so each holding is the
  // change from it: nothing once the worker refused what was sent, and the next is sent whole. Each is sent before it
  // returns, with no wait between reading what was sent and writing it, so two in a row are each the change from the
  // one before.
  const holder = (objects: (r: Replica) => readonly StoredObject[], ask: (delta: questions.Delta) => Promise<null>) => {
    let sent: { readonly replica: Replica; readonly names: ReadonlySet<string> } | null = { replica: locked.NONE.replica, names: new Set() }
    return {
      hold: (replica: Replica): Promise<null> => {
        const was = sent
        if (replica === was?.replica) return Promise.resolve(null)
        const now = objects(replica)
        const names = new Set(now.map(o => o.hash))
        const delta =
          was === null
            ? { add: now, drop: [], whole: true }
            : { add: now.filter(o => !was.names.has(o.hash)), drop: [...was.names].filter(n => !names.has(n)), whole: false }
        sent = { replica, names }
        return ask(delta).catch((e: unknown) => {
          sent = null
          throw e
        })
      },
      // Names the worker wrote itself, so a later holding that lacks them lets them go there too.
      took: (names: readonly string[]): void => {
        if (sent !== null) sent = { replica: sent.replica, names: new Set([...sent.names, ...names]) }
      },
    }
  }
  const inboxHeld = holder(r => r.objects, delta => remote.ask({ type: "HoldInbox", delta }))
  // A reading of a topic store reads no object the box refused, nor what rests on one.
  const topicsHeld: { readonly [N in topics.Store]: ReturnType<typeof holder> } = {
    topics: holder(standing, delta => remote.ask({ type: "HoldTopics", store: "topics", delta })),
    "topic-aliases": holder(standing, delta => remote.ask({ type: "HoldTopics", store: "topic-aliases", delta })),
  }
  const TOPIC_STORES: readonly topics.Store[] = ["topics", "topic-aliases"]
  const holdTopics = (t: stores.T): Promise<unknown> =>
    Promise.all([topicsHeld.topics.hold(t.topics), topicsHeld["topic-aliases"].hold(t["topic-aliases"])])

  // The Inbox's rows at `at`, priced through every chain the tab holds, read from the inbox alone, and the chains they
  // were priced through: what a page that reads no conversation asks, so it pulls the inbox and never the lines.
  export type Proposals = { readonly payload: InboxProposalsPayload; readonly held: readonly string[] }
  export const proposals = async (at: string, memory: MemoryPayload | null): Promise<Proposals> => {
    await inboxReady.get()
    const holding = inboxHeld.hold(source.state().inbox)
    const [, { held, answer }] = await Promise.all([holding, askHeld(() => ({ type: "Proposals", at, memory }))])
    return { payload: answer, held: held.held.map(c => c.name) }
  }

  // `question` about the lines, asked of the worker once the page's stores are ready and the inbox and the topics it
  // reads are held: the worker answers once every line the question reads is held.
  const over = async <K extends questions.Kind>(question: questions.Of<K>, whole: boolean): Promise<questions.Answer<K>> => {
    await ready.get()
    const holding = Promise.all([inboxHeld.hold(source.state().inbox), holdTopics(source.state())])
    const [, answer] = await Promise.all([holding, remote.ask(question)])
    // Asked behind the answer, so it says what the reading left out and the answer waits for none of it.
    if (whole) void remote.ask({ type: "Left" }).then(left => readings.told("stores", excluded.left(left)))
    return answer
  }

  // A reading of every line, said in words where the device lacks lines it reads: how many, and why.
  const held = <A>(read: questions.Whole<A>): A => {
    if (read.type === "NotHere") throw new Error(notHere(read.lines, "the conversations", read.why))
    return read.v
  }

  // The whole of both stores: the list and a search read every conversation, and say what they left out.
  export const list = (): Promise<ConversationsPayload> => over({ type: "Conversations" }, true).then(held)
  export const search = (q: string): Promise<SearchPayload> => over({ type: "Search", q }, true).then(held)

  // The stream a filter shows and the topics in use, read in the worker from one holding of the stores.
  export type Streamed = questions.Answer<"Stream"> extends questions.Whole<infer A> ? A : never
  export const streamed = (chosen: readonly string[], pages: number): Promise<Streamed> => over({ type: "Stream", chosen, pages }, true).then(held)

  // How a topic store's handing over ended: sent, kept for the next sync, or refused in words the owner is told.
  type Pushed = { readonly type: "Sent" } | { readonly type: "Kept" } | { readonly type: "Refused"; readonly why: string }
  const SENT: Pushed = { type: "Sent" }
  const KEPT: Pushed = { type: "Kept" }

  // What this tab wrote in a topic store and the box has not taken, handed over. A refused object stays held as refused,
  // so it leaves the reading and is never posted again; a post with no answer leaves everything for the next sync.
  const push = async (name: topics.Store): Promise<Pushed> => {
    const sync = syncs[name]
    const ended = await handover.of(() => source.state()[name], true, (literals, since) => postStoreObjects(name, literals, since))
    switch (ended.type) {
      case "Nothing":
        return SENT
      case "Unanswered":
        return KEPT
      case "Outdated":
        return { type: "Refused", why: "the box runs a newer build: reload, and this topic goes with the next sync" }
      case "Refused":
        await sync.keep(ended.replica)
        return { type: "Refused", why: `the box refused this topic: ${ended.reason}` }
      case "Taken":
        await sync.keep(ended.replica)
        return ended.refused.length === 0 ? SENT : { type: "Refused", why: `the box refused this topic: ${ended.refused.map(r => r.reason).join("; ")}` }
    }
  }

  // `w` planned in the worker from the topics as held, written into the replica of its store as the actor this tab
  // writes as, kept on the device and handed to the box. Offline, it stays kept and goes with the next sync; a refusal
  // is thrown in the box's words.
  export const write = async (w: topics.Write, at: string): Promise<void> => {
    await ready.get()
    const actor = app.actor(app.now())
    if (actor === null) throw new Error("this device only reads, so it writes no topic")
    const [, planned] = await Promise.all([holdTopics(source.state()), remote.ask({ type: "PlanTopic", write: w, at, actor })])
    if (planned.objects.length === 0) return
    topicsHeld[planned.store].took(planned.objects.map(o => o.hash))
    await syncs[planned.store].keep(appended(source.state()[planned.store], planned.objects))
    const pushed = await push(planned.store)
    if (pushed.type === "Refused") throw new Error(pushed.why)
  }

  // One conversation over its own lines alone, which reads as the whole store reads it; absent where the lines hold no
  // line of it.
  export type One = questions.One

  export const conversation = (id: string, window: Window, memory: MemoryPayload | null): Promise<One> =>
    over({ type: "Conversation", id, window, memory }, false)

  // How a page says lines of `of` are not on this device.
  export const notHere = (lines: number, of: string, why: string): string => `Not on this device yet: ${lines} ${lines === 1 ? "line" : "lines"} of ${of} (${why}).`

  // The same, read back to the turn it must reach, grouped into rounds on the owner's days in `zone`.
  export const window = (id: string, window: grouped.Asked, memory: MemoryPayload | null, zone: time.Zone): Promise<questions.Grouped> =>
    over({ type: "Window", id, window, memory, zone }, false)

  // A conversation read where the store holds none of it, in the words the core refuses one with.
  export const absent = (id: string): never => {
    throw new CoreError(`no conversation named ${id}`)
  }

  // The store a reading's row names, or none for a chain's.
  export const named = (key: string): stores.Name | null => stores.NAMES.find(name => kept(name) === key) ?? null

  // A store's row that does not read, written over by a reset and taken again from the box; a reset that fails is said.
  export const reset = async (name: stores.Name, said: (why: string) => void): Promise<void> => {
    if (name === "lines") return void (await remote.ask({ type: "LinesReset" })).forEach(said)
    await syncs[name].reset(said)
    await restore()
    await sync()
  }

  // What the worker says of the lines: a reading's leavings, a pull's progress, the rendered blocks it forgot, and what
  // moved, heard once a frame however many landings it says it of, so a page asks for its reading again once; a hidden
  // tab, whose frames do not run, hears it within a tenth of a second all the same.
  let moving: linesync.Moved | null = null
  const MOVES_MS = 100
  const flush = (): void => {
    if (moving === null) return
    const moved = moving
    moving = null
    moves.dispatch(moved)
  }
  export const heardWorker = (s: linesync.Said, blocks: rendered.Store = pageBlocks.store): void => {
    switch (s.type) {
      case "Told":
        return readings.told(s.reader, s.left)
      case "Bars":
        return bars.dispatch(s.e)
      case "Forgot":
        return void blocks.forget(s.blocks)
      case "Cleared":
        return void blocks.clear()
      case "Moved": {
        const was = moving
        moving = was === null ? s.conversations : both(was, s.conversations)
        if (was !== null) return
        requestAnimationFrame(flush)
        return void setTimeout(flush, MOVES_MS)
      }
      default: {
        const unheard: never = s
        return unheard
      }
    }
  }

  // Who hears what the worker says of the box and of itself: a revoked key, an older build, a core that did not load.
  export type Heard = { readonly gone: () => void; readonly stale: () => void; readonly unloaded: (why: string) => void }

  // Everything the worker says unasked, each to what hears it: the box's answer to a revoked key and to an older build,
  // a core that did not load, and what it says of the lines.
  const heardFromWorker = (s: questions.Said, fx: Heard, blocks: rendered.Store = pageBlocks.store): void => {
    switch (s.type) {
      case "Gone":
        return fx.gone()
      case "Stale":
        return fx.stale()
      case "Unloaded":
        return fx.unloaded(s.why)
      default:
        return heardWorker(s, blocks)
    }
  }

  // The page hears the worker: what it says unasked, routed as `heardFromWorker` routes it, the page's own table of
  // rendered blocks `blocks`.
  export const hearWorker = (fx: Heard, blocks: rendered.Store = pageBlocks.store): void =>
    remote.hear(said => heardFromWorker(said, fx, blocks))

  // Wakes on every change of what is held, the lines' in the worker included; returns its own removal. A pull changes it
  // once, when it lands.
  export const watch = (listener: () => void): (() => void) => {
    const stop = [source.subscribe(() => listener()), moves.subscribe(() => listener())]
    return () => stop.forEach(f => f())
  }

  // Wakes on every change of what is held but the lines', and on a move of the lines that touched conversation `id`.
  export const watchConversation = (id: string, listener: () => void): (() => void) => {
    const stop = [
      source.subscribe(() => listener()),
      moves.subscribe(() => {
        const moved = moves.state().moved
        if (moved === "All" || moved.includes(id)) listener()
      }),
    ]
    return () => stop.forEach(f => f())
  }

  // Wakes on every move of a store's sync, with how far each has come.
  export const watchProgress = (listener: (p: progress.T) => void): (() => void) => bars.subscribe(() => listener(bars.state()))

  const frame = d.json(payload.StoreFrame)

  // One stream for every store, opened once by the shell, so a store the box adds costs no connection; once a page has
  // read the stores, a reconnect pulls, and so does a frame naming an object the tab does not hold of its store, landed
  // or not, so a pull that was cut is taken up again whatever the tab last heard of the box's heads. `moved` hears each
  // store's heads from the box, the first frame of each included, whether or not a page has read them.
  export const listen = (moved: (name: stores.Name) => void): (() => void) =>
    stream.open(stream.of("/api/stores/events"), {
      opened: () => heard(),
      frame: data => {
        const f = frame(data)
        if (f.type !== "Val") return
        moved(f.v.store)
        if (f.v.store === "lines") return void remote.ask({ type: "LinesHeard", heads: heads.of(f.v.heads) })
        if (stores.behind(heads.of(f.v.heads), syncs[f.v.store].holding())) heard()
      },
    })
}
export default storesync
