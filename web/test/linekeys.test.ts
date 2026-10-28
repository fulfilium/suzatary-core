// The lines' row holds the objects and the keys of their words, joined as a pair of unions: a key an erasure names opens
// nothing, and a purge drops it once the blocks its words fed are forgotten, so nothing the device keeps decodes to
// erased words, whichever tab wrote the key, whether or not that tab lived to purge it, and whichever build rendered
// the blocks.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import type { Core, Lines, Named } from "../src/prodrome/core"
import { conversationOf, deadOf, erasedOf, functionsOf, listOfLines, objectsOf, searchOf } from "../src/prodrome/core"
import lo from "../src/algebra/lo"
import locked from "../src/prodrome/locked"
import purge from "../src/prodrome/purge"
import type { Replica } from "../src/prodrome/replica"
import { NOTHING, headsOf, parentsOf, pulled } from "../src/prodrome/replica"
import { KNOWN, held } from "./held"
import rendered from "../src/app/rendered"
import table from "../src/algebra/table"
import type { StoredObject } from "../src/wire/generated/wire"
import { forAll, rng, type Rng } from "./gen"

const core = ((): Core => {
  const built = process.env["SUZATARY_PRODROME_WASM"]
  if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
  return createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
})()

type Shipped = { hash: string; text: string }
const box = JSON.parse(readFileSync("app/core/tests/web/replicas.json", "utf8")) as {
  inbox: Shipped[]
  conversations: Shipped[]
  keys: Record<string, string>
  stale: Record<string, string>
  erased: Record<string, string[]>
}
const named = (xs: readonly Shipped[]): StoredObject[] => xs.map(o => ({ hash: o.hash, literal: o.text }))
const inbox: Named[] = named(box.inbox)
const all = named(box.conversations)
const erasure = all.find(o => o.literal.includes("event=Erased("))
if (erasure === undefined) throw new Error("replicas.json erases nothing")
const before = all.filter(o => o !== erasure)
// The words the vector erased, and the blocks of them the tab had rendered.
const WORDS = "I will draft that now."
const fed = Object.values(box.erased).flat()
// This build's functions, and the answer block another tab rendered of words nobody erased.
const own = functionsOf(core)
const KEPT = core.answer_key("Nothing else today.")

/** `ask` of the lines holding `r`'s objects and `keys`, freed after. */
const over = <A>(r: Replica, keys: locked.Keys, ask: (lines: Lines) => A): A => {
  const lines = new core.Lines()
  try {
    lines.hold(objectsOf(r.objects), "[]")
    lines.keys(JSON.stringify(keys), "[]")
    return ask(lines)
  } finally {
    lines.free()
  }
}
const dead: locked.Dead = r => Promise.resolve(new Set(over(r.replica, locked.NO_KEYS, deadOf)))

const replicaOf = (objects: readonly StoredObject[]): Replica =>
  pulled(NOTHING, { objects: [...objects], heads: headsOf([...objects]), untrusted: [], root: null, geneses: held(objects), epoch: 1, ...KNOWN }, 1)

// A down-set of the box's lines, and some of the keys a tab was given before the erasure.
const row = (r: Rng): locked.Row => {
  const names = new Set<string>()
  const objects: StoredObject[] = []
  for (const o of all) if (parentsOf(o).every(p => names.has(p)) && r.bool(0.8)) (objects.push(o), names.add(o.hash))
  return locked.of(replicaOf(objects), Object.fromEntries(Object.entries(box.stale).filter(() => r.bool(0.7))))
}
const same = (a: locked.Row, b: locked.Row): boolean =>
  a.replica.objects.length === b.replica.objects.length && locked.sameKeys(a.keys, b.keys)

const effects = (store: rendered.Store): purge.Effects<StoredObject> => ({
  erased: async r => over(r.replica, locked.NO_KEYS, lines => erasedOf(lines, r.keys, [])),
  forget: store.forget,
})

// A device's rendered blocks: the erased answer's, rendered while its key was held, and another.
const device = async (): Promise<rendered.Store> => {
  const store = rendered.memory(own)
  await store.join([...fed.map((k): rendered.Pair => [k, `<p>${WORDS}</p>`]), [KEPT, "<p>Nothing else today.</p>"]])
  return store
}

