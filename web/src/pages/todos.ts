// Home: the open todos as one tree of terms, the detail beside it, and the one place a todo is born.
// The page's value is todos.state's; the head, the filter bar, the keyed tree, the add sheet and the pane are islands.

import { perform, undoOrRedo } from "../todo/acts"
import { addSheet } from "../todo/addsheet"
import type { AddSheet } from "../todo/addsheet"
import briefingsPage from "./briefings"
import asked from "../app/asked"
import lo from "../algebra/lo"
import latest from "../algebra/latest"
import type { Entry } from "../wire/generated/wire"
import empty from "../view/empty"
import { PROJECT_CHAIN } from "../app/chain"
import app from "../app/app"
import remote from "../prodrome/remote"
import { el } from "../view/dom"
import { filterBar } from "../todo/filterbar"
import type { FilterBar } from "../todo/filterbar"
import frame from "../view/frame"
import placeholder from "../view/placeholder"
import reading from "../algebra/reading"
import grouping from "../todo/grouping"
import { chevronIcon, searchIcon } from "../view/icons"
import keyed from "../algebra/keyed"
import type { KeyTarget } from "../shell/keyboard"
import { clearKeyTarget, setKeyTarget } from "../shell/keyboard"
import { WIDE_MIN_PX } from "../shell/layout"
import ui from "../app/ui"
import registers from "../app/registers"
import marks from "../view/marks"
import type { Mounted } from "../view/page"
import { detailPane } from "../todo/pane"
import { timed } from "../app/perf"
import { print } from "../chat/refs"
import { unsentTodos } from "../prodrome/replica"
import { type Route, hrefOf } from "../shell/route"
import { getOverview } from "../wire/api"
import overview from "./overview"
import row from "../todo/row"
import time from "../app/time"
import onscreen, { store } from "../app/store"
import { askHeld, ensureFold, held, watchOthers, watchReplica } from "../prodrome/sync"
import { toast } from "../view/toast"
import { DEFAULT_FILTER, applicable, finish, headline, narrowing, parseFilter, printFilter } from "../todo/todos"
import todosState from "./todos.state"
import address from "../shell/address"

export namespace todos {
  type T = todosState.T
  type Parts = "head" | "tree" | "sheet" | "scrim" | "detail"


  const GROUPINGS: readonly (readonly [grouping.Grouping, string])[] = [
    ["category", "Category"],
    ["urgency", "Urgency"],
    ["flat", "Flat"],
  ]

  const wide = (): boolean => window.matchMedia(`(min-width: ${WIDE_MIN_PX}px)`).matches

  const needs = (n: number, m: number): (Node | string)[] => [
    n === 0 ? "Nothing needs attention" : `${n} ${n === 1 ? "needs" : "need"} attention`,
    marks.diamond(),
    el("a", { class: "inboxlink", href: hrefOf({ type: "inbox" }) }, m === 0 ? "nothing waits on you" : `${m} ${m === 1 ? "waits" : "wait"} on you`),
  ]

  // The title with its one control, the magnifier, which opens the search, the filters and the grouping under it and says
  // what narrows the list while they are shut; then what needs attention. An island, so a pressed pill keeps its focus.
  const head = (on: string, dispatch: (e: todosState.Event) => void, bar: FilterBar): frame.Island<T> => {
    const attention = el("p", { class: "needs" })
    const pills = GROUPINGS.map(([by, label]) =>
      el("button", { type: "button", class: "pill", "data-by": by, onclick: () => (ui.cell(registers.by).set(by), dispatch({ type: "By", by })) }, label),
    )
    const said = el("span", { class: "findsaid" })
    const toggle = el(
      "button",
      {
        type: "button",
        class: "findbtn",
        "aria-controls": "finder",
        "aria-label": "search, filter and group",
        onclick: () => dispatch({ type: "ToggleFind" }),
      },
      searchIcon(),
      said,
    )
    const panel = el(
      "div",
      { class: "finder", id: "finder" },
      bar.node,
      el("div", { class: "groupby", role: "group", "aria-label": "group by" }, el("span", { class: "groupbylabel" }, "Group by"), pills),
    )
    return {
      node: marks.head(
        el("div", { class: "titlerow" }, el("h1", { class: on === PROJECT_CHAIN ? "pagetitle" : "pagetitle destination" }, on === PROJECT_CHAIN ? "Project" : "Todos"), toggle),
        attention,
        panel,
      ),
      update: v => {
        toggle.setAttribute("aria-expanded", v.find.type === "Open" ? "true" : "false")
        panel.hidden = v.find.type === "Shut"
        said.textContent = narrowing(v.filter).join(" · ")
        toggle.setAttribute("aria-label", said.textContent === "" ? "search, filter and group" : `search, filter and group: ${said.textContent}`)
        for (const p of pills) p.setAttribute("aria-pressed", p.getAttribute("data-by") === v.by ? "true" : "false")
        bar.update(onscreen.entries(v.store))
        if (onscreen.loaded(v.store) && v.grown !== null) attention.replaceChildren(...needs(row.attention(v.grown), v.waiting))
      },
    }
  }

