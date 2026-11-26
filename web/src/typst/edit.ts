// Editing Typst source in the tab: what the editor's worker is asked and answers, the source drawn in shaded runs, and
// the text a stored title opens as.

import d from "../wire/decoder"
import lo from "../algebra/lo"

export namespace edit {
  // Every offset is UTF-16, the unit a textarea's selection counts in.
  export type Span = { readonly from: number; readonly to: number; readonly tag: string }
  export type Diagnostic = { readonly message: string; readonly hints: readonly string[]; readonly from: number | null; readonly to: number | null }
  export type Checked = { readonly type: "Html"; readonly html: string; readonly warnings: readonly Diagnostic[] } | { readonly type: "Failed"; readonly diagnostics: readonly Diagnostic[] }
  export type Item = { readonly label: string; readonly insert: string; readonly cursor: number; readonly detail: string | null }
  export type Offer = { readonly from: number; readonly items: readonly Item[] }

  export type Ask =
    | { readonly type: "Shade"; readonly source: string }
    | { readonly type: "Check"; readonly source: string }
    | { readonly type: "Complete"; readonly source: string; readonly cursor: number; readonly explicit: boolean }
    | { readonly type: "Hover"; readonly source: string; readonly cursor: number }

  export type Answer =
    | { readonly type: "Shade"; readonly spans: readonly Span[] }
    | { readonly type: "Check"; readonly checked: Checked }
    | { readonly type: "Complete"; readonly offer: Offer | null }
    | { readonly type: "Hover"; readonly text: string }

  export type Asked = { readonly id: number; readonly ask: Ask }
  export type Answered = { readonly id: number; readonly answer: Answer }

  export type Run = { readonly text: string; readonly tag: string | null; readonly marked: boolean }

  // The key the text is compiled under, in the worker and on the box alike.
  export const MAIN = "/main.typ"

  export const fold = <R>(c: Checked, on: { html: (html: string, warnings: readonly Diagnostic[]) => R; failed: (ds: readonly Diagnostic[]) => R }): R =>
    c.type === "Html" ? on.html(c.html, c.warnings) : on.failed(c.diagnostics)

  export const failed = (message: string): Checked => ({ type: "Failed", diagnostics: [{ message, hints: [], from: null, to: null }] })

  type Said = { severity: "error" | "warning"; message: string; hints: string[]; file: string | null; from: number | null; to: number | null }
  const said = d.obj<Said>({
    severity: d.oneOf({ error: null, warning: null }),
    message: d.str,
    hints: d.arr(d.str),
    file: d.nullable(d.str),
    from: d.nullable(d.num),
    to: d.nullable(d.num),
  })
  const compiled = d.json(d.obj<{ html: string | null; diagnostics: Said[] }>({ html: d.nullable(d.str), diagnostics: d.arr(said) }))

  // Typst prints this with every HTML compile; it is about the export, not about the text.
  const EXPERIMENTAL = "html export is under active development and incomplete"

  const SHAPES: readonly string[] = ["rect", "square", "circle", "ellipse", "line", "polygon", "curve", "path"]

  // The reason the HTML export drops an element, in the owner's words.
  const because = (element: string): string =>
    SHAPES.includes(element) ? "shapes have no HTML form" : element === "place" ? "placed content has no HTML form" : element === "h" || element === "v" ? "fixed spacing has no HTML form" : "it has no HTML form"

  const IGNORED = /^(\S+) was ignored during HTML export$/

  // Typst's "ignored" warning in the owner's terms; any other warning keeps Typst's words.
  const plain = (x: Diagnostic): Diagnostic => {
    const element = IGNORED.exec(x.message)?.[1]
    return element === undefined ? x : { ...x, message: `${element} is not shown: ${because(element)}`, hints: [] }
  }

  // A warning is not a failure; a diagnostic about another file than the text marks nothing in it.
  export const checked = (text: string): Checked => {
    const r = compiled(text)
    if (r.type === "Error") return failed(`typst-wasm answered an unexpected shape: ${r.e}`)
    const located = (x: Said): Diagnostic => (x.file === MAIN ? { message: x.message, hints: x.hints, from: x.from, to: x.to } : { message: x.message, hints: x.hints, from: null, to: null })
    const errors = r.v.diagnostics.filter(x => x.severity === "error").map(located)
    const warnings = r.v.diagnostics.filter(x => x.severity === "warning" && x.message !== EXPERIMENTAL).map(x => plain(located(x)))
    return r.v.html !== null && errors.length === 0 ? { type: "Html", html: r.v.html, warnings } : errors.length === 0 ? failed("typst produced no document") : { type: "Failed", diagnostics: errors }
  }

  export const spans = (text: string): readonly Span[] =>
    lo.withDefault(d.json(d.arr(d.obj<Span>({ from: d.num, to: d.num, tag: d.str })))(text), [])

  export const offer = (text: string): Offer | null =>
    lo.withDefault(
      d.json(d.nullable(d.obj<Offer>({ from: d.num, items: d.arr(d.obj<Item>({ label: d.str, insert: d.str, cursor: d.num, detail: d.nullable(d.str) })) })))(text),
      null,
    )

  export const hover = (text: string): string =>
    lo.withDefault(d.json(d.nullable(d.obj<{ text: string }>({ text: d.str })))(text), null)?.text ?? ""

