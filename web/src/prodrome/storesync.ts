// The stores' effectful shell: kept on the device, pulled from the box by heads, told of a move by each store's
// stream, and read by the core in its worker (`questions`), which holds the inbox's objects and the lines' content by
// what changed and reads them by what moved, so this thread does no reading that grows with the stores. A store's row
// holds its replica and the keys of their words; a key an erasure names is dropped once the blocks its words fed are
// forgotten. The inbox's replica holds its objects. The lines' holds their skeleton, and a reading fetches the content
// it needs by name (`content`), asking the worker only once every line it reads is held (`lines.need`).

import type { Erased, Window } from "./core"
import { CoreError } from "./core"
import remote from "./remote"
import questions from "./questions"
import { askHeld } from "./sync"
import heads from "./heads"
import excluded from "./excluded"
import readings from "./readings"
import locked from "./locked"
import take from "./take"
import lines from "./lines"
import content from "./content"
import progress from "./progress"
import type { Rows } from "./objects"
import { CHAIN, RENDERED_OWED, alterRow, dropRow, dropRows, heldRow, heldRows, holdRow, keepRow, replicaRow } from "./objects"
import idb from "../app/idb"
import rendered from "../app/rendered"
import { canonical, losses, pulled, replicaOf, sinceOf, wentBack } from "./replica"
import type { Held, Obj, Replica, Said } from "./replica"
import stores from "./stores"
import serial from "../algebra/serial"
import once from "../algebra/once"
import lo from "../algebra/lo"
import sources from "../algebra/source"
import stream from "../algebra/stream"
import time from "../app/time"
import d from "../wire/decoder"
import payload from "../wire/generated/payload"
import { storeContent, storeObjects, storeSkeleton } from "../wire/api"
import type { ConversationPayload, ConversationsPayload, InboxProposalsPayload, MemoryPayload, SearchPayload, SkeletonNode, StoredObject } from "../wire/generated/wire"
import pieces from "./pieces"
import http from "../wire/http"

export namespace storesync {
  const NONE_DEAD: ReadonlySet<string> = new Set()

  const source = sources.create<stores.T, stores.Event>(stores.NONE, stores.step)
  const bars = sources.create<progress.T, progress.Event>(progress.NONE, progress.step)

  // A store's row on the device sits beside the chains', under a key no chain can be named.
  const kept = (name: stores.Name): string => `store/${name}`

  // A row as this build keeps it, or, for the inbox alone, as a build before the keys kept it: a bare replica, which is
  // deleted under a reset's rule, since the box alone writes the store and a pull takes all of it again.
  type Shape<O extends Obj> = { readonly type: "Keyed"; readonly row: locked.Row<O> } | { readonly type: "Bare" }

  // One piece the box answered, as each store's pull answers it.
  type Answer<O extends Obj> = {
    readonly said: Said<O>
    readonly keys: locked.Keys
    readonly erased: readonly string[]
    readonly rest: number
    readonly size: number
    readonly weight: number
  }

  // What differs between the stores: what their replicas hold, how a piece is asked for, and what of it is erased.
  type Store<O extends Obj> = {
    readonly name: stores.Name
    readonly held: () => locked.Row<O>
    readonly hold: (row: locked.Row<O>) => void
    readonly none: locked.Row<O>
    readonly rows: Rows<locked.Row<O>>
    readonly shape: (raw: unknown) => lo.Settled<Shape<O>>
    readonly ask: (since: string) => Promise<Answer<O>>
    readonly dead: locked.Dead<O>
    // Every key `r`'s objects name as erased and every key in `told`, with the blocks `r.keys` opens of each.
    readonly erased: (r: locked.Row<O>, told: readonly string[]) => Promise<Erased>
    // What follows a landing that moved the row.
    readonly landed: (row: locked.Row<O>) => Promise<void>
    // What runs before the row is first read.
    readonly restoring: () => Promise<void>
  }

