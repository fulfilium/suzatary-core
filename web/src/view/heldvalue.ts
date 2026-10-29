// A held value's placeholder as the box writes it into mail and file text (`suzatary_core::held_value`), and the chip a tap reveals.

import { el } from "./dom"

export namespace heldvalue {
  export type Token =
    | { type: "Text"; text: string }
    | { type: "Held"; kind: string; hint: string; handle: string | null; spelled: string }

  // What a tap read back: a number, a file's bytes in base64, or nothing when the box no longer holds it or answered a replay.
  export type Opened = { type: "Shown"; value: string } | { type: "File"; name: string; media: string; bytes: string } | { type: "Gone" }

  export type Reveal = (handle: string) => Promise<Opened>

  // A number's kind and last digits, or `file` and the file's name and size; the handle names a message's value, a file's value, or a file.
  const PATTERN =
    /\[(?:(SSN|ITIN|TIN|EIN|verification code|card|account|routing number|IBAN)(?: ending (\d{1,4}))?|(file) ([^[\]·\r\n]+?, (?:under 1 KB|\d+\.\d [KM]B))) · (?:tap to show · (held:(?:file:)?[A-Za-z0-9_-]+\.(?:\d+|file))|withheld)\]/g

  export const text = (t: string): Token => ({ type: "Text", text: t })

  export const fold = <R>(t: Token, onText: (text: string) => R, onHeld: (kind: string, hint: string, handle: string | null) => R): R =>
    t.type === "Text" ? onText(t.text) : onHeld(t.kind, t.hint, t.handle)

  export const print = (t: Token): string => (t.type === "Text" ? t.text : t.spelled)

  // Total: the tokens print back to the input.
  export const tokens = (s: string): Token[] => {
    const out: Token[] = []
    let at = 0
    for (const m of s.matchAll(PATTERN)) {
      const start = m.index ?? 0
      if (start > at) out.push(text(s.slice(at, start)))
      out.push({ type: "Held", kind: m[1] ?? m[3] ?? "", hint: m[2] ?? m[4] ?? "", handle: m[5] ?? null, spelled: m[0] })
      at = start + m[0].length
    }
    return at < s.length ? [...out, text(s.slice(at))] : out
  }

  export const holds = (s: string): boolean => tokens(s).some((t) => t.type === "Held")

  const isFile = (kind: string): boolean => kind === "file"

  // As the box's card names it: `SSN ending 3456`, a code, whose digits it never shows, or `file w2.pdf, 48.2 KB`.
  const described = (kind: string, hint: string): string => (hint === "" ? kind : isFile(kind) ? `${kind} ${hint}` : `${kind} ending ${hint}`)

  const dotted = (kind: string, hint: string): string => (isFile(kind) ? `${kind} ${hint}` : `${kind} ${hint === "" ? "••••••" : `••••${hint}`}`)

  // The file as a link on this device, named as the box read it, never as a model wrote it; its object URL lives in this node
  // until the link is used once, then it is revoked.
  const fileLink = (name: string, media: string, bytes: string): HTMLElement => {
    const raw = atob(bytes)
    const data = new Uint8Array(raw.length)
    for (let i = 0; i < raw.length; i++) data[i] = raw.charCodeAt(i)
    const url = URL.createObjectURL(new Blob([data], { type: media }))
    const link = el("a", { href: url, download: name, class: "heldfile" }, `Save ${name}`)
    link.addEventListener("click", () => {
      setTimeout(() => {
        URL.revokeObjectURL(url)
        link.replaceWith(document.createTextNode(`${name} (saved)`))
      }, 0)
    })
    return link
  }

  // The last digits a placeholder shows of a value, `""` when it shows none.
  const lastDigits = (value: string): string => {
    const digits = value.replace(/[^0-9]/g, "")
    return digits.length >= 9 ? digits.slice(-4) : ""
  }

  // Dots and the last digits, or the file's name; on a tap the value or the file's link replaces them in place, kept nowhere but this node.
  export const chip = (kind: string, hint: string, handle: string | null, reveal: Reveal): HTMLElement => {
    const label = el("span", { class: "heldlabel" }, dotted(kind, hint))
    const box = el("span", { class: "heldvalue", title: handle === null ? `${described(kind, hint)}, withheld` : described(kind, hint) }, label)
    if (handle === null) return box
    const button = el("button", { type: "button", class: "heldshow" }, isFile(kind) ? "Open" : "Show")
    button.addEventListener("click", () => {
      button.disabled = true
      reveal(handle).then(
        (opened) => {
          switch (opened.type) {
            case "Shown":
              label.textContent =
                lastDigits(opened.value) === hint ? opened.value : `${dotted(kind, hint)} (changed since it was read; read it again)`
              break
            case "File":
              label.replaceChildren(fileLink(opened.name, opened.media, opened.bytes))
              break
            case "Gone":
              label.textContent = `${dotted(kind, hint)} (no longer held; ${isFile(kind) ? "read the file" : "open the message"} again)`
              break
          }
          button.remove()
        },
        () => {
          button.disabled = false
        },
      )
    })
    box.appendChild(button)
    return box
  }

  export const nodes = (s: string, reveal: Reveal): Node[] =>
    tokens(s).map((t) =>
      fold<Node>(t, (x) => document.createTextNode(x), (kind, hint, handle) => chip(kind, hint, handle, reveal)),
    )

  // Every placeholder in `root`'s text, a model's answer or a mail body, drawn as its chip.
  export const within = <N extends Element>(root: N, reveal: Reveal): N => {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
    const found: Text[] = []
    for (let n = walker.nextNode(); n !== null; n = walker.nextNode()) if (n instanceof Text && holds(n.data)) found.push(n)
    for (const t of found) t.replaceWith(...nodes(t.data, reveal))
    return root
  }
}
export default heldvalue
