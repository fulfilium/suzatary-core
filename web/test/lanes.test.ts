// The core's worker serves the page on screen before the shell: `questions.listen` over a port the test delivers to,
// an answerer whose answers wait where the test says, and every task the queue asks for run when the test says.

import { test } from "node:test"
import assert from "node:assert/strict"

import lanes from "../src/prodrome/lanes"
import questions from "../src/prodrome/questions"
import { forAllAsync } from "./gen"

type Seen = { readonly type: "Arrived" | "Started" | "Read" | "Sent"; readonly id: number }

// Every promise continuation that can run now, run.
const flushed = (): Promise<void> => new Promise(done => setTimeout(done, 0))
const KEYS = ["a", "b", "c"]

// Each question is asked for one of a few readings, in the lane `lanes.watchers` gives it then: the page on screen and a
// program behind it (the Inbox's badge) each watch some of the readings and let them go, as the test says.
test("law: a shell question starts, reads and is answered only while no page question waits or is being answered, each lane starts in the order it came, and a reading the page on screen watches is never behind a later one only a program behind it does", () =>
  forAllAsync(300, async r => {
    // An answer now, or one that waits and then reads after its turn, or reads nothing more.
    type Answers = "Now" | "WaitsThenReads" | "Waits"
    const asked = r.array(1 + r.int(12), (): { readonly key: string; readonly answers: Answers } => ({
      key: r.pick(KEYS),
      answers: r.pick<Answers>(["Now", "WaitsThenReads", "Waits"]),
    }))
    const watchers = lanes.watchers()
    const stops = new Map<string, () => void>()
    const lanesOf: lanes.Lane[] = []
    const visible: boolean[] = []
    const seen: Seen[] = []
    const waiting = new Map<number, () => void>()
    const tasks: (() => void)[] = []
    let deliver: (e: { readonly data: questions.Asked }) => void = () => {}
    const port: questions.Port<questions.Asked, questions.Answered> = {
      postMessage: m => {
        assert.deepEqual(m.answer, { type: "Val", v: [String(m.id)] })
        seen.push({ type: "Sent", id: m.id })
      },
      addEventListener: (_, f) => void (deliver = f),
    }
    // The question's number travels as its one pair's key, and comes back as its one key.
    const answer: questions.Answering = (q, turn) => {
      const id = q.type === "KeepBlocks" ? Number(q.pairs[0]?.[0]) : -1
      seen.push({ type: "Started", id })
      const read = (): string[] => (seen.push({ type: "Read", id }), [String(id)])
      const waited = (): Promise<void> => new Promise<void>(done => waiting.set(id, done))
      const answers = asked[id]?.answers ?? "Now"
      return answers === "Now" ? read() : answers === "WaitsThenReads" ? waited().then(turn).then(read) : waited().then((): string[] => [String(id)])
    }
    questions.listen(port, Promise.resolve(answer), f => void tasks.push(f))
    let next = 0
    for (;;) {
      await flushed()
      const can = [
        ...(next < asked.length ? ["Deliver" as const, "Watch" as const] : []),
        ...(waiting.size > 0 ? ["Resolve" as const] : []),
        ...(tasks.length > 0 ? ["Task" as const] : []),
      ]
      if (can.length === 0) break
      const step = r.pick(can)
      if (step === "Deliver") {
        const id = next
        next += 1
        seen.push({ type: "Arrived", id })
        lanesOf[id] = watchers.laneOf(asked[id]?.key ?? "")
        visible[id] = stops.has(`Screen ${asked[id]?.key ?? ""}`)
        deliver({ data: { id, lane: lanesOf[id], question: { type: "KeepBlocks", pairs: [[String(id), ""]] } } })
      } else if (step === "Watch") {
        const [who, key] = [r.pick(["Screen", "Behind"]), r.pick(KEYS)]
        const stop = stops.get(`${who} ${key}`)
        if (stop === undefined) stops.set(`${who} ${key}`, watchers.watch(key, who === "Screen" ? lanes.FIRST : lanes.LAST))
        else (stop(), stops.delete(`${who} ${key}`))
      } else if (step === "Resolve") {
        const [id, done] = r.pick([...waiting])
        waiting.delete(id)
        done()
      } else tasks.shift()?.()
    }
    const lane = (id: number): lanes.Lane => lanesOf[id] ?? lanes.FIRST
    const owed = new Set<number>()
    for (const s of seen) {
      if (s.type === "Arrived" && lane(s.id) === "Page") owed.add(s.id)
      if (s.type === "Sent") owed.delete(s.id)
      if (s.type !== "Arrived" && lane(s.id) === "Shell") assert.deepEqual([...owed], [], `shell question ${s.id} ${s.type} while page questions ${[...owed]} were owed`)
    }
    for (const l of lanes.ORDER) {
      const of = (type: Seen["type"]): number[] => seen.filter(s => s.type === type && lane(s.id) === l).map(s => s.id)
      assert.deepEqual(of("Started"), of("Arrived"), `the ${l} lane starts in the order it came`)
    }
    const at = (type: Seen["type"], id: number): number => seen.findIndex(s => s.type === type && s.id === id)
    for (const [v] of visible.entries()) {
      for (let b = v + 1; b < asked.length; b += 1) {
        if (visible[v] === true && visible[b] === false) assert.ok(at("Started", b) > at("Sent", v), `${b}, only behind the page, started before ${v}, on screen, was answered`)
      }
    }
    assert.deepEqual(seen.filter(s => s.type === "Sent").map(s => s.id).sort((a, b) => a - b), asked.map((_, i) => i), "every question is answered once")
  }))
