// A value on its way: still loading, failed in the box's words, or here. A
// value with phases of its own nests a second sum in `V`, never a boolean
// beside this one.

export namespace lo {
  export type T<V> =
    | { readonly type: "Loading" }
    | { readonly type: "Error"; readonly e: string }
    | { readonly type: "Val"; readonly v: V }

  // A value that has settled: failed or here, never on its way; what a reading of bytes already held answers.
  export type Settled<V> = Exclude<T<V>, { readonly type: "Loading" }>

  export const loading: T<never> = { type: "Loading" }
  export const error = (e: string): Settled<never> => ({ type: "Error", e })
  export const val = <V>(v: V): Settled<V> => ({ type: "Val", v })

  export const fold = <V, R>(l: T<V>, on: { loading: () => R; error: (e: string) => R; val: (v: V) => R }): R =>
    l.type === "Val" ? on.val(l.v) : l.type === "Error" ? on.error(l.e) : on.loading()

  // An arm without a value passes through unchanged, the same object, so a source wakes nobody.
  export function bind<A, B>(l: Settled<A>, f: (a: A) => Settled<B>): Settled<B>
  export function bind<A, B>(l: T<A>, f: (a: A) => T<B>): T<B>
  export function bind<A, B>(l: T<A>, f: (a: A) => T<B>): T<B> {
    return l.type === "Val" ? f(l.v) : l
  }
  export function map<A, B>(l: Settled<A>, f: (a: A) => B): Settled<B>
  export function map<A, B>(l: T<A>, f: (a: A) => B): T<B>
  export function map<A, B>(l: T<A>, f: (a: A) => B): T<B> {
    return bind(l, a => val(f(a)))
  }
  export const flatten = <A>(l: T<T<A>>): T<A> => bind(l, a => a)

  // A monoid with `combine()` = `val([])`: the first error, else loading if any is, else every value in order.
  export const combine = <A>(...ls: T<A>[]): T<A[]> =>
    ls.reduce<T<A[]>>(
      (acc, l) =>
        acc.type === "Error" ? acc
        : l.type === "Error" ? l
        : acc.type === "Loading" ? acc
        : map(l, v => [...acc.v, v]),
      val([]),
    )

  export const withDefault = <A, D = A>(l: T<A>, d: D): A | D => (l.type === "Val" ? l.v : d)
  export const isVal = <A>(l: T<A>): boolean => l.type === "Val"

  // A failure in the words a person reads: an error's message, anything else as it prints.
  export const reason = (e: unknown): string => (e instanceof Error ? e.message : String(e))

  export const of = <A>(p: Promise<A>): Promise<Settled<A>> => p.then(val, (e: unknown) => error(reason(e)))
}
export default lo
