// A term's JSON as a tree: one notion of a child, a Piecewise's rows included, and its folds' laws.

import { test } from "node:test"
import assert from "node:assert/strict"

import type { SpecNode } from "../src/wire/generated/wire"
import term from "../src/todo/term"
import { forAll } from "./gen"
import type { Rng } from "./gen"

const genTerm = (r: Rng, depth = 3): SpecNode => {
  const leaf = (): SpecNode =>
    r.pick<() => SpecNode>([
      () => ({ kind: "flat", value: r.int(101) / 100 }),
      () => ({ kind: "decay", endDate: `2026-09-${String(10 + r.int(19))}T00:00:00`, horizonHours: 24 * r.int(30) }),
      () => ({ kind: "ref", todo: r.pick(["aaa", "bbb", "ccc"]) }),
      () => ({ kind: "curve", points: [{ at: "2026-09-01T00:00:00", value: 1 }, { at: "2026-09-08T00:00:00", value: 0.5 }] }),
    ])()
  if (depth === 0) return leaf()
  const sub = (): SpecNode => genTerm(r, depth - 1)
  return r.pick<() => SpecNode>([
    leaf,
    () => ({ kind: "conj", p: -4, terms: r.array(r.int(4), sub) }),
    () => ({ kind: "gate", gate: sub(), body: sub() }),
    () => ({ kind: "piecewise", head: sub(), pieces: r.array(r.int(3), () => ({ at: "2026-09-05T00:00:00", term: sub() })) }),
  ])()
}

test("a Piecewise's children are its head and every piece's term; a curve's rows hold none", () => {
  const [a, b, c]: SpecNode[] = [{ kind: "flat", value: 0.1 }, { kind: "ref", todo: "x" }, { kind: "flat", value: 0.3 }]
  const t: SpecNode = { kind: "piecewise", head: a, pieces: [{ at: "2026-09-01T00:00:00", term: b }, { at: "2026-09-02T00:00:00", term: c }] }
  assert.deepEqual(term.children(t), [a, b, c])
  assert.deepEqual(term.children({ kind: "curve", points: [{ at: "2026-09-01T00:00:00", value: 1 }] }), [])
})

test("law: nodes is the cata that lists, root first, and every node is reached through children", () => {
  forAll(200, r => {
    const t = genTerm(r)
    const ns = term.nodes(t)
    assert.equal(ns[0], t)
    assert.equal(ns.length, term.cata((_, rs: number[]) => 1 + rs.reduce((x, y) => x + y, 0), t))
    assert.deepEqual(ns.slice(1), term.children(t).flatMap(term.nodes))
  })
})

test("law: find answers the first node, root first, that its function answers for", () => {
  forAll(200, r => {
    const t = genTerm(r)
    const f = (n: SpecNode): string | null => (n.kind === "ref" && typeof n["todo"] === "string" ? n["todo"] : null)
    assert.equal(term.find(f, t), term.nodes(t).map(f).find(x => x !== null) ?? null)
  })
})