  // The keyed tree keeps its nodes, its cursor's focus and the keyboard; the cursor is a line's key, so a reorder cannot move it.
  const treeOf = (on: string, dispatch: (e: todosState.Event) => void, sheet: AddSheet, bar: FilterBar): frame.Island<T> => {
    const list = el("div", { class: "tree" })
    const cards = keyed.list(list)
    const inner = new Map<string, { section: HTMLElement; head: keyed.List; rows: keyed.List }>()
    let now: T | null = null

    const card = (key: string): HTMLElement => {
      const section = el("section", { class: "group", "data-group": key })
      const top = el("div", { class: "grouptop" })
      const body = el("div", { class: "grouprows" })
      section.append(top, marks.rule(), body)
      inner.set(key, { section, head: keyed.list(top), rows: keyed.list(body) })
      return section
    }

    const fx: row.Fx = {
      fold: (key, open) => dispatch({ type: "Fold", key, open }),
      tap: (todo, key) => {
        dispatch({ type: "Cursor", key })
        if (wide()) address.go({ type: "todo", id: todo })
        else dispatch({ type: "Open", id: now?.opened === todo ? "" : todo })
      },
      under: todo => {
        const e = onscreen.entry(store.state(), todo)
        sheet.under({ todo, title: e === undefined ? todo : headline(e), category: e?.authored?.category ?? "" })
      },
      acted: () => {},
      ask: todo => hrefOf({ type: "secretary", refs: [print({ kind: "todo", chain: on, id: todo })], topics: [] }),
      href: todo => hrefOf({ type: "todo", id: todo }),
    }

    const keyNode = (key: string): HTMLElement | null =>
      [...list.querySelectorAll<HTMLElement>(".grouphead, .rowmain")].find(n => n.getAttribute("data-key") === key) ?? null

    const draw = (s: T, grown: NonNullable<T["grown"]>): void => {
      list.setAttribute("data-at", s.basis.type === "Folded" ? s.basis.at : "")
      const had = list.contains(document.activeElement)
      const byId = new Map(onscreen.entries(s.store).map(e => [e.todo, e]))
      const entryOf = (id: string): Entry | undefined => byId.get(id)
      const cs = row.cards(grown, s.by, todosState.shownFolds(s))
      cards.draw(cs.map(c => ({ key: c.key, sig: "", make: () => card(c.key) })))
      const mayWrite = app.writes(app.now())
      for (const c of cs) {
        const it = inner.get(c.key)
        if (it === undefined) continue
        it.section.classList.toggle("open", c.open)
        it.section.setAttribute("data-worth", String(grouping.worth.toNumber(c.node.worth)))
        it.head.draw([{ key: "head", sig: row.headSignature(c), make: () => row.head(c, fx) }])
        const lines = c.lines.flatMap((l): keyed.Item[] => {
          const id = grouping.place.todo(l.v.place) ?? ""
          const e = entryOf(id)
          const shown: row.Shown = {
            open: row.isOpen(s.folds, grouping.place.key(l.v.place), l.v.worth),
            act: s.pending.get(id) ?? "Taken",
          }
          const item = { key: l.key, sig: row.lineSignature(l, e, shown), make: () => row.line(l, e, shown, fx) }
          return s.opened !== id || e === undefined
            ? [item]
            : [item, { key: `${l.key}#open`, sig: row.panelSignature(e, l.depth, mayWrite), make: () => row.panel(e, l.depth, mayWrite, fx) }]
        })
        const peeked = (p: grouping.Node): keyed.Item[] => {
          const e = entryOf(grouping.place.todo(p.place) ?? "")
          return [{ key: `${c.key}#peek`, sig: row.peekSignature(p, e), make: () => row.peekLine(p, e) }]
        }
        it.rows.draw(c.open || c.peek === null ? lines : peeked(c.peek))
        for (const l of c.lines) {
          const id = grouping.place.todo(l.v.place) ?? ""
          const n = it.rows.node(l.key)
          if (n !== undefined) row.mark(n, { opened: s.opened === id, selected: s.selected === id })
        }
      }
      for (const k of [...inner.keys()]) if (!cs.some(c => c.key === k)) inner.delete(k)
      if (had && !list.contains(document.activeElement)) keyNode(s.cursor)?.focus()
      if (cs.length > 0) timed(onscreen.fresh(s.store) ? "first rows (network)" : "first rows (cache)")
    }

    const stops = (): row.Stop[] => (now === null || now.grown === null ? [] : row.stops(row.cards(now.grown, now.by, todosState.shownFolds(now))))
    const here = (): row.Stop | undefined => stops().find(p => p.key === now?.cursor)
    const todoAt = (): string => {
      const p = here()
      return p?.type === "Line" ? (grouping.place.todo(p.line.v.place) ?? "") : ""
    }
    const cursorTo = (key: string): void => {
      dispatch({ type: "Cursor", key })
      const n = keyNode(key)
      n?.focus()
      n?.scrollIntoView({ block: "nearest" })
    }
    const apply = (m: row.Move): void => {
      switch (m.type) {
        case "Fold":
          return dispatch({ type: "Fold", key: m.key, open: m.open })
        case "Open":
          return dispatch({ type: "Open", id: m.todo })
        case "Close":
          return dispatch({ type: "Open", id: "" })
        case "Go":
          address.go({ type: "todo", id: m.todo })
          return
        case "Cursor":
          return cursorTo(m.key)
        case "Stay":
          return
      }
    }

    list.addEventListener("focusin", (event: FocusEvent) => {
      const key = event.target instanceof HTMLElement ? event.target.closest("[data-key]")?.getAttribute("data-key") : null
      if (key !== null && key !== undefined) dispatch({ type: "Cursor", key })
    })

    const keys: KeyTarget = {
      move: by => {
        const ps = stops()
        if (ps.length === 0) return
        const at = ps.findIndex(p => p.key === now?.cursor)
        const to = at === -1 ? (by > 0 ? 0 : ps.length - 1) : Math.min(ps.length - 1, Math.max(0, at + by))
        cursorTo(ps[to]?.key ?? "")
      },
      open: () => {
        const p = here()
        if (p !== undefined && now !== null) apply(row.onOpen(p, todosState.shownFolds(now), now.opened, wide()))
      },
      close: () => {
        const p = here()
        if (p !== undefined && now !== null) apply(row.onClose(p, todosState.shownFolds(now), now.opened))
      },
      act: asked => {
        const id = todoAt()
        const e = id === "" ? undefined : onscreen.entry(store.state(), id)
        if (e === undefined) return
        const name = asked === "done" ? finish(e) : asked
        if (!applicable(e).includes(name)) {
          toast(`${id} is ${e.state} — nothing to ${name === "done" ? "complete" : name}`)
          return
        }
        void perform(name, id, fx.acted)
      },
      undo: () => {
        const id = todoAt()
        if (id !== "") void undoOrRedo("undo", id, fx.acted)
      },
      redo: () => {
        const id = todoAt()
        if (id !== "") void undoOrRedo("redo", id, fx.acted)
      },
      focusSearch: () => (dispatch({ type: "Find", find: todosState.opened }), bar.focusSearch()),
      add: () => sheet.open(),
      escape: () => {
        if (sheet.isOpen()) {
          sheet.close()
          return true
        }
        if (now !== null && now.opened !== "") {
          dispatch({ type: "Open", id: "" })
          return true
        }
        if (now === null || now.cursor === "") return false
        keyNode(now.cursor)?.blur()
        dispatch({ type: "Cursor", key: "" })
        return true
      },
    }
    setKeyTarget(keys)

    return {
      node: list,
      update: v => {
        now = v
        ui.cell(registers.folds, on).set(v.folds)
        ui.cell(registers.opened, on).set(v.opened)
        if (onscreen.loaded(v.store) && v.grown !== null) draw(v, v.grown)
      },
      destroy: () => clearKeyTarget(keys),
    }
  }

