// rendered's laws: eviction keeps the newest that fit the budget, and usage is a monoid.

import { test } from "node:test"
import assert from "node:assert/strict"

import idb from "../src/app/idb"
import rendered from "../src/app/rendered"
import { fakeIndexedDB } from "./fakeidb"
import table from "../src/algebra/table"
import { forAll, forAllAsync, keys } from "./gen"
import type { Rng } from "./gen"

const metas = (r: Rng): rendered.Meta[] => keys(r, 20).map(key => ({ key, bytes: 1 + r.int(100), used: r.int(50) }))
const usage = (r: Rng): rendered.Usage => ({ count: r.int(9), bytes: r.int(1000) })

test("law: after eviction the kept fit the budget, and nothing kept is older than anything evicted", () =>
  forAll(500, r => {
    const ms = metas(r)
    const budget = r.int(800)
    const gone = new Set(rendered.evict(ms, budget))
    const kept = ms.filter(m => !gone.has(m.key))
    assert.ok(rendered.usageOf(kept).bytes <= budget)
    for (const k of kept) for (const g of ms.filter(m => gone.has(m.key))) assert.ok(k.used >= g.used, `${k.key} kept over ${g.key}`)
    assert.deepEqual(rendered.evict(kept, budget), [], "evicting twice is evicting once")
  }))

test("law: within budget nothing is evicted", () =>
  forAll(300, r => {
    const ms = metas(r)
    assert.deepEqual(rendered.evict(ms, rendered.usageOf(ms).bytes), [])
  }))

test("law: usage is a monoid under combine, and the usage of a concatenation is the combine", () =>
  forAll(300, r => {
    const [a, b, c] = [usage(r), usage(r), usage(r)]
    assert.deepEqual(rendered.combine(a, rendered.none), a)
    assert.deepEqual(rendered.combine(rendered.none, a), a)
    assert.deepEqual(rendered.combine(rendered.combine(a, b), c), rendered.combine(a, rendered.combine(b, c)))
    const [xs, ys] = [metas(r), metas(r)]
    assert.deepEqual(rendered.usageOf([...xs, ys].flat()), rendered.combine(rendered.usageOf(xs), rendered.usageOf(ys)))
  }))


test("law: joining the box's pairs is union: again is once, and a second value at a key is a finding read as nothing", async () => {
  const store = rendered.memory(new Set(["markup"]))
  const k = table.key("markup", "ab")
  assert.deepEqual(await store.join([[k, "<p>a</p>"]]), [])
  assert.deepEqual(await store.join([[k, "<p>a</p>"]]), [])
  assert.deepEqual(await store.get([k]), ["<p>a</p>"])
  assert.deepEqual(await store.join([[k, "<p>b</p>"]]), [k])
  assert.deepEqual(await store.get([k]), [null])
})

test("through answers what the device holds, or asks once and keeps the answer", async () => {
  const store = rendered.memory(new Set(["answer"]))
  const k = table.key("answer", "cd")
  let asked = 0
  const ask = () => ((asked += 1), Promise.resolve({ html: "<p>x</p>", key: k }))
  assert.equal(await rendered.through(store, k, ask), "<p>x</p>")
  assert.equal(await rendered.through(store, k, ask), "<p>x</p>")
  assert.equal(asked, 1)
})

test("throughAll answers what through would for each, asks once a miss, and a failed ask is null", async () => {
  const store = rendered.memory(new Set(["answer"]))
  const [held, missed, failed] = ["aa", "bb", "cc"].map(n => table.key("answer", n))
  await store.join([[held!, "<p>held</p>"]])
  let asked = 0
  const ask = (k: table.Key, html: string) => () => ((asked += 1), Promise.resolve({ html, key: k }))
  const first = await rendered.throughAll(store, [
    { k: held!, ask: ask(held!, "<p>not asked</p>") },
    { k: missed!, ask: ask(missed!, "<p>missed</p>") },
    { k: failed!, ask: () => Promise.reject(new Error("refused")) },
  ])
  assert.deepEqual(first, ["<p>held</p>", "<p>missed</p>", null])
  assert.equal(asked, 1)
  assert.deepEqual(await store.get([missed!]), ["<p>missed</p>"], "the answer is kept")
})

test("through answers what a box of another build renders, and keeps it under neither name", async () => {
  const store = rendered.memory(new Set(["answer-of-this-build"]))
  const mine = table.key("answer-of-this-build", "cd")
  const theirs = table.key("answer-of-the-box", "cd")
  assert.equal(await rendered.through(store, mine, () => Promise.resolve({ html: "<p>x</p>", key: theirs })), "<p>x</p>")
  assert.deepEqual(await store.get([mine, theirs]), [null, null])
})

test("law: the device holds blocks of its own functions alone, and a forget leaves none of another's", () =>
  forAllAsync(100, async r => {
    const fns = ["markup", "answer", "older-markup", "older-answer"]
    const own = new Set(fns.filter(() => r.bool(0.5)))
    const rows = table.of<string>(table.keep())
    const before = rendered.memory(new Set(fns), rows)
    const pairs = keys(r, 12).map((arg, i): rendered.Pair => [table.key(fns[r.int(fns.length)]!, arg), `<p>${i}</p>`])
    await before.join(pairs)
    const store = rendered.memory(own, rows)
    await store.join(keys(r, 6).map((arg): rendered.Pair => [table.key(fns[r.int(fns.length)]!, arg), "<p>new</p>"]))
    assert.ok(rows.pairs().every(([k]) => own.has(table.fnOf(k)) || pairs.some(([p]) => p === k)), "a join takes no stranger")
    const named = pairs.filter(() => r.bool(0.3)).map(([k]) => k)
    await store.forget(named)
    assert.ok(rows.pairs().every(([k]) => own.has(table.fnOf(k)) && !named.includes(k)), "a forget leaves no stranger and nothing named")
  }))

test("a block under a key the device forgot, back on the device another way, goes at the next forget", async () => {
  const device = fakeIndexedDB()
  const store = rendered.device(new Set(["answer"]))
  const [erased, kept] = [table.key("answer", "erased"), table.key("answer", "kept")]
  await store.join([[erased, "<p>erased words</p>"], [kept, "<p>other words</p>"]])
  await store.forget([erased])
  // A copy written past the worker, as a tab closed mid-write leaves one: the words of a husked line no longer name it.
  await idb.within({ name: rendered.DB, version: 4, store: rendered.ROWS, upgrade: () => {} }, "readwrite", s =>
    idb.done(s.put({ key: erased, html: "<p>erased words</p>", bytes: 1, used: 1 })),
  )
  assert.ok(device.printed().some(r => r.includes("erased words")), "the block is back")
  await store.forget([])
  assert.ok(!device.printed().some(r => r.includes("erased words")), "and goes at the next forget")
  assert.ok(device.printed().some(r => r.includes("other words")), "with nothing else")
})
