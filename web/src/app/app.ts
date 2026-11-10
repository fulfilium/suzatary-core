// The shell's whole state: where the tab is, the chain it reads, who it writes as, the stream, the nav and the build's line.

import { isChainName, remembered } from "./chain"
import shell from "./shell"
import source from "../algebra/source"
import heads from "../prodrome/heads"
import { HOME, type Route } from "../shell/route"
import routelog from "../shell/routelog"
import { type Screen, forwarded, screenFor } from "../shell/layout"
import time from "./time"
import type { WaitingPayload } from "../wire/generated/wire"
import ownerzone from "./ownerzone"
import login from "./login"

export namespace app {
  export type App = {
    // Where the tab is, and where it has been: the route register's log.
    log: routelog.Log
    // Whether the place read now was reached by moving the log's pointer (back, forward), so it is read again.
    again: boolean
    wide: boolean
    chain: string
    who: Who
    served: readonly string[]
    categories: readonly string[]
    zone: time.Zone
    waiting: WaitingPayload | null
    more: boolean
    stream: Stream
    build: shell.Line
    // What this device holds of a revoked key's: `Undeleted` once a wipe finishing at load failed again, so this page
    // stays sealed and writes nothing.
    held: Held
  }
  export type Held = { type: "Kept" } | { type: "Undeleted" }
  // `Asking` until `/api/me` answers, `Unmet` when it did not. A writer's key is `Unchecked` until the replica has read
  // its standing; one the chain does not trust, or a browser that cannot sign, only reads: said, never failed.
  export type Who =
    | { type: "Asking" }
    | { type: "Unmet" }
    | { type: "Box" }
    | { type: "Reads"; login: string }
    | { type: "Writes"; login: string; actor: string; key: Key }
  export type Key = { type: "Unchecked" } | { type: "Trusted" } | { type: "Untrusted" } | { type: "Revoked" } | { type: "Unsupported"; e: string }
  // What this tab may do where a box to write in would be: write, say nothing yet, or say why not and where to go.
  export type Reach = { type: "Writes" } | { type: "Waits" } | { type: "Reads"; said: string; settings: boolean }
  // `heads` are this connection's; `link` is what the live dot last said, kept across a chain switch.
  export type Stream = { heads: heads.Heads; link: Link }
  export type Link = { type: "Quiet" } | { type: "Connected" } | { type: "Named"; heads: heads.Heads } | { type: "Off" }

  export type Event =
    | { type: "Went"; route: Route }
    | { type: "Replaced"; route: Route }
    | { type: "Moved"; at: number; route: Route }
    | { type: "Kept"; kept: routelog.Kept }
    | { type: "Resized"; wide: boolean }
    | { type: "Pinned"; chain: string }
    | { type: "Met"; me: login.Met; served: readonly string[]; categories: readonly string[]; zone: time.Zone }
    | { type: "Unmet" }
    | { type: "Keyed"; key: Key }
    | { type: "Waiting"; waiting: WaitingPayload }
    | { type: "More"; open: boolean }
    | { type: "Opened" }
    | { type: "Heard"; heads: heads.Heads }
    | { type: "GaveUp" }
    | { type: "Build"; line: shell.Line }
    | { type: "Undeleted" }

  export const ASKING: Who = { type: "Asking" }
  export const QUIET: Stream = { heads: heads.NONE, link: { type: "Quiet" } }
  export const NO_LINE: shell.Line = { type: "None" }

  export const initial = (route: Route, wide: boolean, chain: string, zone: time.Zone = time.UTC): App => ({
    log: routelog.of(forwarded(route)),
    again: false,
    wide,
    chain,
    who: ASKING,
    served: [],
    categories: [],
    zone,
    waiting: null,
    more: false,
    stream: QUIET,
    build: NO_LINE,
    held: { type: "Kept" },
  })

  const sameList = (a: readonly string[], b: readonly string[]) => a.length === b.length && a.every((x, i) => x === b[i])

