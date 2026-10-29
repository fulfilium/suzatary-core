// The core's worker against the core read in this thread: the questions go by message over a channel to the same
// `serve` the worker runs, and each answer is the in-thread core's for the same objects, over generated holdings that
// grow, shrink and move between chains. A reading kept by the set and the instant is never folded again.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import type { Core, Named, Todos } from "../src/prodrome/core"
import { chainsOf, entriesOf, foldObjects, linked, linker, linkerOf, objectsOf, verifyObjects } from "../src/prodrome/core"
import lo from "../src/algebra/lo"
import questions from "../src/prodrome/questions"
import remote from "../src/prodrome/remote"
import thirty from "../src/todo/thirty"
import time from "../src/app/time"
import type { SpecNode } from "../src/wire/generated/wire"
import { forAllAsync } from "./gen"
import { heldChain } from "./held"

const built = process.env["SUZATARY_PRODROME_WASM"]
if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
const core = createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
const chain = JSON.parse(readFileSync("app/core/tests/web/chain.json", "utf8")) as { links: Named[]; untrusted: string[] }

// The objects in causal order, so every prefix is a down-set a replica could hold.
const ordered: Named[] = (() => {
  const by = new Map(chain.links.map(l => [l.hash, l]))
  return verifyObjects(core, chain.links).linearisation.flatMap(h => by.get(h) ?? [])
})()
const todos = [...new Set(ordered.flatMap(o => /todo='([^']+)'/.exec(o.literal)?.[1] ?? []))]

// A core whose folds are counted.
const counted = (): { core: Core; folds: () => number } => {
  let n = 0
  const Base = core.Todos
  const Counting = function (objects: string): Todos {
    const t = new Base(objects)
    const fold = t.fold.bind(t)
    t.fold = (at, untrusted) => ((n += 1), fold(at, untrusted))
    return t
  }
  return { core: { ...core, Todos: Counting as unknown as Core["Todos"] }, folds: () => n }
}

// The worker's side and the tab's, joined by a channel, which each test closes so the run can end.
const channel = async (c: Core, check: (r: remote.Remote) => Promise<void>): Promise<void> => {
  const { port1, port2 } = new MessageChannel()
  questions.listen(port1, Promise.resolve(questions.serve(c)))
  const r = remote.over(port2)
  port1.start()
  port2.start()
  try {
    await check(r)
  } finally {
    port1.close()
    port2.close()
  }
}

const instant = (n: number): string => time.instant(time.parse("2026-08-20T00:00:00") + n * 3600)

test("law: an answer by message is the in-thread core's answer, as holdings grow and shrink and chains come and go", () =>
  channel(core, r => forAllAsync(25, async rg => {
    const life = ordered.slice(0, 1 + rg.int(ordered.length))
    // Another chain, holding a different down-set: read at the same instant, it is its own reading.
    const otherSize = rg.int(ordered.length)
    const other = rg.bool() ? [{ name: "other", objects: ordered.slice(0, otherSize === life.length ? otherSize - 1 : otherSize), untrusted: [] }] : []
    const held = [{ name: "life", objects: life, untrusted: chain.untrusted }, ...other.filter(c => c.objects.length > 0)]
    await r.hold(held)
    const at = instant(rg.int(24 * 40))
    const set = chainsOf(core, held.map(c => heldChain(c.name, c.objects, c.untrusted)))
    try {
      assert.deepEqual(await r.ask({ type: "List", chain: "life", at }, "Page"), entriesOf(core, life, at, chain.untrusted, "life", set))
      const read = await r.ask({ type: "Reading", chain: "life", at }, "Page")
      const f = foldObjects(core, life, at, chain.untrusted)
      assert.equal(read.type, "Read")
      if (read.type !== "Read") return
      assert.deepEqual([read.at, read.specs, read.functions], [f.at, f.specs, f.functions])
      for (const o of other.filter(c => c.objects.length > 0)) {
        const theirs = await r.ask({ type: "Reading", chain: o.name, at }, "Page")
        const g = foldObjects(core, o.objects, at, o.untrusted)
        assert.equal(theirs.type, "Read")
        if (theirs.type === "Read") assert.deepEqual([theirs.reading.chain, theirs.specs, theirs.functions], [o.name, g.specs, g.functions], "two chains at one instant are two readings")
      }
      const terms: SpecNode[] = rg.array(1 + rg.int(4), () => {
        const refs = rg.array(1 + rg.int(3), (): SpecNode => ({ kind: "ref", todo: rg.pick([...todos, "nosuchtodo"]) }))
        return refs.length === 1 && refs[0] !== undefined ? refs[0] : { kind: "conj", p: -4, terms: refs }
      })
      const linkSet = linkerOf(set, f.functions, f.at)
      const price = linked(core, linker(t => linkSet.link(t)), f.env, f.at)
      const direct = terms.map((t): questions.Priced => {
        try {
          return { type: "Value", v: price(t) }
        } catch (e) {
          return { type: "Refused", why: e instanceof Error ? e.message : String(e) }
        }
      })
      linkSet.free()
      assert.deepEqual(await r.ask({ type: "Price", basis: { type: "Fold", reading: read.reading }, terms }, "Page"), direct)
      const id = rg.pick(todos)
      const ms = time.ms(at) ?? 0
      const t: Todos = new core.Todos(objectsOf(life))
      assert.deepEqual(await r.ask({ type: "Thirty", chain: "life", id, ms }, "Page"), thirty.sample(core, set, t, chain.untrusted, id, ms))
      t.free()
    } finally {
      set.free()
    }
  })))

test("two chains held and read at one instant are two readings, each its own chain's", () =>
  channel(core, async r => {
    const few = ordered.filter(o => o.literal.startsWith("Genesis(") || o.literal.includes(`todo='${todos[0] ?? ""}'`))
    await r.hold([
      { name: "life", objects: ordered, untrusted: chain.untrusted },
      { name: "work", objects: few, untrusted: chain.untrusted },
    ])
    const at = instant(5)
    const life = await r.ask({ type: "Reading", chain: "life", at }, "Page")
    const work = await r.ask({ type: "Reading", chain: "work", at }, "Page")
    assert.ok(life.type === "Read" && work.type === "Read")
    assert.deepEqual(work.specs, foldObjects(core, few, at, chain.untrusted).specs)
    assert.notDeepEqual(work.specs, life.specs, "work's reading is not life's")
    assert.equal(questions.same(life.reading, work.reading), false)
  }))

test("a holding sends each object once: a second holding of the same chains sends no object", () => {
  const sent: questions.Asked[] = []
  const r = remote.over({ postMessage: m => void sent.push(m), addEventListener: () => {} })
  const held = [{ name: "life", objects: ordered, untrusted: chain.untrusted }]
  void r.hold(held)
  void r.hold(held)
  void r.hold([{ name: "life", objects: ordered.slice(0, 3), untrusted: chain.untrusted }])
  const holdings = sent.map(m => (m.question.type === "Hold" ? m.question.chains[0] : undefined))
  assert.equal(holdings[0]?.add.length, ordered.length)
  assert.deepEqual([holdings[1]?.add, holdings[1]?.drop], [[], []])
  assert.deepEqual([holdings[2]?.add, holdings[2]?.drop.length], [[], ordered.length - 3])
})

test("a holding the worker refused is sent again whole, and the worker holds what it names and nothing else", () =>
  channel(core, async r => {
    // A first port that refuses the holding: what the worker took of it is unknown, so the next is whole.
    const sent: questions.Asked[] = []
    let answer: (m: questions.Answered) => void = () => {}
    const refusing = remote.over({
      postMessage: m => void sent.push(m),
      addEventListener: (_, f) => void (answer = m => f({ data: m })),
    })
    const held = [{ name: "life", objects: ordered, untrusted: chain.untrusted }]
    const first = refusing.hold(held)
    answer({ id: 1, answer: lo.error("refused") })
    await assert.rejects(first)
    void refusing.hold([{ name: "life", objects: ordered.slice(0, 3), untrusted: chain.untrusted }])
    const again = sent[1]?.question
    assert.ok(again?.type === "Hold")
    assert.deepEqual([again.chains[0]?.whole, again.chains[0]?.add.length, again.chains[0]?.drop], [true, 3, []])
    // A worker holding more than the whole names lets the rest go.
    await r.hold(held)
    const at = instant(5)
    await r.ask({ type: "Hold", chains: again.chains }, "Page")
    const read = await r.ask({ type: "Reading", chain: "life", at }, "Page")
    assert.ok(read.type === "Read")
    assert.deepEqual(read.specs, foldObjects(core, ordered.slice(0, 3), at, chain.untrusted).specs)
  }))

test("a remount does not fold: the same chains at the same instant read the reading kept", () => {
  const { core: c, folds } = counted()
  return channel(c, async r => {
    const held = [{ name: "life", objects: ordered, untrusted: chain.untrusted }]
    const at = instant(10)
    const mount = async () => (await r.hold(held), await r.ask({ type: "Reading", chain: "life", at }, "Page"))
    const first = await mount()
    assert.equal(folds(), 1)
    assert.deepEqual(await mount(), first, "the page mounted again reads the same reading")
    assert.equal(folds(), 1, "and nothing was folded for it")
    await r.hold([{ name: "life", objects: ordered.slice(0, -1), untrusted: chain.untrusted }])
    await r.ask({ type: "Reading", chain: "life", at }, "Page")
    assert.equal(folds(), 2, "a chain that moved is folded again")
  })
})

test("the box's rows price a group through the core, a todo as its row", () =>
  channel(core, async r => {
    const functions: Record<string, SpecNode> = { a: { kind: "flat", value: 0.5 }, b: { kind: "absent" } }
    const terms: SpecNode[] = [{ kind: "ref", todo: "a" }, { kind: "conj", p: -4, terms: [{ kind: "ref", todo: "a" }, { kind: "ref", todo: "a" }] }, { kind: "ref", todo: "c" }]
    const [a, group, unknown] = await r.ask({ type: "Price", basis: { type: "Rows", at: instant(0), functions }, terms }, "Page")
    assert.deepEqual(a, { type: "Value", v: 0.5 })
    assert.deepEqual(group, { type: "Value", v: 0.5 })
    assert.equal(unknown?.type, "Refused")
  }))

test("a worker that is gone fails every question waiting and every one after, in words", async () => {
  let fail: (why: string) => void = () => {}
  const r = remote.over({ postMessage: () => {}, addEventListener: () => {} }, f => (fail = f))
  const waiting = r.ask({ type: "List", chain: "life", at: null }, "Page")
  fail("the core's worker stopped")
  await assert.rejects(waiting, /stopped/)
  await assert.rejects(r.ask({ type: "List", chain: "life", at: null }, "Page"), /stopped/)
})

test("a question about a chain the worker does not hold says so", () =>
  channel(core, async r => {
    await r.hold([])
    await assert.rejects(r.ask({ type: "List", chain: "life", at: null }, "Page"), /holds nothing of life/)
    assert.deepEqual(await r.ask({ type: "Reading", chain: "life", at: instant(0) }, "Page"), { type: "Unheld" })
  }))
