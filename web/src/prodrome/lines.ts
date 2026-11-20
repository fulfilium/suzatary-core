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

  // A question as a tab seals it: its words, what it points at, and each file as the upload answered it, named as the
  // person named it.
  export type Asking = {
    readonly text: string
    readonly references: readonly string[]
    readonly files: readonly { readonly attachment: unknown; readonly name: string }[]
  }

  // A line a page asks this tab to seal (`Lines.seal`): a rename, a setup of a template by its name, a question, a topic
  // opened under a title (empty leaves it to the title job) with its first question, or a stop.
  export type Sealing =
    | { readonly type: "Rename"; readonly conversation: string; readonly title: string }
    | { readonly type: "SetUp"; readonly template: string }
    | ({ readonly type: "Say"; readonly conversation: string } & Asking)
    | { readonly type: "Begin"; readonly title: string; readonly say: Asking }
    | { readonly type: "Stop"; readonly conversation: string }

  // A line sealed: its name, and its conversation, a new one's the id of its opening line.
  export type Sealed = { readonly name: string; readonly conversation: string }

  export const WHOLE: Scope = { type: "Whole" }
  export const one = (id: string): Scope => ({ type: "Conversations", ids: [id] })
}
export default lines
