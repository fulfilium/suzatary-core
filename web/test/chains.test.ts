// A chain as the core's worker holds it (`chainsync`), against a fake box that takes what a tab posts and the fake
// IndexedDB. Every act a tab seals, online or not, reaches the box once and is never sealed twice, through any
// interleaving of two tabs' acts, exchanges, hides and reloads, offline and on; once both have exchanged online, each
// reads what a cold core reads of the box. An act an earlier build kept unsent is posted after the update, once, with
// its signature, and only once the box has named the chain's root. A reset lets go only what the reader was shown, and
// a pull asked before it adds nothing to it. A device whose log was emptied under a running tab reads the box whole
// again.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createHash, generateKeyPairSync, sign } from "node:crypto"
import { createRequire } from "node:module"

import type { Core } from "../src/prodrome/core"
import { entriesOf, objectsOf } from "../src/prodrome/core"
import questions from "../src/prodrome/questions"
import type { PostedObjects, ReceiptPayload, StoredObject } from "../src/wire/generated/wire"
import { fakeIndexedDB } from "./fakeidb"
import { forAllAsync } from "./gen"
import { answerOf, headsOf, heldChain, holdAll } from "./held"

const built = process.env["SUZATARY_PRODROME_WASM"]
if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
const core = createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
const chain = { untrusted: ["triage"] }
// The box's chain begins as a store does: its genesis.
const genesis = `Genesis(label='life', nonce='${"0".repeat(32)}')`
const start: StoredObject[] = [{ hash: createHash("sha256").update(genesis, "utf8").digest("hex"), literal: genesis }]
const TODOS = ["book-flight", "email-harrow", "file-permit", "pack-bags", "renew-card", "water-plants"]
const AT = "2026-11-20T09:00:00"

const hashOf = (literal: string): string => createHash("sha256").update(literal, "utf8").digest("hex")
const json = (v: unknown): Response => new Response(JSON.stringify(v), { status: 200, headers: { "content-type": "application/json" } })

// The box's chain `life`, which takes what a tab posts and refuses an act whose note says so, under `root`; or no box at
// all. Each request it answered is in `asked`. A read answers what lies after the frontier it was asked from, as the
// box does, and waits on `gate`, calling `atGate` once it does; a post waits on `postGate`.
let held: StoredObject[] = [...start]
let down = false
let root: string | null = null
let asked: string[] = []
let gate: Promise<void> = Promise.resolve()
let atGate = (): void => {}
let postGate: Promise<void> = Promise.resolve()
let atPost = (): void => {}
// How many objects the box's frontier counts beyond what it serves: a box whose own copy is not closed.
let lacking = 0
// How many reads asked from nothing: a whole pull.
let wholes = 0
// The box's objects are in the order they were taken, so a frontier is how many it held.
const frontier = (): { digest: string; count: number } => ({ digest: String(held.length).padStart(64, "f"), count: held.length + lacking })
const after = (since: string | null): number | null => {
  const n = since === null ? NaN : Number(since.replace(/^f+/, ""))
  return Number.isInteger(n) && n <= held.length ? n : null
}
Object.assign(globalThis, {
  fetch: async (input: string | URL, init?: RequestInit): Promise<Response> => {
    if (down) return new Response("", { status: 503 })
    const url = new URL(String(input), "http://box")
    if (url.pathname !== "/api/life/objects") return new Response("", { status: 404 })
    const method = init?.method ?? "GET"
    asked.push(method)
    if (method === "POST") {
      atPost()
      await postGate
      const posted = JSON.parse(String(init?.body)) as PostedObjects
      const refused = posted.objects.filter(l => l.includes("note='refuse me'"))
      const adopted = posted.objects.filter(l => !refused.includes(l)).map(literal => ({ hash: hashOf(literal), literal })).filter(o => !held.some(h => h.hash === o.hash))
      held = [...held, ...adopted]
      return json({
        heads: headsOf(held),
        adopted: adopted.map(o => o.hash),
        already: posted.objects.filter(l => !refused.includes(l)).map(hashOf).filter(h => !adopted.some(o => o.hash === h)),
        refused: refused.map(l => ({ hash: hashOf(l), reason: "not this one", kind: "final" as const })),
        since: "Known",
        frontier: frontier(),
      } satisfies ReceiptPayload)
    }
    const since = url.searchParams.get("since")
    if (since === null || since === "") wholes += 1
    atGate()
    await gate
    const from = after(since)
    return json({ ...answerOf(held, chain.untrusted, root), objects: from === null ? held : held.slice(from), frontier: frontier() })
  },
})

