// A warm load holds what a cold one holds, and two tabs converge, and a fetch hands the core exactly the prints it
// lacks: the lines are the core's replica the lines are the core's replica, kept on the device as
// its snapshot and the frames after it (`replicalog`), and a tab's worker (`questions.serve` over the real core, a fake
// box and the fake IndexedDB) reads them back on a reload. After any interleaving of the box's growth, two tabs' pulls,
// compactions and reloads, each tab, once pulled, reads the conversations exactly as a core handed the box's whole store
// reads them; and a reload reads what the device kept with no answer from the box at all. Which prints are held is the
// core's alone to say, whatever its loads, landings and evictions.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import type { Core, Lines, Named } from "../src/prodrome/core"
import { framed, heldOf, holdAdmitted, lackingOf, listOfLines, objectsOf } from "../src/prodrome/core"
import content from "../src/prodrome/content"
import questions from "../src/prodrome/questions"
import { headsOf as objectHeads } from "../src/prodrome/replica"
import type { ContentPayload, SkeletonNode, SkeletonPayload, StorePayload, StoredObject } from "../src/wire/generated/wire"
import { fakeIndexedDB } from "./fakeidb"
import { forAllAsync } from "./gen"
import { held as prodromes } from "./held"

const built = process.env["SUZATARY_PRODROME_WASM"]
if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
const core = createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core

type Shipped = { hash: string; text: string }
const box = JSON.parse(readFileSync("app/core/tests/web/replicas.json", "utf8")) as {
  inbox: Shipped[]
  conversations: Shipped[]
  skeleton: SkeletonPayload
  keys: Record<string, string>
}
const inbox: Named[] = box.inbox.map(o => ({ hash: o.hash, literal: o.text }))
const literals = new Map(box.conversations.map(o => [o.hash, o.text]))

// The box's nodes parents first, so each prefix is a store the box held.
const order = ((): SkeletonNode[] => {
  const out: SkeletonNode[] = []
  const left = [...box.skeleton.nodes]
  while (left.length > 0) {
    const at = left.findIndex(n => n.parents.every(p => out.some(o => o.hash === p) || !left.some(l => l.hash === p)))
    out.push(...left.splice(at, 1))
  }
  return out
})()

// The frontier the box names after its first `held` nodes, which a replica names back as `since`.
const digestOf = (held: number): string => String(held).padStart(64, "0")

// The box holding its first `held` nodes, answering a replica at `since` with the nodes it lacks and the keys they name.
const answer = (held: number, since: string): SkeletonPayload => {
  const nodes = order.slice(0, held)
  const from = nodes.findIndex((_, i) => digestOf(i + 1) === since) + 1
  const sent = since === "" ? nodes : nodes.slice(from)
  const named = new Set(sent.flatMap(n => (n.header.node === "line" ? n.header.keys : [])))
  const parents = new Set(nodes.flatMap(n => n.parents))
  return {
    ...box.skeleton,
    heads: { are: "these", heads: nodes.filter(n => !parents.has(n.hash)).map(n => n.hash) },
    nodes: sent,
    since: since === "" || from > 0 ? "Known" : "Unknown",
    frontier: { digest: digestOf(held), count: held },
    keys: Object.fromEntries(Object.entries(box.keys).filter(([name]) => named.has(name))),
    rest: 0,
    size: 0,
    weight: 0,
  }
}

const objectsAnswer = (objects: readonly StoredObject[], since: string): StorePayload => {
  const frontier = { digest: "9".repeat(64), count: objects.length }
  const sent = since === frontier.digest ? [] : [...objects]
  return { objects: sent, heads: objectHeads(objects), untrusted: [], root: null, geneses: prodromes(objects), epoch: 1, since: "Known", frontier, keys: {}, rest: 0, size: 0, weight: 0, erased: [] }
}

