// What the shell says, once and on any page, about this tab's acts the box has not taken: one refused, one waiting for
// this device to be trusted, or one still on its way after a few seconds. In words, naming no object.

import type { Unsent } from "../wire/generated/pages"

export namespace outbox {
  // When each act on its way was first seen, and what was last said about each todo.
  export type T = { readonly since: ReadonlyMap<string, number>; readonly told: ReadonlyMap<string, Unsent> }
  // What a step says, if anything, and whether it is about one refused todo, which the line offers to open.
  export type About = { readonly type: "Refused"; readonly todo: string } | { readonly type: "Several" }
  export type Said = { readonly type: "Quiet" } | { readonly type: "Said"; readonly text: string; readonly about: About }

  export const EMPTY: T = { since: new Map(), told: new Map() }
  // How long an act may be on its way before the shell says so.
  export const LATE_MS = 5000

  const plural = (n: number, one: string, many: string): string => (n === 1 ? one : many.replace("#", String(n)))

  const sentence = (refused: number, trust: number, late: number): string =>
    [
      refused === 0 ? "" : plural(refused, "A change was refused. Open its todo to see why.", "# changes were refused. Open their todos to see why."),
      trust === 0 ? "" : plural(trust, "A change waits for this device to be trusted, in Settings.", "# changes wait for this device to be trusted, in Settings."),
      late === 0 ? "" : plural(late, "A change is still on its way. It goes through once the connection is back.", "# changes are still on their way. They go through once the connection is back."),
    ].filter(s => s !== "").join(" ")

  const aboutOf = (due: readonly (readonly [string, Unsent])[]): About => {
    const [only, ...rest] = due
    return only !== undefined && rest.length === 0 && only[1] === "Refused" ? { type: "Refused", todo: only[0] } : { type: "Several" }
  }

  // Said once per todo and state: a state it was already told of says nothing, a todo that settled is forgotten.
  export const step = (t: T, unsent: ReadonlyMap<string, Unsent>, now: number): { readonly t: T; readonly said: Said } => {
    const since = new Map([...unsent].filter(([, u]) => u === "Sending").map(([k]): [string, number] => [k, t.since.get(k) ?? now]))
    const due = [...unsent].filter(([k, u]) => t.told.get(k) !== u && (u !== "Sending" || now - (since.get(k) ?? now) >= LATE_MS))
    const told = new Map([...t.told].filter(([k, u]) => unsent.get(k) === u))
    for (const [k, u] of due) told.set(k, u)
    const count = (u: Unsent): number => due.filter(([, v]) => v === u).length
    return {
      t: { since, told },
      said: due.length === 0 ? { type: "Quiet" } : { type: "Said", text: sentence(count("Refused"), count("AwaitingTrust"), count("Sending")), about: aboutOf(due) },
    }
  }

  // When a step should be taken again for an act on its way to be said on time: Infinity while none waits.
  export const next = (t: T, unsent: ReadonlyMap<string, Unsent>): number =>
    Math.min(...[...t.since].filter(([k]) => unsent.get(k) === "Sending" && t.told.get(k) !== "Sending").map(([, at]) => at + LATE_MS))
}
export default outbox
