// The page's half of the app shell. A page of an old build loads its own build's files from its cache, so it reloads
// by itself only when they are gone (`lost`); a newer build is offered across the top, and the one a page started on is
// said once.

import http from "../wire/http"
import type d from "../wire/decoder"
import { el } from "../view/dom"
import lo from "../algebra/lo"
import register from "../algebra/register"
import serial from "../algebra/serial"
import inflight from "./inflight"
import registers from "./registers"
import shell from "./shell"
import ui from "./ui"

export namespace newbuild {
  export type About = { readonly version: string; readonly build: string }

  // `/api/about` read as the worker reads it: two strings, a missing one blank. It is what this page asks when an
  // answer is misread, so it is never misread itself, and one misread is one more ask, never a loop.
  export const ABOUT: d.Decoder<About> = u => {
    const field = (k: string): string => {
      const v: unknown = typeof u === "object" && u !== null && k in u ? Object.getOwnPropertyDescriptor(u, k)?.value : undefined
      return typeof v === "string" ? v : ""
    }
    return lo.val({ version: field("version"), build: field("build") })
  }

  // The line's one value: nothing, the build this page started on, a served build offered, or one the reader dismissed.
  export type State =
    | { readonly type: "None" }
    | { readonly type: "Updated"; readonly version: string; readonly revision: string }
    | { readonly type: "Offered"; readonly to: string; readonly version: string }
    | { readonly type: "Dismissed"; readonly to: string }
  // The box named the build it serves; this page started on a build this device has not announced; the reader
  // dismissed the line.
  export type Event =
    | { readonly type: "Served"; readonly about: About }
    | { readonly type: "Started"; readonly version: string; readonly revision: string }
    | { readonly type: "Dismiss" }

  export const NONE: State = { type: "None" }

  // `own` is this page's build. No build named is no answer; a build taken back is no longer offered; a dismissed one
  // stays dismissed until a newer one.
  export const step = (own: string, s: State, e: Event): State => {
    switch (e.type) {
      case "Served": {
        const b = e.about.build
        if (b === "") return s
        if (b === own) return s.type === "Offered" || s.type === "Dismissed" ? NONE : s
        if (s.type === "Dismissed" && s.to === b) return s
        return s.type === "Offered" && s.to === b && s.version === e.about.version ? s : { type: "Offered", to: b, version: e.about.version }
      }
      case "Started":
        return s.type === "None" ? { type: "Updated", version: e.version, revision: e.revision } : s
      case "Dismiss":
        return s.type === "Offered" ? { type: "Dismissed", to: s.to } : s.type === "Updated" ? NONE : s
    }
  }

  // A page whose build's code would not load moves to the box's build, at its address, when the box serves another;
  // otherwise it stays, so it never reloads in a loop.
  export type Lost = { readonly type: "Moved" } | { readonly type: "Unloaded" }
  export const lost = (own: string, served: string): Lost => (served !== "" && served !== own ? { type: "Moved" } : { type: "Unloaded" })

  export const line = (s: State): shell.Line =>
    s.type === "Updated" ? { type: "Updated", version: s.version, revision: s.revision }
    : s.type === "Offered" ? { type: "Ready", version: s.version }
    : { type: "None" }

  export type Acts = { readonly reload: () => void; readonly dismiss: () => void }

  export const view = (l: shell.Line, acts: Acts): Node[] => {
    const button = (label: string, cls: string, run: () => void) => el("button", { type: "button", class: cls, onclick: run }, label)
    switch (l.type) {
      case "None":
        return []
      case "Updated":
        return [el("span", { class: "updatedwhat" }, `Updated to Suzatary ${l.version} (${l.revision})`), button("Dismiss", "updateddismiss", acts.dismiss)]
      case "Ready":
        return [el("span", { class: "updatedwhat" }, `Suzatary ${l.version} is ready`), button("Reload", "updatedreload", acts.reload), button("Dismiss", "updateddismiss", acts.dismiss)]
    }
  }

  // What the page reaches: the box's build, the build this device last announced, the line, and the reload.
  export type Effects = {
    readonly about: () => Promise<About>
    readonly announced: { readonly read: () => readonly string[]; readonly set: (id: string) => void }
    readonly said: (l: shell.Line) => void
    readonly reload: () => void
  }

