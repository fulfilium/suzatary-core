import { test } from "node:test"
import assert from "node:assert/strict"

import act from "../src/view/act"
import app from "../src/app/app"
import time from "../src/app/time"

test("law: the gate follows who writes, and a reading login is offered only what it is told instead", () => {
  app.dispatch({ type: "Unmet" })
  assert.deepEqual(act.gate(), { type: "Reads" })
  assert.equal(act.offered(act.gate(), () => "acts", "told"), "told")
  app.dispatch({ type: "Met", me: { type: "Writes", login: "ada@example.invalid", actor: "ada" }, served: [], categories: [], zone: time.UTC })
  assert.deepEqual(act.gate(), { type: "Writes" })
  assert.equal(act.offered(act.gate(), () => "acts", "told"), "acts")
  assert.deepEqual(act.gate(false), { type: "Reads" })
  app.dispatch({ type: "Unmet" })
})

test("who writes is heard once per change of actor, and not on a repeat or on anything else in App", () => {
  let heard = 0
  const stop = app.hear(app.actor, () => (heard += 1))
  app.dispatch({ type: "Met", me: { type: "Writes", login: "ada@example.invalid", actor: "ada" }, served: [], categories: [], zone: time.UTC })
  app.dispatch({ type: "Met", me: { type: "Writes", login: "ada@example.invalid", actor: "ada" }, served: ["life"], categories: [], zone: time.UTC })
  app.dispatch({ type: "Waiting", waiting: { count: 4, unplaced: {} } })
  app.dispatch({ type: "Unmet" })
  stop()
  app.dispatch({ type: "Met", me: { type: "Writes", login: "ada@example.invalid", actor: "ada" }, served: [], categories: [], zone: time.UTC })
  assert.equal(heard, 2)
  app.dispatch({ type: "Unmet" })
})
