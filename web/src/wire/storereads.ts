// The secretary's stores read from the box, by the frontier a replica holds and by name, and a tab's own objects handed
// to it: plain requests, which the core's worker makes, so this module reads no state of the page.

import type stores from "../prodrome/stores"
import type { ContentPayload, ObjectsPayload, PostedObjects, ReceiptPayload, SkeletonPayload, StorePayload } from "./generated/wire"
import http from "./http"
import wait from "./wait"
import payload from "./generated/payload"

export namespace storereads {
  // Bounded by silence, not length: one object may be larger than a piece, which a phone on a slow network takes longer
  // than `wait.MS` to pull.
  const BOUND: http.Bound = { type: "Silence", ms: wait.MS }
  // `path` asked from the frontier token `since` the box named; none asks for all.
  export const sinceIn = (path: string, since: string): string => (since === "" ? path : `${path}?since=${encodeURIComponent(since)}`)

  // A piece of what a replica of the inbox or the topics holding `since` lacks, the keys of the words it sends, and how
  // much is left.
  export const objects = (store: string, since: string): Promise<StorePayload> =>
    http.get(sinceIn(`/api/stores/${encodeURIComponent(store)}/objects`, since), payload.StorePayload, BOUND)

  // A piece of the lines' skeleton a replica holding `since` lacks: each object without its literal.
  // It carries the keys of the lines' words, so it is never kept beside the reads (`http.Keeping`).
  export const skeleton = (since: string): Promise<SkeletonPayload> => http.get(sinceIn("/api/stores/lines/skeleton", since), payload.SkeletonPayload, BOUND, "Never")

  // The content of `hashes`, ascending with none twice, so one set has one URL. Never kept, by the browser or beside
  // the reads: it can hold the words of a line an erasure later lets go (then answered as its husk).
  export const content = (store: string, hashes: readonly string[]): Promise<ContentPayload> =>
    http.get(`/api/stores/${encodeURIComponent(store)}/content?hashes=${hashes.join(",")}`, payload.ContentPayload, BOUND, "Never")

  // What a replica of the todo chain `chain` holding `since` lacks: one answer, nothing left behind it.
  export const chain = (chain: string, since: string): Promise<ObjectsPayload> =>
    http.get(sinceIn(`/api/${encodeURIComponent(chain)}/objects`, since), payload.ObjectsPayload, BOUND)

  // What a replica of the todo chain `chain` holding `since` lacks on the box, taken as the actor the request was proven
  // for.
  export const chainPost = (chain: string, objects: readonly string[], since: string): Promise<ReceiptPayload> =>
    http.handOver(`/api/${encodeURIComponent(chain)}/objects`, { objects: [...objects], since, keys: {} } satisfies PostedObjects, payload.ReceiptPayload)

  // What a replica of a store a tab writes holding the frontier `since` lacks on the box: the box takes each as the actor
  // the request was proven for. `keys` travel beside the objects, never in one: each slot's key by its line's name, the
  // lines' alone.
  export const post = (store: stores.Written, objects: readonly string[], since: string, keys: Readonly<Record<string, readonly string[]>> = {}): Promise<ReceiptPayload> =>
    http.handOver(
      `/api/stores/${store}/objects`,
      { objects: [...objects], since, keys: Object.fromEntries(Object.entries(keys).map(([name, k]) => [name, [...k]])) } satisfies PostedObjects,
      payload.ReceiptPayload,
    )
}
export default storereads
