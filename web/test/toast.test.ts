// Two notices close together (`notice.step` in `web/src/view/toast.ts`): the answer to a person's act stands through its
// dwell whatever word about the stores arrives meanwhile, and that word is shown once the act's notice ends, the newest
// of them alone.

import { test } from "node:test"
import assert from "node:assert/strict"

import { notice } from "../src/view/toast"
import { forAll } from "./gen"

const act = (message: string): notice.Notice => ({ type: "Act", message })
const aside = (message: string): notice.Notice => ({ type: "Aside", message })
const offered = (n: notice.Notice, at: number): notice.Event => ({ type: "Offered", notice: n, at })

test("an aside that arrives while an act's notice dwells waits for it, and is shown when it ends", () => {
  const added = notice.step(notice.HIDDEN, offered(act("added a"), 0))
  const moved = notice.step(added, offered(aside("updated with changes made elsewhere"), 10))
  assert.deepEqual(notice.shown(moved), act("added a"))
  assert.deepEqual(notice.shown(notice.step(moved, { type: "Ended", at: 6000 })), aside("updated with changes made elsewhere"))
  assert.equal(notice.step(notice.step(moved, { type: "Ended", at: 6000 }), { type: "Ended", at: 12000 }).type, "Hidden")
})

test("an aside offered after an act's dwell ran out, with no timer heard yet, is shown at once", () =>
  assert.deepEqual(notice.shown(notice.step(notice.step(notice.HIDDEN, offered(act("added a"), 0)), offered(aside("moved"), 7000))), aside("moved")))

// Every script of offers and endings, one millisecond to a few seconds apart.
test("law: an act's notice is shown from its offer through its dwell, unless the person ends it or a later act's notice replaces it", () =>
  forAll(2000, r => {
    let t: notice.T = notice.HIDDEN
    let at = 0
    let lastAct: { readonly n: notice.Notice; readonly at: number } | null = null
    for (let i = 0; i < 12; i += 1) {
      at += 1 + r.int(4000)
      const e: notice.Event = r.bool(0.2) ? { type: "Ended", at } : offered(r.bool() ? act(`act ${i}`) : aside(`aside ${i}`), at)
      t = notice.step(t, e)
      if (e.type === "Ended") lastAct = null
      if (e.type === "Offered" && e.notice.type === "Act") lastAct = { n: e.notice, at }
      if (lastAct !== null && at < lastAct.at + 6000) assert.equal(notice.shown(t), lastAct.n)
    }
  }))
