// The core's worker against the core read in this thread: the questions go by message over a channel to the same
// `serve` the worker runs, which holds each chain as the box serves it (a fake box below), and each answer is the
// in-thread core's for the same objects, as the box's chains grow and the tab reads more of them. A reading kept by the
// set and the instant is never folded again.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import type { ChainStore, Core, Named, Todos } from "../src/prodrome/core"
import { entriesOf, foldObjects, linked, linker, linkerOf, objectsOf, verifyObjects } from "../src/prodrome/core"
import questions from "../src/prodrome/questions"
import remote from "../src/prodrome/remote"
import thirty from "../src/todo/thirty"
import time from "../src/app/time"
import type { SpecNode } from "../src/wire/generated/wire"
import { forAllAsync } from "./gen"
import { answerOf, heldChain, holdAll } from "./held"
import type text from "../src/wire/text"

// A reading the worker answers as text, read back as the page's module would.
const readOf = (t: text.Text<{ readonly read: questions.Read }>): questions.Read => (JSON.parse(new TextDecoder().decode(t.bytes)) as { read: questions.Read }).read

const built = process.env["SUZATARY_PRODROME_WASM"]
if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
const core = createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
const chain = JSON.parse(readFileSync("app/api/tests/web/chain.json", "utf8")) as { links: Named[]; untrusted: string[] }

