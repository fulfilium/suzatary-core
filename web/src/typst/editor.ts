// A field that edits Typst: a native textarea over its own shaded twin, completion and hover from typst-ide, the
// diagnostics under it with their spans marked, and a live preview compiled by the worker and checked by the box's
// tripwire before it is parsed. Everything drawn is a function of `edit.State`.

import edit from "./edit"
import type { Tripwire } from "../rich"
import { checked } from "../rich"
import { loadCore } from "../prodrome/core"
import { el } from "../view/dom"

export namespace editor {
  export type Effects = { readonly ask: (a: edit.Ask) => Promise<edit.Answer>; readonly tripwire: () => Promise<Tripwire> }

  // A compile past this, once the worker is ready, is a loop: the worker is terminated and the text refused.
  const LIMIT_MS = 2000

  type Waiting = { readonly ask: edit.Ask; readonly resolve: (a: edit.Answer) => void; timer: number | null }

  const unanswered = (a: edit.Ask, why: string): edit.Answer =>
    a.type === "Check" ? { type: "Check", checked: edit.failed(why) }
    : a.type === "Shade" ? { type: "Shade", spans: [] }
    : a.type === "Complete" ? { type: "Complete", offer: null }
    : { type: "Hover", text: "" }

  let worker: Worker | null = null
  let ready = false
  let next = 0
  const waiting = new Map<number, Waiting>()

  const clock = (w: Waiting): void => {
    if (ready && w.ask.type === "Check" && w.timer === null) w.timer = window.setTimeout(() => stop(`the text did not finish compiling within ${LIMIT_MS / 1000} s`), LIMIT_MS)
  }

  const stop = (why: string): void => {
    worker?.terminate()
    worker = null
    ready = false
    const all = [...waiting.values()]
    waiting.clear()
    all.forEach(w => (w.timer === null ? undefined : window.clearTimeout(w.timer), w.resolve(unanswered(w.ask, why))))
  }

  const spawn = (): Worker => {
    const w = new Worker(new URL("./typstwork.js", import.meta.url), { type: "module" })
    w.addEventListener("message", (e: MessageEvent<edit.Answered>) => {
      ready = true
      const found = waiting.get(e.data.id)
      waiting.forEach(clock)
      if (found === undefined) return
      waiting.delete(e.data.id)
      if (found.timer !== null) window.clearTimeout(found.timer)
      found.resolve(e.data.answer)
    })
    w.addEventListener("error", e => stop(`the editor's Typst failed: ${e.message}`))
    return w
  }

  const ask = (a: edit.Ask): Promise<edit.Answer> =>
    new Promise(resolve => {
      const id = next++
      const w: Waiting = { ask: a, resolve, timer: null }
      waiting.set(id, w)
      clock(w)
      worker ??= spawn()
      worker.postMessage({ id, ask: a } satisfies edit.Asked)
    })

  export const effects: Effects = { ask, tripwire: () => loadCore().then(c => c.suspicious) }

  type Control = { readonly node: HTMLElement; paint(v: string): void; pull(): string }

  // At most one compile in flight per editor; the newest text waits behind it and older ones are dropped.
  export type Look = { readonly cls: string; readonly label: string; readonly hint?: string }

