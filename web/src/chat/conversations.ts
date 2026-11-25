// The conversations as this tab's replica reads them, read again when it moves: the Secretary page's list and the
// palette's group are one reading, so a conversation the replica takes in while either is open is drawn there.

import type { ConversationRow } from "../wire/generated/wire"
import storesync from "../prodrome/storesync"
import lo from "../algebra/lo"
import reading from "../algebra/reading"
import asked from "../app/asked"

export namespace conversations {
  export const rows: reading.Reading<lo.T<readonly ConversationRow[]>> = reading.againOn(
    asked.question(() => lo.of(storesync.list()).then(l => lo.map(l, p => p.conversations))),
    storesync.watchList,
  )
}
export default conversations
