// What a reading left out is a set by name, joined by union: the semilattice laws and the readers' fold.

import { test } from "node:test"
import assert from "node:assert/strict"

import excluded from "../src/prodrome/excluded"
import { forAll, type Rng } from "./gen"

const NAMES = ["a1", "b2", "c3", "d4", "e5", "f6"]
const one = (r: Rng): excluded.Excluded => ({
  name: r.pick(NAMES),
  why: r.pick(["failed to parse: Asked.act: missing field `text`", "not held", "does not hash to its own name"]),
  resting: r.array(r.int(3), () => r.pick(NAMES)),
})
const set = (r: Rng): excluded.T => excluded.of(r.array(r.int(4), () => one(r)))

test("law: join is a semilattice with NONE its identity", () => {
  forAll(300, r => {
    const [a, b, c] = [set(r), set(r), set(r)]
    assert.deepEqual(excluded.join(a, excluded.NONE), a)
    assert.deepEqual(excluded.join(excluded.NONE, a), a)
    assert.deepEqual(excluded.join(a, a), a)
    assert.deepEqual(excluded.join(a, b), excluded.join(b, a))
    assert.deepEqual(excluded.join(excluded.join(a, b), c), excluded.join(a, excluded.join(b, c)))
  })
})

test("law: a set is one name per entry, in name order, whatever order it was read in", () => {
  forAll(200, r => {
    const xs = r.array(r.int(6), () => one(r))
    const t = excluded.of(xs)
    assert.deepEqual(t.map(x => x.name), [...new Set(xs.map(x => x.name))].sort())
    assert.deepEqual(excluded.of([...xs].reverse()), t)
  })
})

const CHAINS = ["life", "suzatary", "music"]
const left = (r: Rng): excluded.Left =>
  excluded.left(set(r), r.array(r.int(3), () => ({ chain: r.pick(CHAINS), why: r.pick(["a hole", "this device could not read what it holds of it"]) })))

test("law: what one reading left out, objects and chains, is a semilattice with NOTHING its identity", () => {
  forAll(300, r => {
    const [a, b, c] = [left(r), left(r), left(r)]
    assert.deepEqual(excluded.joinLeft(a, excluded.NOTHING), a)
    assert.deepEqual(excluded.joinLeft(excluded.NOTHING, a), a)
    assert.deepEqual(excluded.joinLeft(a, a), a)
    assert.deepEqual(excluded.joinLeft(a, b), excluded.joinLeft(b, a))
    assert.deepEqual(excluded.joinLeft(excluded.joinLeft(a, b), c), excluded.joinLeft(a, excluded.joinLeft(b, c)))
    assert.deepEqual(a.chains.map(x => x.chain), [...new Set(a.chains.map(x => x.chain))].sort(), "one per chain, in order")
  })
})

test("law: a reader's step replaces only its own, and a repeat wakes nobody", () => {
  forAll(200, r => {
    const [a, b] = [left(r), left(r)]
    const s = excluded.step(excluded.step(excluded.UNSEEN, { type: "Read", reader: "chain/life", left: a }), { type: "Read", reader: "stores", left: b })
    assert.deepEqual(excluded.all(s), excluded.joinLeft(a, b))
    assert.equal(excluded.step(s, { type: "Read", reader: "stores", left: b }), s)
    assert.deepEqual(excluded.all(excluded.step(s, { type: "Read", reader: "chain/life", left: excluded.NOTHING })), b)
  })
})

test("a chain the box's fold holds as a failure is named once in the one line, with why, and goes when no reading fails it", () => {
  const failed = [{ chain: "life", why: "the chain life does not verify: a hole at its head" }]
  const readers = ["box/life", "inbox"]
  const told = readers.reduce((s, reader) => excluded.step(s, { type: "Read", reader, left: excluded.left(excluded.NONE, failed) }), excluded.UNSEEN)
  const all = excluded.all(told)
  assert.deepEqual(all.chains, failed, "the chain and why, once, whichever readings told it")
  assert.equal(excluded.said(all), "The chain life could not be read, so what refers into it is unpriced until it reads")
  const cleared = readers.reduce((s, reader) => excluded.step(s, { type: "Read", reader, left: excluded.NOTHING }), told)
  assert.ok(excluded.empty(excluded.all(cleared)))
})
