import { test } from "node:test"
import assert from "node:assert/strict"

import type { ConversationKind, ConversationRow, SearchHit } from "../src/wire/generated/wire"
import list from "../src/chat/list"
import lo from "../src/algebra/lo"
import { forAll, SLUG, type Rng } from "./gen"

const row = (r: Rng): ConversationRow => {
  const kind: ConversationKind = r.pick(["default", "topic", "topic", "review"])
  const at = `2026-09-${String(10 + r.int(15)).padStart(2, "0")}T0${r.int(10)}:00:00`
  return {
    id: kind === "default" ? "default" : r.string(SLUG, 1 + r.int(8)),
    title: r.string("abc ", 1 + r.int(6)),
    kind,
    opened: at,
    turns: r.int(9),
    standing: r.int(3),
    unplaced: r.array(r.int(3), () => ({ index: r.int(9), why: r.pick(["notHeld", "unasked"] as const) })),
    last: r.string("xyz ", r.int(8)),
    last_at: at,
  }
}

const rows = (r: Rng): ConversationRow[] => r.array(r.int(12), () => row(r))
const filter = (r: Rng): list.Filter => r.pick([list.TOPICS, list.REVIEWS])
const hits = (r: Rng): lo.T<readonly SearchHit[]> => r.pick([lo.loading, lo.error("no"), lo.val([])])
const query = (r: Rng): string => r.pick(["", " ", "deed", " deed ", "lease"])

const event = (r: Rng): list.Event => {
  switch (r.int(5)) {
    case 0: return { type: "Listed", rows: r.bool() ? lo.val(rows(r)) : lo.error("down") }
    case 1: return { type: "Filtered", filter: filter(r) }
    case 2: return { type: "Typed", q: query(r) }
    case 3: return { type: "More" }
    default: return { type: "Found", q: query(r), found: hits(r) }
  }
}

test("search is on exactly while something is typed, and a stale answer changes nothing", () =>
  forAll(300, r => {
    const t = r.array(r.int(20), () => event(r)).reduce(list.step, list.init(r.bool() ? null : "default", 0))
    const typed = query(r)
    const after = list.step(t, { type: "Typed", q: typed })
    assert.equal(after.search.type, typed.trim() === "" ? "Off" : "On")
    if (after.search.type === "On") {
      assert.equal(after.search.q, typed.trim())
      assert.equal(list.step(after, { type: "Found", q: `${typed}-stale`, found: lo.val([]) }), after)
      const found = list.step(after, { type: "Found", q: typed, found: lo.error("no") })
      assert.deepEqual(found.search, { type: "On", q: typed.trim(), found: lo.error("no") })
    }
    assert.equal(list.step(after, { type: "Typed", q: typed }), after, "typing the same query again is no change")
    assert.equal(list.step(t, { type: "Filtered", filter: t.filter }), t, "the same filter is no change")
    assert.deepEqual(list.step(t, { type: "Listed", rows: lo.loading }).search, t.search, "a listing leaves the search alone")
  }))

test("the list's instant moves only by its tick, and every other event keeps it", () =>
  forAll(200, r => {
    const now = r.int(1_000_000)
    const t = r.array(r.int(20), () => event(r)).reduce(list.step, list.init(null, now))
    assert.equal(t.now, now)
    const later = now + 1 + r.int(1000)
    assert.equal(list.step(t, { type: "Ticked", now: later }).now, later)
    assert.equal(list.step(t, { type: "Ticked", now }), t, "the same instant moves nothing")
  }))

test("a section shows a page of its newest rows, More a page more, and another filter starts again at one page", () =>
  forAll(200, r => {
    const t = r.array(r.int(20), () => event(r)).reduce(list.step, list.init(null, 0))
    assert.ok(t.shown >= list.PAGE && t.shown % list.PAGE === 0)
    assert.equal(list.step(t, { type: "More" }).shown, t.shown + list.PAGE)
    const other = t.filter.type === "Topics" ? list.REVIEWS : list.TOPICS
    assert.equal(list.step(t, { type: "Filtered", filter: other }).shown, list.PAGE)
    assert.equal(list.step(t, { type: "Listed", rows: lo.val(rows(r)) }).shown, t.shown, "a listing keeps how far the reader went")
  }))
