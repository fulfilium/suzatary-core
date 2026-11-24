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

  // The page's last move, when it was to a node's top: kept while the room is held and no reader's input came after `at`,
  // and while the body is `resized`; `y` is where on the box the node stood when last placed.
  type Placed = { readonly type: "Elsewhere" } | { readonly type: "Top"; readonly root: Element; readonly field: string; readonly at: number; readonly y: number }
  const ELSEWHERE: Placed = { type: "Elsewhere" }

  type Box = {
    readonly box: Element
    readerAt: number
    room: number
    last: Scrolled
    placed: Placed
    readonly resized: ResizeObserver
    readonly ears: Set<() => void>
  }

  const boxes = new WeakMap<Element, Box>()

  const page = (box: Element): boolean => box === document.scrollingElement
  const target = (box: Element): EventTarget => (page(box) ? window : box)
  const roomOf = (box: Element): HTMLElement | null => (page(box) ? document : box).querySelector<HTMLElement>("[data-room]")
  const top = (box: Element): number => (page(box) ? 0 : box.getBoundingClientRect().top)
  const shownAt = (b: Element, n: Element): number => n.getBoundingClientRect().top - top(b)
  const offsetOf = (b: Element, n: Element): number => shownAt(b, n) + b.scrollTop
  const nodeOf = (root: Element, field: string): Element | null => root.querySelector(`[data-field="${CSS.escape(field)}"]`)
  // The sticky header covers the top of the page's own box, and of no other.
  const covered = (box: Element): number => (page(box) ? (document.querySelector("body > header")?.getBoundingClientRect().bottom ?? 0) : 0)

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
    if (room === null) return void (elsewhere(b), (b.room = 0))
    const held = room.dataset["room"] === "held"
    // A place outlives neither its room nor its page: a root out of the document is a page left, or a tile hidden, whose
    // box is put back where it stood when it is drawn again, not placed.
    if (!held || (b.placed.type === "Top" && !b.placed.root.isConnected)) elsewhere(b)
    if (!held && b.room === 0) return void ((room.style.height = "0px"), (room.hidden = true))
    const height = b.box.clientHeight
    const shown = room.hidden ? 0 : offsetOf(b.box, room)
    const next = held ? height : Math.min(b.room, Math.max(0, b.box.scrollTop + height - shown))
    b.room = next
    room.style.height = `${next}px`
    room.hidden = next === 0
    // Anchoring misses a sticky header that grows, and a node above that leaves as the anchor is drawn again.
    const p = b.placed
    if (held && p.type === "Top" && b.readerAt < p.at) toTop(b, p.root, p.field, p.at)
  }

  const elsewhere = (b: Box): void => {
    if (b.placed.type === "Top") b.resized.disconnect()
    b.placed = ELSEWHERE
  }

  // Writes only a box away from its place, so the same layout changes nothing; the box scrolls by whole pixels.
  const toTop = (b: Box, root: Element, field: string, at: number): void => {
    const node = nodeOf(root, field)
    if (node === null) return
    if (b.placed.type === "Elsewhere" && page(b.box)) b.resized.observe(document.body)
    const shown = shownAt(b.box, node)
    const from = b.box.scrollTop
    if (Math.abs(shown - covered(b.box)) >= 1) b.box.scrollTop += shown - covered(b.box)
    b.placed = { type: "Top", root, field, at, y: shown - (b.box.scrollTop - from) }
  }

  // One listener for every box, so none on `window` holds a box after its page is gone; a scroll key moves the box
  // around the key's target, or the document's.
  let listening = false
  const keyed = (): void => {
    if (listening) return
    listening = true
    window.addEventListener("keydown", e => {
      const t = e.target
      if (!KEYS.has(e.key) || (t instanceof HTMLElement && t.closest("input, textarea, select, [contenteditable]") !== null)) return
      const now = performance.now()
      for (let n = t instanceof Element ? t : null; n !== null; n = n.parentElement) {
        const b = boxes.get(n)
        if (b !== undefined) b.readerAt = now
      }
      const doc = document.scrollingElement
      const b = doc === null ? undefined : boxes.get(doc)
      if (b !== undefined) b.readerAt = now
    })
  }

  const of_ = (box: Element): Box => {
    const known = boxes.get(box)
    if (known !== undefined) return known
    const b: Box = {
      box,
      readerAt: Number.NEGATIVE_INFINITY,
      room: 0,
      last: { by: "page", at: "bottom" },
      placed: ELSEWHERE,
      resized: new ResizeObserver(() => fit(b)),
      ears: new Set(),
    }
    const read = (): void => void (b.readerAt = performance.now())
    const on = target(box)
    on.addEventListener("wheel", read, { passive: true })
    on.addEventListener("touchmove", read, { passive: true })
    on.addEventListener("pointerdown", e => (e.target === box ? read() : undefined), { passive: true })
    keyed()
    on.addEventListener("scroll", () => {
      const by: Mover = performance.now() - b.readerAt < READER_MS ? "reader" : "page"
      if (by === "reader") read()
      // A scroll that leaves the node where it stood on the box is the browser anchoring it; one that moves it is the
      // reader's, a focus's or a find's, and ends the hold. What anchoring misses moves the node without a scroll.
      const p = b.placed
      const node = p.type === "Top" ? nodeOf(p.root, p.field) : null
      if (p.type === "Top" && node !== null && Math.abs(shownAt(box, node) - p.y) >= 1) elsewhere(b)
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

  // Read before a step's patches; `keep` restores it after them.
  export type Anchor = { readonly box: Element; readonly fromEnd: number }
  export const anchor = (box: Element): Anchor => ({ box, fromEnd: box.scrollHeight - box.scrollTop })

  // A page's move is the page's: it ends the run of scrolls counted as the reader's for following one.
  export const keep = (p: Anchor): void => {
    of_(p.box).readerAt = Number.NEGATIVE_INFINITY
    p.box.scrollTop = p.box.scrollHeight - p.fromEnd
  }

  // Next frame, and only if the reader has not moved the box since: the compositor may be scrolling for a wheel the page
  // has not heard yet. `Keep` is `run.start`'s.
  export const move = (box: Element, root: Element, to: Exclude<Scroll, { readonly type: "Keep" }>): void => {
    const b = of_(box)
    const asked = performance.now()
    requestAnimationFrame(() => {
      if (b.readerAt >= asked) return
      b.readerAt = Number.NEGATIVE_INFINITY
      elsewhere(b)
      fit(b)
      switch (to.type) {
        case "End":
          return void (box.scrollTop = box.scrollHeight)
        case "Top":
          return toTop(b, root, to.field, performance.now())
        case "Into":
          return nodeOf(root, to.field)?.scrollIntoView({ block: "center" })
      }
    })
  }
}
export default scroll
