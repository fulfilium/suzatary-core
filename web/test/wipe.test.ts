// wipe's law: a revoked device seals its databases, deletes every one it holds and every cache, forgets its storage and
// reloads, once; a wipe a reload cuts short at any step is finished by the next load; a database that would not go keeps
// the mark for the next load; and every other tab of the device hears the mark and wipes too.

import { test } from "node:test"
import assert from "node:assert/strict"

import wipe from "../src/app/wipe"
import type signer from "../src/prodrome/signer"
import { SLUG, forAll } from "./gen"
import type { Rng } from "./gen"

type Seen = { dropped: string[]; uncached: string[]; forgot: number; reloaded: number; sealed: number; sealedFirst: boolean; marked: boolean; stuck: number }

const KEY = "a".repeat(64)

const effects = (listed: readonly string[], cached: readonly string[], failing: ReadonlySet<string>, kept: signer.Kept = { type: "Kept", key: KEY }) => {
  const seen: Seen = { dropped: [], uncached: [], forgot: 0, reloaded: 0, sealed: 0, sealedFirst: false, marked: false, stuck: 0 }
  const fx: wipe.Effects = {
    seal: () => void ((seen.sealed += 1), (seen.sealedFirst = seen.dropped.length === 0 && seen.uncached.length === 0 && seen.forgot === 0)),
    databases: () => Promise.resolve(listed),
    drop: n => (failing.has(n) ? Promise.reject(new Error("blocked")) : (seen.dropped.push(n), Promise.resolve())),
    caches: () => Promise.resolve(cached),
    uncache: n => (seen.uncached.push(n), Promise.resolve()),
    forget: () => void ((seen.forgot += 1), (seen.marked = false)),
    mark: () => void (seen.marked = true),
    marked: () => (seen.marked ? { type: "Begun" } : { type: "Clear" }),
    reload: () => void seen.reloaded++,
    signing: () => ({ type: "Held", key: KEY }),
    kept: () => Promise.resolve(kept),
    stuck: () => void seen.stuck++,
  }
  return { fx, seen }
}

test("law: every database it holds and every cache go, a failing one stops none and keeps the mark, and it reloads after", async () => {
  const runs: Promise<void>[] = []
  forAll(200, r => {
    const names = r.array(r.int(5), () => r.string(SLUG, 1 + r.int(8)))
    const cached = r.array(r.int(3), () => r.string(SLUG, 1 + r.int(8)))
    const failing = new Set(names.filter(() => r.bool()))
    const { fx, seen } = effects(names, cached, failing)
    runs.push(
      wipe.all(fx, "Gone").then(() => {
        const every = new Set([...wipe.known(), ...names])
        assert.deepEqual(new Set(seen.dropped), new Set([...every].filter(n => !failing.has(n))))
        assert.deepEqual(new Set(seen.uncached), new Set(cached))
        assert.equal(seen.forgot, 2, "the storage is cleared before the first deletion and after the last")
        assert.equal(seen.marked, failing.size > 0, "the mark stays exactly when a database would not go")
        assert.equal(seen.reloaded, 1)
        assert.equal(seen.sealedFirst, true, "no database opens again once the first is deleted")
      }),
    )
  })
  await Promise.all(runs)
})

// A browser that outlives its pages: what it holds, and the databases that will not go while it is so.
type Browser = { databases: Set<string>; caches: Set<string>; storage: Map<string, string>; failing: Set<string>; reloads: number; stuck: number }

const copy = (b: Browser): Browser => ({
  databases: new Set(b.databases),
  caches: new Set(b.caches),
  storage: new Map(b.storage),
  failing: new Set(b.failing),
  reloads: b.reloads,
  stuck: b.stuck,
})

