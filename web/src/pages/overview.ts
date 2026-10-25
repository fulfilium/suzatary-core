// What is connected, by group: what a line draws, which groups hold lines, and the open cards each line's references name.

import type { FailureClass, Group, LineStanding, Overview, OverviewGroup, OverviewLine } from "../wire/generated/wire"
import type { Row } from "../inbox/inbox"

export namespace overview {
  // A line's words, and whether they ask something of the owner.
  export type Health = { readonly type: "Fine"; readonly text: string } | { readonly type: "Asks"; readonly text: string }
  // A line as drawn: its words alone, every one of them from the box, and a terminal's command kept apart, shown only
  // behind "How to set this up at the box".
  export type Drawn = { readonly label: string; readonly detail: string | null; readonly health: Health | null; readonly atTheBox: string | null }

  const TITLES: Readonly<Record<Group, string>> = {
    storage: "Storage",
    mail: "Mail",
    calendar: "Calendar",
    drive: "Drive",
    models: "Models",
    notifications: "Notifications",
    coding: "Coding sessions",
    plugins: "Other connections",
    box: "This box",
  }

  export const title = (g: Group): string => TITLES[g]

  const and = (xs: readonly string[]): string =>
    xs.length < 2 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`

  /** What a failure's class asks of the owner, `""` for a class nothing but time mends; the status a service answered
   * is in the failure's own words, so a revoked link's remedy does not say it again. */
  export const cause = (c: FailureClass): string =>
    c.kind === "revoked" ? " Its link was changed or revoked, so store the new one."
    : c.kind === "needs_login" ? " Its sign-in lapsed: sign in again."
    : c.kind === "unresolved" ? " A secret it reads is stored nowhere: set it."
    : c.kind === "key_refused" ? " Its key was refused: the card below connects it again."
    : c.kind === "unusable" ? " The link stored for it is not one the box sends: set it again."
    : c.kind === "unreachable" ? " Check the link and set it again."
    : ""

  const ok = (text: string): Health => ({ type: "Fine", text })
  const asks = (text: string): Health => ({ type: "Asks", text })

  /**
   * A failing line: since when, what the failure said, and what its class asks of the owner, each once; the last try
   * is named only when it is not when the failing began.
   */
  const failing = (s: Extract<LineStanding, { kind: "failing" }>, at: (stamp: string) => string): string => {
    const since = at(s.since)
    const tried = at(s.at) === since ? "" : ` (last tried ${at(s.at)})`
    return `Failing since ${since}: ${s.words}${tried}.${cause(s.class)}`
  }

  /** A standing in a sentence, `null` for one in place with nothing to say; `at` formats an instant the box stamped. */
  export const health = (s: LineStanding, at: (stamp: string) => string): Health | null =>
    s.kind === "kept" ? null
    : s.kind === "synced" ? ok(`Synced ${at(s.at)}.`)
    : s.kind === "answered" ? ok(`Answered ${at(s.at)}.`)
    : s.kind === "unobserved" ? ok("Nothing has checked it yet.")
    : s.kind === "locked" ? asks("The secret store is locked, so whether it is stored is not known.")
    : s.kind === "store_failed" ? asks("The secret store could not answer.")
    : s.kind === "unreadable" ? asks(`${s.words.charAt(0).toUpperCase()}${s.words.slice(1)}.`)
    : s.kind === "by_hand" ? asks(`Set up at a terminal on the box: ${s.why}.`)
    : s.kind === "failing" ? asks(failing(s, at))
    : asks(`Needs ${and(s.what)}.`)

  export const drawn = (l: OverviewLine, at: (stamp: string) => string): Drawn =>
    ({ label: l.label, detail: l.detail, health: health(l.standing, at), atTheBox: l.standing.kind === "by_hand" ? l.standing.command : null })

  /** How far setup is: a step per line, done while it asks nothing of the owner. */
  export type Progress = { readonly type: "Finished" } | { readonly type: "Unfinished"; readonly done: number; readonly of: number }

  export const progress = (o: Overview): Progress => {
    const lines = o.groups.flatMap(g => g.lines)
    const done = lines.filter(l => health(l.standing, at => at)?.type !== "Asks").length
    return done === lines.length ? { type: "Finished" } : { type: "Unfinished", done, of: lines.length }
  }

  /** The first screen's line while setup is unfinished; `null` once it is. */
  export const unfinished = (p: Progress): string | null =>
    p.type === "Finished" ? null : `Finish setting up · ${p.done} of ${p.of} done`

  /** The groups something is connected to, in order; the rest are only offered. */
  export const connected = (o: Overview): readonly OverviewGroup[] => o.groups.filter(g => g.lines.length > 0)
  export const offered = (o: Overview): readonly Group[] => o.groups.filter(g => g.lines.length === 0).map(g => g.group)

  /** A row's reference, as the box names a card. */
  export const reference = (r: Row): string | null => (r.kind === "proposal" ? `proposal:${r.conversation.id}/${r.proposal.index}` : null)

  /** A link's card and the grant its origin needs, when a line holds both open: the grant is drawn inside the link's card. */
  export type Allowing = { readonly link: Row; readonly grant: Row }

  const open = (r: Row, kind: string): boolean =>
    r.kind === "proposal" && r.proposal.act.kind === kind && (r.proposal.state === "standing" || r.proposal.state === "failed")

  export const allowing = (rows: readonly Row[]): Allowing | null => {
    const link = rows.find(r => open(r, "secret"))
    const grant = rows.find(r => open(r, "grant"))
    return link === undefined || grant === undefined ? null : { link, grant }
  }

  /** The rows a line's references name, in its order; the box placed each card on one line. */
  export const cards = (l: OverviewLine, rows: readonly Row[]): readonly Row[] =>
    l.cards.flatMap(c => rows.filter(r => reference(r) === c))
}
export default overview
