// What a store's log does with what does not read (`replicalog.take`): an earlier build's copy goes without a word where
// the box alone writes the store, and is kept aside, one copy, where this device writes it; a part that does not read
// in this build's form, or a row that is no part, is kept aside and named. Each part set aside has a key of its own, the
// floor is raised first so no place is given again, and a reset whose snapshot does not print deletes nothing.
//
// The reader stands in for the core: a part's first byte is its form (9 an earlier build's, anything else this build's),
// and a part of this build's form reads where its first byte is 1.

import { test } from "node:test"
import assert from "node:assert/strict"

import replicalog from "../src/prodrome/replicalog"
import { CONTENT, REPLICA } from "../src/prodrome/objects"
import idb from "../src/app/idb"
import { fakeIndexedDB } from "./fakeidb"

const READS = 1
const EARLIER = 9
const UNREAD = 7

const reader = (): replicalog.Reader & { readonly applied: number[] } => {
  const applied: number[] = []
  const reads = (bytes: Uint8Array): void => {
    if (bytes[0] !== READS) throw new Error("does not read")
    applied.push(bytes[1] ?? 0)
  }
  return {
    applied,
    load: (snapshot, frames) => {
      if (snapshot !== undefined) reads(snapshot)
      let at = 0
      while (at < frames.length) {
        const length = new DataView(frames.buffer, frames.byteOffset + at, 4).getUint32(0, true)
        reads(frames.subarray(at + 4, at + 4 + length))
        at += 4 + length
      }
    },
    apply: reads,
    form: bytes => (bytes[0] === EARLIER ? "Other" : "This"),
  }
}

const part = (form: number, n: number): Uint8Array => new Uint8Array([form, n])

// Every key of the replica's store, and the value of each.
const rows = (): Promise<Map<string, unknown>> =>
  idb.within(REPLICA, "readonly", async s => {
    const [keys, values] = await Promise.all([idb.done(s.getAllKeys()), idb.done(s.getAll())])
    return new Map(keys.map((k, i) => [String(k), values[i]]))
  })
const put = (key: string, value: unknown): Promise<void> => idb.within(REPLICA, "readwrite", s => idb.done(s.put(value, key)).then(() => undefined))
const under = (all: Map<string, unknown>, prefix: string): string[] => [...all.keys()].filter(k => k.startsWith(prefix))

const read = async (log: replicalog.T, r: replicalog.Reader): Promise<replicalog.Taken> => {
  const kept = await log.read()
  assert.equal(kept.type, "Val")
  if (kept.type !== "Val") throw new Error("unread")
  const took = await log.take(kept.v, r)
  if (took.type !== "Val") throw new Error(took.e)
  return took.v
}

test("an earlier build's frames and snapshot go without a word where the box alone writes the store", async () => {
  fakeIndexedDB()
  const log = replicalog.of("inbox", CONTENT, "BoxAlone")
  await log.append(part(READS, 1))
  await log.append(part(EARLIER, 2))
  await put("inbox/snapshot", { through: 0, bytes: part(EARLIER, 3) })
  const r = reader()
  const took = await read(log, r)
  assert.deepEqual(took, { taken: true, unread: null }, "nothing is named")
  assert.deepEqual(r.applied, [1], "what reads is held")
  const all = await rows()
  assert.deepEqual(under(all, "aside/"), [], "nothing is kept aside")
  assert.deepEqual(under(all, "inbox/"), ["inbox/log/000000000001"], "the earlier build's parts are gone")
})

test("an earlier build's copy is kept aside, one copy, where this device writes the store, and nothing is named", async () => {
  fakeIndexedDB()
  const log = replicalog.of("topics", CONTENT, "OwnWrites")
  await log.append(part(EARLIER, 1))
  await log.append(part(READS, 2))
  await put("topics/snapshot", { through: 0, bytes: part(EARLIER, 3) })
  const took = await read(log, reader())
  assert.equal(took.unread, null, "an earlier build's copy asks nothing of the owner")
  assert.equal(under(await rows(), "aside/topics/earlier/").length, 2, "the snapshot and the frame, each under a key of its own")
  assert.deepEqual(await log.aside(), { type: "Val", v: { earlier: true, unread: false } })
  // Another earlier copy, as at the next change of form: the older one goes as it is kept.
  await log.append(part(EARLIER, 4))
  await read(log, reader())
  assert.equal(under(await rows(), "aside/topics/earlier/").length, 1, "one copy from before the last change of form")
})

test("a part of this build's form that does not read, and a row that is no part, are kept aside, named, each under its own key", async () => {
  fakeIndexedDB()
  const log = replicalog.of("topics", CONTENT, "OwnWrites")
  await log.append(part(READS, 1))
  await log.append(part(UNREAD, 2))
  await put("topics/log/000000000003", { not: "a frame" })
  const took = await read(log, reader())
  assert.equal(took.unread, "2 of its frames", "named for the store's row")
  assert.equal(under(await rows(), "aside/topics/unread/").length, 2)
  // A second part set aside later, appended by another tab, does not overwrite the first.
  const other = replicalog.of("topics", CONTENT, "OwnWrites")
  await other.read()
  await other.append(part(UNREAD, 4))
  const again = await log.since()
  if (again.type !== "Val") throw new Error("unread")
  await log.take(again.v, reader())
  assert.equal(under(await rows(), "aside/topics/unread/").length, 3)
})

test("setting aside the last frame raises the floor: its place is never given again", async () => {
  fakeIndexedDB()
  const log = replicalog.of("topics", CONTENT, "OwnWrites")
  await log.append(part(READS, 1))
  await log.append(part(UNREAD, 2))
  await read(log, reader())
  // A second tab already read up to place 2; the next frame must come after it, or that tab never reads it.
  const other = replicalog.of("topics", CONTENT, "OwnWrites")
  await other.read()
  await log.append(part(READS, 3))
  assert.ok(under(await rows(), "topics/log/").includes("topics/log/000000000003"), "the next frame takes place 3, after the one set aside")
  const seen = await other.since()
  if (seen.type !== "Val") throw new Error("unread")
  assert.equal(seen.v.from, "After")
  assert.deepEqual(seen.v.frames.map(f => f.at), [3], "the other tab reads it")
})

test("a reset whose snapshot does not print deletes nothing", async () => {
  fakeIndexedDB()
  const log = replicalog.of("topics", CONTENT, "OwnWrites")
  await log.append(part(READS, 1))
  await log.append(part(READS, 2))
  await read(log, reader())
  const before = await rows()
  const done = await log.reset(() => {
    throw new Error("does not print")
  })
  assert.equal(done.type, "Error")
  assert.deepEqual([...(await rows()).keys()].sort(), [...before.keys()].sort(), "every frame is where it was")
})
