// The intent set's laws: a join-semilattice that only climbs, retracted by subsumption, and a lost answer
// that leaves the intent in view until the history holds it or asking again, once, is safe.

import { test } from "node:test"
import assert from "node:assert/strict"

import intents from "../src/prodrome/intents"
import { forAll, type Rng } from "./gen"

type A = { readonly name: string }
const KEYS = ["a", "b", "c", "d"]

const answer = (r: Rng): intents.Answer =>
  r.pick<intents.Answer>([{ type: "Taken" }, { type: "Refused", e: r.pick(["no", "gone"]) }, { type: "Lost", e: "offline" }])

const event = (r: Rng, t: intents.T<A>): intents.Event<A> => {
  const k = r.pick(KEYS)
  const asks = t.asked.get(k)?.asks ?? 0
  return r.pick<intents.Event<A>>([
    { type: "Asked", key: k, act: { name: k } },
    { type: "Answered", key: k, asks: r.bool(0.8) ? asks : asks + 1, answer: answer(r) },
    { type: "Subsumed", key: k },
  ])
}

const history = (r: Rng): intents.T<A> =>
  r.array(r.int(12), () => 0).reduce<intents.T<A>>(t => intents.step(t, event(r, t)), intents.EMPTY)

const seen = (t: intents.T<A>) => ({
  asked: [...t.asked].sort(([x], [y]) => (x < y ? -1 : 1)),
  retracted: [...t.retracted].sort(),
})

const holder = (r: Rng): ((a: A) => boolean) => {
  const held = new Set(KEYS.filter(() => r.bool()))
  return a => held.has(a.name)
}

test("law: join is idempotent, commutative and associative, with EMPTY its unit", () =>
  forAll(300, r => {
    const [a, b, c] = [history(r), history(r), history(r)]
    assert.deepEqual(seen(intents.join(a, a)), seen(a))
    assert.deepEqual(seen(intents.join(intents.EMPTY, a)), seen(a))
    assert.deepEqual(seen(intents.join(a, b)), seen(intents.join(b, a)))
    assert.deepEqual(seen(intents.join(intents.join(a, b), c)), seen(intents.join(a, intents.join(b, c))))
  }))

test("law: every step climbs, so no event un-asks or un-retracts", () =>
  forAll(300, r => {
    const t = history(r)
    const u = intents.step(t, event(r, t))
    assert.deepEqual(seen(intents.join(t, u)), seen(u))
  }))

test("law: asking twice is asking once, while the first is pending, taken or subsumed", () =>
  forAll(300, r => {
    const t = history(r)
    const k = r.pick(KEYS)
    const once = intents.step(t, { type: "Asked", key: k, act: { name: k } })
    assert.equal(intents.step(once, { type: "Asked", key: k, act: { name: k } }), once)
    assert.equal(intents.wants(once, k), false)
  }))

test("law: settling retracts exactly what the history holds, for good, and commutes with join", () =>
  forAll(300, r => {
    const [a, b] = [history(r), history(r)]
    const holds = holder(r)
    const s = intents.settle(a, holds)
    assert.ok(intents.pending(s).every(x => !holds(x)))
    assert.deepEqual(seen(intents.settle(s, holds)), seen(s))
    assert.ok([...a.retracted].every(k => s.retracted.has(k)))
    assert.deepEqual(seen(intents.settle(intents.join(a, b), holds)), seen(intents.join(intents.settle(a, holds), intents.settle(b, holds))))
  }))

test("law: what a card shows is the history's once it holds the act, and the intent's until then", () =>
  forAll(300, r => {
    const t = history(r)
    const k = r.pick(KEYS)
    assert.deepEqual(intents.shown(t, k, true), { type: "Absent" })
    const s = intents.shown(t, k, false)
    assert.equal(s.type === "Absent", !intents.isIn(t, k))
  }))

test("law: a lost answer leaves the intent in view; the next sync subsumes it, or asking again is one more ask", () =>
  forAll(200, r => {
    const k = r.pick(KEYS)
    const asked = intents.step(history(r), { type: "Asked", key: k, act: { name: k } })
    if (!intents.isIn(asked, k)) return
    const asks = asked.asked.get(k)?.asks ?? 0
    const lost = intents.step(asked, { type: "Answered", key: k, asks, answer: { type: "Lost", e: "the network blinked" } })
    if (asked.asked.get(k)?.answer !== null) return
    assert.deepEqual(intents.shown(lost, k, false), { type: "Failed", e: "the network blinked", lost: true })
    if (r.bool()) {
      const synced = intents.settle(lost, a => a.name === k)
      assert.deepEqual(intents.shown(synced, k, false), { type: "Absent" })
      assert.equal(intents.wants(synced, k), false, "nothing the history holds is asked again")
    } else {
      assert.equal(intents.wants(lost, k), true)
      const again = intents.step(lost, { type: "Asked", key: k, act: { name: k } })
      assert.equal(again.asked.get(k)?.asks, asks + 1)
      assert.deepEqual(intents.shown(again, k, false), { type: "Pending" })
      assert.equal(intents.step(again, { type: "Asked", key: k, act: { name: k } }), again, "a double tap is one ask")
      const late = intents.step(again, { type: "Answered", key: k, asks, answer: { type: "Taken" } })
      assert.equal(late, again, "an answer to an earlier ask lands nowhere")
    }
  }))

test("law: the key is the act's content, whatever order its fields were spelled in", () =>
  forAll(200, r => {
    const a = { x: r.int(9), y: { p: r.pick(["s", "t"]), q: [r.int(3)] } }
    const b = { y: { q: a.y.q, p: a.y.p }, x: a.x }
    assert.equal(intents.key(a), intents.key(b))
    assert.notEqual(intents.key(a), intents.key({ ...a, x: a.x + 1 }))
  }))
