// Whether the default conversation holds talk about a reference: a reading of the lines this device holds whole, so no
// program runs to know it and nothing is fetched for it. A listener hears it at once and again as that conversation
// moves, and never a value asked before it began, so a page arrived at is never shown by an older talk. The secretary
// beside a page reads the same window (`At::Panel` in `app/pages/src/conversation.rs`).

import { measure } from "../app/perf"
import storesync from "../prodrome/storesync"
import type aside from "./aside"

export namespace talk {
  const DEFAULT = "default"

  // Each answer after the ask it answers was made, the last ask's alone; nothing once let go.
  export const listen = (about: string, heard: (t: aside.Talk) => void): (() => void) => {
    let asked = 0
    let live = true
    const ask = (): void => {
      const n = ++asked
      const t0 = performance.now()
      void storesync.talk(DEFAULT, about, "Shell").then(
        t => {
          measure("talk", t0, { said: t })
          if (live && n === asked) heard(t)
        },
        () => {},
      )
    }
    ask()
    const unwatch = storesync.watchConversation(DEFAULT, ask)
    return () => {
      live = false
      unwatch()
    }
  }
}
export default talk
