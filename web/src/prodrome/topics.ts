// A topic write a page asks for, and the events the core's reading plans for it, for the store it goes to.

import type { TopicReading } from "./core"
import { CoreError } from "./core"
import d from "../wire/decoder"

export namespace topics {
  export type Write =
    | { readonly type: "Add"; readonly target: string; readonly topic: string }
    | { readonly type: "Remove"; readonly target: string; readonly topic: string }
    | { readonly type: "Rename"; readonly from: string; readonly to: string }

  // The two stores a tab writes, by their path segment.
  export type Store = "topics" | "topic-aliases"

  export const store = (w: Write): Store => (w.type === "Rename" ? "topic-aliases" : "topics")

  const prints = d.json(d.arr(d.str))
  const each = (text: string): readonly string[] => {
    const read = prints(text)
    if (read.type === "Val") return read.v
    throw new CoreError(`a topic plan: unexpected shape (${read.e})`)
  }

  // Each event's print, none where the reading already holds what `w` asks.
  export const plan = (r: TopicReading, w: Write, at: string, actor: string): readonly string[] => {
    switch (w.type) {
      case "Add": {
        const e = r.add(w.target, w.topic, at, actor)
        return e === undefined ? [] : [e]
      }
      case "Remove":
        return each(r.remove(w.target, w.topic, at, actor))
      case "Rename":
        return each(r.rename(w.from, w.to, at, actor))
    }
  }
}
export default topics
