// Every store the tab holds, in the core's worker: one engine each (`engine`), the lines' content named and the rest
// carried in their nodes, and the core's readings of each made again once per move of its row. The page holds none of
// it: it asks questions, says what the box's stream says of each store's heads and whether it is shown, and hears what
// moved. A tab's own topic is written here, into its store's row, and handed to the box from here.

import type { Core, Lines, Proposals, TopicReading, TopicStore } from "./core"
import { appendTopic, objectsOf } from "./core"
import type engine from "./engine"
import type excluded from "./excluded"
import handover from "./handover"
import type heads from "./heads"
import linesync from "./linesync"
import locked from "./locked"
import { ALIAS_ROWS, INBOX_ROWS, TOPIC_ROWS } from "./objects"
import objectsync from "./objectsync"
import type progress from "./progress"
import { appended, standing } from "./replica"
import stores from "./stores"
import topics from "./topics"
import once from "../algebra/once"
import type idb from "../app/idb"
import type rendered from "../app/rendered"
import storereads from "../wire/storereads"
import type { StoredObject } from "../wire/generated/wire"

export namespace storeset {
  // What the worker says of the stores: a reading's leavings by reader, a pull's progress, that a store moved (the
  // conversations a move of the lines touched), the rendered blocks it forgot (by name, or all of them), and a topic this
  // tab wrote that the box refused, in the box's words.
  export type Said =
    | { readonly type: "Told"; readonly reader: string; readonly left: excluded.Left }
    | { readonly type: "Bars"; readonly e: progress.Event }
    | { readonly type: "Moved"; readonly conversations: linesync.Moved }
    | { readonly type: "Forgot"; readonly blocks: readonly string[] }
    | { readonly type: "Cleared" }
    | { readonly type: "Refused"; readonly why: string }

  // What a reading reads: the inbox alone, or every store.
  export type Scope = "Inbox" | "All"
  const SCOPES: { readonly [S in Scope]: readonly stores.Name[] } = { Inbox: ["inbox"], All: stores.NAMES }
  const SCOPE_NAMES: readonly Scope[] = ["Inbox", "All"]

  // The core's replica of each topic store and its one reading of them.
  export type Topics = { readonly replicas: { readonly [N in topics.Store]: TopicStore }; readonly reading: TopicReading }

  export type T = {
    readonly lines: linesync.T
    // The core's reading of the inbox and of the topics as held, made once per move of their rows.
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
    // A store's row written over by a reset and taken again, with what the reset could not do.
    readonly reset: (store: stores.Name) => Promise<readonly string[]>
    // `w` planned from the topics as held, written into its store's row as `actor` and handed to the box; a refusal
    // rejects in the box's words, and a post with no answer leaves it for the next sync.
    readonly write: (w: topics.Write, at: string, actor: string) => Promise<void>
  }

  // How a topic store's handing over ended: sent, kept for the next sync, or refused in words the owner is told.
  type Pushed = { readonly type: "Sent" } | { readonly type: "Kept" } | { readonly type: "Refused"; readonly why: string }
  const SENT: Pushed = { type: "Sent" }
  const KEPT: Pushed = { type: "Kept" }
  const TOPIC_STORES: readonly topics.Store[] = ["topics", "topic-aliases"]

  // The core's reading made from some rows, again only once one of them moved, the reading it replaces freed.
  const readOf = <R extends Readonly<Record<string, locked.Row<StoredObject>>>, A>(rows: () => R, make: (rows: R) => A, free: (a: A) => void): (() => A) => {
    let made: { readonly rows: R; readonly a: A } | null = null
    return () => {
      const now = rows()
      const was = made
      if (was !== null && Object.keys(now).every(k => now[k] === was.rows[k])) return was.a
      if (was !== null) free(was.a)
      made = { rows: now, a: make(now) }
      return made.a
    }
  }

