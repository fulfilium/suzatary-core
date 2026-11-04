# TypeScript style — web/

The owner's own, from his scanman codebase (2026-09-25). Every new or rewritten
module in `web/src/` follows it; a brief for any agent touching `web/` points
here. The web's laws (its tests) and the doors of `docs/WEB-SPEC.md` still
hold, and so do the style rules in `AGENTS.md`, which apply to both languages.

## The rules

1. **A module is a namespace.** `export namespace tree { … }` and
   `export default tree`; callers write `tree.cata(…)`, `lev.map(…)`.
   One idea per namespace, named for the idea, not the page.
2. **Types first.** Write the data type before any function over it. Sums are
   tagged on `type`, never records with booleans:
   ```ts
   export type Lev<V> = { type: "Loading" } | { type: "Error"; e: string } | { type: "Val"; v: V }
   ```
   A nested type is indented to mirror its nesting.
3. **Every sum gets its algebra.** The few total functions that make it usable:
   `map`, `flatten`/`bind`, `combine` (applicative), `fold`, a default. A caller
   uses the algebra, never a `switch` on someone else's tag.
4. **Aggregates are monoids.** `combine(...xs)` with an identity, or a partial
   order's `join`; state which in one line.
5. **A view is a function of a value.** `(value, effects) => Node`. Effects are
   passed in as values: continuations (`withVal`, `onOpen`), getter/setter
   pairs (`{ get, set }`). No view fetches, and no view keeps flags in a
   closure: state is a `Source` value (`web/src/algebra/source.ts`) stepped by a pure
   `step`.
6. **Expressions over statements.** Arrow functions, `map`/`reduce`, ternaries
   and early returns; `let` only where a loop is clearer.
7. **Short names in small scopes** (`k`, `v`, `xs`, `t`), full names at module
   level. Type-level helpers (`KeysOf`, `ToEntries`) where they remove a cast.
8. **Almost no comments.** A module gets one line saying what idea it names; a
   function gets a comment only for a decision the types cannot carry. No
   narration, no history, no restating the code.
9. **Laws are node tests.** Each algebra's laws (functor identity, monoid
   identity and associativity, round trips) are tested in `web/test/` over the
   seeded generator.

## Example

```ts
export namespace tree {
  export type Tree<A> = { v: A; kids: Tree<A>[] }

  export const leaf = <A>(v: A): Tree<A> => ({ v, kids: [] })
  export const map = <A, B>(f: (a: A) => B, t: Tree<A>): Tree<B> =>
    ({ v: f(t.v), kids: t.kids.map(k => map(f, k)) })
  export const cata = <A, R>(alg: (v: A, rs: R[]) => R, t: Tree<A>): R =>
    alg(t.v, t.kids.map(k => cata(alg, k)))
  export const size = <A>(t: Tree<A>) => cata((_, rs: number[]) => 1 + rs.reduce((a, b) => a + b, 0), t)
}
export default tree
```
