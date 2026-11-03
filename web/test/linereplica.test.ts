// The core's rows of the lines are the replica the TypeScript held: after every piece of the box's skeleton, landing,
// start over, purge and join of two tabs, the core's heads, box word, frontier, generation, readable nodes and keys are
// those `replica.ts` and `locked.ts` reach by the same steps, and a core loaded from the frames it took, or from its
// snapshot, holds the same rows.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import type { Core, LineStep, Lines } from "../src/prodrome/core"
import locked from "../src/prodrome/locked"
import { empty, overtaken, pulled, replicaOf, sinceOf, wentBack } from "../src/prodrome/replica"
import type { Frontier, Replica, Said } from "../src/prodrome/replica"
import type { SkeletonNode, SkeletonPayload } from "../src/wire/generated/wire"
import { forAll, type Rng } from "./gen"

const core = ((): Core => {
  const built = process.env["SUZATARY_PRODROME_WASM"]
  if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
  return createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
})()

const box = JSON.parse(readFileSync("app/core/tests/web/replicas.json", "utf8")) as { skeleton: SkeletonPayload; keys: Record<string, string> }

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

const OTHER = "e".repeat(64)
const DIGESTS = ["a".repeat(64), "b".repeat(64), "c".repeat(64)]
const keyNames = Object.keys(box.keys).sort()

type Row = locked.Row<SkeletonNode>
type Tab = { readonly core: Lines; landed: Row; pending: Row; readonly frames: Uint8Array[]; readonly log: Uint8Array[] }

const tab = (): Tab => ({ core: new core.Lines(), landed: locked.none(), pending: locked.none(), frames: [], log: [] })
const holding = (t: Tab): Replica<SkeletonNode> => locked.join(t.landed, t.pending).replica

type Asked = { readonly payload: SkeletonPayload; readonly keys: Record<string, string>; readonly fetchedAt: number }

// One answer of the box: a prefix of its store, its keys or some of them, an epoch, a frontier and how it read `since`.
const answerOf = (r: Rng): Asked => {
  const nodes = order.slice(0, r.int(order.length + 1))
  const parents = new Set(nodes.flatMap(n => n.parents))
  const named = new Set(nodes.flatMap(n => (n.header.node === "line" ? n.header.keys : [])))
  const keys = Object.fromEntries(Object.entries(box.keys).filter(([n]) => named.has(n) && r.bool(0.8)))
  return {
    payload: {
      ...box.skeleton,
      heads: r.bool(0.2) ? { are: "held" } : { are: "these", heads: nodes.filter(n => !parents.has(n.hash)).map(n => n.hash) },
      geneses: r.pick([box.skeleton.geneses, box.skeleton.geneses, box.skeleton.geneses, [], [OTHER], [...box.skeleton.geneses, OTHER].sort()]),
      epoch: r.pick([1, 1, 1, 2, 2, 3]),
      nodes,
      since: r.bool(0.2) ? "Unknown" : "Known",
      frontier: { digest: r.pick(DIGESTS), count: nodes.length },
    },
    keys,
    fetchedAt: r.int(4),
  }
}

const took = (t: Tab, step: LineStep): string => {
  const frame = step.delta
  const moved = step.moved
  step.free()
  t.frames.push(frame)
  t.log.push(frame)
  return moved
}

// What the TypeScript's telling answered a move of the landed row: every conversation where the generation or the epoch
// moved, else those with a line come or gone or naming a key given or taken.
const movedOf = (was: Row, now: Row): unknown => {
  const epoch = (r: Row): number => Math.max(0, ...r.replica.held.words.map(w => w.epoch))
  if (was.replica.generation !== now.replica.generation || epoch(was) !== epoch(now)) return "All"
  const before = new Set(was.replica.objects.map(n => n.hash))
  const after = new Set(now.replica.objects.map(n => n.hash))
  const touched = new Set<string>()
  for (const n of [...was.replica.objects.filter(n => !after.has(n.hash)), ...now.replica.objects.filter(n => !before.has(n.hash))]) {
    if (n.header.node === "line") touched.add(n.header.conversation)
  }
  const keys = new Set([...Object.keys(now.keys).filter(k => was.keys[k] === undefined), ...Object.keys(was.keys).filter(k => now.keys[k] === undefined)])
  for (const n of now.replica.objects) if (n.header.node === "line" && n.header.keys.some(k => keys.has(k))) touched.add(n.header.conversation)
  return [...touched].sort()
}

const startOver = (t: Tab): string => {
  const was = t.landed
  const generation = Math.max(t.landed.replica.generation, t.pending.replica.generation) + 1
  t.landed = locked.of(replicaOf<SkeletonNode>({ ...empty<SkeletonNode>().held, generation }))
  t.pending = locked.none()
  const moved = took(t, t.core.start_over())
  assert.deepEqual(JSON.parse(moved), movedOf(was, t.landed), "a start over moves what the TypeScript's telling said")
  return moved
}

const piece = (t: Tab, a: Asked): void => {
  const said = (r: Replica<SkeletonNode>): Said<SkeletonNode> => ({
    heads: a.payload.heads.are === "held" ? r.heads : a.payload.heads.heads,
    untrusted: [],
    root: null,
    geneses: a.payload.geneses,
    epoch: a.payload.epoch,
    objects: a.payload.nodes,
    since: a.payload.since,
    frontier: a.payload.frontier,
  })
  const first = said(holding(t))
  assert.equal(t.core.went_back(a.payload.epoch), wentBack(holding(t), first), "went back")
  const over = overtaken([t.landed.replica, t.pending.replica], first)
  assert.equal(t.core.overtaken(a.payload.epoch), over, "overtaken")
  if (over) startOver(t)
  const asked = holding(t)
  const row = pulled(replicaOf<SkeletonNode>({ ...empty<SkeletonNode>().held, generation: asked.generation, frontier: asked.held.frontier }), said(asked), a.fetchedAt)
  t.pending = locked.join(t.pending, locked.of(row, a.keys))
  const frame = t.core.piece(JSON.stringify(a.payload), JSON.stringify(a.keys), a.fetchedAt)
  t.frames.push(frame)
  t.log.push(frame)
}

