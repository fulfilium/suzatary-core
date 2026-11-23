// The host of the tiles (`app/pages/src/desk.rs`). `draw` makes the DOM equal to the desk's drawing: one element per
// tile, crumb row and divider, keyed as the drawing names them, each placed by its rectangle's share of the room, and a
// style or a row's crumbs written only where they differ, so the same drawing changes nothing. A tile's element is
// never moved, so no page loses its scroll or its focus. Where a page stands is the desk's; what the host reads of the
// window is the room the tiles stand in (`#view`'s width, the window's height) and whether the window's chrome is the
// wide one (`room`).

import type { Area, Crumbs, Drawing, Drawn, Line, Opens } from "../wire/generated/pages"
import type { Desk } from "../pages/views"
import type { Mounted } from "../view/page"
import { backButton, inert } from "../view/page"
import { el } from "../view/dom"
import { chevronIcon } from "../view/icons"
import pages from "../pages/table"
import type program from "../pages/program"
import { hrefOf, parseRoute, type Route } from "./route"
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
    // The tile's element, which its page draws into.
    readonly root: HTMLElement
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
  type Shown = { readonly type: "Waiting" } | { readonly type: "Tiles"; readonly here: Route } | { readonly type: "Failed" }

  export type Host = {
    // The address moved. `fresh` mounts every page again (a switch of chain); `again` (Back, Forward, a resize) leaves a
    // page drawn alone where the log puts it rather than at its top. Answers the pages drawn anew.
    readonly go: (route: Route, fresh: boolean, again: boolean) => Promise<readonly HTMLElement[]>
    // Whether the window's chrome is the wide one: what the secretary's panel reads until it is the workspace's
    // pinned tile. The same media query as every wide rule of `style.css`, at the token they are pinned to.
    readonly room: () => Opens
    // The routes drawn now.
    readonly drawn: () => readonly Route[]
    // `said` alone, until the address moves.
    readonly failed: (said: HTMLElement) => void
    readonly destroy: () => void
  }

  const label = (href: string): string => {
    const route = parseRoute(href)
    return nav.DESTINATIONS.find(d => d.route.type === nav.activeFor(route))?.label ?? "Back"
  }

  // A row's crumbs: those before the drawn ones go back to their page, the way a page's own Back did; those after go to it.
  const crumbsOf = (row: Crumbs): HTMLElement[] =>
    row.crumbs.flatMap(c => {
      switch (c.at) {
        case "Before":
          return [backButton(label(c.href), parseRoute(c.href))]
        case "After":
          return [el("a", { class: "crumbafter", href: c.href }, label(c.href), chevronIcon())]
        case "Drawn":
          return []
      }
    })

  const pct = (n: number, of: number): string => `${((100 * n) / Math.max(1, of)).toFixed(3)}%`

  // Writes `style` on `e` where it differs from what was written last.
  const restyle = (e: HTMLElement, was: string, style: string): string => {
    if (style !== was) {
      if (style === "") e.removeAttribute("style")
      else e.setAttribute("style", style)
    }
    return style
  }

  export const install = (view: HTMLElement, desk: () => Promise<Desk>, moved: () => void): Host => {
    let live: Tile[] = []
    let drawing: Drawing = { scroll: "Document", tiles: [], rows: [], dividers: [] }
    let room = { w: 0, h: 0 }
    let shown: Shown = { type: "Waiting" }
    let latest = 0
    const lines = el("div", { class: "tilelines" })
    const marks = new Map<string, Mark>()
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
          drew: l.axis,
          make: () => el("div", { class: "tileline" }),
          fill: e => e.setAttribute("data-axis", l.axis),
        })),
      ])
      const roots = new Set<Element>([lines, ...live.map(t => t.root)])
      for (const n of [...view.children]) if (!roots.has(n)) n.remove()
      if (view.firstElementChild !== lines) view.insertBefore(lines, view.firstElementChild)
      live.forEach((t, i) => (view.children[i + 1] === t.root ? undefined : view.insertBefore(t.root, view.children[i + 1] ?? null)))
      for (const [t, { at, top }] of stood) box(t)().scrollTop += at.getBoundingClientRect().top - top
    }

    const newTile = (d: Drawn): Tile => {
      const t: Tile = { key: d.key, route: parseRoute(d.href), opens: d.opens, root: el("div", { class: "tile" }), heard: new Set(), mounted: inert, placed: "", pressed: null }
      t.root.addEventListener("pointerdown", e => void (t.pressed = e.target instanceof Element ? e.target : null), { passive: true })
      return t
    }

    const placedOf = (t: Tile): program.Placed => ({
      opens: () => t.opens,
      hear: moved => (t.heard.add(moved), () => void t.heard.delete(moved)),
    })

    type Arranged = { readonly room: { readonly w: number; readonly h: number }; readonly drawing: Drawing }
    const arrange = (d: Desk): Arranged => {
      const at = roomOf()
      return { room: at, drawing: JSON.parse(d.drawing(at.w, at.h)) }
    }
    // The drawing the DOM is made to; answers the scroller it had.
    const commit = (a: Arranged): Drawing["scroll"] => {
      const was = drawing.scroll
      room = a.room
      drawing = a.drawing
      return was
    }

    const unmount = (): void => {
      live.forEach(t => t.mounted.destroy())
      live = []
      marks.clear()
      lines.replaceChildren()
    }

    const go = async (route: Route, fresh: boolean, again: boolean): Promise<readonly HTMLElement[]> => {
      const turn = ++latest
      const d = await desk()
      if (turn !== latest) return []
      if (shown.type === "Failed") view.replaceChildren()
      shown = { type: "Tiles", here: route }
      d.go(hrefOf(route))
      const next = arrange(d)
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
      const loads = made.map(t => Promise.resolve(pages.load(t.route, { box: box(t), placed: placedOf(t) })))
      const draws = await Promise.all(loads)
      if (turn !== latest) return []
      gone.forEach(t => t.mounted.destroy())
      live = next.drawing.tiles.flatMap(w => kept.get(w.key) ?? made.filter(t => t.key === w.key))
      draw(commit(next))
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
      const next = arrange(d)
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