// A tab's worker over the device. Every tab made is kept until `settled`, since its timers outlive the run that made it.
type Tab = { readonly ask: <K extends questions.Kind>(q: questions.Of<K>) => Promise<questions.Answer<K>> }
let live: Tab[] = []
const tab = async (say: (s: questions.Said) => void = () => {}): Promise<Tab> => {
  const serve = questions.serve(core, say)
  const t: Tab = { ask: async q => (await serve(q, () => Promise.resolve())) as never }
  live.push(t)
  await t.ask({ type: "Served", chains: ["life"] })
  return t
}
const settled = async (): Promise<void> => {
  down = false
  for (const t of live) await t.ask({ type: "Exchange", chain: "life" })
  for (const t of live) await t.ask({ type: "Shown", on: false })
  live = []
}

// A completion whose note names it, so the box's objects say how many times each act was sealed.
const act = (n: number): string => core.lifecycle("Completed", TODOS[n % TODOS.length]!, AT, "owner", `act ${n}`)
const sealings = (n: number): number => held.filter(o => o.literal.includes(`note='act ${n}'`)).length

// The fold a cold core reads of the box.
const cold = (): string => {
  const set = holdAll(new core.Chains(), [heldChain(core, "life", held, chain.untrusted)])
  try {
    return JSON.stringify(entriesOf(core, held, AT, chain.untrusted, "life", set))
  } finally {
    set.free()
  }
}
const read = async (t: Tab): Promise<string> => {
  const listed = await t.ask({ type: "List", chain: "life", at: AT })
  return listed.type === "Read" ? JSON.stringify(listed.fold) : "unheld"
}

test("law: every act two tabs seal reaches the box once, through any acts, exchanges and reloads, online or not", async () => {
  let n = 0
  await forAllAsync(15, async r => {
    fakeIndexedDB()
    held = [...start]
    down = false
    const tabs = [await tab(), await tab()]
    for (const t of tabs) assert.equal(await t.ask({ type: "Exchange", chain: "life" }), true)
    const sealed: number[] = []
    for (let step = 0; step < 10; step += 1) {
      const i = r.int(tabs.length)
      const op = r.pick(["Act", "Act", "Exchange", "Offline", "Online", "Hide", "Reload"] as const)
      switch (op) {
        case "Act":
          n += 1
          await tabs[i]!.ask({ type: "Act", chain: "life", event: act(n), at: AT, actor: "owner" })
          sealed.push(n)
          break
        case "Exchange":
          await tabs[i]!.ask({ type: "Exchange", chain: "life" })
          break
        case "Offline":
          down = true
          break
        case "Online":
          down = false
          break
        case "Hide":
          await tabs[i]!.ask({ type: "Shown", on: false })
          break
        case "Reload":
          tabs[i] = await tab()
          break
      }
    }
    down = false
    for (const t of tabs) await t.ask({ type: "Exchange", chain: "life" })
    for (const t of tabs) await t.ask({ type: "Exchange", chain: "life" })
    for (const k of sealed) assert.equal(sealings(k), 1, `act ${k} sealed once and taken`)
    for (const t of tabs) {
      assert.equal(await read(t), cold(), "a tab reads what a cold core reads of the box")
      const unsent = await t.ask({ type: "Unsent", chain: "life" })
      assert.deepEqual(unsent.losses, [], "nothing left unsent")
    }
    await settled()
  })
})

