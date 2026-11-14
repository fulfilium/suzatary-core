// How far each store's sync has come, in bytes: a pull of pieces, whose first piece says how many bytes the whole pull
// brings, and the content fetched by name, whose sizes the skeleton gives. What the bar under the header draws.

import type stores from "./stores"

export namespace progress {
  // A pull: nothing asked, or the bytes it brings and those still lacking after its latest piece.
  export type Pull = { readonly type: "Idle" } | { readonly type: "Pulling"; readonly needed: number; readonly lacking: number }
  // Content asked for by name: nothing out, or the bytes asked for and those that came.
  export type Fetch = { readonly type: "Idle" } | { readonly type: "Fetching"; readonly needed: number; readonly done: number }
  export type One = { readonly pull: Pull; readonly fetch: Fetch }
  export type T = { readonly [N in stores.Name]: One }

  export type Event =
    // A piece came: `size` bytes were lacking when it was asked for, and it brought `weight` of them.
    | { readonly type: "Piece"; readonly name: stores.Name; readonly size: number; readonly weight: number }
    // The pull ended, whole or not.
    | { readonly type: "Pulled"; readonly name: stores.Name }
    // A fetch will ask for `bytes`; a batch of `bytes` came, or was lost.
    | { readonly type: "Asked"; readonly name: stores.Name; readonly bytes: number }
    | { readonly type: "Came"; readonly name: stores.Name; readonly bytes: number }
    | { readonly type: "Lost"; readonly name: stores.Name; readonly bytes: number }

  const IDLE: One = { pull: { type: "Idle" }, fetch: { type: "Idle" } }
  export const NONE: T = { inbox: IDLE, lines: IDLE, topics: IDLE, "topic-aliases": IDLE, asks: IDLE }

  // The first piece's size is the pull's; a later piece that says more lacking than was left grew the pull by as much.
  const pulled = (p: Pull, size: number, weight: number): Pull =>
    p.type === "Idle"
      ? size === 0
        ? p
        : { type: "Pulling", needed: size, lacking: size - weight }
      : { type: "Pulling", needed: p.needed + Math.max(0, size - p.lacking), lacking: size - weight }

  // Settled once every byte asked for came or was lost.
  const fetched = (needed: number, done: number): Fetch => (done >= needed ? { type: "Idle" } : { type: "Fetching", needed, done })

  const stepOne = (o: One, e: Event): One => {
    switch (e.type) {
      case "Piece":
        return { ...o, pull: pulled(o.pull, e.size, e.weight) }
      case "Pulled":
        return { ...o, pull: { type: "Idle" } }
      case "Asked":
        return { ...o, fetch: o.fetch.type === "Idle" ? fetched(e.bytes, 0) : fetched(o.fetch.needed + e.bytes, o.fetch.done) }
      case "Came":
        return { ...o, fetch: o.fetch.type === "Idle" ? o.fetch : fetched(o.fetch.needed, o.fetch.done + e.bytes) }
      case "Lost":
        return { ...o, fetch: o.fetch.type === "Idle" ? o.fetch : fetched(o.fetch.needed - e.bytes, o.fetch.done) }
    }
  }

  export const step = (t: T, e: Event): T => ({ ...t, [e.name]: stepOne(t[e.name], e) })

  // The bytes come and the bytes needed, or nothing while the store is not syncing.
  export type Bar = { readonly type: "Hidden" } | { readonly type: "Shown"; readonly done: number; readonly needed: number }

  export const bar = (o: One): Bar => {
    const pull = o.pull.type === "Idle" ? { done: 0, needed: 0 } : { done: o.pull.needed - o.pull.lacking, needed: o.pull.needed }
    const fetch = o.fetch.type === "Idle" ? { done: 0, needed: 0 } : { done: o.fetch.done, needed: o.fetch.needed }
    return o.pull.type === "Idle" && o.fetch.type === "Idle" ? { type: "Hidden" } : { type: "Shown", done: pull.done + fetch.done, needed: pull.needed + fetch.needed }
  }

  export const LABEL: { readonly [N in stores.Name]: string } = { inbox: "the inbox", lines: "conversations", topics: "topics", "topic-aliases": "topic names", asks: "refusals" }

  // Megabytes to one decimal from a tenth of one, else kilobytes.
  const amount = (done: number, needed: number): string =>
    needed >= 100_000 ? `${(done / 1e6).toFixed(1)} of ${(needed / 1e6).toFixed(1)} MB` : `${Math.round(done / 1e3)} of ${Math.max(1, Math.round(needed / 1e3))} KB`

  export const said = (name: stores.Name, done: number, needed: number): string => `Syncing ${LABEL[name]} · ${amount(done, needed)}`
}
export default progress
