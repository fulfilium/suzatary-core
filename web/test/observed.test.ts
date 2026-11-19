// The fold as the screen observes it: over the core's fold of the fixture chain, between two observed changes the
// drawn list is the same bytes whatever the clock says and computes nothing, while at a change exactly the rows
// whose whole percent moved compute again.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import type { Core } from "../src/prodrome/core"
import { entriesOf } from "../src/prodrome/core"
import memo from "../src/algebra/memo"
import table from "../src/algebra/table"
import onscreen from "../src/app/store"
import time from "../src/app/time"
import fulfillment from "../src/todo/fulfillment"
import viewstate from "../src/typst/state"
import viewtree from "../src/typst/tree"
import { forAll } from "./gen"

test("the screen's whole percent is idempotent, within half a percent, and the core's `PERCENT` at its edges", () => {
  forAll(500, r => {
    const v = r.int(1_000_001) / 1_000_000
    const w = fulfillment.whole(v)
    assert.equal(fulfillment.whole(w), w)
    assert.ok(Math.abs(w - v) <= 0.005 + 1e-12, `${v} -> ${w}`)
  })
  assert.equal(fulfillment.whole(0.424), 0.42)
  assert.equal(fulfillment.whole(0.4251), 0.43)
  assert.equal(fulfillment.whole(0.999), 1)
  assert.equal(fulfillment.observed("absent"), "absent")
})

const built = process.env["SUZATARY_PRODROME_WASM"]
if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
const core = createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
const chain = JSON.parse(readFileSync("app/api/tests/web/chain.json", "utf8")) as { links: { hash: string; literal: string }[]; untrusted: string[] }
const none = new core.Chains()
const reads: viewstate.Reads = { prose: m => core.prose(m) ?? undefined }
const named = memo.tabulatedNamer((own, kids) => core.memo_name(own, kids), table.of(table.lru(8192)))
const T0 = "2026-09-02T12:00:00"

// The store as the tab fills it, folded by the core at `at`.
const folded = (at: string) => {
  const answer = entriesOf(core, chain.links, at, chain.untrusted, "life", none)
  const f = onscreen.step(onscreen.UNLOADED, { type: "Local", entries: answer.entries, at: answer.at, next: answer.next, heads: [], fresh: true, ancestry: () => "Unknown" })
  return { entries: onscreen.entries(f), at: onscreen.at(f) ?? at, next: onscreen.next(f) }
}

// The list as drawn: each node's own bytes around its children's, so equal bytes are an equal page.
const page = memo.pointwise<viewtree.Part, string>("page", (p, rs) => `<${p.own}>${rs.join("")}</>`)
const drawn = (s: ReturnType<typeof folded>, cache: table.Table<string>) => {
  const state = viewstate.of(reads, { entries: s.entries, at: s.at, chain: "life", open: "", writes: true })
  const tree = viewtree.of(state, named)
  const [out, report] = memo.fold(page, tree, cache)
  const rows = new Map<memo.Name, string>()
  const walk = (t: viewtree.Tree): void => (t.v.node.type === "Row" ? void rows.set(t.name, t.v.node.row.ref) : undefined, t.kids.forEach(walk))
  walk(tree)
  return { out, state, computedRows: memo.names(report, "Computed").flatMap(n => rows.get(n) ?? []) }
}

test("between two observed changes the drawn list is the same bytes whatever the clock says, and computes nothing", () => {
  const first = folded(T0)
  assert.ok(first.next !== null, "the fixture's prices move")
  const cache = table.of<string>(table.lru(4096))
  const before = drawn(first, cache)
  const [from, to] = [time.parse(T0) * 1000, time.parse(first.next.at) * 1000]
  forAll(40, r => {
    const at = time.moment(from + r.int(to - from))
    const then = folded(at)
    const after = drawn(then, cache)
    assert.equal(after.out, before.out, `drawn at ${at}`)
    assert.deepEqual(after.computedRows, [], `nothing computed at ${at}`)
  })
})

test("at an observed change exactly the rows whose whole percent moved compute again", () => {
  let s = folded(T0)
  const cache = table.of<string>(table.lru(4096))
  let was = drawn(s, cache)
  for (let i = 0; i < 6 && s.next !== null; i++) {
    const then = folded(s.next.at)
    const now = drawn(then, cache)
    const value = (st: viewstate.State) => new Map(st.rows.map(x => [x.ref, x.value]))
    const [a, b] = [value(was.state), value(now.state)]
    const moved = [...b.keys()].filter(k => a.get(k) !== b.get(k)).sort()
    if (s.next.exact) assert.ok(moved.length > 0, `something moved at ${s.next.at}`)
    assert.deepEqual([...now.computedRows].sort(), moved, `at ${s.next.at}`)
    s = then
    was = now
  }
})
