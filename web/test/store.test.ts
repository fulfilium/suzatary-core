// The fold on screen is one value, stepped by a pure function: which fold is shown is decided by who read it and
// what it read, never by which answered last, and it carries the numbers it is handed as the screen observes them.

import { test } from "node:test"
import assert from "node:assert/strict"

import type { Entry } from "../src/wire/generated/wire"
import onscreen, { store } from "../src/app/store"
import heads from "../src/prodrome/heads"
import fulfillment from "../src/todo/fulfillment"
import { SLUG, forAll, type Rng } from "./gen"

const entry = (r: Rng, todo: string): Entry => ({
  todo,
  state: r.pick(["open", "completed", "cancelled"]),
  states: [],
  at: "",
  claimed: "",
  unconfirmed: r.bool(0.2),
  recurring: r.bool(0.2),
  value: r.int(101) / 100,
  conflicts: {},
  authored: null,
  candidates: [],
  stream: [],
})

const entries = (r: Rng): Entry[] => {
  const seen = new Set<string>()
  return r.array(r.int(6), () => {
    let todo = r.string(SLUG, 1 + r.int(6))
    while (seen.has(todo)) todo = `${todo}${r.string(SLUG, 1)}`
    seen.add(todo)
    return entry(r, todo)
  })
}

// A small history: a root, two writes in a line, a third beside the second, and a merge of the two.
const hex = (c: string): string => c.repeat(64)
const [G, H1, H2, H3, H4] = ["0", "1", "2", "3", "4"].map(hex) as [string, string, string, string, string]
const history: ReadonlyMap<string, readonly string[]> = new Map([[G, []], [H1, [G]], [H2, [H1]], [H3, [H1]], [H4, [H2, H3]]])

// How two head sets stand, read off `known`, as the core reads them off what a chain holds: a set naming what is not
// known is unknown to every other.
const ancestry = (known: ReadonlyMap<string, readonly string[]>): onscreen.Ancestry => {
  const under = (names: readonly string[]): Set<string> | null => {
    const seen = new Set<string>()
    const left = [...names]
    for (let n = left.pop(); n !== undefined; n = left.pop()) {
      const parents = known.get(n)
      if (parents === undefined) return null
      if (!seen.has(n)) (seen.add(n), left.push(...parents))
    }
    return seen
  }
  return (a, b) => {
    if (heads.same(heads.of(a), heads.of(b))) return "Same"
    const [x, y] = [under(a), under(b)]
    if (x === null || y === null) return "Unknown"
    return a.every(n => y.has(n)) ? "Below" : b.every(n => x.has(n)) ? "Above" : "Apart"
  }
}
const read = ancestry(history)
const FRONTIERS: string[][] = [[G], [H1], [H2], [H3], [H2, H3], [H4]]
const names = (r: Rng): string[] => r.pick(FRONTIERS)

const box = (r: Rng, heads = names(r), order: onscreen.Ancestry = read): onscreen.Event =>
  ({ type: "Box", payload: { at: "2026-09-16T09:00:00", entries: entries(r), failed: [], heads, next: null }, fetchedAt: 1 + r.int(1000), ancestry: order })
const local = (r: Rng, heads = names(r), order: onscreen.Ancestry = read): onscreen.Event =>
  ({ type: "Local", entries: entries(r), at: "2026-09-16T09:00:00", next: null, heads, fresh: r.bool(), ancestry: order })
const cached = (r: Rng): onscreen.Event => ({ type: "Cached", fold: { at: "2026-09-15T09:00:00", fetchedAt: 1 + r.int(1000), entries: entries(r) } })

const event = (r: Rng): onscreen.Event =>
  r.pick<() => onscreen.Event>([() => box(r), () => local(r), () => cached(r), () => ({ type: "Stale" }), () => ({ type: "Reset" })])()

const walk = (r: Rng): onscreen.Fold => r.array(r.int(8), () => event(r)).reduce(onscreen.step, onscreen.UNLOADED)
const step2 = (f: onscreen.Fold, a: onscreen.Event, b: onscreen.Event): onscreen.Fold => onscreen.step(onscreen.step(f, a), b)