const land = (t: Tab): void => {
  const was = t.landed
  t.landed = locked.join(t.landed, t.pending)
  t.pending = locked.none()
  assert.deepEqual(JSON.parse(took(t, t.core.land())), movedOf(was, t.landed), "a landing moves what the TypeScript's telling said")
}

const forget = (t: Tab, gone: readonly string[]): void => {
  const was = t.landed
  t.landed = locked.without(t.landed, gone)
  t.pending = locked.without(t.pending, gone)
  assert.deepEqual(JSON.parse(took(t, t.core.forget(JSON.stringify(gone)))), movedOf(was, t.landed), "a purge moves what the TypeScript's telling said")
}

const frontierOf = (f: Frontier): unknown => (f.type === "Named" ? { digest: f.digest, count: f.count, era: f.era } : null)

type Read = { generation: number; box: unknown; frontier: unknown; nodes: string[]; heads: string[]; keys: string[] }
type Rows = { landed: Read; pending: Read; holding: Read }

const readOf = (r: Row): Omit<Read, "generation"> => {
  const replica = r.replica
  return {
    box:
      replica.box.type === "Heard"
        ? { epoch: replica.box.word.epoch, heads: [...replica.box.word.heads].sort(), geneses: [...replica.box.word.geneses].sort(), fetchedAt: replica.box.word.fetchedAt }
        : null,
    frontier: frontierOf(replica.held.frontier),
    nodes: replica.objects.map(n => n.hash).sort(),
    heads: [...replica.heads].sort(),
    keys: Object.keys(r.keys).sort(),
  }
}

const without = (r: Read): Omit<Read, "generation"> => ({ box: r.box, frontier: r.frontier, nodes: r.nodes, heads: r.heads, keys: r.keys })

// The core's rows are the TypeScript's: the landed row whole, the pending pieces but for the generation an empty one
// stands in (the core's names the generation it was emptied in, the TypeScript's none), and what is held, landed and not.
const same = (t: Tab, said: string): void => {
  const rows = JSON.parse(t.core.rows()) as Rows
  const both = locked.join(t.landed, t.pending)
  assert.deepEqual(without(rows.landed), readOf(t.landed), `${said}: the landed row`)
  assert.equal(rows.landed.generation, t.landed.replica.generation, `${said}: the landed generation`)
  assert.deepEqual(without(rows.pending), readOf(t.pending), `${said}: the pending pieces`)
  assert.deepEqual(without(rows.holding), readOf(both), `${said}: what is held`)
  assert.equal(rows.holding.generation, both.replica.generation, `${said}: the generation held`)
  assert.deepEqual(JSON.parse(t.core.heads()), [...both.replica.heads].sort(), `${said}: the heads`)
  assert.equal(t.core.since(), sinceOf(both.replica), `${said}: since`)
  assert.equal(t.core.heard(), t.landed.replica.box.type === "Heard", `${said}: heard`)
}

const framed = (frames: readonly Uint8Array[]): Uint8Array => {
  const out = new Uint8Array(frames.reduce((n, f) => n + 4 + f.length, 0))
  let at = 0
  for (const f of frames) {
    new DataView(out.buffer).setUint32(at, f.length, true)
    out.set(f, at + 4)
    at += 4 + f.length
  }
  return out
}

test("law: the core's rows of the lines are the TypeScript replica's, step by step, and load back from their frames or snapshot", () => {
  forAll(60, (r: Rng) => {
    const a = tab()
    const b = tab()
    try {
      for (let step = 0; step < 24; step += 1) {
        const t = r.bool() ? a : b
        const what = r.int(10)
        if (what < 5) piece(t, answerOf(r))
        else if (what < 8) land(t)
        else if (what < 9) {
          // A purge on one tab alone, so the other's frames, which name none, join into it as rows do.
          if (t === a) forget(a, keyNames.filter(() => r.bool(0.4)))
          else startOver(b)
        } else {
          // Another tab's frames, the first to the last, taken once both tabs landed: the landed rows join.
          land(a)
          land(b)
          a.landed = locked.join(a.landed, b.landed)
          for (const frame of b.frames) {
            a.core.apply(frame)
            a.log.push(frame)
          }
          same(a, `step ${step}, the join`)
        }
        same(t, `step ${step}`)
        if (r.bool(0.15)) {
          const fromLog = new core.Lines()
          const fromSnapshot = new core.Lines()
          try {
            fromLog.load(undefined, framed(t.log))
            fromSnapshot.load(t.core.snapshot(), new Uint8Array())
            assert.deepEqual(JSON.parse(fromLog.rows()), JSON.parse(t.core.rows()), `step ${step}: the log loads back`)
            assert.deepEqual(JSON.parse(fromSnapshot.rows()), JSON.parse(t.core.rows()), `step ${step}: the snapshot loads back`)
          } finally {
            fromLog.free()
            fromSnapshot.free()
          }
        }
      }
    } finally {
      a.core.free()
      b.core.free()
    }
  })
})