// An earlier build's device: version 13, its chain's row holding what it pulled and what the box has not taken.
const earlier = async (...unsent: readonly StoredObject[]): Promise<void> => {
  await new Promise<void>((done, failed) => {
    const open = indexedDB.open("suzatary", 13)
    open.onupgradeneeded = () => {
      for (const name of ["chain", "content", "replica"]) open.result.createObjectStore(name)
      open.result.createObjectStore("blocks", { keyPath: "key" }).createIndex("used", "used")
      const row = { generation: 0, objects: [...held, ...unsent], synced: held.map(o => o.hash), refused: [], words: [], frontier: { type: "Nothing" } }
      open.transaction?.objectStore("chain").put(row, "objects:life")
    }
    open.onsuccess = () => done()
    open.onerror = () => failed(new Error("the earlier build's device did not open"))
  })
}

test("an act an earlier build kept unsent is posted after the update once, and a second start carries nothing", async () => {
  const device = fakeIndexedDB()
  held = [...start]
  down = false
  const todos = new core.Todos(objectsOf(held))
  const written = JSON.parse(todos.append(act(9_000), null)) as { objects: { hash: string; text: string }[] }
  todos.free()
  const unsent = written.objects.map(o => ({ hash: o.hash, literal: o.text }))[0]!
  await earlier(unsent)
  // Offline first: the act is the core's at once, and shown.
  down = true
  const first = await tab()
  assert.deepEqual((await first.ask({ type: "Unsent", chain: "life" })).losses, [unsent.hash], "carried as this tab's own")
  assert.equal(device.rows("suzatary", "chain").has("carry:life"), false, "the carried row is gone")
  down = false
  await first.ask({ type: "Exchange", chain: "life" })
  assert.equal(sealings(9_000), 1, "taken once")
  const second = await tab()
  await second.ask({ type: "Exchange", chain: "life" })
  assert.deepEqual((await second.ask({ type: "Unsent", chain: "life" })).losses, [], "the next start carries nothing")
  assert.equal(sealings(9_000), 1, "and posts nothing twice")
  await settled()
})

test("a reset lets go only what the reader was shown: an act sealed since stops it, and once shown both are let go everywhere", async () => {
  fakeIndexedDB()
  held = [...start]
  down = false
  const [one, other] = [await tab(), await tab()]
  await one.ask({ type: "Exchange", chain: "life" })
  down = true
  await one.ask({ type: "Act", chain: "life", event: act(9_001), at: AT, actor: "owner" })
  // Offline, the exchange reads what the other tab kept and posts nothing.
  await other.ask({ type: "Exchange", chain: "life" })
  const shown = (await other.ask({ type: "Unsent", chain: "life" })).losses
  assert.equal(shown.length, 1, "the other tab's act is shown")
  await one.ask({ type: "Act", chain: "life", event: act(9_002), at: AT, actor: "owner" })
  const refused = await other.ask({ type: "ResetChain", chain: "life", shown })
  assert.equal(refused.length, 1, "refused, saying why")
  // Shown both, the other tab lets them go: the tab that sealed them reads that before it posts anything.
  await other.ask({ type: "Exchange", chain: "life" })
  const both = (await other.ask({ type: "Unsent", chain: "life" })).losses
  assert.deepEqual(await other.ask({ type: "ResetChain", chain: "life", shown: both }), [])
  down = false
  await one.ask({ type: "Exchange", chain: "life" })
  await other.ask({ type: "Exchange", chain: "life" })
  assert.deepEqual([sealings(9_001), sealings(9_002)], [0, 0], "an act let go in another tab is never posted")
  await settled()
})

// A device key's signature of `object`, as the core prints it: what an earlier build sealed beside an act under a root.
const signedBy = (object: string): StoredObject => {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519")
  const raw = publicKey.export({ format: "der", type: "spki" }).subarray(-32)
  const signature = sign(null, new TextEncoder().encode(`prodrome object ${object}`), privateKey)
  const hex = (bytes: Uint8Array): string => Array.from(bytes, b => b.toString(16).padStart(2, "0")).join("")
  const s = JSON.parse(core.signed(object, hex(raw), hex(signature))) as { hash: string; text: string }
  return { hash: s.hash, literal: s.text }
}

