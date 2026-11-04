// The Todos page's step: one test per event, the fold law over random walks, and the tree kept in step with its inputs.

import { test } from "node:test"
import assert from "node:assert/strict"

import type { Entry, SpecNode } from "../src/wire/generated/wire"
import { ANY, DEFAULT_FILTER, is } from "../src/todo/todos"
import type { Filter } from "../src/todo/todos"
import onscreen from "../src/app/store"
import todosState from "../src/pages/todos.state"
import frame from "../src/view/frame"
import lo from "../src/algebra/lo"
import tree from "../src/algebra/tree"
import row from "../src/todo/row"
import grouping from "../src/todo/grouping"
import type questions from "../src/prodrome/questions"
import { forAll, rng, type Rng } from "./gen"

// The page's own step, lifted by the frame as it runs on the page.
const lifted = frame.step(todosState.step)
const step = (p: lo.T<todosState.T>, e: Event): lo.T<todosState.T> => lifted(p, frame.own(e))
const initial = (...a: Parameters<typeof todosState.initial>): lo.T<todosState.T> => lo.val(todosState.initial(...a))
type Event = todosState.Event

const entry = (todo: string, value: number | null, category: string): Entry => ({
  todo,
  state: "open",
  states: ["open"],
  at: "",
  claimed: "",
  value,
  unconfirmed: false,
  recurring: false,
  conflicts: {},
  candidates: [],
  stream: [],
  authored: {
    rich: { body: null, detail: null, rationale: null, waiting_on: null, keys: { body: null, detail: null, rationale: null, waiting_on: null } },
    block: { kind: "todo", name: "todo" },
    created: "2026-09-01",
    body: todo,
    text: todo,
    detail: "",
    category,
    waiting_on: "",
    rationale: [],
    spec: null,
    actor: "ada",
    at: "2026-09-01T00:00:00",
    source: null,
    subtodos: [],
  },
})

const storeState = (r: Rng): onscreen.Fold =>
  r.bool()
    ? onscreen.UNLOADED
    : {
        type: "Shown",
        entries: r.array(r.int(5), () => entry(r.string("abcdef", 3), r.bool(0.8) ? r.int(101) / 100 : null, r.pick(["work", "home"]))),
        at: "2026-09-02T12:00:00",
        by: { type: "Local", heads: [r.string("0123456789abcdef", 8)], next: null, fresh: r.bool() },
      }

const filter = (r: Rng): Filter => ({
  state: r.pick(["open", "done", "all"] as const),
  category: r.bool() ? ANY : is(r.pick(["work", "home"])),
  kind: ANY,
  query: r.string("ab ", r.int(3)),
  attention: false,
  waiting: false,
  dated: false,
})

const READING: questions.Name = { chain: "life", set: "life:1", at: "2026-09-02T12:00:00" }

const basis = (reading: questions.Name = READING): todosState.Basis => ({
  type: "Folded",
  reading,
  at: "2026-09-02T12:00:00",
  specs: {},
  waits: () => null,
})

// What the tree wants, or nothing.
const wanted = (s: todosState.T): { of: todosState.Of; terms: readonly SpecNode[] } =>
  s.wants.type === "None" ? { of: { type: "Rows", at: "" }, terms: [] } : { of: s.wants.of, terms: s.wants.terms }

// The worker's answer to what the tree wants, every term priced `v`.
const answered = (s: todosState.T, v: number): Event => ({
  type: "Priced",
  ...wanted(s),
  prices: wanted(s).terms.map(() => ({ type: "Value", v })),
})

const event = (r: Rng): Event =>
  r.pick<() => Event>([
    () => ({ type: "Store", state: storeState(r) }),
    () => ({ type: "Filter", filter: filter(r) }),
    () => ({ type: "By", by: r.pick(["category", "urgency", "flat"] as const) }),
    () => ({ type: "Find", find: r.pick([todosState.shut, todosState.opened]) }),
    () => ({ type: "ToggleFind" }),
    () => ({ type: "Fold", key: r.pick(["c:work", "c:home", "t:abc"]), open: r.bool() }),
    () => ({ type: "Basis", basis: basis() }),
    () => ({ type: "Select", id: r.bool() ? "" : r.string("abc", 3) }),
    () => ({ type: "Open", id: r.bool() ? "" : r.string("abc", 3) }),
    () => ({ type: "Cursor", key: r.bool() ? "" : r.string("abc/", 5) }),
    () => ({ type: "Shared", body: r.bool() ? "" : r.string("ab ", 4) }),
    () => ({ type: "Briefing", briefing: r.pick([{ type: "None" as const }, { type: "Today" as const, date: "2026-09-02" }]) }),
    () => ({ type: "Problem", reason: r.bool() ? null : "the box did not answer", ofReading: r.bool() }),
    () => ({ type: "Waiting", count: r.int(4) }),
    () => ({ type: "Pending", todos: new Map([[r.string("abc", 3), r.pick(["Sending", "Refused", "AwaitingTrust"] as const)]]) }),
    () => ({ type: "Poke" }),
  ])()

