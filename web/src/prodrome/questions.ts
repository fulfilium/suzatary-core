// The questions the tab asks of the core in its worker, and the one function that answers them. The worker holds each
// chain's objects, sent once and changed by the names added and dropped, and reads them into the core once per change:
// a question names a chain, an instant, a reading or terms, never objects. The same `serve` runs in the worker and,
// under test, in this thread, so an answer by message is the answer of the core read directly.

import type {
  ConversationPayload,
  ConversationsPayload,
  FoldPayload,
  InboxProposalsPayload,
  MemoryPayload,
  SearchPayload,
  SpecNode,
  StreamPayload,
  TopicsPayload,
} from "../wire/generated/wire"
import type { Acted, Chains, Core, CoreEnv, CoreFold, Erased, Linker, Named, Proposals, Standing, Todos, TopicReading, TopicStore, Window } from "./core"
import {
  appendTo,
  appendTopic,
  barOf,
  conversationOf,
  deadOf,
  erasedOf,
  excludedOf,
  foldOf,
  holdIn,
  inboxOf,
  linesLeft,
  linked,
  linker,
  linkerOf,
  listOf,
  listOfLines,
  objectsOf,
  searchOf,
  standingIn,
  streamOf,
  within,
} from "./core"
import topics from "./topics"
import excluded from "./excluded"
import lo from "../algebra/lo"
import thirty from "../todo/thirty"

export namespace questions {
  // A chain as the tab holds it now, by what changed since the last holding: names the worker lacks with their bytes,
  // and names it holds that the tab no longer does; or, `whole`, every name it holds, replacing what the worker holds,
  // after a holding the worker refused, since nothing then says what it took of it.
  export type Holding = {
    readonly name: string
    readonly untrusted: readonly string[]
    readonly add: readonly Named[]
    readonly drop: readonly string[]
    readonly whole: boolean
  }

  // What names a reading: the chain folded, the chains it is linked through as they were held (each chain and the
  // version of what it held), and the instant asked. Two chains read at one instant are two readings.
  export type Name = { readonly chain: string; readonly set: string; readonly at: string }
  export const same = (a: Name, b: Name): boolean => a.chain === b.chain && a.set === b.set && a.at === b.at

  // A reading of a chain at one instant, linked through every chain held.
  export type Read =
    | { readonly type: "Read"; readonly reading: Name; readonly at: string; readonly specs: CoreFold["specs"]; readonly functions: CoreFold["functions"] }
    | { readonly type: "Unheld" }

  // What prices a term: a reading's functions, or the box's rows, each todo's `Ref` its row's value at `at`.
  export type Basis = { readonly type: "Fold"; readonly reading: Name } | { readonly type: "Rows"; readonly at: string; readonly functions: Readonly<Record<string, SpecNode>> }

  // A store's objects as the tab holds them now, by what changed since it last said: the names the worker lacks with
  // their bytes, and the names it holds that the tab no longer does.
  export type Delta = { readonly add: readonly Named[]; readonly drop: readonly string[]; readonly whole: boolean }

  // The keys of the lines' words by the same rule: each key's hex by its name, and the names let go.
  export type Keys = { readonly add: Readonly<Record<string, string>>; readonly drop: readonly string[]; readonly whole: boolean }

  // A term's price: its value (`null` for ∅), or why it does not link.
  export type Priced = { readonly type: "Value"; readonly v: number | null } | { readonly type: "Refused"; readonly why: string }

