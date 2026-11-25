// Every store the tab holds, in the core's worker: their pulls, the hand-over of what this tab wrote, its refusals,
// and what moved. The page holds none of it.

import type { Core, Lines, ObjectStore, Proposals, TopicReading, TopicStore } from "./core"
import type excluded from "./excluded"
import type heads from "./heads"
import ownlines from "./ownlines"
import linesync from "./linesync"
import type sealing from "./lines"
import type progress from "./progress"
import replicalog from "./replicalog"
import replicasync from "./replicasync"
import stores from "./stores"
import topics from "./topics"
import written from "./written"
import last from "../algebra/last"
import lo from "../algebra/lo"
import once from "../algebra/once"
import type rendered from "../app/rendered"
import storereads from "../wire/storereads"
import type { StorePayload } from "../wire/generated/wire"
import { measure } from "../app/perf"
import d from "../wire/decoder"

export namespace storeset {
  export type Said =
    | { readonly type: "Told"; readonly reader: string; readonly left: excluded.Left }
    | { readonly type: "Bars"; readonly e: progress.Event }
    | { readonly type: "Moved"; readonly conversations: linesync.Moved }
    // Words of these conversations came to the core: no line moved, so only a reading that answers before its words are
    // held reads again.
    | { readonly type: "Came"; readonly conversations: readonly string[]; readonly whole: readonly string[] }
    | { readonly type: "Refused"; readonly why: string }

  export type Scope = "Inbox" | "All"
  const SCOPES: { readonly [S in Scope]: readonly stores.Name[] } = { Inbox: ["inbox"], All: stores.NAMES }
  const SCOPE_NAMES: readonly Scope[] = ["Inbox", "All"]

  // The core's two topic stores and their one reading, made once per move of either.
  export type Topics = { readonly replicas: { readonly [N in topics.Store]: TopicStore }; readonly reading: TopicReading }

  export type T = {
    readonly lines: linesync.T
    readonly proposals: () => Proposals
    readonly topics: () => Topics
    // Local-first: a store this device has landed answers at once; one it never landed is waited for until whole.
    readonly ready: (scope: Scope) => Promise<void>
    // Every store pulled, what this tab wrote handed over first; whether every pull ended whole.
    readonly pull: () => Promise<boolean>
    readonly heard: (store: stores.Name, box: heads.Heads) => void
    readonly opened: () => void
    // Whether a page is shown, as the page says: a hidden page may be closed, so every store's log is compacted.
    readonly shown: (on: boolean) => Promise<void>
    // A store dropped and taken again; what the reset could not do.
    readonly reset: (store: stores.Name) => Promise<readonly string[]>
    // `w` written into its store as `actor` and signed, settled before the box hears of it; a refusal arrives as a move.
    readonly write: (w: topics.Write, at: string, actor: string) => Promise<void>
    // `line` sealed as `actor`'s and signed, kept and read at once, handed over behind the answer.
    readonly seal: (line: sealing.Sealing, at: string, actor: string) => Promise<sealing.Sealed>
    // The person acted on the refusals of the own lines named `lines`: the device lets their words go.
    readonly acted: (lines: readonly string[]) => Promise<void>
    // Each own line refused for good, with words, of a topic the box refused (`[Waiting]`).
    readonly refused: () => string
    // The stores this device writes of which it keeps an earlier build's copy aside.
    readonly earlier: () => Promise<readonly stores.Written[]>
  }

