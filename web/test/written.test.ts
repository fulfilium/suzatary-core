// The hand-over of what this tab wrote (`written`), against a fake store and a fake box: only the rule's own refusal,
// in a receipt, is final; and a refusal is said with the person's words, once, before its record may go.

import { test } from "node:test"
import assert from "node:assert/strict"

import type { Core, Written } from "../src/prodrome/core"
import written from "../src/prodrome/written"
import http from "../src/wire/http"
import storereads from "../src/wire/storereads"
import ownlines from "../src/prodrome/ownlines"
import signer from "../src/prodrome/signer"
import { fakeIndexedDB } from "./fakeidb"

type Fake = { readonly store: Written; readonly calls: string[] }

// The lines' entry, posted to the lines' route and signed.
const lines = (e: Pick<written.Entry, "noun" | "store" | "stepped" | "told">): ((name: "lines") => written.Entry) => () => ({
  ...e,
  exclusive: f => f(),
  post: (literals, since, keys) => storereads.post("lines", literals, since, keys),
  signs: () => "Signs",
})

// A store holding `pieces` posts of one unsent line each, its refusals `refusals` (the core's JSON), each move it is asked for recorded.
const fake = (refusals: unknown[] = [], pieces = 1, waiting: unknown[] = []): Fake => {
  const calls: string[] = []
  const store = {
    unsigned: () => "[]",
    unsent: () => JSON.stringify(Array.from({ length: pieces }, (_, k) => [{ hash: String(k).repeat(64), literal: "Change(…)", keys: [] }])),
    since: () => "",
    answered: () => (calls.push("answered"), undefined),
    refuse: (_: string, why: string) => (calls.push(`refuse:${why}`), undefined),
    refusals: () => JSON.stringify(refusals),
    waiting: () => JSON.stringify(waiting),
  } as unknown as Written
  return { store, calls }
}

test("an answer for later leaves every line unsent, refuses nothing, stops the hand-over, and is said once", async () => {
  for (const status of [400, 401, 403, 404, 405, 408, 409, 410, 415, 429]) {
    let posts = 0
    Object.assign(globalThis, { fetch: async () => (posts++, new Response(JSON.stringify({ error: `not yet (${status})` }), { status })) })
    const { store, calls } = fake([], 2)
    const said: string[] = []
    const writes = written.of({} as Core, lines({ noun: "line", store, stepped: async () => {} }), why => void said.push(why))
    await writes.handOver("lines")
    await writes.handOver("lines")
    assert.deepEqual(calls, [], `${status}: nothing answered, nothing refused`)
    assert.equal(posts, 2, `${status}: the first piece alone, at each hand-over`)
    assert.equal(said.length, 1, `${status}: said once`)
  }
})

test("a refusal of the post itself refuses its lines for good, as the rule's does, and the next piece goes", async () => {
  for (const status of [413, 422]) {
    let posts = 0
    Object.assign(globalThis, { fetch: async () => (posts++, new Response(JSON.stringify({ error: `never (${status})` }), { status })) })
    const { store, calls } = fake([], 2)
    const writes = written.of({} as Core, lines({ noun: "line", store, stepped: async () => {} }), () => {})
    await writes.handOver("lines")
    assert.deepEqual(calls, [`refuse:never (${status})`, `refuse:never (${status})`], `${status}: each piece refused`)
    assert.equal(posts, 2, `${status}: no piece holds back the next`)
  }
})

test("every status is read as exactly one answer, and only 413 and 422 refuse a post's lines for good", () => {
  for (let status = 0; status < 600; status++) {
    const read = written.failed(new http.ApiError(`status ${status}`, status)).type
    const expected = status >= 500 ? "Unanswered" : status === http.OUTDATED ? "Outdated" : status === 413 || status === 422 ? "Refused" : "Later"
    assert.equal(read, expected, `${status}`)
  }
})

test("an answer that is no HTTP answer at all is no answer", () => {
  assert.equal(written.failed(new TypeError("offline")).type, "Unanswered")
})

test("a refusal a load finds is said with the person's words, once, and marked seen", async () => {
  const refused = { hash: "b".repeat(64), kind: "Final", reason: "say something", literal: "Change(…said…)", words: "Is the permit due?", signature: false }
  const { store } = fake([refused])
  const seen: string[][] = []
  let told = new Set<string>()
  const said: string[] = []
  const writes = written.of(
    {} as Core,
    lines({
      noun: "line",
      store,
      stepped: async () => {},
      told: { seen: async literals => void seen.push([...literals]), told: async () => told },
    }),
    why => void said.push(why),
  )
  await writes.said("lines")
  assert.equal(said.length, 1)
  assert.match(said[0] ?? "", /Is the permit due\?/, "the words, to copy")
  assert.deepEqual(seen, [[refused.literal]])
  told = new Set([refused.literal])
  await writes.said("lines")
  assert.equal(said.length, 1, "a refusal already said is not said again")
})

