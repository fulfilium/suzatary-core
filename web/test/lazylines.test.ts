// A device holding part of the lines' content holds a state the box could have sent: its skeleton is a down-set the box
// sent, its content is the box's bytes for their names, and every reading the worker answers of the content held is the
// reading of the whole pull at the same skeleton, or the page names what it still fetches.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import type { Core, Lines, Named } from "../src/prodrome/core"
import { storeOf } from "./held"
import { conversationOf, lackingIn, listOfLines, objectsOf, planOf, searchOf } from "../src/prodrome/core"
import lines from "../src/prodrome/lines"
import content from "../src/prodrome/content"
import { sha256Hex } from "../src/prodrome/verify"
import { empty, pulled } from "../src/prodrome/replica"
import type { Replica } from "../src/prodrome/replica"
import type { SkeletonNode, SkeletonPayload } from "../src/wire/generated/wire"
import { forAll, forAllAsync, type Rng } from "./gen"
import { fakeIndexedDB } from "./fakeidb"
import idb from "../src/app/idb"
import { CONTENT } from "../src/prodrome/objects"

const core = ((): Core => {
  const built = process.env["SUZATARY_PRODROME_WASM"]
  if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
  return createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
})()

type Shipped = { hash: string; text: string }
const box = JSON.parse(readFileSync("app/core/tests/web/replicas.json", "utf8")) as {
  inbox: Shipped[]
  conversations: Shipped[]
  skeleton: SkeletonPayload
  keys: Record<string, string>
}
const inbox: Named[] = box.inbox.map(o => ({ held: "literal" as const, hash: o.hash, literal: o.text }))
const literals = new Map(box.conversations.map(o => [o.hash, o.text]))
const ids = [...new Set(box.skeleton.nodes.flatMap(n => (n.header.node === "line" ? [n.header.conversation] : [])))]

// A fetcher's reader: the prints handed to it, and those of some names it lacks, as the core's `Lines` answers them.
const readerOf = (): {
  readonly prints: ReadonlySet<string>
  readonly lacking: content.Effects["lacking"]
  readonly held: content.Effects["held"]
  readonly husked: content.Effects["husked"]
} => {
  const prints = new Set<string>()
  return {
    prints,
    lacking: names => new Set(names.filter(n => !prints.has(n))),
    held: rows => Promise.resolve(void rows.forEach(r => prints.add(content.rowOf(r).hash))),
    husked: husks => Promise.resolve(void husks.forEach(h => prints.add(h.hash))),
  }
}

// A down-set of the skeleton the box's pieces could have left, in the order they arrived.
const skeleton = (r: Rng): Replica<SkeletonNode> => {
  const held = new Set<string>()
  const left = [...box.skeleton.nodes]
  const nodes: SkeletonNode[] = []
  for (let n = r.int(left.length + 1); n > 0; n -= 1) {
    const next = r.pick(left.filter(o => o.parents.every(p => held.has(p))))
    nodes.push(next)
    held.add(next.hash)
    left.splice(left.indexOf(next), 1)
  }
  return pulled(empty<SkeletonNode>(), { ...box.skeleton, heads: box.skeleton.heads.are === "these" ? box.skeleton.heads.heads : [], objects: nodes, untrusted: [], root: null }, 1)
}

// Some of the box's literals for nodes the skeleton holds: content is fetched for a node the skeleton names, and let go
// once none names it.
const fetched = (r: Rng, s: Replica<SkeletonNode>): ReadonlyMap<string, string> =>
  new Map(s.objects.filter(() => r.bool(0.7)).map(n => [n.hash, literals.get(n.hash)!]))

// The core's lines given `nodes` and `keys` as one landed piece of the box's skeleton.
const landed = (held: Lines, nodes: readonly SkeletonNode[], keys: Readonly<Record<string, string>>): void => {
  const parents = new Set(nodes.flatMap(n => n.parents))
  const answer: SkeletonPayload = {
    ...box.skeleton,
    heads: { are: "these", heads: nodes.filter(n => !parents.has(n.hash)).map(n => n.hash) },
    nodes: [...nodes],
  }
  held.piece(JSON.stringify(answer), JSON.stringify(keys), 1)
  held.land().free()
}

