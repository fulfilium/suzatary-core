// The tab's side of the core's worker: a question by message, with its answer as a promise. Nothing here reads a store.

import { CoreError } from "./core"
import type questions from "./questions"
import lanes from "./lanes"

export namespace remote {
  export type Remote = {
    readonly ask: <K extends questions.Kind>(q: questions.Of<K>, lane: lanes.Lane) => Promise<questions.Answer<K>>
  }

  // Who hears what the worker says unasked; one hearer, the page's shell, set before the worker starts.
  let hearer: (s: questions.Said) => void = () => {}
  export const hear = (f: (s: questions.Said) => void): void => {
    hearer = f
  }

  // `broken` is said once the worker is gone (its script did not load, or it threw outside a question): every question
  // waiting and every one after fails with why, rather than waiting on an answer that will not come.
  export const over = (port: questions.Port<questions.Out, questions.Asked>, broken: (fail: (why: string) => void) => void = () => {}): Remote => {
    const waiting = new Map<number, { ok: (v: questions.Answer<questions.Kind>) => void; no: (e: Error) => void }>()
    let gone: string | null = null
    broken(why => {
      gone = why
      for (const w of waiting.values()) w.no(new CoreError(why))
      waiting.clear()
    })
    let n = 0
    port.addEventListener("message", e => {
      if ("said" in e.data) return hearer(e.data.said)
      const { id, answer } = e.data
      const w = waiting.get(id)
      waiting.delete(id)
      if (answer.type === "Val") w?.ok(answer.v)
      else w?.no(new CoreError(answer.e))
    })
    const ask = <K extends questions.Kind>(question: questions.Of<K>, lane: lanes.Lane): Promise<questions.Answer<K>> =>
      new Promise((ok, no) => {
        if (gone !== null) return no(new CoreError(gone))
        const id = (n += 1)
        // The worker answers question `id` with what that question's kind answers.
        waiting.set(id, { ok: v => ok(v as questions.Answer<K>), no })
        port.postMessage({ id, lane, question })
      })
    return { ask }
  }

  // The tab's one worker, started by the first question.
  let started: Remote | null = null
  const worker = (): Remote => {
    if (started !== null) return started
    const w = new Worker(new URL("./prodromework.js", import.meta.url), { type: "module" })
    started = over(w, fail => w.addEventListener("error", e => fail(`the core's worker stopped: ${e.message}`)))
    return started
  }

  export const ask = <K extends questions.Kind>(q: questions.Of<K>, lane: lanes.Lane): Promise<questions.Answer<K>> => worker().ask(q, lane)
}
export default remote