  export const control = (look: Look, rows: number, set: (v: string) => void, fx: Effects = effects): Control => {
    let s = edit.of("")
    let inflight = false
    let queued = false
    let frame: number | null = null

    const input = el("textarea", {
      class: `typedinput ${look.cls}`,
      rows,
      placeholder: look.hint ?? null,
      spellcheck: "false",
      autocapitalize: "off",
      autocomplete: "off",
      "aria-label": look.label,
    })
    const shade = el("pre", { class: "typedshade", "aria-hidden": "true" })
    const menu = el("ul", { class: "typedoffer", role: "listbox", hidden: true })
    const hint = el("p", { class: "typedhover", "aria-live": "polite" })
    const problems = el("ul", { class: "typedproblems", role: "status" })
    const preview = el("div", { class: "typedpreview rich", "aria-label": "preview" })

    const dispatch = (e: edit.Event): void => {
      const was = s
      s = edit.step(s, e)
      if (s !== was) draw(was)
    }

    const fit = (): void => {
      input.style.height = "auto"
      input.style.height = `${input.scrollHeight}px`
    }

    const accept = (i: edit.Item): void => {
      if (s.offer === null) return
      const made = edit.accept(s.source, input.selectionStart, s.offer, i)
      input.value = made.source
      input.setSelectionRange(made.caret, made.caret)
      dispatch({ type: "Closed" })
      typed()
    }

    const draw = (was: edit.State | null): void => {
      shade.replaceChildren(
        ...edit.drawn(s).map(r => el("span", { class: [r.tag ?? "", r.marked ? "typedmark" : ""].join(" ").trim() || null }, r.text)),
        // A <pre> swallows a final newline; one more keeps the last line's height.
        "\n",
      )
      const ps = edit.problems(s)
      if (was === null || edit.problems(was) !== ps) problems.replaceChildren(...ps.map(p => el("li", null, edit.line(s.source, p))))
      menu.hidden = s.offer === null
      if (was === null || was.offer !== s.offer || was.chosen !== s.chosen)
        menu.replaceChildren(
          ...(s.offer?.items.slice(0, 12) ?? []).map((i, n) =>
            el(
              "li",
              { role: "option", "aria-selected": n === s.chosen ? "true" : "false", onpointerdown: (e: Event) => (e.preventDefault(), accept(i)) },
              el("code", null, i.label),
              i.detail === null ? null : el("span", { class: "meta" }, ` ${i.detail}`),
            ),
          ),
        )
      hint.textContent = s.hover
      preview.classList.toggle("stale", edit.stale(s))
      if (was === null || was.shown !== s.shown) void showPreview(s.shown)
    }

    const showPreview = async (html: string | null): Promise<void> => {
      if (html === null) return void preview.replaceChildren()
      const out = checked(html, await fx.tripwire())
      if (s.shown !== html) return
      preview.replaceChildren(...(out.type === "Elements" ? out.elements : [el("p", { class: "problem" }, out.reason)]))
    }

    const compile = (): void => {
      if (inflight) return void (queued = true)
      inflight = true
      const source = s.source
      void Promise.all([fx.ask({ type: "Shade", source }), fx.ask({ type: "Check", source })]).then(([a, b]) => {
        if (a.type === "Shade") dispatch({ type: "Shaded", source, spans: a.spans })
        if (b.type === "Check") dispatch({ type: "Checked", source, checked: b.checked })
        inflight = false
        if (queued) (queued = false, compile())
      })
    }

    const offer = (explicit: boolean): void => {
      const source = s.source
      void fx.ask({ type: "Complete", source, cursor: input.selectionStart, explicit }).then(a => (a.type === "Complete" ? dispatch({ type: "Offered", source, offer: a.offer }) : undefined))
    }

    const describe = (): void => {
      const source = s.source
      void fx.ask({ type: "Hover", source, cursor: input.selectionStart }).then(a => (a.type === "Hover" ? dispatch({ type: "Hovered", source, text: a.text }) : undefined))
    }

    // Debounced to a frame: keystrokes inside one frame compile once.
    const typed = (): void => {
      set(input.value)
      dispatch({ type: "Typed", source: input.value })
      fit()
      if (frame === null) frame = window.requestAnimationFrame(() => ((frame = null), compile()))
    }

    input.addEventListener("input", () => {
      typed()
      if (edit.asks(input.value.slice(0, input.selectionStart))) offer(false)
      else dispatch({ type: "Closed" })
    })
    input.addEventListener("keydown", (e: KeyboardEvent) => {
      if (e.key === " " && e.ctrlKey) return void (e.preventDefault(), offer(true))
      if (s.offer === null) return
      const items = s.offer.items.slice(0, 12)
      if (e.key === "ArrowDown" || e.key === "ArrowUp") return void (e.preventDefault(), dispatch({ type: "Chose", by: e.key === "ArrowDown" ? 1 : -1 }))
      if (e.key === "Enter" || e.key === "Tab") {
        const i = items[s.chosen]
        return void (i === undefined ? undefined : (e.preventDefault(), accept(i)))
      }
      if (e.key === "Escape") return void (e.preventDefault(), dispatch({ type: "Closed" }))
    })
    input.addEventListener("click", () => (dispatch({ type: "Closed" }), describe()))
    input.addEventListener("keyup", (e: KeyboardEvent) => void (e.key.startsWith("Arrow") && s.offer === null ? describe() : undefined))
    input.addEventListener("blur", () => dispatch({ type: "Closed" }))

    draw(null)
    const node = el("div", { class: "typed" }, el("div", { class: "typedfield" }, shade, input), menu, hint, problems, preview)
    return {
      node,
      paint: v => {
        if (input.value !== v) input.value = v
        if (s.source === v) return
        dispatch({ type: "Typed", source: v })
        window.requestAnimationFrame(fit)
        compile()
      },
      pull: () => input.value,
    }
  }
}
export default editor
