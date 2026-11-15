// What this tab wrote, handed over by one engine for every store it writes: each change signed by the key only the page
// mints, posted in pieces the box takes whole, each answer kept, and each refusal for good said once in the box's words.

import type { Core, LineStep, ObjectStore, Written } from "./core"
import { CoreError, signedObject } from "./core"
import signer from "./signer"
import type stores from "./stores"
import lo from "../algebra/lo"
import d from "../wire/decoder"
import http from "../wire/http"
import storereads from "../wire/storereads"

export namespace written {
  // A store this tab writes: the noun a refusal names, its core class, and how a step of it is kept.
  export type Entry = {
    readonly noun: string
    readonly store: Written
    readonly stepped: (step: LineStep) => Promise<void>
  }
  // One for every store a tab writes, so a store left out does not compile.
  export type Entries = { readonly [N in stores.Written]: Entry }

  export type T = {
    // Every unsigned change of `name` signed, so a write made keyless is signed later.
    readonly sign: (name: stores.Written) => Promise<void>
    // What `name` holds unsent, signed and posted; an answer kept, a refusal said.
    readonly handOver: (name: stores.Written) => Promise<void>
    // The asks' refusals of every store's writes, held as refused.
    readonly refusedIn: (asks: ObjectStore) => Promise<void>
  }

  const BAD_REQUEST = 400

  // In posts the box takes whole, each change beside its signatures.
  const Unsent = d.json(d.arr(d.arr(d.obj<{ hash: string; literal: string }>({ hash: d.str, literal: d.str }))))
  const Names = d.json(d.arr(d.str))
  const Refusals = d.json(d.arr(d.obj<{ hash: string; reason: string }>({ hash: d.str, reason: d.str })))
  const read = <A>(what: string, decoded: lo.Settled<A>): A => {
    if (decoded.type === "Val") return decoded.v
    throw new CoreError(`${what}: unexpected shape (${decoded.e})`)
  }
  const refusalsIn = (store: Written): ReadonlyMap<string, string> => new Map(read("refusals", Refusals(store.refusals())).map(r => [r.hash, r.reason]))

  export const of = (core: Core, entries: Entries, say: (why: string) => void): T => {
    const entry = (name: stores.Written): Entry => entries[name]

    // `step`, a move of a store this tab writes, kept; each newly refused object said once, in the box's words.
    const settled = async (e: Entry, step: () => LineStep | undefined): Promise<void> => {
      const before = refusalsIn(e.store)
      const made = step()
      if (made === undefined) return
      await e.stepped(made)
      for (const [hash, reason] of refusalsIn(e.store)) if (!before.has(hash)) say(`The box refused a ${e.noun}: ${reason}`)
    }

    const sign = async (name: stores.Written): Promise<void> => {
      const e = entry(name)
      const unsigned = read("unsigned", Names(e.store.unsigned()))
      if (unsigned.length === 0) return
      const key = await signer.existing()
      if (key === null) return
      for (const hash of unsigned) await e.stepped(e.store.signed(signedObject(core, hash, key.public, await signer.sign(key, hash)).literal))
    }

    // A post with no answer leaves everything for the next sync. A 400 refuses the post's shape, not its objects: they
    // stay unsent, and the box's words are said once per tab.
    const shapes = new Set<string>()
    const handOver = async (name: stores.Written): Promise<void> => {
      await sign(name)
      const e = entry(name)
      for (const posting of read("unsent", Unsent(e.store.unsent()))) {
        try {
          const receipt = await storereads.post(
            name,
            posting.map(o => o.literal),
            e.store.since(),
          )
          await settled(e, () => e.store.answered(JSON.stringify(receipt)))
        } catch (x: unknown) {
          if (x instanceof http.ApiError && x.status === http.OUTDATED) return say(`The box runs a newer build: reload, and this ${e.noun} goes with the next sync.`)
          if (http.unanswered(x)) return
          const reason = lo.reason(x)
          if (x instanceof http.ApiError && x.status === BAD_REQUEST) {
            if (!shapes.has(reason)) say(`The box did not take a post of this tab's writes: ${reason}`)
            shapes.add(reason)
            return
          }
          await settled(e, () => e.store.refuse(JSON.stringify(posting.map(o => o.hash)), reason))
        }
      }
    }

    const refusedIn = async (asks: ObjectStore): Promise<void> => {
      for (const e of Object.values<Entry>(entries)) await settled(e, () => e.store.refused_in(asks))
    }

    return { sign, handOver, refusedIn }
  }
}
export default written