  const LINE = "line"
  // What the owner reads where a line sealed under older rules is refused, or one the box did not answer: the words stay
  // where they were typed.
  export const OUTDATED = "This page is older than the box: reload to send it. Your words stay here."
  const UNANSWERED = "The box did not answer: your words stay here, and the line goes with the next sync."
  type Kind = "Trust" | "Later" | "Final" | "Outdated"
  type Refused = { Refused: { kind: Kind; reason: string } }
  const Answered = d.json(
    d.alt(
      d.alt(d.lit("Unsent"), d.lit("Taken")),
      d.obj<Refused>({ Refused: d.obj<Refused["Refused"]>({ kind: d.oneOf<Kind>({ Trust: null, Later: null, Final: null, Outdated: null }), reason: d.str }) }),
    ),
  )
  // The bytes a seal draws its keys from: one key of 32 for each slot. A line this tab writes holds one, and a seal
  // makes at most two lines (a begun topic's opening and question, an edit's `Again` and question).
  const ENTROPY = 64
  // The topic stores this tab writes, each with the noun a refusal names.
  const NOUNS: { readonly [N in topics.Store]: string } = { topics: "topic", "topic-aliases": "topic's new name" }
  const listed = <const L extends readonly topics.Store[]>(names: L): L => names
  const WRITTEN = listed(["topics", "topic-aliases"])
  // Every topic store is in the list: a name left out fails to compile here.
  const everyWritten: [Exclude<topics.Store, (typeof WRITTEN)[number]>] extends [never] ? true : never = true
  void everyWritten

