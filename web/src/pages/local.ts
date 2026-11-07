// What the tab keeps of its own that a page watches (`Local` in `app/pages/src/runtime.rs`), each a reading read again
// whenever what it reads moves. Nothing here knows a page.

import type { Local, LocalAnswers, Register } from "../wire/generated/pages"
import device from "../app/device"
import asked from "../app/asked"
import app from "../app/app"
import ui from "../app/ui"
import registers from "../app/registers"
import time from "../app/time"
import { boxFolds, ensureFold, held as replicaHeld, signingHere, watchOthers, watchReplica } from "../prodrome/sync"
import { awaitingTrust, losses, stranded, stuckBehind, unsentTodos } from "../prodrome/replica"
import onscreen, { store } from "../app/store"
import signer from "../prodrome/signer"
import storesync from "../prodrome/storesync"
import intentsync from "../prodrome/intentsync"
import intents from "../prodrome/intents"
import readings from "../prodrome/readings"
import excluded from "../prodrome/excluded"
import reading from "../algebra/reading"
import dressed from "../chat/dressed"
import drafts from "../chat/drafts"
import draftbox from "../chat/draftbox"
import onScreen from "../inbox/onscreen"
import scroll from "../painter/scroll"
import d from "../wire/decoder"
import lanes from "../prodrome/lanes"

export namespace local {
  type Kind = Local["type"]
  // The generated answer, read-only all the way down as the tab's own values are.
  type Frozen<T> = T extends readonly (infer U)[] ? readonly Frozen<U>[] : T extends object ? { readonly [P in keyof T]: Frozen<T[P]> } : T
  type Answer<K extends Kind> = Frozen<LocalAnswers[K]>

  // Keyed by the `Local` as decoded.
  const watchers = lanes.watchers()
  const laneOf = (l: Local): lanes.Lane => watchers.laneOf(JSON.stringify(l))
  export const watching = (l: Local, lane: lanes.Lane): (() => void) => watchers.watch(JSON.stringify(l), lane)

  const kept = reading.againOn(
    asked.question(() => device.kept().catch(() => null)),
    storesync.watch,
  )

  // Reads the replica: a key trusted or revoked arrives there after the box moved.
  const signing = reading.againOn(asked.question(() => device.signing(() => signingHere("Page"))), heard => {
    const unhear = [watchReplica(heard), signer.watch(heard), app.hear(app.actor, heard)]
    return () => unhear.forEach(u => u())
  })

  const here = reading.map(reading.both(kept, signing), ([k, s]) => (k === null ? null : { ...k, signing: s }))

  // What it was priced without goes to the shell's line of what the device left out.
  const proposals = reading.againOn(
    asked.question(async () => {
      const read = await dressed.inbox(time.moment(), await dressed.asked(), laneOf({ type: "Proposals" }))
      readings.told("inbox", excluded.left(excluded.NONE, read.payload.failed))
      return read
    }),
    storesync.watch,
  )

  // Decision entries alone, kept while the chain's heads stand, so a move decodes no other todo.
  const decisions = asked.family(
    (chain: string) => chain,
    (chain: string) => boxFolds.of(chain).then(f => ({ ...f, entries: f.entries.filter(e => e.authored?.block.kind === "decision") })),
  )

  const listed = reading.againOn(asked.question(() => storesync.list(laneOf({ type: "Conversations" }))), storesync.watch)

  type Search = Extract<Local, { type: "Search" }>
  const searches = asked.family((s: Search) => s.q, (s: Search) => storesync.search(s.q, laneOf(s)))

  type Stream = Extract<Local, { type: "Stream" }>
  const streams = asked.family(
    (s: Stream) => JSON.stringify([s.chosen, s.pages]),
    (s: Stream) => dressed.stream(s.chosen, s.pages, laneOf(s)).then(r => ({ stream: r.stream, bar: r.bar })),
  )

  type Window = Extract<Local, { type: "Window" }>
  const windows = asked.family(
    (w: Window) => JSON.stringify([w.id, w.window]),
    (w: Window) => dressed.window(w.id, w.window, app.now().zone, laneOf(w)),
  )

  type Cards = Extract<Local, { type: "Cards" }>
  const cards = asked.family((c: Cards) => c.conversation, (c: Cards) => dressed.asked().then(m => dressed.cards(c.conversation, m, laneOf(c))))

  // A seal and a summary are no page's; a topic is named without its instant.
  const held = (): Answer<"Intents"> => {
    const t = intentsync.held()
    return {
      asked: [...t.asked].flatMap(([key, v]) =>
        v.act.type === "Seal" || v.act.type === "Retract"
          ? []
          : [{ act: v.act.type === "Topic" ? { type: "Topic", write: v.act.write } : v.act, answer: v.answer, retracted: !intents.isIn(t, key) }],
      ),
    }
  }
  const asking = reading.held(held, intentsync.watch)