  const keyedRows = <O extends Obj>(objects: d.Decoder<O>, none: locked.Row<O>, unsent: (r: Replica<O>) => readonly StoredObject[]): Rows<locked.Row<O>> => {
    const Kept = d.obj<{ replica: Held<O>; keys: locked.Keys }>({ replica: replicaRow(objects), keys: d.rec(d.str) })
    return {
      parse: raw => lo.map(Kept(raw), k => locked.of(replicaOf(canonical(k.replica)), k.keys)),
      print: r => ({ replica: r.replica.held, keys: r.keys }),
      generation: r => r.replica.generation,
      unsent: r => unsent(r.replica),
      join: locked.join,
      after: g => locked.of(replicaOf<O>({ ...none.replica.held, generation: g + 1 })),
      nothing: none,
    }
  }

  const keyed =
    <O extends Obj>(rows: Rows<locked.Row<O>>) =>
    (raw: unknown): lo.Settled<Shape<O>> =>
      lo.map(rows.parse(raw), (row): Shape<O> => ({ type: "Keyed", row }))

  const objectRows = keyedRows(payload.StoredObject, locked.NONE, losses)

  // A store pulled whole, its objects with their literals, which locks no words.
  type Whole = "inbox" | "topics" | "topic-aliases"
  const wholeStore = (name: Whole, hold: (row: locked.Row) => void): Store<StoredObject> => ({
    name,
    held: () => source.state()[name],
    hold,
    none: locked.NONE,
    rows: objectRows,
    shape: keyed(objectRows),
    ask: since => storeObjects(name, since).then(a => ({ said: a, keys: a.keys, erased: a.erased, rest: a.rest, size: a.size, weight: a.weight })),
    dead: () => Promise.resolve(NONE_DEAD),
    erased: () => Promise.resolve({}),
    landed: () => Promise.resolve(),
    restoring: () => Promise.resolve(),
  })

  const INBOX: Store<StoredObject> = {
    ...wholeStore("inbox", row => source.dispatch({ type: "Held", name: "inbox", row })),
    shape: raw => {
      const read = keyed(objectRows)(raw)
      return read.type === "Val" || CHAIN.parse(raw).type !== "Val" ? read : lo.val({ type: "Bare" })
    },
  }
  const TOPICS = wholeStore("topics", row => source.dispatch({ type: "Held", name: "topics", row }))
  const ALIASES = wholeStore("topic-aliases", row => source.dispatch({ type: "Held", name: "topic-aliases", row }))

  // The lines' content, fetched by name, each batch's bytes told to the bar.
  const fetching = content.fetcher({
    ask: hashes => storeContent("lines", hashes),
    asked: bytes => bars.dispatch({ type: "Asked", name: "lines", bytes }),
    came: bytes => bars.dispatch({ type: "Came", name: "lines", bytes }),
    lost: bytes => bars.dispatch({ type: "Lost", name: "lines", bytes }),
    atOnce: () => content.atOnce(http.protocol()),
    held: rows => remote.ask({ type: "HoldLines", delta: { add: rows, drop: [], whole: false }, keys: NO_KEYS }).then(() => undefined),
  })
  const NO_KEYS: questions.Keys = { add: {}, drop: [], whole: false }

  const linesNone = locked.none<SkeletonNode>()
  // A skeleton holds no write of this device's: the box alone writes the lines.
  const linesRows = keyedRows(payload.SkeletonNode, linesNone, () => [])
  const LINES: Store<SkeletonNode> = {
    name: "lines",
    held: () => source.state().lines,
    hold: row => source.dispatch({ type: "Held", name: "lines", row }),
    none: linesNone,
    rows: linesRows,
    shape: keyed(linesRows),
    ask: since =>
      storeSkeleton(since).then(a => ({ said: { ...a, objects: a.nodes, untrusted: [], root: null }, keys: a.keys, erased: a.erased, rest: a.rest, size: a.size, weight: a.weight })),
    // The worker reads the largest down-set of the content it holds, and holds content only by the skeleton's names:
    // fetched by them, and what the skeleton no longer names let go at its landing (`content.keepOnly`). A word was only
    // ever rendered from a conversation read whole.
    dead: () => remote.ask({ type: "Dead" }).then(names => new Set(names)),
    // The blocks a key's words fed are found by opening its lines, so their content is fetched first; a fetch that
    // fails rejects, and the keys stay owed.
    erased: async (r, told) => {
      await fetching("First", lines.naming(r.replica, locked.names(r.keys)))
      return remote.ask({ type: "Erased", keys: r.keys, told })
    },
    landed: async row => {
      if (row.replica.objects.length > 0) await retire()
      const gone = await content.keepOnly(
        new Set(row.replica.objects.map(n => n.hash)),
        drop => remote.ask({ type: "HoldLines", delta: { add: [], drop, whole: false }, keys: NO_KEYS }).then(() => undefined),
      )
      if (gone.type === "Error") told("lines", gone.e)
    },
    restoring: () => cleared(),
  }

