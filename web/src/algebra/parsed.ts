// What was typed, read into the value it means, or the sentence to show instead of sending it and what that sentence is about.

export namespace parsed {
  // So a form says a refusal beside what it is about.
  export type About = { readonly type: "Whole" } | { readonly type: "Field"; readonly name: string }
  export type T<A> = { readonly type: "Ok"; readonly v: A } | { readonly type: "Refused"; readonly reason: string; readonly about: About }

  export const whole: About = { type: "Whole" }
  export const field = (name: string): About => ({ type: "Field", name })

  export const ok = <A>(v: A): T<A> => ({ type: "Ok", v })
  export const refused = <A = never>(reason: string, about: About = whole): T<A> => ({ type: "Refused", reason, about })
  export const of = <A>(name: string, p: T<A>): T<A> => (p.type === "Ok" ? p : { ...p, about: field(name) })

  export const fold = <A, R>(p: T<A>, alg: { Ok: (v: A) => R; Refused: (reason: string) => R }): R =>
    p.type === "Ok" ? alg.Ok(p.v) : alg.Refused(p.reason)
  export const bind = <A, B>(p: T<A>, f: (a: A) => T<B>): T<B> => (p.type === "Ok" ? f(p.v) : p)
  export const map = <A, B>(p: T<A>, f: (a: A) => B): T<B> => bind(p, a => ok(f(a)))
}
export default parsed
