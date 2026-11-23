// A store's pull by pieces against a box that answers as `routes::store_objects_since` does: what a replica holding
// the frontier it names lacks, parents first, cut to a budget of literal bytes, naming the frontier the asker holds
// after it. A pull the network cuts keeps the pieces that came, and the next asks only for the rest. The tab's replica
// is the core's, a chain's store of objects, as the worker holds it.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import type { Core, Named } from "../src/prodrome/core"
import { verifyObjects } from "../src/prodrome/core"
import pieces from "../src/prodrome/pieces"
import type { StorePayload, StoredObject } from "../src/wire/generated/wire"
import { forAllAsync } from "./gen"
import { headsOf, held as prodromes, restsOn } from "./held"
import d from "../src/wire/decoder"

const built = process.env["SUZATARY_PRODROME_WASM"]
if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
const core = createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
const chain = JSON.parse(readFileSync("app/api/tests/web/chain.json", "utf8")) as { links: Named[] }
// The box's objects parents first, as the box serves them.
const all: StoredObject[] = (() => {
  const by = new Map(chain.links.map(l => [l.hash, l]))
  return verifyObjects(core, chain.links).linearisation.flatMap(h => by.get(h) ?? [])
})()

// Everything `since` rests on, itself included, by one pass back over the box's parents-first order.
const closure = (since: readonly string[]): Set<string> => {
  const held = new Set<string>()
  const wanted = new Set(since)
  for (const o of [...all].reverse()) {
    if (!wanted.has(o.hash)) continue
    held.add(o.hash)
    for (const p of restsOn(o)) wanted.add(p)
  }
  return held
}

// The frontiers the box recorded, each a down-set of what it holds under a digest of its own.
// Named by a count that never goes back, as a digest names one set and no other, whatever the box forgot.
const recorded = new Map<string, Set<string>>()
let named = 0
const record = (have: Set<string>): string => {
  const key = [...have].sort().join(",")
  const found = [...recorded].find(([, set]) => [...set].sort().join(",") === key)
  if (found !== undefined) return found[0]
  named += 1
  const digest = named.toString(16).padStart(64, "0")
  recorded.set(digest, have)
  return digest
}

// The box's answer: what the frontier `since` names lacks, the longest run whose literals fit `budget`, never less than
// one object, and the frontier the asker holds once it has taken them.
const answer = (budget: number) => (since: string): StorePayload => {
  const known = since === "" ? new Set<string>() : recorded.get(since)
  const held = closure([...(known ?? [])])
  const missing = all.filter(o => !held.has(o.hash))
  const objects: StoredObject[] = []
  let bytes = 0
  for (const o of missing) {
    bytes += o.literal.length
    if (bytes > budget && objects.length > 0) break
    objects.push(o)
  }
  const have = new Set([...held, ...objects.map(o => o.hash)])
  const frontier = { digest: record(have), count: have.size }
  return { objects, heads: headsOf(all), untrusted: [], root: null, geneses: prodromes(all), epoch: 1, since: known === undefined ? "Unknown" : "Known", frontier, keys: {}, rest: missing.length - objects.length, size: missing.reduce((n, o) => n + o.literal.length, 0), weight: objects.reduce((n, o) => n + o.literal.length, 0), erased: [] }
}

// A tab's store as its engine holds it: the core's replica, which keeps each piece beside the landed row readings read
// and lands them in one step at the end. Every move it made is a frame, which another tab of the device reads back.
const Linearised = d.json(d.obj<{ linearisation: string[] }>({ linearisation: d.arr(d.str) }))
const tab = (ask: (since: string) => Promise<StorePayload>) => {
  const store = new core.Chain()
  const frames: Uint8Array[] = []
  const asked: string[] = []
  const counts = { lands: 0, shown: 0, written: 0 }
  const kept = (step: { readonly delta: Uint8Array; readonly moved: string; readonly free: () => void }): string => {
    frames.push(step.delta)
    const moved = step.moved
    step.free()
    return moved
  }
  // What the names a set of heads rests on are, read off the box's objects: a replica holds a down-set.
  const under = (names: readonly string[]): string[] => {
    const wanted = new Set(names)
    for (const o of [...all].reverse()) if (wanted.has(o.hash)) for (const p of restsOn(o)) wanted.add(p)
    return all.filter(o => wanted.has(o.hash)).map(o => o.hash)
  }
  const fx: pieces.Effects<StorePayload> = {
    ask: () => {
      const since = store.since()
      asked.push(since)
      return ask(since)
    },
    keep: async a => {
      frames.push(store.piece(JSON.stringify(a), 1))
      counts.written += a.objects.reduce((n, o) => n + o.literal.length, 0)
    },
    land: async () => {
      counts.lands += 1
      if (kept(store.land()) !== "[]") counts.shown += 1
    },
    held: () => [store.since(), store.heads()],
  }
  return {
    fx,
    asked,
    counts,
    frames,
    // What another tab kept of its pull comes back with the device's log, each frame a join.
    join: (theirs: readonly Uint8Array[]): void => theirs.forEach(f => void store.apply(f)),
    behind: (box: readonly string[]): boolean => store.behind(JSON.stringify(box)),
    held: () => under(JSON.parse(store.heads()) as string[]),
    shown: (): string[] => {
      const t = store.todos()
      try {
        const read = Linearised(t.verify())
        return read.type === "Val" ? read.v.linearisation : []
      } finally {
        t.free()
      }
    },
  }
}

