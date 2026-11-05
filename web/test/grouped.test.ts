// A page holds one answer of the worker, never a join of several: it asks for every turn it holds, back to its first
// (`app/pages/src/conversation.rs`'s `asking`), and the worker reads back to that turn (`grouped.covering`), so the
// window holds every turn from it to the newest, and its rounds are those turns'.

import { test } from "node:test"
import assert from "node:assert/strict"

import type { ConversationPayload, ProposalRow, TurnRow } from "../src/wire/generated/wire"
import type { Window } from "../src/prodrome/core"
import grouped from "../src/chat/grouped"
import time from "../src/app/time"
import { aCard } from "./cards"
import { forAll, type Rng } from "./gen"

const turnAt = (r: Rng, index: number, day: number): TurnRow => {
  const at = r.bool(0.05) ? "" : `2026-09-${String(10 + day).padStart(2, "0")}T${String(r.int(24)).padStart(2, "0")}:00:00`
  const base = { index, text: r.bool(0.1) ? "" : `turn ${index}`, at, attempt: r.bool(0.1) ? ("earlier" as const) : ("current" as const), gone: null }
  const kind = index === 0 ? 0 : r.int(4)
  if (kind === 0) return { ...base, who: "person", references: [], attachments: [], queued: false }
  if (kind === 1) return { ...base, who: "model", blocks: [] }
  if (kind === 2) return { ...base, who: "verb", verb: r.pick(["search", "todo_show", "context"]), outcome: { is: "answered" } }
  return { ...base, who: "act", references: [] }
}

const card = (index: number, turn: number): ProposalRow => ({
  index,
  turn,
  act: { kind: "email", account: "a", to: [], cc: [], subject: "s", body: "b", attachments: [], held: { type: "Read", values: [] } },
  card: aCard(),
  why: "because",
  sources: ["mail:abc123abc123"],
  state: "pending",
} as unknown as ProposalRow)

// A conversation of `n` turns over a few days, and its cards, each said after a turn.
const conversation = (r: Rng) => {
  const n = 1 + r.int(30)
  let day = 0
  const turns = Array.from({ length: n }, (_, i) => {
    if (r.bool(0.15)) day += 1
    return turnAt(r, i, day)
  })
  const cards = Array.from({ length: r.int(6) }, (_, i) => card(i, r.int(n + 1)))
  return { turns, cards }
}

// The core's window of the first `total` turns: `[max(0, before - limit), before)`, and every card said by then.
const reader = (c: ReturnType<typeof conversation>, total: number) => (w: Window): ConversationPayload => {
  const before = Math.min(w.before ?? total, total)
  const first = Math.max(0, before - (w.limit ?? 60))
  return {
    id: "c",
    title: "t",
    model: "m",
    opened: "2026-09-10",
    turns: c.turns.slice(first, before),
    proposals: c.cards.filter(p => p.turn <= total),
    unplaced: [],
    state: "idle",
    first,
    total,
    again: null,
  }
}

test("law: a window asked to reach back to a turn holds every turn from it to the newest, grouped as its turns are", () =>
  forAll(300, r => {
    const c = conversation(r)
    const total = 1 + r.int(c.turns.length)
    const read = reader(c, total)
    const from = r.int(total)
    const limit = 1 + r.int(80)
    const p = grouped.covering({ limit, from }, read)
    const indices = p.turns.map(t => t.index)
    assert.ok(p.first <= from, `it reaches turn ${from} (from ${p.first})`)
    assert.deepEqual(indices, Array.from({ length: total - p.first }, (_, i) => p.first + i), "every turn from its first to the newest")
    assert.deepEqual(grouped.of(p, time.UTC).rounds, grouped.of({ ...read({}), turns: [...p.turns], first: p.first }, time.UTC).rounds, "its rounds are its turns'")
  }))
