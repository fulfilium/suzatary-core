// Every store the tab holds, in the core's worker: the lines (`linesync`), whose content is named and fetched, and the
// inbox and the two topic stores (`replicasync`), whose objects carry their literals. The core holds each store's
// replica and answers every reading of it; this drives their pulls, hands the box what this tab wrote in a topic
// store, and says what moved. The page holds none of it: it asks questions, says what the box's stream says of each
// store's heads and whether it is shown, and hears what moved.

import type { Core, Lines, ObjectStore, Proposals, TopicReading, TopicStore } from "./core"
import { CoreError } from "./core"
import type excluded from "./excluded"
import type heads from "./heads"
import linesync from "./linesync"
import type progress from "./progress"
import replicasync from "./replicasync"
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
  // What the worker says of the stores: a reading's leavings by reader, a pull's progress, that a store moved (the
  // conversations a move of the lines touched), the rendered blocks it forgot, and a topic this tab wrote that the box
  // refused, in the box's words.
  export type Said =
    | { readonly type: "Told"; readonly reader: string; readonly left: excluded.Left }
    | { readonly type: "Bars"; readonly e: progress.Event }
    | { readonly type: "Moved"; readonly conversations: linesync.Moved }
    | { readonly type: "Forgot"; readonly blocks: readonly string[] }
    | { readonly type: "Refused"; readonly why: string }

  // What a reading reads: the inbox alone, or every store.
  export type Scope = "Inbox" | "All"
  const SCOPES: { readonly [S in Scope]: readonly stores.Name[] } = { Inbox: ["inbox"], All: stores.NAMES }
  const SCOPE_NAMES: readonly Scope[] = ["Inbox", "All"]

  // The core's two topic stores and their one reading, made once per move of either.
  export type Topics = { readonly replicas: { readonly [N in topics.Store]: TopicStore }; readonly reading: TopicReading }

  export type T = {
    readonly lines: linesync.T
    // The core's inbox, and its reading of the topics as held.
    readonly proposals: () => Proposals
    readonly topics: () => Topics
    // Local-first, store by store: a store this device has landed answers at once and is pulled behind; one it never
    // landed is waited for until it is whole, so a reading never reads part of it and never reads nothing for empty.
    readonly ready: (scope: Scope) => Promise<void>
    // Every store pulled, what this tab wrote handed over first; whether every pull ended whole.
    readonly pull: () => Promise<boolean>
    // The box's stream named a store's heads, or opened again.
    readonly heard: (store: stores.Name, box: heads.Heads) => void
    readonly opened: () => void
    // Whether a page is shown, as the page says: a hidden page may be closed, so every store's log is compacted.
    readonly shown: (on: boolean) => Promise<void>
    // A store's copy dropped and taken again, with what the reset could not do.
    readonly reset: (store: stores.Name) => Promise<readonly string[]>
    // `w` planned from the topics as held, written into its store as `actor` and handed to the box; a refusal rejects in
    // the box's words, and a post with no answer leaves it for the next sync.
    readonly write: (w: topics.Write, at: string, actor: string) => Promise<void>
  }

  // How a topic store's handing over ended: sent, kept for the next sync, or refused in words the owner is told.
  type Pushed = { readonly type: "Sent" } | { readonly type: "Kept" } | { readonly type: "Refused"; readonly why: string }
  const SENT: Pushed = { type: "Sent" }
  const KEPT: Pushed = { type: "Kept" }
  const TOPIC_STORES: readonly topics.Store[] = ["topics", "topic-aliases"]

  const Unsent = d.json(d.arr(d.obj<{ hash: string; literal: string }>({ hash: d.str, literal: d.str })))
  const unsentIn = (store: ObjectStore): readonly { readonly hash: string; readonly literal: string }[] => {
    const read = Unsent(store.unsent())
    if (read.type === "Val") return read.v
    throw new CoreError(`unsent: unexpected shape (${read.e})`)
  }

  export const of = (core: Core, held: Lines, say: (s: Said) => void, blocks: rendered.Store): T => {
    const told = (reader: string, left: excluded.Left): void => say({ type: "Told", reader, left })
    const bars = (e: progress.Event): void => say({ type: "Bars", e })
    // A store a later epoch started over is waited for again by every reading of it.
    const replaced = (name: stores.Name) => (): void => {
      for (const s of SCOPE_NAMES) if (SCOPES[s].includes(name)) readies[s].forget()
    }
    // A store of objects: the core's, driven as its pull asks, a move of what it reads touching every conversation.
    const objectStore = <S extends ObjectStore>(name: Exclude<stores.Name, "lines">, store: S): { readonly store: S; readonly sync: replicasync.T } => ({
      store,
      sync: replicasync.of<StorePayload>(
        {
          name,
          held: store,
          ask: since => storereads.objects(name, since),
          piece: (a, at) => store.piece(JSON.stringify(a), at),
          landed: () => Promise.resolve(),
        },
        { told, bars, moved: said => (said === "[]" ? undefined : say({ type: "Moved", conversations: "All" })), replaced: replaced(name) },
      ),
    })

    const lines = linesync.of(
      held,
      {
        told,
        bars,
        moved: conversations => say({ type: "Moved", conversations }),
        forgot: gone => say({ type: "Forgot", blocks: gone }),
        replaced: replaced("lines"),
      },
      blocks,
    )
    const inbox = objectStore("inbox", new core.Proposals())
    const topicStores = { topics: objectStore("topics", new core.Memberships()), "topic-aliases": objectStore("topic-aliases", new core.TopicAliases()) }
    const engines: { readonly [N in stores.Name]: replicasync.Store } = { lines: lines.engine, inbox: inbox.sync, topics: topicStores.topics.sync, "topic-aliases": topicStores["topic-aliases"].sync }

    // The topics as the core reads both stores, read again once either moved.
    const reading = last.of<string, TopicReading>(
      (a, b) => a === b,
      r => r.free(),
    )
    const topicsNow = (): Topics => {
      const replicas = { topics: topicStores.topics.store, "topic-aliases": topicStores["topic-aliases"].store }
      return { replicas, reading: reading(`${replicas.topics.version()}/${replicas["topic-aliases"].version()}`, () => new core.TopicReading(replicas.topics, replicas["topic-aliases"])) }
    }

    // What this tab wrote in a topic store and the box has not taken, handed over, the box's answer kept as a move of the
    // store. A refused object stays held as refused, so it leaves the reading and is never posted again; a post with no
    // answer leaves everything for the next sync.
    const push = async (name: topics.Store): Promise<Pushed> => {
      const { store, sync } = topicStores[name]
      const posting = unsentIn(store)
      if (posting.length === 0) return SENT
      try {
        const receipt = await storereads.post(
          name,
          posting.map(o => o.literal),
          store.since(),
        )
        await sync.stepped(store.answered(JSON.stringify(receipt)))
        return receipt.refused.length === 0 ? SENT : { type: "Refused", why: `the box refused this topic: ${receipt.refused.map(r => r.reason).join("; ")}` }
      } catch (e: unknown) {
        if (e instanceof http.ApiError && e.status === http.OUTDATED) return { type: "Refused", why: "the box runs a newer build: reload, and this topic goes with the next sync" }
        if (http.unanswered(e)) return KEPT
        const reason = lo.reason(e)
        await sync.stepped(store.refuse(JSON.stringify(posting.map(o => o.hash)), reason))
        return { type: "Refused", why: `the box refused this topic: ${reason}` }
      }
    }

    // Which stores a reading has asked for so far. A reconnect pulls those and nothing else, so before a reading asks it
    // pulls nothing.
    let wanted: "None" | Scope = "None"

    // The lines, then the inbox: the box files a proposal before the line that names it, so an inbox pulled after the
    // lines holds every proposal they name. The topic stores name rounds, never a proposal, so they are pulled beside
    // them, each after what this tab wrote in it is handed over; a refusal is said, and a push with no answer waits.
    const all = (): Promise<boolean> => {
      wanted = "All"
      const topicPulls = Promise.all(
        TOPIC_STORES.map(name =>
          topicStores[name].sync.sync.restored
            .get()
            .then(() => push(name))
            .then(pushed => (pushed.type === "Refused" ? say({ type: "Refused", why: pushed.why }) : undefined), () => undefined)
            .then(() => topicStores[name].sync.want()),
        ),
      )
      return lines.engine.want().then(l => inbox.sync.want().then(i => topicPulls.then(ts => l && i && ts.every(t => t))))
    }
    // The inbox alone, for a reading of the proposals; a move pulls the rest too once a reading asked for all.
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

    // Each event the topics as held plan for `w`, sealed into its store as this tab's and kept, then handed to the box.
    const write = async (w: topics.Write, at: string, actor: string): Promise<void> => {
      await readies.All.get()
      const name = topics.store(w)
      const { store, sync } = topicStores[name]
      for (const event of topics.plan(topicsNow().reading, w, at, actor)) {
        const step = store.append(event)
        if (step !== undefined) await sync.stepped(step)
      }
      const pushed = await push(name)
      if (pushed.type === "Refused") throw new Error(pushed.why)
    }

    return {
      lines,
      proposals: () => inbox.store,
      topics: topicsNow,
      ready: scope => readies[scope].get(),
      pull: all,
      heard: (store, box) => engines[store].heard(box),
      opened: () => void (wanted === "All" ? all() : wanted === "Inbox" ? inboxOnly() : undefined),
      shown: on => Promise.all([lines.shown(on), inbox.sync.shown(on), ...TOPIC_STORES.map(n => topicStores[n].sync.shown(on))]).then(() => undefined),
      reset: store => engines[store].reset(),
      write,
    }
  }
}
export default storeset