test("law: the box's fold never replaces the tab's own, so a box answer landing after a local act cannot hide it", () => {
  forAll(300, r => {
    const f = walk(r)
    const [b, l] = [box(r), local(r)]
    assert.deepEqual(step2(f, b, l), step2(f, l, b), "either order shows the local fold")
    const shownLocal = onscreen.step(f, l)
    assert.equal(onscreen.step(shownLocal, b), shownLocal, "and the box's changes nothing after it")
  })
})

test("law: of two folds by one reader, the later in the causal order stays, whichever came last", () => {
  forAll(300, r => {
    const f = walk(r)
    // A straight line ({H1} below {H2}), a merge above both sides, and a set below its own merge.
    for (const [below, above] of [[[H1], [H2]], [[H2], [H4]], [[H2, H3], [H4]], [[G], [H3]]] as const)
      for (const make of [box, local]) {
        const [a, b] = [make(r, [...below]), make(r, [...above])]
        assert.deepEqual(step2(f, a, b), step2(f, b, a))
      }
  })
})

test("law: of two concurrent folds by one reader the one shown stays; where the tab holds neither, the later answer", () => {
  forAll(100, r => {
    for (const make of [box, local]) {
      const [a, b] = [make(r, [H2]), make(r, [H3])]
      assert.deepEqual(step2(onscreen.UNLOADED, a, b), onscreen.step(onscreen.UNLOADED, a))
      const unheld = (): onscreen.Event => make(r, names(r), ancestry(new Map()))
      const [x, y] = [unheld(), unheld()]
      assert.deepEqual(step2(onscreen.UNLOADED, x, y), onscreen.step(onscreen.UNLOADED, y))
    }
  })
})

test("law: a cached fold only boots an empty tab", () => {
  forAll(300, r => {
    const f = walk(r)
    const after = onscreen.step(f, cached(r))
    if (f.type === "Unloaded") assert.equal(onscreen.loaded(after), true)
    else assert.equal(after, f)
    assert.equal(onscreen.fresh(after), f.type === "Unloaded" ? false : onscreen.fresh(f), "a cache is never about now")
  })
})

test("law: stale keeps the rows and drops only the claim that they are current; reset forgets them; both idempotent", () => {
  forAll(300, r => {
    const f = walk(r)
    const s = onscreen.step(f, { type: "Stale" })
    assert.deepEqual(onscreen.entries(s), onscreen.entries(f))
    assert.equal(onscreen.fresh(s), false)
    assert.equal(onscreen.step(s, { type: "Stale" }), s)
    const gone = onscreen.step(f, { type: "Reset" })
    assert.equal(gone, onscreen.UNLOADED)
    assert.equal(onscreen.step(gone, { type: "Reset" }), gone)
  })
})

test("law: a fold is total: rows exactly while shown, one per todo, and each found by its id", () => {
  forAll(300, r => {
    const f = walk(r)
    assert.equal(onscreen.at(f) === null, !onscreen.loaded(f))
    const ids = onscreen.entries(f).map(e => e.todo)
    assert.equal(new Set(ids).size, ids.length)
    for (const e of onscreen.entries(f)) assert.equal(onscreen.entry(f, e.todo), e)
    assert.equal(onscreen.entry(f, `${r.string(SLUG, 3)}-absent`), undefined)
  })
})

test("law: the fold is held as the screen observes it, so folds alike at the whole percent are one state", () => {
  forAll(300, r => {
    const raw = entries(r).map(e => ({ ...e, value: r.int(1_000_001) / 1_000_000 }))
    const whole = raw.map(e => ({ ...e, value: fulfillment.whole(e.value) }))
    const of = (es: Entry[]) => onscreen.step(onscreen.UNLOADED, { type: "Box", payload: { at: "t", entries: es, failed: [], heads: [], next: null }, fetchedAt: 1, ancestry: read })
    assert.deepEqual(of(raw), of(whole))
  })
})

test("the app's store is the step as a source: a step that changes nothing wakes nobody", () => {
  let heard = 0
  const stop = store.subscribe(() => (heard += 1))
  store.dispatch({ type: "Reset" })
  assert.equal(heard, 0, "already unloaded")
  store.dispatch({ type: "Box", payload: { at: "t", entries: [], failed: [], heads: ["a"], next: null }, fetchedAt: 1, ancestry: read })
  assert.equal(heard, 1)
  assert.equal(onscreen.loaded(store.state()), true, "an empty fold is loaded, which is not the same as unasked")
  stop()
  store.dispatch({ type: "Reset" })
})