const START = initial(onscreen.UNLOADED, DEFAULT_FILTER, "category", {}, "", "")
const value = (p: typeof START) => {
  const v = lo.withDefault(p, null)
  if (v === null) throw new Error("this page is always loaded")
  return v
}

test("store: replaces the store, and supersedes any earlier refusal", () => {
  const refused = step(START, { type: "Problem", reason: "the box did not answer", ofReading: false })
  const s = storeState(rng(7))
  const next = value(step(refused, { type: "Store", state: s }))
  assert.equal(next.store, s)
  assert.equal(next.problem, null)
})

const SHOWN: onscreen.Fold = {
  type: "Shown",
  at: "2026-09-02T12:00:00",
  entries: [entry("aaa", 0.3, "work"), entry("bbb", 0.9, "home"), entry("ccc", 0.8, "home")],
  by: { type: "Box", heads: [], fetchedAt: 1, next: null, fresh: true },
}

// What a tree shows: each node's key and number, in the order drawn.
const shown = (t: tree.Tree<grouping.Node> | null): [string, number | null][] =>
  t === null ? [] : tree.nodes(t).map(n => [grouping.place.key(n.place), grouping.worth.toNumber(n.worth)])

test("the rows draw from the box's fold before any reading: a todo is its row's value, and only its groups are asked of the core", () => {
  const stored = value(step(START, { type: "Store", state: SHOWN }))
  assert.equal(stored.grown, null, "nothing is drawn until every node has a number")
  assert.equal(stored.basis.type, "Rows")
  assert.ok(wanted(stored).terms.length > 0 && wanted(stored).terms.every(t => t.kind !== "ref"), "a row's own value is never asked for")
  assert.deepEqual(stored.wants.type === "Wanting" && stored.wants.basis.type === "Rows" ? stored.wants.basis.functions : null, {
    aaa: { kind: "flat", value: 0.3 },
    bbb: { kind: "flat", value: 0.9 },
    ccc: { kind: "flat", value: 0.8 },
  })
  const drawn = value(step(lo.val(stored), answered(stored, 0.85)))
  assert.deepEqual(shown(drawn.grown).filter(([k]) => k.startsWith("t:")), [["t:aaa", 0.3], ["t:ccc", 0.8], ["t:bbb", 0.9]])
  assert.equal(drawn.wants.type, "None")
})

test("a reading landing keeps the rows drawn until it is priced, then draws the same keys in the same order", () => {
  const stored = value(step(START, { type: "Store", state: SHOWN }))
  const rows = value(step(lo.val(stored), answered(stored, 0.85)))
  const read = value(step(lo.val(rows), { type: "Basis", basis: basis() }))
  assert.equal(read.grown, rows.grown, "the rows stand while the reading is priced")
  assert.deepEqual(wanted(read).of, { type: "Fold", reading: READING })
  // The reading prices each todo as its row did, so nothing moves.
  const values = new Map([["aaa", 0.3], ["bbb", 0.9], ["ccc", 0.8]])
  const prices = wanted(read).terms.map(t => ({ type: "Value" as const, v: t.kind === "ref" ? (values.get(String(t["todo"])) ?? 0) : 0.85 }))
  const folded = value(step(lo.val(read), { type: "Priced", ...wanted(read), prices }))
  assert.deepEqual(shown(folded.grown), shown(rows.grown))
  assert.notEqual(folded.grown, rows.grown, "drawn from the reading")
})

test("an answer about a basis since replaced is dropped", () => {
  const stored = value(step(START, { type: "Store", state: SHOWN }))
  const late = answered(stored, 0.5)
  const read = step(lo.val(stored), { type: "Basis", basis: basis() })
  assert.equal(step(read, late), read)
})

test("a reading of another chain at the same instant is another basis: its answer is not this one's", () => {
  const stored = value(step(START, { type: "Store", state: SHOWN }))
  const read = value(step(lo.val(stored), { type: "Basis", basis: basis() }))
  const other: Event = { type: "Priced", of: { type: "Fold", reading: { ...READING, chain: "work" } }, terms: wanted(read).terms, prices: wanted(read).terms.map(() => ({ type: "Value", v: 0.5 })) }
  assert.equal(value(step(lo.val(read), other)), read)
})