test("a post the box did not answer leaves every line unsent and says nothing: the next sync posts it", async () => {
  for (const status of [500, 502, 503]) {
    Object.assign(globalThis, { fetch: async () => new Response("", { status }) })
    const { store, calls } = fake()
    const said: string[] = []
    const writes = written.of({} as Core, lines({ noun: "line", store, stepped: async () => {} }), why => void said.push(why))
    await writes.handOver("lines")
    assert.deepEqual(calls, [], `${status}: nothing answered, nothing refused`)
    assert.deepEqual(said, [], `${status}: nothing said`)
  }
})

test("a line the box has not taken yet is said once in its rule's words, and told apart from a refusal", async () => {
  const waits = { hash: "c".repeat(64), reason: "it is dated 600 s ahead of the box's clock", literal: "Change(…ahead…)", words: "Is it due?", at: null, signature: false }
  const { store } = fake([], 1, [waits])
  const seen: string[] = []
  let told = new Set<string>()
  const said: string[] = []
  const writes = written.of(
    {} as Core,
    lines({
      noun: "line",
      store,
      stepped: async () => {},
      told: { seen: async literals => void seen.push(...literals), told: async () => told },
    }),
    why => void said.push(why),
  )
  await writes.said("lines")
  assert.equal(said.length, 1)
  assert.match(said[0] ?? "", /has not taken a line yet: it is dated 600 s ahead/)
  assert.match(said[0] ?? "", /Is it due\?/, "the words, to copy")
  assert.deepEqual(seen, [waits.literal + ownlines.WAITS], "marked as told it waits, not as told it was refused")
  told = new Set(seen)
  await writes.said("lines")
  assert.equal(said.length, 1, "said once")
})

test("a signature refused or waiting with what it signs says nothing of its own: one sentence for one line", async () => {
  const line = { hash: "d".repeat(64), kind: "Final", reason: "say something", literal: "Change(…said…)", words: "Is it due?", signature: false }
  const signed = { hash: "e".repeat(64), kind: "Final", reason: "rests on dddddddddddd…, which this box refused", literal: "Signed(…)", words: null, signature: true }
  const waits = { ...line, hash: "f".repeat(64), literal: "Change(…ahead…)", reason: "it is dated 600 s ahead of the box's clock", at: null }
  const waitsSigned = { ...signed, hash: "0".repeat(64), literal: "Signed(…ahead…)", reason: "rests on ffffffffffff…, which this box has not taken yet, and waits with it", at: null }
  const { store } = fake([line, signed], 1, [waits, waitsSigned])
  const seen: string[] = []
  const said: string[] = []
  const writes = written.of(
    {} as Core,
    lines({
      noun: "line",
      store,
      stepped: async () => {},
      told: { seen: async literals => void seen.push(...literals), told: async () => new Set(seen) },
    }),
    why => void said.push(why),
  )
  await writes.said("lines")
  assert.equal(said.length, 2, `one refusal and one wait, each said once: ${JSON.stringify(said)}`)
  assert.ok(!said.some(s => s.includes("dddddddddddd") || s.includes("ffffffffffff")), "no sentence for a signature")
  assert.deepEqual(
    [...seen].sort(),
    [line.literal, signed.literal, waits.literal + ownlines.WAITS, waitsSigned.literal + ownlines.WAITS].sort(),
    "every record marked told, the signatures' too, so each may go",
  )
})

test("a line dated ahead is due again when the box's clock reaches its instant, and a line of the box's past never", () => {
  const line = (at: string | null) => ({ hash: "d".repeat(64), reason: "ahead", literal: null, words: null, at, signature: false })
  const now = Date.parse("2026-11-20T10:00:00Z")
  for (const ahead of [null, 0, 90, -90]) {
    const due = written.due([line("2026-11-20T10:10:00Z"), line("2026-11-20T10:20:00Z"), line("2026-11-20T09:00:00Z"), line(null)], ahead, now)
    assert.deepEqual(due, { type: "At", ms: Date.parse("2026-11-20T10:10:00Z") + (ahead ?? 0) * 1000, exact: true }, `${ahead}`)
  }
  assert.deepEqual(written.due([line("2026-11-20T09:00:00Z")], 0, now), { type: "Never" })
})