// One page of `b`, stopped by a reload after `cut` of its effects have run: from then on it does nothing, and a wait it
// is in never ends. A deletion asked before the cut still happens, as a browser's does when its page goes. `ran` counts
// the effects the page ran.
const page = (b: Browser, cut: number) => {
  const count = { ran: 0 }
  const live = (): boolean => (count.ran += 1) <= cut
  const never = new Promise<never>(() => {})
  const fx: wipe.Effects = {
    seal: () => void live(),
    databases: () => (live() ? Promise.resolve([...b.databases]) : never),
    drop: n => (!live() ? never : b.failing.has(n) ? Promise.reject(new Error("blocked")) : (b.databases.delete(n), Promise.resolve())),
    caches: () => (live() ? Promise.resolve([...b.caches]) : never),
    uncache: n => (live() ? (b.caches.delete(n), Promise.resolve()) : never),
    forget: () => void (live() && b.storage.clear()),
    mark: () => void (live() && b.storage.set(wipe.MARK, "1")),
    marked: () => wipe.markOf(b.storage.get(wipe.MARK) ?? null),
    reload: () => void (live() && (b.reloads += 1)),
    signing: () => ({ type: "None" }),
    kept: () => Promise.resolve({ type: "Absent" }),
    stuck: () => void (live() && (b.stuck += 1)),
  }
  return { fx, count }
}

const settle = () => new Promise(resolve => setTimeout(resolve, 0))

const browserOf = (r: Rng): Browser => ({
  databases: new Set([...wipe.known(), ...r.array(r.int(4), () => r.string(SLUG, 1 + r.int(8)))]),
  caches: new Set(r.array(r.int(3), () => r.string(SLUG, 1 + r.int(8)))),
  storage: new Map([["suzatary.e2e.kept", "1"], ...r.array(r.int(3), (): [string, string] => [r.string(SLUG, 4), "x"])]),
  failing: new Set(),
  reloads: 0,
  stuck: 0,
})

test("law: a wipe a reload cuts short at every step is finished by the next load, storage and all", async () => {
  const browsers: Browser[] = []
  forAll(60, r => void browsers.push(browserOf(r)))
  for (const b of browsers) {
    // How many effects a whole wipe of this browser runs.
    const whole = page(copy(b), Infinity)
    await wipe.all(whole.fx, "Gone")
    // Seal, forget and mark run in one task, before the first wait, and a reload falls only between tasks.
    for (let cut = 3; cut <= whole.count.ran; cut += 1) {
      const held = copy(b)
      void wipe.all(page(held, cut).fx, "Gone")
      await settle()
      // The load after it, with nothing to stop it, finishes what is marked; a page with no mark wipes nothing.
      const next = page(held, Infinity).fx
      if (next.marked().type === "Begun") await wipe.all(next, "Resumed")
      assert.deepEqual([...held.databases], [], `cut after ${cut}: a database is left`)
      assert.deepEqual([...held.caches], [], `cut after ${cut}: a cache is left`)
      assert.deepEqual([...held.storage.keys()], [], `cut after ${cut}: the storage is kept`)
    }
  }
})

test("law: a database that will not go keeps the mark, a resumed wipe that fails again stays sealed, and the load after the failure clears finishes it", async () => {
  const browsers: Browser[] = []
  forAll(60, r => {
    const b = browserOf(r)
    const names = [...b.databases]
    b.failing = new Set([names[r.int(names.length)] ?? "suzatary"])
    browsers.push(b)
  })
  for (const b of browsers) {
    await wipe.all(page(b, Infinity).fx, "Gone")
    assert.equal(b.reloads, 1, "the revoked page loads afresh")
    assert.deepEqual(wipe.markOf(b.storage.get(wipe.MARK) ?? null), { type: "Begun" }, "the mark stays for the next load")
    await wipe.all(page(b, Infinity).fx, "Resumed")
    assert.equal(b.reloads, 1, "a resumed wipe that fails again does not reload into the same failure")
    assert.equal(b.stuck, 1, "and the page says it could not delete what it holds")
    assert.deepEqual([...b.storage.keys()], [wipe.MARK], "and keeps nothing but the mark")
    b.failing.clear()
    await wipe.all(page(b, Infinity).fx, "Resumed")
    assert.deepEqual([...b.databases], [])
    assert.deepEqual([...b.storage.keys()], [], "the mark goes with the last database")
    assert.equal(b.reloads, 2)
  }
})

test("law: a page told of another tab's mark while it is set seals first and wipes, with no resume before it", async () => {
  const runs: Promise<void>[] = []
  forAll(100, r => {
    const names = r.array(r.int(4), () => r.string(SLUG, 1 + r.int(8)))
    const { fx, seen } = effects(names, [], new Set())
    fx.mark()
    wipe.page(fx).heard()
    runs.push(
      settle().then(() => {
        assert.equal(seen.sealedFirst, true, "sealed before anything is cleared or deleted")
        assert.deepEqual(new Set(seen.dropped), new Set([...wipe.known(), ...names]))
        assert.equal(seen.reloaded, 1)
      }),
    )
  })
  await Promise.all(runs)
})

