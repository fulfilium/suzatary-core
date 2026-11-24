// The host of the tiles (`app/pages/src/desk.rs`). `draw` makes the DOM equal to the desk's drawing: one element per
// tile, crumb row and divider, keyed as the drawing names them, each placed by its rectangle's share of the room, and a
// style or a row's crumbs written only where they differ, so the same drawing changes nothing. A tile's element is
// never moved, so no page loses its scroll or its focus. Where a page stands is the desk's; what the host reads of the
// window is the room the tiles stand in (`#view`'s width, the window's height) and whether the window's chrome is the
// wide one (`room`).
//
// Every act on the tree goes through the desk: a link followed in a tile and its page's `Go` open from that tile, a
// divider's drag, tap and keys resize it, and the address the host hands the history is the desk's print.

import type { Area, Crumbs, Drawing, Drawn, Line, Notch, Opens } from "../wire/generated/pages"
import type { Desk } from "../pages/views"
import type { Mounted } from "../view/page"
import { backButton, inert } from "../view/page"
import { el } from "../view/dom"
import { chevronIcon } from "../view/icons"
import pages from "../pages/table"
import type program from "../pages/program"
import { type Address, hrefOf, hrefOfAddress, parseAddress, parseRoute, type Route } from "./route"
import address from "./address"
import app from "../app/app"
import nav from "./nav"
import soon from "../painter/soon"
import tokens from "../view/tokens"

export namespace tiles {
  // A tile on screen. `key` is the arrangement's id of the leaf it draws now: an address read again mints new ids,
  // and a live tile drawing the same page, or one its page takes on itself (`pages.absorbs`), is kept under the new.
  type Tile = {
    key: number
    route: Route
    opens: Opens
    // The tile's element, which its page draws into, and its part of the window's layer, where its page draws what it
    // marks as the window's: never inside the tile, which is a query container, and older engines make such an element
    // the containing block of its fixed children.
    readonly root: HTMLElement
    readonly layer: HTMLElement
    readonly heard: Set<() => void>
    mounted: Mounted
    // The style last written on `root`.
    placed: string
    // What the person last pressed in it: a click or a tap, which focuses nothing in every browser.
    pressed: Element | null
  }

  // A crumb row or a divider, with the style and the crumbs last written.
  type Mark = { readonly root: HTMLElement; placed: string; drew: string }
  // A mark the drawing names: `drew` is what `fill` writes, compared before it is written again.
  type Want = { readonly key: string; readonly rect: Area; readonly drew: string; readonly make: () => HTMLElement; readonly fill: (e: HTMLElement) => void }

  // Nothing drawn yet; the tiles of an address; or a page that could not load, drawn alone until the address moves.
  type Shown = { readonly type: "Waiting" } | { readonly type: "Tiles"; readonly here: Address } | { readonly type: "Failed" }

  export type Host = {
    // The address moved. `fresh` mounts every page again (a switch of chain); `again` (Back, Forward, a resize) leaves a
    // page drawn alone where the log puts it rather than at its top. Answers the pages drawn anew.
    readonly go: (to: Address, fresh: boolean, again: boolean) => Promise<readonly HTMLElement[]>
    // Whether the window's chrome is the wide one, which the secretary's panel reads as the rest of the chrome does: the
    // same media query as every wide rule of `style.css`, at the token they are pinned to.
    readonly room: () => Opens
    // The routes drawn now.
    readonly drawn: () => readonly Route[]
    // The focused workspace's home, which the bar lights, as the desk's tree holds it; none before the first address.
    readonly workspace: () => Route | null
    // `said` alone, until the address moves.
    readonly failed: (said: HTMLElement) => void
    readonly destroy: () => void
  }

  // A crumb names its page: a workspace's home by the bar's word, any other page by what it is.
  const NOUNS: { readonly [K in Route["type"]]?: string } = {
    todo: "Todo",
    message: "Message",
    thread: "Thread",
    briefing: "Briefing",
    conversation: "Conversation",
    newtopic: "New topic",
    memory: "Memory",
  }
  const label = (href: string): string => {
    const route = parseRoute(href)
    return NOUNS[route.type] ?? nav.DESTINATIONS.find(d => d.route.type === nav.activeFor(route))?.label ?? "Back"
  }

