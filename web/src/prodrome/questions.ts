// The questions the tab asks the core in its worker, and `serve`, which answers them. A question names a chain, an
// instant, a reading or terms, never objects; under test the same `serve` runs in this thread.

import type {
  ConversationPayload,
  ConversationsPayload,
  FoldPayload,
  ProposalRow,
  InboxProposalsPayload,
  MemoryPayload,
  SearchPayload,
  SpecNode,
  TurnRow,
} from "../wire/generated/wire"
import type { Dressed, Streamed, UnsentRead, WindowAbout, Windowed as PageWindow } from "../wire/generated/pages"
import type { AnsweredBlock, Chains, Core, CoreEnv, CoreFold, Linker, CorePlaced, Beside, CoreSpot, Order, Standing, Todos, Window } from "./core"
import { grow } from "./core"
import {
  barOf,
  chosenOf,
  conversationOf,
  excludedOf,
  foldOf,
  functionsOf,
  holdIn,
  inboxOf,
  lackingIn,
  linesLeft,
  linked,
  linker,
  linkerOf,
  listOf,
  listOfLines,
  orderIn,
  policyOf,
  unsentIn,
  searchOf,
  streamOf,
  talksOf,
  within,
} from "./core"
import chainsync from "./chainsync"
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
import { measure } from "../app/perf"
import text from "../wire/text"

export namespace questions {
  // A chain's fold as the core reads what stands of it: none before a pull of it landed; `settled` while the box holds
  // all of it and nothing it holds is this tab's alone.
  export type Listed = { readonly type: "Unheld" } | { readonly type: "Read"; readonly fold: FoldPayload; readonly settled: boolean }

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

  // `Absent` once a whole pull says the box holds no line of it either; `Unheard` until then; `Refused` where it is a
  // topic this device began and the box refused, in the box's words.
  export type Unheard = { readonly type: "Unheard" }
  export type Unheld = { readonly type: "Absent" } | Unheard | { readonly type: "Refused"; readonly why: string }
  const ABSENT: Unheld = { type: "Absent" }
  const UNHEARD: Unheard = { type: "Unheard" }

  export type One = Unheld | { readonly type: "Held"; readonly payload: ConversationPayload } | NotHere

  // A reading as its page parses it, dressed with the HTML the device holds of its model blocks and the proposals as
  // the box drew them (`Drawn`, which the page holds and hands over); beside it, the blocks it holds no HTML of
  // (`missing`), the proposals not drawn (`drawing`), and how many rows read the memory's words (`receipts`), which the
  // page reads from the box and asks with again.
  export type Worded<T> = { readonly text: text.Text<T>; readonly missing: readonly AnsweredBlock[] }
  export type Windowed = Worded<PageWindow> & { readonly drawing: readonly ProposalRow[]; readonly receipts: number }
  // A proposal as the box drew it, and the print of the row it drew it from: kept while that row stands.
  export type Drawn = { readonly from: string; readonly row: ProposalRow }

  export type Grouped = Unheld | ({ readonly type: "Held" } & Windowed) | NotHere

  // `null` is ∅.
  export type Priced = { readonly type: "Value"; readonly v: number | null } | { readonly type: "Refused"; readonly why: string }

  // The list's tree of the todos it places: each node where it stands, its term and that term's price.
  export type Placed = CorePlaced
  export type Line = { readonly place: CoreSpot; readonly term: SpecNode; readonly priced: Priced; readonly children: readonly Line[] }

