// The tab's side of the core's worker: each chain's objects sent once, then only the names added and dropped, and a
// question by message with its answer as a promise. Nothing here reads a chain; what the worker holds is what the last
// `hold` said, since the messages arrive in the order they were sent.

import type { Named } from "./core"
import { CoreError } from "./core"
import questions from "./questions"
import lanes from "./lanes"

export namespace remote {
  // A chain as the tab holds it: its objects and its policy.
  export type Chain = { readonly name: string; readonly objects: readonly Named[]; readonly untrusted: readonly string[] }

  export type Remote = {
    // The chains held, each by what changed since the last holding.
    readonly hold: (chains: readonly Chain[]) => Promise<questions.Answer<"Hold">>
    // `q` in the lane of whoever asks it (`lanes`).
    readonly ask: <K extends questions.Kind>(q: questions.Of<K>, lane: lanes.Lane) => Promise<questions.Answer<K>>
  }

  // Who hears what the worker says unasked; one hearer, the page's shell, set before the worker starts.
  let hearer: (s: questions.Said) => void = () => {}
  export const hear = (f: (s: questions.Said) => void): void => {
    hearer = f
  }

  // What each chain holds as `hold` last told it, by name.
  // Nothing, once the worker refused a holding: the next is sent whole.
  type Sent = Map<string, ReadonlySet<string>> | null

  // The change from what was sent to `c`: pure, so a holding is a function of the two sets.
  const delta = (sent: ReadonlySet<string> | null, c: Chain): questions.Holding => {
    const now = new Set(c.objects.map(o => o.hash))
    return {
      name: c.name,
      untrusted: c.untrusted,
      add: sent === null ? c.objects : c.objects.filter(o => !sent.has(o.hash)),
      drop: sent === null ? [] : [...sent].filter(h => !now.has(h)),
      whole: sent === null,
    }
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
    let sent: Sent = new Map()
    return {
      // Sent before it returns, with no wait between reading `sent` and writing it, so two holdings in a row are each the
      // change from the one before; a refused one makes the next whole. Every holding goes in the first lane, so the
      // worker takes them in the order they were sent, each before any question asked after it.
      hold: chains => {
        const was = sent
        const holding = chains.map(c => delta(was === null ? null : (was.get(c.name) ?? new Set()), c))
        sent = new Map(chains.map(c => [c.name, new Set(c.objects.map(o => o.hash))]))
        return ask({ type: "Hold", chains: holding }, lanes.FIRST).catch((e: unknown) => {
          sent = null
          throw e
        })
      },
      ask,
    }
  }

  // The tab's one worker, started by the first question.
  let started: Remote | null = null
  const worker = (): Remote => {
    if (started !== null) return started
    const w = new Worker(new URL("./prodromework.js", import.meta.url), { type: "module" })
    started = over(w, fail => w.addEventListener("error", e => fail(`the core's worker stopped: ${e.message}`)))
    return started
  }

  export const hold = (chains: readonly Chain[]): Promise<questions.Answer<"Hold">> => worker().hold(chains)
  export const ask = <K extends questions.Kind>(q: questions.Of<K>, lane: lanes.Lane): Promise<questions.Answer<K>> => worker().ask(q, lane)
}
export default remote