// The page's effect against a worker: answers land as the step folds them.
const run = async (s: todosState.T, price: Parameters<typeof todosState.ask>[1]): Promise<todosState.T> => {
  let now = s
  const dispatch = (e: Event): void => {
    now = todosState.step(now, e)
    todosState.ask(now, price, dispatch)
  }
  todosState.ask(now, price, dispatch)
  for (let i = 0; i < 10; i += 1) await Promise.resolve()
  return now
}

test("a failed worker prices what it was asked as refused: the rows draw, the groups unlinked, and the page says why", async () => {
  const stored = value(step(START, { type: "Store", state: SHOWN }))
  const failed = await run(stored, () => Promise.reject(new Error("the core's worker stopped")))
  assert.ok(failed.grown !== null, "the rows are drawn")
  // Each todo is its row's value; a group that does not link sorts first, as one did before any group was priced.
  assert.deepEqual(shown(failed.grown).filter(([k]) => k.startsWith("t:")).sort(), [["t:aaa", 0.3], ["t:bbb", 0.9], ["t:ccc", 0.8]])
  const groups = tree.nodes(failed.grown).filter(n => n.term.kind === "conj")
  assert.ok(groups.length > 0 && groups.every(n => n.worth.type === "Unlinked"), "each group of several is unlinked")
  assert.equal(failed.problem?.reason, "the core's worker stopped")
  assert.equal(failed.wants.type, "None", "and nothing is asked again")
  // A reading on a failed worker draws the same way.
  const read = await run(todosState.step(failed, { type: "Basis", basis: basis() }), () => Promise.reject(new Error("gone")))
  assert.ok(read.grown !== null && tree.nodes(read.grown).every(n => n.worth.type === "Unlinked"), "the reading's terms, refs included, are refused")
})

test("the effect asks once for what the tree wants, however many values are drawn before the answer", async () => {
  const stored = value(step(START, { type: "Store", state: SHOWN }))
  let asks = 0
  let answer: () => void = () => {}
  const price = (_: questions.Basis, terms: readonly SpecNode[]): Promise<readonly questions.Priced[]> => {
    asks += 1
    return new Promise(ok => (answer = () => ok(terms.map(() => ({ type: "Value", v: 0.5 })))))
  }
  let now = stored
  const dispatch = (e: Event): void => void (now = todosState.step(now, e))
  for (let i = 0; i < 5; i += 1) {
    todosState.ask(now, price, dispatch)
    now = todosState.step(now, { type: "Poke" })
  }
  assert.equal(asks, 1)
  assert.equal(now.wants.type, "Asking")
  answer()
  for (let i = 0; i < 5; i += 1) await Promise.resolve()
  assert.equal(now.wants.type, "None")
  assert.ok(now.grown !== null)
})

test("a todo done here and not yet taken by the box stays on the Open list, saying so, until the box takes it", () => {
  const done: Entry = { ...entry("aaa", 1, "work"), state: "completed", states: ["completed"] }
  const s: onscreen.Fold = {
    type: "Shown",
    at: "2026-09-02T12:00:00",
    entries: [done, entry("bbb", 0.4, "work")],
    by: { type: "Local", heads: [], next: null, fresh: false },
  }
  // Every tree the steps grow waits for its terms, so each is answered as the worker would.
  const settled = (p: lo.T<todosState.T>): lo.T<todosState.T> => step(p, answered(value(p), 0.5))
  const shown = settled(step(step(START, { type: "Store", state: s }), { type: "Basis", basis: { type: "Rows" } }))
  const lists = (p: lo.T<todosState.T>, todo: string): boolean => JSON.stringify(value(p).grown).includes(`"${todo}"`)
  assert.equal(lists(shown, "aaa"), false, "a done todo is not open")
  const sending = settled(step(shown, { type: "Pending", todos: new Map([["aaa", "Sending"]]) }))
  assert.equal(lists(sending, "aaa"), true)
  // Its card stays open though the act made the card's value Fine, unless the viewer folded it.
  assert.equal(todosState.shownFolds(value(sending))["c:work"], true)
  assert.equal(todosState.shownFolds(value(step(sending, { type: "Fold", key: "c:work", open: false })))["c:work"], false)
  assert.deepEqual(todosState.shownFolds(value(shown)), value(shown).folds, "with nothing on its way, the viewer's folds alone")
  assert.equal(row.said("Sending", done), "done · sending")
  assert.equal(row.said("Sending", entry("bbb", 0.4, "work")), "sending")
  assert.equal(lists(settled(step(sending, { type: "Pending", todos: new Map() })), "aaa"), false, "taken, it leaves the list")
})

