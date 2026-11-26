// lease's law: a device that heard the box and has not heard it for longer than the lease wipes once it holds nothing
// the box has not taken, and no other does; a heard answer is written down at most once an hour, and always after the
// clock moves back.

import { test } from "node:test"
import assert from "node:assert/strict"

import lease from "../src/app/lease"
import type kv from "../src/app/kv"
import { forAll } from "./gen"

const store = (): { kv: kv.Kv; writes: number } => {
  const held = new Map<string, string>()
  const s = { writes: 0, kv: { get: (k: string) => held.get(k) ?? null, set: (k: string, v: string) => void (held.set(k, v), s.writes++), drop: (k: string) => void held.delete(k) } }
  return s
}

test("law: a device lapses exactly when it heard the box and has not for longer than the lease", () => {
  forAll(500, r => {
    const at = r.int(1_000_000_000)
    const now = at + r.int(3 * lease.MS) - lease.MS
    assert.equal(lease.lapsed(lease.read(String(at)), now), now - at > lease.MS, `${at} ${now}`)
    assert.equal(lease.lapsed(lease.read(null), now), false)
    assert.equal(lease.lapsed(lease.read("not a time"), now), false)
  })
})

test("the lease is the owner's two months: 59 days keep a device, 61 days lapse it", () => {
  const day = 24 * 60 * 60 * 1000
  assert.equal(lease.DAYS, 60)
  assert.equal(lease.lapsed(lease.read("0"), 59 * day), false)
  assert.equal(lease.lapsed(lease.read("0"), 61 * day), true)
})

test("law: the lease wipes only a lapsed device that holds nothing the box has not taken", () => {
  forAll(500, r => {
    const at = r.int(1_000_000_000)
    const now = at + r.int(3 * lease.MS)
    const owed = r.bool() ? 0 : 1 + r.int(5)
    const heard = lease.read(String(at))
    assert.equal(lease.wipes(heard, now, owed), owed === 0 && lease.lapsed(heard, now), `${now - at} ${owed}`)
  })
  assert.equal(lease.wipes(lease.read("0"), 2 * lease.MS, 1), false, "one unsent write keeps the device")
})

test("law: a heard answer is written down at most once an hour, and after the clock moves back", () => {
  forAll(300, r => {
    const s = store()
    const start = r.int(1_000_000_000)
    lease.heard(s.kv, start)
    const later = start + r.int(2 * lease.EVERY_MS)
    lease.heard(s.kv, later)
    assert.equal(s.writes, later - start >= lease.EVERY_MS ? 2 : 1)
    lease.heard(s.kv, start - 1 - r.int(1000))
    assert.equal(lease.read(s.kv.get(lease.KEY)).type, "At")
    assert.ok(Number(s.kv.get(lease.KEY)) < start, "a clock moved back is written down")
  })
})
