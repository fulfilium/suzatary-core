import type { Child } from "./dom"
import { el } from "./dom"
import marks from "./marks"
import placeholder from "./placeholder"
import typed from "./typed"
import type { Mounted } from "./page"
import { backButton } from "./page"
import type { Screen } from "../shell/layout"
import type { Route } from "../shell/route"
import lo from "../algebra/lo"
import type reading from "../algebra/reading"
import source from "../algebra/source"
import data from "../algebra/data"
import { when } from "./format"
import app from "../app/app"

// A page declared once: the reading it is a view of, how its own events move the value, how it looks. It loads
// nothing: what it reads is held by the tab (a replica, a register) or asked of the box in one place (`asked`).
export namespace frame {
  export type Event<S, E> = { readonly type: "read"; readonly read: lo.T<S> } | { readonly type: "own"; readonly event: E }

  export type Page<S, E = never, K extends string = never> = {
    readonly className: string
    readonly wide?: boolean
    readonly title: string | ((v: S | null) => string)
    // A masthead drawn from the value replaces the title's once there is one.
    readonly masthead?: "destination" | ((v: S, at: At<K>) => Child)
    readonly back?: { readonly label: string; readonly fallback: Route }
    readonly legend?: (v: S) => string
    // Read at mount: what the page is a view of.
    readonly reads: () => reading.Reading<S>
    readonly step?: (v: S, e: E) => S
    // What of the page's own state outlives a new reading of what it reads; by default nothing does.
    readonly keeps?: (was: S, read: S) => S
    readonly listen?: (dispatch: (e: E) => void, again: () => void) => () => void
    // What the page asks of the world given what it shows, run once a value is drawn; what comes back is an event.
    readonly effects?: (v: S, fx: Effects<E>) => void
    readonly islands?: (dispatch: (e: E) => void, again: () => void) => { readonly [k in K]: Island<S> }
    // What stands around the column, drawn once: a column beside it.
    readonly around?: (column: HTMLElement, at: At<K>) => HTMLElement
    // A route change the page takes on itself, staying mounted with its value, its scroll and its focus.
    readonly absorb?: (screen: Screen, dispatch: (e: E) => void) => boolean
    // The shape of what the page draws, for the placeholder shown while it waits; a page of text by default.
    readonly shape?: placeholder.Shape
    // Whether a value read still waits on more before it has anything to show (the box asked, the chain folding): the
    // frame's placeholder stays, the one node since the page opened.
    readonly waits?: (v: S) => boolean
    readonly destroy?: () => void
    readonly now?: (dispatch: (e: E) => void) => Child
    readonly view: (v: S, dispatch: (e: E) => void, again: () => void, at: At<K>) => Child
  }

  // A part mounted once with the page and handed every value; it keeps only what the DOM owns (scroll, focus, a draft, streamed nodes).
  export type Island<S> = { readonly node: Node; readonly update: (v: S) => void; readonly destroy?: () => void }
  export type At<K extends string> = { readonly [k in K]?: Node }

  type Parts = {
    readonly column: HTMLElement
    readonly back: HTMLElement | null
    readonly heading: HTMLElement
    readonly head: HTMLElement
    readonly legend: HTMLElement
    readonly top: HTMLElement
    readonly status: HTMLElement
    // Made once, so a redraw while the page waits does not start its 300 ms again.
    readonly waiting: HTMLElement
    readonly body: HTMLElement
  }
  export type Effects<E> = { readonly again: () => void; readonly dispatch: (e: E) => void }

  export const read = <S>(l: lo.T<S>): Event<S, never> => ({ type: "read", read: l })
  export const own = <E>(event: E): Event<never, E> => ({ type: "own", event })

  export const fold = <S, E, R>(alg: { read: (l: lo.T<S>) => R; own: (e: E) => R }, e: Event<S, E>): R =>
    e.type === "read" ? alg.read(e.read) : alg.own(e.event)
  export const mapOwn = <S, E, F>(f: (e: E) => F, e: Event<S, E>): Event<S, F> => (e.type === "own" ? own(f(e.event)) : e)

  // A value read replaces what is shown, keeping what the page keeps of its own; once a value is shown, a reading
  // on its way or failed leaves it. An own event before there is a value, or one that changes nothing, is nothing.
  export const step =
    <S, E>(ownStep: (v: S, e: E) => S, keeps?: (was: S, read: S) => S) =>
    (p: lo.T<S>, e: Event<S, E>): lo.T<S> =>
      fold<S, E, lo.T<S>>(
        {
          read: l =>
            l.type === "Val" ? lo.val(keeps === undefined || p.type !== "Val" ? l.v : keeps(p.v, l.v))
            : p.type === "Val" ? p
            : l,
          own: x => lo.bind(p, v => ((n: S) => (n === v ? p : lo.val(n)))(ownStep(v, x))),
        },
        e,
      )