// Every answer the worker's lines holding `objects` give about `scope`, or the refusal they give instead.
const read = (objects: readonly Named[], scope: Extract<lines.Scope, { readonly type: "Whole" | "Conversations" }>): string => {
  const held = new core.Lines()
  const proposals = storeOf(new core.Proposals(), inbox)
  try {
    held.hold(objectsOf(objects), "[]")
    landed(held, [], box.keys)
    return scope.type === "Whole"
      ? JSON.stringify([listOfLines(held, proposals), searchOf(held, "draft"), searchOf(held, "train")])
      : JSON.stringify(scope.ids.map(id => conversationOf(held, proposals, id, {}, null)))
  } catch (e: unknown) {
    return `refused: ${String(e)}`
  } finally {
    proposals.free()
    held.free()
  }
}

// The stream's scope, the drawn rounds' objects, is the Rust's law (`held_lines.rs`), since only the core chooses it.
const scopes = [lines.WHOLE, ...ids.map(lines.one), lines.one("never-opened"), { type: "Conversations", ids } satisfies lines.Scope].flatMap(s =>
  s.type === "Whole" || s.type === "Conversations" ? [s] : [],
)

// A core holding the skeleton `s` and the prints `kept`: what the core's worker asks of its own skeleton.
const coreOf = (s: Replica<SkeletonNode>, kept: ReadonlyMap<string, string>): Lines => {
  const held = new core.Lines()
  landed(held, s.objects, {})
  held.hold(objectsOf([...kept].map(([hash, literal]) => ({ hash, literal }))), "[]")
  return held
}

test("law: a reading of part-fetched lines is the whole pull's reading at the same skeleton, or names what it fetches", () => {
  forAll(40, r => {
    const s = skeleton(r)
    const kept = fetched(r, s)
    const has = (name: string): boolean => kept.has(name)
    const names = new Set(s.objects.map(n => n.hash))
    // What the box's whole pull at this skeleton would have sent: every node with its literal.
    const whole = s.objects.map(n => ({ held: "literal" as const, hash: n.hash, literal: literals.get(n.hash)! }))
    const asked = coreOf(s, kept)
    try {
      for (const scope of scopes) {
        const need = lackingIn(asked, scope)
        if (need.length > 0) {
          assert.ok(need.every(n => names.has(n.hash) && !has(n.hash)), "it fetches only what the skeleton names and the device lacks")
          continue
        }
        const held = [...kept].map(([hash, literal]) => ({ hash, literal }))
        assert.equal(read(held, scope), read(whole, scope), JSON.stringify(scope))
      }
    } finally {
      asked.free()
    }
  })
})

test("law: content is taken only where its bytes hash to the name asked for, in the place asked", async () => {
  const objects = box.conversations.map(o => ({ held: "literal" as const, hash: o.hash, literal: o.text }))
  await forAllAsync(20, async r => {
    const asked = objects.filter(() => r.bool(0.5)).slice(0, content.MOST_HASHES)
    if (asked.length === 0) return
    const names = asked.map(o => o.hash)
    assert.equal((await content.admit(names, { objects: asked })).type, "Val")
    const i = r.int(asked.length)
    const forged = asked.map((o, j) => (j === i ? { ...o, literal: `${o.literal} ` } : o))
    assert.equal((await content.admit(names, { objects: forged })).type, "Error", "a literal that is not its name's")
    if (asked.length > 1) assert.equal((await content.admit(names, { objects: [...asked].reverse() })).type, "Error", "answers out of place")
    assert.equal((await content.admit(names, { objects: asked.slice(1) })).type, "Error", "an answer short of the question")
    // A husk is not its name's hash: it is taken for the reader to check against the line's node, in its place alone.
    const husked = asked.map((o, j) => (j === i ? { held: "husk" as const, hash: o.hash, husk: "a husk" } : o))
    const taken = await content.admit(names, { objects: husked })
    assert.ok(taken.type === "Val" && taken.v.husks.length === 1 && taken.v.literals.length === asked.length - 1)
    if (asked.length > 1) assert.equal((await content.admit(names, { objects: [...husked].reverse() })).type, "Error", "a husk out of place")
  })
})