  // A row's crumbs. The one just before the drawn ones is Back where Back lands on its page: the history's previous entry
  // is that page, or there is none and Back puts that page in place. Any other before, and one after, is a link to its
  // page, so a crumb goes where it says.
  const crumbsOf = (row: Crumbs): HTMLElement[] => {
    const log = app.now().log
    const previous = log.entries[log.at - 1]?.address.route
    const last = row.crumbs.map(c => c.at).lastIndexOf("Before")
    const back = previous === undefined || row.crumbs[last]?.href === hrefOf(previous) ? last : -1
    return row.crumbs.flatMap((c, i) => {
      switch (c.at) {
        case "Before":
          return [i === back ? backButton(label(c.href), parseRoute(c.href)) : el("a", { class: "back", href: c.href }, chevronIcon(), label(c.href))]
        case "After":
          return [el("a", { class: "crumbafter", href: c.href }, label(c.href), chevronIcon())]
        case "Drawn":
          return []
      }
    })
  }

  // The stop of `l` nearest `at`, a length along its axis in the room's frame, from the middle of its gap.
  const nearest = (l: Line, at: number): number => {
    const half = (l.axis === "Across" ? l.rect.w : l.rect.h) / 2
    let best = l.now
    l.stops.forEach((s, i) => (Math.abs(s.at + half - at) < Math.abs((l.stops[best]?.at ?? 0) + half - at) ? (best = i) : undefined))
    return best
  }

  // Where a divider stands, in a person's words: the part of the two pages the first takes. Left to right, top to bottom;
  // a right-to-left page is not handled.
  const PARTS: Readonly<Record<string, string>> = { "1:3": "a quarter", "1:2": "a third", "2:3": "two fifths", "1:1": "half", "3:2": "three fifths", "2:1": "two thirds", "3:1": "three quarters" }
  const told = (l: Line): string => {
    const [a, b] = l.stops[l.now]?.shares ?? [1, 1]
    // Weights of 1 to 3 share no factor but when equal.
    const g = a === b ? a : 1
    const part = PARTS[`${a / g}:${b / g}`] ?? `${a} to ${b}`
    return `${l.axis === "Across" ? "The left page" : "The upper page"} takes ${part}`
  }

  const pct = (n: number, of: number): string => `${((100 * n) / Math.max(1, of)).toFixed(3)}%`

  // Writes `style` on `e` where it differs from what was written last.
  const restyle = (e: HTMLElement, was: string, style: string): string => {
    if (style !== was) {
      if (style === "") e.removeAttribute("style")
      else e.setAttribute("style", style)
    }
    return style
  }

