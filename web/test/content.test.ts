// The worker keeps a file named by its bytes once, and only as the bytes its name promises.

import { test } from "node:test"
import assert from "node:assert/strict"

import shell from "../src/app/shell"
import worker from "../src/app/worker"
import { forAllAsync } from "./gen"

const WASM = "application/wasm"
const KEY = "https://box.example.invalid/c/typst"

const store = (): worker.Store & { held: Map<string, Response> } => {
  const held = new Map<string, Response>()
  return { held, match: k => Promise.resolve(held.get(k)?.clone()), put: (k, r) => Promise.resolve(void held.set(k, r)) }
}
const named = async (bytes: Uint8Array<ArrayBuffer>): Promise<shell.ContentNamed> => {
  const hex = [...new Uint8Array(await crypto.subtle.digest("SHA-256", bytes))].map(b => b.toString(16).padStart(2, "0")).join("")
  const n = shell.byContent(`typst_bg-${hex}.wasm`)
  if (n === null) throw new Error("a SHA-256 in hex is a content name")
  return n
}
type Sent = { bytes: Uint8Array<ArrayBuffer>; status: number; type: string; honest: boolean }

test("law: a file named by its bytes is kept only as a 200 of its type whose bytes hash to its name, and every other answer is the page's and kept nowhere", () =>
  forAllAsync(200, async r => {
    const bytes = new Uint8Array(r.array(1 + r.int(64), () => r.int(256)))
    const n = await named(bytes)
    // What the box may answer: the bytes, a part of them, other bytes, a partial answer, a refusal, another type, no body.
    const { bytes: sent, status, type, honest } = r.pick<() => Sent>([
      () => ({ bytes, status: 200, type: WASM, honest: true }),
      () => ({ bytes, status: 200, type: `${WASM}; charset=binary`, honest: true }),
      () => ({ bytes: bytes.slice(0, r.int(bytes.length)), status: 200, type: WASM, honest: false }),
      () => ({ bytes: bytes.map(b => b ^ 1), status: 200, type: WASM, honest: false }),
      () => ({ bytes: bytes.slice(0, r.int(bytes.length)), status: 206, type: WASM, honest: false }),
      () => ({ bytes: new TextEncoder().encode("Not Found"), status: 404, type: "text/plain", honest: false }),
      () => ({ bytes, status: 200, type: "text/html", honest: false }),
      () => ({ bytes: new Uint8Array(0), status: 304, type: WASM, honest: false }),
      () => ({ bytes: new Uint8Array(0), status: 204, type: WASM, honest: false }),
    ])()
    const s = store()
    let fetched = 0
    const answer = await worker.content(n, KEY, s, () => ((fetched += 1), Promise.resolve(new Response(status === 304 || status === 204 ? null : sent, { status, headers: { "content-type": type } }))), new Map())
    assert.equal(s.held.has(KEY), honest, "kept exactly when it is the promised bytes")
    const promised = status === 200 && type.startsWith(WASM)
    assert.equal(answer.status, honest ? 200 : promised ? 502 : status, "bytes that break their name's promise are a 502, any other refusal as it came")
    if (!honest && !promised) assert.deepEqual([answer.headers.get("content-type"), new Uint8Array(await answer.arrayBuffer())], [type, sent], "passed on with its type and bytes")
    if (honest) {
      assert.deepEqual(new Uint8Array(await answer.arrayBuffer()), bytes)
      const again = await worker.content(n, KEY, s, () => Promise.reject(new Error("asked the network again")), new Map())
      assert.deepEqual([fetched, again.headers.get("content-type"), new Uint8Array(await again.arrayBuffer())], [1, WASM, bytes], "the second ask is the kept file's")
    }
  }))

test("a store with no room left still answers the file", async () => {
  const bytes = new Uint8Array([0, 97, 115, 109])
  const full: worker.Store = { match: () => Promise.resolve(undefined), put: () => Promise.reject(new DOMException("full", "QuotaExceededError")) }
  const answer = await worker.content(await named(bytes), KEY, full, () => Promise.resolve(new Response(bytes, { headers: { "content-type": WASM } })), new Map())
  assert.deepEqual([answer.status, new Uint8Array(await answer.arrayBuffer())], [200, bytes])
})

test("a body that fails part way is kept nowhere, and the page's request fails", async () => {
  const bytes = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0])
  const s = store()
  const cut = new ReadableStream<Uint8Array>({
    start: c => (c.enqueue(bytes.slice(0, 4)), c.error(new TypeError("the connection was reset"))),
  })
  await assert.rejects(worker.content(await named(bytes), KEY, s, () => Promise.resolve(new Response(cut, { headers: { "content-type": WASM } })), new Map()), /reset/)
  assert.equal(s.held.size, 0)
})

test("two cold requests for one file share one download, and each reads its own copy", async () => {
  const bytes = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0])
  const n = await named(bytes)
  const s = store()
  const inflight: worker.Inflight = new Map()
  let fetched = 0
  const ask = () => worker.content(n, KEY, s, () => ((fetched += 1), Promise.resolve(new Response(bytes, { headers: { "content-type": WASM } }))), inflight)
  const [a, b] = await Promise.all([ask(), ask()])
  assert.deepEqual([fetched, inflight.size], [1, 0], "one download, and nothing left in flight once it settled")
  for (const r of [a, b]) assert.deepEqual(new Uint8Array(await r.arrayBuffer()), bytes)
})