// The next answer a fetcher sends: each ask settles the wait for it, so a test moves when an answer goes out and never on
// a timer. An answer that goes out after one came waits on `admit`, whose hashing settles on a later task. A wait that
// sees no answer go out in `HANG_MS` fails, naming how many had, rather than holding the suite.
const HANG_MS = 10_000
const asking = (asked: readonly unknown[]): { readonly sent: () => void; readonly next: () => Promise<void> } => {
  let sent = (): void => {}
  return {
    sent: () => sent(),
    next: () =>
      new Promise((done, fail) => {
        const stop = setTimeout(() => fail(new Error(`no answer went out in ${HANG_MS} ms after the ${asked.length} that had`)), HANG_MS)
        sent = () => (clearTimeout(stop), done())
      }),
  }
}

test("an opened conversation's names go out before the whole store's batches still queued, and each name is asked once", async () => {
  const asked: string[][] = []
  const answers: (() => void)[] = []
  const { sent, next } = asking(asked)
  const { fetch } = content.fetcher({
    ask: hashes => {
      asked.push([...hashes])
      sent()
      return new Promise(done => answers.push(() => done({ objects: hashes.map(h => ({ held: "literal" as const, hash: h, literal: literals.get(h)! })) })))
    },
    asked: () => {},
    came: () => {},
    lost: () => {},
    atOnce: () => 4,
    ...readerOf(),
    shown: () => true,
  })
  // One name a batch, so the whole store queues more batches than go out at once.
  const one = box.skeleton.nodes.map(n => ({ ...n, size: content.MOST_BYTES }))
  const whole = fetch("Behind", one)
  await new Promise(done => setTimeout(done, 0))
  assert.equal(asked.length, 4, "as many out as may be")
  const opened = one.filter(n => !asked.flat().includes(n.hash)).slice(-2)
  const first = fetch("First", opened)
  await new Promise(done => setTimeout(done, 0))
  for (let i = 0; i < opened.length; i += 1) {
    const going = next()
    answers.shift()!()
    await going
  }
  assert.deepEqual(
    asked.slice(4, 4 + opened.length).flat().sort(),
    opened.map(n => n.hash).sort(),
    "the next out are the opened conversation's",
  )
  while (asked.flat().length < one.length) {
    const more = next()
    answers.shift()!()
    await more
  }
  for (const answer of answers.splice(0)) answer()
  await Promise.all([whole, first])
  assert.equal(new Set(asked.flat()).size, asked.flat().length, "no name asked twice")
})

test("over HTTP/1.1 one content answer is out at a time, beside the three streams, under the browser's six a host", () => {
  assert.equal(content.atOnce("http/1.1"), 1)
  assert.equal(content.atOnce(""), 1, "a protocol the browser has not said is read as HTTP/1.1")
  assert.equal(content.atOnce("h2"), 4)
})