  // A row that does not read is said, with its own reset, never passed over.
  const told = (name: stores.Name, unread: string | null): void =>
    readings.told(`row/${kept(name)}`, excluded.left(excluded.NONE, [], unread === null ? [] : [excluded.unread(kept(name), unread)]))

  // A purge that could not forget is said, and asked again at the next sync; the keys it owes stay until it can.
  const owing = (name: stores.Name, failed: string | null): void =>
    readings.told(
      `purge/${kept(name)}`,
      excluded.left(excluded.NONE, [], failed === null ? [] : [{ chain: kept(name), why: `the blocks some erased words were rendered to are still on this device (${failed}); it tries again at the next sync`, cure: "None" }]),
    )

  // The plaintext conversations a build before the lines kept: deleted once the lines' row holds what replaces them,
  // under a reset's rule, the blocks they rendered cleared first, since nothing names which of those words went since.
  const PLAIN = "store/conversations"
  const retired = once.of(async (): Promise<void> => {
    const was = await heldRow(CHAIN, PLAIN)
    if (was.type === "Absent") return
    const cleared = await rendered.store.clear()
    if (cleared.type === "Error") throw new Error(cleared.e)
    const gone = await dropRow(CHAIN, PLAIN, new Set())
    if (gone.type === "Error") throw new Error(gone.e)
  })

  // A retirement that failed is said, and tried again at the next pull of the lines.
  const retire = (): Promise<void> =>
    retired.get().then(
      () => readings.told(`row/${PLAIN}`, excluded.NOTHING),
      (e: unknown) =>
        readings.told(
          `row/${PLAIN}`,
          excluded.left(excluded.NONE, [], [{ chain: PLAIN, why: `the conversations a build before the lines kept are still on this device (${lo.reason(e)}); it tries again at the next sync`, cure: "None" }]),
        ),
    )

  // The blocks a build before version 5 rendered, cleared once after the upgrade that dropped the lines they were
  // rendered from, since no purge can find them by their words any more; a failure is said and tried again.
  const owedClear = once.of(async (): Promise<void> => {
    const owed = await heldRow({ parse: raw => (raw === true ? lo.val(true) : lo.error("not the upgrade's mark")) }, RENDERED_OWED)
    if (owed.type === "Absent") return
    const done = await rendered.store.clear()
    if (done.type === "Error") throw new Error(done.e)
    const gone = await dropRow(CHAIN, RENDERED_OWED, new Set())
    if (gone.type === "Error") throw new Error(gone.e)
  })
  const cleared = (): Promise<void> =>
    owedClear.get().then(
      () => readings.told(`row/${RENDERED_OWED}`, excluded.NOTHING),
      (e: unknown) =>
        readings.told(
          `row/${RENDERED_OWED}`,
          excluded.left(excluded.NONE, [], [{ chain: RENDERED_OWED, why: `the blocks a build before this one rendered are still on this device (${lo.reason(e)}); it tries again at the next load`, cure: "None" }]),
        ),
    )

  // A pull that did not end whole is asked again after a wait that doubles from two seconds to a minute, and whatever
  // a stream says meanwhile asks sooner; a whole pull starts the waits over.
  const AGAIN_MS = { first: 2_000, most: 60_000 }

  // How a landing settled: the store landed, or a later epoch replaced the landing before it did (`startOver`).
  type Settled = "Landed" | "Replaced"
  type Landing = { readonly once: Promise<Settled>; readonly done: (how: Settled) => void }
  const awaited = (): Landing => {
    let done = (_how: Settled): void => {}
    const once = new Promise<Settled>(resolve => (done = resolve))
    return { once, done }
  }

