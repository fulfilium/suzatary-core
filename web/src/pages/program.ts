// A page that is a Rust program (`app/pages`): its patches painted, and every effect of the runtime's sum
// (`app/pages/src/runtime.rs`) performed here, the same for each page. Nothing here knows a page.

import type { Act, ActName, Answer, Basis as CoreBasis, BriefingAt, CalendarAt, Context, ConversationFlags, Core, DraftFlags, Effect, InboxFlags, Input, MailFlags, MessageAt, NeedsYouFlags, Nothing, Reading, Secret, SecretaryFlags, SettingsFlags, ThreadAt, TodoFlags, TodosFlags, TypstFlags, Keyed } from "../wire/generated/pages"
import type { Draw } from "../view/page"
import type { Screen } from "../shell/layout"
import http from "../wire/http"
import d from "../wire/decoder"
import asked from "../app/asked"
import app from "../app/app"
import time from "../app/time"
import lo from "../algebra/lo"
import type reading from "../algebra/reading"
import once from "../algebra/once"
import stream from "../algebra/stream"
import { toast } from "../view/toast"
import frame from "../view/frame"
import address from "../shell/address"
import { parseRoute } from "../shell/route"
import run from "../painter/run"
import scroll from "../painter/scroll"
import blocks from "../app/blocks"
import signer from "../prodrome/signer"
import intentsync from "../prodrome/intentsync"
import type intents from "../prodrome/intents"
import type asks from "../prodrome/asks"
import type questions from "../prodrome/questions"
import vouch from "../prodrome/vouch"
import storesync from "../prodrome/storesync"
import { askHeld, resetReplica } from "../prodrome/sync"
import { lifecycleEvent, loadCore } from "../prodrome/core"
import payload from "../wire/generated/payload"
import parts from "./parts"
import type { KeyTarget } from "../shell/keyboard"
import { clearKeyTarget, setKeyTarget } from "../shell/keyboard"
import local from "./local"

export namespace program {
  // Once per tab; the load is a User Timing measure the bench reads.
  export const module = once.of(async () => {
    const t0 = performance.now()
    const [glue, compiled] = await Promise.all([
      import("./views.js"),
      WebAssembly.compileStreaming(fetch(new URL("./views_bg.wasm", import.meta.url).href)),
    ])
    // V8 compiles a function of the module on its first call, and keeps the code with the module: the rehearsal worker
    // makes those first calls (`rehearse.ts`), and the module is the page's once it has, so a page's first input finds
    // its code compiled. The readings a page asks for come from the core worker, whose restore runs meanwhile.
    await Promise.all([glue.default({ module_or_path: compiled }), rehearsed(compiled)])
    performance.measure("views:load", { start: t0, end: performance.now() })
    return glue
  }).get

  // Settled when the worker says it is done, or fails: a rehearsal that cannot run costs the page only its warmth.
  const rehearsed = (compiled: WebAssembly.Module): Promise<void> =>
    new Promise(done => {
      const worker = new Worker(new URL("./pagework.js", import.meta.url), { type: "module" })
      worker.addEventListener("message", () => done())
      worker.addEventListener("error", () => done())
      worker.postMessage(compiled)
    })

  // Once per zone, ten years back to a day ahead: an instant from older mail is read at the oldest offset.
  const offsets = new Map<string, readonly time.Offset[]>()
  export const offsetsOf = (z: time.Zone): readonly time.Offset[] => {
    const known = offsets.get(z.name)
    if (known !== undefined) return known
    const made = time.offsets(z, time.now() - 10 * 365 * 86_400_000, time.now() + 86_400_000)
    offsets.set(z.name, made)
    return made
  }

  type Who = { readonly me: string | null; readonly reach: app.Reach; readonly wide: boolean }
  const whoOf = (a: app.App): Who => ({ me: app.actor(a), reach: app.reach(a), wide: a.wide })
  const same = (w: Who, was: Who): boolean => w.me === was.me && w.wide === was.wide && app.sameReach(w.reach, was.reach)

