// The one door to the box: a GET or an act, and its answer decoded or refused.

import type d from "./decoder"
import lo from "../algebra/lo"
import payload from "./generated/payload"
import kv from "../app/kv"
import inflight from "../app/inflight"
import primed from "../app/primed"
import wait from "./wait"

export namespace http {
  // `status` is 0 when the answer came and its shape was wrong, so a page can tell a 404 from a drift without reading prose.
  export class ApiError extends Error {
    readonly status: number
    constructor(message: string, status = 0) {
      super(message)
      this.status = status
    }
  }

  // A custom header forces a CORS preflight the box never answers, so only a page the box served can act (`gate::check_csrf`).
  export const ACT_HEADER = "X-Suzatary-Act"

  // The write contract this build keeps, which the box's `gate::CLIENT_MIN` must not pass: 2 stamps every instant in UTC,
  // 3 signs every write with this device's key, 4 reads the conversations from the lines and forgets what is erased, 5
  // reads the box's turns, card answers, proposed objects, reviews and stored objects as tagged sums, 6 keeps the
  // lines as a skeleton and their content by hash, in a device database a page of contract 5 cannot open, 7 names what
  // a replica holds by the frontier the box last named, in a device database a page of contract 6 cannot open.
  export const CLIENT_HEADER = "X-Suzatary-Client"
  export const CLIENT = 7
  // A box that has moved past this build's contract: it performed nothing, and the page offers the new build (`newbuild`).
  export const OUTDATED = 426
  export const NOT_SENT = "Not sent: this version of the app is older than the box's. Reload to the new version, then send it again."

  // A box this build may no longer read: it refused this build's contract, or answered in a shape this build does not
  // know. Who hears it asks the box which build it serves and offers it; one heard before anyone listens is kept.
  let unheard = false
  let stale = (): void => void (unheard = true)
  export const whenStale = (f: () => void): void => {
    stale = f
    if (unheard) f()
    unheard = false
  }

  export const actRequest = (body: unknown): RequestInit => ({
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json", [ACT_HEADER]: "1", [CLIENT_HEADER]: String(CLIENT) },
    body: JSON.stringify(body),
  })

  // The box's own sentence for a refusal, shown verbatim: it names the field that was wrong.
  export const errorOf = (u: unknown): string | null => lo.withDefault(lo.map(payload.ErrorPayload(u), p => p.error), null)

  const decoded = <A>(path: string, decoder: d.Decoder<A>, u: unknown): A => {
    const r = decoder(u)
    if (r.type === "Val") return r.v
    stale()
    throw new ApiError(`${path} → unexpected shape (${r.e})`)
  }


  // Each read's last answer by path, with the tag the box gave it (`serve::validated`): the next ask of the path sends
  // the tag, and a 304 is that answer again. The text is kept and parsed again, never the value, which a decoder may
  // hand back as it was given. The most recently asked `KEPT` paths, so a tab reading mail after mail holds a few.
  export const KEPT = 24
  type Kept = { readonly tag: string; readonly text: string }
  const kept = new Map<string, Kept>()
  const keep = (path: string, k: Kept): void => {
    kept.delete(path)
    kept.set(path, k)
    for (const old of kept.keys()) if (kept.size > KEPT) kept.delete(old)
  }
  export const held = (path: string): string | null => kept.get(path)?.tag ?? null

  // The protocol the box last answered an API read over ("h2", "http/1.1"), or "" where the browser has not said. The
  // page's own load may come from the worker's cache, which names none, so the API's answers are what is read.
  export const protocol = (): string => {
    const entries = typeof performance === "undefined" ? [] : performance.getEntriesByType("resource")
    for (let i = entries.length - 1; i >= 0; i -= 1) {
      const e = entries[i]
      if (e !== undefined && e.name.includes("/api/") && "nextHopProtocol" in e && typeof e.nextHopProtocol === "string" && e.nextHopProtocol !== "") return e.nextHopProtocol
    }
    return ""
  }
  export const NOT_MODIFIED = 304

