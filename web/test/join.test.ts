// The replicas are the free join-semilattice on the box's objects: `join` is idempotent, commutative and associative
// with `NOTHING` its unit, it keeps a causal order, and what the box said last is the word that has seen more.

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import type { Replica } from "../src/prodrome/replica"
import { NOTHING, ancestry, appended, boxHeadsOf, headsOf, join, parentsOf, prodromeOf, pulled, reset, said, settled, untrustedOf } from "../src/prodrome/replica"
import type { ObjectsPayload, RefusedObject, StoredObject } from "../src/wire/generated/wire"
import { forAll, rng, type Rng } from "./gen"
import { KNOWN, held as prodromes } from "./held"

const box = JSON.parse(readFileSync("app/core/tests/web/replicas.json", "utf8")) as { conversations: { hash: string; text: string }[] }
// The box's history in the order it took the objects, so the box at any instant holds a prefix of it.
const history: StoredObject[] = box.conversations.map(o => ({ hash: o.hash, literal: o.text }))

const downSet = (r: Rng, from: readonly StoredObject[], held: readonly StoredObject[]): StoredObject[] => {
  const names = new Set(held.map(o => o.hash))
  const left = from.filter(o => !names.has(o.hash))
  const objects = [...held]
  for (let n = r.int(left.length + 1); n > 0; n -= 1) {
    const ready = left.filter(o => parentsOf(o).every(p => names.has(p)))
    if (ready.length === 0) break
    const next = r.pick(ready)
    objects.push(next)
    names.add(next.hash)
    left.splice(left.indexOf(next), 1)
  }
  return objects
}

// A second prodrome the box migrates to, beside the history's: its genesis, then a line of changes.
const genesis = (hash: string): StoredObject => ({ hash, literal: "Genesis(schema='inbox', at=datetime(2026, 10, 1, 0, 0, 0))" })
const change = (hash: string, genesis: string, deps: readonly string[]): StoredObject => ({
  hash,
  literal: `Change(genesis='${genesis}', deps=(${deps.map(d => `'${d}',`).join(" ")}), event=Completed(todo='alpha', at=datetime(2026, 9, 7, 12, 0, 0), actor='ada', note=''))`,
})
const OLD = history.flatMap(o => prodromeOf(o) ?? [])[0] ?? ""
const NEW = "2".repeat(64)
const moved: StoredObject[] = [genesis(NEW), ...["a", "b", "c", "d"].map((x, i, xs) => change(x.repeat(64), NEW, i === 0 ? [] : [(xs[i - 1] ?? "").repeat(64)]))]

// What the box holds in each era of a migration, a drop and two rollbacks, the prodromes it says it holds, and the
// epoch it answers with, one later per era since each moves the set it serves.
type Era = { readonly objects: readonly StoredObject[]; readonly geneses: readonly string[]; readonly epoch: number }
const ERAS: readonly Era[] = [
  { objects: history, geneses: [OLD], epoch: 1, ...KNOWN },
  { objects: [...history, ...moved], geneses: [OLD, NEW], epoch: 2, ...KNOWN },
  { objects: moved, geneses: [NEW], epoch: 3, ...KNOWN },
  { objects: [...history, ...moved], geneses: [OLD, NEW], epoch: 4, ...KNOWN },
  { objects: history, geneses: [OLD], epoch: 5, ...KNOWN },
  // A deploy of an earlier checkpoint of the same store: no store moves, but the heads answered last are gone.
  { objects: history.slice(0, Math.max(1, history.length - 3)), geneses: [OLD], epoch: 6, ...KNOWN },
]
const [BEFORE, BOTH, DROPPED, ROLLED, BACK, EARLIER] = ERAS as [Era, Era, Era, Era, Era, Era]
// The box at some instant of an era: what it held so far, and the prodromes it serves.
const during = (r: Rng, era: Era): Era => ({
  objects: era.objects.slice(0, r.int(era.objects.length + 1)),
  geneses: era.geneses,
  epoch: era.epoch,
})
const heard = (era: Era, untrusted: string[] = [], root: string | null = null): ObjectsPayload => ({
  objects: [...era.objects],
  heads: headsOf(era.objects),
  untrusted,
  root,
  geneses: [...era.geneses],
  epoch: era.epoch,
  since: "Known",
  frontier: { digest: "0".repeat(64), count: era.objects.length },
})

// Where an answer leaves a replica: one of a few frontiers, read as known or as one the box lost.
const named = (r: Rng): Pick<ObjectsPayload, "since" | "frontier"> => ({
  since: r.pick(["Known", "Unknown"] as const),
  frontier: { digest: r.pick(["a", "b", "c"]).repeat(64), count: r.int(40) },
})

