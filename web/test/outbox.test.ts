// What the shell says about acts the box has not taken: once per todo and state, an act on its way only once it is late,
// and never an object's name.

import { test } from "node:test"
import assert from "node:assert/strict"

import outbox from "../src/shell/outbox"
import type { Unsent } from "../src/wire/generated/pages"
import { forAll } from "./gen"

const STATES: readonly Unsent[] = ["Sending", "AwaitingTrust", "Refused"]

test("an act on its way is said only once it is late, and an act taken before then says nothing", () => {
  const sending = new Map<string, Unsent>([["pack-bags", "Sending"]])
  const early = outbox.step(outbox.EMPTY, sending, 0)
  assert.deepEqual(early.said, { type: "Quiet" })
  assert.equal(outbox.next(early.t, sending), outbox.LATE_MS)
  assert.deepEqual(outbox.step(early.t, new Map(), 1000).said, { type: "Quiet" }, "taken in time")
  const late = outbox.step(early.t, sending, outbox.LATE_MS)
  assert.equal(late.said.type === "Said" && late.said.about.type, "Several")
  assert.equal(outbox.next(late.t, sending), Infinity, "nothing left to wait for once said")
})

test("law: whatever the walk, a todo is told of a state at most once while it stands in it, and no line names an object", () =>
  forAll(300, r => {
    let t = outbox.EMPTY
    let was = new Map<string, Unsent>()
    const told = new Map<string, number>()
    for (let now = 0; now < 40; now += 1) {
      const unsent = new Map(["a", "b", "c"].filter(() => r.bool(0.7)).map((k): [string, Unsent] => [k, r.pick(STATES)]))
      for (const k of ["a", "b", "c"]) if (unsent.get(k) !== was.get(k)) told.set(k, 0)
      const out = outbox.step(t, unsent, now * 1000)
      for (const [k, u] of out.t.told) if (t.told.get(k) !== u) told.set(k, (told.get(k) ?? 0) + 1)
      if (out.said.type === "Said") assert.ok(!/[0-9a-f]{8}/.test(out.said.text), out.said.text)
      for (const [k, n] of told) assert.ok(n <= 1, `${k} told ${n} times`)
      t = out.t
      was = unsent
    }
  }))