test("an act an earlier build kept is posted with its signature only once the box names the root, and one the box refuses stays named", async () => {
  fakeIndexedDB()
  held = [...start]
  down = false
  root = "c".repeat(64)
  asked = []
  const sealed = (note: string): StoredObject => {
    const todos = new core.Todos(objectsOf(held))
    const written = JSON.parse(todos.append(core.lifecycle("Completed", "pack-bags", AT, "owner", note), null)) as { objects: { hash: string; text: string }[] }
    todos.free()
    return written.objects.map(o => ({ hash: o.hash, literal: o.text }))[0]!
  }
  const kept = sealed("act 9100")
  const refused = sealed("refuse me")
  await earlier(kept, signedBy(kept.hash), refused)
  const t = await tab()
  await t.ask({ type: "Exchange", chain: "life" })
  assert.ok(asked.indexOf("GET") < asked.indexOf("POST"), `nothing is posted before the box names the root: ${asked.join(" ")}`)
  assert.equal(sealings(9100), 1, "the act is taken once")
  assert.equal(held.filter(o => o.literal.startsWith("Signed(") && o.literal.includes(kept.hash)).length, 1, "with its signature")
  const unsent = await t.ask({ type: "Unsent", chain: "life" })
  assert.deepEqual([unsent.losses, unsent.refused], [[refused.hash], ["not this one"]], "the refused one stays, named")
  root = null
  await settled()
})

test("a pull asked before a reset adds nothing to it: an answer that comes after the reset is passed over, and the chain holds the box whole", async () => {
  fakeIndexedDB()
  held = [...start]
  down = false
  let moved = (): void => {}
  const t = await tab(s => {
    if (s.type === "Chain") moved()
  })
  await t.ask({ type: "Exchange", chain: "life" })
  await t.ask({ type: "Act", chain: "life", event: act(9200), at: AT, actor: "owner" })
  await t.ask({ type: "Exchange", chain: "life" })
  // The pull's read is held at the box until the reset has run; meanwhile another device's act reaches the box, so the
  // answer is a piece resting on what the reset emptied.
  let open = (): void => {}
  gate = new Promise(go => (open = go))
  const reached = new Promise<void>(go => (atGate = go))
  const pulling = t.ask({ type: "Exchange", chain: "life" })
  await reached
  atGate = () => {}
  const other = new core.Todos(objectsOf(held))
  const written = JSON.parse(other.append(act(9201), null)) as { objects: { hash: string; text: string }[] }
  other.free()
  held = [...held, ...written.objects.map(o => ({ hash: o.hash, literal: o.text }))]
  // The reset's own turn runs while the read is held, its start-over the next move of the chain; its pull waits behind
  // the one held.
  const startedOver = new Promise<void>(go => (moved = go))
  const resetting = t.ask({ type: "ResetChain", chain: "life", shown: [] })
  await startedOver
  open()
  gate = Promise.resolve()
  await Promise.all([pulling, resetting])
  await t.ask({ type: "Exchange", chain: "life" })
  assert.equal(sealings(9201), 1)
  assert.equal(await read(t), cold(), "the chain the reset started holds what the box holds, with every object under its heads")
  await settled()
})

test("an act is sealed, kept and drawn while a post is held open at the box", async () => {
  fakeIndexedDB()
  held = [...start]
  down = false
  const t = await tab()
  await t.ask({ type: "Exchange", chain: "life" })
  let open = (): void => {}
  postGate = new Promise(go => (open = go))
  const posting = new Promise<void>(go => (atPost = go))
  await t.ask({ type: "Act", chain: "life", event: act(9600), at: AT, actor: "owner" })
  // The first act's post reaches the box and is held open there; the second act neither waits for it nor is lost.
  await posting
  atPost = () => {}
  const second = await Promise.race([
    t.ask({ type: "Act", chain: "life", event: act(9601), at: AT, actor: "owner" }).then(listed => listed.type),
    new Promise<string>(go => setTimeout(() => go("waited on the post"), 2000)),
  ])
  assert.equal(second, "Read", "the second act is answered with the fold while the first post is open")
  assert.equal((await t.ask({ type: "Unsent", chain: "life" })).losses.length, 2, "both acts are kept, unsent")
  open()
  postGate = Promise.resolve()
  await t.ask({ type: "Exchange", chain: "life" })
  assert.deepEqual([sealings(9600), sealings(9601)], [1, 1])
  assert.equal(await read(t), cold())
  await settled()
})