// A store of objects on a device: each event appends a change named by its index, a frame written is kept whole, a change
// is unsigned until a kept `Signed(<name>)` names it, and each signature the hand-over makes is recorded. A core prints a
// signature as `Signed(<name>)`.
const objects = (): { readonly store: Written & written.Appends; readonly kept: string[]; readonly signedLater: string[] } => {
  const kept: string[] = []
  const signedLater: string[] = []
  const names = (literals: readonly string[]): string[] => literals.filter(l => !l.startsWith("Signed(")).map(l => l.slice(0, 64))
  const store = {
    append: (events: string) => JSON.stringify((JSON.parse(events) as string[]).map((e, k) => ({ hash: String(k + 1).repeat(64), literal: String(k + 1).repeat(64) + e }))),
    writing: (literals: string) => new TextEncoder().encode(literals),
    unsigned: () => JSON.stringify(names(kept).filter(n => !kept.includes(`Signed(${n})`) && !signedLater.includes(`Signed(${n})`))),
    signed: (literal: string) => (signedLater.push(literal), { delta: new Uint8Array(), moved: "[]", free: () => {} }),
    unsent: () => "[]",
    since: () => "",
    refusals: () => "[]",
    waiting: () => "[]",
  } as unknown as Written & written.Appends
  return { store, kept, signedLater }
}
const printing = { signed: (object: string) => JSON.stringify({ hash: "9".repeat(64), text: `Signed(${object})` }) } as unknown as Core
const entryOf = (store: Written) => (): written.Entry => ({ noun: "topic", store, exclusive: f => f(), stepped: async () => {}, post: storereads.post.bind(null, "topics"), signs: () => "Signs" })
// The device's key signs its first `ok` signatures and refuses every one after.
const signingOnly = (ok: number): (() => void) => {
  const real = crypto.subtle.sign.bind(crypto.subtle)
  let n = 0
  Object.defineProperty(crypto.subtle, "sign", {
    configurable: true,
    value: (...a: Parameters<SubtleCrypto["sign"]>) => (n++ < ok ? real(...a) : Promise.reject(new Error("the key is gone"))),
  })
  return () => void Reflect.deleteProperty(crypto.subtle, "sign")
}

test("an own write keeps its changes with every signature made before a failing one, in one frame, and the hand-over signs the rest", async () => {
  fakeIndexedDB()
  assert.equal((await signer.renew()).type, "Held")
  for (const ok of [0, 1, 2]) {
    const { store, kept, signedLater } = objects()
    const said: string[] = []
    const writes = written.of(printing, entryOf(store), why => void said.push(why))
    const restore = signingOnly(ok)
    const signed = await writes.own("topics", store, ["Added(a)", "Added(b)"], async frame => void kept.push(...(JSON.parse(new TextDecoder().decode(frame)) as string[])))
    restore()
    const changes = ["1".repeat(64), "2".repeat(64)]
    assert.equal(signed.type, ok < 2 ? "Partly" : "Signed", `${ok} signed`)
    assert.deepEqual(kept, [...changes.map(c => c + (c[0] === "1" ? "Added(a)" : "Added(b)")), ...changes.slice(0, ok).map(c => `Signed(${c})`)], `${ok}: both changes and the signatures made, in one frame`)
    await writes.handOver("topics")
    assert.deepEqual(signedLater, changes.slice(ok).map(c => `Signed(${c})`), `${ok}: the hand-over signs what the write could not`)
  }
})

test("an own write whose frame the device does not keep throws, and holds nothing", async () => {
  fakeIndexedDB()
  await signer.renew()
  const { store } = objects()
  const writes = written.of(printing, entryOf(store), () => {})
  await assert.rejects(writes.own("topics", store, ["Added(a)"], async () => Promise.reject(new Error("the disk is full"))), /the disk is full/)
})

test("signing what the core holds never throws: a store or a key that throws at any step answers what was made before it", async () => {
  fakeIndexedDB()
  await signer.renew()
  for (const at of ["unsigned", "signed", "sign"] as const) {
    let made = 0
    const store = {
      unsigned: () => {
        if (at === "unsigned") throw new Error("unreadable")
        return JSON.stringify(["1".repeat(64), "2".repeat(64)])
      },
      signed: () => {
        if (at === "signed" && made === 1) throw new Error("not a signature")
        made++
        return { delta: new Uint8Array(), moved: "[]", free: () => {} }
      },
    } as unknown as Written
    const writes = written.of(printing, entryOf(store), () => {})
    const restore = signingOnly(at === "sign" ? 1 : 2)
    const signed = await writes.signatures("topics")
    restore()
    assert.equal(signed.type, "Partly", at)
    assert.equal(signed.made.length, at === "unsigned" ? 0 : 1, `${at}: what was made before the throw`)
  }
})
