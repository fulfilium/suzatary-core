// The one door to the box: a GET or an act, and its answer decoded or refused.

import type d from "./decoder"
import lo from "../algebra/lo"
import payload from "./generated/payload"
import kv from "../app/kv"
import inflight from "../app/inflight"
import primed from "../app/primed"
import wait from "./wait"
import text from "./text"

export namespace http {
  // `status` is 0 when the answer came and its shape was wrong, so a page can tell a 404 from a drift without reading prose.
  export class ApiError extends Error {
    readonly status: number
    constructor(message: string, status = 0) {
      super(message)
      this.status = status
    }
  }

  // Whether the box may not have decided: no answer came, or a 5xx; a 4xx and status 0 were decided.
  export const unanswered = (e: unknown): boolean => !(e instanceof ApiError) || e.status >= 500

  // A custom header forces a CORS preflight the box never answers, so only a page the box served can act (`gate::check_csrf`).
  export const ACT_HEADER = "X-Suzatary-Act"

  // The write contract this build keeps: a page reloads when the box's `gate::CLIENT_MIN` passes this number.
  export const CLIENT_HEADER = "X-Suzatary-Client"
  export const CLIENT = 15
  // A box that has moved past this build's contract: it performed nothing, and the page offers the new build (`newbuild`).
  export const OUTDATED = 426
  export const NOT_SENT = "Not sent: this version of the app is older than the box's. Reload to the new version, then send it again."

  // A box this build may no longer read; one heard before anyone listens is kept.
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


  // Each read's last answer by path, with its tag (`serve::validated`): a 304 is that answer again, kept as text.
  export const KEPT = 24
  type Kept = { readonly tag: string; readonly text: string }
  const kept = new Map<string, Kept>()
  const keep = (path: string, k: Kept): void => {
    kept.delete(path)
    kept.set(path, k)
    for (const old of kept.keys()) if (kept.size > KEPT) kept.delete(old)
  }
  export const held = (path: string): string | null => kept.get(path)?.tag ?? null
  // Every answer kept, as text: what a read of this module's memory finds.
  export const keptTexts = (): readonly string[] => [...kept.values()].map(k => k.text)

  // A read's answer that carries the keys of locked words is never kept: they would outlive their erasure.
  export type Keeping = "Kept" | "Never"

  // "h2", "http/1.1", or "".
  export const protocol = (): string => {
    const entries = typeof performance === "undefined" ? [] : performance.getEntriesByType("resource")
    for (let i = entries.length - 1; i >= 0; i -= 1) {
      const e = entries[i]
      if (e !== undefined && e.name.includes("/api/") && "nextHopProtocol" in e && typeof e.nextHopProtocol === "string" && e.nextHopProtocol !== "") return e.nextHopProtocol
    }
    return ""
  }
  export const NOT_MODIFIED = 304

  // How a read is bounded: a whole-answer timeout (`Wall`), or a longest silence between pieces (`Silence`).
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

  // The body as text, each piece counting as heard.
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

