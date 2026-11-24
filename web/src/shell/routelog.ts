// The address register's history: every place this tab has been, in order, and the pointer to the one it reads now.
// Going somewhere writes past the pointer (what lay ahead is dropped); back and forward move the pointer and write
// nothing, so the place read after a back is the place, and what was kept of it, before the going.

import { type Address, type Route, HOME, bare, hrefOfAddress } from "./route"

export namespace routelog {
  // What the view kept of a place while away from it: the window's scroll and the key of what had the keyboard.
  export type Kept = { readonly scroll: number; readonly focus: string }
  export type Entry = { readonly address: Address; readonly kept: Kept }
  // `at` indexes `entries`, which is never empty.
  export type Log = { readonly entries: readonly Entry[]; readonly at: number }

  export const FRESH: Kept = { scroll: 0, focus: "" }

  export const of = (address: Address): Log => ({ entries: [{ address, kept: FRESH }], at: 0 })

  export const here = (l: Log): Entry => l.entries[l.at] ?? { address: bare(HOME), kept: FRESH }
  export const address = (l: Log): Address => here(l).address
  export const route = (l: Log): Route => here(l).address.route

  // An address is its print: two spellings of one place are one place.
  export const same = (a: Address, b: Address): boolean => hrefOfAddress(a) === hrefOfAddress(b)

  const put = (l: Log, e: Entry): Log => ({ entries: l.entries.map((x, i) => (i === l.at ? e : x)), at: l.at })

  // What the view keeps of the place it is leaving.
  export const keep = (l: Log, kept: Kept): Log =>
    here(l).kept.scroll === kept.scroll && here(l).kept.focus === kept.focus ? l : put(l, { ...here(l), kept })

  export const go = (l: Log, a: Address): Log =>
    same(a, address(l)) ? l : { entries: [...l.entries.slice(0, l.at + 1), { address: a, kept: FRESH }], at: l.at + 1 }

  // Where the reader is stays where it is, read as another place: a share consumed, a page that forwards.
  export const replace = (l: Log, a: Address): Log => (same(a, address(l)) ? l : put(l, { address: a, kept: FRESH }))

  export const back = (l: Log): Log => (l.at === 0 ? l : { ...l, at: l.at - 1 })
  export const forward = (l: Log): Log => (l.at === l.entries.length - 1 ? l : { ...l, at: l.at + 1 })

  // The browser moved to its entry `at`, showing `a`. Where this log wrote that entry, the pointer moves there; an
  // entry it never wrote (one from before this document) starts the log again at `a`.
  export const to = (l: Log, at: number, a: Address): Log => {
    const e = l.entries[at]
    return e !== undefined && same(e.address, a) ? (at === l.at ? l : { ...l, at }) : same(a, address(l)) ? l : of(a)
  }

  export const canBack = (l: Log): boolean => l.at > 0
}
export default routelog
