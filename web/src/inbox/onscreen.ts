// Each todo's title (its body's first line, marks dropped) off the fold on screen, read again as it moves, so a link
// opened before the todos arrived names its sources once they have.

import type { Entry } from "../wire/generated/wire"
import reading from "../algebra/reading"
import onscreen, { store } from "../app/store"

export namespace onScreen {
  export type Titles = ReadonlyMap<string, string>

  const titlesOf = (entries: readonly Entry[]): Titles =>
    new Map(entries.flatMap((e): [string, string][] => {
      const t = ((e.authored?.body ?? "").split("\n")[0] ?? "").replace(/[#*_\\[\]]/g, "").trim()
      return t === "" ? [] : [[e.todo, t]]
    }))

  const same = (a: Titles, b: Titles): boolean => a.size === b.size && [...a].every(([k, t]) => b.get(k) === t)

  export const titles: reading.Reading<Titles> = reading.settled(
    reading.map(reading.held(() => onscreen.entries(store.state()), heard => store.subscribe(() => heard())), titlesOf),
    same,
  )
}
export default onScreen