// The box as `fetch` reaches it: `holds` of its lines, its inbox, two topic stores holding nothing; or no box at all.
let holds = 1
let down = false
const json = (v: unknown): Response => new Response(JSON.stringify(v), { status: 200, headers: { "content-type": "application/json" } })
Object.assign(globalThis, {
  fetch: async (input: string | URL): Promise<Response> => {
    if (down) return new Response("", { status: 503 })
    const url = new URL(String(input), "http://box")
    const since = url.searchParams.get("since") ?? ""
    if (url.pathname === "/api/stores/lines/skeleton") return json(answer(holds, since))
    const store = /^\/api\/stores\/(inbox|topics|topic-aliases)\/objects$/.exec(url.pathname)?.[1]
    if (store !== undefined) return json(objectsAnswer(store === "inbox" ? box.inbox.map(o => ({ hash: o.hash, literal: o.text })) : [], since))
    if (url.pathname === "/api/stores/lines/content") {
      const hashes = (url.searchParams.get("hashes") ?? "").split(",")
      return json({ objects: hashes.flatMap(hash => (literals.has(hash) ? [{ hash, literal: literals.get(hash)! }] : [])) } satisfies ContentPayload)
    }
    return new Response("", { status: 404 })
  },
})

// A tab's worker over the device.
type Tab = { readonly ask: <K extends questions.Kind>(q: questions.Of<K>) => Promise<questions.Answer<K>> }
const tab = (): Tab => {
  const serve = questions.serve(core)
  return { ask: async q => (await serve(q, () => Promise.resolve())) as never }
}

// The conversations as a tab's worker lists them.
const listed = async (t: Tab): Promise<string> => {
  const a = await t.ask({ type: "Conversations" })
  assert.equal(a.type, "Held", JSON.stringify(a))
  return JSON.stringify(a.type === "Held" ? a.v : null)
}

// A reading of a reload with the box gone, which never answers when the device does not hold the lines landed: it
// waits for a landing no box can bring. That is said, rather than waited on.
const offline = <A>(read: Promise<A>): Promise<A> =>
  Promise.race([read, new Promise<never>((_, no) => setTimeout(() => no(new Error("the reload waits for the box: the device does not hold the lines landed")), 10_000))])

// The conversations as a core handed the box's whole store, cold, lists them.
const cold = async (): Promise<string> => {
  const held = new core.Lines()
  const proposals = new core.Proposals(objectsOf(inbox))
  try {
    const whole = answer(holds, "")
    held.piece(JSON.stringify(whole), JSON.stringify(whole.keys), 0)
    held.land().free()
    const rows = await content.admit(
      order.slice(0, holds).map(n => n.hash),
      { objects: order.slice(0, holds).map(n => ({ hash: n.hash, literal: literals.get(n.hash)! })) },
    )
    assert.equal(rows.type, "Val")
    if (rows.type === "Val") holdAdmitted(held, rows.v)
    return JSON.stringify(listOfLines(held, proposals))
  } finally {
    proposals.free()
    held.free()
  }
}

test("law: a warm load reads what a cold load reads, and two tabs converge, whatever their pulls, compactions and reloads", async () => {
  // How many reloads read a log with frames, and how many read a snapshot, so the runs read both.
  const seen = { frames: 0, snapshots: 0 }
  await forAllAsync(30, async r => {
    const device = fakeIndexedDB()
    down = false
    holds = 1 + r.int(order.length)
    const tabs = [tab(), tab()]
    // A device that never landed the lines waits for the box to read them, and a pull brings their skeleton alone, their
    // content coming as a reading reads it: each tab lands and reads them first, so the device holds what a reload reads.
    for (const t of tabs) {
      assert.equal(await t.ask({ type: "Pull" }), true)
      await listed(t)
    }
    for (let step = 0; step < 12; step += 1) {
      const i = r.int(tabs.length)
      const op = r.pick(["Grow", "Pull", "Hide", "Reload"] as const)
      switch (op) {
        case "Grow":
          holds = Math.min(order.length, holds + 1 + r.int(4))
          break
        case "Pull":
          assert.equal(await tabs[i]!.ask({ type: "Pull" }), true, "the pull ends whole")
          break
        case "Hide":
          await tabs[i]!.ask({ type: "Shown", on: false })
          break
        case "Reload": {
          const kept = [...device.rows("suzatary", "replica").keys()].map(String)
          if (kept.includes("lines/snapshot")) seen.snapshots += 1
          if (kept.some(k => k.startsWith("lines/log/"))) seen.frames += 1
          // The tabs pulled after the step before, so the device holds the box's lines: a reload with the box gone reads
          // them from the device alone.
          down = true
          try {
            tabs[i] = tab()
            assert.equal(await offline(listed(tabs[i]!)), await cold(), "a reload reads what a cold core reads, with no answer from the box")
          } finally {
            down = false
          }
          break
        }
      }
      for (const t of tabs) {
        await t.ask({ type: "Pull" })
        assert.equal(await listed(t), await cold(), `after ${op} of tab ${i}, a tab reads what a cold core reads`)
      }
    }
  })
  assert.ok(seen.frames > 0 && seen.snapshots > 0, `the reloads read frames (${seen.frames}) and a snapshot (${seen.snapshots})`)
})

