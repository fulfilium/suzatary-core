// What a reading of the lines reads of the skeleton, as the core's worker asks its core (`Lines.lacking`): a reading is
// whole or it is not made (`engine.over`), so every node it reads is held, or the nodes still lacking are fetched, never
// a fold over part of what it needs. The core reads its own skeleton; this names the question.

export namespace lines {
  // The whole store, for the list and a search; some conversations, each of which reads its own lines and what they
  // rest on, for a window; the lines whose words the stream's drawn rounds read, and nothing they rest on, as the core's
  // skeleton names them; or every conversation a line naming one of some keys is in, whose words a purge opens.
  export type Scope =
    | { readonly type: "Whole" }
    | { readonly type: "Conversations"; readonly ids: readonly string[] }
    | { readonly type: "Objects"; readonly names: readonly string[] }
    | { readonly type: "Naming"; readonly keys: readonly string[] }

  export const WHOLE: Scope = { type: "Whole" }
  export const one = (id: string): Scope => ({ type: "Conversations", ids: [id] })
}
export default lines
