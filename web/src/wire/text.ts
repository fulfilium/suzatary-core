// The JSON of a value of `T` in UTF-8, as whoever holds the value printed it: the box, the core, or this tab. A reading
// crosses from the worker to the views module as one and is parsed there once. Bytes, not a string: a large string
// copied across `postMessage` is garbage on the page's heap, and an ArrayBuffer's bytes are not on it.

export namespace text {
  // A value read-only all the way down, as a text's value is: whoever prints one cannot change it after.
  export type Frozen<T> = T extends readonly (infer U)[] ? readonly Frozen<U>[] : T extends object ? { readonly [P in keyof T]: Frozen<T[P]> } : T
  export type Text<T> = { readonly bytes: Uint8Array; readonly of?: Frozen<T> }

  const utf8 = new TextEncoder()

  // A value this tab made, printed once.
  export const printed = <T>(v: T): Text<T> => ({ bytes: utf8.encode(JSON.stringify(v)) })

  // A text another printed: the core's answer.
  export const of = <T>(bytes: Uint8Array): Text<T> => ({ bytes })

  // A text another printed as a string: the box's body.
  export const said = <T>(json: string): Text<T> => ({ bytes: utf8.encode(json) })

  // The object of these fields, each text set in whole: no field is parsed or printed again.
  export const object = <T>(fields: { readonly [K in keyof T]: Text<T[K]> }): Text<T> => {
    const entries: [string, Text<unknown>][] = Object.entries(fields)
    const parts = [utf8.encode("{"), ...entries.flatMap(([k, t], i) => [utf8.encode(`${i === 0 ? "" : ","}${JSON.stringify(k)}:`), t.bytes]), utf8.encode("}")]
    const bytes = new Uint8Array(parts.reduce((n, p) => n + p.length, 0))
    parts.reduce((at, p) => (bytes.set(p, at), at + p.length), 0)
    return { bytes }
  }
}
export default text
