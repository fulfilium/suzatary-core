// A tab's act is the core's `append`: a `Change` over its todo's heads in the tab's own fold, and two tabs writing apart converge by adoption in either order.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createHash } from "node:crypto"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import type { Core, Named } from "../src/prodrome/core"
import { appendTo, lifecycleEvent, objectsOf, verifyObjects } from "../src/prodrome/core"
import cast from "./cast"
import { forAll, type Rng } from "./gen"

const core = ((): Core => {
  const built = process.env["SUZATARY_PRODROME_WASM"]
  if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
  return createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
})()
const chain = JSON.parse(readFileSync("app/api/tests/web/chain.json", "utf8")) as {
  links: Named[]
  untrusted: string[]
}
const base: Named[] = chain.links.map(l => ({ hash: l.hash, literal: l.literal }))
const actor = cast.world.owner.actor
const ids = [cast.world.todos.urgent.id, cast.world.todos.flat.id, cast.world.todos.checklist.id, cast.world.todos.high.id]
const AT = "2026-09-30T00:00:00"

const sha = (text: string): string => createHash("sha256").update(text, "utf8").digest("hex")

/** A replica's acts, each sealed by the one core instance that read it: the objects cross once per act. */
const writing = <T>(held: readonly Named[], write: (append: (event: string) => Named[]) => T): T => {
  const t = new core.Todos(objectsOf(held))
  try {
    return write(event => appendTo(t, event).objects)
  } finally {
    t.free()
  }
}

const listed = (held: readonly Named[]): string => {
  const t = new core.Todos(objectsOf(held))
  try {
    return t.list(AT, JSON.stringify(chain.untrusted), "life", new core.Chains())
  } finally {
    t.free()
  }
}

const tips = (held: readonly Named[]): string[] => [...verifyObjects(core, held).tips].sort()

const act = (r: Rng, day: number): string =>
  lifecycleEvent(core, r.pick(["Completed", "Cancelled", "Reopened"] as const), r.pick(ids), `2026-09-${String(day).padStart(2, "0")}T${String(r.int(24)).padStart(2, "0")}:00:00`, actor)

const union = (...sets: (readonly Named[])[]): Named[] => {
  const seen = new Set<string>()
  return sets.flat().filter(o => !seen.has(o.hash) && (seen.add(o.hash), true))
}

test("an act is a Change over its todo's heads in this replica, named by the sha256 of its print", () => {
  const todo = cast.world.todos.urgent.id
  const event = lifecycleEvent(core, "Completed", todo, "2026-09-10T12:00:00", actor)
  const [object, ...more] = writing(base, append => append(event))
  assert.ok(object !== undefined && more.length === 0, "one new object")
  assert.equal(object.hash, sha(object.literal), "the name is what the box rehashes")
  const found = /^Change\(genesis='([0-9a-f]{64})', deps=\(([^)]*)\), event=(.*)\)$/.exec(object.literal)
  assert.ok(found !== null, object.literal)
  assert.equal(found[3], event)
  const writes = base.filter(o => o.literal.includes(`todo='${todo}'`)).map(o => o.hash)
  const deps = [...(found[2] ?? "").matchAll(/[0-9a-f]{64}/g)].map(m => m[0])
  assert.ok(deps.length > 0 && deps.every(dep => writes.includes(dep)), "it rests on writes to its own todo, never on the box's head")
  const verdict = verifyObjects(core, [...base, object])
  assert.deepEqual(verdict.found, { type: "Reached" })
})

test("a twin of a held event writes nothing and answers the held name", () => {
  const event = lifecycleEvent(core, "Cancelled", cast.world.todos.flat.id, "2026-09-11T08:00:00", actor)
  const [first] = writing(base, append => append(event))
  assert.ok(first !== undefined)
  const again = writing([...base, first], append => append(event))
  assert.deepEqual(again, [], "the box already lacks nothing of it")
})

test("several heads are no reason to refuse: a replica that wrote to two todos apart writes a third", () => {
  const events = ids.map((todo, n) => lifecycleEvent(core, "Completed", todo, `2026-09-1${n}T09:00:00`, actor))
  const written = writing(base, append => events.flatMap(append))
  const held = [...base, ...written]
  assert.ok(tips(held).length > 1, "the acts rest on different todos' heads")
  assert.equal(verifyObjects(core, held).found.type, "Reached")
})

test("two tabs writing apart converge by adoption in either order", () => {
  forAll(12, r => {
    const left = writing(base, append => r.array(1 + r.int(4), () => append(act(r, 10 + r.int(9)))).flat())
    const right = writing(base, append => r.array(1 + r.int(4), () => append(act(r, 10 + r.int(9)))).flat())
    const leftFirst = union(base, left, right)
    const rightFirst = union(base, right, left)
    assert.equal(verifyObjects(core, leftFirst).found.type, "Reached")
    assert.deepEqual(tips(leftFirst), tips(rightFirst), "one set of heads")
    assert.equal(listed(leftFirst), listed(rightFirst), "one reading, whichever arrived first")
    // Having adopted each other, the next act is the same bytes on either side: deps come from the union.
    const next = act(r, 20)
    assert.deepEqual(
      writing(leftFirst, append => append(next)),
      writing(rightFirst, append => append(next)),
    )
  })
})