  // Why it is empty has three answers, and they are different news.
  const nothing = (s: T): string =>
    onscreen.entries(s.store).length === 0
      ? "This list has nothing on it yet. Add the first todo with the sheet at the bottom."
      : printFilter(s.filter) === printFilter(DEFAULT_FILTER)
        ? "Nothing is open. Everything on this list has been settled."
        : "Nothing matches. Tap the magnifier to widen the search."

  // How far setup is, one question of the box that outlives the page, so the list mounted again draws the line it drew
  // before; asked again when the box moves or what waits on the owner moves.
  const setup = asked.question(
    () => getOverview().then(overview.progress),
    (a, b) => overview.unfinished(a) === overview.unfinished(b),
  )

  // The functions and environment the core's worker folds from the objects the tab holds, at the fold's own instant,
  // linked through every chain it holds; the chains that set holds as failures come from the same holding, so the line
  // naming them never contradicts a price beside it. The worker keeps a reading by the set and the instant, so a page
  // mounted again over the same chains folds nothing; one landing after a later-asked one is dropped.
  const rebasing = (dispatch: (e: todosState.Event) => void): { rebase: () => void; close: () => void } => {
    const readings = latest.of<todosState.Basis | null>(l => {
      if (l.type === "Error") return dispatch({ type: "Problem", reason: l.e, ofReading: true })
      if (l.v !== null) dispatch({ type: "Basis", basis: l.v })
    })
    const rebase = (): void => {
      const at = onscreen.at(store.state())
      if (at === null) return
      const on = app.now().chain
      void readings.ask(async (): Promise<todosState.Basis | null> => {
        const { held: h, answer: read } = await askHeld(() => ({ type: "Reading", chain: on, at }), "Page")
        // Holding nothing of the open chain yet, the rows price the tree, and its first objects ask again.
        return read.type === "Unheld"
          ? null
          : { type: "Folded", reading: read.reading, at: read.at, specs: read.specs, waits: grouping.unheld(read.functions, new Set(h.held.map(c => c.name))) }
      })
    }
    // Another chain moving reprices what refers into it, though the open chain stood still.
    const unwatch = watchOthers(rebase)
    return { rebase, close: () => (readings.close(), unwatch()) }
  }

