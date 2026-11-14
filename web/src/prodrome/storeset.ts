// Every store the tab holds, in the core's worker: their pulls, the hand-over of what this tab wrote, its refusals,
// and what moved. The page holds none of it.

import type { Core, LineStep, Lines, ObjectStore, Proposals, TopicReading, TopicStore } from "./core"
import { CoreError, signedObject } from "./core"
import type excluded from "./excluded"
import type heads from "./heads"
import linesync from "./linesync"
import type progress from "./progress"
import replicasync from "./replicasync"
import signer from "./signer"
import stores from "./stores"
import topics from "./topics"
import last from "../algebra/last"
import lo from "../algebra/lo"
import once from "../algebra/once"
import type rendered from "../app/rendered"
import d from "../wire/decoder"
import http from "../wire/http"
import storereads from "../wire/storereads"
import type { StorePayload } from "../wire/generated/wire"

export namespace storeset {
  // What the worker says of the stores: leavings, progress, a move, and a write of this tab's the box refused.
  export type Said =
    | { readonly type: "Told"; readonly reader: string; readonly left: excluded.Left }
    | { readonly type: "Bars"; readonly e: progress.Event }
    | { readonly type: "Moved"; readonly conversations: linesync.Moved }
    | { readonly type: "Refused"; readonly why: string }

  export type Scope = "Inbox" | "All"
  const SCOPES: { readonly [S in Scope]: readonly stores.Name[] } = { Inbox: ["inbox"], All: stores.NAMES }
  const SCOPE_NAMES: readonly Scope[] = ["Inbox", "All"]

  // The core's two topic stores and their one reading, made once per move of either.
  export type Topics = { readonly replicas: { readonly [N in topics.Store]: TopicStore }; readonly reading: TopicReading }

  export type T = {
    readonly lines: linesync.T
    readonly proposals: () => Proposals
    readonly topics: () => Topics
    // Local-first: a store this device has landed answers at once; one it never landed is waited for until whole.
    readonly ready: (scope: Scope) => Promise<void>
    // Every store pulled, what this tab wrote handed over first; whether every pull ended whole.
    readonly pull: () => Promise<boolean>
    readonly heard: (store: stores.Name, box: heads.Heads) => void
    readonly opened: () => void
    // Whether a page is shown, as the page says: a hidden page may be closed, so every store's log is compacted.
    readonly shown: (on: boolean) => Promise<void>
    // A store dropped and taken again; what the reset could not do.
    readonly reset: (store: stores.Name) => Promise<readonly string[]>
    // `w` written into its store as `actor` and signed, settled before the box hears of it; a refusal arrives as a move.
    readonly write: (w: topics.Write, at: string, actor: string) => Promise<void>
  }

  const BAD_REQUEST = 400
  // The stores this tab writes, each with the noun a refusal names.
  const WRITTEN: { readonly [N in topics.Store]: string } = { topics: "topic", "topic-aliases": "topic's new name" }
  const WRITTEN_NAMES: readonly topics.Store[] = ["topics", "topic-aliases"]

  // In posts the box takes whole, each change beside its signatures.
  const Unsent = d.json(d.arr(d.arr(d.obj<{ hash: string; literal: string }>({ hash: d.str, literal: d.str }))))
  const unsentIn = (store: ObjectStore): readonly (readonly { readonly hash: string; readonly literal: string }[])[] => {
    const read = Unsent(store.unsent())
    if (read.type === "Val") return read.v
    throw new CoreError(`unsent: unexpected shape (${read.e})`)
  }
  const Names = d.json(d.arr(d.str))
  const unsignedIn = (store: ObjectStore): readonly string[] => {
    const read = Names(store.unsigned())
    if (read.type === "Val") return read.v
    throw new CoreError(`unsigned: unexpected shape (${read.e})`)
  }
  const Refusals = d.json(d.arr(d.obj<{ hash: string; reason: string }>({ hash: d.str, reason: d.str })))
  const refusalsIn = (store: ObjectStore): ReadonlyMap<string, string> => {
    const read = Refusals(store.refusals())
    if (read.type === "Val") return new Map(read.v.map(r => [r.hash, r.reason]))
    throw new CoreError(`refusals: unexpected shape (${read.e})`)
  }

