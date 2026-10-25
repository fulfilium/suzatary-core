// Settings: what is connected, by group, and this device.
// A line offers no act of its own: it draws the open proposals about it, the Inbox's own cards, and Ask opens a conversation.
// A secret is set only by tapping its card, whose field sends it once; the box never answers with it.

import device from "../app/device"
import { askTrust, getOverview, pairDevice } from "../wire/api"
import pairing from "../app/pairing"
import { signingHere, watchReplica } from "../prodrome/sync"
import signer from "../prodrome/signer"
import type { Group, OverviewGroup, OverviewLine } from "../wire/generated/wire"
import type { Row } from "../inbox/inbox"
import app from "../app/app"
import type { Child } from "../view/dom"
import { el } from "../view/dom"
import { when } from "../view/format"
import frame from "../view/frame"
import asked from "../app/asked"
import reading from "../algebra/reading"
import { hrefOf } from "../shell/route"
import inboxPage from "./inbox"
import { proposalCard } from "../inbox/proposal"
import { sameRows } from "../inbox/inbox"
import intentsync from "../prodrome/intentsync"
import storesync from "../prodrome/storesync"
import overview from "./overview"
import kept from "../inbox/kept"
import settingsState from "./settings.state"
import time from "../app/time"

export namespace settingsPage {
  type T = settingsState.T
  type Effects = { readonly again: () => void }

  const stamp = (at: string): string => {
    const ms = time.ms(at)
    return ms === null ? at : when(ms, app.now().zone)
  }

  // Settings draws the box's own cards, whose sources name earlier cards and never a todo.
  const card = (r: Row, taken?: () => void): HTMLElement | null =>
    r.kind === "proposal" ? proposalCard(r.conversation.id, r.proposal, { titles: new Map(), ...(taken === undefined ? {} : { taken }) }) : null

  // A grant drawn inside the link's card it was asked for: allowing the origin sends the link again, once.
  const linkCards = (cards: readonly Row[]): Child[] => {
    const a = overview.allowing(cards)
    if (a === null) return cards.map(r => card(r))
    return cards.filter(r => r !== a.grant).map(r => {
      const node = card(r)
      if (r !== a.link || node === null) return node
      const grant = card(a.grant, () => kept.resend(a.link))
      if (grant !== null) node.appendChild(el("div", { class: "proposalallow" }, grant))
      return node
    })
  }

  // The box's own cards to connect a model: while one stands, no model answers.
  const signsIn = (r: Row): boolean => r.kind === "proposal" && (r.proposal.act.kind === "sign_in" || r.proposal.act.kind === "sign_in_code")

  const line = (g: Group, l: OverviewLine, cards: readonly Row[]): HTMLElement => {
    const d = overview.drawn(l, stamp)
    const failing = d.health?.type === "Asks"
    return el(
      "section",
      { class: failing ? "settingsline settingsline-failing" : "settingsline", "data-group": g, "data-line": l.key, "data-key": l.key },
      el("h3", null, d.label),
      d.detail === null ? null : el("p", { class: "settingsmeta" }, d.detail),
      d.health === null ? null : el("p", { class: failing ? "settingshealth settingshealth-failing" : "settingshealth" }, d.health.text),
      d.atTheBox === null ? null : el("details", { class: "settingsatbox" }, el("summary", null, "How to set this up at the box"), el("code", null, d.atTheBox)),
      linkCards(cards),
    )
  }

  const group = (v: T) => (g: OverviewGroup): HTMLElement => {
    const cards = (l: OverviewLine) => overview.cards(l, v.rows)
    return el(
      "section",
      { class: "settingsgroup", "data-group": g.group },
      el("h2", null, overview.title(g.group)),
      // Until a model answers, the box's own cards to connect one stand first: nothing else here can help.
      g.group === "models" && g.lines.flatMap(cards).some(signsIn)
        ? el("p", { class: "settingshealth settingshealth-failing settingsunanswered" }, "No model answers on this box yet. Connect one:")
        : null,
      g.lines.map(l => line(g.group, l, cards(l))),
    )
  }

  const ask = (v: T, words: string): Child =>
    v.me === null ? null : el("a", { class: "act act-ask", href: hrefOf({ type: "conversation", id: v.overview.ask, refs: [], focus: true }) }, words)

  const offered = (v: T): Child => {
    const gs = overview.offered(v.overview)
    return gs.length === 0 ? null : el(
      "section",
      { class: "settingsoffered" },
      el("p", { class: "settingsmeta" }, `Nothing connected for ${gs.map(g => overview.title(g).toLowerCase()).join(", ")}.`),
      ask(v, "Ask to add one"),
    )
  }

  export const view = (v: T, fx: Effects): Child => {
    // What this browser answers for itself, and nothing if it cannot.
    const here = el("div", { class: "settingshere" }, v.device === null ? null : [el("h2", null, "This device"), device.view(v.device, {
      ask: key => void askTrust(key).then(fx.again, fx.again),
      renew: () => void signer.renew().then(fx.again, fx.again),
      ...pairing.acts(pairDevice, fx.again),
    })])
    // An untrusted device can tap nothing else here, so its own step comes first.
    const first = v.device !== null && v.device.signing.type === "Keyed" && v.device.signing.standing === "untrusted"
    return [
      first ? here : null,
      el("div", { class: "settingsask" }, ask(v, "Ask to add or change anything")),
      el("div", { class: "settingsbody" }, overview.connected(v.overview).map(group(v)), offered(v)),
      first ? null : here,
    ]
  }

  // What of the Inbox a line draws: its proposals, which the tab holds, so the box answering for its folds moves
  // nothing here.
  const proposed = reading.settled(
    reading.map(inboxPage.waits, w => w.rows.filter(r => r.kind === "proposal")),
    sameRows,
  )

  // The overview and what this browser keeps, asked together, and asked again when the inbox or the conversations
  // move: a card the box files changes which line asks for what.
  const connected = reading.againOn(
    asked.question(() => Promise.all([getOverview(), device.kept().catch(() => null)])),
    storesync.watch,
  )

  // Who this device writes as, read again when what it reads moves: the replica (a key trusted or revoked arrives there
  // after the box moved, so the box moving is too early to read it), this device's key and who writes.
  const here = reading.againOn(asked.question(() => device.signing(signingHere)), heard => {
    const unhear = [watchReplica(heard), signer.watch(heard), app.hear(app.actor, heard)]
    return () => unhear.forEach(u => u())
  })

  export const page: frame.Page<T, settingsState.Event> = {
    className: "settingspage",
    title: "Settings",
    // The Inbox's own rows: a line's decision is answered here too, and read again as the Inbox's are.
    reads: () =>
      reading.map(reading.both(reading.both(connected, here), proposed), ([[[o, k], signing], rows]): T => ({
        overview: o,
        device: k === null ? null : { ...k, signing },
        rows,
        me: app.actor(app.now()),
        writes: app.writes(app.now()),
      })),
    step: settingsState.step,
    listen: (dispatch, again) => {
      const unhear = app.hear(
        a => ({ me: app.actor(a), writes: app.writes(a) }),
        w => dispatch({ type: "Writes", ...w }),
        (w, was) => w.me === was.me && w.writes === was.writes,
      )
      const unask = intentsync.watch(again)
      return () => (unhear(), unask())
    },
    view: (v, _, again) => view(v, { again }),
  }

  export const mount = (root: HTMLElement) => frame.mount(root, page)
}
export default settingsPage
