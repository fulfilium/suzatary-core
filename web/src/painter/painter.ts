// The DOM painter: applies the patches a Rust program answers (format: `app/viewtree/src/patch.rs`) and reports events
// by element id. It decides nothing; its only state is which node each id is.

import { richOrText } from "../rich"
import files from "./files"

export namespace painter {
  export type Foreign = (kind: string, key: string, props: string) => Node
  // A part told new props: true when the host brought it to them in place (its focus, scroll and DOM kept), false when
  // the painter is to draw it anew.
  export type Refresh = (node: Node, kind: string, key: string, props: string) => boolean
  // The HTML the host holds for a block's key, or null. It is placed only through `rich.ts`, the one door from a string
  // to markup, and the block shows its plain text until then.
  export type Blocks = (key: string) => Promise<string | null>
  // Answers whether the view took it: an event no handler takes (a key left to the browser) keeps its own effect.
  export type Report = (id: number, on: number, words: string) => boolean
  export type T = { readonly apply: (patches: Uint8Array) => void; readonly destroy: () => void }

  // In the order of `Tag` and `On` (`app/viewtree/src/node.rs`), whose numbers index them.
  const TAGS: readonly (keyof HTMLElementTagNameMap)[] = ["div", "section", "h2", "h3", "p", "a", "button", "input", "details", "summary", "code", "span", "h1", "ul", "li", "dl", "dt", "dd", "strong", "form", "pre", "textarea", "nav", "article", "img", "label", "aside", "ol"]
  const EVENTS: readonly string[] = ["click", "input", "submit", "keydown", "toggle", "change", "intent"]

  type Reader = { readonly bytes: Uint8Array; at: number }

  const byte = (r: Reader): number => {
    const b = r.bytes[r.at]
    if (b === undefined) throw new Error(`patches end inside a patch at byte ${r.at}`)
    r.at += 1
    return b
  }

  const varint = (r: Reader): number => {
    let n = 0
    for (let shift = 0; shift < 35; shift += 7) {
      const b = byte(r)
      n |= (b & 0x7f) << shift
      if ((b & 0x80) === 0) return n >>> 0
    }
    throw new Error(`a number longer than five bytes at byte ${r.at}`)
  }

  const utf8 = new TextDecoder()
  const string = (r: Reader): string => {
    const n = varint(r)
    const s = utf8.decode(r.bytes.subarray(r.at, r.at + n))
    r.at += n
    return s
  }

  // Only an address inside the app, an `http(s):` or `mailto:` one, or a path on the box is a link; anything else is
  // dropped. An outside one or a box's path opens a new tab with no handle back on this one, whatever the view asked.
  type Href = "Inside" | "Outside" | "Refused"
  const onTheBox = (path: string): boolean => path.startsWith("/api/") && !path.startsWith("//")
  const hrefOf = (href: string): Href =>
    href.startsWith("#") ? "Inside" : /^(https?:|mailto:)/i.test(href) || onTheBox(href) ? "Outside" : "Refused"

  const field = (e: Element): e is HTMLInputElement | HTMLTextAreaElement => e instanceof HTMLInputElement || e instanceof HTMLTextAreaElement

  // The event's words in the format `node.rs` reads: a field's value, a key as `Key::print`, `"1"` for a details open.
  const said = (name: string, e: Element, ev: Event): string =>
    name === "keydown" && ev instanceof KeyboardEvent
      ? `${ev.ctrlKey ? 1 : 0}${ev.metaKey ? 1 : 0}${ev.shiftKey ? 1 : 0}${ev.key}\0${field(e) ? e.value : ""}`
      : name === "toggle" && e instanceof HTMLDetailsElement
        ? e.open ? "1" : ""
        : field(e) ? e.value
        : ev.target instanceof Element && field(ev.target) ? ev.target.value
        : ""