test("an act sealed after another tab's reset, before this tab read it, is sealed onto the reset chain and posted once", async () => {
  fakeIndexedDB()
  held = [...start]
  down = false
  const [one, other] = [await tab(), await tab()]
  await one.ask({ type: "Exchange", chain: "life" })
  await other.ask({ type: "Exchange", chain: "life" })
  down = true
  await one.ask({ type: "Act", chain: "life", event: act(9500), at: AT, actor: "owner" })
  await other.ask({ type: "Exchange", chain: "life" })
  const shown = (await other.ask({ type: "Unsent", chain: "life" })).losses
  assert.equal(shown.length, 1)
  assert.deepEqual(await other.ask({ type: "ResetChain", chain: "life", shown }), [])
  down = false
  // `one` has not read the log since the reset when it seals.
  await one.ask({ type: "Act", chain: "life", event: act(9501), at: AT, actor: "owner" })
  await one.ask({ type: "Exchange", chain: "life" })
  await other.ask({ type: "Exchange", chain: "life" })
  assert.deepEqual([sealings(9500), sealings(9501)], [0, 1], "the act let go is never posted; the act sealed after it is, once")
  assert.equal(await read(one), cold())
  assert.equal(await read(other), cold())
  await settled()
})

test("a device whose log was emptied under a running tab reads the box whole again, and the act sealed after it is not lost", async () => {
  fakeIndexedDB()
  held = [...start]
  down = false
  const t = await tab()
  await t.ask({ type: "Exchange", chain: "life" })
  await t.ask({ type: "Act", chain: "life", event: act(9300), at: AT, actor: "owner" })
  await t.ask({ type: "Exchange", chain: "life" })
  // The browser clears the device's data while the tab runs.
  await new Promise<void>((done, failed) => {
    const open = indexedDB.open("suzatary")
    open.onsuccess = () => {
      const clear = open.result.transaction("replica", "readwrite").objectStore("replica").clear()
      clear.onsuccess = () => done()
      clear.onerror = () => failed(new Error("not cleared"))
    }
    open.onerror = () => failed(new Error("not opened"))
  })
  down = true
  await t.ask({ type: "Act", chain: "life", event: act(9301), at: AT, actor: "owner" })
  down = false
  const reloaded = await tab()
  await reloaded.ask({ type: "Exchange", chain: "life" })
  await t.ask({ type: "Exchange", chain: "life" })
  assert.deepEqual([sealings(9300), sealings(9301)], [1, 1])
  assert.equal(await read(reloaded), cold(), "the reload reads what a cold core reads of the box")
  await settled()
})

test("an act the device cannot keep is refused in words, and nothing of it is recorded or posted", async () => {
  const device = fakeIndexedDB()
  held = [...start]
  down = false
  const t = await tab()
  await t.ask({ type: "Exchange", chain: "life" })
  device.failing = key => key.startsWith("chain/life/")
  await assert.rejects(t.ask({ type: "Act", chain: "life", event: act(9400), at: AT, actor: "owner" }), /could not keep the act .* nothing was recorded/)
  device.failing = () => false
  await t.ask({ type: "Exchange", chain: "life" })
  const reloaded = await tab()
  await reloaded.ask({ type: "Exchange", chain: "life" })
  assert.equal(sealings(9400), 0, "never posted")
  assert.deepEqual((await reloaded.ask({ type: "Unsent", chain: "life" })).losses, [], "and kept nowhere")
  assert.equal(await read(t), cold(), "the tab reads the box, not the act it could not keep")
  await settled()
})