  const nodes = (c: Child): (Node | string)[] =>
    c === null || c === undefined || c === false ? []
    : Array.isArray(c) ? c.flatMap(nodes)
    : [c instanceof Node ? c : String(c)]

  // Puts `kids` in `parent` in order and takes out the rest; a kept node already in order is never taken out.
  const place = (parent: Node, kids: readonly (Node | string)[], kept: ReadonlySet<Node>): void => {
    const want = kids.map(k => (typeof k === "string" ? document.createTextNode(k) : k))
    for (const c of [...parent.childNodes]) if (!kept.has(c) || !want.includes(c)) parent.removeChild(c)
    want.reduce<Node | null>((at, k) => (k === at ? k.nextSibling : (parent.insertBefore(k, at), at)), parent.firstChild)
  }

  const skeleton = <S, E, K extends string>(f: Page<S, E, K>): Parts => {
    const heading = el("h1", { class: f.masthead === "destination" ? "pagetitle destination" : "pagetitle" })
    return {
      column: el("div", { class: f.wide === true ? f.className : `column ${f.className}` }),
      back: f.back === undefined ? null : backButton(f.back.label, f.back.fallback),
      heading,
      head: marks.head(heading),
      legend: el("p", { class: "legend", hidden: true }),
      top: el("div", { class: "pagenow" }),
      status: el("div", { class: "legend" }),
      waiting: placeholder.view(f.shape ?? placeholder.LINES),
      body: el("div", { class: "pagebody" }),
    }
  }

  // What the status line says: the page waits, failed, shows what may be behind, or nothing.
  type Said = { readonly type: "Waiting" } | { readonly type: "Failed"; readonly e: string } | { readonly type: "Behind"; readonly behind: reading.Behind } | { readonly type: "Quiet" }

  const said = <S, E, K extends string>(f: Page<S, E, K>, p: lo.T<S>, stale: reading.Behind | null): Said =>
    lo.fold<S, Said>(p, {
      loading: () => ({ type: "Waiting" }),
      error: e => ({ type: "Failed", e }),
      val: v => (f.waits?.(v) === true ? { type: "Waiting" } : stale === null ? { type: "Quiet" } : { type: "Behind", behind: stale }),
    })

  // The status line alone, drawn on its own when only that moved, so a slow ask never draws the body again. The column
  // says whether the page waits, so its title can wait with it.
  const status = <E>(parts: Parts, now: Said, fx: Effects<E>): void => {
    const retry = (): HTMLElement => el("button", { type: "button", class: "retry", onclick: fx.again }, "Retry")
    const [cls, kids] = ((): readonly [string, (Node | string)[]] => {
      switch (now.type) {
        case "Waiting": return ["pagewait", [parts.waiting]]
        case "Failed": return ["problem", [`Could not load: ${now.e} `, retry()]]
        case "Behind": {
          const read = when(now.behind.since, app.now().zone)
          return ["legend stale", now.behind.why === null ? [`As of ${read}; asking the box again…`] : [`Could not refresh: ${now.behind.why}. This is what the tab last read, at ${read}. `, retry()]]
        }
        case "Quiet": return ["legend", []]
      }
    })()
    parts.status.className = cls
    parts.column.classList.toggle("pagewaiting", now.type === "Waiting")
    place(parts.status, kids, new Set([parts.waiting]))
    const shown = now.type !== "Quiet"
    if (shown && parts.status.parentNode !== parts.column) parts.column.insertBefore(parts.status, parts.body)
    if (!shown) parts.status.parentNode?.removeChild(parts.status)
  }