// What a take leaves: the join, less every key the purge could drop.
const taken = async (store: rendered.Store, was: locked.Row, incoming: locked.Row): Promise<{ row: locked.Row; gone: lo.Settled<readonly string[]> }> => {
  const joined = locked.join(was, incoming)
  const gone = await purge.forgotten(effects(store), dead, joined)
  return { row: gone.type === "Val" ? locked.without(joined, gone.v) : joined, gone }
}

const opens = (r: locked.Row): string => {
  const proposals = new core.Proposals(objectsOf(inbox))
  try {
    return over(r.replica, r.keys, lines =>
      JSON.stringify([listOfLines(lines, proposals), conversationOf(lines, proposals, "about-the-deed", {}, null), searchOf(lines, "draft")]),
    )
  } finally {
    proposals.free()
  }
}

test("law: the row's join is a semilattice with NONE its unit", () => {
  forAll(20, r => {
    const [a, b, c] = [row(r), row(r), row(r)]
    assert.ok(same(locked.join(a, a), locked.join(locked.NONE, a)), "idempotent, and NONE is its unit")
    assert.ok(same(locked.join(a, b), locked.join(b, a)), "commutative")
    assert.ok(same(locked.join(locked.join(a, b), c), locked.join(a, locked.join(b, c))), "associative")
  })
})

test("law: a key an erasure names opens nothing, whatever copy brought it", async () => {
  for (let seed = 1; seed <= 10; seed += 1) {
    const r = rng(seed)
    const stale = locked.join(locked.join(row(r), locked.of(replicaOf(before), box.stale)), locked.of(replicaOf(all), box.keys))
    assert.deepEqual((await locked.owed(dead, stale)).sort(), Object.keys(box.erased).sort(), `the stale key is owed (seed ${seed})`)
    assert.ok(!opens(stale).includes(WORDS), "and opens nothing")
  }
})

test("law: after a purge nothing the row or the rendered blocks hold decodes to the erased words", async () => {
  const store = await device()
  const { row: now, gone } = await taken(store, locked.of(replicaOf(before), box.stale), locked.of(replicaOf(all), box.keys))
  assert.equal(gone.type, "Val")
  assert.deepEqual(await locked.owed(dead, now), [], "no key owed is left")
  assert.deepEqual(await store.get(fed), fed.map(() => null), "every block the erased words fed is forgotten")
  assert.deepEqual(await store.get([KEPT]), ["<p>Nothing else today.</p>"], "and nothing else")
  assert.ok(!JSON.stringify({ replica: now.replica.held, keys: now.keys }).includes(WORDS), "the row holds the words' ciphertext only")
  assert.ok(!opens(now).includes(WORDS))
  await store.join(fed.map((k): rendered.Pair => [k, `<p>${WORDS}</p>`]))
  assert.deepEqual(await store.get(fed), fed.map(() => null), "an answer landing after the purge takes nothing back")
})

test("a key the box's pull says is erased is forgotten with its blocks from the first piece, before the erasure's line is held", async () => {
  const store = await device()
  const told = Object.keys(box.erased)
  const held = locked.of(replicaOf(before), box.stale)
  assert.ok(opens(held).includes(WORDS), "without the erasure's line, the key held from before it opens the words")
  const gone = await purge.forgotten(
    { erased: async r => over(r.replica, locked.NO_KEYS, lines => erasedOf(lines, r.keys, told)), forget: store.forget },
    r => dead(r).then(d => new Set([...d, ...told])),
    held,
  )
  assert.equal(gone.type, "Val")
  const now = gone.type === "Val" ? locked.without(held, gone.v) : held
  assert.deepEqual(await store.get(fed), fed.map(() => null), "every block the erased words fed is forgotten")
  assert.deepEqual(await store.get([KEPT]), ["<p>Nothing else today.</p>"], "and nothing else")
  assert.ok(!opens(now).includes(WORDS), "and the words open nowhere")
})