test("law: the core's plan keeps the newest conversations whole, in order, within the budget", () => {
  forAll(60, r => {
    const s = skeleton(r)
    const total = s.objects.reduce((k, n) => k + n.size, 0)
    const budget = r.int(total + 2)
    const held = coreOf(s, new Map())
    const plan = ((): ReturnType<typeof planOf> => {
      try {
        return planOf(held, budget)
      } finally {
        held.free()
      }
    })()
    // No print is held, so every name kept is the plan's, and the plan's nodes all lack their print.
    const planned = plan.ahead
    const kept = new Set(planned.map(n => n.hash))
    assert.deepEqual([...plan.keep].sort(), [...kept].sort(), "the names kept are the plan's")
    assert.equal(kept.size, planned.length, "each node once")
    assert.ok(planned.reduce((k, n) => k + n.size, 0) <= budget, "within the budget")
    assert.ok(planned.every(n => n.parents.every(p => kept.has(p) || !s.objects.some(o => o.hash === p))), "a down-set of the skeleton")
    // Each conversation, newest line first, with every node its lines rest on.
    const byName = new Map(s.objects.map(n => [n.hash, n]))
    const under = (id: string): Set<string> => {
      const out = new Set<string>()
      const walk = s.objects.filter(n => n.header.node === "line" && n.header.conversation === id).map(n => n.hash)
      for (let h = walk.pop(); h !== undefined; h = walk.pop()) {
        if (out.has(h) || !byName.has(h)) continue
        out.add(h)
        walk.push(...byName.get(h)!.parents)
      }
      return out
    }
    const newest = (id: string): string =>
      s.objects.reduce((m, n) => (n.header.node === "line" && n.header.conversation === id && n.header.at > m ? n.header.at : m), "")
    const recent = [...new Set(s.objects.flatMap(n => (n.header.node === "line" ? [n.header.conversation] : [])))].sort((a, b) =>
      newest(a) < newest(b) ? 1 : newest(a) > newest(b) ? -1 : a < b ? -1 : 1,
    )
    const whole = recent.map(id => [...under(id)].every(h => kept.has(h)))
    const first = whole.indexOf(false)
    assert.ok(first === -1 || whole.slice(first).every(w => !w), "the conversations kept are the newest, each whole")
    const ofKept = new Set(recent.filter((_, i) => whole[i]))
    assert.ok(planned.every(n => n.header.node !== "line" || ofKept.has(n.header.conversation)), "no line of a conversation not kept whole")
    if (budget >= total) assert.ok(s.objects.every(n => n.header.node !== "line" || kept.has(n.hash)), "a budget past the store keeps every line")
  })
})

test("a prefetch goes out one answer at a time, only when no other is out, and stops at its first failure", async () => {
  const own = new Map(await Promise.all(Array.from({ length: 9 }, async (_, i): Promise<[string, string]> => [await sha256Hex(`prefetched line ${i}`), `prefetched line ${i}`])))
  const template = box.skeleton.nodes[0]!
  const one = [...own.keys()].map(hash => ({ ...template, hash, parents: [], size: content.MOST_BYTES }))
  const asked: string[][] = []
  const answers: ((ok: boolean) => void)[] = []
  const { sent, next } = asking(asked)
  // How many page answers had come when each answer went out.
  let came = 0
  const cameBefore: number[] = []
  const { fetch } = content.fetcher({
    ask: hashes => {
      asked.push([...hashes])
      cameBefore.push(came)
      sent()
      return new Promise((done, fail) =>
        answers.push(ok => (ok ? done({ objects: hashes.map(h => ({ held: "literal" as const, hash: h, literal: own.get(h)! })) }) : fail(new Error("offline")))),
      )
    },
    asked: () => {},
    came: () => void (came += 1),
    lost: () => {},
    atOnce: () => 4,
    ...readerOf(),
    shown: () => true,
  })
  const tick = (): Promise<void> => new Promise(done => setTimeout(done, 0))
  const ahead = fetch("Ahead", one.slice(0, 6))
  await tick()
  assert.equal(asked.length, 1, "one prefetch answer out")
  const first = fetch("First", one.slice(6, 9))
  await tick()
  assert.equal(asked.length, 4, "a page's names go out beside it, up to the bound")
  const goesOn = next()
  for (let i = 0; i < 4; i += 1) answers.shift()!(true)
  await first
  await goesOn
  assert.equal(asked.length, 5, "the prefetch goes on")
  assert.equal(cameBefore[4], 3, "only once every page answer came")
  answers.shift()!(false)
  await assert.rejects(ahead)
  await tick()
  assert.equal(asked.length, 5, "and stops at its first failure")
})

