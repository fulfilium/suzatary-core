import { test } from "node:test"
import assert from "node:assert/strict"

import routelog from "../src/shell/routelog"
import { hrefOf, hrefOfAddress, parseAddress, parseRoute } from "../src/shell/route"
import { address, forAll, route, type Rng } from "./gen"

const kept = (r: Rng): routelog.Kept => ({ scroll: r.int(2000), focus: r.pick(["", "row/a", "group/home"]) })

// A log a tab could hold: places gone to, left with something kept, and moved over.
const walk = (r: Rng): routelog.Log =>
  r.array(r.int(12), () => r.int(4)).reduce<routelog.Log>(
    (l, k) => (k === 0 ? routelog.go(routelog.keep(l, kept(r)), address(r)) : k === 1 ? routelog.back(l) : k === 2 ? routelog.forward(l) : routelog.keep(l, kept(r))),
    routelog.of(address(r)),
  )

test("law: a route printed and read back is the same route, and the log holds addresses only by their print", () =>
  forAll(300, r => {
    const x = route(r)
    assert.deepEqual(parseRoute(hrefOf(x)), x)
    const a = address(r)
    if (a.tree !== "") assert.deepEqual(parseAddress(hrefOfAddress(a)), a, "a tree after a route reads back as it was written")
    const l = routelog.go(walk(r), a)
    assert.equal(hrefOfAddress(routelog.address(l)), hrefOfAddress(a))
  }))

test("law: back after going is the identity on the view — the place and what was kept of it", () =>
  forAll(300, r => {
    const l = routelog.keep(walk(r), kept(r))
    const x = address(r)
    const there = routelog.go(l, x)
    if (routelog.same(x, routelog.address(l))) return assert.equal(there, l)
    assert.deepEqual(routelog.here(routelog.back(there)), routelog.here(l))
    assert.deepEqual(routelog.here(routelog.forward(routelog.back(there))), { address: x, kept: routelog.FRESH })
  }))

test("law: going somewhere drops what lay ahead; going where the reader is writes nothing", () =>
  forAll(300, r => {
    const l = walk(r)
    const x = address(r)
    const there = routelog.go(l, x)
    assert.equal(routelog.go(there, x), there)
    if (there === l) return
    assert.equal(there.entries.length, l.at + 2)
    assert.equal(routelog.forward(there), there)
  }))

test("law: the browser moving to an entry the log wrote is the pointer moving there, and nothing else", () =>
  forAll(300, r => {
    const l = walk(r)
    const at = r.int(l.entries.length)
    const moved = routelog.to(l, at, (l.entries[at] as routelog.Entry).address)
    assert.deepEqual(moved.entries, l.entries)
    assert.equal(moved.at, at)
    assert.deepEqual(routelog.to(l, l.at - 1, routelog.address(routelog.back(l))), routelog.back(l))
  }))

test("law: an entry the log never wrote starts it again at that place", () =>
  forAll(200, r => {
    const l = walk(r)
    const x = address(r)
    const moved = routelog.to(l, l.entries.length + r.int(3), x)
    assert.equal(hrefOfAddress(routelog.address(moved)), hrefOfAddress(routelog.same(x, routelog.address(l)) ? routelog.address(l) : x))
  }))