  export type Table = {
    // The chains the box serves: each is held, and pulled with the open one's exchange.
    Served: { q: { readonly chains: readonly string[] }; a: null }
    // The box's `GET …/fold`, from what stands.
    List: { q: { readonly chain: string; readonly at: string | null }; a: Listed }
    // Sealed as `actor`'s, kept, and handed over behind the answer: the fold after it.
    Act: { q: { readonly chain: string; readonly event: string; readonly at: string; readonly actor: string }; a: Listed }
    // What the tab wrote that the box has not taken.
    Unsent: { q: { readonly chain: string }; a: UnsentRead }
    // What the tab wrote handed over, then the chain pulled, and the others served after it; whether its pull ended whole.
    Exchange: { q: { readonly chain: string }; a: boolean }
    // The unsent writes `shown` let go and the chain taken again; what the reset could not do.
    ResetChain: { q: { readonly chain: string; readonly shown: readonly string[] }; a: readonly string[] }
    // Of the heads the box names after `was`, those this tab did not make.
    Elsewhere: { q: { readonly chain: string; readonly was: heads.Heads; readonly now: heads.Heads }; a: readonly string[] }
    // How the heads `a` stand to `b` over what the chain holds.
    Order: { q: { readonly chain: string; readonly a: readonly string[]; readonly b: readonly string[] }; a: Order }
    // The root a device key counts under, none before the box named one.
    Root: { q: { readonly chain: string }; a: string | null }
    // What a trusted device signs to vouch for `key` as `actor` (SPEC law 42).
    KeyAdded: { q: { readonly chain: string; readonly actor: string; readonly key: string }; a: string }
    // Kept while the set and the instant stand.
    Reading: { q: { readonly chain: string; readonly at: string }; a: text.Text<{ readonly held: readonly string[]; readonly read: Read }> }
    Price: { q: { readonly basis: Basis; readonly terms: readonly SpecNode[] }; a: readonly Priced[] }
    Tree: { q: { readonly basis: Basis; readonly members: readonly Placed[]; readonly by: string }; a: text.Text<Line> }
    Thirty: { q: { readonly chain: string; readonly id: string; readonly ms: number }; a: text.Text<thirty.Thirty> }
    // None where the box names no root.
    Standing: { q: { readonly chain: string; readonly actor: string; readonly key: string }; a: Standing | null }
    // Pulls every store now and after every word of the box's stream; true when every pull ended whole.
    Pull: { q: Record<never, never>; a: boolean }
    Heard: { q: { readonly store: stores.Name; readonly heads: heads.Heads }; a: null }
    // The box's stream reopened: pulls what a reading asked for.
    Opened: { q: Record<never, never>; a: null }
    // A prefetch waits while no page is shown.
    Shown: { q: { readonly on: boolean }; a: null }
    // Answers with what the reset could not do.
    Reset: { q: { readonly store: stores.Name }; a: readonly string[] }
    // Says what it was priced without as the inbox's reader.
    Proposals: { q: { readonly at: string; readonly memory: MemoryPayload | null }; a: text.Text<{ readonly payload: InboxProposalsPayload; readonly held: readonly string[] }> }
    // Every conversation the skeleton holds, each row as far as the device holds its words, asked for behind it.
    Conversations: { q: Record<never, never>; a: ConversationsPayload }
    Conversation: { q: { readonly id: string; readonly window: Window; readonly memory: MemoryPayload | null }; a: One }
    // Whether the conversation, as this device holds it whole, has a turn about `about`; nothing is fetched or pulled for
    // it, so a conversation the device holds no whole copy of has said nothing yet. The read is the conversation's
    // memoized transcript and one pass over its turns.
    Talk: { q: { readonly id: string; readonly about: string }; a: "Quiet" | "Talking" }
    // Read back to the turn it must reach (`grouped.covering`), grouped into rounds on the owner's days in `zone`.
    Window: {
      q: { readonly id: string; readonly window: grouped.Asked; readonly memory: MemoryPayload | null; readonly drawn: readonly Drawn[]; readonly zone: time.Zone }
      a: Grouped
    }
    // Over the conversations whose every line the device holds, saying how many of how many; the rest asked for behind
    // it.
    Search: { q: { readonly q: string }; a: SearchPayload }
    Left: { q: Record<never, never>; a: excluded.T }
    // The device's rendered blocks: the worker alone reads and writes them.
    Blocks: { q: { readonly keys: readonly string[] }; a: (string | null)[] }
    KeepBlocks: { q: { readonly pairs: readonly (readonly [string, string])[] }; a: string[] }
    BlocksUsage: { q: Record<never, never>; a: rendered.Usage }
    // Answered once the lines of the conversations the rounds are in are held.
    Stream: { q: { readonly chosen: readonly string[]; readonly pages: number }; a: Whole<Worded<Dressed<Streamed>>> }
    // A refusal fails in the box's words.
    Topic: { q: { readonly write: topics.Write; readonly at: string; readonly actor: string }; a: null }
    // Sealed as this tab's line, read at once and handed over behind the answer: its name and its conversation.
    Line: { q: { readonly line: lines.Sealing; readonly at: string; readonly actor: string }; a: lines.Sealed }
    Acted: { q: { readonly lines: readonly string[] }; a: null }
    Refused: { q: Readonly<Record<never, never>>; a: string }
    // The copies this device keeps from before the last change of form: the stores it writes and the chains, by name.
    Earlier: { q: Readonly<Record<never, never>>; a: readonly string[] }
  }
  export type Kind = keyof Table
  export type Question = { [k in Kind]: { readonly type: k } & Table[k]["q"] }[Kind]
  export type Of<K extends Kind> = Extract<Question, { readonly type: K }>
  export type Answer<K extends Kind> = Table[K]["a"]

