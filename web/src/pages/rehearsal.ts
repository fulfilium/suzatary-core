// The page's side of the rehearsal worker (`rehearse.ts`, bundled as `pagework.js`), and what the two say, in order:
// the zone the device holds as the load starts, then the module; that zone's offsets, then that the rehearsal is over.

import type time from "../app/time"

export namespace rehearsal {
  export type Said = { readonly type: "Zone"; readonly zone: string; readonly now: number } | { readonly type: "Module"; readonly module: WebAssembly.Module }
  export type Told = { readonly type: "Offsets"; readonly zone: string; readonly offsets: readonly time.Offset[] } | { readonly type: "Rehearsed" }

  // Ten years back to a day ahead: an instant from older mail is read at the oldest offset.
  export const span = (now: number): readonly [number, number] => [now - 10 * 365 * 86_400_000, now + 86_400_000]

  // What of a `Worker` this uses, so a test can stand in for one.
  export type Port = {
    readonly postMessage: (said: Said) => void
    readonly addEventListener: (type: "message" | "error", f: (e: MessageEvent<Told>) => void) => void
    readonly terminate: () => void
  }

  export type T = {
    // The zone's offsets, held, or the worker failed.
    readonly offsets: Promise<void>
    readonly rehearse: (m: WebAssembly.Module) => Promise<void>
    // `load`'s answer; a load that fails ends the worker, which would otherwise wait for a module for the tab's life.
    readonly loading: <A>(load: () => Promise<A>) => Promise<A>
  }

  // `held` keeps the zone's offsets when they come. A worker that fails settles every wait: it costs only warmth.
  export const start = (port: Port, z: time.Zone, now: number, held: (zone: string, offsets: readonly time.Offset[]) => void): T => {
    const waits = new Map<Told["type"], () => void>()
    const waited = (type: Told["type"]): Promise<void> => new Promise(done => waits.set(type, done))
    port.addEventListener("message", e => {
      const t = e.data
      if (t.type === "Offsets") held(t.zone, t.offsets)
      waits.get(t.type)?.()
    })
    port.addEventListener("error", () => waits.forEach(done => done()))
    const offsets = waited("Offsets")
    const rehearsed = waited("Rehearsed")
    port.postMessage({ type: "Zone", zone: z.name, now })
    const loading = async <A>(load: () => Promise<A>): Promise<A> => {
      try {
        return await load()
      } catch (e) {
        port.terminate()
        throw e
      }
    }
    return { offsets, rehearse: m => (port.postMessage({ type: "Module", module: m }), rehearsed), loading }
  }
}
export default rehearsal
