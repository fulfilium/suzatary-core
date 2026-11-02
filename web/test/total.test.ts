// A reading is total over what a replica holds (Prodrome SPEC law 44): a print that does not parse at its store's
// schema, as an inbox card from before `Remember` carried its `text` did on the owner's phone, is left out with what
// rests on it, every other reading answers as the box does, and the line says what was left out.

import { test } from "node:test"
import assert from "node:assert/strict"
import { createHash } from "node:crypto"
import { createRequire } from "node:module"
import { readFileSync } from "node:fs"

import type { Core, Lines, Named, Proposals } from "../src/prodrome/core"
import { conversationOf, entriesOf, excludedOf, holdIn, inboxOf, linesLeft, listOfLines, objectsOf, searchOf } from "../src/prodrome/core"
import excluded from "../src/prodrome/excluded"
import type { MemoryPayload } from "../src/wire/generated/wire"

const core = ((): Core => {
  const built = process.env["SUZATARY_PRODROME_WASM"]
  if (built === undefined || built === "") throw new Error("SUZATARY_PRODROME_WASM is unset — run scripts/web-test.sh")
  return createRequire(import.meta.url)(`${built}/nodejs/prodrome.js`) as Core
})()

type Shipped = { hash: string; text: string }
const box = JSON.parse(readFileSync("app/core/tests/web/replicas.json", "utf8")) as {
  at: string
  inbox: Shipped[]
  conversations: Shipped[]
  keys: Record<string, string>
  memory: MemoryPayload
}
const named = (xs: readonly Shipped[]): Named[] => xs.map(o => ({ hash: o.hash, literal: o.text }))
const print = (literal: string): Named => ({ hash: createHash("sha256").update(literal, "utf8").digest("hex"), literal })

const asked = box.inbox.find(o => o.text.includes("act=Remember(text="))
// A dismissal resting on its card; the lagging conversation's own dismissal rests on nothing.
const dismissed = box.inbox.find(o => o.text.includes("event=Dismissed(") && /deps=\('[0-9a-f]{64}',\)/.test(o.text))
if (asked === undefined || dismissed === undefined) throw new Error("replicas.json has no remember card and dismissal")
const LOST = "p00000000000000000000000000000001"
const proposal = (text: string): string => text.replace(/proposal='p[0-9a-f]{32}'/, `proposal='${LOST}'`)
// The card as an older schema printed it, and a dismissal resting on it.
const unread = print(proposal(asked.text).replace(/act=Remember\(text='[0-9a-f]+', /, "act=Remember("))
const resting = print(proposal(dismissed.text).replace(/deps=\('[0-9a-f]{64}',\)/, `deps=('${unread.hash}',)`))

// No chain held: every proposal's price reads as it would with nothing to refer into.
const none = new core.Chains()
const inbox = named(box.inbox)
const conversations = named(box.conversations)
const held = [...inbox, unread, resting]

// The inbox `i` and the box's lines as the worker reads them.
type Reading = { readonly p: Proposals; readonly lines: Lines }
const answers = ({ p, lines }: Reading): unknown[] => [
  inboxOf(p, box.at, box.memory, none),
  listOfLines(lines, p),
  ...listOfLines(lines, p).conversations.map(c => conversationOf(lines, p, c.id, {}, box.memory)),
  searchOf(lines, "deed"),
]
const leftOf = ({ p, lines }: Reading): excluded.T => excluded.join(excludedOf(p), linesLeft(lines))
const reading = <A>(i: readonly Named[], ask: (s: Reading) => A): A => {
  const lines = new core.Lines()
  const p = new core.Proposals(objectsOf(i))
  try {
    lines.hold(objectsOf(conversations), "[]")
    lines.keys(JSON.stringify(box.keys), "[]")
    return ask({ p, lines })
  } finally {
    p.free()
    lines.free()
  }
}

test("law: an unparseable print and what rests on it change no reading of the rest", () => {
  assert.deepEqual(reading(held, answers), reading(inbox, answers))
  assert.deepEqual(reading(inbox, leftOf), excluded.NONE)
})

test("law: what was left out is exactly the print, in the parse's words, with what rests on it", () => {
  const left = reading(held, leftOf)
  assert.deepEqual(left.map(x => [x.name, x.resting]), [[unread.hash, [resting.hash]]])
  assert.match(left[0]?.why ?? "", /^failed to parse: Asked\.act: Remember\(\.\.\.\) has no field text$/)
  assert.equal(excluded.said(excluded.left(left)), "1 object on this device could not be read and was left out (1 resting on it)")
})

test("law: a chain held over an unreadable print reads what it can, and the set says what it left out", () => {
  const junk = print("Sealed(prev=None, event=Nonsense())")
  assert.deepEqual(entriesOf(core, [junk], null, [], "life", none).entries, [])
  const set = new core.Chains()
  try {
    const left = holdIn(set, "life", [junk], [])
    assert.deepEqual(left.objects.map(x => x.name), [junk.hash])
    assert.deepEqual(left.chains, [])
  } finally {
    set.free()
  }
})
