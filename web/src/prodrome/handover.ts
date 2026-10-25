// What a replica holds that the box lacks, handed over in one post, and how the box answered, each a value: what it
// took, with the replica held then settled by the receipt; that it moved past this build's contract; that it decided no
// to the whole post, with every object posted held as refused; or no decision at all, which leaves them unsent.

import type { ReceiptPayload, RefusedObject } from "../wire/generated/wire"
import type { Replica } from "./replica"
import { refusing, settled, sinceOf, toPush } from "./replica"
import http from "../wire/http"
import lo from "../algebra/lo"

export namespace handover {
  export type Ended =
    | { readonly type: "Nothing" }
    | { readonly type: "Taken"; readonly replica: Replica; readonly refused: readonly RefusedObject[] }
    | { readonly type: "Outdated" }
    | { readonly type: "Refused"; readonly reason: string; readonly replica: Replica }
    | { readonly type: "Unanswered"; readonly e: unknown }

  // `held` is read again once the box answers, so what landed meanwhile is settled with the rest; the post names the
  // frontier the replica held when it was sent, and the receipt's moves only that generation's.
  export const of = async (held: () => Replica, trusted: boolean, post: (literals: readonly string[], since: string) => Promise<ReceiptPayload>): Promise<Ended> => {
    const posting = held()
    const pushing = toPush(posting, trusted)
    if (pushing.length === 0) return { type: "Nothing" }
    try {
      const receipt = await post(pushing.map(o => o.literal), sinceOf(posting))
      return { type: "Taken", replica: settled(held(), receipt, posting.generation), refused: receipt.refused }
    } catch (e: unknown) {
      if (e instanceof http.ApiError && e.status === http.OUTDATED) return { type: "Outdated" }
      if (http.unanswered(e)) return { type: "Unanswered", e }
      const reason = lo.reason(e)
      return { type: "Refused", reason, replica: refusing(held(), pushing.map((o): RefusedObject => ({ hash: o.hash, reason, kind: "final" }))) }
    }
  }

  // A refusal said to the owner in the box's words; imported lazily so the shells stay testable without a DOM.
  export const told = (said: string): void => {
    void import("../view/toast").then(module => module.toast(said))
  }
}
export default handover
