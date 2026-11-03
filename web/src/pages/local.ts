// What the tab keeps of its own that a page watches (`Local` in `app/pages/src/runtime.rs`): this device, and the
// Inbox's open cards. Each is a reading, handed to a page by the host as it hands the box's answers; no GET answers it.

import type { Card, CardAct, Device, Local } from "../wire/generated/pages"
import type { Row } from "../inbox/inbox"
import { sameRows } from "../inbox/inbox"
import device from "../app/device"
import asked from "../app/asked"
import app from "../app/app"
import { signingHere, watchReplica } from "../prodrome/sync"
import signer from "../prodrome/signer"
import storesync from "../prodrome/storesync"
import reading from "../algebra/reading"
import lo from "../algebra/lo"
import inboxPage from "./inbox"
import overview from "./overview"

export namespace local {
  // What this browser keeps, asked again when the stores move.
  const kept = reading.againOn(
    asked.question(() => device.kept().catch(() => null)),
    storesync.watch,
  )

  // Who this device writes as, read again when what it reads moves: the replica (a key trusted or revoked arrives there
  // after the box moved, so the box moving is too early to read it), this device's key and who writes.
  const signing = reading.againOn(asked.question(() => device.signing(() => signingHere("Page"))), heard => {
    const unhear = [watchReplica(heard), signer.watch(heard), app.hear(app.actor, heard)]
    return () => unhear.forEach(u => u())
  })

  const here: reading.Reading<Device | null> = reading.map(reading.both(kept, signing), ([k, s]) => (k === null ? null : { ...k, signing: s }))

  // The Inbox's proposals, which the tab holds: on their way while the conversations sync.
  const proposals = reading.settled(
    reading.map(inboxPage.waits, w => w.rows.filter(r => r.kind === "proposal")),
    sameRows,
  )

  const actOf = (kind: string): CardAct =>
    kind === "sign_in" || kind === "sign_in_code" ? "sign_in" : kind === "secret" ? "secret" : kind === "grant" ? "grant" : "other"

  const cardOf = (r: Row): readonly Card[] => {
    const reference = overview.reference(r)
    return r.kind === "proposal" && reference !== null
      ? [{ reference, act: actOf(r.proposal.act.kind), stands: r.proposal.state === "standing" || r.proposal.state === "failed" ? "open" : "settled", version: JSON.stringify(r.proposal) }]
      : []
  }

  const cards: reading.Reading<readonly Card[]> = reading.map(proposals, rs => rs.flatMap(cardOf))

  export const of = (l: Local): reading.Reading<unknown> => {
    switch (l) {
      case "device":
        return here
      case "cards":
        return cards
    }
  }

  // The proposal a page names by reference, from what the tab holds now: the cards a page places as parts.
  export const row = (ref: string): Row => {
    const r = lo.withDefault(proposals.now(), []).find(x => overview.reference(x) === ref)
    if (r === undefined) throw new Error(`no card ${ref} in what the tab holds`)
    return r
  }
}
export default local
