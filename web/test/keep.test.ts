import { test } from "node:test"
import assert from "node:assert/strict"

import keep from "../src/shell/keep"
import { forAll } from "./gen"

type Page = { readonly key: number; readonly href: string; readonly seen: number; readonly mounted: boolean }
const of: keep.Of<Page> = { key: t => t.key, same: (t, href) => t.href === href, seen: t => t.seen, mounted: t => t.mounted }

test("every running page is drawn, hidden or let go, once; the drawn first, the hidden bounded and the latest drawn", () =>
  forAll(500, r => {
    const pages = Array.from({ length: 4 + r.int(12) }, (_, i) => `#/p/${i}`)
    // The tree holds a page once, so the pages running are of distinct addresses.
    const live: Page[] = pages.filter(() => r.bool(0.7)).map(href => ({ key: r.int(100), href, seen: r.int(1000), mounted: r.bool(0.9) }))
    let key = 100
    const leaf = (): { key: number; href: string } => ({ key: key++, href: r.pick(pages) })
    const at = { tiles: Array.from({ length: 1 + r.int(3) }, leaf), behind: Array.from({ length: r.int(30) }, leaf), keep: r.int(12) }
    const k = keep.kept(live, at, of)
    const kept = [...k.drawn.values(), ...k.hidden.values()].map(h => h.tile)
    assert.equal(new Set(kept).size, kept.length, "a page is kept under one leaf")
    assert.deepEqual(new Set([...kept, ...k.gone]), new Set(live), "every page is accounted for")
    assert.equal(kept.length + k.gone.length, live.length)
    assert.ok(k.hidden.size <= at.keep, `${k.hidden.size} hidden of ${at.keep}`)
    assert.ok(kept.every(t => t.mounted), "a page never mounted is let go")
    // A drawn leaf whose page runs, mounted, takes it.
    const others = [...k.gone, ...[...k.hidden.values()].map(h => h.tile)]
    for (const w of at.tiles) if (!k.drawn.has(w.key)) assert.ok(!others.some(t => t.href === w.href && t.mounted), `${w.href} runs but is not drawn`)
    for (const [key, h] of [...k.drawn, ...k.hidden]) {
      assert.equal(h.tile.href, h.href)
      assert.equal([...at.tiles, ...at.behind].find(w => w.key === key)?.href, h.href)
    }
    // A mounted page let go that a behind leaf would hold was drawn no later than every page kept hidden.
    const behind = new Set(at.behind.map(w => w.href))
    const least = Math.min(...[...k.hidden.values()].map(h => h.tile.seen))
    if (k.hidden.size === at.keep) for (const t of k.gone) if (t.mounted && behind.has(t.href)) assert.ok(t.seen <= least, `${t.seen} let go before ${least}`)
  }))

test("a page drawn is never let go for a page held behind", () => {
  const live: Page[] = [{ key: 1, href: "#/a", seen: 1, mounted: true }]
  const k = keep.kept(live, { tiles: [{ key: 7, href: "#/a" }], behind: [{ key: 8, href: "#/a" }], keep: 10 }, of)
  assert.equal(k.drawn.get(7)?.tile, live[0])
  assert.equal(k.hidden.size, 0)
})

test("of two leaves of one address, each page goes to the leaf it drew last", () => {
  const live: Page[] = [{ key: 3, href: "#/a", seen: 1, mounted: true }, { key: 4, href: "#/a", seen: 2, mounted: true }]
  const k = keep.kept(live, { tiles: [{ key: 4, href: "#/a" }], behind: [{ key: 3, href: "#/a" }], keep: 10 }, of)
  assert.equal(k.drawn.get(4)?.tile, live[1])
  assert.equal(k.hidden.get(3)?.tile, live[0])
})

test("a page made but never mounted is let go, so it is made again rather than drawn blank", () => {
  const live: Page[] = [{ key: 1, href: "#/a", seen: 1, mounted: false }]
  const k = keep.kept(live, { tiles: [], behind: [{ key: 1, href: "#/a" }], keep: 10 }, of)
  assert.equal(k.hidden.size, 0)
  assert.deepEqual(k.gone, live)
  assert.equal(keep.kept(live, { tiles: [{ key: 1, href: "#/a" }], behind: [], keep: 10 }, of).drawn.size, 0)
})
