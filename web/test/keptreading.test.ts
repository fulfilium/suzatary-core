// An append extends the reading a store keeps rather than making another, and a signing keeps it. So for every store of
// objects the tab holds, whatever it pulls, writes, signs or has refused, the reading it keeps must be the reading a
// store made afresh from the same objects reads, and an append must write what an append to that fresh store writes.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { createHash } from "node:crypto"
import { readFileSync } from "node:fs"

import type { Core, ObjectStore } from "../src/prodrome/core"
import type { StoredObject } from "../src/wire/generated/wire"
import { storeOf } from "./held"
import { forAllAsync, type Rng } from "./gen"

const core = ((): Core => {
  const built = process.env["SUZATARY_PRODROME_WASM"]
  if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
  return createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
})()

const sha = (s: string): string => createHash("sha256").update(s, "utf8").digest("hex")
const genesis = (label: string): StoredObject => {
  const literal = `Genesis(label='${label}', nonce='${"0".repeat(32)}')`
  return { hash: sha(literal), literal }
}
const at = (n: number): string => new Date(Date.UTC(2026, 10, 14, 8, 0, 0) + n * 1000).toISOString().replace("Z", "")
const datetime = (n: number): string => {
  const d = new Date(Date.UTC(2026, 10, 14, 8, 0, 0) + n * 1000)
  return `datetime(${d.getUTCFullYear()}, ${d.getUTCMonth() + 1}, ${d.getUTCDate()}, ${d.getUTCHours()}, ${d.getUTCMinutes()}, ${d.getUTCSeconds()})`
}
const unsentOf = (s: ObjectStore): StoredObject[] => (JSON.parse(s.unsent()) as StoredObject[][]).flat()

// What every reading of a store reads, as one string: what its reading leaves out, and its registers where its schema
// has a JSON view.
const read = (s: ObjectStore, registers: boolean): string => JSON.stringify([s.tips(), registers ? s.readings(null, "[]") : null])

// A store made afresh from `s`'s snapshot: the same objects, and a reading made from them.
const afresh = <S extends ObjectStore>(s: S, make: () => S): S => {
  const t = make()
  t.load(s.snapshot(), new Uint8Array())
  return t
}

type Kind = {
  readonly name: string
  readonly make: () => ObjectStore
  readonly registers: boolean
  // The box's store, parents first, genesis first.
  readonly box: (r: Rng) => StoredObject[]
  // An event the tab may write next, given the store as held; none for a store the tab never writes.
  readonly event: ((s: ObjectStore, r: Rng, n: number) => string | undefined) | null
}

// The topic stores' writes, planned by the topics as the core reads them with an empty other store.
const memberships = (s: ObjectStore, r: Rng, n: number): string | undefined => {
  const aliases = storeOf(new core.TopicAliases(), [genesis("topic-aliases")])
  const reading = new core.TopicReading(s, aliases)
  const target = `round:c${r.int(3)}/${r.int(3)}`
  const topic = r.pick(["travel", "work", "garden"])
  const e = r.bool() ? reading.add(target, topic, at(n), r.pick(["ada", "bob"])) : (JSON.parse(reading.remove(target, topic, at(n), "ada")) as string[])[0]
  reading.free()
  aliases.free()
  return e
}
const aliasing = (s: ObjectStore, r: Rng, n: number): string | undefined => {
  const members = storeOf(new core.Memberships(), [genesis("topics")])
  const reading = new core.TopicReading(members, s)
  const e = (JSON.parse(reading.rename(r.pick(["travel", "work"]), r.pick(["trips", "job"]), at(n), "ada")) as string[])[0]
  reading.free()
  members.free()
  return e
}
const refusal = (_: ObjectStore, r: Rng, n: number): string =>
  `Refused(request='${sha(String(r.int(1e9)))}', at=${datetime(n)}, actor='box', store='${r.pick(["topics", "topic-aliases"])}', reason='rests on what this box does not hold')`