test("law: a page told after the mark cleared, a frozen or cached tab hearing it late, deletes nothing and reloads once, sealed", async () => {
  const { fx, seen } = effects(["suzatary", "suzatary-keys"], ["shell-1"], new Set())
  const told = wipe.page(fx)
  told.heard()
  told.heard()
  told.gone()
  await settle()
  assert.deepEqual(seen.dropped, [], "the new device's databases stay")
  assert.deepEqual(seen.uncached, [])
  assert.equal(seen.forgot, 0, "and its storage")
  assert.equal(seen.sealed, 1, "this page's replica of the old device is sealed in it")
  assert.equal(seen.reloaded, 1)
})

test("law: a page's key was replaced exactly when it signs with a key this device no longer keeps", () => {
  forAll(200, r => {
    const key = (): string => r.pick(["a", "b", "c"]).repeat(64)
    const here: signer.Here = r.bool(0.2) ? { type: "None" } : { type: "Held", key: key() }
    const kept: signer.Kept = r.bool(0.2) ? { type: "Absent" } : { type: "Kept", key: key() }
    const expected = here.type === "Held" && !(kept.type === "Kept" && kept.key === here.key)
    assert.equal(wipe.replaced(here, kept), expected)
  })
})

test("a page shown again from the back and forward cache wipes while the mark is set, reloads sealed if its key was replaced, and otherwise keeps its place", async () => {
  const same = effects(["suzatary"], [], new Set())
  await wipe.page(same.fx).shown()
  assert.equal(same.seen.sealed + same.seen.reloaded, 0, "a normal restore never reloads")

  const replaced = effects(["suzatary"], ["shell-1"], new Set(), { type: "Kept", key: "b".repeat(64) })
  await wipe.page(replaced.fx).shown()
  assert.deepEqual(replaced.seen.dropped, [], "the new device's databases stay")
  assert.equal(replaced.seen.forgot, 0, "and its storage")
  assert.equal(replaced.seen.sealed, 1, "the old device's replica in this page is sealed")
  assert.equal(replaced.seen.reloaded, 1)

  const none = effects(["suzatary"], [], new Set(), { type: "Absent" })
  await wipe.page(none.fx).shown()
  assert.equal(none.seen.reloaded, 1, "a key gone with nothing in its place was replaced too")

  const marked = effects([], [], new Set())
  marked.fx.mark()
  await wipe.page(marked.fx).shown()
  await settle()
  assert.equal(marked.seen.reloaded, 1)
  assert.ok(marked.seen.forgot > 0, "a marked wipe runs")
})

test("a page wipes once: a load with no mark wipes nothing, a marked load wipes, and a told page or a second answer reloads nothing more", async () => {
  const unmarked = effects([], [], new Set())
  wipe.page(unmarked.fx).resume()
  await settle()
  assert.equal(unmarked.seen.reloaded, 0)
  const { fx, seen } = effects([], [], new Set())
  fx.mark()
  const p = wipe.page(fx)
  p.resume()
  p.heard()
  p.gone()
  await settle()
  assert.equal(seen.reloaded, 1)
})

test("it deletes the replica, the rendered text and the key, whatever the browser lists", () => {
  assert.deepEqual([...wipe.known()].sort(), ["suzatary", "suzatary-keys", "suzatary-rendered"])
})

test("the lines' row, its keys among it, its snapshot and the rendered blocks are in databases a wipe deletes even unlisted", async () => {
  const { DB_NAME, SNAPSHOTS } = await import("../src/prodrome/objects")
  const rendered = (await import("../src/app/rendered")).default
  assert.ok(wipe.known().includes(DB_NAME), "every store's row, objects and keys, is a row of the replica's database")
  assert.ok(wipe.known().includes(SNAPSHOTS.name), "and so is every snapshot, which holds the keys")
  assert.ok(wipe.known().includes(rendered.DB))
  const { fx, seen } = effects([], [], new Set())
  await wipe.all(fx, "Gone")
  assert.ok(seen.dropped.includes(DB_NAME) && seen.dropped.includes(rendered.DB), "a browser that lists no database still deletes them")
})
