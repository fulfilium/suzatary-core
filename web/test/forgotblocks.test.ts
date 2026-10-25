// A purge runs in the core's worker, which forgets the blocks erased words fed from its own table and the device's; the
// page keeps a table of its own, so what the worker says it forgot reaches that table through the wiring the page's
// entry runs (`storesync.hearWorker`), and a block the page asked for before the purge is not written back after it.

import { test } from "node:test"
import assert from "node:assert/strict"

import rendered from "../src/app/rendered"
import table from "../src/algebra/table"
import type questions from "../src/prodrome/questions"
import remote from "../src/prodrome/remote"
import storesync from "../src/prodrome/storesync"

// A worker's port: what the page posts is dropped, and `say` sends the page a message as the worker would.
const worker = (): { readonly port: questions.Port<questions.Out, questions.Asked>; readonly say: (s: questions.Said) => void } => {
  let listener: (e: { data: questions.Out }) => void = () => {}
  return {
    port: { postMessage: () => {}, addEventListener: (_, f) => void (listener = f) },
    say: said => listener({ data: { said } }),
  }
}

test("what the worker says it forgot, by name or all of it, is forgotten from the page's table, and stays forgotten", async () => {
  const page = rendered.memory(new Set(["answer"]))
  const [gone, kept] = [table.key("answer", "erased"), table.key("answer", "kept")]
  await page.join([[gone, "<p>erased words</p>"], [kept, "<p>other words</p>"]])
  const w = worker()
  remote.over(w.port)
  storesync.hearWorker({ gone: () => {}, stale: () => {}, unloaded: () => {} }, page)
  w.say({ type: "Forgot", blocks: [gone] })
  assert.deepEqual(await page.get([gone, kept]), [null, "<p>other words</p>"])
  // A join asked before the purge that lands after it does not bring the erased block back.
  await page.join([[gone, "<p>erased words</p>"]])
  assert.deepEqual(await page.get([gone]), [null])
  w.say({ type: "Cleared" })
  assert.deepEqual(await page.get([kept]), [null])
})
