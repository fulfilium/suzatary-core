// A store's replica in the tab steps by the replicas' one join, and a step that adds nothing wakes nobody.

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import stores from "../src/prodrome/stores"
import locked from "../src/prodrome/locked"
import type { Replica, Word } from "../src/prodrome/replica"
import { NOTHING, boxHeadsOf, headsOf, parentsOf, pulled, replicaOf, reset, said, untrustedOf } from "../src/prodrome/replica"
import type { StoredObject } from "../src/wire/generated/wire"
import { forAll, type Rng } from "./gen"
import { KNOWN, held as prodromes } from "./held"

const box = JSON.parse(readFileSync("app/core/tests/web/replicas.json", "utf8")) as { conversations: { hash: string; text: string }[] }
const all: StoredObject[] = box.conversations.map(o => ({ hash: o.hash, literal: o.text }))

// A down-set of the box's objects in a causal order, and a contact with the box at some instant.
const replica = (r: Rng): Replica => {
  const held = new Set<string>()
  const left = [...all]
  const objects: StoredObject[] = []
  for (let n = r.int(all.length + 1); n > 0; n -= 1) {
    const ready = left.filter(o => parentsOf(o).every(p => held.has(p)))
    const next = r.pick(ready)
    objects.push(next)
    held.add(next.hash)
    left.splice(left.indexOf(next), 1)
  }
  return pulled(NOTHING, { objects, heads: headsOf(objects), untrusted: [], root: null, geneses: prodromes(objects), epoch: 1, ...KNOWN }, 1 + r.int(1000))
}

const step = stores.step
// The inbox's replica holds objects and locks no words; the lines' row steps by the same join over nodes, with the keys
// of their words beside it (`stores.taken`).
const held = (name: "inbox", replica: Replica): stores.Event => ({ type: "Held", name, replica })

test("law: a step that adds nothing wakes nobody", () => {
  forAll(30, r => {
    const name = "inbox" as const
    const t = step(stores.NONE, held(name, replica(r)))
    assert.equal(step(t, { type: "Held", name, replica: t[name] }), t)
    assert.equal(step(t, held(name, NOTHING)), t)
  })
})

test("law: a refusal that replaces another, of another object or for another reason, is a step", () => {
  forAll(30, r => {
    const base = replica(r)
    const refused = (hash: string, reason: string): Replica =>
      replicaOf({ ...base.held, refused: [{ hash, reason, kind: "final" }] })
    const one = refused("a".repeat(64), "not trusted")
    assert.ok(stores.same(one, refused("a".repeat(64), "not trusted")))
    assert.ok(!stores.same(one, refused("b".repeat(64), "not trusted")), "another object refused")
    assert.ok(!stores.same(one, refused("a".repeat(64), "too large")), "another reason")
  })
})

test("law: a key the box gave the lines is a step, though no object moved", () => {
  forAll(30, r => {
    const row = locked.of(replica(r))
    const next = stores.taken(row, { type: "Held", row: locked.of(row.replica, { ["a".repeat(64)]: "00" }) })
    assert.notEqual(next, row)
    assert.deepEqual(locked.names(next.keys), ["a".repeat(64)])
  })
})

test("law: a forgotten key leaves the lines' row, and forgetting what it does not hold wakes nobody", () => {
  forAll(30, r => {
    const k = "a".repeat(64)
    const row = locked.of(replica(r), { [k]: "00" })
    assert.deepEqual(locked.names(stores.taken(row, { type: "Forgot", keys: [k] }).keys), [])
    assert.equal(stores.taken(row, { type: "Forgot", keys: ["b".repeat(64)] }), row)
  })
})

test("law: a policy the box changed with no new object is a step, and the later contact's policy is the one kept", () => {
  forAll(30, r => {
    const name = "inbox" as const
    const t = step(stores.NONE, held(name, replica(r)))
    const was = t[name]
    const w = said<Pick<Word, "heads" | "untrusted" | "root" | "fetchedAt" | "geneses">>(was.box, { heads: [], untrusted: [], root: null, fetchedAt: 0, geneses: [] }, w => w)
    const later = pulled(was, { objects: [], heads: [...w.heads], untrusted: [...w.untrusted, "scout"], root: w.root, geneses: [...w.geneses], epoch: 1, ...KNOWN }, w.fetchedAt + 1)
    const next = step(t, held(name, later))
    assert.notEqual(next, t, "the policy moved, so the reading must")
    assert.deepEqual(untrustedOf(next[name]), untrustedOf(later))
    assert.ok(!stores.same(was, later))
    const reversed = replicaOf({ ...later.held, words: later.held.words.map(v => ({ ...v, untrusted: [...v.untrusted].reverse() })) })
    assert.ok(stores.same(later, reversed), "a policy is a set")
  })
})

