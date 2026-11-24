// The loader's decisions: which builds to try, in which order, and what each is asked for beside its app.

export namespace loader {
  export type Effects = {
    // The build the box serves, as the worker last heard it; null when nothing answered.
    readonly served: () => Promise<string | null>
    // The build of the worker serving this page, which is the build of this loader; null with no worker.
    readonly worker: () => string | null
    // Asks for build `b`'s stylesheets, its app and `chunks` of it beside each other, before any of them is needed.
    readonly preload: (b: string, chunks: readonly string[]) => void
    readonly load: (b: string) => Promise<boolean>
    readonly said: (text: string) => void
  }

  export type Started = { readonly type: "Ran"; readonly build: string } | { readonly type: "Refused"; readonly said: string }

  export const NAMELESS = "The box did not say which version it serves. Reload to ask again."
  export const UNLOADED = "This version of the app did not load. Reload to ask the box again."

  // The box's build first, then the worker's, each once. `chunks` are named by this loader's build (the worker's when
  // one serves the page, else the box's), so another build is asked for its stylesheets and app alone.
  export const start = async (fx: Effects, chunks: readonly string[]): Promise<Started> => {
    const served = await fx.served().catch(() => null)
    const worker = fx.worker()
    const own = worker ?? served
    const tries = [served, worker].filter((b, i, all): b is string => b !== null && all.indexOf(b) === i)
    const refused = (said: string): Started => (fx.said(said), { type: "Refused", said })
    if (tries.length === 0) return refused(NAMELESS)
    for (const b of tries) {
      fx.preload(b, b === own ? chunks : [])
      if (await fx.load(b)) return { type: "Ran", build: b }
    }
    return refused(UNLOADED)
  }
}
export default loader