  export const install = (view: HTMLElement, layers: HTMLElement, desk: () => Promise<Desk>, moved: () => void): Host => {
    let live: Tile[] = []
    let drawing: Drawing = { scroll: "Document", tiles: [], rows: [], dividers: [] }
    let room = { w: 0, h: 0 }
    let shown: Shown = { type: "Waiting" }
    // The focused workspace's home, read from the desk's tree as an address moves it, before any page of it has loaded.
    let workspace: Route | null = null
    let latest = 0
    const lines = el("div", { class: "tilelines" })
    const marks = new Map<string, Mark>()
    // Each divider's element's line now, which its drag, tap and keys read.
    const dividers = new Map<HTMLElement, Line>()
    const wide = window.matchMedia(`(min-width: ${tokens.measure.wide / 16}rem)`)

    // The room in pixels at the 16px root, floored as a media query compares: a person's larger text moves the
    // arrangement the way `rem` moves CSS.
    // `main`'s box, read unrounded and multiplied before it is divided (at a zoom of 110% a room of 959.82 px is not 960),
    // as if the document had no classic scrollbar. One tile alone may scroll the document and tiles together never do,
    // so this is the room tiles together get, and the tiles a window draws never hang on whether its document scrolls.
    const roomOf = (): { readonly w: number; readonly h: number } => {
      const root = parseFloat(getComputedStyle(document.documentElement).fontSize)
      const bar = window.innerWidth - document.documentElement.clientWidth
      return { w: Math.floor(((view.getBoundingClientRect().width + bar) * 16) / root), h: Math.floor((window.innerHeight * 16) / root) }
    }

    const lone = (): boolean => drawing.scroll === "Document"
    const page_ = (): Element => document.scrollingElement ?? document.documentElement
    const box = (t: Tile) => (): Element => (lone() ? page_() : t.root)

    const tell = (t: Tile, opens: Opens): void => {
      if (t.opens === opens) return
      t.opens = opens
      t.heard.forEach(f => f())
    }

    const at = (r: Area): string =>
      lone() ? "" : `left:${pct(r.x, room.w)};top:${pct(r.y, room.h)};width:${pct(r.w, room.w)};height:${pct(r.h, room.h)}`

    // A tile's place, and where its page stands inside it as padding in rem: none, for every page that flows.
    const placeOf = (d: Drawn): string => {
      const [t, r, b, l] = [d.page.y - d.rect.y, d.rect.x + d.rect.w - d.page.x - d.page.w, d.rect.y + d.rect.h - d.page.y - d.page.h, d.page.x - d.rect.x]
      const inset = t === 0 && r === 0 && b === 0 && l === 0 ? "" : `padding:${t / 16}rem ${r / 16}rem ${b / 16}rem ${l / 16}rem`
      return [at(d.rect), inset].filter(x => x !== "").join(";")
    }

    // The marks named `want`, made by `make` where missing and dropped where no longer named.
    const mark = (want: readonly Want[]): void => {
      const named = new Set(want.map(w => w.key))
      for (const [k, m] of marks) if (!named.has(k)) (m.root.remove(), marks.delete(k))
      want.forEach(({ key, rect, drew, make, fill }, i) => {
        const m = marks.get(key) ?? { root: make(), placed: "", drew: "" }
        marks.set(key, m)
        m.placed = restyle(m.root, m.placed, at(rect))
        if (m.drew !== drew) (fill(m.root), (m.drew = drew))
        if (lines.children[i] !== m.root) lines.insertBefore(m.root, lines.children[i] ?? null)
      })
    }

    // What stands still in a tile whose scroller changes, which the person sees: what they last pressed in it while it is
    // on screen (the row clicked), else the element crossing the top of what the tile shows, else its page.
    const anchor = (t: Tile): Element => {
      const box = t.root.getBoundingClientRect()
      const shown = (e: Element): boolean => {
        const r = e.getBoundingClientRect()
        return r.bottom > Math.max(box.top, 0) && r.top < Math.min(box.bottom, window.innerHeight)
      }
      if (t.pressed !== null && t.pressed.isConnected && t.root.contains(t.pressed) && shown(t.pressed)) return t.pressed
      const top = document.elementsFromPoint(box.left + box.width / 2, Math.max(box.top, 0) + 1).find(e => e !== t.root && t.root.contains(e))
      return top ?? t.root.firstElementChild ?? t.root
    }

    // Makes the DOM the drawing's. A kept tile whose scroller changes (alone, it scrolls the document; beside another, it
    // scrolls itself) keeps its anchor where it stood on the screen, though the page reflows to its new width.
    const draw = (was: Drawing["scroll"]): void => {
      const turns = was !== drawing.scroll
      const stood = new Map(turns ? live.filter(t => t.root.isConnected).map(t => [t, { at: anchor(t), top: anchor(t).getBoundingClientRect().top }]) : [])
      view.classList.toggle("tiled", !lone())
      for (const t of live) {
        const d = drawing.tiles.find(x => x.key === t.key)
        if (d === undefined) continue
        t.placed = restyle(t.root, t.placed, placeOf(d))
        tell(t, d.opens)
      }
      mark([
        ...drawing.rows.map((r: Crumbs): Want => ({
          key: `row ${r.node.join(".")}`,
          rect: r.rect,
          drew: JSON.stringify(r.crumbs),
          make: () => el("nav", { class: "crumbs", "aria-label": "Where this page stands" }),
          fill: e => e.replaceChildren(...crumbsOf(r)),
        })),
        ...drawing.dividers.map((l: Line): Want => ({
          key: `line ${l.node.join(".")} ${l.before}`,
          rect: l.rect,
          // What its attributes say; the stops' places are read from `dividers` when it is dragged.
          drew: `${l.axis} ${l.now} ${l.stops.length} ${told(l)}`,
          make: divider,
          fill: e => {
            dividers.set(e, l)
            e.setAttribute("data-axis", l.axis)
            e.setAttribute("aria-orientation", l.axis === "Across" ? "vertical" : "horizontal")
            e.setAttribute("aria-valuemin", "0")
            e.setAttribute("aria-valuemax", String(l.stops.length - 1))
            e.setAttribute("aria-valuenow", String(l.now))
            e.setAttribute("aria-valuetext", told(l))
            // A divider with one stop has nowhere to go: no handle, and nothing to drag.
            e.toggleAttribute("data-fixed", l.stops.length < 2)
          },
        })),
      ])
      for (const [e, l] of dividers) {
        const now = drawing.dividers.find(x => x.node.join(".") === l.node.join(".") && x.before === l.before)
        if (now === undefined) dividers.delete(e)
        else dividers.set(e, now)
      }
      // The lines' layer is last, above the tiles, so a divider's handle is taken where it reaches over a tile's edge.
      const roots = new Set<Element>([lines, ...live.map(t => t.root)])
      for (const n of [...view.children]) if (!roots.has(n)) n.remove()
      live.forEach((t, i) => (view.children[i] === t.root ? undefined : view.insertBefore(t.root, view.children[i] ?? null)))
      if (view.lastElementChild !== lines) view.appendChild(lines)
      for (const [t, { at, top }] of stood) box(t)().scrollTop += at.getBoundingClientRect().top - top
    }

    // A page opened from the tile `t`, by the desk's one rule; the address of the tree it makes is a new entry of the
    // history (`Went`) or takes the place of the one the reader is at (`Replaced`).
    const open = async (t: Tile, href: string, how: "Went" | "Replaced"): Promise<void> => {
      const d = await desk()
      d.open(t.key, href)
      const to = parseAddress(d.address())
      if (how === "Went") address.went(to)
      else address.replaced(to)
    }

    const newTile = (d: Drawn): Tile => {
      const t: Tile = {
        key: d.key,
        route: parseRoute(d.href),
        opens: d.opens,
        root: el("div", { class: "tile" }),
        layer: el("div", { class: "tilelayer" }),
        heard: new Set(),
        mounted: inert,
        placed: "",
        pressed: null,
      }
      t.root.addEventListener("pointerdown", e => void (t.pressed = e.target instanceof Element ? e.target : null), { passive: true })
      const follow = (e: MouseEvent): void => {
        const href = address.followed(e)
        if (href === null) return
        e.preventDefault()
        void open(t, href, "Went")
      }
      t.root.addEventListener("click", follow)
      t.layer.addEventListener("click", follow)
      for (const on of [t.root, t.layer])
        address.hearOpens(on, (href, e) => {
          e.preventDefault()
          void open(t, href, "Went")
        })
      layers.appendChild(t.layer)
      return t
    }

    const leave = (t: Tile): void => {
      t.mounted.destroy()
      t.layer.remove()
    }

    const placedOf = (t: Tile): program.Placed => ({
      opens: () => t.opens,
      hear: moved => (t.heard.add(moved), () => void t.heard.delete(moved)),
      go: (href, how) => void open(t, href, how),
    })

    // The divider `l` moved to its stop `i`: the same tiles, placed again.
    const snap = async (l: Line, i: number): Promise<void> => {
      const stop: Notch | undefined = l.stops[i]
      if (stop === undefined || i === l.now) return
      const d = await desk()
      d.resize(l.before, l.after, stop.shares[0], stop.shares[1])
      draw(commit(arrange(d, room)))
    }

    // A length along `l`'s axis in the room's frame, from where a pointer stands in the window.
    const along = (l: Line, e: PointerEvent): number => {
      const box = view.getBoundingClientRect()
      const root = parseFloat(getComputedStyle(document.documentElement).fontSize)
      return ((l.axis === "Across" ? e.clientX - box.left : e.clientY - box.top) * 16) / root
    }

    // A divider: dragged by a mouse or a finger, it stands at the stop nearest the pointer, and a tap steps it to the next
    // stop; as a separator, the arrow keys step it and Home and End take it to the first and last.
    const divider = (): HTMLElement => {
      const e = el("div", { class: "tileline", role: "separator", tabindex: "0", "aria-label": "Resize the tiles either side" })
      // No press; a press not yet moved past a tap's slop, by its pointer, from where it went down and whether it went down
      // on the handle; or a drag by its pointer. A second finger is not the press.
      type Press =
        | { readonly type: "Up" }
        | { readonly type: "Down"; readonly id: number; readonly x: number; readonly y: number; readonly handle: boolean }
        | { readonly type: "Dragging"; readonly id: number }
      let press: Press = { type: "Up" }
      const letGo = (): void => {
        press = { type: "Up" }
        e.classList.remove("dragged")
      }
      // Within the handle, the tap target at the divider's middle; the rest of the gap is a mouse's to drag.
      const onHandle = (p: PointerEvent): boolean => {
        const r = e.getBoundingClientRect()
        const reach = parseFloat(getComputedStyle(document.documentElement).fontSize) * (tokens.measure.tap / 16) / 2
        return Math.abs(p.clientX - (r.left + r.width / 2)) <= reach && Math.abs(p.clientY - (r.top + r.height / 2)) <= reach
      }
      e.addEventListener("pointerdown", p => {
        const l = dividers.get(e)
        if (p.button !== 0 || press.type !== "Up" || l === undefined || l.stops.length < 2) return
        // No text is selected while the divider is dragged.
        p.preventDefault()
        e.setPointerCapture(p.pointerId)
        press = { type: "Down", id: p.pointerId, x: p.clientX, y: p.clientY, handle: onHandle(p) }
        e.classList.add("dragged")
      })
      e.addEventListener("pointermove", p => {
        const l = dividers.get(e)
        if (press.type === "Up" || p.pointerId !== press.id || l === undefined) return
        if (press.type === "Down" && Math.hypot(p.clientX - press.x, p.clientY - press.y) <= 4) return
        press = { type: "Dragging", id: press.id }
        const to = nearest(l, along(l, p))
        if (to !== l.now) void snap(l, to)
      })
      e.addEventListener("pointerup", p => {
        const l = dividers.get(e)
        if (press.type === "Up" || p.pointerId !== press.id) return
        const tapped = press.type === "Down" && press.handle
        letGo()
        if (tapped && l !== undefined) void snap(l, (l.now + 1) % l.stops.length)
      })
      e.addEventListener("pointercancel", p => (press.type !== "Up" && p.pointerId === press.id ? letGo() : undefined))
      e.addEventListener("lostpointercapture", p => (press.type !== "Up" && p.pointerId === press.id ? letGo() : undefined))
      e.addEventListener("keydown", k => {
        const l = dividers.get(e)
        if (l === undefined) return
        const to =
          k.key === "ArrowLeft" || k.key === "ArrowUp" ? l.now - 1
          : k.key === "ArrowRight" || k.key === "ArrowDown" ? l.now + 1
          : k.key === "Home" ? 0
          : k.key === "End" ? l.stops.length - 1
          : null
        if (to === null) return
        k.preventDefault()
        void snap(l, Math.max(0, Math.min(l.stops.length - 1, to)))
      })
      return e
    }

    type Arranged = { readonly room: { readonly w: number; readonly h: number }; readonly drawing: Drawing }
    const arrange = (d: Desk, at: { readonly w: number; readonly h: number }): Arranged => ({ room: at, drawing: JSON.parse(d.drawing(at.w, at.h)) })
    // The drawing the DOM is made to; answers the scroller it had.
    const commit = (a: Arranged): Drawing["scroll"] => {
      const was = drawing.scroll
      room = a.room
      drawing = a.drawing
      return was
    }

    const unmount = (): void => {
      live.forEach(leave)
      live = []
      marks.clear()
      dividers.clear()
      lines.replaceChildren()
    }

    const go = async (to: Address, fresh: boolean, again: boolean): Promise<readonly HTMLElement[]> => {
      const turn = ++latest
      const d = await desk()
      if (turn !== latest) return []
      if (shown.type === "Failed") view.replaceChildren()
      shown = { type: "Tiles", here: to }
      d.go(hrefOfAddress(to))
      workspace = parseRoute(d.workspace())
      moved()
      // An address the desk reads otherwise than it was written (a tree it cannot read) is shown as the desk prints it.
      const printed = parseAddress(d.address())
      if (hrefOfAddress(printed) !== hrefOfAddress(to)) {
        address.replaced(printed)
        return []
      }
      const next = arrange(d, roomOf())
      const taken = new Set<Tile>()
      const kept = new Map<number, Tile>()
      const made: Tile[] = []
      for (const want of next.drawing.tiles) {
        const now = parseRoute(want.href)
        const t = fresh ? undefined : live.find(t => !taken.has(t) && (hrefOf(t.route) === want.href || pages.absorbs(t.route, now)))
        if (t === undefined) {
          made.push(newTile(want))
          continue
        }
        taken.add(t)
        t.key = want.key
        t.route = now
        kept.set(want.key, t)
      }
      const gone = live.filter(t => !taken.has(t))
      // The pages to draw are loaded first, so a page left keeps its place until the next can draw.
      const loads = made.map(t => Promise.resolve(pages.load(t.route, { box: box(t), placed: placedOf(t), layer: t.layer })))
      const draws = await Promise.all(loads)
      if (turn !== latest) {
        made.forEach(t => t.layer.remove())
        return []
      }
      gone.forEach(leave)
      live = next.drawing.tiles.flatMap(w => kept.get(w.key) ?? made.filter(t => t.key === w.key))
      // Arranged again as it is drawn, for the room its tiles were chosen for: a divider moved while the pages loaded moved
      // the shares, never the tiles; a room that moved meanwhile is the observer's to draw.
      draw(commit(arrange(d, next.room)))
      // Freeing the pages left is a long task of its own; each page drawn takes a task of its own after it.
      for (const [i, t] of made.entries()) {
        await new Promise<void>(done => soon.run(done))
        if (turn !== latest) return []
        const draw = draws[i]
        if (draw !== undefined) t.mounted = draw(t.root)
      }
      // A page new to the screen, alone, starts at its top, unless the log says where it was.
      if (!again && lone() && made.length > 0) window.scrollTo(0, 0)
      return made.map(t => t.root)
    }

    // The room moved: the window, the open panel, the gutters. A tile that comes or goes is a page mounted or left,
    // just like a move of the address; otherwise the same tiles are placed again.
    const resized = async (): Promise<void> => {
      // A page that could not load draws no tiles, but the chrome beside it still reads the window.
      if (shown.type !== "Tiles") return moved()
      const now = roomOf()
      if (now.w === room.w && now.h === room.h) return
      const d = await desk()
      const same = (x: Drawing): string => JSON.stringify(x.tiles.map(t => [t.key, t.href]))
      const next = arrange(d, now)
      if (same(next.drawing) === same(drawing)) draw(commit(next))
      else await go(shown.here, false, true)
      moved()
    }

    let sizing = 0
    const resize = (): void => {
      if (sizing !== 0) return
      sizing = requestAnimationFrame(() => {
        sizing = 0
        void resized()
      })
    }
    new ResizeObserver(resize).observe(view)
    window.addEventListener("resize", resize)

    return {
      go,
      room: () => (wide.matches ? "Beside" : "Over"),
      drawn: () => live.map(t => t.route),
      workspace: () => (shown.type === "Tiles" ? workspace : null),
      failed: said => {
        latest += 1
        shown = { type: "Failed" }
        unmount()
        view.classList.remove("tiled")
        view.replaceChildren(said)
      },
      destroy: () => {
        latest += 1
        shown = { type: "Waiting" }
        unmount()
        view.replaceChildren()
      },
    }
  }
}
export default tiles
