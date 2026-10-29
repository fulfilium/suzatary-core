// One value kept by what it was made from, made again only once that moved; the value it replaces is handed to `free`
// once its successor is made.

export namespace last {
  export type T<K, A> = (k: K, make: () => A) => A

  export const of = <K, A>(same: (a: K, b: K) => boolean, free: (a: A) => void = () => {}): T<K, A> => {
    let kept: { readonly type: "None" } | { readonly type: "Made"; readonly k: K; readonly a: A } = { type: "None" }
    return (k, make) => {
      if (kept.type === "Made" && same(kept.k, k)) return kept.a
      const a = make()
      if (kept.type === "Made") free(kept.a)
      kept = { type: "Made", k, a }
      return a
    }
  }
}
export default last