test("a chain the box has not yet said whom it believes is not held and names no failure; once it has, it is read under that", async () => {
  fakeIndexedDB()
  held = [...start]
  down = false
  const said: { type: string; reader?: string; left?: { chains: unknown[] } }[] = []
  const serve = questions.serve(core, s => said.push(s as never))
  const ask = <K extends questions.Kind>(q: questions.Of<K>): Promise<questions.Answer<K>> => serve(q, () => Promise.resolve()) as never
  await ask({ type: "Served", chains: ["life"] })
  assert.equal((await ask({ type: "List", chain: "life", at: null })).type, "Unheld", "not read before the box speaks")
  await ask({ type: "Exchange", chain: "life" })
  assert.equal((await ask({ type: "List", chain: "life", at: null })).type, "Read")
  assert.deepEqual(said.filter(s => s.type === "Told" && (s.left?.chains.length ?? 0) > 0), [], "no chain is said not to read")
  await ask({ type: "Shown", on: false })
})

test("a box whose own copy is not closed is held as it came, said once, and never started over for that again", async () => {
  fakeIndexedDB()
  held = [...start]
  down = false
  lacking = 3
  asked = []
  wholes = 0
  const told: string[] = []
  const say = (s: questions.Said): void => {
    if (s.type === "Told" && s.reader === "open/store/chain/life" && s.left.rows.length > 0) told.push(s.reader)
  }
  const [one, other] = [await tab(say), await tab(say)]
  for (let round = 0; round < 3; round += 1) {
    await one.ask({ type: "Act", chain: "life", event: act(9700 + round), at: AT, actor: "owner" }).catch(() => undefined)
    await one.ask({ type: "Exchange", chain: "life" })
    await other.ask({ type: "Exchange", chain: "life" })
  }
  // A first pull from nothing in each tab, and at most one start-over each once the other's frames are read: never a
  // whole pull a round.
  assert.ok(wholes <= 4, `the start-over is bounded (${wholes} whole pulls in three rounds of two tabs)`)
  assert.ok(told.length >= 1 && told.length <= 2, `said once a tab (${told.length})`)
  assert.equal((await one.ask({ type: "List", chain: "life", at: null })).type, "Read", "and read as it came")
  lacking = 0
  await settled()
})

test("a frame of an earlier build's form is kept aside on a chain, listed as a copy from before the update, and named nowhere else", async () => {
  fakeIndexedDB()
  held = [...start]
  down = false
  const rows: unknown[] = []
  const t = await tab(s => {
    if (s.type === "Told" && s.left.rows.length > 0) rows.push(s)
  })
  await t.ask({ type: "Exchange", chain: "life" })
  // A frame of this build, its form's four bytes changed: an earlier build's frame, as an update leaves it.
  await new Promise<void>((done, failed) => {
    const open = indexedDB.open("suzatary")
    open.onsuccess = () => {
      const store = open.result.transaction("replica", "readwrite").objectStore("replica")
      const keys = store.getAllKeys(IDBKeyRange.bound("chain/life/log/", "chain/life/log/\uffff"))
      keys.onsuccess = () => {
        const last = String(keys.result.at(-1))
        const get = store.get(last)
        get.onsuccess = () => {
          const bytes = new Uint8Array(get.result as Uint8Array)
          bytes[0] = (bytes[0] ?? 0) ^ 0x7f
          const put = store.put(bytes, "chain/life/log/000000099999")
          put.onsuccess = () => done()
        }
      }
    }
    open.onerror = () => failed(new Error("not opened"))
  })
  const reloaded = await tab(s => {
    if (s.type === "Told" && s.left.rows.length > 0) rows.push(s)
  })
  await reloaded.ask({ type: "Exchange", chain: "life" })
  assert.deepEqual(await reloaded.ask({ type: "Earlier" }), ["chain/life"], "listed where the device's copies are")
  assert.deepEqual(rows, [], "and named in no row that asks for the owner")
  assert.equal(await read(reloaded), cold(), "the chain reads the box")
  await settled()
})
