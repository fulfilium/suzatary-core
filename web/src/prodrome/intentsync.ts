// The intents' effectful shell, in the in-memory tier beside the replicas: an ask goes into the set, then to the
// box or the tab's replica, its answer is folded in, and the next reading of the replicas retracts what it holds.

import asks from "./asks"
import intents from "./intents"
import storesync from "./storesync"
import { appendAct } from "./sync"
import http from "../wire/http"
import sources from "../algebra/source"
import beacon from "../app/beacon"
import lo from "../algebra/lo"
import type { ConversationPayload } from "../wire/generated/wire"

export namespace intentsync {
  export type T = intents.T<asks.Ask>
  export type Outcome = { readonly type: "Already" } | { readonly type: "Answered"; readonly answer: intents.Answer; readonly held: boolean }

  const source = sources.create<T, intents.Event<asks.Ask>>(intents.EMPTY, intents.step)

  export const held = (): T => source.state()

  // Wakes on every change of the set; returns its own removal.
  export const watch = (listener: () => void): (() => void) => source.subscribe(() => listener())

  const TAKEN: intents.Answer = { type: "Taken" }
  // A reading that refused is told to the box, never dropped; the ask it would have retracted stays in view.
  const told = (e: unknown): void => beacon.tell("rejection", e)

  // An answer the box decided is a refusal in its own words; one it may not have decided may have reached it or not.
  const answerOf = (a: asks.Ask) => (e: unknown): intents.Answer =>
    a.type === "Seal" || a.type === "Topic" || !http.unanswered(e)
      ? { type: "Refused", e: lo.reason(e) }
      : { type: "Lost", e: lo.reason(e) }

  const retract = (holds: (a: asks.Ask) => boolean): void => {
    const t = source.state()
    for (const [key, v] of t.asked) if (intents.isIn(t, key) && holds(v.act)) source.dispatch({ type: "Subsumed", key })
  }

  // Read each conversation an open ask lands in, once, and retract every ask the reading holds.
  export const settle = async (): Promise<void> => {
    const ids = [...new Set(intents.pending(source.state()).map(asks.conversation).filter((c): c is string => c !== null))]
    if (ids.length === 0) return
    // A conversation not yet held is absent from the store, never a caught refusal.
    const read = new Map(
      await Promise.all(ids.map(id => storesync.conversation(id, {}, null).then((one): readonly [string, ConversationPayload | null] => [id, one.type === "Held" ? one.payload : null]))),
    )
    retract(asks.holds(id => read.get(id) ?? null))
  }

  // One ask: nothing if it is already asked; otherwise sent, answered (told to `heard`), and read back. A seal or a topic
  // is held once the tab's replica has it, and a forgotten fact by the box's answer, since no replica carries the
  // memory's words.
  export const ask = async (a: asks.Ask, secret?: string, heard: (answer: intents.Answer) => void = () => {}): Promise<Outcome> => {
    const key = intents.key(a)
    if (!intents.wants(source.state(), key)) return { type: "Already" }
    source.dispatch({ type: "Asked", key, act: a })
    const asked = source.state().asked.get(key)?.asks ?? 0
    const answer = await (a.type === "Seal" ? appendAct(a.event) : a.type === "Topic" ? storesync.write(a.write, a.at) : asks.send(a, secret)).then(() => TAKEN, answerOf(a))
    source.dispatch({ type: "Answered", key, asks: asked, answer })
    heard(answer)
    if (answer.type === "Taken" && (a.type === "Seal" || a.type === "Topic" || a.type === "Forget")) source.dispatch({ type: "Subsumed", key })
    else if (answer.type === "Taken") {
      // A pull asked after the box's answer holds what it wrote, whatever the record's words became on the way in; a pull
      // that did not reach the box leaves the ask in view, and the next sync retracts it.
      if (await storesync.asked().then(() => true, () => false)) await settle().then(() => retract(x => intents.key(x) === key), told)
    } else if (answer.type === "Lost") await storesync.sync().then(settle).catch(told)
    return { type: "Answered", answer, held: !intents.isIn(source.state(), key) }
  }

  // An ask for a caller that says a failure itself: rejects with the box's words, or the network's.
  export const asked = (a: asks.Ask): Promise<void> =>
    ask(a).then(o => {
      if (o.type === "Answered" && o.answer.type !== "Taken") throw new Error(o.answer.e)
    })

  // The same, settled by the box's answer, while the read-back runs behind it: what a field closes on, since a pull of
  // every store is minutes on a device taking its lines again. The ask stays in the set until a reading holds it.
  export const answered = (a: asks.Ask): Promise<void> =>
    new Promise((taken, refused) => {
      const heard = (answer: intents.Answer): void => (answer.type === "Taken" ? taken() : refused(new Error(answer.e)))
      ask(a, undefined, heard).then(o => (o.type === "Already" ? taken() : undefined), refused)
    })

  // Every move of the replicas may hold an open ask.
  export const listen = (): (() => void) => storesync.watch(() => void settle().catch(told))
}
export default intentsync