  export type Asked = { readonly id: number; readonly lane: lanes.Lane; readonly question: Question }
  export type Answered = { readonly id: number; readonly answer: lo.Settled<Answer<Kind>> }

  // Said unasked. `Gone`: the box refused this device's key on a worker read; `Stale`: refused this build's contract;
  // `Unloaded`: the core did not load.
  export type Said = storeset.Said | chainsync.Said | { readonly type: "Gone" } | { readonly type: "Stale" } | { readonly type: "Unloaded"; readonly why: string }
  export type Out = Answered | { readonly said: Said }

  // A proposal the box draws against its chain, whose card counts what it draws: its whole row is the box's.
  const drawnByTheBox = (p: ProposalRow): boolean =>
    p.act.kind === "changes" || p.act.kind === "secret" || p.act.kind === "connect" || p.coding?.state === "reported"

  // The rows of a window that read the memory's words: a remembered verb, and a card that remembers or consolidates.
  const receiptsOf = (w: ConversationPayload): number =>
    w.turns.filter(t => t.who === "verb" && t.outcome.is === "remembered").length +
    w.proposals.filter(p => p.act.kind === "remember" || p.act.kind === "consolidate").length

  // The page's reading, and the one it replaced while a page still reads it.
  const KEPT = 4

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
    const chains = chainsync.of(core, {
      told: (reader, left) => say({ type: "Told", reader, left }),
      said: say,
      refused: why => say({ type: "Refused", why }),
    })
    const set: Chains = new core.Chains()
    const inSet = new Map<string, number>()
    const readings: Reading[] = []

