// What this tab wrote, handed over by one engine for every store it writes: each change signed by the key only the page
// mints, posted in pieces the box takes whole, each answer kept, and each refusal for good said once in the box's words.

import type { Core, LineStep, Written } from "./core"
import { CoreError, signedObject } from "./core"
import signer from "./signer"
import clock from "../algebra/clock"
import lo from "../algebra/lo"
import d from "../wire/decoder"
import http from "../wire/http"
import type { ReceiptPayload } from "../wire/generated/wire"
import ownlines from "./ownlines"

export namespace written {
  // A store this tab writes: the noun a refusal names, its core class, how a step of it is kept, where its writes are
  // posted (each slot's key by its line's name travels beside them), whether its writes are signed (a chain's only under
  // the root the box names), and, for a store whose writes hold the person's words, where the device keeps which
  // refusals it told them of.
  export type Entry = {
    readonly noun: string
    readonly store: Written
    // `f` alone among the store's steps: a hand-over, a signing and a settling each read the store and write it in one turn.
    readonly exclusive: <R>(f: () => Promise<R>) => Promise<R>
    // Called only inside `exclusive`.
    readonly stepped: (step: LineStep) => Promise<void>
    readonly post: (literals: readonly string[], since: string, keys: Readonly<Record<string, readonly string[]>>) => Promise<ReceiptPayload>
    // Whether its writes are signed; `Unknown` until the box has said, when nothing of it is posted.
    readonly signs: () => Signing
    readonly told?: { readonly seen: (literals: readonly string[]) => Promise<void>; readonly told: () => Promise<ReadonlySet<string>> }
  }

  export type Signing = "Signs" | "Unsigned" | "Unknown"

  // A store of objects: the changes events make, unkept, and the frame writing literals as the tab's own.
  export type Appends = { readonly append: (events: string) => string; readonly writing: (literals: string) => Uint8Array }

  export type T<N extends string> = {
    // The signatures of every unsigned change of `name` the core holds, as moves the caller keeps in its own write.
    readonly signatures: (name: N) => Promise<Signed<LineStep>>
    // `events` written into `name` as this tab's own and kept in one frame with their signatures, so no reading holds
    // them unsigned; throws, nothing held, where the frame was not kept.
    readonly own: (name: N, store: Appends, events: readonly string[], kept: (frame: Uint8Array) => Promise<void>) => Promise<Signed<string>>
    // What `name` holds unsent, signed and posted; an answer kept, a refusal said.
    readonly handOver: (name: N) => Promise<void>
    // A move of `name` the caller made, kept; each newly refused write said once, in the box's words.
    readonly settled: (name: N, step: () => LineStep | undefined) => Promise<void>
    // Each refusal of `name` the person was not told of yet, said with their words: what a load found.
    readonly said: (name: N) => Promise<void>
  }

  // A signature that failed stops the signing: those `made` before are kept, and the hand-over signs the rest.
  export type Signed<A> = { readonly type: "Signed"; readonly made: readonly A[] } | { readonly type: "Partly"; readonly made: readonly A[]; readonly why: string }

  // In posts the box takes whole, each change beside its signatures, and each line beside its slots' keys, which
  // travel beside the objects and never in one.
  type Posted = { hash: string; literal: string; keys: readonly string[] }
  const Unsent = d.json(d.arr(d.arr(d.obj<Posted>({ hash: d.str, literal: d.str, keys: d.arr(d.str) }))))
  const Names = d.json(d.arr(d.str))
  const Changes = d.json(d.arr(d.obj<{ hash: string; literal: string }>({ hash: d.str, literal: d.str })))
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
  type Waits = { hash: string; reason: string; literal: string | null; words: string | null; at: string | null; signature: boolean }
  const Waiting = d.json(
    d.arr(d.obj<Waits>({ hash: d.str, reason: d.str, literal: d.nullable(d.str), words: d.nullable(d.str), at: d.nullable(d.str), signature: d.bool })),
  )
  // When a line dated ahead may be posted again, on this device's clock: once the box's clock reaches the line's own
  // instant, which is past the rule's bound for it, by the skew the last answer said. A line dated in the box's past
  // waits for something else (its parent, its signer's trust), so no instant moves it.
  export const due = (waiting: readonly Waits[], ahead: number | null, now: number): clock.Next =>
    clock.sooner(
      ...waiting.map(w => {
        const at = w.at === null ? NaN : Date.parse(w.at)
        const on = at + (ahead ?? 0) * 1000
        return Number.isNaN(at) || on <= now ? clock.never : clock.at(on)
      }),
    )
  // The worker has no window: its own timers.
  const TIMERS: clock.Timers = { now: () => Date.now(), set: (k, ms) => Number(setTimeout(k, ms)), clear: h => clearTimeout(h) }
  const waitingIn = (store: Written): ReadonlyMap<string, Waits> => new Map(read("waiting", Waiting(store.waiting())).map(w => [w.hash, w]))
  const waits = (noun: string, w: Waits): string =>
    `The box has not taken a ${noun} yet: ${w.reason}. It goes again with the next sync${w.words === null || w.words === "" ? "" : `. What you wrote: “${w.words}”`}`
  // A refusal in the box's words, and the person's own words where the write held some, to copy.
  const sentence = (noun: string, r: Refused): string =>
    `The box refused a ${noun}: ${r.reason}${r.words === null || r.words === "" ? "" : `. What you wrote: “${r.words}”`}`