  // How a read is bounded: its whole answer within `wait.MS` (`Wall`), or no silence of `ms` between two pieces of its
  // body (`Silence`), for a store's objects, which a phone on a slow network pulls for longer than that and is still pulling.
  export type Bound = { readonly type: "Wall" } | { readonly type: "Silence"; readonly ms: number }
  export const WALL: Bound = { type: "Wall" }

  // A signal that aborts after `ms` with nothing heard, failing the read in those words; each `heard` starts the count again.
  const silence = (path: string, ms: number): { readonly signal: AbortSignal; readonly heard: () => void; readonly done: () => void } => {
    const c = new AbortController()
    let t: ReturnType<typeof setTimeout> | undefined
    const heard = (): void => {
      clearTimeout(t)
      t = setTimeout(() => c.abort(new Error(`${path}: nothing arrived from the box for ${ms / 1000} s`)), ms)
    }
    heard()
    return { signal: c.signal, heard, done: () => clearTimeout(t) }
  }

  // The body as text, every piece that arrives counting as heard; the read fails as the box not answering once the signal aborts.
  const textOf = async (response: Response, signal: AbortSignal, heard: () => void): Promise<string> => {
    const body = response.body
    if (body === null) return response.text()
    const reader = body.getReader()
    const stop = (): void => void reader.cancel(signal.reason).catch(() => undefined)
    if (signal.aborted) stop()
    signal.addEventListener("abort", stop, { once: true })
    const decoder = new TextDecoder()
    let text = ""
    try {
      for (;;) {
        const piece = await reader.read()
        if (signal.aborted) throw signal.reason
        if (piece.done) return text + decoder.decode()
        heard()
        text += decoder.decode(piece.value, { stream: true })
      }
    } finally {
      signal.removeEventListener("abort", stop)
    }
  }

  // The loader's read of this path, when it started one: its answer, or its failure, its timeout's the same error as this
  // read's own. Else, or when the box refused it, this read's own, signed, with the answer it holds, which a 304 means
  // however many paths were kept since.
  const answer = async (path: string, signal: AbortSignal): Promise<[Response, Kept | undefined]> => {
    const early = await primed.take(path)
    if (early?.type === "Answered" && early.r.ok) return [early.r, undefined]
    if (early?.type === "Failed") wait.waited(path)(early.e)
    const had = kept.get(path)
    const [init] = await signed(path, {
      method: "GET",
      headers: { accept: "application/json", ...(had === undefined ? {} : { "if-none-match": had.tag }) },
      signal,
    })
    return [await fetch(path, init).catch(wait.waited(path)), had]
  }

  // A read is signed too, so a device whose key was revoked is told to wipe itself however it next asks.
  export const get = async <A>(path: string, decoder: d.Decoder<A>, bound: Bound = WALL): Promise<A> => {
    const timer = bound.type === "Wall" ? { signal: AbortSignal.timeout(wait.MS), heard: () => undefined, done: () => undefined } : silence(path, bound.ms)
    try {
      const [response, had] = await answer(path, timer.signal)
      if (response.status === GONE) gone()
      // A box past what this build reads: the page offers the new one, as it does for a write.
      if (response.status === OUTDATED) stale()
      if (response.status === NOT_MODIFIED && had !== undefined) {
        keep(path, had)
        return decoded(path, decoder, JSON.parse(had.text))
      }
      if (!response.ok) {
        const answer: unknown = await response.json().catch(() => null)
        throw new ApiError(errorOf(answer) ?? `${path} → HTTP ${response.status}`, response.status)
      }
      const text = await textOf(response, timer.signal, timer.heard).catch(wait.waited(path))
      const tag = response.headers.get("etag")
      if (tag === null) kept.delete(path)
      else keep(path, { tag, text })
      return decoded(path, decoder, JSON.parse(text))
    } finally {
      timer.done()
    }
  }

  // What a write shows of this device (SPEC law 42): its key, and how it signs a request's name; nothing where it cannot.
  export type Signer = { readonly key: string; readonly sign: (name: string) => Promise<string> }
  export type Signs = () => Promise<Signer | null>

