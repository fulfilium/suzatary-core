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
import ownlines from "./ownlines"

export namespace written {
  // A store this tab writes: the noun a refusal names, its core class, how a step of it is kept, and, for a store whose
  // writes hold the person's words, where the device keeps which refusals it told them of.
  export type Entry = {
    readonly noun: string
    readonly store: Written
    readonly stepped: (step: LineStep) => Promise<void>
    readonly told?: { readonly seen: (literals: readonly string[]) => Promise<void>; readonly told: () => Promise<ReadonlySet<string>> }
  }
  // One for every store `N` names, so a store left out does not compile.
  export type Entries<N extends stores.Written> = { readonly [K in N]: Entry }

  export type T<N extends stores.Written> = {
    // Every unsigned change of `name` signed, so a write made keyless is signed later.
    readonly sign: (name: N) => Promise<void>
    // What `name` holds unsent, signed and posted; an answer kept, a refusal said.
    readonly handOver: (name: N) => Promise<void>
    // The asks' refusals of every store's writes, held as refused.
    readonly refusedIn: (asks: ObjectStore) => Promise<void>
    // Each refusal of `name` the person was not told of yet, said with their words: what a load found.
    readonly said: (name: N) => Promise<void>
  }

  // In posts the box takes whole, each change beside its signatures, and each line beside its slots' keys, which
  // travel beside the objects and never in one.
  type Posted = { hash: string; literal: string; keys: readonly string[] }
  const Unsent = d.json(d.arr(d.arr(d.obj<Posted>({ hash: d.str, literal: d.str, keys: d.arr(d.str) }))))
  const Names = d.json(d.arr(d.str))
  // `signature`: a signature of a write, refused or waiting with it; it tells nothing of its own.
  type Refused = { hash: string; reason: string; literal: string | null; words: string | null; signature: boolean }
  const Refusals = d.json(
    d.arr(d.obj<Refused>({ hash: d.str, reason: d.str, literal: d.nullable(d.str), words: d.nullable(d.str), signature: d.bool })),
  )
  const read = <A>(what: string, decoded: lo.Settled<A>): A => {
    if (decoded.type === "Val") return decoded.v
    throw new CoreError(`${what}: unexpected shape (${decoded.e})`)
  }
  const refusalsIn = (store: Written): ReadonlyMap<string, Refused> => new Map(read("refusals", Refusals(store.refusals())).map(r => [r.hash, r]))
  // What the box has not taken yet, in its rule's words: a refusal for now, said once and posted again.
  type Waits = { hash: string; reason: string; literal: string | null; words: string | null; signature: boolean }
  const Waiting = d.json(
    d.arr(d.obj<Waits>({ hash: d.str, reason: d.str, literal: d.nullable(d.str), words: d.nullable(d.str), signature: d.bool })),
  )
  const waitingIn = (store: Written): ReadonlyMap<string, Waits> => new Map(read("waiting", Waiting(store.waiting())).map(w => [w.hash, w]))
  const waits = (noun: string, w: Waits): string =>
    `The box has not taken a ${noun} yet: ${w.reason}. It goes again with the next sync${w.words === null || w.words === "" ? "" : `. What you wrote: “${w.words}”`}`
  // A refusal in the box's words, and the person's own words where the write held some, to copy.
  const sentence = (noun: string, r: Refused): string =>
    `The box refused a ${noun}: ${r.reason}${r.words === null || r.words === "" ? "" : `. What you wrote: “${r.words}”`}`

  // What an answer that is no receipt means for the post it answers. Total over every failure: the box gone or failing,
  // or a refusal of who posts or how often, leaves the post for the next sync; a newer build waits for a reload; a box of
  // another build (its own 409 naming it, or an older box's 404 for a store it does not keep) is a function of the builds,
  // never of the post, and leaves it for later too; any other refusal is a function of the post alone, so posting it
  // again gets the same answer, and its lines are refused for good as the rule refuses one. A 200 whose receipt does not
  // decode is no `ApiError`, so it reads as no answer: the post goes again, which is safe whether or not the box took it.
  // An older box's 400 for a body of another shape cannot be told from the post's own fault, and refuses its lines for
  // good: a box older than its own 409 for another build is the one case read wrongly, accepted and left for later.
  export type Failed =
    | { readonly type: "Unanswered" }
    | { readonly type: "Outdated" }
    | { readonly type: "Later"; readonly why: string }
    | { readonly type: "Refused"; readonly why: string }
  const LATER: ReadonlySet<number> = new Set([0, 401, 403, 404, 408, 409, 410, 425, 429])
  export const failed = (x: unknown): Failed => {
    if (http.unanswered(x)) return { type: "Unanswered" }
    if (x instanceof http.ApiError && x.status === http.OUTDATED) return { type: "Outdated" }
    const why = lo.reason(x)
    if (!(x instanceof http.ApiError) || LATER.has(x.status)) return { type: "Later", why }
    return { type: "Refused", why }
  }