  const titles = reading.map(onScreen.titles, t => Object.fromEntries(t))

  // One per conversation for the tab's life: what the box holds, and the other words a conflict sets beside it.
  const boxes = new Map<string, { readonly draft: drafts.Draft; box: draftbox.T }>()
  const boxOf = (id: string): { readonly draft: drafts.Draft; box: draftbox.T } => {
    const known = boxes.get(id)
    if (known !== undefined) return known
    const draft = drafts.of(ui.tab, id)
    const made = { draft, box: draftbox.open(draft) }
    boxes.set(id, made)
    return made
  }
  const draftOf = (id: string): reading.Reading<Answer<"Draft">> => {
    const b = boxOf(id)
    const now = (): Answer<"Draft"> => {
      const g = b.draft.read()
      return { text: b.box.text, others: g.type === "Conflict" ? g.vs.filter(v => v !== b.box.text) : [] }
    }
    return reading.held(now, heard => b.draft.hear(() => ((b.box = draftbox.heard(b.draft, b.box)), heard())))
  }

  const route = reading.held(
    () => {
      const a = app.now()
      const r = app.route(a)
      const s = app.screen(a)
      return {
        page: r.type,
        conversation: r.type === "conversation" ? r.id : "",
        selected: s.type === "todos" ? s.selected : s.type === "typst" ? s.open : "",
        shared: s.type === "todos" ? s.prefill : "",
      }
    },
    heard => app.hear(a => JSON.stringify([app.route(a), app.screen(a)]), heard),
  )

  // The open chain's rows as the tab's fold shows them; a kept copy asks the box for a fold about now.
  const folded = reading.held(
    (): Answer<"Fold"> => {
      const f = store.state()
      return onscreen.loaded(f) ? { entries: f.entries, at: f.at } : null
    },
    heard => {
      void ensureFold().catch(() => {})
      return store.subscribe(() => heard())
    },
  )

  // What this tab wrote that the box has not taken, read again on every commit to the replica.
  const unsent = reading.held((): Answer<"Unsent"> => {
    const r = replicaHeld()
    const { held: blocked, reasons } = stuckBehind(r)
    return {
      todos: Object.fromEntries(unsentTodos(r)),
      refused: reasons.map(one => one.reason),
      blocked,
      stranded: stranded(r).length,
      losses: losses(r).map(object => object.hash),
      waiting: awaitingTrust(r),
    }
  }, watchReplica)

  const waiting = reading.held((): number => app.now().waiting, heard => app.hear(a => a.waiting, heard))

  const onDevice = (key: string): reading.Reading<unknown> => {
    const c = ui.cell(registers.kept, key)
    return reading.held(c.get, c.hear)
  }

  const page = (key: string): reading.Reading<unknown> => {
    const c = ui.cell(registers.page, key)
    return reading.held(c.get, c.hear)
  }

  type Of = { readonly [K in Kind]: Extract<Local, { readonly type: K }> }
  type Readers = { readonly [K in Kind]: (l: Of[K], box: Element) => reading.Reading<Answer<K>> }

  const readers: Readers = {
    Device: () => here,
    Proposals: () => proposals,
    Decisions: l => reading.againOn(decisions(l.chain), watchOthers),
    Conversations: () => listed,
    Search: l => searches(l),
    Stream: l => reading.againOn(streams(l), storesync.watch),
    Window: l => reading.againOn(windows(l), heard => storesync.watchConversation(l.id, heard)),
    Cards: l => reading.againOn(reading.againOn(cards(l), storesync.watch), watchOthers),
    Intents: () => asking,
    Titles: () => titles,
    Draft: l => draftOf(l.id),
    Ui: l => page(l.key),
    Scroll: (_, box) => scroll.of(box),
    Route: () => route,
    Fold: () => folded,
    Unsent: () => unsent,
    Waiting: () => waiting,
    Kept: l => onDevice(l.key),
  }

  export const of = <K extends Kind>(kind: K, l: Of[K], box: Element): reading.Reading<Answer<K>> => readers[kind](l, box)

  const drafted = d.obj({ type: d.oneOf({ Typed: null, Settled: null }), text: d.str })

  export const keep = (r: Register, value: unknown): void => {
    switch (r.type) {
      case "Draft": {
        const b = boxOf(r.id)
        const v = drafted(value)
        if (v.type === "Error") throw new Error(`a draft a page kept does not read: ${v.e}`)
        b.box = v.v.type === "Typed" ? draftbox.typed(b.draft, b.box, v.v.text) : draftbox.settled(b.draft, v.v.text)
        return
      }
      case "Ui":
        return ui.cell(registers.page, r.key).set(value)
      case "Kept":
        return ui.cell(registers.kept, r.key).set(value)
    }
  }
}
export default local