  // One store's pull, its pieces kept beside its row, and its landing.
  type Sync = {
    readonly restored: once.T<void>
    readonly pull: () => Promise<boolean>
    readonly holding: () => Replica<Obj>
    readonly landed: () => boolean
    readonly whole: () => Promise<void>
    readonly reset: (said: (why: string) => void) => Promise<void>
  }

  const syncOf = <O extends Obj>(st: Store<O>, replaced: () => void): Sync => {
    const name = st.name
    const taking = (told: readonly string[]): take.Effects<O> => ({
      held: st.held,
      dispatch: e => {
        if (e.type === "Held") return st.hold(e.row)
        source.dispatch({ type: "Forgot", name, keys: e.keys })
        // The worker lets go of a forgotten key's words now, not at the next reading, which may never come.
        if (name === "lines") void holdKeys(source.state().lines.keys).catch(() => undefined)
      },
      purge: { erased: r => st.erased(r, told), forget: rendered.store.forget },
      dead: told.length === 0 ? st.dead : r => st.dead(r).then(dead => new Set([...dead, ...told])),
      alter: gone => (idb.available() ? alterRow(st.rows, kept(name), r => locked.without(r, gone)) : Promise.resolve(lo.val(null))),
      owing: failed => owing(name, failed),
    })

    // The pieces of the pull that have not landed: held beside the row and asked from, never read by a page.
    let pending = st.none
    // What the tab holds of the store, landed and not: what a pull asks from, and what a stream's heads are compared with.
    const holding = (): Replica<O> => locked.join(st.held(), pending).replica
    // The two parts of it, each asked on its own whether a later epoch overtook it.
    const parts = (): readonly Replica<O>[] => [st.held().replica, pending.replica]

    // Each piece kept in a row of its own, named by how many objects were held when it was asked for, so keeping one
    // writes that piece alone and a run of them reads back in the order they came.
    const piecesOf = `${kept(name)}/piece/`
    const pieceAt = (at: number): string => `${piecesOf}${String(at).padStart(12, "0")}`

    // One read of the row per page.
    const restoredOne = async (): Promise<void> => {
      await st.restoring()
      const read = await heldRow({ parse: st.shape }, kept(name))
      if (read.type === "Held" && read.v.type === "Bare") {
        const gone = await dropRow(CHAIN, kept(name), new Set())
        return told(name, gone.type === "Error" ? gone.e : null)
      }
      told(name, read.type === "Unreadable" ? read.why : null)
      if (read.type === "Held" && read.v.type === "Keyed") await take.into(taking([]), read.v.row)
      // The pieces a pull kept and did not land, another tab's included: held beside the row, never read by a page.
      const left = await heldRows(st.rows, piecesOf)
      pending = left.rows.reduce(locked.join, pending)
      if (left.unread.length > 0) told(name, left.unread.join("; "))
    }
    const restored = once.of(restoredOne)

    type Piece = { readonly asked: Replica<O>; readonly answer: Answer<O>; readonly rest: number }

    // One piece kept: the erasures it names forgotten from what pages read, then the piece held beside it, less any key
    // those name, and written on the device by itself. It holds only what was asked of a replica, so another tab's reset
    // kept meanwhile, a later generation, keeps it out when it lands, and the next piece is asked of the reset. It stands
    // where that replica stood, so the frontier it names is read against the one asked from: an answer that did not
    // know it moves the era on from there, and a later piece of the same era is the later frontier.
    const keep = async ({ asked, answer }: Piece): Promise<void> => {
      readings.told(`epoch/${kept(name)}`, excluded.behind(kept(name), wentBack(asked, answer.said)))
      bars.dispatch({ type: "Piece", name, size: answer.size, weight: answer.weight })
      // Only while more pieces are to come: the last lands with the erasure's lines, which then read as erased.
      if (answer.erased.length > 0 && answer.rest > 0) await take.into(taking(answer.erased), st.none)
      const piece = locked.without(locked.of(pulled(replicaOf<O>({ ...st.none.replica.held, generation: asked.generation, frontier: asked.held.frontier }), answer.said, time.now()), answer.keys), answer.erased)
      pending = locked.without(locked.join(pending, piece), answer.erased)
      if (idb.available() && answer.said.objects.length > 0) {
        const written = await holdRow(st.rows, pieceAt(asked.objects.length), { type: "Join", replica: piece })
        if (written.type === "Error") told(name, written.e)
      }
    }

    let landing = awaited()

    // What the pieces brought, landed in one step, so a page reading the store reads it once, whole; then the row written
    // once with what another tab kept joined in, and the pieces' own rows deleted.
    const land = async (last: Piece): Promise<void> => {
      const was = st.held()
      const brought = pending
      await take.into(taking(last.answer.erased), brought)
      pending = st.none
      const now = st.held()
      // Without IndexedDB what is held is the only copy, as the chains' is.
      if (now !== was && idb.available()) {
        const { row, unread } = await keepRow(st.rows, kept(name), { type: "Join", replica: now })
        if (row.type === "Val") await take.into(taking(last.answer.erased), row.v)
        told(name, unread)
      }
      if (brought !== st.none && idb.available()) {
        const gone = await dropRows(piecesOf)
        if (gone.type === "Error") told(name, gone.e)
      }
      landing.done("Landed")
      await st.landed(st.held())
    }

    // One wait at a time: a later failure moves it, never adds another.
    let wait = AGAIN_MS.first
    let timer: ReturnType<typeof setTimeout> | undefined
    const retry = (): void => {
      clearTimeout(timer)
      timer = setTimeout(heard, wait)
      wait = Math.min(wait * 2, AGAIN_MS.most)
    }
    const settled = (): void => {
      clearTimeout(timer)
      wait = AGAIN_MS.first
    }

    // A piece asked of what is held, landed and not.
    const askOf = (): Promise<Piece> => {
      const asked = holding()
      return st.ask(sinceOf(asked)).then((answer): Piece => ({ asked, answer, rest: answer.rest }))
    }

    // What the box's later epoch overtook dropped, on the device first, so no copy from before it, another tab's included,
    // reaches it: the row reset to an empty one of the generation after, keys and all, the kept pieces deleted, and the
    // pull's progress with them, so it starts again from nothing.
    const startOver = async (): Promise<void> => {
      const row = idb.available()
        ? await holdRow(st.rows, kept(name), { type: "Reset", shown: new Set(), floor: 0, over: "Unread" })
        : lo.val(locked.reset(st.held()))
      if (row.type === "Error") throw new Error(row.e)
      if (idb.available()) {
        const gone = await dropRows(piecesOf)
        if (gone.type === "Error") throw new Error(gone.e)
      }
      pending = st.none
      // The store is unlanded again, so a reading waits for its next landing, never reads the emptied row. The landing
      // a reading already waits on is settled as it is replaced, so that reading asks again and waits on this one.
      const was = landing
      landing = awaited()
      was.done("Replaced")
      replaced()
      settled()
      await take.into(taking([]), row.v)
    }

    // By pieces, each asked from what is held, landed and not, and kept before the next. A pull that did not end whole
    // answers false, keeps what came, and is asked again for the rest. A piece of a later epoch than all that is held
    // starts the pull over, and the store is asked for again in pieces.
    const pullNow = (): Promise<boolean> =>
      restored
        .get()
        .then(() =>
          pieces.pull({
            ask: async () => {
              const piece = await askOf()
              if (!stores.overtaken(parts(), piece.answer.said)) return piece
              await startOver()
              return askOf()
            },
            keep,
            land,
            held: () => [st.held(), pending],
          }),
        )
        .catch((e: unknown): pieces.Ended => ({ type: "Cut", taken: 0, e }))
        .then(ended => {
          bars.dispatch({ type: "Pulled", name })
          if (ended.type === "Whole") {
            settled()
            return true
          }
          retry()
          return false
        })

    const landed = (): boolean => st.held().replica.box.type === "Heard"
    // A replaced landing is waited on again through the one that replaced it; a landed one ends the wait, even empty.
    const whole = (): Promise<void> => (landed() ? Promise.resolve() : landing.once.then(how => (how === "Landed" ? undefined : whole())))

    // A row that does not read, written over by a reset and taken again from the box; a reset that fails is said.
    const reset = async (said: (why: string) => void): Promise<void> => {
      const done = await holdRow(st.rows, kept(name), { type: "Reset", shown: new Set(), floor: 0, over: "Unread" })
      if (done.type === "Error") said(`the reset was not kept: ${done.e}`)
      const gone = await dropRows(piecesOf)
      if (gone.type === "Error") said(`the pieces of a pull were not deleted: ${gone.e}`)
      pending = st.none
      restored.forget()
    }

    // One pull at a time, so a pull asked for during another is a pull that starts after it.
    return { restored, pull: serial.of(pullNow), holding, landed, whole, reset }
  }