  // Idempotent: the parts show `p` whatever they showed before, and an island's node stays where it is.
  // `stale` is whether what is shown may be behind, said under the heading with when it was read, and a Retry when an
  // ask failed.
  const paint = <S, E, K extends string>(parts: Parts, f: Page<S, E, K>, p: lo.T<S>, stale: reading.Behind | null, fx: Effects<E>, at: At<K>, kept: ReadonlySet<Node>, carry: (to: readonly Node[]) => void): void => {
    const v = lo.withDefault(p, null)
    const mast = v === null || typeof f.masthead !== "function" ? [parts.head] : nodes(f.masthead(v, at))
    const legend = v === null || f.legend === undefined ? "" : f.legend(v)
    // A page with no legend, and a status with nothing to say, are out of the page rather than hidden in it.
    const now = said(f, p, stale)
    const lines = [...(f.legend === undefined ? [] : [parts.legend]), parts.top, ...(now.type === "Quiet" ? [] : [parts.status])]
    place(parts.column, [...(parts.back === null ? [] : [parts.back]), ...mast, ...lines, parts.body], kept)
    parts.heading.textContent = typeof f.title === "string" ? f.title : f.title(v)
    parts.legend.textContent = legend
    parts.legend.hidden = legend === ""
    status(parts, now, fx)
    const body = v === null ? [] : nodes(f.view(v, fx.dispatch, fx.again, at))
    // A body drawn again keeps what a person typed in it, whatever reading landed meanwhile.
    carry(body.filter(n => typeof n !== "string"))
    place(parts.body, body, kept)
  }

  // Settles once a page first draws what it read, or why it could not: what the shell's own readings wait for.
  let drew = (): void => {}
  export const firstDrawn: Promise<void> = new Promise(done => (drew = done))

  // Synchronous, so a caller swapping one page for another can destroy it without waiting.
  export const open =<S, E = never, K extends string = never>(root: HTMLElement, f: Page<S, E, K>): Mounted => {
    const ownStep = f.step ?? (v => v)
    const reads = f.reads()
    // A page opens on what its readings already hold, so a visit to a page read before draws its value first.
    let read0: lo.T<S> = reads.now()
    let stale: reading.Behind | null = reads.behind()
    const s = source.create<lo.T<S>, Event<S, E>>(read0, step(ownStep, f.keeps))
    const parts = skeleton(f)
    const carry = typed.keeper(parts.body)
    // What a listener hears before the first value is folded into it when it lands, in order, never dropped.
    const early: E[] = []
    const heard = (): void => {
      const l = reads.now()
      const t = reads.behind()
      if (!data.same(t, stale)) {
        stale = t
        if (l === read0) return status(parts, said(f, s.state(), stale), fx)
      }
      if (l === read0) return
      read0 = l
      s.dispatch(read(lo.isVal(s.state()) ? l : lo.map(l, v => early.splice(0).reduce(ownStep, v))))
    }
    const fx: Effects<E> = { again: reads.again, dispatch: e => (lo.isVal(s.state()) ? s.dispatch(own(e)) : void early.push(e)) }
    const made = f.islands?.(fx.dispatch, fx.again)
    const all: readonly Island<S>[] = made === undefined ? [] : Object.values<Island<S>>(made)
    const at: { -readonly [k in K]?: Node } = {}
    for (const k in made) at[k] = made[k].node
    const kept = new Set<Node>([...all.map(i => i.node), ...[parts.back, parts.head, parts.legend, parts.top, parts.status, parts.body].filter(n => n !== null)])
    root.replaceChildren(f.around?.(parts.column, at) ?? parts.column)
    parts.top.replaceChildren(...nodes(f.now?.(fx.dispatch)))
    // Each draw is a User Timing measure, read by the e2e scene that measures every page; never by the app.
    const draw = (p: lo.T<S>): void => {
      const t0 = performance.now()
      paint(parts, f, p, stale, fx, at, kept, carry)
      if (said(f, p, stale).type !== "Waiting") drew()
      if (p.type === "Val") for (const i of all) i.update(p.v)
      if (p.type === "Val") f.effects?.(p.v, fx)
      performance.measure("page:draw", { start: t0, end: performance.now(), detail: { page: f.className, read: p.type } })
    }
    // What a listener says as the page opens is folded in and drawn once, when it has had its say.
    let opening = true
    const unsubscribe = s.subscribe(p => (opening ? undefined : draw(p)))
    const unlisten = f.listen?.(fx.dispatch, fx.again)
    opening = false
    draw(s.state())
    const unheard = reads.hear(heard)
    heard()
    return {
      destroy: () => (unsubscribe(), unheard(), unlisten?.(), all.forEach(i => i.destroy?.()), f.destroy?.()),
      ...(f.absorb === undefined ? {} : { absorb: (screen: Screen) => f.absorb?.(screen, fx.dispatch) === true }),
    }
  }

  export const mount = <S, E = never, K extends string = never>(root: HTMLElement, f: Page<S, E, K>): Promise<Mounted> =>
    Promise.resolve(open(root, f))
}
export default frame

