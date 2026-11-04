// The add sheet's reading: each refusal names the field it is about, so the sheet says it on that field's line.

import { test } from "node:test"
import assert from "node:assert/strict"

import { draft, emptyDraft } from "../src/todo/draft"
import parsed from "../src/algebra/parsed"
import price from "../src/todo/price"

const about = (p: parsed.T<unknown>): parsed.About | null => (p.type === "Refused" ? p.about : null)

test("each refusal of the add sheet's reading names the field it is about", () => {
  const blank = emptyDraft(["errands", "health"])
  assert.deepEqual(about(draft(blank)), parsed.field("body"), "no action line")
  const said = { ...blank, body: { ...blank.body, v: "Renew the library card" } }
  assert.deepEqual(about(draft(said)), parsed.field("category"), "no category")
  const filed = { ...said, category: { ...said.category, v: "errands" } }
  const undated = { ...filed, price: { ...filed.price, v: price.deadline("", 14) } }
  assert.deepEqual(about(draft(undated)), parsed.field("price"), "a deadline with no date")
  assert.equal(draft(filed).type, "Ok")
})
