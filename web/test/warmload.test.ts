// A warm load holds what a cold one holds: after any interleaving of the box's landings, two tabs' pulls, the device's
// snapshot writes and reloads, the skeleton and keys a tab's core holds are those of a core handed the tab's row whole,
// and every print the tab handed its core is held still, so a conversation it read is never one the core lacks.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import type { Core, Lines, Named } from "../src/prodrome/core"
import { conversationOf, nodesInto, objectsOf } from "../src/prodrome/core"
import engine from "../src/prodrome/engine"
import linesync from "../src/prodrome/linesync"
import locked from "../src/prodrome/locked"
import noderows from "../src/prodrome/noderows"
import { LINE_ROWS } from "../src/prodrome/objects"
import type pull from "../src/prodrome/pull"
import lo from "../src/algebra/lo"
import payload from "../src/wire/generated/payload"
import type { SkeletonNode, SkeletonPayload } from "../src/wire/generated/wire"
import { fakeIndexedDB } from "./fakeidb"
import { forAllAsync } from "./gen"

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

// The box holding the first `held` nodes answers with all of them and the keys they name.
const answer = (held: number): pull.Answer<SkeletonNode> => {
  const nodes = order.slice(0, held)
  const named = new Set(nodes.flatMap(n => (n.header.node === "line" ? n.header.keys : [])))
  const keys = Object.fromEntries(Object.entries(box.keys).filter(([name]) => named.has(name)))
  const parents = new Set(nodes.flatMap(n => n.parents))
  return {
    said: {
      heads: nodes.filter(n => !parents.has(n.hash)).map(n => n.hash),
      untrusted: [],
      root: null,
      geneses: box.skeleton.geneses,
      epoch: box.skeleton.epoch,
      objects: nodes,
      since: "Known",
      frontier: { digest: String(held).padStart(64, "0"), count: held },
    },
    erased: [],
    piece: r => locked.of(r, keys),
    rest: 0,
    size: 0,
    weight: 0,
  }
}

const KEEP = (): Promise<lo.Settled<null>> => Promise.resolve(lo.val(null))

// A tab of the device: its core, the lines' rows with the core's snapshot beside them, and its store, asking the box.
type Tab = { readonly held: Lines; readonly place: noderows.Place<SkeletonNode>; readonly store: engine.T<SkeletonNode> }
const tabOf = (boxHolds: () => number): Tab => {
  const held = new core.Lines()
  const told = linesync.telling(held, () => {})
  const place = noderows.placeOf(LINE_ROWS, payload.SkeletonNode, { version: linesync.FORMAT, dropped: KEEP }, { type: "Snapshot", print: told.printed })
  const store = engine.of<SkeletonNode>(
    {
      name: "lines",
      place,
      reader: told.reader,
      ask: () => Promise.resolve(answer(boxHolds())),
      content: engine.INLINE,
      words: engine.open(),
      took: told.took,
      restoring: () => Promise.resolve(),
      landed: () => Promise.resolve(),
    },
    { told: () => {}, bars: () => {}, replaced: () => {} },
  )
  return { held, place, store }
}

// What a core handed `row` whole holds, as its snapshot prints it.
const cold = (row: locked.Row<SkeletonNode>): Uint8Array => {
  const held = new core.Lines()
  try {
    nodesInto(held, row.replica.objects, [])
    held.keys(JSON.stringify(row.keys), "[]")
    return held.snapshot()
  } finally {
    held.free()
  }
}

// The prints of `nodes` a stream's reading handed the core, the lines and what they rest on, and the conversations
// they are of.
const handed = (held: Lines, nodes: readonly SkeletonNode[]): string[] => {
  held.hold(objectsOf(nodes.flatMap(n => (literals.has(n.hash) ? [{ hash: n.hash, literal: literals.get(n.hash)! }] : []))), "[]")
  return [...new Set(nodes.flatMap(n => (n.header.node === "line" ? [n.header.conversation] : [])))]
}

const holdsWhatColdHolds = (t: Tab, said: string): void =>
  assert.deepEqual([...t.held.snapshot()], [...cold(t.store.row())], `${said}: the core holds what a core handed its row whole holds`)

test("a warm load's core holds what a cold load's does, and keeps every print it was handed", async () => {
  // How many reloads were warm, and how many of those read rows that had moved beside their snapshot.
  const seen = { warm: 0, moved: 0 }
  await forAllAsync(40, async r => {
    fakeIndexedDB()
    let holds = 1 + r.int(order.length)
    const boxHolds = (): number => holds
    const tabs: Tab[] = [tabOf(boxHolds), tabOf(boxHolds)]
    for (const t of tabs) await t.store.sync.restored.get()
    const proposals = new core.Proposals(objectsOf(inbox))
    try {
      for (let step = 0; step < 14; step += 1) {
        const i = r.int(tabs.length)
        const op = r.pick(["Grow", "Pull", "Settle", "Reload"] as const)
        switch (op) {
          case "Grow":
            holds = Math.min(order.length, holds + 1 + r.int(4))
            break
          case "Pull":
            await tabs[i]!.store.want()
            break
          case "Settle":
            await tabs[i]!.place.settle()
            break
          case "Reload": {
            await tabs[i]!.place.settle()
            tabs[i]!.held.free()
            const t = tabOf(boxHolds)
            tabs[i] = t
            const loaded = await t.store.sync.loaded()
            // A stream answered from the snapshot hands the core the prints of what it read, before the rows are read.
            const read = loaded ? handed(t.held, order) : []
            // Meanwhile the other tab may land what the box opened since, and the rows move beside the snapshot.
            if (loaded) seen.warm += 1
            if (r.bool()) {
              if (loaded) seen.moved += 1
              holds = Math.min(order.length, holds + 1 + r.int(4))
              await tabs[1 - i]!.store.want()
            }
            await t.store.sync.restored.get()
            for (const id of read) {
              const refused = ((): string | null => {
                try {
                  conversationOf(t.held, proposals, id, {}, null)
                  return null
                } catch (e: unknown) {
                  return String(e)
                }
              })()
              assert.equal(refused, null, `${id}, read before the rows, is read after them`)
            }
            break
          }
        }
        for (const [n, t] of tabs.entries()) holdsWhatColdHolds(t, `after ${op} of tab ${i}, tab ${n}`)
      }
    } finally {
      // Each settle clears the quiet's timer, so nothing waits on it.
      for (const t of tabs) {
        await t.place.settle()
        t.held.free()
      }
      proposals.free()
    }
  })
  assert.ok(seen.warm > 0 && seen.moved > 0, `the runs loaded warm (${seen.warm}) and moved the rows beside a snapshot (${seen.moved})`)
})
