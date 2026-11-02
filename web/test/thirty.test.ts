// The todo page's thirty days, sampled by the built core from the fixture chain.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import type { Core } from "../src/prodrome/core"
import { appendTo, foldObjects, lifecycleEvent, linked, linker, linkerOf, objectsOf, within } from "../src/prodrome/core"
import curve from "../src/todo/curve"
import time from "../src/app/time"
import thirty from "../src/todo/thirty"
import cast from "./cast"
import { heldChain, holdAll } from "./held"

const core = (): Core => {
  const built = process.env["SUZATARY_PRODROME_WASM"]
  if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
  return createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
}
const wasm = core()
const chain = JSON.parse(readFileSync("app/core/tests/web/chain.json", "utf8")) as {
  links: { hash: string; literal: string }[]
  untrusted: string[]
}
const urgent = cast.world.todos.urgent.id
// The set the line links through: none held, so only what the todo's own chain says draws.
const none = new wasm.Chains()
// A sample of `links`, read once by the core and freed after it.
const sample = (links: readonly { hash: string; literal: string }[], ...rest: [readonly string[], string, number]): thirty.Thirty => {
  const t = new wasm.Todos(objectsOf(links))
  try {
    return thirty.sample(wasm, none, t, ...rest)
  } finally {
    t.free()
  }
}
const SECOND = time.parse("2026-09-02T12:00:00") * 1000

const nowValue = (t: thirty.Thirty): number | null =>
  thirty.fold(t, {
    Sampling: () => null,
    Unheld: () => null,
    Unpriced: () => null,
    Drawn: s => curve.fold(s, { Unsampled: () => null, Sampled: d => d.ahead[0].v }),
  })

// The box authors with microseconds and the page samples a moment later in the same second: what it wrote is believed.
test("an event dated earlier in the sampling second is part of the thirty days", () => {
  const replica = new wasm.Todos(objectsOf(chain.links))
  const done = appendTo(replica, lifecycleEvent(wasm, "Completed", urgent, "2026-09-02T12:00:00.300", cast.world.owner.actor))
  replica.free()
  const objects = [...chain.links, ...done.objects]
  const before = nowValue(sample(chain.links, chain.untrusted, urgent, SECOND + 600))
  assert.ok(before !== null && before < 1, `open, it is worth ${String(before)}`)
  assert.equal(nowValue(sample(objects, chain.untrusted, urgent, SECOND + 600)), 1)
})

test("the curve's first sample from now on is the sampling moment itself", () => {
  const t = sample(chain.links, chain.untrusted, urgent, SECOND + 600)
  assert.equal(
    thirty.fold(t, {
      Sampling: () => null,
      Unheld: () => null,
      Unpriced: () => null,
      Drawn: s => curve.fold(s, { Unsampled: () => null, Sampled: d => d.ahead[0].t }),
    }),
    (SECOND + 600) / 1000,
  )
})

test("a todo no held object names is unheld, not an error", () => {
  assert.equal(sample(chain.links, chain.untrusted, "nosuchtodo", SECOND).type, "Unheld")
  assert.equal(sample([], chain.untrusted, urgent, SECOND).type, "Unheld")
})

test("a todo held with no spec and no checklist has no function, and says so", () => {
  const created = chain.links.filter(l => l.literal.includes(`event=Created(todo='${urgent}'`))
  assert.equal(created.length, 1)
  // A replica is a down-set: the change, and the genesis it rests on.
  const genesis = chain.links.filter(l => l.literal.startsWith("Genesis("))
  assert.equal(sample([...genesis, ...created], chain.untrusted, urgent, SECOND).type, "Unpriced")
})

// A reference into a chain the set holds reads that chain's row; one into a chain it does not hold reads ∅.
test("a reference into another chain is linked through the set the rows are priced through", () => {
  const f = foldObjects(wasm, chain.links, "2026-09-02T12:00:00", chain.untrusted)
  const home = linked(wasm, linker(within(wasm, f.functions)), f.env, f.at)({ kind: "ref", todo: urgent })
  assert.ok(home !== null, "the todo prices in its own chain")
  const held = holdAll(new wasm.Chains(), [heldChain("life", chain.links, chain.untrusted)])
  try {
    const across = { kind: "refIn", store: "life", entity: urgent } as const
    const [through, nothing] = [linkerOf(held, {}, f.at), linkerOf(none, {}, f.at)]
    try {
      assert.equal(linked(wasm, linker(t => through.link(t)), f.env, f.at)(across), home)
      assert.equal(linked(wasm, linker(t => nothing.link(t)), f.env, f.at)(across), null)
    } finally {
      through.free()
      nothing.free()
    }
  } finally {
    held.free()
  }
})