const BUDGET = Math.ceil(all.reduce((n, o) => n + o.literal.length, 0) / 5)

test("a pull taken whole asks piece after piece, each from the frontier the last named, and ends holding the box", async () => {
  const one = tab(since => Promise.resolve(answer(BUDGET)(since)))
  const ended = await pieces.pull(one.fx)
  assert.equal(ended.type, "Whole")
  assert.ok(ended.taken >= 3, `${ended.taken} pieces`)
  assert.deepEqual(new Set(one.held()), new Set(all.map(o => o.hash)))
  assert.equal(one.asked[0], "")
})

test("a pull interrupted after k pieces resumes at piece k+1", async () => {
  // The pieces of a pull nobody cuts, in order.
  const whole: StorePayload[] = []
  const once = tab(since => {
    const a = answer(BUDGET)(since)
    whole.push(a)
    return Promise.resolve(a)
  })
  await pieces.pull(once.fx)
  for (let k = 0; k < whole.length; k += 1) {
    // The network answers k asks, then goes until it is back.
    let answers = k
    const cut = tab(since => (answers-- > 0 ? Promise.resolve(answer(BUDGET)(since)) : Promise.reject(new Error("the network went"))))
    const first = await pieces.pull(cut.fx)
    assert.deepEqual(first, { type: "Cut", taken: k, e: new Error("the network went") })
    // What arrived is kept, the first k pieces and nothing else, and none of it is shown until the pull is whole.
    assert.deepEqual(new Set(cut.held()), new Set(whole.slice(0, k).flatMap(p => p.objects.map(o => o.hash))))
    assert.deepEqual(cut.shown(), [])
    answers = Infinity
    let next: StorePayload | null = null
    const resumed = await pieces.pull({
      ...cut.fx,
      ask: () =>
        cut.fx.ask().then(a => {
          next ??= a
          return a
        }),
    })
    // The same box, asked again by the tab that was cut, once the network is back.
    assert.equal(resumed.type, "Whole")
    assert.deepEqual(next, whole[k])
    assert.deepEqual(new Set(cut.held()), new Set(all.map(o => o.hash)))
  }
})

test("law: every piece taken leaves the tab holding the ancestors of all it holds", async () => {
  await forAllAsync(20, async r => {
    const budget = 1 + r.int(BUDGET)
    const seen: string[][] = []
    const one = tab(since => Promise.resolve(answer(budget)(since)))
    const keep = one.fx.keep
    const ended = await pieces.pull({
      ...one.fx,
      keep: async a => {
        await keep(a)
        seen.push(one.held())
      },
    })
    assert.deepEqual(ended.type, "Whole", JSON.stringify(ended))
    for (const held of seen) {
      const names = new Set(held)
      for (const o of all.filter(o => names.has(o.hash))) assert.ok(restsOn(o).every(p => names.has(p)), `${o.hash} without what it rests on`)
    }
  })
})

test("a box that answers a piece the tab already holds stalls the pull rather than asking forever", async () => {
  const one = tab(() => Promise.resolve({ ...answer(BUDGET)(""), objects: [], rest: 3 }))
  const ended = await pieces.pull(one.fx)
  assert.equal(ended.type, "Stalled")
  assert.ok(one.asked.length <= 2, `${one.asked.length} asks`)
})

