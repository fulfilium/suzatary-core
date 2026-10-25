// A pass is drawn in slices, each a task of its own and the first after the task that asked; a hidden tab, whose tasks
// are throttled, draws it in one pass, and a tab hidden mid-way draws the rest at once.

import { test } from "node:test"
import assert from "node:assert/strict"

import slices from "../src/chat/slices"

// A clock whose time moves `step` ms each reading, and whose tasks wait in `tasks` until a test runs them.
const clockOf = (step: number) => {
  let t = 0
  let hidden = false
  const tasks: (() => void)[] = []
  const clock: slices.Clock = {
    hidden: () => hidden,
    now: () => (t += step),
    later: f => {
      tasks.push(f)
      return () => void tasks.splice(tasks.indexOf(f), 1)
    },
  }
  return { clock, tasks, hide: (h: boolean) => void (hidden = h), next: () => tasks.shift()?.() }
}

// Work of `n` units, counting what ran.
const workOf = (n: number) => {
  const ran = { units: 0, done: 0 }
  const w: slices.Work = { unit: () => (ran.units += 1) < n, done: () => void (ran.done += 1) }
  return { w, ran }
}

test("a visible tab draws a pass in slices, each a task, the first after the task that asked, at least one unit each", () => {
  const c = clockOf(slices.BUDGET_MS + 1)
  const { w, ran } = workOf(3)
  slices.of(c.clock).run(w)
  assert.deepEqual(ran, { units: 0, done: 0 }, "nothing is drawn in the task that asked")
  c.next()
  assert.deepEqual(ran, { units: 1, done: 0 })
  c.next()
  c.next()
  assert.deepEqual(ran, { units: 3, done: 1 })
  assert.equal(c.tasks.length, 0)
})

test("a hidden tab draws the pass in one pass, in the task that asked, and leaves no task", () => {
  const c = clockOf(slices.BUDGET_MS + 1)
  c.hide(true)
  const { w, ran } = workOf(50)
  slices.of(c.clock).run(w)
  assert.deepEqual(ran, { units: 50, done: 1 })
  assert.equal(c.tasks.length, 0)
})

test("a tab hidden mid-way draws the rest at once, and a pass replaced draws nothing more", () => {
  const c = clockOf(slices.BUDGET_MS + 1)
  const s = slices.of(c.clock)
  const { w, ran } = workOf(10)
  s.run(w)
  c.next()
  c.hide(true)
  s.flush()
  assert.deepEqual(ran, { units: 10, done: 1 })
  assert.equal(c.tasks.length, 0)
  c.hide(false)
  const first = workOf(10)
  s.run(first.w)
  c.next()
  s.run(workOf(1).w)
  c.next()
  assert.equal(first.ran.units, 1, "the replaced pass drew no more")
})
