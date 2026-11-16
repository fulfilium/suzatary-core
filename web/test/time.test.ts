// An instant reads the same on every device: naive is UTC, an offset is honoured, and this device's zone never enters.

import { test } from "node:test"
import assert from "node:assert/strict"

import time from "../src/app/time"
import { forAll } from "./gen"
import { everywhere } from "./zones"

test("the harness moves this device between the zones", () => {
  const offsets: number[] = []
  everywhere(() => void offsets.push(new Date(Date.UTC(2026, 9, 6, 12)).getTimezoneOffset()))
  assert.deepEqual(offsets, [240, -60, -120, 0])
})

test("an instant written by moment reads back as itself, whatever this device's zone", () =>
  everywhere(() =>
    forAll(300, r => {
      const t = Date.UTC(2000, 0, 1) + r.int(2 ** 30) * 1000 + r.int(1000)
      assert.equal(time.ms(time.moment(t)), t)
      assert.equal(time.parse(time.instant(t / 1000)), Math.floor(t / 1000))
    }),
  ))

test("a naive instant is UTC, and an explicit offset is taken off", () =>
  everywhere(() => {
    assert.equal(time.ms("2026-10-06T02:01:00"), Date.UTC(2026, 9, 6, 2, 1))
    assert.equal(time.ms("2026-10-06T02:01:00.123456"), Date.UTC(2026, 9, 6, 2, 1, 0, 123))
    assert.equal(time.ms("2026-10-06 02:01"), Date.UTC(2026, 9, 6, 2, 1))
    assert.equal(time.ms("2026-10-06"), Date.UTC(2026, 9, 6))
    assert.equal(time.ms("2026-10-06T02:01:00Z"), Date.UTC(2026, 9, 6, 2, 1))
    assert.equal(time.ms("2026-10-05T22:01:00-04:00"), Date.UTC(2026, 9, 6, 2, 1))
    assert.equal(time.ms("2026-10-06T05:01:00+0300"), Date.UTC(2026, 9, 6, 2, 1))
    assert.equal(time.ms("2026-10-06T03:01:00.5+01:00"), Date.UTC(2026, 9, 6, 2, 1, 0, 500))
  }))

test("a string that names no instant reads as none", () =>
  everywhere(() => {
    for (const s of ["", "now", "2026-02-30", "2026-04-31T00:00:00", "2026-10-06T24:00:00", "2026-10-06T12:60", "2026-13-01", "06/10/2026"])
      assert.equal(time.ms(s), null, s)
    assert.throws(() => time.parse("never"))
  }))

test("across New York's fall-back, the two half past ones are an hour apart", () =>
  everywhere(() => {
    // 2026-11-01 01:30 EDT and 01:30 EST: one wall-clock spelling, two instants, which UTC tells apart.
    const edt = time.ms("2026-11-01T05:30:00")
    const est = time.ms("2026-11-01T06:30:00")
    assert.ok(edt !== null && est !== null)
    assert.equal(est - edt, 3_600_000)
  }))

const NEW_YORK: time.Zone = { name: "America/New_York" }

test("the owner's day across New York's fall-back is twenty-five hours, on a device in any zone", () =>
  everywhere(() => {
    assert.equal(time.start(NEW_YORK, "2026-11-01"), Date.UTC(2026, 10, 1, 4), "midnight EDT")
    assert.equal(time.nextDay(NEW_YORK, Date.UTC(2026, 10, 1, 12)), Date.UTC(2026, 10, 2, 5), "midnight EST")
    assert.equal(time.date(NEW_YORK, Date.UTC(2026, 10, 2, 4, 30)), "2026-11-01", "23:30 EST is still Sunday")
    assert.equal(time.date(NEW_YORK, Date.UTC(2026, 10, 2, 5)), "2026-11-02")
    assert.equal(time.minute(NEW_YORK, Date.UTC(2026, 10, 1, 5, 30)), "2026-11-01 01:30", "the first half past one, EDT")
    assert.equal(time.minute(NEW_YORK, Date.UTC(2026, 10, 1, 6, 30)), "2026-11-01 01:30", "and the second, EST")
  }))

test("a day whose midnight its clock skips begins when its clock does", () =>
  everywhere(() => {
    const havana: time.Zone = { name: "America/Havana" }
    const begins = time.start(havana, "2026-03-08")
    assert.equal(time.date(havana, begins), "2026-03-08")
    assert.equal(time.date(havana, begins - 60_000), "2026-03-07")
  }))

test("every instant falls in the owner's day it reads as, between its start and the next", () =>
  everywhere(() =>
    forAll(150, r => {
      const zone: time.Zone = { name: r.pick(["UTC", "America/New_York", "Europe/London", "Europe/Berlin", "Pacific/Chatham"]) }
      const t = Date.UTC(2026, 0, 1) + r.int(366 * 24 * 60) * 60_000 + r.int(60_000)
      const today = time.date(zone, t)
      assert.ok(time.start(zone, today) <= t && t < time.nextDay(zone, t), `${zone.name} ${t}`)
      assert.equal(time.daysBetween(today, time.date(zone, time.nextDay(zone, t))), 1, "and the next is the following day")
    }),
  ))

// The offset in force at `t`: the last that starts at or before it.
const inForce = (os: readonly time.Offset[], t: number): number => os.filter(o => o.from <= t).at(-1)?.minutes ?? NaN

test("law: the offsets read the owner's wall clock, every day of ten years and every minute a change starts", () => {
  const to = Date.UTC(2026, 10, 16)
  const from = to - 10 * 365 * 86_400_000
  for (const name of ["America/Santiago", "America/New_York", "Africa/Casablanca", "Australia/Lord_Howe", "Asia/Kolkata", "UTC"]) {
    const z = time.zoneOf(name)
    assert.ok(z !== null, name)
    const os = time.offsets(z, from, to)
    const reads = (t: number): string => new Date(t + inForce(os, t) * 60_000).toISOString().slice(0, 16).replace("T", " ")
    for (let t = from; t <= to; t += 86_400_000) assert.equal(reads(t), time.minute(z, t), `${name} at ${new Date(t).toISOString()}`)
    for (const o of os.slice(1)) {
      assert.equal(reads(o.from), time.minute(z, o.from), `${name}: the change at ${new Date(o.from).toISOString()}`)
      assert.notEqual(inForce(os, o.from - 60_000), o.minutes, `${name}: a change starts where the offset moves`)
    }
  }
})
