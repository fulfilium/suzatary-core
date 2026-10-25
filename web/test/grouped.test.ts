// A page holds one answer of the worker, never a join of several: it asks for every turn it holds (`source.asking`) and
// the worker reads back to them (`grouped.covering`). So whatever it reads, newest or older, as the conversation grows,
// it holds every turn from its first to the newest and never fewer than it held, and its rounds are those turns'.

import { test } from "node:test"
import assert from "node:assert/strict"

import type { ConversationPayload, ProposalRow, TurnRow } from "../src/wire/generated/wire"
import type { Window } from "../src/prodrome/core"
import grouped from "../src/chat/grouped"
import { asking, step } from "../src/chat/source"
import type { SourceState } from "../src/chat/source"
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

test("law: whatever a page reads, it holds every turn from its first to the newest, never fewer, grouped", () =>
  forAll(300, r => {
    const c = conversation(r)
    let total = 1 + r.int(c.turns.length)
    let state: SourceState = { kind: "loading", id: "c", about: undefined }
    let held: readonly number[] = []
    for (let k = 0; k < 8; k += 1) {
      const older = state.kind === "loaded" && state.first > 0 && r.bool(0.4)
      total = Math.min(c.turns.length, total + r.int(older ? 2 : 80))
      const read = reader(c, total)
      const payload = grouped.of(grouped.covering(asking(state, older ? "older" : "newest", undefined), read), time.UTC)
      state = step(state, { kind: "window", mode: older ? "older" : "newest", payload, fallback: false, at: 0 })
      assert.ok(state.kind === "loaded")
      const indices = state.turns.map(t => t.index)
      const what = `after read ${k} (${older ? "older" : "newest"}, total ${total})`
      assert.deepEqual(indices, Array.from({ length: total - state.first }, (_, i) => state.kind === "loaded" ? state.first + i : 0), `${what}: every turn from the first to the newest`)
      assert.ok(held.every(i => indices.includes(i)), `${what}: every turn it held`)
      assert.deepEqual(state.rounds, grouped.of({ ...read({}), turns: [...state.turns], first: state.first }, time.UTC).rounds, `${what}: its rounds are its turns'`)
      held = indices
    }
  }))
