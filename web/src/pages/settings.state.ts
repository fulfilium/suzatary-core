// The Settings page's value: the box's overview, the Inbox's own open rows (on their way while the conversations
// sync, so the page never waits on them), who writes, whether this device writes as them, and this device.
// Only a new reading moves `overview`; a secret is only ever set by tapping its card, drawn on the line it is about.

import type { Overview } from "../wire/generated/wire"
import type { Row } from "../inbox/inbox"
import type device from "../app/device"
import type lo from "../algebra/lo"

export namespace settingsState {
  export type T = {
    readonly overview: Overview
    readonly rows: lo.T<readonly Row[]>
    readonly me: string | null
    readonly writes: boolean
    readonly device: device.Report | null
  }
  // Who writes, or whether this device may: a key the chain trusts once its replica says so draws the cards' acts.
  export type Event = { readonly type: "Writes"; readonly me: string | null; readonly writes: boolean }

  export const step = (v: T, e: Event): T => (v.me === e.me && v.writes === e.writes ? v : { ...v, me: e.me, writes: e.writes })
}
export default settingsState
