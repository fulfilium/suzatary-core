import { test } from "node:test"
import assert from "node:assert/strict"

import mailbox from "../src/painter/mailbox"
import { rng } from "./gen"

test("law: an input sent while a step runs is taken after that step, in the order sent", () => {
  for (let seed = 1; seed <= 300; seed += 1) {
    const r = rng(seed * 2654435761)
    const log: string[] = []
    const sent: number[] = []
    let next = 0
    let depth = 0
    const fresh = (): number => {
      next += 1
      sent.push(next)
      return next
    }
    const box = mailbox.of<number>(n => {
      depth += 1
      assert.equal(depth, 1, `seed ${seed}: input ${n} was taken inside another step`)
      log.push(`take ${n}`)
      if (next < 40) for (let k = r.int(3); k > 0; k -= 1) box.send(fresh())
      depth -= 1
    })
    for (let turn = 0; turn < 4; turn += 1)
      if (r.bool()) box.send(fresh())
      else box.turn(() => {
        log.push("turn")
        for (let k = r.int(3); k > 0; k -= 1) box.send(fresh())
      })
    const taken = log.filter(l => l.startsWith("take ")).map(l => Number(l.slice(5)))
    assert.deepEqual(taken, sent, `seed ${seed}: the inputs were taken in another order than sent`)
  }
})
