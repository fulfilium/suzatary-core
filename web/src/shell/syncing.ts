import { el } from "../view/dom"
import { percent } from "../view/format"
import pie from "../view/pie"
import progress from "../prodrome/progress"
import stores from "../prodrome/stores"

export namespace syncing {
  export type Line = { readonly name: stores.Name; readonly done: number; readonly needed: number }

  export const lines = (p: progress.T): readonly Line[] =>
    stores.NAMES.flatMap(name => {
      const b = progress.bar(p[name])
      return b.type === "Hidden" ? [] : [{ name, done: b.done, needed: b.needed }]
    })

  const line = (l: Line): Node => {
    const share = l.done / l.needed
    return el("div", { class: "syncline" }, pie.view(share, pie.neutral, `${percent(share)} synced`), el("span", null, progress.said(l.name, l.done, l.needed)))
  }

  export const install = (bar: HTMLElement): ((p: progress.T) => void) => p => {
    const shown = lines(p)
    bar.hidden = shown.length === 0
    bar.replaceChildren(...shown.map(line))
  }
}
export default syncing
