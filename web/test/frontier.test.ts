// A pull names what its replica holds by the frontier the box's last answer named: a request that does not grow with the
// replica, a frontier joined as the later of two, and one named after the box lost the frontier asked from standing over
// every copy from before.

import { test } from "node:test"
import assert from "node:assert/strict"

import type { Frontier } from "../src/prodrome/replica"
import { NOTHING, answered, join, laterFrontier, pulled, reset, settled, sinceOf } from "../src/prodrome/replica"
import { sinceIn } from "../src/wire/api"
import type { StoredObject } from "../src/wire/generated/wire"
import { forAll, type Rng } from "./gen"

const hex = (n: number): string => n.toString(16).padStart(64, "0")

// One prodrome and ten thousand changes resting on its genesis alone: a replica with ten thousand heads.
const GENESIS = "0".repeat(64)
const wide: StoredObject[] = [
  { hash: GENESIS, literal: "Genesis(schema='lines', nonce='0')" },
  ...Array.from({ length: 10_000 }, (_, i) => ({
    hash: hex(i + 1),
    literal: `Change(genesis='${GENESIS}', deps=(), event=Completed(todo='t${i}', at=datetime(2026, 9, 7, 12, 0, 0), actor='ada', note=''))`,
  })),
]

test("law: a pull's request stays under a kilobyte, ten thousand heads or none", () => {
  const replica = pulled(NOTHING, {
    objects: wide,
    heads: wide.slice(1).map(o => o.hash),
    untrusted: [],
    root: null,
    geneses: [GENESIS],
    epoch: 1,
    since: "Known",
    frontier: { digest: "f".repeat(64), count: wide.length },
  }, 1)
  assert.equal(replica.heads.length, 10_000)
  for (const path of ["/api/stores/lines/skeleton", "/api/stores/inbox/objects", "/api/life/objects"]) {
    for (const r of [NOTHING, replica]) assert.ok(sinceIn(path, sinceOf(r)).length < 1024, path)
  }
})

const frontier = (r: Rng): Frontier =>
  r.int(5) === 0 ? { type: "Nothing" } : { type: "Named", digest: r.pick(["a", "b", "c"]).repeat(64), count: r.int(20), era: r.int(3) }

test("law: frontiers join as the later, a semilattice with nothing its unit", () => {
  forAll(200, r => {
    const [a, b, c] = [frontier(r), frontier(r), frontier(r)]
    assert.deepEqual(laterFrontier(a, a), a)
    assert.deepEqual(laterFrontier({ type: "Nothing" }, a), a)
    assert.deepEqual(laterFrontier(a, b), laterFrontier(b, a))
    assert.deepEqual(laterFrontier(laterFrontier(a, b), c), laterFrontier(a, laterFrontier(b, c)))
  })
})

test("a frontier named after the box lost the one asked from stands over every copy from before, however far it reached", () => {
  const before = pulled(NOTHING, { objects: [], heads: [], untrusted: [], root: null, geneses: [], epoch: 1, since: "Known", frontier: { digest: "a".repeat(64), count: 900 } }, 1)
  const lost = pulled(before, { objects: [], heads: [], untrusted: [], root: null, geneses: [], epoch: 1, since: "Unknown", frontier: { digest: "b".repeat(64), count: 3 } }, 2)
  assert.equal(sinceOf(lost), "b".repeat(64))
  assert.equal(sinceOf(join(before, lost)), "b".repeat(64), "another tab's copy of the lost one does not win it back")
  const known = answered(lost.held.frontier, "Known", { digest: "c".repeat(64), count: 4 })
  assert.deepEqual(known, { type: "Named", digest: "c".repeat(64), count: 4, era: 1 })
})

test("a receipt for a post made before another tab's reset leaves the reset asking from nothing", () => {
  const posting = pulled(NOTHING, { objects: [], heads: [], untrusted: [], root: null, geneses: [], epoch: 1, since: "Known", frontier: { digest: "a".repeat(64), count: 900 } }, 1)
  const receipt = { heads: [], adopted: [], already: [], refused: [], since: "Known" as const, frontier: { digest: "b".repeat(64), count: 901 } }
  assert.equal(sinceOf(settled(posting, receipt, posting.generation)), "b".repeat(64))
  const reset_ = reset(posting)
  assert.equal(sinceOf(settled(reset_, receipt, posting.generation)), "", "the reset holds nothing the receipt's frontier names")
})
