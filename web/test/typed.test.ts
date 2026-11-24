// Drawing runs no handler: typed.carry over stub fields that count every event dispatched to them, over generated
// rows that a reading reorders, adds to and draws with values of its own.

import { test } from "node:test"
import assert from "node:assert/strict"

import typed from "../src/view/typed"
import { forAll } from "./gen"
import type { Rng } from "./gen"

class El {
  parent: El | null = null
  readonly kids: El[] = []
  readonly attrs = new Map<string, string>()
  className = ""
  dispatched = 0
  constructor(readonly tagName: string) {}
  add(...cs: El[]): this {
    for (const c of cs) ((c.parent = this), this.kids.push(c))
    return this
  }
  getAttribute(k: string): string | null {
    return this.attrs.get(k) ?? null
  }
  contains(n: El): boolean {
    return n === this || this.kids.some(k => k.contains(n))
  }
  closest(sel: string): El | null {
    if (sel !== "[data-key]") throw new Error(`no stub for ${sel}`)
    return this.attrs.has("data-key") ? this : (this.parent?.closest(sel) ?? null)
  }
  get parentElement(): El | null {
    return this.parent
  }
  querySelectorAll(sel: string): El[] {
    if (sel !== "input, textarea") throw new Error(`no stub for ${sel}`)
    return this.kids.flatMap(k => [...(k instanceof Field ? [k] : []), ...k.querySelectorAll(sel)])
  }
  dispatchEvent(): boolean {
    this.dispatched += 1
    return true
  }
}
class Field extends El {
  value = ""
  checked = false
  defaultChecked = false
  selectionStart = 0
  selectionEnd = 0
  isConnected = true
  constructor(tag: string, readonly type: string, readonly defaultValue: string) {
    super(tag)
    this.value = defaultValue
  }
  focused = false
  focus(): void {
    this.focused = true
  }
  setSelectionRange(): void {}
}
class Input extends Field {
  constructor(type: string, drawn: string) {
    super("INPUT", type, drawn)
  }
}
class Area extends Field {
  constructor(drawn: string) {
    super("TEXTAREA", "textarea", drawn)
  }
}
Object.assign(globalThis, { Element: El, HTMLElement: El, HTMLInputElement: Input, HTMLTextAreaElement: Area, document: { activeElement: null } })

type Row = { readonly key: string; readonly drawn: readonly string[] }
const KINDS = ["text", "checkbox", "textarea"] as const

// A row's fields, one of each kind, drawn with the values it holds.
const rowOf = (r: Row): El => {
  const [text, box, area] = KINDS.map((k, i): Field => (k === "textarea" ? new Area(r.drawn[i] ?? "") : new Input(k, k === "checkbox" ? "on" : (r.drawn[i] ?? ""))))
  const node = new El("DIV").add(...[text, box, area].filter((f): f is Field => f !== undefined))
  node.attrs.set("data-key", r.key)
  return node
}
const fieldsOf = (n: El): Field[] => n.querySelectorAll("input, textarea").filter((f): f is Field => f instanceof Field)

const rows = (r: Rng): Row[] =>
  [...new Set(r.array(1 + r.int(5), () => r.string("abc", 2)))].map(key => ({ key, drawn: KINDS.map(() => r.pick(["", "drawn"])) }))

test("law: carrying what was typed into a body drawn again dispatches nothing, to the old fields or the new", () =>
  forAll(300, (r, seed) => {
    const before = rows(r)
    const from = new El("DIV").add(...before.map(rowOf))
    for (const f of fieldsOf(from)) {
      if (f.type === "checkbox" && r.bool()) f.checked = !f.checked
      else if (f.type !== "checkbox" && r.bool()) f.value = r.string("xyz", 1 + r.int(4))
    }
    const after = [...r.array(r.int(2), () => ({ key: "new", drawn: ["", "", ""] })), ...[...before].reverse()].map(row =>
      r.bool(0.3) ? { ...row, drawn: row.drawn.map(d => (d === "" ? "moved" : d)) } : row,
    )
    const to = after.map(rowOf)
    typed.carry(from as unknown as Element, to as unknown as Node[])
    const all = [...fieldsOf(from), ...to.flatMap(fieldsOf)]
    assert.equal(all.reduce((n, f) => n + f.dispatched, 0), 0, `seed ${seed}: an event was dispatched`)
    for (const t of to) {
      const key = t.getAttribute("data-key")
      const was = from.kids.find(k => k.getAttribute("data-key") === key)
      fieldsOf(t).forEach((g, i) => {
        const f = was === undefined ? undefined : fieldsOf(was)[i]
        if (g.type === "checkbox") return assert.equal(g.checked, g.defaultChecked, `seed ${seed}: a box is the page's value`)
        const carried = f !== undefined && f.value !== f.defaultValue && f.defaultValue === g.defaultValue
        assert.equal(g.value, carried ? f.value : g.defaultValue, `seed ${seed}: row ${key ?? ""} field ${i}`)
      })
    }
  }))

test("law: the field holding the keyboard hands it to its counterpart, typed into or not", async () => {
  for (let seed = 1; seed <= 50; seed++) {
    const r = (await import("./gen")).rng(seed)
    const before = rows(r)
    const from = new El("DIV").add(...before.map(rowOf))
    const fields = fieldsOf(from).filter(f => f.type !== "checkbox")
    const held = r.pick(fields)
    if (r.bool()) held.value = "typed"
    Object.assign(globalThis, { document: { activeElement: held } })
    const to = [...before].reverse().map(rowOf)
    typed.carry(from as unknown as Element, to as unknown as Node[])
    await Promise.resolve()
    const key = held.closest("[data-key]")?.getAttribute("data-key")
    const i = fieldsOf(held.parent ?? from).indexOf(held)
    const counterpart = fieldsOf(to.find(t => t.getAttribute("data-key") === key) ?? from)[i]
    assert.equal(counterpart?.focused, true, `seed ${seed}`)
  }
  Object.assign(globalThis, { document: { activeElement: null } })
})
