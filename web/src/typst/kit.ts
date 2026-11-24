// The kit and a view, fetched from files beside the module: a view is a Typst file named here, so a page is data.

import shell from "../app/shell"
import d from "../wire/decoder"
import lo from "../algebra/lo"
import type memo from "../algebra/memo"
import type compiler from "./compiler"

export namespace kit {
  export type Module = { readonly main: string; readonly files: compiler.Files }

  export type View = "list.typ"

  const text = (file: string): Promise<string> =>
    fetch(new URL(`./typst/${file}`, import.meta.url).href).then(r => (r.ok ? r.text() : Promise.reject(new Error(`${file}: HTTP ${r.status}`))))

  // Every package file, already keyed the way typst-wasm finds it (`@local/typst-design:0.1.0/lib.typ`).
  const packages = d.json(d.rec(d.str))

  const library = (): Promise<compiler.Files> =>
    Promise.all([
      ...shell.TYPST.map(f => text(f).then((t): compiler.Files => ({ [`/${f}`]: t }))),
      text(shell.PACKAGES).then(t =>
        lo.fold<compiler.Files, Promise<compiler.Files>>(packages(t), {
          loading: () => Promise.reject(new Error(`${shell.PACKAGES}: nothing`)),
          error: e => Promise.reject(new Error(`${shell.PACKAGES}: ${e}`)),
          val: files => Promise.resolve(files),
        }),
      ),
    ]).then((fs): compiler.Files => Object.assign({}, ...fs))

  const loaded = new Map<View, Promise<Module>>()

  export const load = (view: View): Promise<Module> => {
    const known = loaded.get(view)
    if (known !== undefined) return known
    const asked = library().then(files => ({ main: `/${view}`, files }))
    asked.catch(() => loaded.delete(view))
    loaded.set(view, asked)
    return asked
  }

  // The module's name: its main and every file's key and text, so a cache entry drawn by one kit is never read by another.
  export const name = (m: Module, named: memo.Namer): memo.Name =>
    named(JSON.stringify([m.main, Object.entries(m.files).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))]), [])
}
export default kit
