// Settings drawn by its Rust program (`app/pages/src/settings.rs`), behind the `suzatary.viewtree` flag: the page reads
// what `settings.ts` reads, hands each reading to the program, performs what it asks, and places the Inbox's cards
// where it names them, which are still TypeScript until the Inbox is converted.

import { askTrust, beginSetup, pairDevice } from "../wire/api"
import type { Act, Card, CardAct, Effect, Input, Shown } from "../wire/generated/settings"
import time from "../app/time"
import type { Row } from "../inbox/inbox"
import signer from "../prodrome/signer"
import app from "../app/app"
import { el } from "../view/dom"
import { toast } from "../view/toast"
import frame from "../view/frame"
import lo from "../algebra/lo"
import once from "../algebra/once"
import { hrefOf } from "../shell/route"
import address from "../shell/address"
import run from "../painter/run"
import overview from "./overview"
import settingsPage from "./settings"
import type settingsState from "./settings.state"

export namespace settingsView {
  type T = settingsState.T

  const actOf = (kind: string): CardAct =>
    kind === "sign_in" || kind === "sign_in_code" ? "sign_in" : kind === "secret" ? "secret" : kind === "grant" ? "grant" : "other"

  const cardOf = (r: Row): readonly Card[] => {
    const reference = overview.reference(r)
    return r.kind === "proposal" && reference !== null
      ? [{ reference, act: actOf(r.proposal.act.kind), stands: r.proposal.state === "standing" || r.proposal.state === "failed" ? "open" : "settled", version: JSON.stringify(r.proposal) }]
      : []
  }

  // The owner's zone over the two years before now and the day after, read once per zone: every instant Settings names.
  const offsets = new Map<string, readonly time.Offset[]>()
  const offsetsOf = (z: time.Zone): readonly time.Offset[] => {
    const known = offsets.get(z.name)
    if (known !== undefined) return known
    const made = time.offsets(z, time.now() - 2 * 365 * 86_400_000, time.now() + 86_400_000)
    offsets.set(z.name, made)
    return made
  }

  const shown = (v: T): Shown => ({
    overview: v.overview,
    rows: lo.map(v.rows, rs => rs.flatMap(cardOf)),
    gate: v.writes ? "writes" : "reads",
    ask: v.me === null ? null : hrefOf({ type: "conversation", id: v.overview.ask, refs: [], focus: true }),
    device: v.device,
    offsets: [...offsetsOf(app.now().zone)],
  })

  // The module and its glue in one round trip, once per tab; the load is a User Timing measure the bench reads.
  const module = once.of(async () => {
    const t0 = performance.now()
    const [glue, response] = await Promise.all([import("./views.js"), fetch(new URL("./views_bg.wasm", import.meta.url).href)])
    await glue.default({ module_or_path: response })
    performance.measure("views:load", { start: t0, end: performance.now() })
    return glue
  }).get

  const perform = (a: Act): Promise<unknown> => {
    switch (a.type) {
      case "Begin":
        return beginSetup(a.template).then(c => address.go({ type: "conversation", id: c.id, refs: [], focus: true }))
      case "AskTrust":
        return askTrust(a.key)
      case "Renew":
        return signer.renew()
      case "Pair":
        return pairDevice(a.code)
    }
  }

  // Each act's answer goes back to the program, which decides what it says and whether the page reads again.
  const handle = (again: () => void): run.Handler<Input, Effect> => (e, send) => {
    const answered = (act: Act) => (p: Promise<unknown>): void =>
      void p.then(
        () => send({ type: "Answered", act, outcome: { type: "Done" } }),
        (x: unknown) => send({ type: "Answered", act, outcome: { type: "Refused", words: lo.reason(x) } }),
      )
    switch (e.type) {
      case "Run":
        return answered(e.act)(perform(e.act))
      case "Toast":
        return void toast(e.words)
      case "Again":
        return again()
    }
  }

  // The Inbox's cards, by reference, from the latest reading; the program names only cards that reading holds.
  const cards = (rows: () => readonly Row[]) => (kind: string, key: string, props: string): Node => {
    const row = (ref: string): Row => {
      const r = rows().find(x => overview.reference(x) === ref)
      if (r === undefined) throw new Error(`no card ${ref} in the reading the page was handed`)
      return r
    }
    const node = kind === "allowing" ? settingsPage.allowing({ link: row(key), grant: row(props.split("\n")[0] ?? "") }) : settingsPage.card(row(key))
    if (node === null) throw new Error(`${key} is not a proposal`)
    return node
  }

  type Glue = Awaited<ReturnType<typeof module>>
  // Where the island stands: nothing yet, a value before the module, the module before a value, running, or gone.
  type Running =
    | { readonly type: "Empty" }
    | { readonly type: "Valued"; readonly v: T }
    | { readonly type: "Loaded"; readonly glue: Glue }
    | { readonly type: "Running"; readonly page: run.T<Input> }
    | { readonly type: "Gone" }

  // One island: the painter's root, started once both the module and a value are here, and handed every value after.
  const island = (again: () => void): frame.Island<T> => {
    const root = el("div", { class: "settingsview" })
    let latest: readonly Row[] = []
    let running: Running = { type: "Empty" }
    const start = (glue: Glue, v: T): Running => {
      const t0 = performance.now()
      const page = run.start(root, new glue.SettingsPage(JSON.stringify(shown(v))), cards(() => latest), handle(again))
      performance.measure("page:draw", { start: t0, end: performance.now(), detail: { page: "settingsview", read: "Val" } })
      return { type: "Running", page }
    }
    // Asked as the page opens, so the module loads while the page's readings are asked, never after them.
    module().then(
      glue => (running = running.type === "Empty" ? { type: "Loaded", glue } : running.type === "Valued" ? start(glue, running.v) : running),
      (e: unknown) => toast(`Settings could not load: ${lo.reason(e)}`),
    )
    const update = (v: T): void => {
      latest = lo.withDefault(v.rows, [])
      switch (running.type) {
        case "Empty":
        case "Valued":
          return void (running = { type: "Valued", v })
        case "Loaded":
          return void (running = start(running.glue, v))
        case "Running":
          return running.page.send({ type: "Shown", ...shown(v) })
        case "Gone":
          return
      }
    }
    return {
      node: root,
      update,
      destroy: () => {
        if (running.type === "Running") running.page.destroy()
        running = { type: "Gone" }
      },
    }
  }

  export const page: frame.Page<T, settingsState.Event, "view"> = {
    ...settingsPage.page,
    islands: (_, again) => ({ view: island(again) }),
    view: (_v, _d, _a, at) => at.view ?? null,
  }

  export const mount = (root: HTMLElement) => frame.mount(root, page)
}
export default settingsView
