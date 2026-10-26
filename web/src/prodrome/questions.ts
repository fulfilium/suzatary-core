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
  TurnRow,
} from "../wire/generated/wire"
import type { Acted, Chains, Core, CoreEnv, CoreFold, Linker, Named, Proposals, Standing, Todos, TopicReading, TopicStore, Window } from "./core"
import {
  appendTo,
  appendTopic,
  barOf,
  conversationOf,
  excludedOf,
  foldOf,
  functionsOf,
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
import type heads from "./heads"
import lines from "./lines"
import linesync from "./linesync"
import lo from "../algebra/lo"
import thirty from "../todo/thirty"
import grouped from "../chat/grouped"
import rendered from "../app/rendered"
import type time from "../app/time"

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

  // A conversation not on this device: how many of its lines the device lacks, and why they could not be fetched.
  export type NotHere = { readonly type: "NotHere"; readonly lines: number; readonly why: string }

  // A reading of every line, or how many lines the device lacks and why: the same shape as one conversation's.
  export type Whole<A> = { readonly type: "Held"; readonly v: A } | NotHere

  // One conversation's window, absent where the lines hold no line of it, or not on this device.
  export type One = { readonly type: "Absent" } | { readonly type: "Held"; readonly payload: ConversationPayload } | NotHere
  // The key each answer block is kept by, by its source.
  export type Keyed = Readonly<Record<string, string>>

  // One conversation's window grouped into rounds, with the key of each of its answer blocks, absent, or not on this
  // device.
  export type Grouped = { readonly type: "Absent" } | { readonly type: "Held"; readonly payload: grouped.Window; readonly keys: Keyed } | NotHere

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
    // A page reads the lines: pull them now, and after every word of the box's stream; whether the pull ended whole.
    LinesWant: { q: Record<never, never>; a: boolean }
    // The box's stream named the lines' heads.
    LinesHeard: { q: { readonly heads: heads.Heads }; a: null }
    // Whether a page is shown: a prefetch waits while none is.
    LinesShown: { q: { readonly on: boolean }; a: null }
    // The lines' row written over by a reset and taken again, with what the reset could not do.
    LinesReset: { q: Record<never, never>; a: readonly string[] }
    // The Inbox's rows at `at`, priced through every chain held.
    Proposals: { q: { readonly at: string; readonly memory: MemoryPayload | null }; a: InboxProposalsPayload }
    // Every conversation, newest first.
    Conversations: { q: Record<never, never>; a: Whole<ConversationsPayload> }
    // One conversation's window, read over its own lines once they are held.
    Conversation: { q: { readonly id: string; readonly window: Window; readonly memory: MemoryPayload | null }; a: One }
    // The same, read back to the turn it must reach (`grouped.covering`) and grouped into rounds on the owner's days in
    // `zone`, with the key each answer block is kept by.
    Window: { q: { readonly id: string; readonly window: grouped.Asked; readonly memory: MemoryPayload | null; readonly zone: time.Zone }; a: Grouped }
    // Every conversation's turns holding `q`.
    Search: { q: { readonly q: string }; a: Whole<SearchPayload> }
    // What the readings of the inbox and the lines leave out, joined.
    Left: { q: Record<never, never>; a: Whole<excluded.T> }
    // The rendered blocks the device keeps, which the worker alone reads and writes: those of `keys`, the box's answers
    // `pairs` joined (the findings it made), how much it keeps, and the functions this build keeps blocks of.
    Blocks: { q: { readonly keys: readonly string[] }; a: (string | null)[] }
    KeepBlocks: { q: { readonly pairs: readonly (readonly [string, string])[] }; a: string[] }
    BlocksUsage: { q: Record<never, never>; a: rendered.Usage }
    BlocksOwn: { q: Record<never, never>; a: ReadonlySet<string> }
    // The key each answer block of `sources` is kept by, named by this build's core.
    AnswerKeys: { q: { readonly sources: readonly string[] }; a: string[] }
    // A topic store's objects changed by `delta`.
    HoldTopics: { q: { readonly store: topics.Store; readonly delta: Delta }; a: null }
    // The newest `pages` pages of the rounds carrying every topic of `chosen`, and the topics in use, once every line is
    // held.
    Stream: { q: { readonly chosen: readonly string[]; readonly pages: number }; a: Whole<{ readonly stream: StreamPayload; readonly bar: TopicsPayload; readonly keys: Keyed }> }
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

  // What the worker says unasked: what the lines' sync tells, that the box refused this device's key (`Gone`) or this
  // build's contract (`Stale`) on a read the worker made, and that its core did not load, and why (`Unloaded`).
  export type Said = linesync.Said | { readonly type: "Gone" } | { readonly type: "Stale" } | { readonly type: "Unloaded"; readonly why: string }
  // Everything the worker sends: an answer, or something said.
  export type Out = Answered | { readonly said: Said }

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
  // An answer now, or once the lines a question reads are held.
  export type Answering = (q: Question) => Answer<Kind> | Promise<Answer<Kind>>

  export const serve = (core: Core, say: (s: Said) => void = () => {}): Answering => {
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
    // The lines, pulled and held here, decoded once each and read by what moved: the core keeps every reading.
    const held = new core.Lines()
    // The rendered blocks the device keeps, read and written here alone, of the functions this build keeps blocks of.
    const own = functionsOf(core)
    const blocks = rendered.device(own)
    const sync = linesync.of(held, say, blocks)
    // A reading of conversation `id` that failed while lines of it are not on this device failed for want of them.
    const notHere = (scope: lines.Scope, e: unknown): NotHere => {
      const lacked = sync.lacking(scope)
      if (lacked === 0) throw e
      return { type: "NotHere", lines: lacked, why: lo.reason(e) }
    }
    // A reading of every line once they are held, or how many the device lacks where they cannot be fetched.
    const whole = <A>(read: () => A): Promise<Whole<A>> =>
      sync.over(lines.WHOLE, (): Whole<A> => ({ type: "Held", v: read() })).catch((e: unknown): Whole<A> => notHere(lines.WHOLE, e))

    // The key each answer block of `turns` is kept by, named once here, where the core is.
    const keyed = (turns: readonly TurnRow[]): Keyed =>
      Object.fromEntries(turns.flatMap(t => (t.who === "model" ? t.blocks.map(b => [b.source, core.answer_key(b.source)]) : [])))

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
    const topicObjects = (store: topics.Store): Named[] => [...topicHeld[store]].map(([hash, literal]) => ({ hash, literal }))
    const topicsNow = (): TopicsRead => {
      if (topicsRead !== null) return topicsRead
      const memberships = new core.Memberships(objectsOf(topicObjects("topics")))
      const aliases = new core.TopicAliases(objectsOf(topicObjects("topic-aliases")))
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
        case "LinesWant":
          return sync.want()
        case "LinesHeard":
          sync.heard(q.heads)
          return null
        case "LinesShown":
          sync.shown(q.on)
          return null
        case "LinesReset":
          return sync.reset()
        case "Proposals":
          setNow()
          return inboxOf(proposals(), q.at, q.memory, set)
        case "Conversations":
          return whole(() => listOfLines(held, proposals()))
        case "Conversation": {
          const { id, window, memory } = q
          return sync
            .holds(id)
            .then((holds): Promise<One> | One =>
              holds ? sync.over({ type: "Conversation", id }, (): One => ({ type: "Held", payload: conversationOf(held, proposals(), id, window, memory) })) : { type: "Absent" },
            )
            .catch((e: unknown): One => notHere({ type: "Conversation", id }, e))
        }
        case "Window": {
          const { id, window, memory, zone } = q
          const read = (w: Window): ConversationPayload => conversationOf(held, proposals(), id, w, memory)
          return sync
            .holds(id)
            .then((holds): Promise<Grouped> | Grouped =>
              holds
                ? sync.over({ type: "Conversation", id }, (): Grouped => {
                    const payload = grouped.of(grouped.covering(window, read), zone)
                    return { type: "Held", payload, keys: keyed(payload.turns) }
                  })
                : { type: "Absent" },
            )
            .catch((e: unknown): Grouped => notHere({ type: "Conversation", id }, e))
        }
        case "Search":
          return whole(() => searchOf(held, q.q))
        // Over the same lines the reading beside it waits for, so it says what that reading left out.
        case "Left":
          return whole(() => excluded.join(excludedOf(proposals()), linesLeft(held)))
        case "Blocks":
          return blocks.get(q.keys)
        case "KeepBlocks":
          return blocks.join(q.pairs)
        case "BlocksUsage":
          return blocks.usage()
        case "BlocksOwn":
          return own
        case "AnswerKeys":
          return q.sources.map(source => core.answer_key(source))
        case "HoldTopics":
          return holdTopics(q.store, q.delta)
        case "Stream":
          return whole(() => {
            const read = topicsNow()
            const stream = streamOf(held, read.reading, q.chosen, q.pages)
            return { stream, bar: barOf(read.reading), keys: keyed(stream.rounds.flatMap(r => (r.answer === null ? [] : [r.answer]))) }
          })
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

  // Each question answered once the core is ready, in the order it came where it answers at once and when the lines it
  // reads are held where it waits on them; a core that did not load fails every one.
  export const listen = (port: Port<Asked, Answered>, ready: Promise<Answering>): void =>
    port.addEventListener("message", e => {
      const { id, question } = e.data
      void ready
        .then(answer => answer(question))
        .then(
          (v): lo.Settled<Answer<Kind>> => lo.val(v),
          (x: unknown): lo.Settled<Answer<Kind>> => lo.error(lo.reason(x)),
        )
        .then(settled => port.postMessage({ id, answer: settled }))
    })
}
export default questions