  // A store that lands empty moves no row, so the landing itself is what is waited on; a later epoch forgets the cells
  // that waited on the landing it replaced, so the store is waited for again until it lands again.
  const syncs: { readonly [N in stores.Name]: Sync } = {
    inbox: syncOf(INBOX, () => {
      ready.forget()
      inboxReady.forget()
    }),
    lines: syncOf(LINES, () => ready.forget()),
    topics: syncOf(TOPICS, () => ready.forget()),
    "topic-aliases": syncOf(ALIASES, () => ready.forget()),
  }

  const restore = (): Promise<void> => Promise.all(stores.NAMES.map(name => syncs[name].restored.get())).then(() => undefined)

  export const pull = (name: stores.Name): Promise<boolean> => syncs[name].pull()

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
  // lines holds every proposal they name. The topic stores name rounds, never a proposal, so they are pulled beside them.
  const all = (): Promise<boolean> => {
    wanted = "All"
    const topicStores = Promise.all([pull("topics"), pull("topic-aliases")])
    return pull("lines").then(lines => pull("inbox").then(inbox => topicStores.then(([t, a]) => lines && inbox && t && a)))
  }

  // The inbox alone, for a reading of the proposals; a move of the stores pulls the rest too once a page asked for all.
  const inboxOnly = (): Promise<boolean> => {
    if (wanted === "None") wanted = "Inbox"
    return pull("inbox")
  }

