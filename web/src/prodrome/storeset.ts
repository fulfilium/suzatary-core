// Every store the tab holds, in the core's worker: their pulls, the hand-over of what this tab wrote, its refusals,
// and what moved. The page holds none of it.

import type { Core, Lines, ObjectStore, Proposals, TopicReading, TopicStore } from "./core"
import type excluded from "./excluded"
import type heads from "./heads"
import linesync from "./linesync"
import type progress from "./progress"
import replicasync from "./replicasync"
import stores from "./stores"
import topics from "./topics"
import written from "./written"
import last from "../algebra/last"
import lo from "../algebra/lo"
import once from "../algebra/once"
import type rendered from "../app/rendered"
import storereads from "../wire/storereads"
import type { StorePayload } from "../wire/generated/wire"
import { measure } from "../app/perf"

export namespace storeset {
  export type Said =
    | { readonly type: "Told"; readonly reader: string; readonly left: excluded.Left }
    | { readonly type: "Bars"; readonly e: progress.Event }
    | { readonly type: "Moved"; readonly conversations: linesync.Moved }
    | { readonly type: "Refused"; readonly why: string }

  export type Scope = "Inbox" | "All"
  const SCOPES: { readonly [S in Scope]: readonly stores.Name[] } = { Inbox: ["inbox"], All: stores.NAMES }
  const SCOPE_NAMES: readonly Scope[] = ["Inbox", "All"]

  // The core's two topic stores and their one reading, made once per move of either.
  export type Topics = { readonly replicas: { readonly [N in stores.Written]: TopicStore }; readonly reading: TopicReading }

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

  // The stores this tab writes, each with the noun a refusal names.
  const NOUNS: { readonly [N in stores.Written]: string } = { topics: "topic", "topic-aliases": "topic's new name" }
  const listed = <const L extends readonly stores.Written[]>(names: L): L => names
  const WRITTEN = listed(["topics", "topic-aliases"])
  // Every store a tab writes is in the list: a name left out fails to compile here.
  const everyWritten: [Exclude<stores.Written, (typeof WRITTEN)[number]>] extends [never] ? true : never = true
  void everyWritten

  export const of = (core: Core, held: Lines, say: (s: Said) => void, blocks: rendered.Device): T => {
    const told = (reader: string, left: excluded.Left): void => say({ type: "Told", reader, left })
    const bars = (e: progress.Event): void => say({ type: "Bars", e })
    // A store a later epoch started over no longer holds what a reading was ready over, so every reading of it waits
    // again, as for a store never landed, until a pull of it ends whole.
    const replaced = (name: stores.Name) => (): void => {
      for (const s of SCOPE_NAMES) if (SCOPES[s].includes(name)) readies[s].forget()
    }
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
    const topicStores: { readonly [N in stores.Written]: { readonly store: TopicStore; readonly sync: replicasync.T } } = {
      topics: objectStore("topics", new core.Memberships()),
      "topic-aliases": objectStore("topic-aliases", new core.TopicAliases()),
    }
    const asks: { readonly store: ObjectStore; readonly sync: replicasync.T } = objectStore("asks", new core.Asks(), () => void writes.refusedIn(asks.store))
    const engines: { readonly [N in stores.Name]: replicasync.Store } = { lines: lines.engine, inbox: inbox.sync, topics: topicStores.topics.sync, "topic-aliases": topicStores["topic-aliases"].sync, asks: asks.sync }

    const reading = last.of<string, TopicReading>(
      (a, b) => a === b,
      r => r.free(),
    )
    const topicsNow = (): Topics => {
      const replicas = { topics: topicStores.topics.store, "topic-aliases": topicStores["topic-aliases"].store }
      return { replicas, reading: reading(`${replicas.topics.version()}/${replicas["topic-aliases"].version()}`, () => new core.TopicReading(replicas.topics, replicas["topic-aliases"])) }
    }

    // Read again whenever it moves: each store this tab writes holds what it records as refused.
    const entry = (name: stores.Written): written.Entry => ({ noun: NOUNS[name], store: topicStores[name].store, stepped: step => topicStores[name].sync.stepped(step) })
    const writes = written.of(core, { topics: entry("topics"), "topic-aliases": entry("topic-aliases") }, why => say({ type: "Refused", why }))
    // A hand-over that failed is said, and the next sync tries it again.
    const failed = (name: stores.Written) => (e: unknown): void => say({ type: "Refused", why: `This tab's ${NOUNS[name]} writes were not handed over (${lo.reason(e)}); the next sync tries again.` })

    // Which stores a reading has asked for so far: a reconnect pulls those alone.
    let wanted: "None" | Scope = "None"

    // The lines, then the inbox, which then holds every proposal they name; the topic stores beside them after their
    // hand-over, and the asks after those posts.
    const all = (): Promise<boolean> => {
      wanted = "All"
      const topicPulls = Promise.all(
        WRITTEN.map(name =>
          topicStores[name].sync.sync.restored
            .get()
            .then(() => writes.handOver(name))
            .catch(failed(name))
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
      const name = topics.store(w)
      const measured = async <A>(step: string, f: () => A | Promise<A>): Promise<A> => {
        const start = performance.now()
        const a = await f()
        measure(`write:${step}`, start, { store: name })
        return a
      }
      await measured("ready", () => readies.All.get())
      const { store, sync } = topicStores[name]
      const events = await measured("plan", () => topics.plan(topicsNow().reading, w, at, actor))
      for (const event of events) {
        const step = await measured("append", () => store.append(event))
        if (step !== undefined) await measured("keep", () => sync.stepped(step))
      }
      await measured("sign", () => writes.sign(name))
      void writes.handOver(name).catch(failed(name))
    }

    return {
      lines,
      proposals: () => inbox.store,
      topics: topicsNow,
      ready: scope => readies[scope].get(),
      pull: all,
      heard: (store, box) => engines[store].heard(box),
      opened: () => void (wanted === "All" ? all() : wanted === "Inbox" ? inboxOnly() : undefined),
      shown: on => Promise.all([lines.shown(on), inbox.sync.shown(on), asks.sync.shown(on), ...WRITTEN.map(name => topicStores[name].sync.shown(on))]).then(() => undefined),
      reset: store => engines[store].reset(),
      write,
    }
  }
}
export default storeset