// What a core that landed the box's first `count` nodes holds, as its snapshot prints it.
const snapshotOf = (count: number): Uint8Array => {
  const held = new core.Lines()
  try {
    landed(held, count)
    return held.snapshot()
  } finally {
    held.free()
  }
}

// `held` with the box's first `count` nodes landed, and the keys they name.
const landed = (held: Lines, count: number): void => {
  const whole = answer(count, "")
  held.piece(JSON.stringify(whole), JSON.stringify(whole.keys), 0)
  held.land().free()
}

test("law: a fetch hands the core exactly the prints it lacks, and asks the box for none it holds, whatever came before", async () => {
  const lines = order.filter(n => literals.has(n.hash))
  const names = (nodes: readonly SkeletonNode[]): string[] => nodes.map(n => n.hash)
  // How many fetches found a print the core held and one it lacked, so the runs read both.
  const seen = { held: 0, lacked: 0 }
  await forAllAsync(40, async r => {
    fakeIndexedDB()
    let held = new core.Lines()
    let handed: string[] = []
    let asked: string[] = []
    const fetcherOf = (reader: Lines): content.Fetcher =>
      content.fetcher({
        ask: hashes => {
          asked.push(...hashes)
          return Promise.resolve({ objects: hashes.map(h => ({ hash: h, literal: literals.get(h)! })) })
        },
        asked: () => {},
        came: () => {},
        lost: () => {},
        atOnce: () => 4,
        lacking: wanted => lackingOf(reader, wanted),
        held: rows => {
          handed.push(...rows.map(x => content.rowOf(x).hash))
          holdAdmitted(reader, rows)
          return Promise.resolve()
        },
        shown: () => true,
      })
    let fetcher = fetcherOf(held)
    try {
      for (let step = 0; step < 12; step += 1) {
        const op = r.pick(["Fetch", "Load", "Land", "Evict", "Reload"] as const)
        switch (op) {
          case "Fetch": {
            const some = lines.filter(() => r.bool())
            const lacked = lackingOf(held, names(some))
            if (lacked.size > 0) seen.lacked += 1
            if (lacked.size < some.length) seen.held += 1
            handed = []
            asked = []
            await fetcher.fetch(r.pick(["First", "Behind"] as const), some)
            assert.deepEqual([...handed].sort(), [...lacked].sort(), "the core is handed exactly the prints it lacked")
            assert.ok(asked.every(h => lacked.has(h)), "the box is asked for none the core held")
            assert.equal(lackingOf(held, names(some)).size, 0, "and the core lacks none of them after")
            break
          }
          case "Load":
            held.load(snapshotOf(1 + r.int(order.length)), framed([]))
            break
          case "Land":
            landed(held, 1 + r.int(order.length))
            break
          case "Evict": {
            const kept = new Set(names(lines.filter(() => r.bool())))
            const was = heldOf(held)
            const done = await content.keepOnly(kept, was, drop => Promise.resolve(held.hold("[]", JSON.stringify(drop))))
            assert.equal(done.type, "Val")
            assert.deepEqual([...lackingOf(held, was)].sort(), was.filter(n => !kept.has(n)).sort(), "an eviction lets go exactly what it does not keep")
            break
          }
          case "Reload":
            held.free()
            held = new core.Lines()
            fetcher = fetcherOf(held)
            break
        }
      }
    } finally {
      held.free()
    }
  })
  assert.ok(seen.held > 0 && seen.lacked > 0, `the runs fetched prints the core held (${seen.held}) and lacked (${seen.lacked})`)
})