// The objects in causal order, so every prefix is a down-set a replica could hold.
const ordered: Named[] = (() => {
  const by = new Map(chain.links.map(l => [l.hash, l]))
  return verifyObjects(core, chain.links).linearisation.flatMap(h => by.get(h) ?? [])
})()
const todos = [...new Set(ordered.flatMap(o => /todo='([^']+)'/.exec(o.literal)?.[1] ?? []))]

// What the box serves of each chain, by name: a GET of `/api/<chain>/objects` answers all it holds.
const served = new Map<string, { readonly objects: readonly Named[]; readonly untrusted: readonly string[] }>()
Object.assign(globalThis, {
  fetch: async (input: string | URL): Promise<Response> => {
    const name = /^\/api\/([a-z][a-z0-9-]*)\/objects$/.exec(new URL(String(input), "http://box").pathname)?.[1]
    const chain = name === undefined ? undefined : served.get(name)
    if (chain === undefined) return new Response("", { status: 404 })
    return new Response(JSON.stringify(answerOf(chain.objects, chain.untrusted)), { status: 200, headers: { "content-type": "application/json" } })
  },
})

// A core whose folds are counted.
const counted = (): { core: Core; folds: () => number } => {
  let n = 0
  const Base = core.Chain
  const Counting = function (): ChainStore {
    const c = new Base()
    const todos = c.todos.bind(c)
    c.todos = (): Todos => {
      const t = todos()
      const fold = t.fold.bind(t)
      t.fold = (at, untrusted) => ((n += 1), fold(at, untrusted))
      return t
    }
    return c
  }
  return { core: { ...core, Chain: Counting as unknown as Core["Chain"] }, folds: () => n }
}

// The box serving `chains`, and the worker told of them and pulled.
const serving = async (r: remote.Remote, chains: readonly { readonly name: string; readonly objects: readonly Named[]; readonly untrusted: readonly string[] }[]): Promise<void> => {
  served.clear()
  for (const c of chains) served.set(c.name, c)
  await r.ask({ type: "Served", chains: chains.map(c => c.name) }, "Page")
  for (const c of chains) await r.ask({ type: "Exchange", chain: c.name }, "Page")
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

test("law: an answer by message is the in-thread core's answer, as the box's chains grow and the tab reads more of them", () =>
  channel(core, r => {
    let grown = 0
    let otherGrown = 0
    return forAllAsync(25, async rg => {
    grown = Math.min(ordered.length, grown + 1 + rg.int(Math.ceil(ordered.length / 8)))
    const life = ordered.slice(0, grown)
    // Another chain, holding a different down-set that grows as well: read at the same instant, it is its own reading.
    otherGrown = Math.max(otherGrown, rg.int(grown))
    const other = otherGrown > 0 && rg.bool() ? [{ name: "other", objects: ordered.slice(0, otherGrown), untrusted: [] }] : []
    const held = [{ name: "life", objects: life, untrusted: chain.untrusted }, ...other]
    await serving(r, held)
    const at = instant(rg.int(24 * 40))
    const set = holdAll(new core.Chains(), held.map(c => heldChain(core, c.name, c.objects, c.untrusted)))
    try {
      const listed = await r.ask({ type: "List", chain: "life", at }, "Page")
      assert.equal(listed.type, "Read")
      if (listed.type === "Read") assert.deepEqual(listed.fold, entriesOf(core, life, at, chain.untrusted, "life", set))
      const read = readOf(await r.ask({ type: "Reading", chain: "life", at }, "Page"))
      const f = foldObjects(core, life, at, chain.untrusted)
      assert.equal(read.type, "Read")
      if (read.type !== "Read") return
      assert.deepEqual([read.at, read.specs, read.functions], [f.at, f.specs, f.functions])
      for (const o of other.filter(c => c.objects.length > 0)) {
        const theirs = readOf(await r.ask({ type: "Reading", chain: o.name, at }, "Page"))
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
      assert.deepEqual(JSON.parse(new TextDecoder().decode((await r.ask({ type: "Thirty", chain: "life", id, ms }, "Page")).bytes)), JSON.parse(JSON.stringify(thirty.sample(core, set, t, chain.untrusted, id, ms))))
      t.free()
    } finally {
      set.free()
    }
    })
  }))

test("two chains held and read at one instant are two readings, each its own chain's", () =>
  channel(core, async r => {
    const few = ordered.filter(o => o.literal.startsWith("Genesis(") || o.literal.includes(`todo='${todos[0] ?? ""}'`))
    await serving(r, [
      { name: "life", objects: ordered, untrusted: chain.untrusted },
      { name: "work", objects: few, untrusted: chain.untrusted },
    ])
    const at = instant(5)
    const life = readOf(await r.ask({ type: "Reading", chain: "life", at }, "Page"))
    const work = readOf(await r.ask({ type: "Reading", chain: "work", at }, "Page"))
    assert.ok(life.type === "Read" && work.type === "Read")
    assert.deepEqual(work.specs, foldObjects(core, few, at, chain.untrusted).specs)
    assert.notDeepEqual(work.specs, life.specs, "work's reading is not life's")
    assert.equal(questions.same(life.reading, work.reading), false)
  }))

test("a remount does not fold: the same chains at the same instant read the reading kept", () => {
  const { core: c, folds } = counted()
  return channel(c, async r => {
    const at = instant(10)
    await serving(r, [{ name: "life", objects: ordered.slice(0, -1), untrusted: chain.untrusted }])
    const mount = async () => readOf(await r.ask({ type: "Reading", chain: "life", at }, "Page"))
    const first = await mount()
    assert.equal(folds(), 1)
    assert.deepEqual(await mount(), first, "the page mounted again reads the same reading")
    assert.equal(folds(), 1, "and nothing was folded for it")
    await serving(r, [{ name: "life", objects: ordered, untrusted: chain.untrusted }])
    readOf(await r.ask({ type: "Reading", chain: "life", at }, "Page"))
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

test("a chain no pull of which has landed reads as unheld", () =>
  channel(core, async r => {
    served.clear()
    await r.ask({ type: "Served", chains: ["life"] }, "Page")
    assert.deepEqual(await r.ask({ type: "List", chain: "life", at: null }, "Page"), { type: "Unheld" })
    assert.deepEqual(readOf(await r.ask({ type: "Reading", chain: "life", at: instant(0) }, "Page")), { type: "Unheld" })
  }))
