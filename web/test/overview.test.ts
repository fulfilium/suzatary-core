// The overview's answers as the box writes them: they decode, hold every group once in order and name no secret.

import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import payload from "../src/wire/generated/payload"
import { GROUPS } from "./gen"

test("the box's own overview decodes, holds every group once in order, and names no secret", () => {
  const samples: unknown = JSON.parse(readFileSync("app/core/tests/web/answers.json", "utf8"))
  const sampled = typeof samples === "object" && samples !== null && "Overview" in samples && Array.isArray(samples.Overview) ? samples.Overview : []
  assert.ok(sampled.length > 0, "the box writes an overview")
  for (const sample of sampled) {
    const read = payload.Overview(sample.value)
    if (read.type !== "Val") throw new Error(`an Overview sample does not decode: ${JSON.stringify(read)}`)
    assert.deepEqual(read.v.groups.map(g => g.group), [...GROUPS])
    assert.equal(/PLUGIN_|_API_KEY|_TOKEN/.test(JSON.stringify(read.v)), false)
  }
})

