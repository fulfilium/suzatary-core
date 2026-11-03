// A page that is a Rust program (`app/pages`): its module started from the tab's context and the screen's arguments, its
// patches painted, and every effect of the runtime's sum (`app/pages/src/runtime.rs`) performed here, the same for each
// page. Nothing here knows a page: a page names the box's paths, the app's addresses and its own questions, and this
// asks, goes and says what it was told.

import type { Answer, BriefingAt, CalendarAt, Context, Effect, Input, MailFlags, MessageAt, Nothing, Reading, SettingsFlags, ThreadAt } from "../wire/generated/pages"
import type { Mounted } from "../view/page"
import http from "../wire/http"
import d from "../wire/decoder"
import asked from "../app/asked"
import app from "../app/app"
import time from "../app/time"
import lo from "../algebra/lo"
import type reading from "../algebra/reading"
import once from "../algebra/once"
import { toast } from "../view/toast"
import frame from "../view/frame"
import address from "../shell/address"
import { parseRoute } from "../shell/route"
import run from "../painter/run"
import blocks from "../app/blocks"
import signer from "../prodrome/signer"
import parts from "./parts"
import local from "./local"

export namespace program {
  // The module and its glue in one round trip, once per tab; the load is a User Timing measure the bench reads.
  export const module = once.of(async () => {
    const t0 = performance.now()
    const [glue, response] = await Promise.all([import("./views.js"), fetch(new URL("./views_bg.wasm", import.meta.url).href)])
    await glue.default({ module_or_path: response })
    performance.measure("views:load", { start: t0, end: performance.now() })
    return glue
  }).get

  // The owner's zone over the ten years before now and the day after, read once per zone: an instant a page names from
  // older mail is read at the oldest offset.
  const offsets = new Map<string, readonly time.Offset[]>()
  export const offsetsOf = (z: time.Zone): readonly time.Offset[] => {
    const known = offsets.get(z.name)
    if (known !== undefined) return known
    const made = time.offsets(z, time.now() - 10 * 365 * 86_400_000, time.now() + 86_400_000)
    offsets.set(z.name, made)
    return made
  }

  // Who writes, and whether this device writes as them: what a page's gate is read from.
  type Who = { readonly me: string | null; readonly writes: boolean }
  const whoOf = (a: app.App): Who => ({ me: app.actor(a), writes: app.writes(a) })
  const same = (w: Who, was: Who): boolean => w.me === was.me && w.writes === was.writes

  const context = (): Context => {
    const w = whoOf(app.now())
    return { gate: w.writes ? "writes" : w.me !== null ? "reads" : "views", offsets: [...offsetsOf(app.now().zone)] }
  }

  // A 4xx is the box deciding no, an answer the page reads; no answer, or a 5xx, is a failure.
  const answer = (p: Promise<unknown>): Promise<Answer> =>
    p.then(
      (body): Answer => ({ type: "Done", body }),
      (e: unknown): Answer => {
        if (e instanceof http.ApiError && !http.unanswered(e)) return { type: "Refused", status: e.status, words: e.message }
        throw e
      },
    )

  // Once, whatever came: what a one-time ask or an act hands back.
  const settled = (p: Promise<unknown>): Promise<Answer> =>
    answer(p).catch((e: unknown): Answer => ({ type: "Refused", status: e instanceof http.ApiError ? e.status : 0, words: lo.reason(e) }))

  // Every held question by its path: one register each, so a page opened again draws what the tab last read.
  const held = asked.family(
    (path: string) => path,
    (path: string) => answer(http.get(path, d.unknown)),
  )

  // The register a page holds `path` by, for a TypeScript page that reads the same path, and a held answer read as an `A`.
  export const read = (path: string): reading.Reading<Answer> => held(path)
  export const decoded = <A>(a: Answer, decoder: d.Decoder<A>): lo.Settled<A> => (a.type === "Refused" ? lo.error(a.words) : decoder(a.body))

  // The pages the module exports, by the class `viewtree::export!` names each, with what a screen hands each.
  type Args = {
    readonly MailPage: Omit<MailFlags, "context">
    readonly MessagePage: MessageAt
    readonly ThreadPage: ThreadAt
    readonly MemoryPage: Nothing
    readonly AboutPage: Nothing
    readonly CalendarPage: CalendarAt
    readonly BriefingsPage: Nothing
    readonly BriefingPage: BriefingAt
    readonly SettingsPage: Omit<SettingsFlags, "context">
  }
  export type Page = keyof Args
  type In = Input<unknown, unknown>
  type Out = Effect<unknown, unknown>

  export const mount = async <P extends Page>(root: HTMLElement, page: P, args: Args[P]): Promise<Mounted> => {
    const glue = await module()
    const stops: (() => void)[] = []
    // Settles the shell's wait on the first page drawn, once something was read.
    const drawn = (r: Reading<Answer>): void => (r.type === "Loading" ? undefined : frame.drawn())
    const handle: run.Handler<In, Out> = (e, send) => {
      switch (e.type) {
        case "Hold": {
          const q = held(e.path)
          const tell = (): void => {
            const reading = q.now()
            drawn(reading)
            send({ type: "Held", ask: e.ask, reading, behind: q.behind() })
          }
          stops.push(q.hear(tell))
          return tell()
        }
        case "Watch": {
          const r = local.of(e.local)
          const tell = (): void => send({ type: "Held", ask: e.ask, reading: lo.map(r.now(), (body): Answer => ({ type: "Done", body })), behind: null })
          stops.push(r.hear(tell))
          return tell()
        }
        case "Again":
          return held(e.path).again()
        case "Get":
          return void settled(http.get(e.path, d.unknown)).then(a => send({ type: "Answered", ask: e.ask, answer: a }))
        case "Post":
          return void settled(http.act(e.path, e.body, d.unknown)).then(a => send({ type: "Answered", ask: e.ask, answer: a }))
        case "NewKey":
          return void settled(signer.renew().then(() => null)).then(a => send({ type: "Answered", ask: e.ask, answer: a }))
        case "Go":
          return address.go(parseRoute(e.href))
        case "Back":
          return address.back(parseRoute(e.fallback))
        case "Toast":
          return void toast(e.words)
      }
    }
    // The painter draws into an empty root, as a fresh draw does; the page before left its own.
    root.replaceChildren()
    const t0 = performance.now()
    const program = new glue[page](JSON.stringify({ ...args, context: context() }))
    const running = run.start<In, Out>(root, program, parts.draw, blocks.held, handle, parts.gone)
    performance.measure("page:draw", { start: t0, end: performance.now(), detail: { page, read: "Program" } })
    stops.push(app.hear(whoOf, () => running.send({ type: "Context", ...context() }), same))
    return { destroy: () => (stops.forEach(stop => stop()), running.destroy()) }
  }
}
export default program