  // Each question, and what answers it.
  export type Table = {
    // The chains held, replacing the last holding; what each chain's reading leaves out.
    Hold: { q: { readonly chains: readonly Holding[] }; a: readonly (readonly [string, excluded.Left])[] }
    // The box's `GET …/fold` of `chain` at `at`, priced through every chain held.
    List: { q: { readonly chain: string; readonly at: string | null }; a: FoldPayload }
    // `event` sealed onto `chain` as held, and the fold after it.
    Act: { q: { readonly chain: string; readonly event: string; readonly at: string }; a: Acted }
    // The reading at `at`, kept while the set and the instant stand.
    Reading: { q: { readonly chain: string; readonly at: string }; a: Read }
    // Each term's price under `basis`, in the order asked.
    Price: { q: { readonly basis: Basis; readonly terms: readonly SpecNode[] }; a: readonly Priced[] }
    // A todo's thirty days at `ms`.
    Thirty: { q: { readonly chain: string; readonly id: string; readonly ms: number }; a: thirty.Thirty }
    // What `key` is to `actor` under `root` in `chain`.
    Standing: { q: { readonly chain: string; readonly root: string | null; readonly actor: string; readonly key: string }; a: Standing }
    // The inbox's objects changed by `delta`.
    HoldInbox: { q: { readonly delta: Delta }; a: null }
    // The lines' objects and keys changed: each object decoded once, here, as it arrives.
    HoldLines: { q: { readonly delta: Delta; readonly keys: Keys }; a: null }
    // The Inbox's rows at `at`, priced through every chain held.
    Proposals: { q: { readonly at: string; readonly memory: MemoryPayload | null }; a: InboxProposalsPayload }
    // Every conversation, newest first.
    Conversations: { q: Record<never, never>; a: ConversationsPayload }
    // One conversation's window, or a refusal naming it where no line of it is held.
    Conversation: { q: { readonly id: string; readonly window: Window; readonly memory: MemoryPayload | null }; a: ConversationPayload }
    // Every conversation's turns holding `q`.
    Search: { q: { readonly q: string }; a: SearchPayload }
    // The keys an erasure names, and every key in `told`, with the blocks their words fed where `keys` opens them.
    Erased: { q: { readonly keys: Readonly<Record<string, string>>; readonly told: readonly string[] }; a: Erased }
    // Every key an erasure of its own conversation names.
    Dead: { q: Record<never, never>; a: readonly string[] }
    // What the readings of the inbox and the lines leave out, joined.
    Left: { q: Record<never, never>; a: excluded.T }
    // A topic store's objects changed by `delta`.
    HoldTopics: { q: { readonly store: topics.Store; readonly delta: Delta }; a: null }
    // The newest `pages` pages of the rounds carrying every topic of `chosen`, and the topics in use.
    Stream: { q: { readonly chosen: readonly string[]; readonly pages: number }; a: { readonly stream: StreamPayload; readonly bar: TopicsPayload } }
    // `write` planned by the topics as held and appended to the replica of its store: the objects it wrote there.
    PlanTopic: { q: { readonly write: topics.Write; readonly at: string; readonly actor: string }; a: { readonly store: topics.Store; readonly objects: readonly Named[] } }
  }
  export type Kind = keyof Table
  export type Question = { [k in Kind]: { readonly type: k } & Table[k]["q"] }[Kind]
  // The question of one kind.
  export type Of<K extends Kind> = Extract<Question, { readonly type: K }>
  export type Answer<K extends Kind> = Table[K]["a"]

  // On the wire: each question numbered, each answer under its number, a failure in the core's words.
  export type Asked = { readonly id: number; readonly question: Question }
  export type Answered = { readonly id: number; readonly answer: lo.Settled<Answer<Kind>> }

  // How many readings are kept at once: the page's, and the one it replaced while a page still reads it.
  const KEPT = 4

  type Chain = { objects: Map<string, string>; untrusted: readonly string[]; version: number; todos: { readonly version: number; readonly t: Todos } | null }
  type Reading = { readonly answer: Read & { type: "Read" }; readonly set: Linker; readonly price: (t: SpecNode) => number | null; readonly prices: Map<string, Priced> }

  // The box's rows carry their values, so nothing is bound when one is linked.
  const UNBOUND: CoreEnv = { outcomes: {}, tended: {} }

  const priced = (price: (t: SpecNode) => number | null, t: SpecNode): Priced => {
    try {
      return { type: "Value", v: price(t) }
    } catch (e) {
      return { type: "Refused", why: lo.reason(e) }
    }
  }