// A tab's replica: the box as it stood at its last contact, in some era of the migration, under some policy, plus
// what it took from another tab since, writes into either store included, and refusals of some of what it holds.
const replica = (r: Rng): Replica => {
  if (r.int(6) === 0) return NOTHING
  const era = during(r, r.pick(ERAS))
  const contact = pulled(NOTHING, { ...heard(era, r.int(2) === 0 ? [] : ["scout"], r.int(2) === 0 ? null : "f".repeat(64)), ...named(r) }, 1 + r.int(4))
  const more = appended(contact, downSet(r, [...history, ...moved], contact.held.objects).slice(contact.held.objects.length))
  const refused: RefusedObject[] = more.held.objects
    .filter(() => r.int(8) === 0)
    .map(o => ({ hash: o.hash, reason: r.pick(["no", "not yet"]), kind: r.pick(["final", "trust"] as const) }))
  const withRefusals = settled(more, { heads: [], adopted: [], already: [], refused, ...named(r) }, more.generation)
  // Most replicas are of the first generation, some saw a reset, and one reset is kept whole over what came before it.
  return r.int(4) === 0 ? join(reset(withRefusals), pulled(reset(withRefusals), heard(era), 2)) : withRefusals
}

// A replica as a value: what it holds and every reading of it, its sets in any order and its objects as a set.
const names = (objects: readonly StoredObject[]): string[] => objects.map(o => o.hash).sort()
const seen = (a: Replica) => ({
  ...a,
  held: { ...a.held, objects: names(a.held.objects), synced: [...a.held.synced].sort() },
  objects: names(a.objects),
  heads: [...a.heads].sort(),
  synced: [...new Set(a.synced)].sort(),
})

test("law: join is idempotent, commutative and associative, with NOTHING its unit", () => {
  forAll(80, r => {
    const [a, b, c] = [replica(r), replica(r), replica(r)]
    assert.deepEqual(seen(join(a, a)), seen(a), "idempotent")
    assert.deepEqual(seen(join(NOTHING, a)), seen(a), "unit on the left")
    assert.deepEqual(seen(join(a, NOTHING)), seen(a), "unit on the right")
    assert.deepEqual(seen(join(a, b)), seen(join(b, a)), "commutative")
    assert.deepEqual(seen(join(join(a, b), c)), seen(join(a, join(b, c))), "associative")
  })
})

test("law: a join only climbs, within a generation, and keeps every object after the objects it rests on", () => {
  forAll(80, r => {
    const [a, b] = [replica(r), replica(r)]
    const joined = join(a, b)
    const held = new Set(joined.held.objects.map(o => o.hash))
    const kept = [a, b].filter(x => x.generation === Math.max(a.generation, b.generation))
    for (const o of kept.flatMap(x => x.held.objects)) assert.ok(held.has(o.hash), `${o.hash} is held after the join`)
    const placed = new Set<string>()
    for (const o of joined.held.objects) {
      assert.ok(parentsOf(o).every(p => placed.has(p)), o.hash)
      placed.add(o.hash)
    }
    assert.deepEqual(joined.heads, headsOf(joined.objects), "the heads are derived")
  })
})

test("law: the box's word that has seen more wins, whichever contact was later", () => {
  forAll(60, r => {
    const k = 1 + r.int(history.length)
    const early = history.slice(0, r.int(k))
    const late = history.slice(0, k)
    const word = (objects: StoredObject[], untrusted: string[], at: number): Replica =>
      pulled(NOTHING, { objects, heads: headsOf(objects), untrusted, root: null, geneses: prodromes(history), epoch: 1, ...KNOWN }, at)
    // The fuller word was heard first, by the tab's clock: the clock does not decide.
    const fuller = word(late, ["scout"], 1)
    const older = word(early, [], 2)
    assert.deepEqual(untrustedOf(join(fuller, older)), ["scout"])
    assert.deepEqual(boxHeadsOf(join(older, fuller)), [...headsOf(late)].sort())
    // The same heads heard twice: the later contact's policy is the box's now.
    assert.deepEqual(untrustedOf(join(word(late, [], 1), word(late, ["scout"], 2))), ["scout"])
  })
})

