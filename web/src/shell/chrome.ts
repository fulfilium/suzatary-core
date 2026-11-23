// What the shell draws around the page: plain data made from App and the fold it dates, painted part by part.

import app from "../app/app"
import aside from "../chat/aside"
import nav from "./nav"
import excluded from "../prodrome/excluded"
import heads from "../prodrome/heads"
import stale from "./stale"
import type onscreen from "../app/store"
import type shell from "../app/shell"
import type { Theme } from "../app/theme"
import type { Opens } from "../wire/generated/pages"

export namespace chrome {
  export type Chrome = {
    nav: nav.View
    panel: aside.Placement
    live: Live
    stale: stale.Banner
    excluded: excluded.Left
    signing: Signing
    build: shell.Line
  }
  // What the banner dates besides App: the fold on screen and what its readings of the objects it holds left out; the
  // theme the More sheet offers; and whether the window has room for the panel beside the page.
  export type Seen = { fold: onscreen.Fold; excluded: excluded.Left; theme: Theme; room: Opens }
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

  // One door to the secretary on each width: on a phone the Secretary tab, and a page's own "Ask the secretary" for
  // what it shows; on a wide screen the panel (`aside.placement`), whose toggle carries what the page shows, so the
  // page's own link is not drawn there (`style.css`, beside the panel's rules).
  export const of = (a: app.App, seen: Seen): Chrome => {
    return {
      nav: nav.of(a, seen.theme),
      panel: aside.placement(a, seen.room),
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
      part("panel", now)
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