  const context = (): Context => {
    const a = app.now()
    const w = whoOf(a)
    const key = a.who.type === "Writes" && (a.who.key.type === "Untrusted" || a.who.key.type === "Revoked") ? a.who.key.type === "Untrusted" ? "untrusted" : "revoked" : "not_the_key"
    return {
      wide: a.wide,
      gate: w.reach.type === "Writes" ? "writes" : w.me !== null ? "reads" : "views",
      reach: w.reach.type === "Reads" ? { type: "Reads", said: w.reach.said, settings: w.reach.settings, key } : { type: w.reach.type },
      offsets: [...offsetsOf(a.zone)],
      categories: [...a.categories],
    }
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

  const settled = (p: Promise<unknown>): Promise<Answer> =>
    answer(p).catch((e: unknown): Answer => ({ type: "Refused", status: e instanceof http.ApiError ? e.status : 0, words: lo.reason(e) }))

  // One register per path, so a page opened again draws what the tab last read.
  const held = asked.family(
    (path: string) => path,
    (path: string) => answer(http.get(path, d.unknown)),
  )

  // For a TypeScript page that reads the same path.
  export const read = (path: string): reading.Reading<Answer> => held(path)
  export const decoded = <A>(a: Answer, decoder: d.Decoder<A>): lo.Settled<A> => (a.type === "Refused" ? lo.error(a.words) : decoder(a.body))

  // Kept in this tab's memory only (no register, replica or storage) so a later act resends it; the page never holds it.
  const secrets = new Map<string, string>()
  const secretOf = (root: HTMLElement, s: Secret): { readonly value: string | undefined } | null => {
    switch (s.type) {
      case "None":
        return { value: undefined }
      case "Field": {
        const field = root.querySelector(`[data-field="${CSS.escape(s.field)}"]`)
        const value = field instanceof HTMLInputElement ? field.value : ""
        secrets.set(s.keep, value)
        return { value }
      }
      case "Kept": {
        const value = secrets.get(s.kept)
        secrets.delete(s.kept)
        return value === undefined ? null : { value }
      }
    }
  }

  // An act as the tab's set of intents names it: a topic with the instant it is asked at, and a lifecycle event spelled
  // by the core at that instant as the actor this device writes as, to be sealed.
  const ask = async (a: Act): Promise<asks.Ask> =>
    a.type === "Topic" ? { type: "Topic", write: a.write, at: time.moment() }
    : a.type === "Lifecycle" ? { type: "Seal", chain: a.chain, event: lifecycleEvent(await loadCore(), a.kind, a.todo, time.moment(), app.actor(app.now()) ?? "", a.note) }
    : a

  // A question for the core in the worker, its terms and reading read back into the questions' own types; a reading
  // answers with the chains held beside it.
  const name = d.obj({ chain: d.str, set: d.str, at: d.str })
  const decoded_ = <A>(r: lo.Settled<A>): A => {
    if (r.type === "Error") throw new Error(`a question a page asked does not read: ${r.e}`)
    return r.v
  }
  const basisOf = (b: CoreBasis): questions.Basis =>
    b.type === "Fold"
      ? { type: "Fold", reading: decoded_(name(b.reading)) }
      : { type: "Rows", at: b.at, functions: decoded_(d.rec(payload.SpecNode)(b.functions)) }
  const placed: d.Decoder<questions.Placed> = d.obj({
    todo: d.str,
    title: d.str,
    category: d.str,
    worth: d.sum<questions.Placed["worth"], "type">("type", { Value: { v: d.nullable(d.num) }, Unlinked: {} }),
    specs: d.arr(payload.SpecNode),
  })
  const core = (q: Core): Promise<unknown> => {
    switch (q.type) {
      case "Reading":
        return askHeld(() => ({ type: "Reading", chain: q.chain, at: q.at }), "Page").then(({ held: h, answer: read }) => ({ held: h.held.map(c => c.name), read }))
      case "Thirty":
        return askHeld(() => ({ type: "Thirty", chain: q.chain, id: q.id, ms: q.ms }), "Page").then(r => r.answer)
      case "Tree": {
        const basis = basisOf(q.basis)
        return askHeld(() => ({ type: "Tree", basis, members: decoded_(d.arr(placed)(q.members)), by: q.by }), "Page").then(r => r.answer)
      }
    }
  }

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
    readonly InboxPage: Omit<InboxFlags, "context" | "now">
    readonly SecretaryPage: Omit<SecretaryFlags, "context" | "now">
    readonly ConversationPage: Omit<ConversationFlags, "context" | "now">
    readonly DraftPage: Omit<DraftFlags, "context">
    readonly NeedsYouPage: Omit<NeedsYouFlags, "context">
    readonly TodoPage: Omit<TodoFlags, "context" | "now">
    readonly TodosPage: Omit<TodosFlags, "context" | "now">
    readonly TypstPage: Omit<TypstFlags, "context">
  }
  export type Page = keyof Args
  type In = Input<unknown, unknown, unknown>
  type Out = Effect<unknown, unknown, unknown>