// The box's lines as a migration leaves them: `tip`, a head of one conversation, written again as `again`, a sibling
// resting on what it rested on, and `tip` gone. A replica that heard the box before holds `tip`.
const rewritten = (): { before: StoredObject[]; after: StoredObject[]; tip: string; again: string } => {
  const tip = headsOf(all).find(h => all.find(o => o.hash === h)?.literal.startsWith("Change("))!
  const was = all.find(o => o.hash === tip)!
  const again = { hash: "f".repeat(64), literal: was.literal.replace(/Locked\(key='[0-9a-f]{64}'/, `Locked(key='${"e".repeat(64)}'`) }
  return { before: all, after: [...all.filter(o => o.hash !== tip), again], tip, again: again.hash }
}

const word = (objects: StoredObject[], epoch: number, since: StoredObject[] = []) => ({
  ...KNOWN,
  objects: objects.filter(o => !since.some(s => s.hash === o.hash)),
  heads: headsOf(objects),
  untrusted: [],
  root: null,
  geneses: prodromes(objects),
  epoch,
})

test("a replica overtaken by the box's later epoch starts over and holds only what the box holds, once and on every tab", () => {
  const { before, after, tip, again } = rewritten()
  const heard = locked.of(pulled(NOTHING, word(before, 1), 1), { k: "00" })
  // What the box answers it after the migration: what moved since its heads, in a later epoch.
  const answer = word(after, 2, before)
  assert.ok(stores.overtaken([heard.replica], answer))
  // Joined as any answer is, it keeps the rewritten tip beside its image: two heads where the box holds one.
  const forked = pulled(heard.replica, answer, 2)
  assert.ok(forked.heads.includes(tip) && forked.heads.includes(again), "the fork the reset prevents")
  // Started over and asked again of nothing, it is the box's set.
  const fresh = locked.reset(heard)
  assert.equal(stores.overtaken([fresh.replica], word(after, 2)), false, "a replica that heard nothing is not overtaken")
  const whole = locked.of(pulled(fresh.replica, word(after, 2), 3))
  assert.deepEqual(new Set(whole.replica.objects.map(o => o.hash)), new Set(after.map(o => o.hash)))
  assert.ok(!whole.replica.heads.includes(tip))
  assert.deepEqual(whole.keys, {}, "the old keys went with it")
  // Another tab's copy from before the reset adds nothing to it, in either order.
  assert.equal(locked.join(whole, heard), whole)
  assert.equal(locked.join(heard, whole), whole)
  // And a later answer of the same epoch is no reason to start over again.
  assert.equal(stores.overtaken([whole.replica], word(after, 2)), false)
})

test("law: only an answer of a later epoch than every word heard overtakes a replica", () => {
  forAll(30, r => {
    const held = replica(r)
    const epoch = held.held.words[0]?.epoch ?? 0
    const at = (e: number) => ({ objects: [], heads: [...(boxHeadsOf(held) ?? [])], untrusted: [], root: null, geneses: prodromes(held.objects), epoch: e, ...KNOWN })
    assert.equal(stores.overtaken([held], at(epoch)), false, "the same epoch")
    assert.equal(stores.overtaken([held], at(epoch - 1)), false, "an earlier one, which is the box gone back")
    assert.equal(stores.overtaken([held], at(epoch + 1 + r.int(5))), held.held.words.length > 0)
    assert.equal(stores.overtaken([NOTHING], at(epoch + 1)), false, "nothing heard")
  })
})

test("a reset is a step even of a row that holds nothing, so a pull asks of the reset", () => {
  const name = "inbox" as const
  const t = step(stores.NONE, held(name, reset(NOTHING)))
  assert.notEqual(t, stores.NONE)
  assert.equal(t[name].generation, 1)
})