test("a refused act and one waiting for this device's trust stay on the list saying so, never as sending", () => {
  const done: Entry = { ...entry("aaa", 1, "work"), state: "completed", states: ["completed"] }
  assert.equal(row.said("Refused", done), "refused: open it")
  assert.equal(row.said("AwaitingTrust", done), "waits for this device to be trusted")
  const s: onscreen.Fold = { type: "Shown", at: "2026-09-02T12:00:00", entries: [done], by: { type: "Local", heads: [], next: null, fresh: false } }
  const shown = step(step(START, { type: "Store", state: s }), { type: "Basis", basis: { type: "Rows" } })
  for (const act of ["Refused", "AwaitingTrust"] as const)
    assert.ok(JSON.stringify(value(step(shown, { type: "Pending", todos: new Map([["aaa", act]]) })).grown).includes('"aaa"'), act)
})

test("a fold, a selection, the cursor and a row opened in place: each set once, a repeat a no-op", () => {
  const cases: Event[] = [
    { type: "Fold", key: "c:work", open: false },
    { type: "Select", id: "aaa" },
    { type: "Open", id: "aaa" },
    { type: "Cursor", key: "c:work/t:aaa" },
    { type: "By", by: "flat" },
    { type: "Find", find: todosState.opened },
    { type: "Waiting", count: 2 },
  ]
  for (const e of cases) {
    const once = step(START, e)
    assert.notEqual(once, START, e.type)
    assert.equal(step(once, e), once, `${e.type} twice is ${e.type} once`)
  }
})

test("a share is counted, so the same text shared twice is two shares, and an empty one is none", () => {
  const withPrefill = initial(onscreen.UNLOADED, DEFAULT_FILTER, "category", {}, "", "a shared link")
  assert.deepEqual(value(withPrefill).share, { body: "a shared link", n: 1 })
  const again = step(withPrefill, { type: "Shared", body: "a shared link" })
  assert.deepEqual(value(again).share, { body: "a shared link", n: 2 })
  assert.equal(step(again, { type: "Shared", body: "" }), again)
  assert.deepEqual(value(START).share, { body: "", n: 0 })
})

test("the search, the filters and the grouping start shut, and the magnifier opens and shuts them", () => {
  assert.deepEqual(value(START).find, todosState.shut)
  const opened = step(START, { type: "ToggleFind" })
  assert.deepEqual(value(opened).find, todosState.opened)
  assert.deepEqual(value(step(opened, { type: "ToggleFind" })).find, todosState.shut)
  assert.deepEqual(value(step(opened, { type: "Find", find: todosState.opened })).find, todosState.opened, "the keyboard's / opens, never shuts")
})

test("poke always commits, so a late actor still redraws", () => {
  assert.equal(value(step(step(START, { type: "Poke" }), { type: "Poke" })).poke, 2)
})

test("law: the state after any walk is the fold of its events, never leaves loaded, and its tree is its inputs' tree", () => {
  forAll(200, r => {
    const events = r.array(r.int(30), () => event(r))
    let state = START
    const walked: Event[] = []
    for (const e of events) {
      // Now and then the worker answers what the tree wants.
      const next = r.bool(0.3) ? answered(value(state), r.int(101) / 100) : e
      walked.push(next)
      state = step(state, next)
      assert.equal(state.type, "Val")
      const v = value(state)
      assert.deepEqual(todosState.regrown(v), v, "growing again moves nothing")
    }
    assert.deepEqual(state, walked.reduce(step, START))
  })
})

test("law: a reading's basis clears a problem from a reading, and none about no reading", () =>
  forAll(200, (r, seed) => {
    const s = r.array(r.int(12), () => event(r)).reduce(todosState.step, todosState.initial(storeState(r), filter(r), "category", {}, "", ""))
    const failed = todosState.step(s, { type: "Problem", reason: "the reading failed", ofReading: true })
    assert.equal(todosState.step(failed, { type: "Basis", basis: basis() }).problem, null, `seed ${seed}`)
    const box = todosState.step(s, { type: "Problem", reason: "the box did not answer", ofReading: false })
    assert.equal(todosState.step(box, { type: "Basis", basis: basis() }).problem?.reason, "the box did not answer", `seed ${seed}`)
  }))
