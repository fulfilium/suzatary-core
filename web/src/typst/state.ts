// What a Typst view reads: one declared value, printed into the compile's `/state.json`. Every text in it is a
// `Text`: prose, which the kit prints verbatim, or a trusted actor's confirmed markup, which the kit evaluates.

import type { Entry } from "../wire/generated/wire"
import fulfillment from "../todo/fulfillment"
import { print } from "../chat/refs"
import { DEFAULT_FILTER, applicable, finish, matches, preview } from "../todo/todos"

export namespace viewstate {
  // The schema a view is written against; a field is added under the same name, a change of meaning is a new name.
  export const SCHEMA = "suzatary.view/1"

  // Rendered means confirmed: `Markup` is only ever a source the box itself rendered for this field
  // (`view::renders`, a trusted actor's), so an untrusted or unrendered text cannot become one.
  export type Text =
    | { readonly type: "Prose"; readonly text: string }
    | { readonly type: "Markup"; readonly source: string }

  export const prose = (text: string): Text => ({ type: "Prose", text })

  export const fold = <R>(t: Text, on: { readonly Prose: (text: string) => R; readonly Markup: (source: string) => R }): R =>
    t.type === "Prose" ? on.Prose(t.text) : on.Markup(t.source)

  export type Row = {
    readonly ref: string
    readonly title: Text
    readonly category: string
    readonly value: number | null
    readonly unconfirmed: boolean
    // Whether the complete intent applies now: open, and this login writes.
    readonly complete: boolean
  }

  export type Opened =
    | { readonly type: "None" }
    | {
        readonly type: "Todo"
        readonly row: Row
        readonly created: string
        readonly detail: Text
        readonly why: readonly Text[]
      }

  export type State = {
    readonly schema: typeof SCHEMA
    readonly at: string
    readonly chain: string
    readonly rows: readonly Row[]
    readonly open: Opened
  }

  export type Held = {
    readonly entries: readonly Entry[]
    readonly at: string
    readonly chain: string
    readonly open: string
    readonly writes: boolean
  }

  // The core's reading of escaped markup back into prose (`unescape_markup`); markup a person wrote stays its source.
  export type Reads = { readonly prose: (markup: string) => string | undefined }

  const plain = (r: Reads, markup: string): string => preview(r.prose(markup) ?? markup)

  // Escaped prose reads back into itself; a person's markup is markup only where the box rendered it (`rich`
  // non-null), kept byte for byte, since collapsing its whitespace could change what it means.
  const text = (r: Reads, markup: string, rendered: string | null): Text => {
    const p = r.prose(markup)
    return p === undefined && rendered !== null ? { type: "Markup", source: markup } : prose(preview(p ?? markup))
  }

  const row = (r: Reads, h: Held, e: Entry): Row => {
    const body = e.authored?.body ?? ""
    return {
      ref: print({ kind: "todo", chain: h.chain, id: e.todo }),
      title: preview(body) === "" ? prose(e.todo) : text(r, body, e.authored?.rich.body ?? null),
      category: e.authored?.category ?? "",
      // The store holds the fold as the screen observes it, a whole percent, so a row's data, and typst's memo of
      // it, move only where the core said the percent would.
      value: fulfillment.number(e.value),
      unconfirmed: e.unconfirmed,
      complete: h.writes && applicable(e).includes(finish(e)),
    }
  }

  const opened = (r: Reads, h: Held, e: Entry | undefined): Opened =>
    e === undefined
      ? { type: "None" }
      : {
          type: "Todo",
          row: row(r, h, e),
          created: e.authored?.created ?? "",
          detail: text(r, e.authored?.detail ?? "", e.authored?.rich.detail ?? null),
          why: (e.authored?.rationale ?? []).map(l => plain(r, l)).filter(l => l !== "").map(prose),
        }

  // The rows today's list shows by default, in the fold's order; the order on screen is the view's to choose.
  export const of = (r: Reads, h: Held): State => ({
    schema: SCHEMA,
    at: h.at,
    chain: h.chain,
    rows: h.entries.filter(e => matches(e, DEFAULT_FILTER, h.at)).map(e => row(r, h, e)),
    open: opened(r, h, h.open === "" ? undefined : h.entries.find(e => e.todo === h.open)),
  })

  export const printed = (s: State): string => JSON.stringify(s)
}
export default viewstate
