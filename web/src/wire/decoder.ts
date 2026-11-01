// Bytes off a wire become a typed value here or are refused, naming where: a missing field is a break, an extra one is dropped.

import lo from "../algebra/lo"

export namespace d {
  export type Decoder<A> = (u: unknown) => lo.Settled<A>
  export type Fields<A> = { readonly [K in keyof A]-?: Decoder<A[K]> }
  type Arms<A, T extends keyof A> = {
    readonly [K in A[T] & string]: Fields<Omit<Extract<A, { readonly [P in T]: K }>, T>>
  }

  // The one place a checked value is given its type: every caller below has just checked it.
  const checked = <A>(u: unknown): A => u as A

  const under = (seg: string, e: string): string => (/^[.[]/.test(e) ? `${seg}${e}` : `${seg}: ${e}`)
  const shown = (u: unknown): string =>
    u === null ? "null" : Array.isArray(u) ? "an array" : typeof u === "string" ? JSON.stringify(u) : typeof u
  const expected = (what: string, u: unknown) => lo.error(`expected ${what}, got ${shown(u)}`)
  const isRecord = (u: unknown): u is Record<string, unknown> => typeof u === "object" && u !== null && !Array.isArray(u)

  export const str: Decoder<string> = u => (typeof u === "string" ? lo.val(u) : expected("a string", u))
  export const num: Decoder<number> = u =>
    typeof u === "number" && Number.isFinite(u) ? lo.val(u) : expected("a number", u)
  export const bool: Decoder<boolean> = u => (typeof u === "boolean" ? lo.val(u) : expected("a boolean", u))
  export const unknown: Decoder<unknown> = lo.val
  // Bytes as the device keeps them: a structured clone of a `Uint8Array` reads back as one.
  export const bytes: Decoder<Uint8Array> = u => (u instanceof Uint8Array ? lo.val(u) : expected("bytes", u))

  export const lit =
    <const L extends string | number | boolean | null>(l: L): Decoder<L> =>
    u =>
      u === l ? lo.val(l) : expected(JSON.stringify(l), u)

  // Keyed by every member, so a member the type gains and this table lacks does not compile.
  export const oneOf = <A extends string>(members: { readonly [K in A]: null }): Decoder<A> => {
    const known = new Set<unknown>(Object.keys(members))
    return u => (known.has(u) ? lo.val(checked<A>(u)) : expected(`one of ${[...known].join(" | ")}`, u))
  }

  export const nullable =
    <A>(a: Decoder<A>): Decoder<A | null> =>
    u =>
      u === null ? lo.val(null) : a(u)

  export const arr =
    <A>(a: Decoder<A>): Decoder<A[]> =>
    u => {
      if (!Array.isArray(u)) return expected("an array", u)
      const out: A[] = []
      for (const [i, x] of u.entries()) {
        const r = a(x)
        if (r.type === "Error") return lo.error(under(`[${i}]`, r.e))
        out.push(r.v)
      }
      return lo.val(out)
    }

  export const rec =
    <A>(a: Decoder<A>): Decoder<Record<string, A>> =>
    u => {
      if (!isRecord(u)) return expected("an object", u)
      const out: Record<string, A> = {}
      for (const [k, x] of Object.entries(u)) {
        const r = a(x)
        if (r.type === "Error") return lo.error(under(`[${JSON.stringify(k)}]`, r.e))
        out[k] = r.v
      }
      return lo.val(out)
    }

  const fields = (spec: Readonly<Record<string, Decoder<unknown>>>, u: Record<string, unknown>): lo.Settled<Record<string, unknown>> => {
    const out: Record<string, unknown> = {}
    for (const [k, a] of Object.entries(spec)) {
      if (!(k in u)) return lo.error(`missing field "${k}"`)
      const r = a(u[k])
      if (r.type === "Error") return lo.error(under(`.${k}`, r.e))
      out[k] = r.v
    }
    return lo.val(out)
  }

  export const obj =
    <A>(spec: Fields<A>): Decoder<A> =>
    u =>
      isRecord(u) ? lo.map(fields(spec, u), checked<A>) : expected("an object", u)

  // A closed sum tagged on `tag`: one arm per tag value, each forcing its own fields.
  export const sum = <A, T extends keyof A & string>(tag: T, arms: Arms<A, T>): Decoder<A> => {
    const table: Readonly<Record<string, Readonly<Record<string, Decoder<unknown>>>>> = arms
    return u => {
      if (!isRecord(u)) return expected("an object", u)
      if (!(tag in u)) return lo.error(`missing field "${tag}"`)
      const k = u[tag]
      const arm = typeof k === "string" && Object.hasOwn(table, k) ? table[k] : undefined
      return arm === undefined
        ? lo.error(under(`.${tag}`, `expected one of ${Object.keys(table).join(" | ")}, got ${shown(k)}`))
        : lo.map(fields(arm, u), o => checked<A>({ [tag]: k, ...o }))
    }
  }

  export const tuple =
    <A, B>(a: Decoder<A>, b: Decoder<B>): Decoder<[A, B]> =>
    u =>
      Array.isArray(u) && u.length === 2
        ? lo.bind(a(u[0]), x => lo.map(b(u[1]), y => [x, y]))
        : expected("a pair", u)

  export const map =
    <A, B>(a: Decoder<A>, f: (a: A) => B): Decoder<B> =>
    u =>
      lo.map(a(u), f)

  export const bind =
    <A, B>(a: Decoder<A>, f: (a: A) => lo.Settled<B>): Decoder<B> =>
    u =>
      lo.bind(a(u), f)

  // A refusal of both says why each refused, so the one the value was meant to be is named.
  export const alt =
    <A, B>(a: Decoder<A>, b: Decoder<B>): Decoder<A | B> =>
    u => {
      const r = a(u)
      if (r.type === "Val") return r
      const q = b(u)
      return q.type === "Error" ? lo.error(`${r.e}; or ${q.e}`) : q
    }

  // For a decoder defined below its first use.
  export const lazy =
    <A>(a: () => Decoder<A>): Decoder<A> =>
    u =>
      a()(u)

  // An object with the named string fields and any others, kept whole: a term's JSON, walked generically.
  export const open =
    <A extends Record<string, unknown>>(spec: { readonly [K in keyof A]-?: Decoder<A[K]> }): Decoder<A> =>
    u =>
      isRecord(u) ? lo.map(fields(spec, u), o => checked<A>({ ...u, ...o })) : expected("an object", u)

  // JSON text, parsed and then decoded: an SSE frame, a stored row, the core's answer.
  export const json =
    <A>(a: Decoder<A>): Decoder<A> =>
    u => {
      if (typeof u !== "string") return expected("JSON text", u)
      try {
        return a(JSON.parse(u))
      } catch {
        return lo.error("not JSON")
      }
    }

  export const is =
    <A>(a: Decoder<A>) =>
    (u: unknown): u is A =>
      a(u).type === "Val"
}
export default d