  // Cut points of a sorted, disjoint span list and of the marked ranges, in order; the runs between them partition the source.
  export const runs = (source: string, shades: readonly Span[], marks: readonly Diagnostic[]): readonly Run[] => {
    const n = source.length
    const ranges = marks.flatMap(m => (m.from === null || m.to === null ? [] : [{ from: Math.min(m.from, n), to: Math.min(Math.max(m.to, m.from + 1), n) }]))
    const cuts = [...new Set([0, n, ...shades.flatMap(s => [s.from, s.to]), ...ranges.flatMap(r => [r.from, r.to])])]
      .filter(c => c >= 0 && c <= n)
      .sort((a, b) => a - b)
    const tagAt = (at: number): string | null => shades.find(s => s.from <= at && at < s.to)?.tag ?? null
    const markedAt = (at: number): boolean => ranges.some(r => r.from <= at && at < r.to)
    return cuts.slice(0, -1).map((from, i) => ({ text: source.slice(from, cuts[i + 1]), tag: tagAt(from), marked: markedAt(from) }))
  }

  // `line:column`, one-based, of a UTF-16 offset.
  export const position = (source: string, at: number): string => {
    const before = source.slice(0, at).split("\n")
    return `${before.length}:${(before[before.length - 1] ?? "").length + 1}`
  }

  export const line = (source: string, x: Diagnostic): string =>
    `${x.from === null ? "" : `${position(source, x.from)}: `}${x.message}${x.hints.map(h => ` (hint: ${h})`).join("")}`

  // A title stored in escaped prose opens in that prose, so editing it never shows the escape's backslashes; markup
  // a writer wrote opens unchanged.
  export type Opened = { readonly text: string; readonly wasProse: boolean }
  export const opened = (stored: string, prose: (markup: string) => string | undefined): Opened => {
    const p = prose(stored)
    return p === undefined || p === stored ? { text: stored, wasProse: false } : { text: p, wasProse: true }
  }

  export const accept = (source: string, caret: number, o: Offer, i: Item): { readonly source: string; readonly caret: number } =>
    ({ source: source.slice(0, o.from) + i.insert + source.slice(caret), caret: o.from + i.cursor })

  // Typing a name (or `#` before one) asks for completions; anything else closes them.
  export const asks = (before: string): boolean => /[\p{L}\p{N}_.#-]$/u.test(before)

  export type State = {
    readonly source: string
    readonly shade: { readonly source: string; readonly spans: readonly Span[] }
    readonly check: { readonly source: string; readonly checked: Checked } | null
    // The last page that compiled, kept while a later text does not, so the preview never empties under the cursor.
    readonly shown: string | null
    readonly offer: Offer | null
    readonly chosen: number
    readonly hover: string
  }

  export type Event =
    | { readonly type: "Typed"; readonly source: string }
    | { readonly type: "Shaded"; readonly source: string; readonly spans: readonly Span[] }
    | { readonly type: "Checked"; readonly source: string; readonly checked: Checked }
    | { readonly type: "Offered"; readonly source: string; readonly offer: Offer | null }
    | { readonly type: "Chose"; readonly by: number }
    | { readonly type: "Closed" }
    | { readonly type: "Hovered"; readonly source: string; readonly text: string }

  export const of = (source: string): State =>
    ({ source, shade: { source: "", spans: [] }, check: null, shown: null, offer: null, chosen: 0, hover: "" })

  // An answer about a source the editor no longer holds changes nothing.
  export const step = (s: State, e: Event): State =>
    e.type === "Typed" ? (e.source === s.source ? s : { ...s, source: e.source, hover: "" })
    : e.type === "Closed" ? (s.offer === null ? s : { ...s, offer: null, chosen: 0 })
    : e.type === "Chose" ? (s.offer === null || s.offer.items.length === 0 ? s : { ...s, chosen: (s.chosen + e.by + s.offer.items.length) % s.offer.items.length })
    : e.source !== s.source ? s
    : e.type === "Shaded" ? { ...s, shade: { source: e.source, spans: e.spans } }
    : e.type === "Checked" ? { ...s, check: { source: e.source, checked: e.checked }, shown: e.checked.type === "Html" ? e.checked.html : s.shown }
    : e.type === "Offered" ? { ...s, offer: e.offer !== null && e.offer.items.length > 0 ? e.offer : null, chosen: 0 }
    : { ...s, hover: e.text }

  // What typst says is wrong with the text the editor holds now; nothing while it has not said.
  export const problems = (s: State): readonly Diagnostic[] =>
    s.check === null || s.check.source !== s.source ? [] : fold(s.check.checked, { html: () => [], failed: ds => ds })

  // What the compile dropped from the text the editor holds now; the text still saves.
  export const dropped = (s: State): readonly Diagnostic[] =>
    s.check === null || s.check.source !== s.source ? [] : fold(s.check.checked, { html: (_, ws) => ws, failed: () => [] })

  // The source in runs, shaded once the worker has shaded this very text and plain until then.
  export const drawn = (s: State): readonly Run[] =>
    runs(s.source, s.shade.source === s.source ? s.shade.spans : [], problems(s))

  // The preview is stale while the text it shows is not the text held, or that text does not compile.
  export const stale = (s: State): boolean => s.check === null || s.check.source !== s.source || s.check.checked.type !== "Html"
}
export default edit