test("a prefetch sends nothing while the page is hidden, and once it is shown a wake sends exactly one answer", async () => {
  const own = new Map(await Promise.all(Array.from({ length: 3 }, async (_, i): Promise<[string, string]> => [await sha256Hex(`hidden line ${i}`), `hidden line ${i}`])))
  const template = box.skeleton.nodes[0]!
  const nodes = [...own.keys()].map(hash => ({ ...template, hash, parents: [], size: content.MOST_BYTES }))
  const asked: string[][] = []
  let shown = false
  const { fetch, wake } = content.fetcher({
    ask: hashes => {
      asked.push([...hashes])
      return new Promise(() => {})
    },
    asked: () => {},
    came: () => {},
    lost: () => {},
    atOnce: () => 4,
    ...readerOf(),
    shown: () => shown,
  })
  const tick = (): Promise<void> => new Promise(done => setTimeout(done, 0))
  void fetch("Ahead", nodes)
  await tick()
  assert.equal(asked.length, 0, "no prefetch answer goes out while the page is hidden")
  wake()
  await tick()
  assert.equal(asked.length, 0, "nor on a wake while it is still hidden")
  shown = true
  wake()
  await tick()
  assert.equal(asked.length, 1, "once shown, a wake sends one answer")
})

test("concurrent whole readings read each object the device keeps once, in one transaction, and ask the box nothing", async () => {
  const device = fakeIndexedDB()
  await content.keepOnly(new Set(), [], () => Promise.resolve())
  const nodes = box.skeleton.nodes
  await idb.within(CONTENT, "readwrite", s => Promise.all(nodes.map(n => idb.done(s.put(literals.get(n.hash)!, n.hash)))))
  const handed: string[] = []
  const reader = readerOf()
  const { fetch } = content.fetcher({
    ask: () => Promise.reject(new Error("the device keeps every object")),
    asked: () => {},
    came: () => {},
    lost: () => {},
    atOnce: () => 4,
    lacking: reader.lacking,
    held: rows => reader.held(rows).then(() => void handed.push(...rows.map(r => content.rowOf(r).hash))),
    husked: reader.husked,
    shown: () => true,
  })
  const before = [device.transactions(), device.read()] as const
  await fetch("Ahead", nodes)
  assert.equal(device.transactions() - before[0], 1, "a prefetch reads the device in one transaction")
  assert.equal(device.read() - before[1], 0, "by name alone")
  assert.deepEqual(handed, [], "and hands the reader nothing")
  const [begun, read] = [device.transactions(), device.read()]
  await Promise.all(Array.from({ length: 10 }, (_, i) => fetch(i % 2 === 0 ? "Behind" : "First", nodes)))
  assert.equal(device.transactions() - begun, 1, "one transaction for the one batch")
  assert.ok(device.read() - read <= nodes.length, `${device.read() - read} values read for ${nodes.length} objects`)
  assert.deepEqual([...handed].sort(), nodes.map(n => n.hash).sort(), "each object handed to the reader once")
  assert.ok(nodes.every(n => reader.prints.has(n.hash)))
})

test("a prefetch's husks go to the device's store and not to the reader, which takes them from there when a reading asks", async () => {
  fakeIndexedDB()
  await content.keepOnly(new Set(), [], () => Promise.resolve())
  const template = box.skeleton.nodes[0]!
  const nodes = ["a", "b", "c"].map(c => ({ ...template, hash: c.repeat(64), parents: [], size: 300 }))
  let asks = 0
  const husked: string[] = []
  const { fetch } = content.fetcher({
    ask: hashes => {
      asks += 1
      return Promise.resolve({ objects: hashes.map(hash => ({ held: "husk" as const, hash, husk: `husk of ${hash}` })) })
    },
    asked: () => {},
    came: () => {},
    lost: () => {},
    atOnce: () => 4,
    lacking: names => new Set(names.filter(n => !husked.includes(n))),
    held: () => Promise.resolve(),
    husked: husks => Promise.resolve(void husked.push(...husks.map(h => h.hash))),
    shown: () => true,
  })
  await fetch("Ahead", nodes)
  assert.equal(asks, 1)
  assert.deepEqual(husked, [], "a prefetch hands the reader no husk, as it hands it no literal")
  await fetch("First", nodes)
  assert.equal(asks, 1, "a reading takes the prefetched husks from the device and asks the box nothing")
  assert.deepEqual(husked.sort(), nodes.map(n => n.hash).sort())
})
