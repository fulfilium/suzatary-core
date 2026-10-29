/** The memory page's answer: a fact's kind is one of the box's four, and its number is a number. */

import { test } from "node:test"
import assert from "node:assert/strict"

import payloads from "../src/wire/generated/payload"

test("a fact's kind is one of the four the box names, and its number is a number", () => {
  const row = { id: 3, at: "2026-09-25T09:00:00", fact: "f", kind: "owner", said: { how: "said", on: { kind: "turn", conversation: "default", turn: 0 } } }
  const answer = (f: unknown) => payloads.MemoryPayload({ facts: [f], cap: 200, forgotten: [2], merged: [], cards: [] }).type
  assert.equal(answer(row), "Val")
  assert.equal(answer({ ...row, kind: "pet" }), "Error", "a fifth kind does not read")
  assert.equal(answer({ ...row, id: "3" }), "Error", "a fact's number is not a string")
})
