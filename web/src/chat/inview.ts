/** The reference in view, which the Ask button and the secretary panel attach to: derived from the route alone, never typed. */

import type { Reference } from "./refs";
import type { Route } from "../shell/route";

export type ViewReference =
  | { readonly type: "none" }
  | { readonly type: "one"; readonly reference: Reference };

/** `chain` is `App.chain`: a todo's route carries no chain of its own. */
export function referenceInView(screen: Route, chain: string): ViewReference {
  switch (screen.type) {
    case "todo":
      return { type: "one", reference: { kind: "todo", chain, id: screen.id } };
    case "message":
      return { type: "one", reference: { kind: "mail", hash: screen.id } };
    case "thread":
      return { type: "one", reference: { kind: "thread", id: screen.id } };
    case "briefing":
      return { type: "one", reference: { kind: "briefing", date: screen.date } };
    // Every other page is about the whole conversation.
    default:
      return { type: "none" };
  }
}
