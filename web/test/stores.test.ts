// A store's row in the core's worker steps by the rows' one join (`locked.taken`), and a step that adds nothing wakes
// nobody.

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import locked from "../src/prodrome/locked"
import type { Replica, Word } from "../src/prodrome/replica"
import { NOTHING, boxHeadsOf, headsOf, overtaken, parentsOf, pulled, replicaOf, reset, said, same, untrustedOf } from "../src/prodrome/replica"
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

// Every store's row steps by one join, a store that locks no words holding no keys.
const NONE = locked.none<StoredObject>()
const step = (row: locked.Row<StoredObject>, replica: Replica): locked.Row<StoredObject> => locked.taken(row, { type: "Held", row: locked.of(replica) })

test("law: a step that adds nothing wakes nobody", () => {
  forAll(30, r => {
    const t = step(NONE, replica(r))
    assert.equal(step(t, t.replica), t)
    assert.equal(step(t, NOTHING), t)
  })
})

test("law: a refusal that replaces another, of another object or for another reason, is a step", () => {
  forAll(30, r => {
    const base = replica(r)
    const refused = (hash: string, reason: string): Replica =>
      replicaOf({ ...base.held, refused: [{ hash, reason, kind: "final" }] })
    const one = refused("a".repeat(64), "not trusted")
    assert.ok(same(one, refused("a".repeat(64), "not trusted")))
    assert.ok(!same(one, refused("b".repeat(64), "not trusted")), "another object refused")
    assert.ok(!same(one, refused("a".repeat(64), "too large")), "another reason")
  })
})

test("law: a key the box gave the lines is a step, though no object moved", () => {
  forAll(30, r => {
    const row = locked.of(replica(r))
    const next = locked.taken(row, { type: "Held", row: locked.of(row.replica, { ["a".repeat(64)]: "00" }) })
    assert.notEqual(next, row)
    assert.deepEqual(locked.names(next.keys), ["a".repeat(64)])
  })
})

test("law: a forgotten key leaves the lines' row, and forgetting what it does not hold wakes nobody", () => {
  forAll(30, r => {
    const k = "a".repeat(64)
    const row = locked.of(replica(r), { [k]: "00" })
    assert.deepEqual(locked.names(locked.taken(row, { type: "Forgot", keys: [k] }).keys), [])
    assert.equal(locked.taken(row, { type: "Forgot", keys: ["b".repeat(64)] }), row)
  })
})

test("law: a policy the box changed with no new object is a step, and the later contact's policy is the one kept", () => {
  forAll(30, r => {
    const t = step(NONE, replica(r))
    const was = t.replica
    const w = said<Pick<Word, "heads" | "untrusted" | "root" | "fetchedAt" | "geneses">>(was.box, { heads: [], untrusted: [], root: null, fetchedAt: 0, geneses: [] }, w => w)
    const later = pulled(was, { objects: [], heads: [...w.heads], untrusted: [...w.untrusted, "scout"], root: w.root, geneses: [...w.geneses], epoch: 1, ...KNOWN }, w.fetchedAt + 1)
    const next = step(t, later)
    assert.notEqual(next, t, "the policy moved, so the reading must")
    assert.deepEqual(untrustedOf(next.replica), untrustedOf(later))
    assert.ok(!same(was, later))
    const reversed = replicaOf({ ...later.held, words: later.held.words.map(v => ({ ...v, untrusted: [...v.untrusted].reverse() })) })
    assert.ok(same(later, reversed), "a policy is a set")
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
  assert.ok(overtaken([heard.replica], answer))
  // Joined as any answer is, it keeps the rewritten tip beside its image: two heads where the box holds one.
  const forked = pulled(heard.replica, answer, 2)
  assert.ok(forked.heads.includes(tip) && forked.heads.includes(again), "the fork the reset prevents")
  // Started over and asked again of nothing, it is the box's set.
  const fresh = locked.reset(heard)
  assert.equal(overtaken([fresh.replica], word(after, 2)), false, "a replica that heard nothing is not overtaken")
  const whole = locked.of(pulled(fresh.replica, word(after, 2), 3))
  assert.deepEqual(new Set(whole.replica.objects.map(o => o.hash)), new Set(after.map(o => o.hash)))
  assert.ok(!whole.replica.heads.includes(tip))
  assert.deepEqual(whole.keys, {}, "the old keys went with it")
  // Another tab's copy from before the reset adds nothing to it, in either order.
  assert.equal(locked.join(whole, heard), whole)
  assert.equal(locked.join(heard, whole), whole)
  // And a later answer of the same epoch is no reason to start over again.
  assert.equal(overtaken([whole.replica], word(after, 2)), false)
})

test("law: only an answer of a later epoch than every word heard overtakes a replica", () => {
  forAll(30, r => {
    const held = replica(r)
    const epoch = held.held.words[0]?.epoch ?? 0
    const at = (e: number) => ({ objects: [], heads: [...(boxHeadsOf(held) ?? [])], untrusted: [], root: null, geneses: prodromes(held.objects), epoch: e, ...KNOWN })
    assert.equal(overtaken([held], at(epoch)), false, "the same epoch")
    assert.equal(overtaken([held], at(epoch - 1)), false, "an earlier one, which is the box gone back")
    assert.equal(overtaken([held], at(epoch + 1 + r.int(5))), held.held.words.length > 0)
    assert.equal(overtaken([NOTHING], at(epoch + 1)), false, "nothing heard")
  })
})

test("a reset is a step even of a row that holds nothing, so a pull asks of the reset", () => {
  const t = step(NONE, reset(NOTHING))
  assert.notEqual(t, NONE)
  assert.equal(t.replica.generation, 1)
})
