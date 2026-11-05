// An asked question's register: its answers in the total order of the asks that brought them, so the reading is
// the newest answer however late each arrived, a failure never takes an answer away, and registers join to one.

import { test } from "node:test"
import assert from "node:assert/strict"

import asked from "../src/app/asked"
import lo from "../src/algebra/lo"
import type { Rng } from "./gen"
import { forAll } from "./gen"

type Event = { readonly type: "Answer"; readonly n: number; readonly v: number } | { readonly type: "Fail"; readonly n: number }

// Asks 1..k made in order, their outcomes arriving in any order, some never; an ask has one answer, so every
// history is of one world.
const history = (r: Rng): { asks: number; events: Event[] } => {
  const asks = 1 + r.int(6)
  const events = Array.from({ length: asks }, (_, i): Event | null =>
    r.bool(0.2) ? null : r.bool(0.7) ? { type: "Answer", n: i + 1, v: ((i + 1) * 31) % 97 } : { type: "Fail", n: i + 1 })
    .flatMap(e => (e === null ? [] : [e]))
  for (let i = events.length - 1; i > 0; i -= 1) {
    const j = r.int(i + 1)
    ;[events[i], events[j]] = [events[j] as Event, events[i] as Event]
  }
  return { asks, events }
}

const folded = (asks: number, events: readonly Event[]): asked.Register<number> =>
  events.reduce<asked.Register<number>>(
    (reg, e) => (e.type === "Answer" ? asked.answer(reg, e.n, e.v, e.n * 1000) : asked.fail(reg, e.n, `ask ${e.n} failed`)),
    Array.from({ length: asks }, (_, i) => i + 1).reduce<asked.Register<number>>(asked.asking, asked.empty),
  )

test("law: the reading is the answer of the greatest ask answered, whatever order the answers came in", () =>
  forAll(400, r => {
    const { asks, events } = history(r)
    const answers = events.flatMap(e => (e.type === "Answer" ? [e] : [])).sort((a, b) => b.n - a.n)
    const newest = answers[0]
    const failed = events.some(e => e.type === "Fail" && e.n === asks)
    assert.deepEqual(asked.read(folded(asks, events)), newest !== undefined ? lo.val(newest.v) : failed ? lo.error(`ask ${asks} failed`) : lo.loading)
  }))

test("law: a late answer to an earlier ask never replaces a newer one", () =>
  forAll(200, r => {
    const v = r.int(100)
    const late = asked.answer(asked.answer(asked.asking(asked.asking(asked.empty, 1), 2), 2, v, 2000), 1, v + 1, 1000)
    assert.deepEqual(asked.read(late), lo.val(v))
  }))

test("law: a failure never takes an answer away", () =>
  forAll(200, r => {
    const { asks, events } = history(r)
    const reg = folded(asks, events)
    assert.deepEqual(asked.read(asked.fail(asked.asking(reg, asks + 1), asks + 1, "down")), lo.isVal(asked.read(reg)) ? asked.read(reg) : lo.error("down"))
  }))

test("law: registers join commutatively, associatively and idempotently", () =>
  forAll(300, r => {
    const [a, b, c] = [history(r), history(r), history(r)].map(h => folded(h.asks, h.events)) as [asked.Register<number>, asked.Register<number>, asked.Register<number>]
    assert.deepEqual(asked.join(a, b), asked.join(b, a))
    assert.deepEqual(asked.join(asked.join(a, b), c), asked.join(a, asked.join(b, c)))
    assert.deepEqual(asked.join(a, a), a)
  }))

test("a question is asked when first read, read again while held, and asked again when the box moves", async () => {
  let asks = 0
  const q = asked.family((n: number) => String(n), (n: number) => Promise.resolve(n * 10 + (asks += 1)))
  const r = q(4)
  assert.deepEqual(r.now(), lo.loading)
  let heard = 0
  const off = r.hear(() => (heard += 1))
  await new Promise(ok => setTimeout(ok, 0))
  assert.deepEqual(r.now(), lo.val(41))
  assert.equal(q(4).now(), r.now(), "one register per name")
  asked.moved()
  await new Promise(ok => setTimeout(ok, 0))
  assert.deepEqual(r.now(), lo.val(42))
  assert.equal(heard, 2)
  off()
  asked.moved()
  await new Promise(ok => setTimeout(ok, 0))
  assert.equal(asks, 2, "a question nobody reads is not asked")
})

