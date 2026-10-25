// The law the epoch scene (web/e2e/epoch.test.ts) checks in a browser, over a store row's join and drop and the pull's
// one decision: in whatever order an answer of the old epoch, pieces kept from before and from another tab, another
// tab's pull and this tab's land, a row holding the box's later word holds only objects the box served in that epoch,
// always a set the box could have sent, and a pull ends holding the box's lines whole.

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import locked from "../src/prodrome/locked"
import stores from "../src/prodrome/stores"
import type { Replica } from "../src/prodrome/replica"
import { NOTHING, headsOf, parentsOf, pulled, replicaOf, said } from "../src/prodrome/replica"
import type { StoredObject } from "../src/wire/generated/wire"
import { forAll, type Rng } from "./gen"
import { KNOWN, held as prodromes } from "./held"

const box = JSON.parse(readFileSync("app/core/tests/web/replicas.json", "utf8")) as { conversations: { hash: string; text: string }[] }
const before: StoredObject[] = box.conversations.map(o => ({ hash: o.hash, literal: o.text }))

// The box's lines after a migration: `tip`, a head of one conversation, written again as a sibling resting on what it
// rested on, and `tip` gone.
const tip = headsOf(before).find(h => before.find(o => o.hash === h)?.literal.startsWith("Change("))!
const was = before.find(o => o.hash === tip)!
const again: StoredObject = { hash: "f".repeat(64), literal: was.literal.replace(/Locked\(key='[0-9a-f]{64}'/, `Locked(key='${"e".repeat(64)}'`) }
const after: StoredObject[] = [...before.filter(o => o.hash !== tip), again]

const OLD = 1
const LATER = 2

// A down-set of `objects`, parents first: what one answer asked of nothing, or several of them, can hold.
const downSet = (r: Rng, objects: readonly StoredObject[]): StoredObject[] => {
  const held = new Set<string>()
  const left = [...objects]
  const out: StoredObject[] = []
  for (let n = 1 + r.int(objects.length); n > 0; n -= 1) {
    const ready = left.filter(o => parentsOf(o).every(p => held.has(p)))
    const next = r.pick(ready)
    out.push(next)
    held.add(next.hash)
    left.splice(left.indexOf(next), 1)
  }
  return out
}

// An answer of the box at `epoch`, taken into an empty replica of `generation`, as a pull's piece is; it names the
// prodromes the box held in that epoch, whatever part of them it sends.
const answer = (objects: readonly StoredObject[], epoch: number, generation: number, at: number): Replica =>
  pulled(replicaOf({ ...NOTHING.held, generation }), { objects: [...objects], heads: headsOf([...objects]), untrusted: [], root: null, geneses: prodromes(epoch === LATER ? after : before), epoch, ...KNOWN }, at)

// What the device holds of a store: the row pages read, and the pieces of a pull kept beside it, not yet landed.
type Device = { readonly row: locked.Row; readonly pending: locked.Row }

// The objects of `objects` outside the ancestry of `heads`, parents first: what the box answers `?since=heads`.
const since = (objects: readonly StoredObject[], heads: readonly string[]): StoredObject[] => {
  const byName = new Map(objects.map(o => [o.hash, o]))
  const under = new Set<string>()
  const left = heads.filter(h => byName.has(h))
  for (let h = left.pop(); h !== undefined; h = left.pop()) {
    if (under.has(h)) continue
    under.add(h)
    left.push(...parentsOf(byName.get(h)!).filter(p => byName.has(p)))
  }
  return objects.filter(o => !under.has(o.hash))
}

// A tab's pull as `storesync` runs it: asked of what the device holds; an answer of a later epoch than either part heard
// drops the row and the pieces, and the store is asked for again from nothing; the first `cut` objects of the answer
// are kept as a piece, and a pull that took the rest lands them in the row.
const pull = (d: Device, at: number, cut: number | null): Device => {
  const asked = stores.overtaken([d.row.replica, d.pending.replica], { epoch: LATER })
    ? { row: locked.reset(d.row), pending: locked.NONE }
    : d
  const from = locked.join(asked.row, asked.pending).replica
  const sent = since(after, from.heads)
  const piece = locked.of(answer(cut === null ? sent : sent.slice(0, cut), LATER, from.generation, at), { b: "00" })
  const pending = locked.join(asked.pending, piece)
  return cut === null ? { row: locked.join(asked.row, pending), pending: locked.NONE } : { row: asked.row, pending }
}

type Step = { readonly name: string; readonly apply: (d: Device) => Device }

const steps = (r: Rng): Step[] => [
  { name: "an answer at the old epoch lands", apply: d => ({ ...d, row: locked.join(d.row, locked.of(answer(downSet(r, before), OLD, 0, 2), { a: "00" })) }) },
  { name: "a piece of the old epoch is kept", apply: d => ({ ...d, pending: locked.join(d.pending, locked.of(pulled(NOTHING, { objects: [was], heads: [tip], untrusted: [], root: null, geneses: prodromes(before), epoch: OLD, ...KNOWN }, 3))) }) },
  // Another tab's pull cut after its first piece: kept beside the row, asked of what the device held.
  { name: "another tab keeps a piece and is cut off", apply: d => pull(d, 4, 1 + r.int(3)) },
  // What a tab whose pull was in flight as it closed leaves: a piece of the later epoch, kept after the row it was asked
  // of was written over by a copy from before (the gate's run on 2026-10-19), so the piece alone rests on nothing held.
  {
    name: "a piece of the later epoch is kept beside a row of the one before",
    apply: d => (said(d.row.replica.box, false, w => w.epoch === OLD) ? { ...d, pending: locked.join(d.pending, locked.of(answer([again], LATER, 0, 4), { b: "00" })) } : d),
  },
  // A pull lands only whole, so what another tab lands is the store as the box held it.
  { name: "another tab's pull of the later epoch lands", apply: d => ({ ...d, row: locked.join(d.row, locked.of(answer(after, LATER, 0, 5), { b: "00" })) }) },
  { name: "another tab's copy from before lands", apply: d => ({ ...d, row: locked.join(d.row, locked.of(answer(before, OLD, 0, 6), { a: "00" })) }) },
  { name: "another tab resets the row", apply: d => ({ ...d, row: locked.reset(d.row) }) },
  { name: "this tab pulls", apply: d => pull(d, 7, null) },
]

// What the box could have sent in the epoch the row's word is of: a down-set of what it held then.
const couldHaveSent = (row: locked.Row): string | null => {
  const epoch = said(row.replica.box, null, w => w.epoch)
  if (epoch === null) return row.replica.objects.length === 0 ? null : "objects held beside no word of the box"
  const served = new Set((epoch === LATER ? after : before).map(o => o.hash))
  const held = new Set(row.replica.objects.map(o => o.hash))
  const foreign = row.replica.objects.filter(o => !served.has(o.hash))
  if (foreign.length > 0) return `a word of epoch ${epoch} beside ${foreign.length} object(s) the box did not hold in it`
  return epoch === LATER && row.replica.objects.some(o => parentsOf(o).some(p => !held.has(p))) ? "an object of the later epoch without its parents" : null
}

test("law: in every order, the device never holds the box's later word beside an object it no longer holds", () => {
  forAll(300, (r, seed) => {
    // A shuffle of every step, each perhaps twice: the same answer landing again, from this tab or another.
    const order = steps(r).flatMap(s => (r.bool(0.3) ? [s, s] : [s])).map(s => ({ s, k: r.int(1 << 30) })).sort((x, y) => x.k - y.k).map(({ s }) => s)
    let d: Device = { row: locked.of(answer(before, OLD, 0, 1), { a: "00" }), pending: locked.NONE }
    const taken: string[] = []
    for (const step of [...order, steps(r).at(-1)!]) {
      d = step.apply(d)
      taken.push(step.name)
      assert.equal(couldHaveSent(d.row), null, `seed ${seed}: after ${taken.join(", then ")}`)
    }
    // A pull, whatever came before it, leaves the row holding the box's lines and nothing else.
    assert.deepEqual(new Set(d.row.replica.objects.map(o => o.hash)), new Set(after.map(o => o.hash)), `seed ${seed}: after ${taken.join(", then ")}`)
    assert.deepEqual(locked.names(d.row.keys), ["b"], `seed ${seed}: the keys of the later epoch alone`)
  })
})

test("a row at the later epoch keeps no key of the epoch before", () => {
  const held = locked.of(answer(before, OLD, 0, 1), { a: "00" })
  const later = locked.join(held, locked.of(answer(after, LATER, 0, 2), { b: "00" }))
  assert.deepEqual(locked.names(later.keys), ["b"])
  assert.equal(locked.join(later, held), later, "a copy from before adds nothing, in either order")
  assert.equal(locked.join(held, later), later)
})