  export const of = (core: Core, held: Lines, say: (s: Said) => void, blocks: rendered.Device): T => {
    const told = (reader: string, left: excluded.Left): void => say({ type: "Told", reader, left })
    const bars = (e: progress.Event): void => say({ type: "Bars", e })
    // A store a later epoch started over is waited for again by every reading of it.
    const replaced = (name: stores.Name) => (): void => {
      for (const s of SCOPE_NAMES) if (SCOPES[s].includes(name)) readies[s].forget()
    }
    // A store of objects; a move touches every conversation, then runs `after`.
    const objectStore = <S extends ObjectStore>(name: Exclude<stores.Name, "lines">, store: S, after: () => void = () => {}): { readonly store: S; readonly sync: replicasync.T } => ({
      store,
      sync: replicasync.of<StorePayload>(
        {
          name,
          held: store,
          ask: since => storereads.objects(name, since),
          piece: (a, at) => store.piece(JSON.stringify(a), at),
          landed: () => Promise.resolve(),
        },
        { told, bars, moved: said => (said === "[]" ? undefined : (say({ type: "Moved", conversations: "All" }), after())), replaced: replaced(name) },
      ),
    })

    const lines = linesync.of(
      held,
      {
        told,
        bars,
        moved: conversations => say({ type: "Moved", conversations }),
        replaced: replaced("lines"),
      },
      blocks,
    )
    const inbox = objectStore("inbox", new core.Proposals())
    const topicStores = { topics: objectStore("topics", new core.Memberships()), "topic-aliases": objectStore("topic-aliases", new core.TopicAliases()) }
    // Read again whenever it moves: each store this tab writes holds what it records as refused.
    const asks = objectStore("asks", new core.Asks(), () => void refusedInAsks())
    const engines: { readonly [N in stores.Name]: replicasync.Store } = { lines: lines.engine, inbox: inbox.sync, topics: topicStores.topics.sync, "topic-aliases": topicStores["topic-aliases"].sync, asks: asks.sync }

    const reading = last.of<string, TopicReading>(
      (a, b) => a === b,
      r => r.free(),
    )
    const topicsNow = (): Topics => {
      const replicas = { topics: topicStores.topics.store, "topic-aliases": topicStores["topic-aliases"].store }
      return { replicas, reading: reading(`${replicas.topics.version()}/${replicas["topic-aliases"].version()}`, () => new core.TopicReading(replicas.topics, replicas["topic-aliases"])) }
    }

    // `step`, a move of a store this tab writes, kept; each newly refused object said once, in the box's words.
    const settled = async (name: topics.Store, step: () => LineStep | undefined): Promise<void> => {
      const { store, sync } = topicStores[name]
      const before = refusalsIn(store)
      const made = step()
      if (made === undefined) return
      await sync.stepped(made)
      for (const [hash, reason] of refusalsIn(store)) {
        if (!before.has(hash)) say({ type: "Refused", why: `The box refused a ${WRITTEN[name]}: ${reason}` })
      }
    }

    const refusedInAsks = async (): Promise<void> => {
      for (const name of WRITTEN_NAMES) await settled(name, () => topicStores[name].store.refused_in(asks.store))
    }

    // Each unsigned change this tab wrote, signed by the key only the page mints, so a write made keyless is signed later.
    const signOwn = async (name: topics.Store): Promise<void> => {
      const { store, sync } = topicStores[name]
      const unsigned = unsignedIn(store)
      if (unsigned.length === 0) return
      const key = await signer.existing()
      if (key === null) return
      for (const hash of unsigned) await sync.stepped(store.signed(signedObject(core, hash, key.public, await signer.sign(key, hash)).literal))
    }

    // A post with no answer leaves everything for the next sync. A 400 refuses the post's shape, not its objects: they
    // stay unsent, and the box's words are said once per tab.
    const shapes = new Set<string>()
    const handOver = async (name: topics.Store): Promise<void> => {
      await signOwn(name)
      const { store } = topicStores[name]
      for (const posting of unsentIn(store)) {
        try {
          const receipt = await storereads.post(
            name,
            posting.map(o => o.literal),
            store.since(),
          )
          await settled(name, () => store.answered(JSON.stringify(receipt)))
        } catch (e: unknown) {
          if (e instanceof http.ApiError && e.status === http.OUTDATED) return say({ type: "Refused", why: `The box runs a newer build: reload, and this ${WRITTEN[name]} goes with the next sync.` })
          if (http.unanswered(e)) return
          const reason = lo.reason(e)
          if (e instanceof http.ApiError && e.status === BAD_REQUEST) {
            if (!shapes.has(reason)) say({ type: "Refused", why: `The box did not take a post of this tab's writes: ${reason}` })
            shapes.add(reason)
            return
          }
          await settled(name, () => store.refuse(JSON.stringify(posting.map(o => o.hash)), reason))
        }
      }
    }

    // Which stores a reading has asked for so far: a reconnect pulls those alone.
    let wanted: "None" | Scope = "None"

    // The lines, then the inbox, which then holds every proposal they name; the topic stores beside them after their
    // hand-over, and the asks after those posts.
    const all = (): Promise<boolean> => {
      wanted = "All"
      const topicPulls = Promise.all(
        WRITTEN_NAMES.map(name =>
          topicStores[name].sync.sync.restored
            .get()
            .then(() => handOver(name))
            .catch(() => undefined)
            .then(() => topicStores[name].sync.want()),
        ),
      )
      const asked = topicPulls.then(ts => asks.sync.want().then(a => ts.every(t => t) && a))
      return lines.engine.want().then(l => inbox.sync.want().then(i => asked.then(t => l && i && t)))
    }
    // The inbox alone; a move pulls the rest too once a reading asked for all.
    const inboxOnly = (): Promise<boolean> => {
      if (wanted === "None") wanted = "Inbox"
      return inbox.sync.want()
    }

    // A store is ready once it is restored, and, where it never landed, once a pull of it ended whole.
    const readyOf = (names: readonly stores.Name[], pulling: () => Promise<boolean>): once.T<void> =>
      once.of(() =>
        Promise.all(
          names.map(name => {
            const sync = engines[name].sync
            return sync.restored.get().then(() => (sync.landed() ? [] : [name]))
          }),
        ).then(unlanded => {
          void pulling()
          return Promise.all(unlanded.flat().map(name => engines[name].sync.whole())).then(() => undefined)
        }),
      )
    const readies: { readonly [S in Scope]: once.T<void> } = { Inbox: readyOf(SCOPES.Inbox, inboxOnly), All: readyOf(SCOPES.All, all) }

    // Each event planned for `w`, sealed, signed and kept; handed over behind the answer, so no round trip is waited on.
    const write = async (w: topics.Write, at: string, actor: string): Promise<void> => {
      await readies.All.get()
      const name = topics.store(w)
      const { store, sync } = topicStores[name]
      for (const event of topics.plan(topicsNow().reading, w, at, actor)) {
        const step = store.append(event)
        if (step !== undefined) await sync.stepped(step)
      }
      await signOwn(name)
      void handOver(name).catch(() => undefined)
    }

    return {
      lines,
      proposals: () => inbox.store,
      topics: topicsNow,
      ready: scope => readies[scope].get(),
      pull: all,
      heard: (store, box) => engines[store].heard(box),
      opened: () => void (wanted === "All" ? all() : wanted === "Inbox" ? inboxOnly() : undefined),
      shown: on => Promise.all([lines.shown(on), inbox.sync.shown(on), asks.sync.shown(on), ...WRITTEN_NAMES.map(n => topicStores[n].sync.shown(on))]).then(() => undefined),
      reset: store => engines[store].reset(),
      write,
    }
  }
}
export default storeset
