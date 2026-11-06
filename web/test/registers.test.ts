// Every register this browser keeps reads back what it stored, and anything else stored reads as nothing written.

import { test } from "node:test"
import assert from "node:assert/strict"

import registers from "../src/app/registers"
import register from "../src/algebra/register"
import ui from "../src/app/ui"
import kv from "../src/app/kv"
import { memoryStore } from "./gen"
import lo from "../src/algebra/lo"

test("law: a kept register's initial value reads back through its own parser", () => {
  for (const k of Object.values(registers)) if (k.tier !== "Tab") assert.deepEqual(k.value(JSON.parse(JSON.stringify(k.initial))), lo.val(k.initial), k.name)
})

test("stored junk is nothing written: a theme, and a page's register of any value", () => {
  const m = memoryStore()
  const tab = ui.over(kv.over(() => m), "tab")
  m.setItem("suzatary.ui.theme/", "{not json")
  assert.equal(tab.cell(registers.theme).get(), "auto")
  m.setItem("suzatary.ui.theme/", JSON.stringify(register.write(register.EMPTY, "tab", "sepia")))
  assert.equal(tab.cell(registers.theme).get(), "auto")
  m.setItem("suzatary.ui.kept/todos.filter", "{not json")
  assert.equal(tab.cell(registers.kept, "todos.filter").get(), null)
})

test("a Tab register never reaches storage; a Device one does, under its own name", () => {
  const m = memoryStore()
  const tab = ui.over(kv.over(() => m), "tab")
  tab.cell(registers.page, "todos.opened/life").set("dentist")
  assert.equal(m.size(), 0)
  tab.cell(registers.panel).set({ inbox: "open" })
  assert.equal(m.size(), 0, "the panel's presses are this tab's")
  tab.cell(registers.chain).set("suzatary")
  assert.equal(m.size(), 1)
  assert.notEqual(m.getItem("suzatary.ui.chain/"), null)
})

test("law: every register's name holds no slash, so a stored key names its register and key", () => {
  for (const k of Object.values(registers)) assert.ok(!k.name.includes("/"), k.name)
  const m = memoryStore()
  const [one, two] = [ui.over(kv.over(() => m), "one"), ui.over(kv.over(() => m), "two")]
  let heard = 0
  two.cell(registers.kept, "todos.filter").hear(() => (heard += 1))
  one.cell(registers.kept, "todos.filter").set("x")
  two.heard("suzatary.ui.kept/todos.filter")
  assert.equal(heard, 1)
})