  // `gone` hears each foreign part the painter takes out, so the host can stop what it started for it.
  export const create = (root: HTMLElement, foreign: Foreign, blocks: Blocks, report: Report, gone: (n: Node) => void = () => {}, refresh: Refresh = () => false): T => {
    const nodes = new Map<number, Node>([[0, root]])
    const ids = new WeakMap<Node, number>()
    // Each foreign part's kind and key, so new props ask the host for the same part again.
    const foreigns = new Map<number, readonly [string, string]>()
    // Each block's key, so an answer for a key the block no longer shows is dropped.
    const keys = new Map<number, string>()
    const heard = new Map<string, EventListener>()

    const node = (id: number): Node => {
      const n = nodes.get(id)
      if (n === undefined) throw new Error(`no node ${id}`)
      return n
    }
    const element = (id: number): Element => {
      const n = node(id)
      if (!(n instanceof Element)) throw new Error(`node ${id} is not an element`)
      return n
    }
    const placed = (id: number, n: Node, parent: number, before: number): void => {
      nodes.set(id, n)
      ids.set(n, id)
      node(parent).insertBefore(n, before === 0 ? null : node(before - 1))
    }
    const forget = (n: Node): void => {
      const id = ids.get(n)
      if (id === undefined) return
      nodes.delete(id)
      for (const e of EVENTS) heard.delete(`${id}:${e}`)
      if (foreigns.delete(id)) return gone(n)
      if (keys.delete(id)) return
      n.childNodes.forEach(forget)
    }

    const replaced = (id: number, now: Node): Node => {
      const was = node(id)
      nodes.set(id, now)
      ids.set(now, id)
      was.parentNode?.replaceChild(now, was)
      return was
    }

    // `fallback` now, and the host's HTML for `key` once it answers, if the block still shows `key` then.
    const block = (id: number, key: string, fallback: string): HTMLElement => {
      keys.set(id, key)
      void blocks(key).then(html => {
        if (html !== null && keys.get(id) === key) replaced(id, richOrText(html, fallback, { class: "block" }))
      })
      return richOrText(null, fallback, { class: "block" })
    }

    const set = (e: Element, name: string, value: string): void => {
      if (name === "value" && field(e)) {
        if (e.value !== value) e.value = value
      } else if (name === "checked" && e instanceof HTMLInputElement) {
        e.checked = true
      } else if (name === "src") {
        if (onTheBox(value)) e.setAttribute(name, value)
        else e.removeAttribute(name)
      } else if (name !== "href") e.setAttribute(name, value)
      else {
        const h = hrefOf(value)
        if (h === "Refused") return e.removeAttribute(name)
        e.setAttribute(name, value)
        if (h === "Outside") (e.setAttribute("target", "_blank"), e.setAttribute("rel", "noopener noreferrer"))
        else (e.removeAttribute("target"), e.removeAttribute("rel"))
      }
    }

    const apply = (patches: Uint8Array): void => {
      const r: Reader = { bytes: patches, at: 0 }
      while (r.at < patches.length) {
        const op = byte(r)
        switch (op) {
          case 0: {
            const [id, parent, before] = [varint(r), varint(r), varint(r)]
            const tag = TAGS[byte(r)]
            if (tag === undefined) throw new Error(`no tag at byte ${r.at - 1}`)
            placed(id, document.createElement(tag), parent, before)
            break
          }
          case 1: {
            const [id, parent, before] = [varint(r), varint(r), varint(r)]
            placed(id, document.createTextNode(string(r)), parent, before)
            break
          }
          case 2: {
            const [id, parent, before] = [varint(r), varint(r), varint(r)]
            const [kind, key, props] = [string(r), string(r), string(r)]
            foreigns.set(id, [kind, key])
            placed(id, foreign(kind, key, props), parent, before)
            break
          }
          case 3: {
            const id = varint(r)
            const [name, value] = [string(r), string(r)]
            set(element(id), name, value)
            break
          }
          case 4: {
            const e = element(varint(r))
            const name = string(r)
            if (name === "value" && field(e)) e.value = ""
            if (name === "checked" && e instanceof HTMLInputElement) e.checked = false
            if (name === "href") (e.removeAttribute("target"), e.removeAttribute("rel"))
            e.removeAttribute(name)
            break
          }
          case 5: {
            const id = varint(r)
            node(id).textContent = string(r)
            break
          }
          case 6: {
            const id = varint(r)
            const props = string(r)
            const part = foreigns.get(id)
            if (part === undefined) throw new Error(`node ${id} is not a foreign part`)
            if (refresh(node(id), part[0], part[1], props)) break
            gone(replaced(id, foreign(part[0], part[1], props)))
            break
          }
          case 7: {
            const [id, parent, before] = [varint(r), varint(r), varint(r)]
            node(parent).insertBefore(node(id), before === 0 ? null : node(before - 1))
            break
          }
          case 8: {
            const n = node(varint(r))
            n.parentNode?.removeChild(n)
            forget(n)
            break
          }
          case 9: {
            const id = varint(r)
            const on = byte(r)
            const name = EVENTS[on]
            if (name === undefined) throw new Error(`no event ${on}`)
            const e = element(id)
            // A submit is reported, never performed: the page decides where it goes. Files are read whole first (`files.ts`).
            const listener: EventListener = (ev: Event) => {
              if (name === "intent") {
                // A click inside on an element carrying an intent reports that intent's words, as written; the view parses them.
                const at = ev.target instanceof Element ? ev.target.closest("[data-intent]") : null
                if (at !== null && e.contains(at)) report(id, on, at.getAttribute("data-intent") ?? "")
                return
              }
              if (name === "submit") ev.preventDefault()
              if (name === "change") {
                if (e instanceof HTMLInputElement && e.type === "file") void files.read(e).then(words => void report(id, on, words))
                return
              }
              if (report(id, on, said(name, e, ev)) && name === "keydown") (ev.preventDefault(), ev.stopPropagation())
            }
            heard.set(`${id}:${name}`, listener)
            e.addEventListener(name === "intent" ? "click" : name, listener)
            break
          }
          case 10: {
            const id = varint(r)
            const name = EVENTS[byte(r)]
            const listener = heard.get(`${id}:${name ?? ""}`)
            if (name === undefined || listener === undefined) throw new Error(`node ${id} does not hear that event`)
            element(id).removeEventListener(name === "intent" ? "click" : name, listener)
            heard.delete(`${id}:${name}`)
            break
          }
          case 11: {
            const [id, parent, before] = [varint(r), varint(r), varint(r)]
            const [key, fallback] = [string(r), string(r)]
            placed(id, block(id, key, fallback), parent, before)
            break
          }
          case 12: {
            const id = varint(r)
            const [key, fallback] = [string(r), string(r)]
            if (!keys.has(id)) throw new Error(`node ${id} is not a block`)
            replaced(id, block(id, key, fallback))
            break
          }
          default:
            throw new Error(`no patch ${op} at byte ${r.at - 1}`)
        }
      }
    }

    return { apply, destroy: () => (foreigns.forEach((_, id) => gone(node(id))), root.replaceChildren()) }
  }
}
export default painter
