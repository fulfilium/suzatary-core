// A device that has not heard the box for `DAYS` deletes what it holds, as a revoked one does, once its page runs.

import kv from "./kv"

export namespace lease {
  export const KEY = "suzatary.heard"
  export const DAYS = 60
  export const MS = DAYS * 24 * 60 * 60 * 1000
  export const EVERY_MS = 60 * 60 * 1000

  export type Heard = { type: "Never" } | { type: "At"; at: number }

  export const read = (raw: string | null): Heard => {
    const at = raw === null ? Number.NaN : Number(raw)
    return Number.isFinite(at) ? { type: "At", at } : { type: "Never" }
  }

  export const lapsed = (heard: Heard, now: number): boolean => heard.type === "At" && now - heard.at > MS

  // `owed` counts what the device holds that the box has not taken: acts sealed offline are the owner's own words, so
  // the wipe waits until the box takes every one.
  export const wipes = (heard: Heard, now: number, owed: number): boolean => owed === 0 && lapsed(heard, now)

  export const due = (heard: Heard, now: number): boolean => heard.type === "Never" || now < heard.at || now - heard.at >= EVERY_MS

  export const heard = (store: kv.Kv = kv.local, now: number = Date.now()): void => {
    if (due(read(store.get(KEY)), now)) store.set(KEY, String(now))
  }
}
export default lease
