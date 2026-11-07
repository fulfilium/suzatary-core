// A page's scroll box as the runtime keeps it: who last moved it and where the reader is (`Local::Scroll`), the moves a
// page asks for (`Effect::Scroll`), and the room beneath the end that a page reserves (`data-room`).

import type { Scroll } from "../wire/generated/pages"
import reading from "../algebra/reading"

export namespace scroll {
  export type Place = "bottom" | "above"
  export type Mover = "reader" | "page"
  export type Scrolled = { readonly by: Mover; readonly at: Place }

  // Who moved the box is read from the reader's input, never from the scroll: the box also moves when a page places
  // something, when its content shrinks, and when the browser anchors it. A scroll within READER_MS of a wheel, touch,
  // scroll key or press on the scrollbar is the reader's, and so is every scroll following one without a pause (a fling).
  const READER_MS = 600
  const KEYS = new Set(["PageUp", "PageDown", "ArrowUp", "ArrowDown", "Home", "End", " "])

  type Box = {
    readonly box: Element
    readerAt: number
    room: number
    last: Scrolled
    readonly ears: Set<() => void>
  }

  const boxes = new WeakMap<Element, Box>()

  const page = (box: Element): boolean => box === document.scrollingElement
  const target = (box: Element): EventTarget => (page(box) ? window : box)
  const roomOf = (box: Element): HTMLElement | null => (page(box) ? document : box).querySelector<HTMLElement>("[data-room]")
  const top = (box: Element): number => (page(box) ? 0 : box.getBoundingClientRect().top)
  const offsetOf = (b: Element, n: Element): number => n.getBoundingClientRect().top - top(b) + b.scrollTop

  // Measured against the end of what the box shows, never the room, which goes as soon as the hold does.
  const placed = (b: Box): Place => {
    const { scrollTop, scrollHeight, clientHeight } = b.box
    return Math.max(0, scrollHeight - b.room - clientHeight - scrollTop) <= clientHeight ? "bottom" : "above"
  }

  // A window's height while a page reserves it; otherwise only as much as the reader still sees, so letting it go never
  // leaves the box shorter than the view and never pulls the reader down.
  // A room not held only shrinks, so one already gone stays gone without reading the layout: a step's patches are then
  // laid out once, by the browser's frame, and never forced by the step that made them.
  const fit = (b: Box): void => {
    const room = roomOf(b.box)
    if (room === null) return void (b.room = 0)
    const held = room.dataset["room"] === "held"
    if (!held && b.room === 0) return void ((room.style.height = "0px"), (room.hidden = true))
    const height = b.box.clientHeight
    const shown = room.hidden ? 0 : offsetOf(b.box, room)
    const next = held ? height : Math.min(b.room, Math.max(0, b.box.scrollTop + height - shown))
    b.room = next
    room.style.height = `${next}px`
    room.hidden = next === 0
  }

  const of_ = (box: Element): Box => {
    const known = boxes.get(box)
    if (known !== undefined) return known
    const b: Box = { box, readerAt: Number.NEGATIVE_INFINITY, room: 0, last: { by: "page", at: "bottom" }, ears: new Set() }
    const read = (): void => void (b.readerAt = performance.now())
    const on = target(box)
    on.addEventListener("wheel", read, { passive: true })
    on.addEventListener("touchmove", read, { passive: true })
    on.addEventListener("pointerdown", e => (e.target === box ? read() : undefined), { passive: true })
    window.addEventListener("keydown", e => {
      const t = e.target
      if (KEYS.has(e.key) && !(t instanceof HTMLElement && t.closest("input, textarea, select, [contenteditable]") !== null)) read()
    })
    on.addEventListener("scroll", () => {
      const by: Mover = performance.now() - b.readerAt < READER_MS ? "reader" : "page"
      if (by === "reader") read()
      fit(b)
      b.last = { by, at: placed(b) }
      b.ears.forEach(f => f())
    }, { passive: true })
    boxes.set(box, b)
    return b
  }

  export const of = (box: Element): reading.Reading<Scrolled> => {
    const b = of_(box)
    return reading.held(() => b.last, f => (b.ears.add(f), () => void b.ears.delete(f)))
  }

  // After a step's patches.
  export const settle = (box: Element): void => fit(of_(box))

  // `Keep` at once; the rest next frame, and only if the reader has not moved the box since: the compositor may be
  // scrolling for a wheel the page has not heard yet. The sticky header covers the box's top.
  export const move = (box: Element, root: Element, to: Scroll, fromEnd: number): void => {
    const b = of_(box)
    // A page's move is the page's: it ends the run of scrolls counted as the reader's for following one.
    if (to.type === "Keep") return void ((b.readerAt = Number.NEGATIVE_INFINITY), (box.scrollTop = box.scrollHeight - fromEnd))
    const asked = performance.now()
    requestAnimationFrame(() => {
      if (b.readerAt >= asked) return
      b.readerAt = Number.NEGATIVE_INFINITY
      fit(b)
      const node = to.type === "End" ? null : root.querySelector(`[data-field="${CSS.escape(to.field)}"]`)
      switch (to.type) {
        case "End":
          return void (box.scrollTop = box.scrollHeight)
        case "Top": {
          if (node === null) return
          const header = page(box) ? (document.querySelector("body > header")?.getBoundingClientRect().bottom ?? 0) : 0
          return void (box.scrollTop = offsetOf(box, node) - header)
        }
        case "Into":
          return node?.scrollIntoView({ block: "center" })
      }
    })
  }
}
export default scroll
