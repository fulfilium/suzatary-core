// A loop is a refused text: a compile the editor's worker has not answered two seconds after it was ready terminates
// the worker, and the text is answered with a refusal.

import { test } from "node:test"
import assert from "node:assert/strict"

import editor from "../src/typst/editor"
import type edit from "../src/typst/edit"

type Listener = (e: { readonly data: edit.Answered }) => void

// A worker that compiles the first text at once and never answers another, as a text that loops would not.
class LoopingWorker {
  static readonly made: LoopingWorker[] = []
  readonly listeners: Listener[] = []
  readonly posted: edit.Asked[] = []
  terminated = false
  constructor() {
    LoopingWorker.made.push(this)
  }
  addEventListener(type: string, f: Listener): void {
    if (type === "message") this.listeners.push(f)
  }
  postMessage(m: edit.Asked): void {
    this.posted.push(m)
    const answer: edit.Answer = { type: "Check", checked: { type: "Html", html: "<p>fine</p>" } }
    if (this.posted.length === 1) queueMicrotask(() => this.listeners.forEach(f => f({ data: { id: m.id, answer } })))
  }
  terminate(): void {
    this.terminated = true
  }
}

const settle = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await Promise.resolve()
}

test("a compile past two seconds terminates the worker and is answered as a refused text", async t => {
  Object.assign(globalThis, { window: globalThis, Worker: LoopingWorker })
  t.mock.timers.enable({ apis: ["setTimeout"] })
  const first = await editor.effects.ask({ type: "Check", source: "= Fine" })
  assert.deepEqual(first, { type: "Check", checked: { type: "Html", html: "<p>fine</p>" } })
  let answer: edit.Answer | null = null
  void editor.effects.ask({ type: "Check", source: "#let f(x) = f(x)\n#f(1)" }).then(a => (answer = a))
  t.mock.timers.tick(1999)
  await settle()
  assert.equal(answer, null, "a compile within the limit is waited for")
  assert.equal(LoopingWorker.made[0]?.terminated, false)
  t.mock.timers.tick(1)
  await settle()
  assert.deepEqual(answer, { type: "Check", checked: { type: "Failed", diagnostics: [{ message: "the text did not finish compiling within 2 s", hints: [], from: null, to: null }] } })
  assert.equal(LoopingWorker.made[0]?.terminated, true, "the worker is terminated")
})
