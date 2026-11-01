// The algebra of a reading (`web/src/algebra/reading.ts`): a functor and an applicative on what it reads now, heard
// when any part moves, and a reading that did not move hands back the same object, so nothing downstream wakes.

import { test } from "node:test"
import assert from "node:assert/strict"

import reading from "../src/algebra/reading"
import lo from "../src/algebra/lo"
import { forAll, loState } from "./gen"
import type { Rng } from "./gen"

// A reading of a cell the test moves, heard by whoever listens.
const cell = <V>(v: lo.T<V>) => {
  let now = v
  const ears = new Set<() => void>()
  const r: reading.Reading<V> = { now: () => now, hear: e => (ears.add(e), () => void ears.delete(e)), again: () => {}, behind: () => null }
  return { r, set: (w: lo.T<V>) => ((now = w), ears.forEach(e => e())) }
}
const state = (r: Rng): lo.T<number> => loState(r, () => r.int(100))

test("law: map is a functor on what is read now", () =>
  forAll(200, r => {
    const c = cell(state(r))
    const [f, g] = [(n: number) => n + 1, (n: number) => n * 3]
    assert.deepEqual(reading.map(c.r, x => x).now(), c.r.now())
    assert.deepEqual(reading.map(reading.map(c.r, f), g).now(), reading.map(c.r, x => g(f(x))).now())
  }))

test("law: both reads each part now, as lo.combine's monoid", () =>
  forAll(200, r => {
    const [a, b] = [cell(state(r)), cell(state(r))]
    assert.deepEqual(reading.both(a.r, b.r).now(), lo.map(lo.combine(a.r.now(), b.r.now()), ([x, y]) => [x, y]))
  }))

test("law: both with an unwaited part waits on the other part alone, and carries the unwaited part's state", () =>
  forAll(200, r => {
    const [a, b] = [cell(state(r)), cell(state(r))]
    assert.deepEqual(reading.both(a.r, reading.unwaited(b.r)).now(), lo.map(a.r.now(), x => [x, b.r.now()]))
  }))

test("law: a reading that did not move hands back the same object; one that moved, a new one", () =>
  forAll(200, r => {
    const [a, b] = [cell(state(r)), cell(state(r))]
    const m = reading.both(reading.map(a.r, n => ({ n })), b.r)
    const first = m.now()
    assert.equal(m.now(), first)
    b.set(lo.val(r.int(100)))
    if (lo.isVal(a.r.now())) assert.notEqual(m.now(), first)
  }))

test("a combined reading is heard when either part moves, and not after it is let go", () => {
  const [a, b] = [cell(lo.val(1)), cell(lo.val(2))]
  let heard = 0
  const off = reading.both(a.r, b.r).hear(() => (heard += 1))
  a.set(lo.val(3))
  b.set(lo.val(4))
  off()
  a.set(lo.val(5))
  assert.equal(heard, 2)
})

test("once arrives once, however often it is heard", async () => {
  let arrived = 0
  const r = reading.once(() => Promise.resolve((arrived += 1)))
  const offs = [r.hear(() => {}), r.hear(() => {})]
  await new Promise(ok => setTimeout(ok, 0))
  assert.deepEqual(r.now(), lo.val(1))
  r.again()
  await new Promise(ok => setTimeout(ok, 0))
  assert.equal(arrived, 1)
  offs.forEach(o => o())
})