  let signs: Signs = () => Promise.resolve(null)
  export const signWith = (s: Signs): void => {
    signs = s
  }

  // The box's answer to a request a revoked key signed (`refusal::Status::Wipe`), and who hears it: an answer that
  // arrives before anyone listens is kept, since a module's first read can run before the page installs its hearer.
  export const GONE = 410
  let early = false
  let gone: () => void = () => {
    early = true
  }
  export const whenGone = (f: () => void): void => {
    gone = f
    if (early) f()
  }

  const hex = (bytes: ArrayBuffer): string => Array.from(new Uint8Array(bytes), b => b.toString(16).padStart(2, "0")).join("")
  const sha256 = async (text: string): Promise<string> => hex(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text)))

  // A request's name, `suzatary_core::signatures::request_name`: what it asks, when, a nonce, with which body, and who asks.
  // The box never records the nonce, so a body cannot be guessed back from a record.
  export const requestName = async (method: string, target: string, at: string, nonce: string, body: string, key: string): Promise<string> =>
    sha256(`suzatary request\n${method} ${target}\n${at}\n${nonce}\n${await sha256(body)}\n${key}`)

  const nonce = (): string => hex(crypto.getRandomValues(new Uint8Array(16)).buffer)

  // A signed request's way out, timed by stage; logged only where this device keeps `suzatary.debug.requests` = "1".
  type Trace = { name: string; at: string; stages: [string, number][] }
  const tracing = (): boolean => kv.local.get("suzatary.debug.requests") === "1"

  // Stamped now, so the box answers a resend of this very request from its record, and two asks made apart are two.
  const signed = async (path: string, init: RequestInit): Promise<[RequestInit, Trace | null]> => {
    const started = performance.now()
    const by = await signs()
    if (by === null) return [init, null]
    const keyed = performance.now()
    const at = new Date().toISOString()
    const drawn = nonce()
    const name = await requestName(init.method ?? "GET", path, at, drawn, typeof init.body === "string" ? init.body : "", by.key)
    const signature = await by.sign(name)
    const stages: [string, number][] = [
      ["key", keyed - started],
      ["sign", performance.now() - keyed],
    ]
    return [
      { ...init, headers: { ...init.headers, "X-Suzatary-At": at, "X-Suzatary-Nonce": drawn, "X-Suzatary-Key": by.key, "X-Suzatary-Signature": signature } },
      { name: name.slice(0, 12), at, stages },
    ]
  }

  const said = (path: string, t: Trace | null, status: number, fetched: number): void => {
    if (t === null || !tracing()) return
    const stages = [...t.stages, ["fetch", fetched]].map(([s, ms]) => `${s}=${Math.round(Number(ms))}ms`).join(" ")
    console.debug(`request POST ${path} name=${t.name} at=${t.at} status=${status} ${stages}`)
  }

  // `bound` is how long, in ms, an act waits for its answer; a caller that sets one asks the box what happened instead of waiting forever.
  export const act = <A>(path: string, body: unknown, decoder: d.Decoder<A>, bound?: number): Promise<A> =>
    inflight.held(acting(path, body, decoder, bound))

  const acting = async <A>(path: string, body: unknown, decoder: d.Decoder<A>, bound?: number): Promise<A> => {
    const [init, trace] = await signed(path, actRequest(body))
    const sent = performance.now()
    const response = await fetch(path, { ...init, signal: bound === undefined ? null : AbortSignal.timeout(bound) }).catch((e: unknown) => {
      said(path, trace, 0, performance.now() - sent)
      throw e
    })
    said(path, trace, response.status, performance.now() - sent)
    const answer: unknown = await response.json().catch(() => null)
    if (response.status === GONE) gone()
    // In the page's words, not the box's: the act can be sent again from the build the Reload loads.
    if (response.status === OUTDATED) {
      stale()
      throw new ApiError(NOT_SENT, OUTDATED)
    }
    if (!response.ok) throw new ApiError(errorOf(answer) ?? `${path} → HTTP ${response.status}`, response.status)
    return decoded(path, decoder, answer)
  }
}
export default http
