// The questions the tab asks the core in its worker, and `serve`, which answers them. A question names a chain, an
// instant, a reading or terms, never objects; under test the same `serve` runs in this thread.

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
import type { Acted, Chains, Core, CoreEnv, CoreFold, Linker, Named, Standing, Todos, Window } from "./core"
import {
  appendTo,
  barOf,
  chosenOf,
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
import type topics from "./topics"
import excluded from "./excluded"
import type heads from "./heads"
import lanes from "./lanes"
import lines from "./lines"
import phases from "./phases"
import storeset from "./storeset"
import type stores from "./stores"
import lo from "../algebra/lo"
import idb from "../app/idb"
import last from "../algebra/last"
import thirty from "../todo/thirty"
import grouped from "../chat/grouped"
import rendered from "../app/rendered"
import type time from "../app/time"

export namespace questions {
  // `whole` replaces what the worker holds: sent after a refused holding, since nothing then says what it took.
  export type Holding = {
    readonly name: string
    readonly untrusted: readonly string[]
    readonly add: readonly Named[]
    readonly drop: readonly string[]
    readonly whole: boolean
  }

  // `set` names the version of every chain held.
  export type Name = { readonly chain: string; readonly set: string; readonly at: string }
  export const same = (a: Name, b: Name): boolean => a.chain === b.chain && a.set === b.set && a.at === b.at

  export type Read =
    | { readonly type: "Read"; readonly reading: Name; readonly at: string; readonly specs: CoreFold["specs"]; readonly functions: CoreFold["functions"] }
    | { readonly type: "Unheld" }

  // `Rows`: each todo's `Ref` prices as its box row's value at `at`.
  export type Basis = { readonly type: "Fold"; readonly reading: Name } | { readonly type: "Rows"; readonly at: string; readonly functions: Readonly<Record<string, SpecNode>> }

  export type NotHere = { readonly type: "NotHere"; readonly lines: number; readonly why: string }

  export type Whole<A> = { readonly type: "Held"; readonly v: A } | NotHere

  // `Absent` once a whole pull says the box holds no line of it either; `Unheard` until then.
  export type Unheard = { readonly type: "Unheard" }
  export type Unheld = { readonly type: "Absent" } | Unheard
  const ABSENT: Unheld = { type: "Absent" }
  const UNHEARD: Unheard = { type: "Unheard" }

  export type One = Unheld | { readonly type: "Held"; readonly payload: ConversationPayload } | NotHere
  export type Keyed = Readonly<Record<string, string>>

  export type Grouped = Unheld | { readonly type: "Held"; readonly payload: grouped.Window; readonly keys: Keyed } | NotHere

  // `null` is ∅.
  export type Priced = { readonly type: "Value"; readonly v: number | null } | { readonly type: "Refused"; readonly why: string }

  export type Table = {
    Hold: { q: { readonly chains: readonly Holding[] }; a: null }
    // The box's `GET …/fold`.
    List: { q: { readonly chain: string; readonly at: string | null }; a: FoldPayload }
    Act: { q: { readonly chain: string; readonly event: string; readonly at: string }; a: Acted }
    // Kept while the set and the instant stand.
    Reading: { q: { readonly chain: string; readonly at: string }; a: Read }
    Price: { q: { readonly basis: Basis; readonly terms: readonly SpecNode[] }; a: readonly Priced[] }
    Thirty: { q: { readonly chain: string; readonly id: string; readonly ms: number }; a: thirty.Thirty }
    Standing: { q: { readonly chain: string; readonly root: string | null; readonly actor: string; readonly key: string }; a: Standing }
    // Pulls every store now and after every word of the box's stream; true when every pull ended whole.
    Pull: { q: Record<never, never>; a: boolean }
    Heard: { q: { readonly store: stores.Name; readonly heads: heads.Heads }; a: null }
    // The box's stream reopened: pulls what a reading asked for.
    Opened: { q: Record<never, never>; a: null }
    // A prefetch waits while no page is shown.
    Shown: { q: { readonly on: boolean }; a: null }
    // Answers with what the reset could not do.
    Reset: { q: { readonly store: stores.Name }; a: readonly string[] }
    Proposals: { q: { readonly at: string; readonly memory: MemoryPayload | null }; a: InboxProposalsPayload }
    Conversations: { q: Record<never, never>; a: Whole<ConversationsPayload> }
    Conversation: { q: { readonly id: string; readonly window: Window; readonly memory: MemoryPayload | null }; a: One }
    // Read back to the turn it must reach (`grouped.covering`), grouped into rounds on the owner's days in `zone`.
    Window: { q: { readonly id: string; readonly window: grouped.Asked; readonly memory: MemoryPayload | null; readonly zone: time.Zone }; a: Grouped }
    Search: { q: { readonly q: string }; a: Whole<SearchPayload> }
    Left: { q: Record<never, never>; a: excluded.T }
    // The device's rendered blocks: the worker alone reads and writes them.
    Blocks: { q: { readonly keys: readonly string[] }; a: (string | null)[] }
    KeepBlocks: { q: { readonly pairs: readonly (readonly [string, string])[] }; a: string[] }
    BlocksUsage: { q: Record<never, never>; a: rendered.Usage }
    // Answered once the lines of the conversations the rounds are in are held.
    Stream: { q: { readonly chosen: readonly string[]; readonly pages: number }; a: Whole<{ readonly stream: StreamPayload; readonly bar: TopicsPayload; readonly keys: Keyed }> }
    // A refusal fails in the box's words.
    Topic: { q: { readonly write: topics.Write; readonly at: string; readonly actor: string }; a: null }
  }
  export type Kind = keyof Table
  export type Question = { [k in Kind]: { readonly type: k } & Table[k]["q"] }[Kind]
  export type Of<K extends Kind> = Extract<Question, { readonly type: K }>
  export type Answer<K extends Kind> = Table[K]["a"]

  export type Asked = { readonly id: number; readonly lane: lanes.Lane; readonly question: Question }
  export type Answered = { readonly id: number; readonly answer: lo.Settled<Answer<Kind>> }

  // Said unasked. `Gone`: the box refused this device's key on a worker read; `Stale`: refused this build's contract;
  // `Unloaded`: the core did not load.
  export type Said = storeset.Said | { readonly type: "Gone" } | { readonly type: "Stale" } | { readonly type: "Unloaded"; readonly why: string }
  export type Out = Answered | { readonly said: Said }

  // The page's reading, and the one it replaced while a page still reads it.
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

  // Resolves once the lane may run; an answer awaits it after waiting on a store.
  export type Turn = () => Promise<void>

  export type Answering = (q: Question, turn: Turn) => Answer<Kind> | Promise<Answer<Kind>>

  export const serve = (core: Core, say: (s: Said) => void = () => {}): Answering => {
    const chains = new Map<string, Chain>()
    const set: Chains = new core.Chains()
    const inSet = new Map<string, number>()
    const readings: Reading[] = []
    let versions = 0

    const named = (name: string): Chain => {
      const c = chains.get(name)
      if (c === undefined) throw new Error(`this tab holds nothing of ${name}`)
      return c
    }
    const objectsIn = (c: Chain): Named[] => [...c.objects].map(([hash, literal]) => ({ hash, literal }))
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
    const setNow = (): string => {
      for (const [name, c] of chains) {
        if (inSet.get(name) === c.version) continue
        say({ type: "Told", reader: `chain/${name}`, left: holdIn(set, name, objectsIn(c), c.untrusted) })
        inSet.set(name, c.version)
      }
      for (const name of [...inSet.keys()]) {
        if (chains.has(name)) continue
        set.drop(name)
        inSet.delete(name)
        say({ type: "Told", reader: `chain/${name}`, left: excluded.NOTHING })
      }
      return [...chains].map(([name, c]) => `${name}:${c.version}`).sort().join(";")
    }
    // Newest last; a reading is found by its name, never by a print of it.
    const kept = (name: Name): Reading | undefined => readings.find(r => same(r.answer.reading, name))
    const keep = (r: Reading): void => {
      readings.push(r)
      for (const old of readings.splice(0, Math.max(0, readings.length - KEPT))) old.set.free()
    }
    const sorted = (xs: readonly string[]): string => JSON.stringify([...xs].sort())

    const held = new core.Lines()
    // The rendered blocks the device keeps, read and written here alone, of the functions this build keeps blocks of, as
    // the lines admit them. The database a build before them kept blocks in goes.
    if (idb.available()) indexedDB.deleteDatabase(rendered.DB)
    let admit: rendered.Admit | null = null
    const blocks = rendered.device(functionsOf(core), {
      admits: (ks, s) => (admit === null ? Promise.resolve("Behind") : admit.admits(ks, s)),
      behind: () => admit?.behind() ?? Promise.resolve(),
    })
    const stores = storeset.of(core, held, say, blocks)
    const sync = stores.lines
    admit = sync.admit
    const proposals = stores.proposals
    // A reading that failed while some of its lines are not on this device failed for want of them.
    const notHere = (scope: lines.Scope, e: unknown): NotHere => {
      const lacked = sync.lacking(scope)
      if (lacked === 0) throw e
      return { type: "NotHere", lines: lacked, why: lo.reason(e) }
    }
    const ready = (scope: storeset.Scope, turn: Turn): Promise<void> => stores.ready(scope).then(turn)
    const scoped = <A>(turn: Turn, scope: () => lines.Scope, read: () => A, stores: storeset.Scope = "All"): Promise<Whole<A>> =>
      ready(stores, turn)
        .then(() => sync.over(scope, (): Whole<A> => ({ type: "Held", v: read() })))
        .catch((e: unknown): Whole<A> => notHere(scope(), e))
    const whole = <A>(turn: Turn, read: () => A): Promise<Whole<A>> => scoped(turn, () => lines.WHOLE, read)
    // Where the lines hold none of `id`, the device may not have pulled it yet; only a whole pull says the box holds none.
    const conversation = async <A>(id: string, turn: Turn, read: () => Promise<A> | A): Promise<A | Unheld> => {
      await ready("All", turn)
      if (sync.holds(id)) return read()
      const pulled = await stores.pull()
      await turn()
      return sync.holds(id) ? read() : pulled ? ABSENT : UNHEARD
    }

    // Read again only once the inbox, the chains held or the memory moved; until then an ask gets the first reading,
    // whose `at` names its instant.
    const inbox = last.of<{ readonly version: number; readonly set: string; readonly memory: string }, InboxProposalsPayload>(
      (a, b) => a.version === b.version && a.set === b.set && a.memory === b.memory,
    )

    const keyed = (turns: readonly TurnRow[]): Keyed =>
      Object.fromEntries(turns.flatMap(t => (t.who === "model" ? t.blocks.map(b => [b.source, core.answer_key(b.source)]) : [])))

    const hold = (holding: readonly Holding[]): null => {
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
      return null
    }

    const act = (chain: string, event: string, at: string): Answer<"Act"> => {
      setNow()
      const c = named(chain)
      const t = todosOf(c)
      try {
        return { appended: appendTo(t, event), answer: listOf(t, at, c.untrusted, chain, set) }
      } finally {
        // The worker holds what the tab holds, so the act is read in again once the tab holds it.
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

    return (q, turn) => {
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
        case "Pull":
          return stores.pull()
        case "Heard":
          stores.heard(q.store, q.heads)
          return null
        case "Opened":
          stores.opened()
          return null
        case "Shown":
          return stores.shown(q.on).then(() => null)
        case "Reset":
          return stores.reset(q.store)
        case "Proposals":
          return ready("Inbox", turn).then(() => {
            const of = proposals()
            return inbox({ version: of.version(), set: setNow(), memory: JSON.stringify(q.memory) }, () => inboxOf(of, q.at, q.memory, set))
          })
        case "Conversations":
          return whole(turn, () => listOfLines(held, proposals()))
        case "Conversation": {
          const { id, window, memory } = q
          return conversation(id, turn, () => sync.over(() => lines.one(id), (): One => ({ type: "Held", payload: conversationOf(held, proposals(), id, window, memory) })))
            .catch((e: unknown): One => notHere(lines.one(id), e))
        }
        case "Window": {
          const { id, window, memory, zone } = q
          const read = (w: Window): ConversationPayload => conversationOf(held, proposals(), id, w, memory)
          return conversation(id, turn, () =>
            sync.over(() => lines.one(id), (): Grouped => {
              const payload = grouped.of(grouped.covering(window, read), zone)
              return { type: "Held", payload, keys: keyed(payload.turns) }
            }),
          )
            .catch((e: unknown): Grouped => notHere(lines.one(id), e))
        }
        case "Search":
          return whole(turn, () => searchOf(held, q.q))
        // Asked after a reading, so it says at least what that reading left out.
        case "Left":
          return excluded.join(excludedOf(proposals()), linesLeft(held))
        case "Blocks":
          return blocks.get(q.keys)
        case "KeepBlocks":
          return blocks.join(q.pairs)
        case "BlocksUsage":
          return blocks.usage()
        // The core's skeleton chooses the rounds; only the lines whose words they read are read.
        case "Stream":
          phases.mark("stream asked")
          return scoped(
            turn,
            () => ({ type: "Objects", names: chosenOf(held, stores.topics().reading, q.chosen, q.pages).map(n => n.hash) }),
            () => {
              phases.mark("stream content held")
              const read = stores.topics()
              const stream = streamOf(held, read.reading, q.chosen, q.pages)
              phases.mark("stream drawn")
              return { stream, bar: barOf(read.reading), keys: keyed(stream.rounds.flatMap(r => (r.answer === null ? [] : [r.answer]))) }
            },
          )
        case "Topic":
          return stores.write(q.write, q.at, q.actor).then(() => null)
      }
    }
  }

  export type Port<In, Out> = {
    readonly postMessage: (m: Out) => void
    readonly addEventListener: (type: "message", f: (e: { readonly data: In }) => void) => void
  }

  type Item = { readonly type: "Ask"; readonly asked: Asked } | { readonly type: "Turn"; readonly go: () => void }

  // How long the queue is served before what came meanwhile is let in, so a page's question that arrived during the
  // shell's is chosen before the shell's next.
  const SLICE_MS = 8

  // A core that did not load fails every question. The queue is served in a later task, so what arrives in the tasks
  // between is chosen from.
  export const listen = (port: Port<Asked, Answered>, ready: Promise<Answering>, later: (f: () => void) => void = f => void setTimeout(f, 0)): void => {
    let queue = lanes.empty<Item>()
    let answering: { readonly type: "Loading" } | { readonly type: "Ready"; readonly answer: Answering } = { type: "Loading" }
    let serving: "Idle" | "Scheduled" = "Idle"

    const schedule = (): void => {
      if (answering.type === "Loading" || serving === "Scheduled") return
      serving = "Scheduled"
      later(serve)
    }
    const turn = (lane: lanes.Lane): Turn => () =>
      lanes.free(queue, lane)
        ? Promise.resolve()
        : new Promise(go => {
            queue = lanes.arrived(queue, lane, { type: "Turn", go })
            schedule()
          })
    const settle = (asked: Asked, answer: lo.Settled<Answer<Kind>>): void => {
      queue = lanes.ended(queue, asked.lane)
      port.postMessage({ id: asked.id, answer })
      schedule()
    }
    // An answer that waited is sent on its lane's turn, so the page hears the page's answer first.
    const start = (answer: Answering, asked: Asked): void => {
      queue = lanes.started(queue, asked.lane)
      const again = turn(asked.lane)
      const settled = ((): lo.Settled<Answer<Kind>> | Promise<lo.Settled<Answer<Kind>>> => {
        try {
          const v = answer(asked.question, again)
          return v instanceof Promise ? v.then(lo.val, (x: unknown) => lo.error(lo.reason(x))) : lo.val(v)
        } catch (x) {
          return lo.error(lo.reason(x))
        }
      })()
      if (settled instanceof Promise) void settled.then(s => again().then(() => settle(asked, s)))
      else settle(asked, settled)
    }
    const serve = (): void => {
      serving = "Idle"
      if (answering.type === "Loading") return
      const { answer } = answering
      const from = performance.now()
      for (let n = lanes.next(queue); n.type === "Serve"; n = lanes.next(queue)) {
        queue = n.q
        if (n.a.type === "Turn") n.a.go()
        else start(answer, n.a.asked)
        if (performance.now() - from > SLICE_MS) return schedule()
      }
    }

    port.addEventListener("message", e => {
      queue = lanes.arrived(queue, e.data.lane, { type: "Ask", asked: e.data })
      schedule()
    })
    void ready
      .then(
        answer => answer,
        (x: unknown): Answering => () => {
          throw x
        },
      )
      .then(answer => {
        answering = { type: "Ready", answer }
        schedule()
      })
  }
}
export default questions