  // The scroll box is the document's, or one the page draws itself, found once drawn.
  export type Around = { readonly box?: () => Element; readonly told?: (said: unknown) => void; readonly absorbs?: (screen: Screen) => boolean }
  const page_ = (): Element => document.scrollingElement ?? document.documentElement

  // Only the module is awaited: the page draws when the caller calls what this returns.
  export const mount = async <P extends Page>(page: P, args: Args[P], around: Around = {}): Promise<Draw> => {
    const box = around.box ?? page_
    const glue = await module()
    return root => {
      // By name: a new one under a name lets the one before go.
      const held_ = new Map<string, () => void>()
      const timers = new Set<ReturnType<typeof setTimeout>>()
      const hold = (name: unknown, stop: () => void): void => {
        const k = JSON.stringify(name)
        held_.get(k)?.()
        held_.set(k, stop)
      }
      const drawn = (r: Reading<Answer>): void => (r.type === "Loading" ? undefined : frame.drawn())
      const handle: run.Handler<In, Out> = (e, send, stepped) => {
        const answered = (name: unknown) => (a: Answer): void => send({ type: "Answered", ask: name, answer: a })
        switch (e.type) {
          case "Hold": {
            const q = held(e.path)
            const tell = (): void => {
              const r = q.now()
              drawn(r)
              send({ type: "Held", ask: e.ask, reading: r, behind: q.behind() })
            }
            hold(e.ask, q.hear(tell))
            return tell()
          }
          case "Watch": {
            const r = local.of(e.local, box())
            const tell = (): void => {
              const now = lo.map(r.now(), (body): Answer => ({ type: "Done", body }))
              drawn(now)
              send({ type: "Held", ask: e.ask, reading: now, behind: null })
            }
            hold(e.ask, r.hear(tell))
            return tell()
          }
          case "Keep":
            return local.keep(e.register, e.value)
          case "Keys": {
            const press = (key: Keyed) => (): void => send({ type: "Heard", ask: e.ask, frame: JSON.stringify(key) })
            const on = new Set(e.keys)
            const target: KeyTarget = {
              ...(on.has("Move") ? { move: (by: number) => press({ type: "Move", by })() } : {}),
              ...(on.has("Open") ? { open: press({ type: "Open" }) } : {}),
              ...(on.has("Close") ? { close: press({ type: "Close" }) } : {}),
              ...(on.has("Act") ? { act: (name: ActName) => press({ type: "Act", name })() } : {}),
              ...(on.has("Undo") ? { undo: press({ type: "Undo" }) } : {}),
              ...(on.has("Redo") ? { redo: press({ type: "Redo" }) } : {}),
              ...(on.has("Search") ? { focusSearch: press({ type: "Search" }) } : {}),
              ...(on.has("Add") ? { add: press({ type: "Add" }) } : {}),
              ...(on.has("Escape") ? { escape: () => (press({ type: "Escape" })(), true) } : {}),
            }
            setKeyTarget(target)
            return hold(e.ask, () => clearKeyTarget(target))
          }
          case "Sync":
            return void storesync.sync()
          case "Listen":
            return hold(
              e.ask,
              stream.open(stream.of(e.path), { opened: () => send({ type: "Opened", ask: e.ask }), frame: frame => send({ type: "Heard", ask: e.ask, frame }) }, stream.browser),
            )
          case "Again":
            return held(e.path).again()
          case "Rendered":
            return void blocks.store.join(e.pairs).catch(() => [])
          case "Get":
            return void settled(http.get(e.path, d.unknown)).then(answered(e.ask))
          case "Post":
            return void settled(http.act(e.path, e.body, d.unknown)).then(answered(e.ask))
          case "NewKey":
            return void settled(signer.renew().then(() => null)).then(answered(e.ask))
          case "Act": {
            const secret = secretOf(root, e.secret)
            if (secret === null) return answered(e.ask)({ type: "Done", body: { type: "Already" } })
            // Told as answered, a taken answer is the page's at once while the read-back runs behind it; any other answer, or
            // one told as held, waits for the read-back, which says whether the box did it.
            let told = false
            const heard = (a: intents.Answer): void => {
              if (e.told !== "Answered" || a.type !== "Taken") return
              told = true
              answered(e.ask)({ type: "Done", body: { type: "Answered", answer: a, held: false } })
            }
            return void settled(ask(e.act).then(a => intentsync.ask(a, secret.value, heard))).then(a => (told ? undefined : answered(e.ask)(a)))
          }
          case "Endorse":
            return void settled(vouch.endorsement(e.actor, e.key)).then(answered(e.ask))
          case "Timer": {
            const t = setTimeout(() => (timers.delete(t), answered(e.ask)({ type: "Done", body: time.now() })), e.after)
            return void timers.add(t)
          }
          case "Focus": {
            const field = root.querySelector(`[data-field="${CSS.escape(e.field)}"]`)
            const busy = document.activeElement
            if (!(field instanceof HTMLElement)) return
            if (busy !== field && (busy instanceof HTMLInputElement || busy instanceof HTMLTextAreaElement)) return
            field.focus()
            // A field takes the caret at the end of its words; anything else is brought into sight.
            if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) return field.setSelectionRange(field.value.length, field.value.length)
            return field.scrollIntoView({ block: "nearest" })
          }
          case "Scroll":
            return scroll.move(box(), root, e.to, stepped.fromEnd)
          case "Copy":
            return void navigator.clipboard.writeText(e.text).then(() => toast("Copied"), (x: unknown) => toast(lo.reason(x)))
          case "Go":
            return address.go(parseRoute(e.href))
          case "Replace":
            return address.replace(parseRoute(e.href))
          case "Back":
            return address.back(parseRoute(e.fallback))
          case "Toast":
            return void toast(e.words)
          case "Offer":
            return void toast(e.words, { label: e.label, onClick: () => answered(e.ask)({ type: "Done", body: null }) })
          case "Core":
            return void settled(core(e.question)).then(answered(e.ask))
          case "Reset":
            return void resetReplica(new Set(e.losses))
          case "Tell":
            return around.told?.(e.said)
          case "Mark":
            return void requestAnimationFrame(() => (performance.clearMarks(e.name), performance.mark(e.name)))
        }
      }
      // The painter draws into an empty root; the page before left its own.
      root.replaceChildren()
      const t0 = performance.now()
      const program = new glue[page](JSON.stringify({ ...args, context: context(), now: time.now() }))
      const running = run.start<In, Out>(root, program, parts.draw, blocks.held, handle, parts.gone, box, parts.refresh)
      performance.measure("page:draw", { start: t0, end: performance.now(), detail: { page, read: "Program" } })
      const unheard = app.hear(whoOf, () => running.send({ type: "Context", ...context() }), same)
      return {
        ...(around.absorbs === undefined ? {} : { absorb: around.absorbs }),
        destroy: () => {
          unheard()
          held_.forEach(stop => stop())
          timers.forEach(t => clearTimeout(t))
          running.destroy()
        },
      }
    }
  }
}
export default program