  // `selected` is the todo the pane shows at 62rem and up, "" for none; `prefill` is what `#/add?text=…` puts in the sheet.
  export const page = (selected: string, prefill: string): frame.Page<T, todosState.Event, Parts> => {
    const on = app.now().chain
    return {
      className: "listpane todospane",
      wide: true,
      title: on === PROJECT_CHAIN ? "Project" : "Todos",
      shape: placeholder.ROWS,
      // Until the chain is folded there is no row to draw.
      waits: v => v.problem === null && !(onscreen.loaded(v.store) && v.grown !== null),
      reads: () =>
        reading.constant(todosState.step(
          todosState.initial(
            store.state(),
            parseFilter(ui.cell(registers.filter).get()),
            ui.cell(registers.by).get(),
            ui.cell(registers.folds, on).get(),
            selected,
            prefill,
          ),
          { type: "Open", id: ui.cell(registers.opened, on).get() },
        )),
      step: todosState.step,
      listen: dispatch => {
        const { rebase, close } = rebasing(dispatch)
        const briefed = (): void =>
          lo.fold(briefingsPage.listed.now(), {
            loading: () => {},
            error: () => dispatch({ type: "Briefing", briefing: { type: "None" } }),
            val: p => {
              const today = time.date(app.now().zone)
              dispatch({ type: "Briefing", briefing: p.briefings.some(b => b.date === today) ? { type: "Today", date: today } : { type: "None" } })
            },
          })
        const unbriefed = briefingsPage.listed.hear(briefed)
        briefed()
        // Whether setup is finished; a box that does not answer leaves the line unsaid.
        const setupRead = (): void =>
          lo.fold(setup.now(), { loading: () => {}, error: () => {}, val: p => dispatch({ type: "Setup", setup: p }) })
        const unsetup = setup.hear(setupRead)
        setupRead()
        dispatch({ type: "Waiting", count: app.now().waiting })
        dispatch({ type: "Pending", todos: unsentTodos(held()) })
        // The rows are drawn from the fold the tab has; the objects it holds are folded beside them, and the page moves
        // to that reading when it lands.
        rebase()
        void ensureFold().catch((e: unknown) => dispatch({ type: "Problem", reason: String(e), ofReading: false }))
        const us = [
          store.subscribe(s => (dispatch({ type: "Store", state: s }), rebase())),
          // The stream going up or down is news about the fold, as it was when the fold carried it: it clears a problem.
          app.hear(app.live, () => dispatch({ type: "Store", state: store.state() })),
          watchReplica(() => (dispatch({ type: "Pending", todos: unsentTodos(held()) }), rebase())),
          app.hear(app.actor, () => dispatch({ type: "Poke" })),
          app.hear(a => a.waiting, count => (dispatch({ type: "Waiting", count }), setup.again())),
          ui.cell(registers.by).hear(() => dispatch({ type: "By", by: ui.cell(registers.by).get() })),
        ]
        return () => (us.forEach(u => u()), unbriefed(), unsetup(), close())
      },
      islands: dispatch => {
        const kept = ui.cell(registers.filter)
        const narrowed = (): string => {
          const c = parseFilter(kept.get()).category
          return c.tag === "is" ? c.value : ""
        }
        const sheet = addSheet(() => app.now().categories, narrowed, on)
        // The list reads the filter register, whoever wrote it: this strip, or another tab.
        const unheard = kept.hear(() => dispatch({ type: "Filter", filter: parseFilter(kept.get()) }))
        const bar = filterBar({
          filter: { get: () => parseFilter(kept.get()), set: next => kept.set(printFilter(next)), hear: kept.hear },
          categories: () => app.now().categories,
        })
        const detail = detailPane()
        const recategorised = app.hear(a => a.categories, () => sheet.recategorised())
        let shared = 0
        return {
          head: { ...head(on, dispatch, bar), destroy: () => (unheard(), bar.destroy()) },
          tree: treeOf(on, dispatch, sheet, bar),
          // A share is consumed, not a place: the text goes into the sheet and the hash back to the list's, with no history entry.
          sheet: {
            node: sheet.node,
            update: v => {
              if (v.share.n === shared) return
              shared = v.share.n
              sheet.prefill(v.share.body)
              const home: Route = on === PROJECT_CHAIN ? { type: "project" } : { type: "todos" }
              address.replace(home)
            },
          },
          scrim: { node: sheet.scrim, update: () => {}, destroy: () => (recategorised(), sheet.destroy()) },
          detail: {
            node: detail.node,
            update: v => detail.show(v.selected),
            destroy: detail.destroy,
          },
        }
      },
      // The worker's prices for what the tree lacks, asked once per set of terms; each answer is an event of the page.
      effects: (v, fx) => todosState.ask(v, (basis, terms) => remote.ask({ type: "Price", basis, terms }, "Page"), fx.dispatch),
      masthead: (_, at) => at.head,
      around: (column, at) => el("div", { class: "split todosplit" }, column, at.detail),
      absorb: (screen, dispatch) => {
        if (screen.type !== "todos" || (screen.chain !== "" && screen.chain !== on)) return false
        dispatch({ type: "Select", id: screen.selected })
        dispatch({ type: "Shared", body: screen.prefill })
        return true
      },
      view: (v, _, __, at) => {
        const said = v.problem?.reason ?? null
        const drawn = onscreen.loaded(v.store) && v.grown !== null
        const unfinished = v.setup.type === "Unknown" ? null : overview.unfinished(v.setup)
        return [
          // While setup is unfinished the first screen says so, and where to go next.
          unfinished === null
            ? null
            : el("p", { class: "setupunfinished" }, el("a", { href: hrefOf({ type: "settings" }) }, unfinished, chevronIcon())),
          v.briefing.type !== "Today"
            ? null
            : el("p", { class: "todaybriefing" }, el("a", { href: hrefOf({ type: "briefing", date: v.briefing.date }) }, "Today's briefing", chevronIcon())),
          said === null ? null : el("p", { class: "problem" }, said),
          at.tree,
          drawn && v.grown !== null && row.cards(v.grown, v.by, v.folds).length === 0 ? empty.note(nothing(v)) : null,
          el("div", { class: "sheetspacer" }),
          at.sheet,
          at.scrim,
        ]
      },
    }
  }

  export const mount = (root: HTMLElement, selected: string, prefill = ""): Promise<Mounted> => frame.mount(root, page(selected, prefill))
}
export default todos