// The box's store: its genesis, then writes as `event` plans them over what is written so far.
const written = (label: string, make: () => ObjectStore, event: (s: ObjectStore, r: Rng, n: number) => string | undefined) => (r: Rng): StoredObject[] => {
  const g = genesis(label)
  const s = storeOf(make(), [g])
  for (let n = 0; n < r.int(6); n++) {
    const e = event(s, r, 1000 + n)
    const f = e === undefined ? undefined : s.append(e)
    if (f !== undefined) s.apply(f)
  }
  const objects = [g, ...unsentOf(s)]
  s.free()
  return objects
}

const inbox = (JSON.parse(readFileSync("app/api/tests/web/replicas.json", "utf8")) as { inbox: { hash: string; text: string }[] }).inbox.map(o => ({ hash: o.hash, literal: o.text }))

const KINDS: readonly Kind[] = [
  { name: "topics", make: () => new core.Memberships(), registers: true, box: written("topics", () => new core.Memberships(), memberships), event: memberships },
  { name: "topic-aliases", make: () => new core.TopicAliases(), registers: true, box: written("topic-aliases", () => new core.TopicAliases(), aliasing), event: aliasing },
  { name: "asks", make: () => new core.Asks(), registers: false, box: written("asks", () => new core.Asks(), refusal), event: refusal },
  // The tab never writes the inbox: its law runs over what it pulls.
  { name: "inbox", make: () => new core.Proposals(), registers: true, box: () => inbox, event: null },
]

const hex = (bytes: ArrayBuffer): string => Array.from(new Uint8Array(bytes), b => b.toString(16).padStart(2, "0")).join("")
const key = (await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"])) as CryptoKeyPair
const publicKey = hex(await crypto.subtle.exportKey("raw", key.publicKey))
const signature = async (name: string): Promise<string> =>
  hex(await crypto.subtle.sign({ name: "Ed25519" }, key.privateKey, new TextEncoder().encode(`prodrome object ${name}`)))

test("law: for every store of objects, an append extends the kept reading to the reading of the same objects made afresh, and writes what an append to that fresh store writes", async () => {
  for (const kind of KINDS) {
    await forAllAsync(40, async (r, seed) => {
      const box = kind.box(r)
      const s = storeOf(kind.make(), box.slice(0, 1 + r.int(box.length)))
      for (let n = 0; n < 12; n++) {
        const move = r.int(4)
        if (move === 0) storeOf(s, box.slice(0, 1 + r.int(box.length)))
        else if (move === 1 && kind.event !== null && r.bool()) {
          const e = kind.event(s, r, n)
          if (e === undefined) continue
          const fresh = afresh(s, kind.make)
          const [f, g] = [fresh.append(e), s.append(e)]
          if (f !== undefined) fresh.apply(f)
          if (g !== undefined) s.apply(g)
          assert.equal(s.unsent(), fresh.unsent(), `${kind.name} seed ${seed}: an append to the kept reading writes what one to a fresh reading writes`)
          fresh.free()
        } else if (move === 1) {
          // An event its schema refuses: the append throws, and the reading it took is made again.
          assert.throws(() => s.append("Unheard(of='nothing')"))
        } else if (move === 2) {
          for (const name of JSON.parse(s.unsigned()) as string[]) {
            const signed = JSON.parse(core.signed(name, publicKey, await signature(name))) as { text: string }
            s.signed(signed.text).free()
          }
        } else {
          const own = unsentOf(s)
          if (own.length > 0) s.refuse(JSON.stringify([r.pick(own).hash]), "refused").free()
        }
        const fresh = afresh(s, kind.make)
        assert.equal(read(s, kind.registers), read(fresh, kind.registers), `${kind.name} seed ${seed}, move ${n}: the kept reading is the fresh one`)
        fresh.free()
      }
      s.free()
    })
  }
})
