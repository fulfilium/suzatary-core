// The one loading mark: a pie filling from the fulfillment scale's bad end to its good end.
// Another design is this file and its block in style.css, and nothing else.

import { svg } from "./dom"

export namespace loadmark {
  export const view = (): SVGSVGElement =>
    svg(
      "svg",
      { class: "loadmark", viewBox: "0 0 24 24", "aria-hidden": "true" },
      svg("circle", { class: "loadring", cx: 12, cy: 12, r: 10.25 }),
      svg("circle", { class: "loadfill", cx: 12, cy: 12, r: 4.75 }),
    )
}
export default loadmark