  export const sync = (): Promise<void> => all().then(() => undefined)

  // A sync that must have reached the box, or says it did not.
  export const asked = (): Promise<void> => all().then(ok => (ok ? undefined : Promise.reject(new Error("the box did not answer"))))

  // Local-first, store by store: a store this device has landed answers at once and is pulled behind; one it never
  // landed is waited for until it is whole, so a page never reads part of it and never reads nothing for empty.
  const readyOf = (names: readonly stores.Name[], pulling: () => Promise<boolean>): once.T<void> =>
    once.of(() =>
      Promise.all(names.map(name => syncs[name].restored.get())).then(() => {
        void pulling()
        return Promise.all(names.filter(name => !syncs[name].landed()).map(name => syncs[name].whole())).then(() => undefined)
      }),
    )
  const ready = readyOf(stores.NAMES, all)
  const inboxReady = readyOf(["inbox"], inboxOnly)

  // What the worker was last told of the inbox and of the keys: the inbox right before a reading of it, the keys then
  // and when a purge lets one go. Each is nothing once the worker refused what was sent, and the next is sent whole.
  let inboxSent: { readonly replica: Replica; readonly names: ReadonlySet<string> } | null = { replica: stores.NONE.inbox.replica, names: new Set() }
  let keysSent: locked.Keys | null = locked.NO_KEYS

  // Each sent before it returns, with no wait between reading what was sent and writing it, so two in a row are each
  // the change from the one before.
  const holdInbox = (replica: Replica): Promise<null> => {
    const was = inboxSent
    if (replica === was?.replica) return Promise.resolve(null)
    const names = new Set(replica.objects.map(o => o.hash))
    const delta =
      was === null
        ? { add: replica.objects, drop: [], whole: true }
        : { add: replica.objects.filter(o => !was.names.has(o.hash)), drop: [...was.names].filter(n => !names.has(n)), whole: false }
    inboxSent = { replica, names }
    return remote.ask({ type: "HoldInbox", delta }).catch((e: unknown) => {
      inboxSent = null
      throw e
    })
  }