// A build is the functions it keeps blocks of, named here by the answer function's name; another build names the same
// block, of the same words, by another function.
type Build = { readonly answer: string; readonly own: ReadonlySet<string> }
const THIS: Build = { answer: table.fnOf(KEPT), own }
const OLDER: Build = { answer: "older-answer", own: new Set(["older-answer", "older-markup"]) }
const under = (b: Build, k: table.Key): table.Key => table.key(b.answer, table.argOf(k))
const PAIRS: readonly (readonly [Build, Build])[] = [[THIS, THIS], [THIS, OLDER], [OLDER, THIS], [OLDER, OLDER]]

test("law: blocks one build rendered are gone after another build's purge, and nothing on the device decodes to the erased words", async () => {
  for (const [rendering, erasing] of PAIRS) {
    // One device, one table, read by a tab of each build in turn.
    const rows = table.of<string>(table.keep())
    await rendered.memory(rendering.own, rows).join([...fed.map((k): rendered.Pair => [under(rendering, k), `<p>${WORDS}</p>`]), [under(rendering, KEPT), "<p>Nothing else today.</p>"]])
    const store = rendered.memory(erasing.own, rows)
    const fx: purge.Effects<StoredObject> = {
      erased: async x => Object.fromEntries(Object.entries(over(x.replica, locked.NO_KEYS, lines => erasedOf(lines, x.keys, []))).map(([n, ks]) => [n, ks.map(k => under(erasing, k))])),
      forget: store.forget,
    }
    const gone = await purge.forgotten(fx, dead, locked.join(locked.of(replicaOf(before), box.stale), locked.of(replicaOf(all), box.keys)))
    assert.equal(gone.type, "Val")
    assert.ok(rows.pairs().every(([, html]) => !html.includes(WORDS)), `rendered by ${rendering.answer}, erased by ${erasing.answer}`)
    assert.deepEqual(
      rows.pairs().map(([k]) => k),
      rendering === erasing ? [under(rendering, KEPT)] : [],
      "the same build's other blocks stay; another build's all go",
    )
  }
})

test("a key a tab that never purged left on the device is forgotten, its blocks first, by the tab that reads the row", async () => {
  // The tab that wrote the key and rendered its blocks was closed before it heard the erasure.
  const store = await device()
  const left = locked.of(replicaOf(before), box.stale)
  const { row: now } = await taken(store, locked.of(replicaOf(all), box.keys), left)
  assert.deepEqual(await locked.owed(dead, now), [])
  assert.deepEqual(await store.get(fed), fed.map(() => null))
})

test("a purge that cannot forget a block keeps the key that finds it and says why, and the words still read as erased", async () => {
  const fx: purge.Effects<StoredObject> = { ...effects(rendered.memory(own)), forget: () => Promise.resolve(lo.error("the cache did not open")) }
  const joined = locked.join(locked.of(replicaOf(before), box.stale), locked.of(replicaOf(all), box.keys))
  const gone = await purge.forgotten(fx, dead, joined)
  assert.ok(gone.type === "Error" && gone.e.includes("did not open"))
  assert.deepEqual((await locked.owed(dead, joined)).sort(), Object.keys(box.erased).sort(), "the key stays, to be asked again")
  assert.ok(!opens(joined).includes(WORDS), "and opens nothing meanwhile")
})

test("a purge that cannot find the blocks, its lines' content not fetched, keeps the keys owed and forgets nothing", async () => {
  const store = await device()
  const fx: purge.Effects<StoredObject> = { erased: () => Promise.reject(new Error("the box did not answer")), forget: store.forget }
  const joined = locked.join(locked.of(replicaOf(before), box.stale), locked.of(replicaOf(all), box.keys))
  const gone = await purge.forgotten(fx, dead, joined)
  assert.ok(gone.type === "Error" && gone.e.includes("did not answer"))
  assert.deepEqual(await store.get(fed), fed.map(() => `<p>${WORDS}</p>`), "nothing is forgotten blind")
  assert.deepEqual((await locked.owed(dead, joined)).sort(), Object.keys(box.erased).sort(), "the keys stay, to be asked again")
})

test("a row whose objects erase nothing asks the core for nothing", async () => {
  const fx: purge.Effects<StoredObject> = { erased: () => assert.fail("asked"), forget: () => assert.fail("asked") }
  const gone = await purge.forgotten(fx, () => Promise.resolve(new Set()), locked.of(replicaOf(before), box.keys))
  assert.deepEqual(gone, lo.val([]))
})
