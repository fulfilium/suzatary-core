// A store's objects are read bounded by silence, not by length: a slow answer that keeps arriving finishes, and one that
// stops is let go after the bound of nothing heard.

import { test } from "node:test"
import assert from "node:assert/strict"

import http from "../src/wire/http"
import wait from "../src/wire/wait"
import d from "../src/wire/decoder"
import storereads from "../src/wire/storereads"

const MS = 200

// A box answering `pieces` of a body, one every `every` ms, then holding the connection open with nothing more when `stall`.
const box = (pieces: readonly string[], every: number, stall: boolean): void => {
  const fetch = (): Promise<Response> => {
    let sent = 0
    let t: ReturnType<typeof setTimeout> | undefined
    const body = new ReadableStream<Uint8Array>({
      start: c => {
        const next = (): void => {
          if (sent === pieces.length) {
            if (!stall) c.close()
            return
          }
          c.enqueue(new TextEncoder().encode(pieces[sent++]))
          t = setTimeout(next, every)
        }
        t = setTimeout(next, every)
      },
      cancel: () => clearTimeout(t),
    })
    return Promise.resolve(new Response(body, { headers: { "content-type": "application/json" } }))
  }
  Object.defineProperty(globalThis, "fetch", { value: fetch, configurable: true })
}

const silent = { type: "Silence", ms: MS } as const

test("a slow answer that keeps arriving finishes, though it takes longer than the bound", async () => {
  const pieces = ['{"n":[', ...Array.from({ length: 12 }, (_, i) => `${i},`), '12]}']
  box(pieces, MS / 4, false)
  const started = performance.now()
  assert.deepEqual(await http.get("/api/stores/lines/objects", d.unknown, silent), { n: Array.from({ length: 13 }, (_, i) => i) })
  assert.ok(performance.now() - started > 2 * MS, "the whole answer took longer than one bound")
})

test("an answer that stops is let go once nothing arrived for the bound", async () => {
  box(['{"n":', "[1,"], MS / 4, true)
  const started = performance.now()
  const failed = await http.get("/api/stores/lines/objects", d.unknown, silent).then(
    () => "",
    (e: unknown) => String(e),
  )
  const took = performance.now() - started
  assert.match(failed, /nothing arrived from the box for 0\.2 s/)
  assert.ok(took >= MS + MS / 2 && took < 3 * MS, `let go after the last piece and ${MS} ms of silence, not ${took}`)
})

test("the lines are pulled bounded by 30 s of silence", async t => {
  const body = new ReadableStream<Uint8Array>({ start: c => c.enqueue(new TextEncoder().encode('{"objects":[')) })
  Object.defineProperty(globalThis, "fetch", { value: () => Promise.resolve(new Response(body)), configurable: true })
  t.mock.timers.enable({ apis: ["setTimeout"] })
  let settled = false
  const failed = storereads.objects("lines", "").then(
    () => "",
    (e: unknown) => String(e),
  )
  void failed.then(() => (settled = true))
  // Timers are mocked, so the read is let run a few hundred turns of the microtask queue instead.
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 200; i++) await Promise.resolve()
  }
  await settle()
  t.mock.timers.tick(wait.MS - 1)
  await settle()
  assert.equal(settled, false, "still waiting a moment before the bound")
  t.mock.timers.tick(1)
  assert.match(await failed, /nothing arrived from the box for 30 s/)
})
