// The held kinds' names: the page's table (`heldvalue.NAMES`) against the box's (`held-kinds.json`, from `redact::Kind::name`).

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import heldvalue from "../src/view/heldvalue"

test("every kind is named as the box names it, and each name reads as a placeholder's kind", () => {
  const box: unknown = JSON.parse(readFileSync("app/core/tests/web/held-kinds.json", "utf8"))
  assert.deepEqual(heldvalue.NAMES, box)
  for (const name of Object.values(heldvalue.NAMES)) {
    const s = `[${name} ending 6789 · withheld]`
    assert.deepEqual(
      heldvalue.tokens(s).map((t) => heldvalue.fold(t, (x) => [x], (kind, hint, handle) => [kind, hint, handle])),
      [[name, "6789", null]],
    )
  }
})
