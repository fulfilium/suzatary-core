// Erased words have one place to be forgotten from: the device, whose lines' row the core's worker writes and whose
// rendered blocks each tab's worker reads and writes under the marks an erasure leaves there, while the page holds the
// blocks in view and asks the worker for the rest. So once the box erases words, run through the worker itself
// (`questions.serve` over the real core, a fake box and the fake IndexedDB): no row of the device, no message the worker
// sends, no answer its reads keep and nothing the page holds carries the erased words or the bytes of their key, a join
// the page or another tab sends after the erase writes nothing back, and a reload brings none back.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import { fakeIndexedDB } from "./fakeidb"
import type { Core, Named } from "../src/prodrome/core"
import questions from "../src/prodrome/questions"
import rendered from "../src/app/rendered"
import time from "../src/app/time"
import http from "../src/wire/http"
import { headsOf as objectHeads } from "../src/prodrome/replica"
import type { ContentPayload, SkeletonNode, SkeletonPayload, StorePayload, StoredObject } from "../src/wire/generated/wire"
import { held as prodromes } from "./held"

const built = process.env["SUZATARY_PRODROME_WASM"]
if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
const core = createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core

type Shipped = { hash: string; text: string }
const box = JSON.parse(readFileSync("app/core/tests/web/replicas.json", "utf8")) as {
  inbox: Shipped[]
  conversations: Shipped[]
  skeleton: SkeletonPayload
  stale: Record<string, string>
  erased: Record<string, string[]>
}
const inbox: Named[] = box.inbox.map(o => ({ hash: o.hash, literal: o.text }))
const literals = new Map(box.conversations.map(o => [o.hash, o.text]))
const erasure = box.conversations.find(o => o.text.includes("event=Erased("))!
const conversation = /conversation='([^']+)'/.exec(erasure.text)![1]!
// The words the vector erased, the key that opened them, and the blocks of them a page had rendered.
const WORDS = "I will draft that now."
const KEY = box.skeleton.nodes.flatMap(n => (n.header.node === "line" && n.header.step.step === "erased" ? n.header.step.keys : []))[0]!
const BYTES = box.stale[KEY]!
const FED = box.erased[KEY]!

const headsOf = (nodes: readonly SkeletonNode[]): string[] => {
  const under = new Set(nodes.flatMap(n => n.parents))
  return nodes.map(n => n.hash).filter(h => !under.has(h)).sort()
}
// The box before the erasure: its lines without the erasure, the key that opens the words among their keys.
const before: SkeletonPayload = (() => {
  const nodes = box.skeleton.nodes.filter(n => n.hash !== erasure.hash)
  return { ...box.skeleton, nodes, heads: { are: "these", heads: headsOf(nodes) }, keys: { ...box.skeleton.keys, [KEY]: BYTES }, frontier: { digest: "1".repeat(64), count: nodes.length } }
})()

// A first piece of `p` with the rest to come: the lines naming `key` and all they rest on, else the first half, with
// the keys its lines name, as the box sends them.
const firstPiece = (p: SkeletonPayload, key?: string): SkeletonPayload => {
  const under = new Set<string>()
  const by = new Map(p.nodes.map(n => [n.hash, n]))
  const left = p.nodes.filter(n => n.header.node === "line" && key !== undefined && n.header.keys.includes(key)).map(n => n.hash)
  for (let h = left.pop(); h !== undefined; h = left.pop()) if (!under.has(h)) (under.add(h), left.push(...(by.get(h)?.parents ?? [])))
  const nodes = key === undefined ? p.nodes.slice(0, Math.ceil(p.nodes.length / 2)) : p.nodes.filter(n => under.has(n.hash))
  const named = new Set(nodes.flatMap(n => (n.header.node === "line" ? n.header.keys : [])))
  const keys = Object.fromEntries(Object.entries(p.keys).filter(([k]) => named.has(k)))
  return { ...p, nodes, keys, heads: { are: "these", heads: headsOf(nodes) }, rest: p.nodes.length - nodes.length, frontier: { digest: "2".repeat(64), count: nodes.length } }
}

// A store of objects as the box serves it to a replica holding `since`: all of it to one holding nothing, nothing more
// to one at its frontier.
const objectsOf = (objects: readonly StoredObject[], since: string): StorePayload => {
  const frontier = { digest: "9".repeat(64), count: objects.length }
  const sent = since === frontier.digest ? [] : [...objects]
  const size = sent.reduce((n, o) => n + o.literal.length, 0)
  return { objects: sent, heads: objectHeads(objects), untrusted: [], root: null, geneses: prodromes(objects), epoch: 1, since: "Known", frontier, keys: {}, rest: 0, size, weight: size, erased: [] }
}

// The box as `fetch` reaches it, serving its lines before the erasure or after it (the answers queued first, then the
// whole store), its inbox, and two topic stores holding nothing.
let erased = false
const queued: (SkeletonPayload | "Down")[] = []
// Each answer tagged, as the box tags its reads, so a read this module may keep is kept.
let tags = 0
const json = (v: unknown): Response =>
  new Response(JSON.stringify(v), { status: 200, headers: { "content-type": "application/json", etag: `W/"${(tags += 1)}"` } })
