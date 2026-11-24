// A Typst view's state as a content-addressed tree, named by the core's `memo::name`: the rows least fulfilled
// first under a balanced run, a tree that is a function of the rows and not of their order, and a change that
// renames exactly the path from what changed to the root.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"

import type { Core } from "../src/prodrome/core"
import memo from "../src/algebra/memo"
import viewstate from "../src/typst/state"
import viewtree from "../src/typst/tree"
import type { Rng } from "./gen"
import { forAll } from "./gen"

const built = process.env["SUZATARY_PRODROME_WASM"]
if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
const core = createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
const named: memo.Namer = (own, kids) => core.memo_name(own, kids)

const row = (r: Rng, i: number): viewstate.Row => ({
  ref: `todo:life/r${i}`,
  title: viewstate.prose(r.string("abc ", 1 + r.int(12))),
  category: r.pick(["home", "work", ""]),
  value: r.bool(0.9) ? r.int(101) / 100 : null,
  unconfirmed: r.bool(0.2),
  complete: r.bool(),
})

const state = (rows: viewstate.Row[], open: viewstate.Opened = { type: "None" }): viewstate.State => ({ schema: viewstate.SCHEMA, at: "", chain: "life", rows, open })
const opening = (x: viewstate.Row): viewstate.Opened => ({ type: "Todo", row: x, created: "", detail: viewstate.prose("detail"), why: [] })
const rows = (r: Rng, upTo = 60): viewstate.Row[] => Array.from({ length: r.int(upTo) }, (_, i) => row(r, i))

const walk = (t: viewtree.Tree): viewtree.Tree[] => [t, ...t.kids.flatMap(walk)]
const names = (t: viewtree.Tree): Set<string> => new Set(walk(t).map(n => n.name))
const drawnRows = (t: viewtree.Tree): string[] => walk(t).flatMap(n => (n.v.node.type === "Row" ? [n.v.node.row.ref] : []))

// The nodes from the root down to the row `ref`, the root first.
const path = (t: viewtree.Tree, ref: string): viewtree.Tree[] => {
  if (t.v.node.type === "Row" && t.v.node.row.ref === ref) return [t]
  for (const k of t.kids) {
    const below = path(k, ref)
    if (below.length > 0) return [t, ...below]
  }
  return []
}

test("the tree draws every row once, least fulfilled first, and every node that draws has its own key", () =>
  forAll(100, r => {
    const xs = rows(r)
    const t = viewtree.of(state(xs), named)
    assert.deepEqual(drawnRows(t), viewtree.byUrgency(xs).map(x => x.ref))
    const keys = walk(t).flatMap(n => (n.v.node.type === "Run" ? [] : [viewtree.key(n.v.node)]))
    assert.equal(new Set(keys).size, keys.length)
  }))

test("the tree is a function of the rows, whatever order the fold held them in", () =>
  forAll(100, r => {
    const xs = rows(r)
    const shuffled = xs.map(x => ({ x, k: r.int(1 << 30) })).sort((a, b) => a.k - b.k).map(p => p.x)
    assert.equal(viewtree.of(state(shuffled), named).name, viewtree.of(state(xs), named).name)
  }))

test("a row's change renames exactly the path from it to the root", () =>
  forAll(100, r => {
    const xs = rows(r)
    if (xs.length === 0) return
    const at = r.int(xs.length)
    const changed = xs.map((x, i) => (i === at ? { ...x, title: viewstate.prose(`${r.string("xyz", 4)} changed`) } : x))
    const before = names(viewtree.of(state(xs), named))
    const after = viewtree.of(state(changed), named)
    const moved = [...names(after)].filter(n => !before.has(n))
    assert.deepEqual(new Set(moved), new Set(path(after, xs[at]?.ref ?? "").map(n => n.name)))
  }))

test("opening a todo renames its row's path, under which it reads in place, and nothing else", () =>
  forAll(100, r => {
    const xs = rows(r)
    const x = xs[r.int(xs.length)]
    if (x === undefined) return
    const before = names(viewtree.of(state(xs), named))
    const after = viewtree.of(state(xs, opening(x)), named)
    const row = path(after, x.ref)
    const inplace = row[row.length - 1]?.kids.find(k => k.v.node.type === "InPlace")
    assert.ok(inplace !== undefined)
    const moved = [...names(after)].filter(n => !before.has(n))
    assert.deepEqual(new Set(moved), new Set([...row.map(n => n.name), inplace.name]))
  }))

test("a row's path is logarithmic in the list's length", () =>
  forAll(20, r => {
    const xs = Array.from({ length: 250 + r.int(250) }, (_, i) => row(r, i))
    const t = viewtree.of(state(xs), named)
    const longest = Math.max(...xs.map(x => path(t, x.ref).length))
    assert.ok(longest <= 4 + 4 * Math.ceil(Math.log2(xs.length)), `a path of ${longest} over ${xs.length} rows`)
  }))