  export const of = (core: Core, held: Lines, say: (s: Said) => void, blocks: rendered.Device): T => {
    const told = (reader: string, left: excluded.Left): void => say({ type: "Told", reader, left })
    const bars = (e: progress.Event): void => say({ type: "Bars", e })
    // A store a later epoch started over no longer holds what a reading was ready over, so every reading of it waits
    // again, as for a store never landed, until a pull of it ends whole.
    const replaced = (name: stores.Name) => (): void => {
      for (const s of SCOPE_NAMES) if (SCOPES[s].includes(name)) readies[s].forget()
    }
    const objectStore = <S extends ObjectStore>(name: Exclude<stores.Name, "lines">, store: S, after: () => void = () => {}): { readonly store: S; readonly sync: replicasync.T } => ({
      store,
      sync: replicasync.of<StorePayload>(
        {
          name,
          held: store,
          ask: since => storereads.objects(name, since),
          piece: (a, at) => store.piece(JSON.stringify(a), at),
          landed: () => Promise.resolve(),
          form: bytes => replicalog.formOf(core.objects_form(bytes)),
          // A store this tab writes may hold a write not yet sent; the box alone writes the rest.
          holds: WRITTEN.some(w => w === name) ? "OwnWrites" : "BoxAlone",
        },
        { told, bars: e => bars({ ...e, name }), moved: said => (said === "[]" ? undefined : (say({ type: "Moved", conversations: "All" }), after())), replaced: replaced(name) },
      ),
    })

    const lines = linesync.of(
      held,
      {
        told,
        bars,
        moved: conversations => say({ type: "Moved", conversations }),
        came: (conversations, whole) => say({ type: "Came", conversations, whole }),
        replaced: replaced("lines"),
      },
      blocks,
      bytes => replicalog.formOf(core.lines_form(bytes)),
    )
    const inbox = objectStore("inbox", new core.Proposals())
    const topicStores: { readonly [N in topics.Store]: { readonly store: TopicStore; readonly sync: replicasync.T } } = {
      topics: objectStore("topics", new core.Memberships()),
      "topic-aliases": objectStore("topic-aliases", new core.TopicAliases()),
    }
    // Read again whenever the asks move: each store this tab writes holds what the asks record as refused.
    const refusedIn = async (asks: ObjectStore): Promise<void> => {
      for (const name of WRITTEN) await writes.settled(name, () => topicStores[name].store.refused_in(asks))
      await writes.settled("lines", () => held.refused_in(asks))
    }
    const asks: { readonly store: ObjectStore; readonly sync: replicasync.T } = objectStore("asks", new core.Asks(), () => void refusedIn(asks.store))
    const engines: { readonly [N in stores.Name]: replicasync.Store } = { lines: lines.engine, inbox: inbox.sync, topics: topicStores.topics.sync, "topic-aliases": topicStores["topic-aliases"].sync, asks: asks.sync }

    const reading = last.of<string, TopicReading>(
      (a, b) => a === b,
      r => r.free(),
    )
    const topicsNow = (): Topics => {
      const replicas = { topics: topicStores.topics.store, "topic-aliases": topicStores["topic-aliases"].store }
      return { replicas, reading: reading(`${replicas.topics.version()}/${replicas["topic-aliases"].version()}`, () => new core.TopicReading(replicas.topics, replicas["topic-aliases"])) }
    }

    const post = (name: stores.Written): written.Entry["post"] => (literals, since, keys) => storereads.post(name, literals, since, keys)
    const entry = (name: topics.Store): written.Entry => ({
      noun: NOUNS[name],
      store: topicStores[name].store,
      exclusive: topicStores[name].sync.exclusive,
      stepped: step => topicStores[name].sync.stepped(step),
      post: post(name),
      signs: () => "Signs",
    })
    const entries: { readonly [N in stores.Written]: written.Entry } = {
      topics: entry("topics"),
      "topic-aliases": entry("topic-aliases"),
      // The lines take their turns in their own engine and under the device's lock (`ownlines`).
      lines: { noun: LINE, store: held, exclusive: f => f(), stepped: lines.ownStepped, post: post("lines"), signs: () => "Signs", told: { seen: ownlines.seen, told: ownlines.told } },
    }
    const writes = written.of<stores.Written>(core, name => entries[name], why => say({ type: "Refused", why }))
    // A hand-over that failed is said, and the next sync tries it again.
    const failed = (name: stores.Written) => (e: unknown): void =>
      say({ type: "Refused", why: `This tab's ${name === "lines" ? LINE : NOUNS[name]} writes were not handed over (${lo.reason(e)}); the next sync tries again.` })
    // A line another tab of the device kept is taken and handed over here too: the box answers `already` to the second.
    // Only when the row brought something new: a wake over a line this tab already holds, refused as another tab saw it
    // refused, hands nothing over again.
    ownlines.heard(() => void lines.ownHeard().then(fresh => (fresh ? writes.handOver("lines") : undefined)).catch(failed("lines")))
    // Every other hand-over of the lines reads the device's row first, so a line another tab kept and whose wake this tab
    // missed goes with it, not at the next load.
    const handOverLines = (): Promise<void> => lines.ownHeard().then(() => writes.handOver("lines"))

    // Which stores a reading has asked for so far: a reconnect pulls those alone.
    let wanted: "None" | Scope = "None"

    // The inbox beside the lines, so a reading waits on neither behind the other, and again once the lines are pulled: a
    // pull begun after theirs holds every proposal they name, as the box files a record before its line. The topic
    // stores beside them after their hand-over, and the asks after those posts.
    const all = (): Promise<boolean> => {
      wanted = "All"
      const topicPulls = Promise.all(
        WRITTEN.map(name =>
          topicStores[name].sync.sync.restored
            .get()
            .then(() => writes.handOver(name))
            .catch(failed(name))
            .then(() => topicStores[name].sync.want()),
        ),
      )
      const asked = topicPulls.then(ts => asks.sync.want().then(a => ts.every(t => t) && a))
      // The row was read as the lines were restored, so the hand-over takes what it holds. A refusal the load found that
      // the person was not told of yet is said once the lines are pulled, their words with it.
      const linesPulled = lines.engine.sync.restored
        .get()
        .then(() => writes.handOver("lines"))
        .catch(failed("lines"))
        .then(() => lines.engine.want())
        .then(pulled => {
          void writes.said("lines").catch(failed("lines"))
          return pulled
        })
      const inboxBeside = inbox.sync.want()
      return Promise.all([linesPulled, inboxBeside]).then(([l]) => inbox.sync.want().then(i => asked.then(t => l && i && t)))
    }
    // The inbox alone; a move pulls the rest too once a reading asked for all.
    const inboxOnly = (): Promise<boolean> => {
      if (wanted === "None") wanted = "Inbox"
      return inbox.sync.want()
    }

    // A store is ready once it is restored, and, where it never landed, once a pull of it ended whole.
    const readyOf = (names: readonly stores.Name[], pulling: () => Promise<boolean>): once.T<void> =>
      once.of(() =>
        Promise.all(
          names.map(name => {
            const sync = engines[name].sync
            return sync.restored.get().then(() => (sync.landed() ? [] : [name]))
          }),
        ).then(unlanded => {
          void pulling()
          return Promise.all(unlanded.flat().map(name => engines[name].sync.whole())).then(() => undefined)
        }),
      )
    const readies: { readonly [S in Scope]: once.T<void> } = { Inbox: readyOf(SCOPES.Inbox, inboxOnly), All: readyOf(SCOPES.All, all) }

    // Each event planned for `w`, sealed, signed and kept in one move; handed over behind the answer, so no round trip is
    // waited on.
    const write = async (w: topics.Write, at: string, actor: string): Promise<void> => {
      const name = topics.store(w)
      const measured = async <A>(step: string, f: () => A | Promise<A>): Promise<A> => {
        const start = performance.now()
        const a = await f()
        measure(`write:${step}`, start, { store: name })
        return a
      }
      await measured("ready", () => readies.All.get())
      const { store, sync } = topicStores[name]
      // Planned, sealed, signed and kept in the store's turn, so the reading planned from is the one written over.
      const signed = await sync.exclusive(async () => {
        const events = await measured("plan", () => topics.plan(topicsNow().reading, w, at, actor))
        return measured("keep", () => writes.own(name, store, events, frame => sync.kept(frame)))
      })
      if (signed.type === "Partly") say({ type: "Refused", why: `This tab's ${NOUNS[name]} was kept unsigned (${signed.why}); the next sync signs it.` })
      void writes.handOver(name).catch(failed(name))
    }

    // Sealed in the worker: the first seal compiles the scrub's patterns, paid off the page's thread. A rename, a question,
    // a stop, an `Again` and a topic begun with its question are answered at once and handed over behind the answer, all
    // a seal made in one post. A setup is answered once the box holds it, since what a page does next (the setup's note)
    // is the box's.
    const seal = async (line: sealing.Sealing, at: string, actor: string): Promise<sealing.Sealed> => {
      const start = performance.now()
      // Each part of a seal timed on its own (`seal:*`), so a slow Send says which part it waited on.
      const part = async <A>(name: string, f: () => Promise<A>): Promise<A> => {
        const begun = performance.now()
        const a = await f()
        measure(`seal:${name}`, begun, { store: "lines" })
        return a
      }
      await part("ready", () => readies.All.get())
      // A SEAL, one step under the device's lock: the device's row read into the core (its records, then the acts), the
      // core's seal, which asks whether the conversation is held with no wait before it, so a reset that lands meanwhile
      // is seen, then the new lines signed, and the lines and their signatures kept in one write of the row, which
      // publishes their move. The seal answers once that write is done: no draw sees a line kept and not signed, and the
      // answer leaves before any page has read the move. A line rests on what every other tab of the device wrote: the
      // device's lines are one chain, which no later line of the box reorders. Where the lock is not had (no Web Locks,
      // or a holder frozen past `ownlines.LOCK_MS`) this is per tab: two tabs sealing at once write two lines that rest
      // on nothing of each other, read by their clocks as two devices' are.
      //
      // A seal answers which: sealed, or not sealed for a cause a pull may cure (a conversation the tab does not hold:
      // opened a moment ago, by another device, or a reset that landed meanwhile; one resting on a line the tab does not
      // hold; or no pull landed since a reset, which a topic begun or a setup needs). That one is pulled and the seal tried
      // again, once, outside the lock. Anything else is the seal's own failure, never tried again: after the core sealed,
      // a second try would seal a second line. A signature that fails is no failure of the seal: the line is kept
      // unsigned, and the hand-over signs it.
      type Attempt = { readonly type: "Sealed"; readonly sealed: sealing.Sealed } | { readonly type: "Unheld"; readonly why: string }
      const sealOnce = (): Promise<Attempt> => {
        const asked = performance.now()
        return ownlines.device(ownlines.LOCK, async (): Promise<Attempt> => {
          measure("seal:lock", asked, { store: "lines" })
          await part("row", () => lines.ownNow())
          // Its keys drawn here; kept on the device and read at once, a begun topic's two lines and an edit's in one move.
          const sealing = performance.now()
          const sealed = held.seal(JSON.stringify(line), at, actor, crypto.getRandomValues(new Uint8Array(ENTROPY)))
          measure("seal:core", sealing, { store: "lines" })
          switch (sealed.type) {
            case "Unheld": {
              const why = sealed.why
              sealed.free()
              return { type: "Unheld", why }
            }
            case "Sealed": {
              const kept = { name: sealed.name, conversation: sealed.conversation }
              const signed = await part("sign", () => writes.signatures("lines"))
              if (signed.type === "Partly") say({ type: "Refused", why: `This tab's ${LINE} was kept unsigned (${signed.why}); the next sync signs it.` })
              await part("keep", () => lines.ownKeep([sealed, ...signed.made]))
              return { type: "Sealed", sealed: kept }
            }
          }
        })
      }
      const first = await sealOnce()
      // A pull that fails (offline) is no answer of its own: the second try says what the seal lacks.
      const attempt = first.type === "Sealed" ? first : await part("pull", () => lines.engine.want().catch(() => false)).then(sealOnce)
      if (attempt.type === "Unheld") throw new Error(attempt.why)
      const answer = attempt.sealed
      measure("write:seal", start, { store: "lines" })
      if (line.type !== "SetUp") {
        void handOverLines().catch(failed("lines"))
        return answer
      }
      await handOverLines()
      const said = Answered(held.answer(answer.name))
      if (said.type === "Error") throw new Error(`the box's answer to a line: ${said.e}`)
      const v = said.v
      if (v === "Taken") return answer
      throw new Error(v === "Unsent" ? UNANSWERED : v.Refused.kind === "Outdated" ? OUTDATED : v.Refused.reason)
    }

    // The acts kept on the device, a write it could not keep said as a keep's is, and heard at once in this tab.
    const acted = async (names: readonly string[]): Promise<void> => {
      const kept = await ownlines.acted(names)
      if (kept.type === "Error") say({ type: "Refused", why: `What you did with a refused ${LINE} was not kept on this device (${kept.e}); a reload shows it again.` })
      await lines.ownStepped({ delta: new TextEncoder().encode("[]"), moved: held.acted(JSON.stringify(names)), free: () => {} })
    }

    return {
      lines,
      proposals: () => inbox.store,
      topics: topicsNow,
      ready: scope => readies[scope].get(),
      pull: all,
      heard: (store, box) => engines[store].heard(box),
      opened: () => void (wanted === "All" ? all() : wanted === "Inbox" ? inboxOnly() : undefined),
      shown: on => Promise.all([lines.shown(on), inbox.sync.shown(on), asks.sync.shown(on), ...WRITTEN.map(name => topicStores[name].sync.shown(on))]).then(() => undefined),
      reset: store => engines[store].reset(),
      write,
      seal,
      acted,
      refused: () => held.refused_elsewhere(),
      earlier: async () => {
        const kept = await Promise.all([
          lines.earlier().then((e): stores.Written[] => (e ? ["lines"] : [])),
          ...WRITTEN.map(name => topicStores[name].sync.earlier().then(e => (e ? [name] : []))),
        ])
        return kept.flat()
      },
    }
  }
}
export default storeset
