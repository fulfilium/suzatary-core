// A task's processor time, as the bigstore scene judges it: never more than the task's own time on the clock, and the
// scene still fails on a real task over the bound.

import { test } from "node:test"
import assert from "node:assert/strict"

import type { TraceEvent } from "../e2e/cdp"
import { heaviestOf, taskOf, tasksOf } from "../e2e/trace"

const CPU_MS = 100

const run = (ts: number, dur: number, tdur?: number): TraceEvent => ({ name: "RunTask", cat: "disabled-by-default-devtools.timeline", ph: "X", ts, dur, ...(tdur === undefined ? {} : { tdur }), pid: 1, tid: 1 })
const mark: TraceEvent = { name: "end", cat: "blink.user_timing", ph: "R", ts: 10_000_000, pid: 1, tid: 1, args: { data: { startTime: 10_000 } } }

test("a thread time over the task's time on the clock is read as the clock's", () => {
  // The recorded failure: 133 ms of thread time in a task of 83 ms.
  const [task] = tasksOf([run(1_000_000, 83_000, 133_000), mark], "end")
  assert.equal(task?.threadMs, 133)
  assert.equal(task?.ms, 83)
  assert.equal(task?.cpuMs, 83)
  assert.ok((task?.cpuMs ?? Infinity) <= CPU_MS)
})

test("a real task of 150 ms is over the bound, however the thread time reads", () => {
  for (const tdur of [150_000, 140_000, 400_000]) {
    const task = heaviestOf(tasksOf([run(1_000_000, 150_000, tdur), run(2_000_000, 5_000, 4_000), mark], "end"))
    assert.ok((task?.cpuMs ?? 0) > CPU_MS, `tdur ${tdur}`)
  }
})

test("a task without a thread time counts its time on the clock, and the heaviest is one task with its own readings", () => {
  assert.deepEqual(taskOf(0, 30_000, undefined), { at: 0, ms: 30, threadMs: 30, cpuMs: 30 })
  const tasks = tasksOf([run(1_000_000, 90_000, 20_000), run(2_000_000, 60_000, 59_000), mark], "end")
  const heaviest = heaviestOf(tasks)
  assert.deepEqual([heaviest?.ms, heaviest?.threadMs, heaviest?.cpuMs], [60, 59, 59])
  assert.equal(heaviestOf([]), null)
})
