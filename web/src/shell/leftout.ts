// Nothing is left out silently, and nothing left out fails a page: the page draws what could be read, and this line says what could not.

import { el } from "../view/dom"
import disclosure from "../view/disclosure"
import excluded from "../prodrome/excluded"
import type ui from "../app/ui"

export namespace leftout {
  const object = (x: excluded.Excluded): HTMLElement =>
    el(
      "li",
      null,
      el("code", null, x.name),
      ` — ${x.why}`,
      x.resting.length === 0 ? "" : ` (${x.resting.length} resting on it: ${x.resting.map(n => `${n.slice(0, 12)}…`).join(", ")})`,
    )

  const chain = (c: excluded.Chain): HTMLElement => el("li", { "data-chain": c.chain }, el("code", null, c.chain), ` — ${c.why}`)

  // A copy whose way out is a reset offers it where it is named, in words that say it writes over that copy.
  const row = (reset: (chain: string) => void) => (r: excluded.Row): HTMLElement => {
    const press = el("button", { type: "button", class: "sincedismiss" }, "Write over this copy from the box")
    press.addEventListener("click", () => reset(r.chain))
    return el("li", { "data-row": r.chain }, el("code", null, r.chain), ` — ${r.why}`, ...(r.cure === "Reset" ? [" ", press] : []))
  }

  export const view = (l: excluded.Left, reset: (chain: string) => void, open: disclosure.Open): HTMLElement =>
    disclosure.details(open, {}, `${excluded.said(l)}.`, el("ul", null, ...l.objects.map(object), ...l.chains.map(chain), ...l.rows.map(row(reset))))

  // Every reading may move what is left out, so the line is drawn again under a reader who opened it: it stays open.
  export const install = (bar: HTMLElement, reset: (chain: string) => void, open: ui.Cell<boolean>): ((l: excluded.Left) => void) => l => {
    bar.hidden = excluded.empty(l)
    bar.replaceChildren(...(excluded.empty(l) ? [] : [view(l, reset, disclosure.of(open))]))
  }
}
export default leftout
