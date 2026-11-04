// A sync says how far it has come, in bytes: a pull's first piece names the bytes the whole pull brings, each piece
// moves what came by its own weight, a fetch counts what came of what was asked, and the line is hidden once nothing syncs.

import { test } from "node:test"
import assert from "node:assert/strict"

import progress from "../src/prodrome/progress"
import stores from "../src/prodrome/stores"
import { forAll } from "./gen"

test("law: a pull's first piece names the bytes the whole pull brings, and each piece adds its own weight to what came", () =>
  forAll(300, (r, seed) => {
    const name = r.pick(stores.NAMES)
    const weights = r.array(1 + r.int(6), () => 1 + r.int(5000))
    const whole = weights.reduce((n, w) => n + w, 0)
    let t = progress.NONE
    let lacking = whole
    let came = 0
    for (const weight of weights) {
      t = progress.step(t, { type: "Piece", name, size: lacking, weight })
      lacking -= weight
      came += weight
      assert.deepEqual(progress.bar(t[name]), { type: "Shown", done: came, needed: whole }, `seed ${seed}`)
      for (const other of stores.NAMES) if (other !== name) assert.deepEqual(progress.bar(t[other]), { type: "Hidden" }, `seed ${seed}`)
    }
    assert.deepEqual(progress.bar(progress.step(t, { type: "Pulled", name })[name]), { type: "Hidden" }, `seed ${seed}`)
  }))

test("law: a fetch shows what came of what was asked, a lost batch leaves what is needed, and it hides once every byte is settled", () =>
  forAll(300, (r, seed) => {
    const name = r.pick(stores.NAMES)
    const batches = r.array(1 + r.int(5), () => ({ bytes: 1 + r.int(4000), lost: r.bool(0.3) }))
    const asked = batches.reduce((n, b) => n + b.bytes, 0)
    let t = progress.step(progress.NONE, { type: "Asked", name, bytes: asked })
    let needed = asked
    let came = 0
    batches.forEach((b, i) => {
      t = progress.step(t, b.lost ? { type: "Lost", name, bytes: b.bytes } : { type: "Came", name, bytes: b.bytes })
      if (b.lost) needed -= b.bytes
      else came += b.bytes
      const last = i === batches.length - 1
      assert.deepEqual(progress.bar(t[name]), last ? { type: "Hidden" } : { type: "Shown", done: came, needed }, `seed ${seed}`)
    })
  }))

test("a sync is said in words, in megabytes from a tenth of one and kilobytes below", () => {
  assert.equal(progress.said("lines", 400_000, 1_100_000), "Syncing conversations · 0.4 of 1.1 MB")
  assert.equal(progress.said("inbox", 2_000, 40_000), "Syncing the inbox · 2 of 40 KB")
})