    // Each chain's reading of what stands, made once per move of it.
    const todos = new Map<string, { readonly version: number; readonly t: Todos }>()
    const todosOf = (c: chainsync.Chain): Todos => {
      const known = todos.get(c.name)
      if (known?.version === c.store.version()) return known.t
      known?.t.free()
      const t = c.store.todos()
      todos.set(c.name, { version: c.store.version(), t })
      return t
    }
    // Who a chain does not believe, as the box last said for its rows; a chain it has not said that of is read nowhere,
    // never as one that believes everyone.
    const heldUnder = (c: chainsync.Chain): readonly string[] | null => (c.sync.sync.landed() ? (policyOf(c.store)?.untrusted ?? null) : null)
    // Every chain held, restored, before the set is read.
    const allReady = (): Promise<readonly chainsync.Chain[]> => Promise.all(chains.held().map(c => chains.ready(c.name)))
    const setNow = (): string => {
      for (const c of chains.held()) {
        if (inSet.get(c.name) === c.store.version()) continue
        say({ type: "Told", reader: `chain/${c.name}`, left: holdIn(set, c.name, c.store) })
        inSet.set(c.name, c.store.version())
      }
      return chains
        .held()
        .map(c => `${c.name}:${c.store.version()}`)
        .sort()
        .join(";")
    }
    // The chains a reading is priced through: each that holds what a pull landed.
    const heldNames = (): string[] => chains.held().flatMap(c => (heldUnder(c) === null ? [] : [c.name]))
    // A chain restored with every other, so what it refers to is read too.
    const chain = (name: string): Promise<chainsync.Chain> => chains.ready(name).then(c => allReady().then(() => c))
    // Newest last; a reading is found by its name, never by a print of it.
    const kept = (name: Name): Reading | undefined => readings.find(r => same(r.answer.reading, name))
    const keep = (r: Reading): void => {
      readings.push(r)
      for (const old of readings.splice(0, Math.max(0, readings.length - KEPT))) old.set.free()
    }

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
    // The conversations whose rows the last list drew with their words.
    let drawn: ReadonlySet<string> = new Set()
    // Read once what the device keeps of `scope` is held, never waiting on the box.
    const local = <A>(turn: Turn, scope: lines.Scope, read: () => A): Promise<A> =>
      ready("All", turn)
        .then(() => sync.asked(() => scope))
        .then(turn)
        .then(read)
    // Where the lines hold none of `id`, the device may not have pulled it yet; only a whole pull says the box holds none.
    const conversation = async <A>(id: string, turn: Turn, read: () => Promise<A> | A): Promise<A | Unheld> => {
      await ready("All", turn)
      if (sync.holds(id)) return read()
      // A held conversation is never read as refused; a refused topic is said before the box is asked of it.
      const refused = (): Unheld | null => {
        const why = held.refused_topic(id)
        return why === undefined ? null : { type: "Refused", why }
      }
      const first = refused()
      if (first !== null) return first
      const pulled = await stores.pull()
      await turn()
      return sync.holds(id) ? read() : (refused() ?? (pulled ? ABSENT : UNHEARD))
    }

    // Read again only once the inbox, the chains held or the memory moved; until then an ask gets the first reading,
    // whose `at` names its instant.
    const inbox = last.of<{ readonly version: number; readonly set: string; readonly memory: string }, Beside<InboxProposalsPayload, InboxProposalsPayload["failed"]>>(
      (a, b) => a.version === b.version && a.set === b.set && a.memory === b.memory,
    )

    const blocksOf = (conversation: string, turns: readonly TurnRow[]): AnsweredBlock[] =>
      turns.flatMap(t => (t.who === "model" ? t.blocks.map(b => ({ conversation, source: b.source, key: core.answer_key(b.source) })) : []))

    // The reading set whole beside the HTML the device holds of its blocks and the proposals drawn; the blocks it lacks.
    const dressing = async <T>(read: text.Text<T>, bs: readonly AnsweredBlock[], drawn: readonly ProposalRow[]): Promise<Worded<Dressed<T>>> => {
      const got = bs.length === 0 ? [] : await blocks.get(bs.map(b => b.key))
      const html = Object.fromEntries(bs.flatMap((b, i) => { const h = got[i]; return h === null || h === undefined ? [] : [[b.source, h]] }))
      const missing = bs.filter((_, i) => got[i] === null || got[i] === undefined)
      return { text: text.object<Dressed<T>>({ read, html: text.printed(html), drawn: text.printed(drawn) }), missing }
    }

