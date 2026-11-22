import { test } from "node:test"
import assert from "node:assert/strict"

import rehearsal from "../src/pages/rehearsal"
import type time from "../src/app/time"

// A worker as the page sees it: what it was said, whether it was ended, and a way to tell or fail.
const port = () => {
  const said: rehearsal.Said[] = []
  const ears: { message: ((e: MessageEvent<rehearsal.Told>) => void)[]; error: (() => void)[] } = { message: [], error: [] }
  let ended = 0
  const p: rehearsal.Port = {
    postMessage: s => void said.push(s),
    addEventListener: (type, f) => void (type === "message" ? ears.message.push(f) : ears.error.push(() => f(new MessageEvent("error")))),
    terminate: () => void (ended += 1),
  }
  const tell = (t: rehearsal.Told): void => ears.message.forEach(f => f(new MessageEvent("message", { data: t })))
  return { p, said, tell, fail: () => ears.error.forEach(f => f()), ended: () => ended }
}
const os: readonly time.Offset[] = [{ from: 0, minutes: 60 }]
const settled = (p: Promise<unknown>): Promise<boolean> => Promise.race([p.then(() => true), new Promise<boolean>(done => setTimeout(() => done(false), 20))])

test("a load that fails ends the worker, and one that loads leaves it to close itself", async () => {
  const w = port()
  const work = rehearsal.start(w.p, { name: "UTC" }, 0, () => {})
  await assert.rejects(work.loading(() => Promise.reject(new Error("no module"))), /no module/)
  assert.equal(w.ended(), 1)
  const v = port()
  assert.equal(await rehearsal.start(v.p, { name: "UTC" }, 0, () => {}).loading(() => Promise.resolve(7)), 7)
  assert.equal(v.ended(), 0)
})

test("the zone is told first, and the offsets wait ends with its offsets, held", async () => {
  const w = port()
  const held: string[] = []
  const work = rehearsal.start(w.p, { name: "Pacific/Chatham" }, 5, z => void held.push(z))
  assert.deepEqual(w.said, [{ type: "Zone", zone: "Pacific/Chatham", now: 5 }])
  assert.equal(await settled(work.offsets), false)
  w.tell({ type: "Offsets", zone: "Pacific/Chatham", offsets: os })
  assert.equal(await settled(work.offsets), true)
  assert.deepEqual(held, ["Pacific/Chatham"])
})

test("a worker that fails ends every wait", async () => {
  const w = port()
  const work = rehearsal.start(w.p, { name: "UTC" }, 0, () => {})
  const rehearsed = work.rehearse({} as WebAssembly.Module)
  w.fail()
  assert.equal(await settled(Promise.all([work.offsets, rehearsed])), true)
})
