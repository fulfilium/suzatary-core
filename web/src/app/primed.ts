// The first read, started by the loader while the app is on its way and taken by the app's first ask of the same path
// (`wire/http.ts`). The loader and the app are two bundles, so the one thing they share is a property of the window.

import wait from "../wire/wait"

declare global {
  var suzataryPrimed: Map<string, primed.Started> | undefined
}

export namespace primed {
  // The read's answer, or why it has none; held as a value, so nothing rejects with no one listening.
  export type Read = { readonly type: "Answered"; readonly r: Response } | { readonly type: "Failed"; readonly e: unknown }

  export type Started = { readonly read: Promise<Read>; readonly stop: AbortController }

  // The path of a chain's fold, the read the loader starts.
  export const fold = (chain: string): string => `/api/${encodeURIComponent(chain)}/fold`

  // Asked as a plain read, bounded as every read is (`ms`): the loader holds no key, and a read needs none.
  export const start = (path: string, f: typeof fetch = fetch, ms: number = wait.MS): void => {
    const held = (globalThis.suzataryPrimed ??= new Map())
    const stop = new AbortController()
    const bound = setTimeout(() => stop.abort(new DOMException("the read's bound passed", "TimeoutError")), ms)
    const read = f(path, { headers: { accept: "application/json" }, signal: stop.signal })
      .then(
        (r): Read => ({ type: "Answered", r }),
        (e: unknown): Read => ({ type: "Failed", e }),
      )
      .finally(() => clearTimeout(bound))
    held.set(path, { read, stop })
  }

  // The read started for `path`, once: a second ask of it is the app's own.
  export const take = (path: string): Promise<Read> | null => {
    const held = globalThis.suzataryPrimed
    const started = held?.get(path) ?? null
    held?.delete(path)
    return started === null ? null : started.read
  }

  // The read started for `path` given up, its answer unread: the app read what the device holds instead, and an answer
  // nobody reads would hold the connection open until its timeout.
  export const cancel = (path: string): void => {
    const held = globalThis.suzataryPrimed
    held?.get(path)?.stop.abort()
    held?.delete(path)
  }
}
export default primed
