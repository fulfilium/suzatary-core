// Where an anchor typst emitted may lead, read as the browser reads it (`URL`, after it strips tabs, newlines and C0):
// the app's own route for `todo:`, an outbound URL of a scheme that only navigates, or nowhere. An outbound anchor is
// drawn by the host it leads to, with the writer's label beside it as plain text, so no label can pose as a destination.

export namespace href {
  /** `shown` is where it leads: a web address's host (never a user name before an `@`), a mail or phone address whole. */
  export type Outbound = { readonly type: "Outbound"; readonly href: string; readonly shown: string; readonly address: string }

  export type Href =
    | { readonly type: "Todo"; readonly id: string }
    | Outbound
    | { readonly type: "Inert" }

  /** The schemes an outbound link may name: each navigates and none runs anything. */
  export const OUTBOUND: ReadonlySet<string> = new Set(["http:", "https:", "mailto:", "tel:"])

  const parsed = (raw: string, base: string): URL | null => {
    try {
      return new URL(raw, base)
    } catch {
      return null
    }
  }

  const web = (u: URL): boolean => u.protocol === "http:" || u.protocol === "https:"

  /** An outbound href is kept as written, since the browser reads it as `URL` just did; a bare `todo:` names nothing
   * and stays inert, as every scheme outside `OUTBOUND` does. */
  export const of = (raw: string, base: string): Href => {
    const u = parsed(raw, base)
    return u === null ? { type: "Inert" }
      : u.protocol === "todo:" ? (u.pathname === "" ? { type: "Inert" } : { type: "Todo", id: u.pathname })
      : OUTBOUND.has(u.protocol)
        ? { type: "Outbound", href: raw, shown: web(u) ? u.host : u.pathname, address: web(u) ? u.href : `${u.pathname}${u.search}` }
      : { type: "Inert" }
  }

  /** The writer's label as the text beside the anchor; `""` when it only repeats where the link leads, as typst's own
   * label for a link given none does. */
  export const beside = (out: Outbound, label: string): string => {
    const said = label.trim()
    return [out.href.trim(), out.address, out.shown].includes(said) ? "" : said
  }

  export const fold = <R>(h: Href, on: { Todo: (id: string) => R; Outbound: (out: Outbound) => R; Inert: () => R }): R =>
    h.type === "Todo" ? on.Todo(h.id) : h.type === "Outbound" ? on.Outbound(h) : on.Inert()
}
export default href