test("law: what is held is behind exactly when the newest ask failed after an answer stood, or one is slow, dated by that answer", () =>
  forAll(400, r => {
    const { asks, events } = history(r)
    const reg = folded(asks, events)
    const newest = events.flatMap(e => (e.type === "Answer" ? [e.n] : [])).reduce((m, n) => Math.max(m, n), 0)
    const failedLast = events.some(e => e.type === "Fail" && e.n === asks)
    const since = newest * 1000
    assert.deepEqual(asked.behind(reg, false), newest > 0 && failedLast && asks > newest ? { why: `ask ${asks} failed`, since } : null)
    assert.deepEqual(
      asked.behind(reg, true),
      newest === 0 ? null : failedLast && asks > newest ? { why: `ask ${asks} failed`, since } : asks > newest ? { why: null, since } : null,
    )
  }))

test("an ask long on its way is said slow, and one that fails leaves what stood, said with why", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] })
  let n = 0
  let fail: (e: Error) => void = () => {}
  const r = asked.question(() => ((n += 1) === 1 ? Promise.resolve("first") : new Promise<string>((_, no) => void (fail = no))))
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 10; i++) await Promise.resolve()
  }
  const heard = r.hear(() => {})
  await settle()
  assert.deepEqual(r.now(), { type: "Val", v: "first" })
  r.again()
  assert.equal(r.behind(), null, "not said before it is slow")
  t.mock.timers.tick(asked.SLOW_MS)
  assert.equal(r.behind()?.why, null, "said to be on its way")
  fail(new Error("the box did not answer in 30 s"))
  await settle()
  assert.match(r.behind()?.why ?? "", /did not answer/)
  assert.deepEqual(r.now(), { type: "Val", v: "first" })
  heard()
})

test("an answer equal to the one standing wakes nobody, and one that is not wakes the reader", async () => {
  let v = 1
  const q = asked.family((n: number) => `same ${n}`, () => Promise.resolve({ v }), (a, b) => a.v === b.v)(0)
  let heard = 0
  const off = q.hear(() => (heard += 1))
  await new Promise(ok => setTimeout(ok, 0))
  const first = q.now()
  q.again()
  await new Promise(ok => setTimeout(ok, 0))
  assert.equal(q.now(), first, "the same object while the answer is the same")
  v = 2
  q.again()
  await new Promise(ok => setTimeout(ok, 0))
  assert.notEqual(q.now(), first)
  assert.equal(heard, 2)
  off()
})

test("a family keeps at most KEPT registers nobody reads, and never drops one being read", async () => {
  let asks = 0
  const q = asked.family((n: number) => `kept ${n}`, (n: number) => Promise.resolve(n + 0 * (asks += 1)))
  const read = q(-1)
  const off = read.hear(() => {})
  for (let n = 0; n < asked.KEPT * 2; n++) q(n).now()
  await new Promise(ok => setTimeout(ok, 0))
  assert.deepEqual(read.now(), lo.val(-1), "the register being read stands")
  const before = asks
  q(-1).hear(() => {})
  assert.equal(asks, before, "it was never dropped, so a second reader asks nothing")
  off()
})

test("law: an ask made while a read is in flight is a second read, started after the first lands", async () => {
  // The box may change while the first read is on its way, after that read already read its state: the second
  // read is what shows the change, so it is never merged into the first.
  const started: number[] = []
  const pending: ((v: number) => void)[] = []
  const q = asked.question(() => new Promise<number>(ok => (started.push(pending.length), pending.push(ok))))
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 10; i++) await Promise.resolve()
  }
  const off = q.hear(() => {})
  await settle()
  assert.equal(started.length, 1, "the first read starts")
  q.again()
  q.again()
  await settle()
  assert.equal(started.length, 1, "nothing more starts while the first is in flight")
  pending[0]?.(1)
  await settle()
  assert.equal(started.length, 2, "one more read starts once the first lands")
  pending[1]?.(2)
  await settle()
  assert.deepEqual(q.now(), lo.val(2), "and its answer stands")
  assert.equal(started.length, 2, "and nothing more is asked")
  off()
})
