// How many bytes of the lines' content this device keeps: the core plans the newest conversations, each whole, within
// it (`Lines.plan`), a prefetch fetches what the plan names and the core lacks, and an eviction drops what the device
// holds outside the plan and the prints the core holds.

export namespace keep {
  // The origin's quota as `navigator.storage.estimate()` says it, or nothing where the browser does not say.
  export type Quota = { readonly type: "Unknown" } | { readonly type: "Known"; readonly bytes: number }

  // The live store is about 54 MB, so 256 MiB holds it whole with room to grow, and a tenth of the quota leaves the
  // rest of the origin's storage to the chains, the rendered blocks and the shell.
  export const MOST = 256 * 1024 * 1024
  export const UNKNOWN = 64 * 1024 * 1024

  export const budget = (q: Quota): number => (q.type === "Unknown" ? UNKNOWN : Math.min(MOST, Math.floor(q.bytes / 10)))
}
export default keep