test("a second pull after a whole one asks once and is answered with nothing", async () => {
  const one = tab(since => Promise.resolve(answer(BUDGET)(since)))
  await pieces.pull(one.fx)
  const asked = one.asked.length
  const again = await pieces.pull(one.fx)
  assert.deepEqual(again, { type: "Whole", taken: 1 })
  assert.equal(one.asked.length, asked + 1)
  assert.deepEqual(answer(BUDGET)(one.asked.at(-1) ?? "").objects, [])
})

test("two tabs cut at different pieces join into a row the next pull finishes, sending nothing the tab whose frontier stands held", async () => {
  // Each tab's network answers so many asks, then is offline until it is back.
  const left = [1, 2]
  const cutAt = async (i: number) => {
    const t = tab(since => {
      if ((left[i] ?? 0) <= 0) return Promise.reject(new Error("offline"))
      left[i] = (left[i] ?? 0) - 1
      return Promise.resolve(answer(BUDGET)(since))
    })
    await pieces.pull(t.fx)
    return t
  }
  const [a, b] = await Promise.all([cutAt(0), cutAt(1)])
  left.fill(Infinity)
  // The joined row names the later of the two frontiers, the one reaching further: the next pull may send again what
  // only the other tab held, and never what the one whose frontier stands held.
  const stands = [a, b].reduce((x, y) => (y.held().length > x.held().length ? y : x))
  const standing = new Set(stands.held())
  a.join(b.frames)
  const before = new Set(a.held())
  for (const o of all.filter(o => before.has(o.hash))) assert.ok(restsOn(o).every(p => before.has(p)), "the join is a down-set")
  const sent: string[] = []
  const ended = await pieces.pull({
    ...a.fx,
    ask: () =>
      a.fx.ask().then(p => {
        sent.push(...p.objects.map(o => o.hash))
        return p
      }),
  })
  assert.equal(ended.type, "Whole")
  assert.ok(sent.every(h => !standing.has(h)), "nothing the tab whose frontier stands held is sent again")
  assert.deepEqual(new Set(a.held()), new Set(all.map(o => o.hash)))
})

test("a many-piece pull lands once and writes each object once: one change for the pages, no store rewritten per piece", async () => {
  const total = all.reduce((n, o) => n + o.literal.length, 0)
  const one = tab(since => Promise.resolve(answer(1)(since)))
  const ended = await pieces.pull(one.fx)
  assert.deepEqual(ended, { type: "Whole", taken: all.length })
  assert.equal(one.counts.lands, 1)
  assert.equal(one.counts.shown, 1, "the pages read the store once, whole")
  assert.equal(one.counts.written, total, "each piece written by itself, every object once")
})

test("a frame naming what the tab does not hold takes a cut pull up again, and one it holds asks nothing", async () => {
  let answers = 2
  const one = tab(since => (answers-- > 0 ? Promise.resolve(answer(BUDGET)(since)) : Promise.reject(new Error("offline"))))
  assert.equal((await pieces.pull(one.fx)).type, "Cut")
  // The box's heads, as its stream says them: the tab still lacks objects they name.
  const frame = headsOf(all)
  assert.equal(one.behind(frame), true)
  answers = Infinity
  assert.equal((await pieces.pull(one.fx)).type, "Whole")
  assert.equal(one.behind(frame), false)
})

test("a box that lost its records answers the next pull from nothing once, and the pull ends; twice over", async () => {
  const one = tab(since => Promise.resolve(answer(BUDGET)(since)))
  assert.equal((await pieces.pull(one.fx)).type, "Whole")
  for (const loss of [1, 2]) {
    recorded.clear()
    const from = one.asked.length
    const ended = await pieces.pull(one.fx)
    const asks = one.asked.slice(from)
    assert.equal(ended.type, "Whole", `after loss ${loss}: ${JSON.stringify(ended)}`)
    // The first ask names the frontier the box forgot; every later one a frontier it named after, each asked once.
    assert.equal(new Set(asks).size, asks.length, `after loss ${loss}, a frontier asked twice: ${asks.map(a => a.slice(-4)).join(" ")}`)
    assert.ok(asks.length <= Math.ceil(all.length / 1) + 1, `${asks.length} asks`)
    assert.deepEqual(new Set(one.held()), new Set(all.map(o => o.hash)))
    // And the next pull stands where the last answer left it: one ask, answered with nothing.
    const quiet = one.asked.length
    const q = await pieces.pull(one.fx)
    assert.deepEqual(q, { type: "Whole", taken: 1 })
    assert.equal(one.asked.length, quiet + 1)
    assert.deepEqual(answer(BUDGET)(one.asked.at(-1) ?? "").objects, [])
  }
})
