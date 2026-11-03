/** The receipt line a round draws for what the secretary remembered, with Forget. Its page is `app/pages/src/memory.rs`. */

import type { FactId, Receipt } from "../wire/generated/wire"
import { el } from "../view/dom"
import marks from "../view/marks"
import { hrefOf } from "../shell/route"
import { toast } from "../view/toast"
import intentsync from "../prodrome/intentsync"
import lo from "../algebra/lo"

export namespace memory {
  /** Forgetting, for a login that writes; `null` offers no Forget at all. */
  export type Forget = ((id: FactId) => void) | null

  /** A fact forgotten is a fact the next reading no longer holds, so `after` reads again; a refusal names the fact. */
  export const forgets = (writes: boolean, after: () => unknown): Forget =>
    writes ? id => void intentsync.asked({ type: "Forget", fact: id }).then(after, (e: unknown) => toast(`Could not forget fact ${id}: ${lo.reason(e)}`)) : null

  const forgetButton = (id: FactId, forget: (id: FactId) => void): Node =>
    el("button", { type: "button", class: "act forget", onclick: () => forget(id) }, "Forget")

  /** The line a round shows for what it remembered, never folded into its trace; its words only while the memory holds them. */
  export const receipt = (r: Receipt, forget: Forget): HTMLElement =>
    el(
      "p",
      { class: "receipt", "data-fact": r.fact, "data-state": r.state },
      el("a", { href: hrefOf({ type: "memory" }) }, "Remembered"),
      ": ",
      r.state === "standing"
        ? el("span", { class: "receipttext" }, r.text)
        : el("span", { class: "receiptfact" }, `fact ${r.fact}`),
      r.state === "forgotten" ? [marks.diamond(), el("span", { class: "receiptstate" }, "forgotten")]
      : r.state === "merged" ? [marks.diamond(), el("span", { class: "receiptstate" }, `merged into fact ${r.into}`)]
      : forget === null ? null
      : [marks.diamond(), forgetButton(r.fact, forget)],
    )
}
export default memory
