// The DOM painter: reads the patches a Rust program answers (`app/viewtree/src/patch.rs` says the format) and applies
// them to the DOM under a root, and reports each event it was told to hear as an element's id. It decides nothing:
// every node, attribute and listener is one a patch named, and the only state it keeps is which node each id is.

export namespace painter {
  // A part the host draws (`Foreign` in the view), asked for by kind and key and drawn from its props.
  export type Foreign = (kind: string, key: string, props: string) => Node
  // What a person did on element `id`: `on` is the event's code, `words` a field's value.
  export type Report = (id: number, on: number, words: string) => void
  export type T = { readonly apply: (patches: Uint8Array) => void; readonly destroy: () => void }

  // In the order of `Tag` and `On` (`app/viewtree/src/node.rs`), whose numbers index them.
  const TAGS: readonly (keyof HTMLElementTagNameMap)[] = ["div", "section", "h2", "h3", "p", "a", "button", "input", "details", "summary", "code", "span", "h1", "ul", "li", "dl", "dt", "dd", "strong", "form"]
  const EVENTS: readonly string[] = ["click", "input", "submit"]

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

  // An address inside the app, one outside it on a scheme a link may name, or none: anything else is dropped. An outside
  // one opens a new tab with no handle back on this one, whatever the view asked.
  type Href = "Inside" | "Outside" | "Refused"
  const hrefOf = (href: string): Href =>
    href.startsWith("#") ? "Inside" : /^(https?:|mailto:)/i.test(href) ? "Outside" : "Refused"

  // `gone` hears each foreign part the painter takes out, so the host can stop what it started for it.
  export const create = (root: HTMLElement, foreign: Foreign, report: Report, gone: (n: Node) => void = () => {}): T => {
    const nodes = new Map<number, Node>([[0, root]])
    const ids = new WeakMap<Node, number>()
    // Each foreign part's kind and key, so new props ask the host for the same part again.
    const foreigns = new Map<number, readonly [string, string]>()
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
      n.childNodes.forEach(forget)
    }

    const set = (e: Element, name: string, value: string): void => {
      if (name === "value" && e instanceof HTMLInputElement) {
        if (e.value !== value) e.value = value
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
            if (name === "value" && e instanceof HTMLInputElement) e.value = ""
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
            const was = node(id)
            const part = foreigns.get(id)
            if (part === undefined) throw new Error(`node ${id} is not a foreign part`)
            const now = foreign(part[0], part[1], props)
            nodes.set(id, now)
            ids.set(now, id)
            was.parentNode?.replaceChild(now, was)
            gone(was)
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
            // A submit is reported and never performed: the page decides where it goes.
            const listener: EventListener = (ev: Event) => {
              if (name === "submit") ev.preventDefault()
              report(id, on, e instanceof HTMLInputElement ? e.value : "")
            }
            heard.set(`${id}:${name}`, listener)
            e.addEventListener(name, listener)
            break
          }
          case 10: {
            const id = varint(r)
            const name = EVENTS[byte(r)]
            const listener = heard.get(`${id}:${name ?? ""}`)
            if (name === undefined || listener === undefined) throw new Error(`node ${id} does not hear that event`)
            element(id).removeEventListener(name, listener)
            heard.delete(`${id}:${name}`)
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