  // `check` whenever the page may have missed a deploy (a start, a reconnect, a 426, an answer it cannot read); `lost`
  // when a page's code would not load; and the line's two presses.
  export type Builds = { readonly check: () => Promise<void>; readonly lost: () => Promise<Lost>; readonly acts: Acts }

  export const machine = (own: shell.Build, fx: Effects): Builds => {
    const name = shell.named(own)
    let s: State = NONE
    const go = (e: Event): void => {
      const next = step(name, s, e)
      if (next === s) return
      const said = line(s)
      s = next
      if (JSON.stringify(line(s)) !== JSON.stringify(said)) fx.said(line(s))
    }
    // The build this page started on is said once per device, when the box names it too and so its version.
    const started = (a: About): void => {
      if (a.build !== name) return
      const news = shell.news(fx.announced.read(), own)
      if (news.type === "Known") return
      fx.announced.set(news.id)
      if (news.type === "New") go({ type: "Started", version: a.version, revision: news.id })
    }
    // Each ask starts after the call that wanted it, so a deploy between an answer in flight and the call is heard.
    const check = serial.of(() =>
      fx.about().then(a => {
        started(a)
        go({ type: "Served", about: a })
      }),
    )
    const reload = (): void => void inflight.settled().then(fx.reload)
    return {
      check,
      lost: () =>
        fx.about().then(
          a => {
            go({ type: "Served", about: a })
            const l = lost(name, a.build)
            if (l.type === "Moved") reload()
            return l
          },
          (): Lost => ({ type: "Unloaded" }),
        ),
      acts: {
        // Writes on their way are kept or refused first; the loader then starts the build the box serves.
        reload,
        dismiss: () => go({ type: "Dismiss" }),
      },
    }
  }

  // A dev box keeps nothing: its workers and their caches go.
  const forget = async (sw: ServiceWorkerContainer): Promise<void> => {
    const regs = await sw.getRegistrations()
    await Promise.all(regs.map(r => r.unregister()))
    const names = await caches.keys()
    await Promise.all(names.filter(n => n.startsWith(shell.PREFIX)).map(n => caches.delete(n)))
  }

  // The box's build as heard, kept where the worker answers `b/current` (`app/worker.ts`), so the next start runs it
  // with nothing asked of the network.
  const heard = async (a: About): Promise<About> => {
    if (a.build === "" || !("caches" in window)) return a
    await caches
      .open(shell.HEARD)
      .then(c => c.put(new URL(shell.CURRENT, document.baseURI).href, new Response(a.build)))
      .catch(() => {})
    return a
  }

  export const install = (said: (l: shell.Line) => void): Builds => {
    const own = shell.ownBuild(import.meta.url)
    const announced = ui.cell(registers.announced)
    // The build the box last named: only a page on it registers its worker, whose install asks the same.
    let served = ""
    const builds = machine(own, {
      about: () =>
        http
          .get("/api/about", ABOUT)
          .then(heard)
          .then(a => ((served = a.build), a)),
      announced: { read: () => register.candidates(announced.read()), set: announced.set },
      said,
      reload: () => window.location.reload(),
    })
    const sw = "serviceWorker" in navigator ? navigator.serviceWorker : null
    if (sw === null) {
      void builds.check().catch(() => {})
      return builds
    }
    // The worker asks each open page which build it runs before it lets a build's cache go.
    sw.addEventListener("message", e => {
      if (typeof e.data === "object" && e.data !== null && "type" in e.data && e.data.type === "which") e.ports[0]?.postMessage(shell.named(own))
    })
    // A page on the box's build that no worker serves registers that build's, so a cold start offline runs it; an
    // install that failed is tried again at the next check. The first asks the worker to sweep the caches no page runs.
    let swept = false
    const keep = (): Promise<void> => {
      const serving = sw.controller === null ? null : shell.named(shell.buildOf(sw.controller.scriptURL))
      const settled = (id: string): boolean => serving === id || served !== id
      return shell
        .fold(own, { stamped: id => (settled(id) ? Promise.resolve() : sw.register(shell.script(id)).then(() => {})), dev: () => forget(sw) })
        .then(() => {
          if (swept) return
          swept = true
          sw.controller?.postMessage({ type: "sweep" })
        })
        .catch(() => {})
    }
    const kept: Builds = { ...builds, check: () => builds.check().finally(() => void keep()) }
    void kept.check().catch(() => {})
    return kept
  }
}
export default newbuild