  const pin = (a: App, chain: string): App =>
    !isChainName(chain) || chain === a.chain ? a : { ...a, chain, stream: { ...a.stream, heads: heads.NONE } }

  // How a new place was reached; a step that wrote no place leaves it.
  const via = (a: App, b: App, again: boolean): App => (b.log === a.log || b.again === again ? b : { ...b, again })

  // A new place closes the sheet; a list pins the chain its destination names.
  const placed = (a: App, log: routelog.Log, wide: boolean): App => {
    const moved = !routelog.same(routelog.route(log), route(a))
    const next = log === a.log && wide === a.wide ? a : { ...a, log, wide, more: a.more && !moved }
    const s = screenFor(routelog.route(log), wide)
    return s.type === "todos" && s.chain !== "" ? pin(next, s.chain) : next
  }

  // The writer named again keeps what its key was read to be; anyone else is who the box says.
  const whoOf = (me: login.Met, was: Who): Who =>
    me.type !== "Writes" ? me
    : was.type === "Writes" && was.login === me.login && was.actor === me.actor ? was
    : { ...me, key: { type: "Unchecked" } }

  const met = (a: App, who: Who, served: readonly string[], categories: readonly string[], zone: time.Zone): App => {
    const w = sameWho(who, a.who) ? a.who : who
    const s = sameList(served, a.served) ? a.served : served
    const c = sameList(categories, a.categories) ? a.categories : categories
    const z = zone.name === a.zone.name ? a.zone : zone
    return w === a.who && s === a.served && c === a.categories && z === a.zone ? a : { ...a, who: w, served: s, categories: c, zone: z }
  }

  const actorOf = (w: Who): string | null => {
    switch (w.type) {
      case "Asking":
      case "Unmet":
      case "Box":
      case "Reads":
        return null
      case "Writes":
        return w.actor
    }
  }
  const sameKey = (k: Key, was: Key): boolean => k.type === was.type && (k.type !== "Unsupported" || (was.type === "Unsupported" && k.e === was.e))
  const sameWho = (w: Who, was: Who): boolean => {
    switch (w.type) {
      case "Asking":
      case "Unmet":
      case "Box":
        return was.type === w.type
      case "Reads":
        return was.type === "Reads" && was.login === w.login
      case "Writes":
        return was.type === "Writes" && was.login === w.login && was.actor === w.actor && sameKey(w.key, was.key)
    }
  }

  // A count is whole and never negative; zero draws no badge.
  const counted = (n: number) => Math.max(0, Math.trunc(n))

  const linked = (a: App, link: Link): App => (a.stream.link.type === link.type ? a : { ...a, stream: { ...a.stream, link } })

  const heard = (a: App, h: heads.Heads): App =>
    heads.same(h, a.stream.heads) ? a : { ...a, stream: { ...a.stream, heads: h, link: { type: "Named", heads: h } } }

  export const step = (a: App, e: Event): App => {
    switch (e.type) {
      case "Went":
        return via(a, placed(a, routelog.go(a.log, forwarded(e.route)), a.wide), false)
      case "Replaced":
        return via(a, placed(a, routelog.replace(a.log, forwarded(e.route)), a.wide), false)
      case "Moved":
        return via(a, placed(a, routelog.to(a.log, e.at, forwarded(e.route)), a.wide), true)
      case "Kept":
        return placed(a, routelog.keep(a.log, e.kept), a.wide)
      case "Resized":
        return placed(a, a.log, e.wide)
      case "Pinned":
        return pin(a, e.chain)
      case "Met":
        return met(a, whoOf(e.me, a.who), e.served, e.categories, e.zone)
      case "Unmet":
        return met(a, { type: "Unmet" }, a.served, a.categories, a.zone)
      case "Keyed":
        return a.who.type !== "Writes" || sameKey(e.key, a.who.key) ? a : { ...a, who: { ...a.who, key: e.key } }
      case "Waiting": {
        const w = { ...e.waiting, count: counted(e.waiting.count) }
        return a.waiting !== null && JSON.stringify(a.waiting) === JSON.stringify(w) ? a : { ...a, waiting: w }
      }
      case "More":
        return e.open === a.more ? a : { ...a, more: e.open }
      case "Opened":
        return linked(a, { type: "Connected" })
      case "Heard":
        return heard(a, e.heads)
      case "GaveUp":
        return linked(a, { type: "Off" })
      case "Build":
        return JSON.stringify(e.line) === JSON.stringify(a.build) ? a : { ...a, build: e.line }
      case "Undeleted":
        return a.held.type === "Undeleted" ? a : { ...a, held: { type: "Undeleted" } }
    }
  }