test("law: a prodrome the box dropped reads as forgotten, whatever copy is joined in, until a later word serves it again", () => {
  const [GONE, KEPT, A, B] = ["1".repeat(64), "3".repeat(64), "a".repeat(64), "b".repeat(64)]
  // A tab that heard the box first, at the epoch before the migration.
  const before = pulled(NOTHING, { objects: [genesis(GONE), change(A, GONE, [])], heads: [A], untrusted: [], root: null, geneses: [GONE], epoch: 1, ...KNOWN }, 1)
  const after = pulled(before, { objects: [genesis(KEPT), change(B, KEPT, [])], heads: [B], untrusted: [], root: null, geneses: [KEPT], epoch: 2, ...KNOWN }, 2)
  assert.deepEqual(after.objects.map(o => o.hash), [KEPT, B])
  assert.deepEqual(after.gone, [GONE])
  assert.deepEqual(join(after, before).objects.map(o => o.hash), [KEPT, B], "the other tab's copy does not bring it back")
  assert.deepEqual(boxHeadsOf(join(before, after)), [B], "nor its word")
  assert.deepEqual(names(after.held.objects), names([genesis(GONE), change(A, GONE, []), genesis(KEPT), change(B, KEPT, [])]), "it is still held")
  // The box rolled back to a checkpoint that serves the old store too: nothing is fetched again, and it reads.
  const back = pulled(after, { objects: [], heads: [A, B], untrusted: [], root: null, geneses: [GONE, KEPT], epoch: 3, ...KNOWN }, 3)
  assert.deepEqual(names(back.objects), names([genesis(GONE), change(A, GONE, []), genesis(KEPT), change(B, KEPT, [])]))
  assert.deepEqual(back.gone, [])
})

// A tab that heard the box in one era, then took more from another tab.
const tab = (r: Rng, era: Era): Replica =>
  appended(pulled(NOTHING, heard(era), 1 + r.int(4)), downSet(r, [...history, ...moved], era.objects).slice(era.objects.length))

test("law: the box's word is its latest epoch's, whatever copies are joined, a drop and a rollback included", () => {
  forAll(80, r => {
    const picked = ERAS.filter(() => r.int(2) === 0)
    const latest = picked.at(-1)
    if (latest === undefined) return
    const tabs = picked.map(era => tab(r, era))
    // In any order: a random rotation, then each tab joined in on either side.
    const turned = [...tabs.slice(r.int(tabs.length)), ...tabs].slice(0, tabs.length)
    const joined = turned.reduce((acc, t) => (r.int(2) === 0 ? join(acc, t) : join(t, acc)), NOTHING)
    assert.deepEqual(
      said(joined.box, null, w => ({ epoch: w.epoch, heads: [...new Set(w.heads)].sort() })),
      { epoch: latest.epoch, heads: [...headsOf(latest.objects)].sort() },
      "the latest epoch, and its heads beside it",
    )
    const kept = new Set(latest.geneses)
    assert.deepEqual(joined.gone, [OLD, NEW].filter(g => !kept.has(g) && joined.held.objects.some(o => prodromeOf(o) === g)).sort(), "and what it does not serve, gone")
  })
})

test("a drop, heard after the box served both, reads the dropped store as gone", () => {
  const joined = join(tab(rng(1), BOTH), tab(rng(2), DROPPED))
  assert.deepEqual(joined.gone, [OLD])
  assert.ok(!joined.objects.some(o => prodromeOf(o) === OLD), "OLD's objects read as forgotten")
})

test("a rollback within the same store reads its earlier heads, though it reaches less", () => {
  const joined = join(tab(rng(1), BACK), tab(rng(2), EARLIER))
  assert.deepEqual(boxHeadsOf(joined), [...headsOf(EARLIER.objects)].sort(), "the box's heads are the checkpoint's, not the later ones")
  assert.deepEqual(boxHeadsOf(join(tab(rng(3), EARLIER), tab(rng(4), BACK))), boxHeadsOf(joined), "in either order")
})

test("a rollback to the old store reads the new one as gone, its heads the old store's", () => {
  const joined = join(tab(rng(1), DROPPED), tab(rng(2), BACK))
  assert.deepEqual(joined.gone, [NEW])
  assert.deepEqual(boxHeadsOf(joined), [...headsOf(history)].sort())
  assert.deepEqual(join(tab(rng(3), BEFORE), tab(rng(4), ROLLED)).gone, [], "a rollback serving both forgets neither")
})

test("law: the word read is one held, and no word held rests strictly under another", () => {
  forAll(60, r => {
    const joined = join(replica(r), replica(r))
    const words = joined.held.words
    const order = ancestry(joined.held.objects)
    for (const w of words) assert.ok(!words.some(v => order(w.heads, v.heads) === "Below"), "a word under another is not kept")
    // The very word held, not its heads beside another's epoch or policy.
    assert.ok(joined.box.type === "Unheard" ? words.length === 0 : words.includes(joined.box.word), "the word read is one held, whole")
  })
})

test("law: a later generation is kept whole, whatever copy from before it is joined in", () => {
  forAll(80, r => {
    const [a, b] = [replica(r), replica(r)]
    const later = reset(a.generation >= b.generation ? a : b)
    assert.equal(join(later, a), later)
    assert.equal(join(b, later), later)
  })
})
