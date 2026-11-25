import { test } from "node:test"
import assert from "node:assert/strict"

import placing from "../src/shell/placing"
import { forAll, type Rng } from "./gen"

// A fake host: an element is an object, and the container is an array that applies the edits it is given.
type El = { readonly n: number }
const apply = (children: readonly El[], edits: readonly placing.Edit<El>[]): El[] => {
  const out = [...children]
  for (const edit of edits) {
    const at = out.indexOf(edit.e)
    if (at >= 0) out.splice(at, 1)
    if (edit.type === "Insert") {
      const to = edit.before === null ? out.length : out.indexOf(edit.before)
      out.splice(to < 0 ? out.length : to, 0, edit.e)
    }
  }
  return out
}
const names = (xs: readonly El[]): number[] => xs.map(e => e.n)

const shuffled = <T>(r: Rng, xs: readonly T[]): T[] => {
  const out = [...xs]
  for (let i = out.length - 1; i > 0; i -= 1) {
    const j = r.int(i + 1)
    ;[out[i], out[j]] = [out[j] as T, out[i] as T]
  }
  return out
}

const square = { x: 0, y: 0, w: 10, h: 10 }
const tile = (id: number): placing.Tile => ({ id, rect: square, fill: square })

// A host of up to ten elements, each keyed by its number; some tiles drawn, some elements in the container already.
const world = (r: Rng): { elements: Map<number, El>; tiles: placing.Tile[]; attached: El[] } => {
  const all = Array.from({ length: 1 + r.int(10) }, (_, n): El => ({ n }))
  return {
    elements: new Map(all.map(e => [e.n, e])),
    tiles: shuffled(r, all.filter(() => r.bool(0.6))).map(e => tile(e.n)),
    attached: shuffled(r, all.filter(() => r.bool(0.5))),
  }
}

test("the container holds the drawn elements once and none hidden, and the hidden are kept", () =>
  forAll(500, r => {
    const { elements, tiles, attached } = world(r)
    const placed = placing.place(tiles, elements, attached, null)
    assert.deepEqual(names(apply(attached, placed.edits)).sort(), names(placed.drawn).sort())
    assert.deepEqual(new Set([...placed.drawn, ...placed.hidden]), new Set(elements.values()), "every element is drawn or hidden, and none let go")
  }))

test("an element that stays is never moved, and the container is in the arrangement's order where those that stay are", () =>
  forAll(500, r => {
    const { elements, tiles, attached } = world(r)
    const placed = placing.place(tiles, elements, attached, null)
    const stays = placed.drawn.filter(e => attached.includes(e))
    assert.ok(placed.edits.every(e => e.type === "Remove" || !stays.includes(e.e)), "an edit moves a stayer")
    assert.deepEqual(apply(attached, placed.edits).filter(e => stays.includes(e)), attached.filter(e => stays.includes(e)))
    // With the stayers already in the arrangement's order, the container is that order.
    const ordered = placed.drawn.filter(e => attached.includes(e))
    assert.deepEqual(apply(ordered, placing.place(tiles, elements, ordered, null).edits), placed.drawn)
  }))

test("a newcomer goes before the element that stays after it, else before `end`, which a layer of the host's can be", () =>
  forAll(500, r => {
    const { elements, tiles, attached } = world(r)
    const layer: El = { n: 100 }
    const placed = placing.place(tiles, elements, attached, layer)
    const stays = new Set(placed.drawn.filter(e => attached.includes(e)))
    placed.drawn.forEach((e, i) => {
      if (stays.has(e)) return
      const next = placed.drawn.slice(i + 1).find(u => stays.has(u)) ?? layer
      assert.ok(placed.edits.some(x => x.type === "Insert" && x.e === e && x.before === next), `${e.n} goes before ${next.n}`)
    })
    assert.equal(apply([...attached, layer], placed.edits).at(-1), layer, "the layer stays last")
  }))

test("placing the result again edits nothing", () =>
  forAll(500, r => {
    const { elements, tiles, attached } = world(r)
    const placed = placing.place(tiles, elements, attached, null)
    assert.deepEqual(placing.place(tiles, elements, apply(attached, placed.edits), null).edits, [])
  }))

test("a rectangle is its shares of the room, and a page that fills its tile has no inset", () => {
  assert.equal(placing.rectStyle({ w: 200, h: 100 }, { x: 50, y: 25, w: 100, h: 50 }), "left:25.000%;top:25.000%;width:50.000%;height:50.000%")
  assert.equal(placing.inset(tile(0), 16), "")
  const paged = { id: 0, rect: { x: 0, y: 0, w: 100, h: 200 }, fill: { x: 0, y: 32, w: 100, h: 136 } }
  assert.equal(placing.inset(paged, 16), "padding:2rem 0rem 2rem 0rem")
})

test("every remove comes before any insert, so the host's layer is placed once the hidden elements are out", () =>
  forAll(500, r => {
    const { elements, tiles, attached } = world(r)
    const types = placing.place(tiles, elements, attached, null).edits.map(e => e.type)
    const first = types.indexOf("Insert")
    assert.ok(first < 0 || !types.slice(first).includes("Remove"), types.join())
  }))
