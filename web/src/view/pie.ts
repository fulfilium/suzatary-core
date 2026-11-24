// A fraction in [0, 1] as the app's one mark: typst-design's `pie`, drawn in SVG to its geometry, so a pie here and
// one in a Typst page are the same mark. Its colour says what kind of fraction it is, and nothing else does.

import { svg } from "./dom"
import { percent } from "./format"
import fulfillment from "../todo/fulfillment"

export namespace pie {
  // A judgement (a todo's fulfillment, an allowance left) wears the fulfillment scale at its own value; anything else
  // (a sync's progress, a share of a whole) wears the one neutral accent, so a pull at 10% never looks urgent.
  export type Kind = { readonly type: "Judgement" } | { readonly type: "Neutral" }

  export const judgement: Kind = { type: "Judgement" }
  export const neutral: Kind = { type: "Neutral" }

  export const SIZE = 14

  // The package's outline, as a share of the size.
  const EDGE = 0.067

  // Filled clockwise from twelve o'clock on a faint disc of its own colour, outlined thin in it: the wedge is a dash of
  // length `share` on a circle of path length 1.
  export const view = (share: number, kind: Kind, said: string, size: number = SIZE): SVGSVGElement => {
    const c = size / 2
    const edge = size * EDGE
    const ink = kind.type === "Judgement" ? { class: "pie", style: `color:${fulfillment.colour(share)}` } : { class: "pie pieneutral", style: null }
    return svg(
      "svg",
      { ...ink, viewBox: `0 0 ${size} ${size}`, width: size, height: size, role: "img", "aria-label": said },
      svg("circle", { class: "pietrack", cx: c, cy: c, r: c }),
      svg("circle", {
        class: "piefill",
        cx: c,
        cy: c,
        r: c / 2,
        pathLength: 1,
        "stroke-width": c,
        "stroke-dasharray": `${share} 1`,
        transform: `rotate(-90 ${c} ${c})`,
      }),
      svg("circle", { class: "pieline", cx: c, cy: c, r: c - edge / 2, "stroke-width": edge }),
    )
  }

  export const fulfilled = (v: number, size: number = SIZE): SVGSVGElement => view(v, judgement, `${percent(v)} fulfilled`, size)
}
export default pie