  export const route = (a: App): Route => routelog.route(a.log)
  export const screen = (a: App): Screen => screenFor(route(a), a.wide)
  export const actor = (a: App): string | null => actorOf(a.who)
  const WRITES: Reach = { type: "Writes" }
  const WAITS: Reach = { type: "Waits" }
  const reads = (said: string, settings = false): Reach => ({ type: "Reads", said, settings })
  const keyReach = (k: Key): Reach => {
    switch (k.type) {
      case "Unchecked":
      case "Trusted":
        return WRITES
      case "Untrusted":
        return reads("This device is not trusted to write yet: trust it from Settings → This device with the code from the box, or ask there, then tap its card on a trusted device.", true)
      case "Revoked":
        return reads("This device's key was revoked, so nothing it writes is taken. Put a new key in its place from Settings → This device, then tap its card on a trusted device.", true)
      case "Unsupported":
        return reads("This browser cannot sign what it writes, so it can read here and not write. Write from a browser that signs.")
    }
  }
  const UNDELETED =
    "This device's key was revoked and it could not delete what it holds, so it writes nothing. Close every tab of this site and open it again: it tries again when it loads."
  // Where a box to write in would be: the box, nothing while the box is asked, or what this device may do and where to
  // write from.
  export const reach = (a: App): Reach => {
    if (a.held.type === "Undeleted") return reads(UNDELETED)
    switch (a.who.type) {
      case "Asking":
        return WAITS
      case "Unmet":
        return reads("The box did not say who this login is, so nothing here writes until it does. This page asks again when it reconnects or comes back.")
      case "Box":
        return reads("This is the box's own browser: it can read here and never write. Write from your phone or your computer on the tailnet.")
      case "Reads":
        return reads(`Signed in as ${a.who.login}, this device can read here and not write. Write from a device signed in as a login that writes.`)
      case "Writes":
        return keyReach(a.who.key)
    }
  }
  export const sameReach = (r: Reach, was: Reach): boolean =>
    r.type === was.type && (r.type !== "Reads" || (was.type === "Reads" && r.said === was.said && r.settings === was.settings))
  export const writes = (a: App): boolean => reach(a).type === "Writes"
  export const unsigning = (a: App): string | null =>
    a.held.type === "Undeleted"
      ? UNDELETED
      : a.who.type === "Writes" && a.who.key.type === "Unsupported"
      ? `This browser cannot sign what it writes (${a.who.key.e}), and this deployment takes only signed writes, so here you can read and not write.`
      : null
  export const live = (a: App): "unknown" | "on" | "off" =>
    a.stream.link.type === "Quiet" ? "unknown" : a.stream.link.type === "Off" ? "off" : "on"
  // The one place a chain becomes a URL; the routes that are no chain's (mail, briefings, devices) do not use it.
  export const apiBase = (a: App): string => `/api/${encodeURIComponent(a.chain)}`

  // This tab's App. The shell dispatches what it hears; every other module reads it, or hears one value of it move.
  export const tab = source.create(initial(HOME, false, remembered(), ownerzone.remembered()), step)
  export const now = (): App => tab.state()
  export const dispatch = (e: Event): void => tab.dispatch(e)
  export const hear = <T>(of: (a: App) => T, heard: (t: T) => void, same: (t: T, was: T) => boolean = Object.is): (() => void) => {
    let last = of(now())
    return tab.subscribe(a => {
      const t = of(a)
      if (same(t, last)) return
      last = t
      heard(t)
    })
  }
}
export default app