  export const of = (core: Core, held: Lines, say: (s: Said) => void, blocks: rendered.Store): T => {
    const told = (reader: string, left: excluded.Left): void => say({ type: "Told", reader, left })
    const bars = (e: progress.Event): void => say({ type: "Bars", e })
    // A store a later epoch started over is waited for again by every reading of it.
    const replaced = (name: stores.Name) => (): void => {
      for (const s of SCOPE_NAMES) if (SCOPES[s].includes(name)) readies[s].forget()
    }
    const objectStore = (name: objectsync.Name, db: idb.Db): engine.T<StoredObject> =>
      objectsync.of(name, db, {
        told,
        bars,
        moved: () => say({ type: "Moved", conversations: "All" }),
        replaced: replaced(name),
      })

    const lines = linesync.of(
      held,
      {
        told,
        bars,
        moved: conversations => say({ type: "Moved", conversations }),
        forgot: gone => say({ type: "Forgot", blocks: gone }),
        cleared: () => say({ type: "Cleared" }),
        replaced: replaced("lines"),
      },
      blocks,
    )
    const inbox = objectStore("inbox", INBOX_ROWS)
    const topicStores: { readonly [N in topics.Store]: engine.T<StoredObject> } = {
      topics: objectStore("topics", TOPIC_ROWS),
      "topic-aliases": objectStore("topic-aliases", ALIAS_ROWS),
    }
    const engines: { readonly [N in stores.Name]: engine.Any } = { lines: lines.engine, inbox, ...topicStores }

    const proposals = readOf(
      () => ({ inbox: inbox.row() }),
      r => new core.Proposals(objectsOf(r.inbox.replica.objects)),
      p => p.free(),
    )
    // A reading of a topic store reads no object the box refused, nor what rests on one.
    const topicsNow = readOf(
      () => ({ memberships: topicStores.topics.row(), aliases: topicStores["topic-aliases"].row() }),
      (r): Topics => {
        const memberships = new core.Memberships(objectsOf(standing(r.memberships.replica)))
        const aliases = new core.TopicAliases(objectsOf(standing(r.aliases.replica)))
        return { replicas: { topics: memberships, "topic-aliases": aliases }, reading: new core.TopicReading(memberships, aliases) }
      },
      t => [t.reading, t.replicas.topics, t.replicas["topic-aliases"]].forEach(x => x.free()),
    )

    // What this tab wrote in a topic store and the box has not taken, handed over. A refused object stays held as
    // refused, so it leaves the reading and is never posted again; a post with no answer leaves everything for the next
    // sync.
    const push = async (name: topics.Store): Promise<Pushed> => {
      const store = topicStores[name]
      const ended = await handover.of(() => store.row().replica, true, (literals, since) => storereads.post(name, literals, since))
      switch (ended.type) {
        case "Nothing":
          return SENT
        case "Unanswered":
          return KEPT
        case "Outdated":
          return { type: "Refused", why: "the box runs a newer build: reload, and this topic goes with the next sync" }
        case "Refused":
          await store.sync.keep(locked.of(ended.replica))
          return { type: "Refused", why: `the box refused this topic: ${ended.reason}` }
        case "Taken":
          await store.sync.keep(locked.of(ended.replica))
          return ended.refused.length === 0 ? SENT : { type: "Refused", why: `the box refused this topic: ${ended.refused.map(r => r.reason).join("; ")}` }
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
          topicStores[name].sync.restored
            .get()
            .then(() => push(name))
            .then(pushed => (pushed.type === "Refused" ? say({ type: "Refused", why: pushed.why }) : undefined), () => undefined)
            .then(() => topicStores[name].want()),
        ),
      )
      return lines.engine.want().then(l => inbox.want().then(i => topicPulls.then(ts => l && i && ts.every(t => t))))
    }
    // The inbox alone, for a reading of the proposals; a move pulls the rest too once a reading asked for all.
    const inboxOnly = (): Promise<boolean> => {
      if (wanted === "None") wanted = "Inbox"
      return inbox.want()
    }

    const readyOf = (names: readonly stores.Name[], pulling: () => Promise<boolean>): once.T<void> =>
      once.of(() =>
        Promise.all(names.map(name => engines[name].sync.restored.get())).then(() => {
          void pulling()
          return Promise.all(names.filter(name => !engines[name].sync.landed()).map(name => engines[name].sync.whole())).then(() => undefined)
        }),
      )
    const readies: { readonly [S in Scope]: once.T<void> } = { Inbox: readyOf(SCOPES.Inbox, inboxOnly), All: readyOf(SCOPES.All, all) }

    const write = async (w: topics.Write, at: string, actor: string): Promise<void> => {
      await readies.All.get()
      const read = topicsNow()
      const name = topics.store(w)
      const objects = topics.plan(read.reading, w, at, actor).flatMap(e => appendTopic(read.replicas[name], e).objects)
      if (objects.length === 0) return
      const store = topicStores[name]
      await store.sync.keep(locked.of(appended(store.row().replica, objects)))
      const pushed = await push(name)
      if (pushed.type === "Refused") throw new Error(pushed.why)
    }

    return {
      lines,
      proposals,
      topics: topicsNow,
      ready: scope => readies[scope].get(),
      pull: all,
      heard: (store, box) => engines[store].heard(box),
      opened: () => void (wanted === "All" ? all() : wanted === "Inbox" ? inboxOnly() : undefined),
      reset: store => engines[store].reset(),
      write,
    }
  }
}
export default storeset
