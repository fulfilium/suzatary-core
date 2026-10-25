// What this device keeps: whether the browser may evict it, how much of the render budget is spent, and the key it signs with.

import { el } from "../view/dom"
import rendered from "./rendered"
import blocks from "./blocks"
import once from "../algebra/once"
import type { Standing } from "../prodrome/core"
import pairing from "./pairing"
import cardui from "../inbox/cardui"

export namespace device {
  export type Persist = { type: "Unsupported" } | { type: "Granted" } | { type: "Denied" }
  // Who this device writes as, by its key (SPEC law 42): none before it has read the box's root.
  export type Signing =
    | { type: "Unheard" }
    | { type: "Reads" }
    | { type: "Unsupported"; e: string }
    | { type: "Unreadable"; e: string }
    | { type: "Keyed"; key: string; standing: Standing }
  export type Kept = { persist: Persist; usage: rendered.Usage; budget: number }
  export type Report = Kept & { signing: Signing }

  export const persistOf = (supported: boolean, granted: boolean): Persist =>
    !supported ? { type: "Unsupported" } : granted ? { type: "Granted" } : { type: "Denied" }

  export const mb = (bytes: number) => `${(bytes / (1024 * 1024)).toFixed(1)} MB`

  export const lines = (r: Report): [string, string] => [
    `Rendered text kept here: ${mb(r.usage.bytes)} of ${mb(r.budget)} (${r.usage.count} ${r.usage.count === 1 ? "piece" : "pieces"}).`,
    r.persist.type === "Granted"
      ? "Storage is persistent: the browser will not evict it under pressure."
      : r.persist.type === "Denied"
        ? "Storage is best-effort: the browser may evict it under pressure."
        : "This browser cannot make storage persistent.",
  ]

  export const said = (s: Signing): string =>
    s.type === "Unheard"
      ? "This device has not read the box's root yet, so it signs nothing until it has."
      : s.type === "Reads"
        ? "This login only reads, so this device signs nothing."
        : s.type === "Unsupported"
        ? `This browser cannot sign (${s.e}), so the box refuses what it writes.`
        : s.type === "Unreadable"
        ? `This device cannot sign: ${s.e}, so no other is made over it. A new key, once trusted, writes again.`
        : s.standing === "trusted"
          ? `This device signs with the key ${s.key.slice(0, 12)}…, which the chain trusts.`
          : s.standing === "revoked"
            ? `This device's key ${s.key.slice(0, 12)}… was revoked: nothing it writes is taken. A new key, once trusted, writes again; what the box refused of the old one's stays refused.`
            : `This device's key ${s.key.slice(0, 12)}… is not trusted yet. Trust it with the code that suzatary device pair prints at the box, or ask, then tap the card in the Inbox on a trusted device.`

  // What the section may do: trust an untrusted key with the box's code, file its card, and put a new key in this one's
  // place whatever its standing, since a key thought copied is to be replaced before anyone revokes it.
  export type Acts = { ask: (key: string) => void; renew: () => void } & pairing.Acts


  const renewing = (label: string, acts: Acts): Node => el("button", { type: "button", class: "act act-renewkey", onclick: acts.renew }, label)

  const offered = (s: Signing, acts: Acts): Node[] =>
    s.type === "Unreadable"
      ? [renewing("Use a new key on this device", acts)]
      : s.type !== "Keyed"
      ? []
      : [
          ...(s.standing === "untrusted"
            ? [
                pairing.view(cardui.of(pairing.KEY), acts),
                el("button", { type: "button", class: "act act-asktrust", onclick: () => acts.ask(s.key) }, "Ask to trust this device"),
              ]
            : []),
          renewing(s.standing === "trusted" ? "Use a new key on this device (it must be trusted again)" : "Use a new key on this device", acts),
        ]

  export const view = (r: Report, acts: Acts): Node =>
    el(
      "section",
      { class: "settingsdevice" },
      ...lines(r).map(l => el("p", { class: "settingsmeta" }, l)),
      el("p", { class: "settingsmeta settingskey", "data-key": r.signing.type === "Keyed" ? r.signing.key : "" }, said(r.signing)),
      ...offered(r.signing, acts),
    )

  // Asked once per load: a browser that said no keeps saying no until the site is used more, and asking changes nothing;
  // a browser that failed to answer is asked again.
  export const persist = once.of(async (): Promise<Persist> => {
    const s = typeof navigator === "undefined" ? undefined : navigator.storage
    if (s === undefined || typeof s.persist !== "function") return persistOf(false, false)
    return persistOf(true, (await s.persisted()) || (await s.persist()))
  }).get

  // What this device keeps, read with the box's overview: it moves when the page is read again, not with the replica.
  export const kept = async (store: rendered.Store = blocks.store): Promise<Kept> => ({
    persist: await persist().catch(() => persistOf(false, false)),
    usage: await store.usage(),
    budget: rendered.BUDGET,
  })

  // Who this device writes as, read again whenever the replica, the key or the login moves.
  export const signing = (read: () => Promise<Signing>): Promise<Signing> =>
    read().catch((e: unknown): Signing => ({ type: "Unsupported", e: String(e) }))
}
export default device
