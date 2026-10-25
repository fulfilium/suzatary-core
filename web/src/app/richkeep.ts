// A fold's typst HTML kept by the source it renders, so a fold this tab computes offline still reads rendered.

import type { Authored, Entry } from "../wire/generated/wire"
import type rendered from "./rendered"
import blocks from "./blocks"

export namespace richkeep {
  export type Field = "body" | "detail" | "rationale" | "waiting_on"
  export const FIELDS: readonly Field[] = ["body", "detail", "rationale", "waiting_on"]

  export type Want = { at: number; field: Field; k: string }
  export type Effects = { store: rendered.Store }

  const live: Effects = { store: blocks.store }

  // Only what the box rendered is kept, and it rendered only what a trusted actor wrote; each under the key the core
  // named it by, the escaped text a field renders included.
  export const given = (es: readonly Entry[]): (Want & { html: string })[] =>
    es.flatMap((e, at) => {
      const a = e.authored
      return a === null ? [] : FIELDS.flatMap(field => {
        const [html, k] = [a.rich[field], a.rich.keys[field]]
        return html === null || k === null ? [] : [{ at, field, k, html }]
      })
    })

  // And only a confirmed row's fields are looked up: rendered means confirmed, whoever computed the fold.
  export const wanted = (es: readonly Entry[]): Want[] =>
    es.flatMap((e, at) => {
      const a = e.authored
      return a === null || e.unconfirmed ? [] : FIELDS.flatMap(field => {
        const k = a.rich.keys[field]
        return a.rich[field] === null && k !== null ? [{ at, field, k }] : []
      })
    })

  export const filled = (es: readonly Entry[], found: readonly (Want & { html: string })[]): Entry[] =>
    found.length === 0
      ? [...es]
      : es.map((e, at) => {
          const mine = found.filter(f => f.at === at)
          const a: Authored | null = e.authored
          return mine.length === 0 || a === null
            ? e
            : { ...e, authored: { ...a, rich: mine.reduce((r, f) => ({ ...r, [f.field]: f.html }), a.rich) } }
        })

  // The box's pairs joined into this device's: what it already held, again, changes nothing.
  export const keep = async (es: readonly Entry[], fx: Effects = live): Promise<void> =>
    void (await fx.store.join(given(es).map((w): rendered.Pair => [w.k, w.html])))

  export const fill = async (es: readonly Entry[], fx: Effects = live): Promise<Entry[]> => {
    const ws = wanted(es)
    const got = await fx.store.get(ws.map(w => w.k))
    return filled(es, ws.flatMap((w, i) => {
      const html = got[i] ?? null
      return html === null ? [] : [{ ...w, html }]
    }))
  }
}
export default richkeep
