// Typst in the tab: the Prodrome's typst-wasm, fetched and instantiated once, the first time a Typst view is drawn.

import d from "../wire/decoder"
import once from "../algebra/once"

export namespace compiler {
  // `fetch` ends at the response's headers; `instantiate` is the body's download and its compile, which overlap.
  export type Phases = { readonly fetch: number; readonly instantiate: number; readonly bytes: number }

  export type Compiled =
    | { readonly type: "Html"; readonly html: string }
    | { readonly type: "Failed"; readonly errors: readonly string[] }

  // Every file the compile may read, by the key typst-wasm finds it under (`/state.json`, `/kit.typ`); nothing else exists.
  export type Files = Readonly<Record<string, string>>

  // A world kept between compiles: its files are parsed once, and each compile is handed only its state, read
  // at `/state.json`.
  export type World = { readonly compile: (main: string, state: string) => Compiled }

  export type Typst = {
    readonly world: (files: Files) => World
    readonly phases: Phases
  }

  export const fold = <R>(c: Compiled, on: { html: (html: string) => R; failed: (errors: readonly string[]) => R }): R =>
    c.type === "Html" ? on.html(c.html) : on.failed(c.errors)

  type Diagnostic = { severity: "error" | "warning"; message: string }
  const answer = d.json(
    d.obj<{ html: string | null; diagnostics: Diagnostic[] }>({
      html: d.nullable(d.str),
      diagnostics: d.arr(d.obj<Diagnostic>({ severity: d.oneOf({ error: null, warning: null }), message: d.str })),
    }),
  )

  // The module's answer, read into a value; a warning is not a failure (html export always warns that it is new).
  export const read = (text: string): Compiled => {
    const r = answer(text)
    if (r.type === "Error") return { type: "Failed", errors: [`typst-wasm answered an unexpected shape: ${r.e}`] }
    const errors = r.v.diagnostics.filter(x => x.severity === "error").map(x => x.message)
    return r.v.html !== null && errors.length === 0 ? { type: "Html", html: r.v.html } : { type: "Failed", errors: errors.length === 0 ? ["no html"] : errors }
  }

  type Project = InstanceType<(typeof import("./typst.js"))["Project"]>

  // `files` set once in `p`; a file the module refuses fails every compile, saying why.
  export const world = (p: Project, files: Files): World => {
    const refused = Object.entries(files).flatMap(([k, text]) => [p.set(k, text) ?? []].flat())
    return { compile: (main, state) => (refused.length > 0 ? { type: "Failed", errors: refused } : read(p.compile(main, state))) }
  }

  const ms = (from: number) => performance.now() - from

  const decoded = (url: string): number =>
    performance.getEntriesByName(url).reduce((n, e) => (e instanceof PerformanceResourceTiming ? Math.max(n, e.decodedBodySize) : n), 0)

  // One instance per tab, so every compile shares typst's memo; a failed load is not kept, so a retry retries.
  export const load = once.of(async (): Promise<Typst> => {
    const file = __CONTENT__["typst_bg.wasm"]
    const url = new URL(`./${file}`, import.meta.url).href
    const t0 = performance.now()
    // The glue and the module in one round trip; handed the response, the glue compiles it while it streams
    // (`WebAssembly.instantiateStreaming`, which needs `application/wasm`) and reads it whole only where a browser cannot.
    const [glue, response] = await Promise.all([import("./typst.js"), fetch(url)])
    if (!response.ok) throw new Error(`${file}: HTTP ${response.status} ${response.statusText}`)
    const fetched = ms(t0)
    const t1 = performance.now()
    await glue.default({ module_or_path: response })
    return {
      world: files => world(new glue.Project(), files),
      phases: { fetch: fetched, instantiate: ms(t1), bytes: decoded(url) },
    }
  }).get
}
export default compiler