  const holdKeys = (keys: locked.Keys): Promise<null> => {
    const was = keysSent
    if (keys === was) return Promise.resolve(null)
    const add = was === null ? keys : Object.fromEntries(Object.entries(keys).filter(([n, k]) => was[n] !== k))
    const drop = was === null ? [] : Object.keys(was).filter(n => keys[n] === undefined)
    keysSent = keys
    return remote.ask({ type: "HoldLines", delta: { add: [], drop: [], whole: false }, keys: { add, drop, whole: was === null } }).catch((e: unknown) => {
      keysSent = null
      throw e
    })
  }

  // The Inbox's rows at `at`, priced through every chain the tab holds, read from the inbox alone, and the chains they
  // were priced through: what a page that reads no conversation asks, so it pulls the inbox and never the lines.
  export type Proposals = { readonly payload: InboxProposalsPayload; readonly held: readonly string[] }
  export const proposals = async (at: string, memory: MemoryPayload | null): Promise<Proposals> => {
    await inboxReady.get()
    const holding = holdInbox(source.state().inbox.replica)
    const [, { held, answer }] = await Promise.all([holding, askHeld(() => ({ type: "Proposals", at, memory }))])
    return { payload: answer, held: held.held.map(c => c.name) }
  }

  // `question` asked of the worker once every line `scope` reads is held, fetching what is not, a conversation's before
  // the whole store's. The holdings go out right before it, so it reads what is held now; messages arrive in order.
  const over = async <K extends questions.Kind>(scope: lines.Scope, question: questions.Of<K>): Promise<questions.Answer<K>> => {
    await ready.get()
    for (;;) {
      const t = source.state()
      const need = lines.need(t.lines.replica, content.has, scope)
      if (need.type === "Fetching") {
        await fetching(scope.type === "Whole" ? "Behind" : "First", need.nodes)
        continue
      }
      const holding = Promise.all([holdInbox(t.inbox.replica), holdKeys(t.lines.keys)])
      const [, answer, left] = await Promise.all([holding, remote.ask(question), scope.type === "Whole" ? remote.ask({ type: "Left" }) : null])
      if (left !== null) readings.told("stores", excluded.left(left))
      return answer
    }
  }

  // The whole of both stores: the list and a search read every conversation.
  export const list = (): Promise<ConversationsPayload> => over(lines.WHOLE, { type: "Conversations" })
  export const search = (q: string): Promise<SearchPayload> => over(lines.WHOLE, { type: "Search", q })

  // One conversation over its own lines alone, which reads as the whole store reads it; absent where the store holds no
  // line of it, read off the skeleton without fetching anything.
  export type One = { readonly type: "Absent" } | { readonly type: "Held"; readonly payload: ConversationPayload }

  export const conversation = async (id: string, window: Window, memory: MemoryPayload | null): Promise<One> => {
    await ready.get()
    return lines.holds(source.state().lines.replica, id)
      ? { type: "Held", payload: await over({ type: "Conversation", id }, { type: "Conversation", id, window, memory }) }
      : { type: "Absent" }
  }

  // A conversation read where the store holds none of it, in the words the core refuses one with.
  export const absent = (id: string): never => {
    throw new CoreError(`no conversation named ${id}`)
  }

  // The store a reading's row names, or none for a chain's.
  export const named = (key: string): stores.Name | null => stores.NAMES.find(name => kept(name) === key) ?? null

  // A store's row that does not read, written over by a reset and taken again from the box; a reset that fails is said.
  export const reset = async (name: stores.Name, said: (why: string) => void): Promise<void> => {
    await syncs[name].reset(said)
    await restore()
    await sync()
  }

  // Wakes on every change of what is held; returns its own removal. A pull changes it once, when it lands.
  export const watch = (listener: () => void): (() => void) => source.subscribe(() => listener())

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
        if (stores.behind(heads.of(f.v.heads), syncs[f.v.store].holding())) heard()
      },
    })
}
export default storesync
