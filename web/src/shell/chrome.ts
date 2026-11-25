import app from "../app/app"
import type aside from "../chat/aside"
import nav from "./nav"
import excluded from "../prodrome/excluded"
import heads from "../prodrome/heads"
import stale from "./stale"
import type onscreen from "../app/store"
import type shell from "../app/shell"
import type { Theme } from "../app/theme"
import type { Route } from "./route"

export namespace chrome {
  export type Chrome = {
    nav: nav.View
    beside: aside.Door
    live: Live
    stale: stale.Banner
    excluded: excluded.Left
    signing: Signing
    build: shell.Line
  }
  // `door` is none where the drawing on screen draws the secretary; `workspace` is the desk's focused one.
  export type Seen = { fold: onscreen.Fold; excluded: excluded.Left; theme: Theme; door: aside.Door; workspace: Route }
  export type Live = { className: string; meaning: string }
  // What the page says of this browser's key: nothing, or why it only reads.
  export type Signing = { said: string | null }

  const liveOf = (l: app.Link): Live =>
    l.type === "Quiet"
      ? { className: "live", meaning: "connecting…" }
      : l.type === "Connected"
        ? { className: "live on", meaning: "live — connected to the chain" }
        : l.type === "Named"
          ? { className: "live on", meaning: `live — heads ${heads.shown(l.heads)}` }
          : { className: "live off", meaning: "no live updates" }

  export const of = (a: app.App, seen: Seen): Chrome => {
    return {
      nav: nav.of(a, seen.theme, seen.workspace),
      beside: seen.door,
      live: liveOf(a.stream.link),
      stale: stale.of(app.live(a), seen.fold),
      excluded: seen.excluded,
      signing: { said: app.unsigning(a) },
      build: a.build,
    }
  }

  export type Paint = { [K in keyof Chrome]: (now: Chrome[K], was: Chrome[K] | null) => void }

  // A part is painted only when its value moved, so a tip heard never rebuilds the nav under a finger.
  export const painter = (paint: Paint): ((c: Chrome) => void) => {
    let was: Chrome | null = null
    const part = <K extends keyof Chrome>(k: K, now: Chrome): void => {
      const w = was === null ? null : was[k]
      if (w === null || JSON.stringify(w) !== JSON.stringify(now[k])) paint[k](now[k], w)
    }
    return now => {
      part("nav", now)
      part("beside", now)
      part("live", now)
      part("stale", now)
      part("excluded", now)
      part("signing", now)
      part("build", now)
      was = now
    }
  }
}
export default chrome