  // The answerer: each chain's objects, the core's set of them, and the readings kept, all its own.
  export const serve = (core: Core): ((q: Question) => Answer<Kind>) => {
    const chains = new Map<string, Chain>()
    const set: Chains = new core.Chains()
    const inSet = new Map<string, { readonly version: number; readonly left: excluded.Left }>()
    const readings: Reading[] = []
    let versions = 0

    const named = (name: string): Chain => {
      const c = chains.get(name)
      if (c === undefined) throw new Error(`this tab holds nothing of ${name}`)
      return c
    }
    const objectsIn = (c: Chain): Named[] => [...c.objects].map(([hash, literal]) => ({ hash, literal }))
    // A chain read into the core once per change of what it holds.
    const todosOf = (c: Chain): Todos => {
      if (c.todos?.version === c.version) return c.todos.t
      c.todos?.t.free()
      const t = new core.Todos(objectsOf(objectsIn(c)))
      c.todos = { version: c.version, t }
      return t
    }
    const forget = (c: Chain): void => {
      c.todos?.t.free()
      c.todos = null
    }
    // The set holds each chain as it is now; what names it is each chain's name and version.
    const setNow = (): string => {
      for (const [name, c] of chains) {
        if (inSet.get(name)?.version === c.version) continue
        inSet.set(name, { version: c.version, left: holdIn(set, name, objectsIn(c), c.untrusted) })
      }
      for (const name of [...inSet.keys()]) {
        if (chains.has(name)) continue
        set.drop(name)
        inSet.delete(name)
      }
      return [...chains].map(([name, c]) => `${name}:${c.version}`).sort().join(";")
    }
    // The readings kept, newest last; a reading is found by its name, never by a print of it.
    const kept = (name: Name): Reading | undefined => readings.find(r => same(r.answer.reading, name))
    const keep = (r: Reading): void => {
      readings.push(r)
      for (const old of readings.splice(0, Math.max(0, readings.length - KEPT))) old.set.free()
    }
    const sorted = (xs: readonly string[]): string => JSON.stringify([...xs].sort())

    // The inbox's objects, and the core's reading of them, made again once per change of what is held.
    const inbox = new Map<string, string>()
    let inboxRead: Proposals | null = null
    const proposals = (): Proposals => {
      if (inboxRead !== null) return inboxRead
      const read = new core.Proposals(objectsOf([...inbox].map(([hash, literal]) => ({ hash, literal }))))
      inboxRead = read
      return read
    }
    const holdInbox = (delta: Delta): null => {
      if (delta.whole) inbox.clear()
      for (const hash of delta.drop) inbox.delete(hash)
      for (const o of delta.add) inbox.set(o.hash, o.literal)
      if (delta.whole || delta.add.length > 0 || delta.drop.length > 0) {
        inboxRead?.free()
        inboxRead = null
      }
      return null
    }
    // The lines, held by what changed and read by what moved: the core keeps every decoded object and every reading.
    const lines = new core.Lines()
    // The names of the keys held, so a whole holding lets go of every one it does not name.
    const keysHeld = new Set<string>()
    const holdLines = (delta: Delta, keys: Keys): null => {
      if (delta.add.length > 0 || delta.drop.length > 0) lines.hold(objectsOf(delta.add), JSON.stringify(delta.drop))
      const drop = keys.whole ? [...keysHeld].filter(n => keys.add[n] === undefined) : keys.drop
      if (Object.keys(keys.add).length > 0 || drop.length > 0) lines.keys(JSON.stringify(keys.add), JSON.stringify(drop))
      for (const n of drop) keysHeld.delete(n)
      for (const n of Object.keys(keys.add)) keysHeld.add(n)
      return null
    }

    // The topic stores' objects, and the core's replicas of them with its one reading, made again once per change.
    const topicHeld: { readonly [N in topics.Store]: Map<string, string> } = { topics: new Map(), "topic-aliases": new Map() }
    type TopicsRead = { readonly memberships: TopicStore; readonly aliases: TopicStore; readonly reading: TopicReading }
    let topicsRead: TopicsRead | null = null
    const forgetTopics = (): void => {
      topicsRead?.reading.free()
      topicsRead?.memberships.free()
      topicsRead?.aliases.free()
      topicsRead = null
    }
    const held = (store: topics.Store): Named[] => [...topicHeld[store]].map(([hash, literal]) => ({ hash, literal }))
    const topicsNow = (): TopicsRead => {
      if (topicsRead !== null) return topicsRead
      const memberships = new core.Memberships(objectsOf(held("topics")))
      const aliases = new core.TopicAliases(objectsOf(held("topic-aliases")))
      topicsRead = { memberships, aliases, reading: new core.TopicReading(memberships, aliases) }
      return topicsRead
    }
    const holdTopics = (store: topics.Store, delta: Delta): null => {
      const map = topicHeld[store]
      if (delta.whole) map.clear()
      for (const hash of delta.drop) map.delete(hash)
      for (const o of delta.add) map.set(o.hash, o.literal)
      if (delta.whole || delta.add.length > 0 || delta.drop.length > 0) forgetTopics()
      return null
    }
    // The objects are held here at once, so the next plan reads them before the tab says it holds them.
    const planTopic = (write: topics.Write, at: string, actor: string): Answer<"PlanTopic"> => {
      const read = topicsNow()
      const store = topics.store(write)
      const into = store === "topics" ? read.memberships : read.aliases
      const objects = topics.plan(read.reading, write, at, actor).flatMap(e => appendTopic(into, e).objects)
      for (const o of objects) topicHeld[store].set(o.hash, o.literal)
      if (objects.length > 0) forgetTopics()
      return { store, objects }
    }

    const hold = (holding: readonly Holding[]): Answer<"Hold"> => {
      for (const h of holding) {
        const c = chains.get(h.name) ?? { objects: new Map<string, string>(), untrusted: [], version: 0, todos: null }
        chains.set(h.name, c)
        if (h.whole) c.objects.clear()
        for (const hash of h.drop) c.objects.delete(hash)
        for (const o of h.add) c.objects.set(o.hash, o.literal)
        if (h.whole || h.add.length > 0 || h.drop.length > 0 || sorted(h.untrusted) !== sorted(c.untrusted) || c.version === 0) c.version = versions += 1
        c.untrusted = h.untrusted
      }
      for (const [name, c] of [...chains]) {
        if (holding.some(h => h.name === name)) continue
        forget(c)
        chains.delete(name)
      }
      setNow()
      return [...inSet].map(([name, i]): readonly [string, excluded.Left] => [name, i.left])
    }

    const act = (chain: string, event: string, at: string): Answer<"Act"> => {
      setNow()
      const c = named(chain)
      const t = todosOf(c)
      try {
        return { appended: appendTo(t, event), answer: listOf(t, at, c.untrusted, chain, set) }
      } finally {
        // What it holds is what the tab holds, so the act is read in again once the tab keeps it and holds it.
        forget(c)
      }
    }

    const reading = (chain: string, at: string): Answer<"Reading"> => {
      const name: Name = { chain, set: setNow(), at }
      const known = kept(name)
      if (known !== undefined) return known.answer
      const c = chains.get(chain)
      if (c === undefined) return { type: "Unheld" }
      const f = foldOf(todosOf(c), at, c.untrusted)
      const linkSet = linkerOf(set, f.functions, f.at)
      const read: Reading = {
        answer: { type: "Read", reading: name, at: f.at, specs: f.specs, functions: f.functions },
        set: linkSet,
        price: linked(core, linker(t => linkSet.link(t)), f.env, f.at),
        prices: new Map(),
      }
      keep(read)
      return read.answer
    }

    const price = (basis: Basis, terms: readonly SpecNode[]): Answer<"Price"> => {
      if (basis.type === "Rows") {
        const p = linked(core, linker(within(core, basis.functions)), UNBOUND, basis.at)
        return terms.map(t => priced(p, t))
      }
      const r = kept(basis.reading)
      if (r === undefined) throw new Error("that reading is no longer kept; the page reads again")
      return terms.map(t => {
        const key = JSON.stringify(t)
        const known = r.prices.get(key)
        if (known !== undefined) return known
        const p = priced(r.price, t)
        r.prices.set(key, p)
        return p
      })
    }

    return q => {
      switch (q.type) {
        case "Hold":
          return hold(q.chains)
        case "List": {
          setNow()
          const c = named(q.chain)
          return listOf(todosOf(c), q.at, c.untrusted, q.chain, set)
        }
        case "Act":
          return act(q.chain, q.event, q.at)
        case "Reading":
          return reading(q.chain, q.at)
        case "Price":
          return price(q.basis, q.terms)
        case "Thirty": {
          setNow()
          const c = chains.get(q.chain)
          return c === undefined ? thirty.unheld : thirty.sample(core, set, todosOf(c), c.untrusted, q.id, q.ms)
        }
        case "Standing":
          return standingIn(todosOf(named(q.chain)), q.root, q.actor, q.key)
        case "HoldInbox":
          return holdInbox(q.delta)
        case "HoldLines":
          return holdLines(q.delta, q.keys)
        case "Proposals":
          setNow()
          return inboxOf(proposals(), q.at, q.memory, set)
        case "Conversations":
          return listOfLines(lines, proposals())
        case "Conversation":
          return conversationOf(lines, proposals(), q.id, q.window, q.memory)
        case "Search":
          return searchOf(lines, q.q)
        case "Erased":
          return erasedOf(lines, q.keys, q.told)
        case "Dead":
          return deadOf(lines)
        case "Left":
          return excluded.join(excludedOf(proposals()), linesLeft(lines))
        case "HoldTopics":
          return holdTopics(q.store, q.delta)
        case "Stream": {
          const read = topicsNow()
          return { stream: streamOf(lines, read.reading, q.chosen, q.pages), bar: barOf(read.reading) }
        }
        case "PlanTopic":
          return planTopic(q.write, q.at, q.actor)
      }
    }
  }

  // A port the answers go out on and the questions come in on: a worker's own scope, or one end of a channel.
  export type Port<In, Out> = {
    readonly postMessage: (m: Out) => void
    readonly addEventListener: (type: "message", f: (e: { readonly data: In }) => void) => void
  }

  // Each question answered in the order it came, once the core is ready; a core that did not load fails every one.
  export const listen = (port: Port<Asked, Answered>, ready: Promise<(q: Question) => Answer<Kind>>): void =>
    port.addEventListener("message", e => {
      const { id, question } = e.data
      void ready.then(
        answer => {
          const settled = ((): lo.Settled<Answer<Kind>> => {
            try {
              return lo.val(answer(question))
            } catch (x) {
              return lo.error(lo.reason(x))
            }
          })()
          port.postMessage({ id, answer: settled })
        },
        (x: unknown) => port.postMessage({ id, answer: lo.error(lo.reason(x)) }),
      )
    })
}
export default questions