  export const of = <N extends stores.Written>(core: Core, entries: Entries<N>, say: (why: string) => void): T<N> => {
    const entry = (name: N): Entry => entries[name]

    // `step`, a move of a store this tab writes, kept; each newly refused object said once, in the box's words.
    const settled = async (e: Entry, step: () => LineStep | undefined): Promise<void> => {
      const before = refusalsIn(e.store)
      const made = step()
      if (made === undefined) return
      await e.stepped(made)
      const fresh = [...refusalsIn(e.store)].filter(([hash]) => !before.has(hash)).map(([, r]) => r)
      await tell(e, fresh)
      await waited(e)
    }

    // Each line waiting in the rule's words said once, its words with it; where the device keeps which it told, once
    // across its tabs and loads, else once while this tab knows it. A signature waits with what it signs, unsaid.
    const toldHere = new Set<string>()
    const waited = async (e: Entry): Promise<void> => {
      const told = e.told === undefined ? new Set<string>() : await e.told.told()
      const key = (w: Waits): string => (w.literal ?? w.hash) + ownlines.WAITS
      const fresh = [...waitingIn(e.store).values()].filter(w => !told.has(key(w)) && !toldHere.has(key(w)))
      for (const w of fresh) {
        if (!w.signature) say(waits(e.noun, w))
        toldHere.add(key(w))
      }
      const marked = fresh.flatMap(w => (w.literal === null ? [] : [key(w)]))
      if (marked.length > 0) await e.told?.seen(marked)
    }

    // Each refusal said, with the person's words, and the device told it was said, so a record's words go only after. A
    // signature is refused with what it signs and says nothing of its own: one sentence for one write.
    const tell = async (e: Entry, refusals: readonly Refused[]): Promise<void> => {
      for (const r of refusals) if (!r.signature) say(sentence(e.noun, r))
      await e.told?.seen(refusals.flatMap(r => (r.literal === null ? [] : [r.literal])))
    }

    const said = async (name: N): Promise<void> => {
      const e = entry(name)
      if (e.told === undefined) return
      const told = await e.told.told()
      await tell(e, [...refusalsIn(e.store).values()].filter(r => r.literal !== null && !told.has(r.literal)))
      await waited(e)
    }

    const sign = async (name: N): Promise<void> => {
      const e = entry(name)
      const unsigned = read("unsigned", Names(e.store.unsigned()))
      if (unsigned.length === 0) return
      const key = await signer.existing()
      if (key === null) return
      for (const hash of unsigned) await e.stepped(e.store.signed(signedObject(core, hash, key.public, await signer.sign(key, hash)).literal))
    }

    // Each piece posted in turn, its receipt kept. An answer that is no receipt is read by `failed`: one for later stops
    // the hand-over, said once per tab; a refusal of the post refuses its lines for good, said with their words as a
    // rule's refusal is, and the hand-over goes on to the next piece, so one post can never hold back the rest.
    const shapes = new Set<string>()
    const handOver = async (name: N): Promise<void> => {
      await sign(name)
      const e = entry(name)
      for (const posting of read("unsent", Unsent(e.store.unsent()))) {
        try {
          const keys = Object.fromEntries(posting.filter(o => o.keys.length > 0).map(o => [o.hash, o.keys]))
          const receipt = await storereads.post(
            name,
            posting.map(o => o.literal),
            e.store.since(),
            keys,
          )
          await settled(e, () => e.store.answered(JSON.stringify(receipt)))
        } catch (x: unknown) {
          const answer = failed(x)
          switch (answer.type) {
            case "Unanswered":
              return
            case "Outdated":
              return say(`The box runs a newer build: reload, and this ${e.noun} goes with the next sync.`)
            case "Later":
              if (!shapes.has(answer.why)) say(`The box did not take a post of this tab's writes yet: ${answer.why}`)
              shapes.add(answer.why)
              return
            case "Refused":
              await settled(e, () => e.store.refuse(JSON.stringify(posting.map(o => o.hash)), answer.why))
              break
          }
        }
      }
    }

    const refusedIn = async (asks: ObjectStore): Promise<void> => {
      for (const e of Object.values<Entry>(entries)) await settled(e, () => e.store.refused_in(asks))
    }

    return { sign, handOver, refusedIn, said }
  }
}
export default written
