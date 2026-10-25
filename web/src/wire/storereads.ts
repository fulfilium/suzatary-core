// The secretary's stores read from the box, by the frontier a replica holds and by name: plain reads, which the page
// and the core's worker both make, so this module reads no state of the page.

import type { ContentPayload, SkeletonPayload, StorePayload } from "./generated/wire"
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

  // The literals of `hashes`, in ascending order with none twice, so one set has one URL the browser keeps for good.
  export const content = (store: string, hashes: readonly string[]): Promise<ContentPayload> =>
    http.get(`/api/stores/${encodeURIComponent(store)}/content?hashes=${hashes.join(",")}`, payload.ContentPayload, BOUND)
}
export default storereads