  // The loader's read of this path, else this read's own.
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
    const sent = Date.now()
    const response = await fetch(path, init).catch(wait.waited(path))
    timed(response, sent)
    return [response, had]
  }

  // How many seconds this device's clock reads ahead of the box's (behind where negative), from the `Date` of the last
  // answer no cache can have served (`no-store`, the API's default) against the middle of its round trip; null before any.
  // An `immutable` answer may come from the browser's cache with the `Date` of its first fetch, so it tells no clock.
  let skew: number | null = null
  // Told when the reading moves by a minute or more: the bounds it is read against are minutes apart, and a corrected
  // clock moves it by far more, so a listener is not told of every answer.
  const SKEW_STEP_S = 60
  let told: number | null = null
  const hearing = new Set<() => void>()
  export const heardClock = (k: () => void): (() => void) => (hearing.add(k), () => void hearing.delete(k))
  const timed = (response: Response, sent: number): void => {
    const date = response.headers.get("date")
    const box = date === null ? NaN : Date.parse(date)
    if (Number.isNaN(box) || !(response.headers.get("cache-control") ?? "").includes("no-store")) return
    skew = Math.round(((sent + Date.now()) / 2 - box) / 1000)
    if (told !== null && Math.abs(skew - told) < SKEW_STEP_S) return
    told = skew
    hearing.forEach(k => k())
  }
  export const ahead = (): number | null => skew

  // A read is signed too, so a device whose key was revoked is told to wipe itself however it next asks.
  export const get = <A>(path: string, decoder: d.Decoder<A>, bound: Bound = WALL, keeping: Keeping = "Kept"): Promise<A> =>
    body(path, bound, keeping).then(t => decoded(path, decoder, JSON.parse(t)))

  // The box's body as it came, for a reader that parses it itself.
  export const read = (path: string, bound: Bound = WALL, keeping: Keeping = "Kept"): Promise<text.Text<unknown>> => body(path, bound, keeping).then(text.said)

  const body = async (path: string, bound: Bound, keeping: Keeping): Promise<string> => {
    const timer = bound.type === "Wall" ? { signal: AbortSignal.timeout(wait.MS), heard: () => undefined, done: () => undefined } : silence(path, bound.ms)
    try {
      const [response, had] = await answer(path, timer.signal)
      if (response.status === GONE) gone()
      if (response.status === OUTDATED) stale()
      if (response.status === NOT_MODIFIED && had !== undefined) {
        keep(path, had)
        return had.text
      }
      if (!response.ok) {
        const answer: unknown = await response.json().catch(() => null)
        throw new ApiError(errorOf(answer) ?? `${path} → HTTP ${response.status}`, response.status)
      }
      const got = await textOf(response, timer.signal, timer.heard).catch(wait.waited(path))
      const tag = response.headers.get("etag")
      if (tag === null || keeping === "Never") kept.delete(path)
      else keep(path, { tag, text: got })
      return got
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

  // The box's answer to a request a revoked key signed (`refusal::Status::Wipe`); one heard before anyone listens is kept.
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

  // A request's name, `suzatary_signatures::signatures::request_name`: what it asks, when, a nonce, with which body, and who asks.
  // The box never records the nonce, so a body cannot be guessed back from a record.
  export const requestName = async (method: string, target: string, at: string, nonce: string, body: string, key: string): Promise<string> =>
    sha256(`suzatary request\n${method} ${target}\n${at}\n${nonce}\n${await sha256(body)}\n${key}`)

  const nonce = (): string => hex(crypto.getRandomValues(new Uint8Array(16)).buffer)

  // Timed by stage, logged where `suzatary.debug.requests` = "1".
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
    inflight.held(acting(path, body, bound, "Page")).then(t => decoded(path, decoder, JSON.parse(t)))

  // A post the browser still delivers once the page is gone (`keepalive`), within its 64 KiB for such requests: a larger
  // one goes as `act` does, and what it carried is posted again at the next load.
  export const handOver = <A>(path: string, body: unknown, decoder: d.Decoder<A>): Promise<A> =>
    inflight.held(acting(path, body, undefined, "Outlives")).then(t => decoded(path, decoder, JSON.parse(t)))

  // The most bytes a post that outlives its page carries, under the browser's 64 KiB for every such post in flight.
  const OUTLIVES = 60_000

  // The box's answer as it came, for a reader that parses it itself.
  export const acted = (path: string, body: unknown, bound?: number): Promise<text.Text<unknown>> => inflight.held(acting(path, body, bound, "Page")).then(text.said)

  // A refusal's body, whose error the box words, or null where it is not JSON.
  const parsedOrNull = (body: string): unknown => {
    try {
      return JSON.parse(body)
    } catch {
      return null
    }
  }

  const acting = async (path: string, body: unknown, bound: number | undefined, lives: "Page" | "Outlives"): Promise<string> => {
    const request = actRequest(body)
    const keepalive = lives === "Outlives" && typeof request.body === "string" && request.body.length <= OUTLIVES
    const [init, trace] = await signed(path, { ...request, keepalive })
    const [sent, at] = [performance.now(), Date.now()]
    const response = await fetch(path, { ...init, signal: bound === undefined ? null : AbortSignal.timeout(bound) }).catch((e: unknown) => {
      said(path, trace, 0, performance.now() - sent)
      throw e
    })
    timed(response, at)
    said(path, trace, response.status, performance.now() - sent)
    const answer = await response.text().catch(() => "null")
    if (response.status === GONE) gone()
    // In the page's words, not the box's: the act can be sent again from the build the Reload loads.
    if (response.status === OUTDATED) {
      stale()
      throw new ApiError(NOT_SENT, OUTDATED)
    }
    if (!response.ok) throw new ApiError(errorOf(parsedOrNull(answer)) ?? `${path} → HTTP ${response.status}`, response.status)
    return answer
  }
}
export default http