Object.assign(globalThis, {
  fetch: async (input: string | URL): Promise<Response> => {
    const url = new URL(String(input), "http://box")
    if (url.pathname === "/api/stores/lines/skeleton") {
      const next = queued.shift() ?? (erased ? box.skeleton : before)
      return next === "Down" ? new Response("", { status: 503 }) : json(next)
    }
    const store = /^\/api\/stores\/(inbox|topics|topic-aliases)\/objects$/.exec(url.pathname)?.[1]
    if (store !== undefined) return json(objectsOf(store === "inbox" ? inbox : [], url.searchParams.get("since") ?? ""))
    if (url.pathname === "/api/stores/lines/content") {
      const hashes = (url.searchParams.get("hashes") ?? "").split(",")
      return json({ objects: hashes.flatMap(hash => (literals.has(hash) ? [{ hash, literal: literals.get(hash)! }] : [])) } satisfies ContentPayload)
    }
    return new Response("", { status: 404 })
  },
})

// A worker over the device, every message it sends kept printed, and the page's blocks over it, applying what it says.
const worker = () => {
  const posted: string[] = []
  let page: rendered.Store | null = null
  const serve = questions.serve(core, said => {
    posted.push(JSON.stringify(said))
    if (said.type === "Forgot") void page?.forget(said.blocks)
    if (said.type === "Cleared") void page?.clear()
  })
  const ask = async <K extends questions.Kind>(q: questions.Of<K>): Promise<questions.Answer<K>> => {
    const a = (await serve(q)) as questions.Answer<K>
    posted.push(JSON.stringify(a, (_, v: unknown) => (v instanceof Set ? [...v] : v)))
    return a
  }
  page = rendered.viaWorker({
    get: keys => ask({ type: "Blocks", keys }),
    join: pairs => ask({ type: "KeepBlocks", pairs }),
    usage: () => ask({ type: "BlocksUsage" }),
    own: () => ask({ type: "BlocksOwn" }),
  })
  return { ask, posted, page }
}

// The conversation read through the worker, as the page reads it.
const read = async (w: ReturnType<typeof worker>): Promise<void> => {
  assert.equal(await w.ask({ type: "Pull" }), true, "the pull ends whole")
  const window = await w.ask({ type: "Window", id: conversation, window: {}, memory: null, zone: time.UTC })
  assert.equal(window.type, "Held")
}

test("law: once the box erases words, no row, message or page holds them or their key, nothing writes them back, and a reload brings none", async () => {
  const device = fakeIndexedDB()
  erased = false
  const first = worker()
  // A pull cut after its first piece keeps that piece, the key among its keys, on the device.
  queued.push(firstPiece(before, KEY), "Down")
  assert.equal(await first.ask({ type: "Pull" }), false, "the pull was cut")
  assert.ok(device.printed().some(r => r.includes(BYTES)), "the device holds the key")
  // The page had rendered the words, and its blocks are kept on the device by the worker.
  await first.page.join(FED.map((k): rendered.Pair => [k, `<p>${WORDS}</p>`]))
  assert.ok(device.printed().some(r => r.includes(WORDS)), "the device holds the rendered words")
  assert.deepEqual(await first.page.get(FED), FED.map(() => `<p>${WORDS}</p>`))

  const holders = (posted: readonly string[]): string[] => [
    ...device.printed().filter(r => r.includes(WORDS) || r.includes(BYTES)).map(r => r.slice(0, r.indexOf(" "))),
    ...posted.filter(m => m.includes(WORDS) || m.includes(BYTES)).map(m => `message ${m.slice(0, 60)}`),
    ...http.keptTexts().filter(t => t.includes(WORDS) || t.includes(BYTES)).map(t => `kept answer ${t.slice(0, 60)}`),
  ]

  // The erasure comes in the first piece of the next pull, and the rest after it; the landing's write fails, so its
  // pieces stay on the device.
  erased = true
  queued.push(firstPiece(box.skeleton))
  const from = first.posted.length
  device.failing = k => k === "head"
  await first.ask({ type: "Pull" })
  device.failing = () => false
  assert.deepEqual(holders(first.posted.slice(from)), [], "a landing that failed leaves nothing holding them")
  await read(first)
  // A join the page sent before it heard of the erasure reaches the worker after it, and one the page asks after.
  await first.ask({ type: "KeepBlocks", pairs: FED.map((k): rendered.Pair => [k, `<p>${WORDS}</p>`]) })
  await first.page.join(FED.map((k): rendered.Pair => [k, `<p>${WORDS}</p>`]))
  // Another tab's worker, which heard of no erasure, joins its page's copy of the blocks.
  const other = worker()
  await other.ask({ type: "KeepBlocks", pairs: FED.map((k): rendered.Pair => [k, `<p>${WORDS}</p>`]) })
  assert.deepEqual(holders(first.posted.slice(from)), [], "nothing holds the erased words or their key")
  assert.deepEqual(await first.page.get(FED), FED.map(() => null), "the page holds none of the blocks")

  // A worker made again over the same device restores the lines' row and the blocks from its rows, and pulls. (The
  // content it holds in memory is this process's, so its readings are the first worker's; the rows are what a reload
  // reads.)
  const reloaded = worker()
  assert.equal(await reloaded.ask({ type: "Pull" }), true)
  assert.deepEqual(holders(reloaded.posted), [], "a reload brings none back")
  assert.deepEqual(await reloaded.page.get(FED), FED.map(() => null))
})