    const listed = (c: chainsync.Chain, at: string | null): Listed => {
      const untrusted = heldUnder(c)
      if (untrusted === null) return { type: "Unheld" }
      setNow()
      return { type: "Read", fold: listOf(todosOf(c), at, untrusted, c.name, set), settled: c.store.settled() }
    }
    const reading = (c: chainsync.Chain, at: string): Read => {
      const name: Name = { chain: c.name, set: setNow(), at }
      const known = kept(name)
      if (known !== undefined) return known.answer
      const untrusted = heldUnder(c)
      if (untrusted === null) return { type: "Unheld" }
      const f = foldOf(todosOf(c), at, untrusted)
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
        case "Served":
          chains.serve(q.chains)
          return null
        case "List":
          return chain(q.chain).then(c => listed(c, q.at))
        case "Act":
          // Decided when the act is kept: a reading that fails after it answers that the fold follows with the chain's
          // next move, never that nothing was recorded.
          return chains.act(q.chain, q.event, q.actor).then(c =>
            allReady()
              .then(() => listed(c, q.at))
              .catch((): Listed => ({ type: "Unheld" })),
          )
        case "Unsent":
          return chains.ready(q.chain).then(c => unsentIn(c.store))
        case "Exchange":
          return chains.exchange(q.chain)
        case "ResetChain":
          return chains.reset(q.chain, new Set(q.shown))
        case "Elsewhere":
          return chains.elsewhere(q.chain, q.was, q.now)
        case "Order":
          return chains.ready(q.chain).then(c => orderIn(c.store, q.a, q.b))
        case "Root":
          return chains.ready(q.chain).then(c => policyOf(c.store)?.root ?? null)
        case "KeyAdded":
          return chains.ready(q.chain).then(c => {
            if (heldUnder(c) === null) throw new Error("this device holds no copy of the first chain yet, so it cannot vouch")
            return todosOf(c).key_added(q.actor, q.key)
          })
        case "Reading":
          return chain(q.chain).then(c => text.object({ read: text.printed(reading(c, q.at)), held: text.printed(heldNames()) }))
        case "Price":
          return price(q.basis, q.terms)
        case "Tree": {
          // The core grows it, so the pages' module links no term and no trie; each term priced as `Price` prices it.
          const grown = grow(core, q.members, q.by)
          const prices = (g: ReturnType<typeof grow>): Line => ({ place: g.place, term: g.term, priced: price(q.basis, [g.term])[0] ?? { type: "Refused", why: "not priced" }, children: g.children.map(prices) })
          return text.printed(prices(grown))
        }
        case "Thirty":
          return chain(q.chain).then(c => {
            setNow()
            const untrusted = heldUnder(c)
            return text.printed(untrusted === null ? thirty.unheld : thirty.sample(core, set, todosOf(c), untrusted, q.id, q.ms))
          })
        case "Standing":
          return chains.standing(q.chain, q.actor, q.key)
        case "Pull":
          return stores.pull()
        case "Heard":
          stores.heard(q.store, q.heads)
          return null
        case "Opened":
          stores.opened()
          return null
        case "Shown":
          return Promise.all([stores.shown(q.on), chains.shown(q.on)]).then(() => null)
        case "Reset":
          return stores.reset(q.store)
        case "Proposals":
          return Promise.all([ready("Inbox", turn), allReady()]).then(() => {
            const of = proposals()
            const read = inbox({ version: of.version(), set: setNow(), memory: JSON.stringify(q.memory) }, () => inboxOf(of, q.at, q.memory, set))
            say({ type: "Told", reader: "inbox", left: excluded.left(excluded.NONE, read.said) })
            return text.object({ payload: read.text, held: text.printed(heldNames()) })
          })
        case "Conversations":
          return local(turn, lines.TITLES, () => listOfLines(held, proposals())).then(async (now): Promise<ConversationsPayload> => {
            // A row once drawn with its words keeps them: where a new line it reads is not here yet, the answer waits
            // for that row's own lines (or for their fetch to fail), and for no other row's.
            let read = now
            for (let asked = ""; ; ) {
              const fell = new Set(read.conversations.flatMap(c => (c.words.held !== "read" && drawn.has(c.id) ? [c.id] : [])))
              const need = fell.size === 0 ? [] : lackingIn(held, lines.TITLES).filter(n => n.header.node === "line" && fell.has(n.header.conversation))
              const names = need.map(n => n.hash).join(",")
              if (need.length === 0 || names === asked) break
              asked = names
              const came = await sync.fetched(need).then(
                () => true,
                () => false,
              )
              await turn()
              read = listOfLines(held, proposals())
              if (!came) break
            }
            drawn = new Set(read.conversations.flatMap(c => (c.words.held === "read" ? [c.id] : [])))
            return read
          })
        case "Conversation": {
          const { id, window, memory } = q
          return conversation(id, turn, () => sync.over(() => lines.one(id), (): One => ({ type: "Held", payload: conversationOf(held, proposals(), id, window, memory) })))
            .catch((e: unknown): One => notHere(lines.one(id), e))
        }
        case "Talk": {
          const { id, about } = q
          return sync.engine.sync.restored
            .get()
            .then(turn)
            .then((): "Quiet" | "Talking" =>
              sync.holds(id) && sync.lacking(lines.one(id)) === 0 && talksOf(held, id, about) ? "Talking" : "Quiet",
            )
        }
        case "Window": {
          const { id, window, memory, drawn, zone } = q
          const read = (w: Window): ConversationPayload => conversationOf(held, proposals(), id, w, memory)
          const given = new Map(drawn.map(d => [d.from, d.row]))
          // Asked about a reference no round held touches, the window is the whole conversation, and says whether a pull
          // has ended whole since (`WindowAbout`): a reading of the lines held now, asked again as they move.
          const { about, ...whole } = window
          const windowed = (): { readonly said: WindowAbout; readonly payload: grouped.Window } => {
            const said: WindowAbout = about === undefined || talksOf(held, id, about) ? "Asked" : sync.whole() ? "Quiet" : "Unheard"
            return { said, payload: grouped.of(grouped.covering(said === "Asked" ? window : whole, read), zone) }
          }
          return conversation(id, turn, () => sync.over(() => lines.one(id), windowed))
            .then(async (w): Promise<Grouped> => {
              if ("type" in w) return w
              const { said, payload } = w
              const drawing = payload.proposals.filter(drawnByTheBox)
              const rows = drawing.flatMap(p => given.get(JSON.stringify(p)) ?? [])
              const dressed = await dressing(text.printed(payload), blocksOf(id, payload.turns), rows)
              return {
                type: "Held",
                text: text.object<PageWindow & { readonly type: "Read" }>({ type: text.printed("Read"), payload: dressed.text, about: text.printed(said) }),
                missing: dressed.missing,
                drawing: drawing.filter(p => !given.has(JSON.stringify(p))),
                receipts: receiptsOf(payload),
              }
            })
            .catch((e: unknown): Grouped => notHere(lines.one(id), e))
        }
        case "Search":
          return local(turn, lines.WHOLE, () => searchOf(held, q.q, sync.whole()))
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
            // Measured in the worker as `stream:read`, from its words held to its answer.
            () => {
              phases.mark("stream content held")
              const start = performance.now()
              const read = stores.topics()
              const stream = streamOf(held, read.reading, q.chosen, q.pages)
              phases.mark("stream drawn")
              const answer = { read: text.object<Streamed>({ stream: stream.text, bar: barOf(read.reading) }), blocks: stream.said }
              measure("stream:read", start)
              return answer
            },
          ).then(async (w): Promise<Whole<Worded<Dressed<Streamed>>>> => (w.type === "Held" ? { type: "Held", v: await dressing(w.v.read, w.v.blocks, []) } : w))
        case "Topic":
          return stores.write(q.write, q.at, q.actor).then(() => null)
        case "Line":
          return stores.seal(q.line, q.at, q.actor)
        case "Acted":
          return stores.acted(q.lines).then(() => null)
        case "Refused":
          return Promise.resolve(stores.refused())
        case "Earlier":
          return Promise.all([stores.earlier(), Promise.all(chains.held().map(c => c.sync.earlier().then(e => (e ? [`chain/${c.name}`] : []))))]).then(
            ([written, chained]) => [...written, ...chained.flat()],
          )
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