  // What an answer that is no receipt means for the post it answers, one total table over its status:
  // - no answer, or a 5xx: Unanswered. The post goes with the next sync, unsaid.
  // - 426: Outdated. A newer box; reload.
  // - 413, 422: Refused. A function of the post alone: this build's box answers a body over the cap 413 and every other
  //   fault of a post 422 (`Literals::read`), so posting again gets the same answer: its lines are refused for good, as
  //   the rule refuses one, and said with their words, and the pieces behind it go.
  // - every other status: Later. Who posts or how often (401, 403, 429), or a box of another build: its own 409, an older
  //   box's 404 for a store it does not keep, or its 400 for a body of another shape, which this build's box never
  //   answers. None is a verdict on the post, so its lines stay unsent. A 200 whose receipt does not decode is no
  //   `ApiError`, so it reads as no answer: the post goes again, which is safe whether or not the box took it.
  export type Failed =
    | { readonly type: "Unanswered" }
    | { readonly type: "Outdated" }
    | { readonly type: "Later"; readonly why: string }
    | { readonly type: "Refused"; readonly why: string }
  const REFUSED: ReadonlySet<number> = new Set([413, 422])
  export const failed = (x: unknown): Failed => {
    if (http.unanswered(x)) return { type: "Unanswered" }
    if (x instanceof http.ApiError && x.status === http.OUTDATED) return { type: "Outdated" }
    const why = lo.reason(x)
    return x instanceof http.ApiError && REFUSED.has(x.status) ? { type: "Refused", why } : { type: "Later", why }
  }

  export const of = <N extends string>(core: Core, entry: (name: N) => Entry, say: (why: string) => void, timers: clock.Timers = TIMERS): T<N> => {
    // One wake per store, armed again after each hand-over and load: its lines dated ahead are posted again when the box
    // may take them.
    const wakes = new Map<N, (n: clock.Next, k: () => void) => void>()
    const retried = (name: N): void => {
      const wake = wakes.get(name) ?? clock.pacer(timers)
      wakes.set(name, wake)
      wake(due([...waitingIn(entry(name).store).values()], http.ahead(), timers.now()), () => void handOver(name).catch(() => {}))
    }

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
      retried(name)
    }

    // Each signature of `changes` made into what the caller keeps: none where the store's writes are unsigned or the
    // device keeps no key. TOTAL: a throw at any step (reading what to sign, signing, making) answers what was made
    // before it, so a caller's keep always follows.
    const signing = async <A>(name: N, changes: () => readonly string[], made: (literal: string) => A): Promise<Signed<A>> => {
      const done: A[] = []
      if (entry(name).signs() !== "Signs") return { type: "Signed", made: done }
      try {
        const hashes = changes()
        const key = hashes.length === 0 ? null : await signer.existing()
        if (key !== null) for (const hash of hashes) done.push(made(signedObject(core, hash, key.public, await signer.sign(key, hash)).literal))
        return { type: "Signed", made: done }
      } catch (x: unknown) {
        return { type: "Partly", made: done, why: lo.reason(x) }
      }
    }
    const signatures = (name: N): Promise<Signed<LineStep>> => {
      const e = entry(name)
      return signing(name, () => read("unsigned", Names(e.store.unsigned())), literal => e.store.signed(literal))
    }
    // Every move made is kept, a signature that failed after them too; the failure is then thrown.
    const signIn = async (name: N): Promise<void> => {
      const signed = await signatures(name)
      for (const step of signed.made) await entry(name).stepped(step)
      if (signed.type === "Partly") throw new Error(signed.why)
    }
    const own = async (name: N, store: Appends, events: readonly string[], kept: (frame: Uint8Array) => Promise<void>): Promise<Signed<string>> => {
      const changes = read("changes", Changes(store.append(JSON.stringify(events))))
      if (changes.length === 0) return { type: "Signed", made: [] }
      const signed = await signing(name, () => changes.map(c => c.hash), literal => literal)
      await kept(store.writing(JSON.stringify([...changes.map(c => c.literal), ...signed.made])))
      return signed
    }

    // Each piece posted in turn, its receipt kept. An answer that is no receipt is read by `failed`: one for later stops
    // the hand-over, said once per tab; a refusal of the post refuses its lines for good, said with their words as a
    // rule's refusal is, and the hand-over goes on to the next piece, so one post can never hold back the rest.
    //
    // NO TURN IS HELD ACROSS A REQUEST: what is unsent is signed and read in one of the store's turns, each post goes
    // outside every turn, and each answer is settled in a turn of its own, so an act, a reset or a compaction never waits
    // on the network. The hand-overs of one store run one after another on a lane of their own; a receipt for a write
    // a reset dropped meanwhile names nothing the store holds and is passed over.
    const shapes = new Set<string>()
    const lanes = new Map<N, Promise<void>>()
    const handOver = (name: N): Promise<void> => {
      const next = (lanes.get(name) ?? Promise.resolve()).then(() => posted(name)).finally(() => retried(name))
      lanes.set(name, next.catch(() => {}))
      return next
    }
    // Nothing is posted before the box has said whether the store's writes are signed: one posted unsigned under a root
    // it had not named yet would be refused.
    const posted = async (name: N): Promise<void> => {
      const e = entry(name)
      if (e.signs() === "Unknown") return
      const postings = await e.exclusive(async () => {
        await signIn(name)
        return read("unsent", Unsent(e.store.unsent()))
      })
      for (const posting of postings) {
        try {
          const keys = Object.fromEntries(posting.filter(o => o.keys.length > 0).map(o => [o.hash, o.keys]))
          const receipt = await e.post(
            posting.map(o => o.literal),
            e.store.since(),
            keys,
          )
          await e.exclusive(() => settled(e, () => e.store.answered(JSON.stringify(receipt))))
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
              await e.exclusive(() => settled(e, () => e.store.refuse(JSON.stringify(posting.map(o => o.hash)), answer.why)))
              break
          }
        }
      }
    }

    return { signatures, own, handOver, settled: (name, step) => entry(name).exclusive(() => settled(entry(name), step)), said }
  }
}
export default written
