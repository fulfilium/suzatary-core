// newbuild over fake effects: a running page never reloads without a tap, it offers a newer build and says once per
// build which it started on, and this build keeps the box's contract.

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import newbuild from "../src/app/newbuild"
import inflight from "../src/app/inflight"
import http from "../src/wire/http"
import type shell from "../src/app/shell"
import { forAll, rng } from "./gen"

const stamped = (id: string): shell.Build => ({ type: "Stamped", id })
const settle = () => new Promise(ok => setTimeout(ok, 0))

// A box serving `build` and a page: what was asked, said and reloaded, on a device that announced `announced`.
const box = (build: string, announced: string[] = []) => {
  let served = build
  let version = "0.17.0"
  let asks = 0
  let reloads = 0
  const lines: shell.Line[] = []
  const fx: newbuild.Effects = {
    about: () => (asks++, Promise.resolve({ version, build: served })),
    announced: { read: () => announced, set: id => void announced.splice(0, announced.length, id) },
    said: l => void lines.push(l),
    reload: () => void reloads++,
  }
  return {
    fx,
    asks: () => asks,
    reloads: () => reloads,
    lines,
    deploy: (b: string, v = version) => void ((served = b), (version = v)),
  }
}

test("this build writes by a contract the box takes", () => {
  const box: unknown = JSON.parse(readFileSync("app/core/tests/web/client.json", "utf8"))
  const min = typeof box === "object" && box !== null && "min" in box ? Number(box.min) : NaN
  assert.ok(http.CLIENT >= min, `the bundle keeps contract ${http.CLIENT}, the box takes ${min} or later`)
  const headers = new Headers(http.actRequest({}).headers)
  assert.equal(headers.get(http.CLIENT_HEADER), String(http.CLIENT), "every write names it")
})

test("/api/about is read as two strings, so it is never misread, whatever the box answers: a misread answer asks it once, never in a loop", () =>
  forAll(300, r => {
    const u = r.pick<unknown>([null, 7, "x", [], {}, { build: 3 }, { build: "b1" }, { version: "0.17.0", build: "b2", extra: true }, { build: { id: "b" } }])
    assert.equal(newbuild.ABOUT(u).type, "Val")
  }))

test("law: a running page never reloads without a tap, whatever it hears", async () => {
  for (let seed = 1; seed <= 200; seed++) {
    const r = rng(seed * 2654435761)
    const b = box("b1", r.bool() ? [] : ["b0"])
    const m = newbuild.machine(stamped("b1"), b.fx)
    for (let i = r.int(12); i > 0; i--)
      await r.pick([
        () => m.check(),
        () => Promise.resolve(b.deploy(r.pick(["", "b1", "b2", "b3", "dev"]), r.pick(["0.17.0", "0.18.0"]))),
        () => Promise.resolve(m.acts.dismiss()),
      ])()
    await settle()
    assert.equal(b.reloads(), 0, `seed ${seed}`)
    m.acts.reload()
    await settle()
    assert.equal(b.reloads(), 1, `seed ${seed}: the tap reloads`)
  }
})

test("law: a page whose code is lost moves to the box's build only when the box serves another, and never on its own", async () => {
  for (const [served, moves] of [["b2", true], ["b1", false], ["", false], ["dev", true]] as const) {
    const b = box(served)
    const m = newbuild.machine(stamped("b1"), b.fx)
    const l = await m.lost()
    await settle()
    assert.equal(l.type, moves ? "Moved" : "Unloaded", served)
    assert.equal(b.reloads(), moves ? 1 : 0, `${served}: it reloads exactly when it moves`)
  }
  // A box that does not answer moved nothing the page can know of: it stays, and says it could not load.
  const down: newbuild.Effects = { ...box("b2").fx, about: () => Promise.reject(new Error("offline")), reload: () => assert.fail("no reload") }
  assert.equal((await newbuild.machine(stamped("b1"), down).lost()).type, "Unloaded")
  for (let seed = 1; seed <= 200; seed++) {
    const r = rng(seed * 2654435761)
    const own = r.pick(["b1", "b2"])
    const served = r.pick(["", "b1", "b2", "b3"])
    assert.equal(newbuild.lost(own, served).type === "Moved", served !== "" && served !== own, `seed ${seed}`)
  }
})

test("a newer build is offered, and a dismissed one stays dismissed until a newer one is served", async () => {
  const b = box("b1", ["b1"])
  const m = newbuild.machine(stamped("b1"), b.fx)
  await m.check()
  assert.deepEqual(b.lines, [], "the build this page runs offers nothing")
  b.deploy("b2", "0.18.0")
  await m.check()
  assert.deepEqual(b.lines.at(-1), { type: "Ready", version: "0.18.0" })
  m.acts.dismiss()
  assert.deepEqual(b.lines.at(-1), { type: "None" })
  await m.check()
  assert.equal(b.lines.length, 2, "dismissed, the same build is not offered again")
  b.deploy("b3", "0.19.0")
  await m.check()
  assert.deepEqual(b.lines.at(-1), { type: "Ready", version: "0.19.0" })
  b.deploy("b1")
  await m.check()
  assert.deepEqual(b.lines.at(-1), { type: "None" }, "a build taken back is no longer offered")
})

test("an answer that names no build is no answer", async () => {
  const b = box("", ["b1"])
  const m = newbuild.machine(stamped("b1"), b.fx)
  await m.check()
  assert.deepEqual(b.lines, [])
})

test("law: a page says which build it started on once per build, and a device's first build is not news", async () => {
  const device: string[] = []
  const first = box("b1", device)
  await newbuild.machine(stamped("b1"), first.fx).check()
  assert.deepEqual(first.lines, [], "first build")
  const next = box("b2", device)
  next.deploy("b2", "0.18.0")
  const m = newbuild.machine(stamped("b2"), next.fx)
  await m.check()
  assert.deepEqual(next.lines, [{ type: "Updated", version: "0.18.0", revision: "b2" }])
  await m.check()
  m.acts.dismiss()
  assert.deepEqual(next.lines, [{ type: "Updated", version: "0.18.0", revision: "b2" }, { type: "None" }], "said once, dismissed once")
  const again = box("b2", device)
  await newbuild.machine(stamped("b2"), again.fx).check()
  assert.deepEqual(again.lines, [], "a second tab, or a reload, says nothing")
})

test("Reload waits for the writes on their way", async () => {
  const b = box("b1", ["b1"])
  const m = newbuild.machine(stamped("b1"), b.fx)
  let done = (): void => {}
  void inflight.held(new Promise<void>(ok => (done = ok)))
  m.acts.reload()
  await settle()
  assert.equal(b.reloads(), 0, "a write on its way")
  done()
  await settle()
  await settle()
  assert.equal(b.reloads(), 1)
})
