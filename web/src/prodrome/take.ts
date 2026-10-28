// What a store's row takes in, a pull or the device's row, in order: the blocks its owed keys' words fed forgotten,
// then the row joined, then the forgotten keys dropped, in memory and then on the device, so nothing draws a row whose
// keys are owed a forget that already happened. A forget that fails keeps the keys and is said, to be asked again.

import lo from "../algebra/lo"
import locked from "./locked"
import purge from "./purge"
import type { Obj } from "./replica"

export namespace take {
  export type Effects<O extends Obj> = {
    readonly held: () => locked.Row<O>
    readonly dispatch: (e: locked.Event<O>) => void
    readonly purge: purge.Effects<O>
    readonly dead: locked.Dead<O>
    readonly alter: (gone: readonly string[]) => Promise<lo.Settled<null>>
    readonly owing: (failed: string | null) => void
  }

  export const into = async <O extends Obj>(fx: Effects<O>, incoming: locked.Row<O>): Promise<void> => {
    const gone = await purge.forgotten(fx.purge, fx.dead, locked.join(fx.held(), incoming))
    fx.dispatch({ type: "Held", row: incoming })
    if (gone.type === "Error") return fx.owing(gone.e)
    if (gone.v.length === 0) return fx.owing(null)
    fx.dispatch({ type: "Forgot", keys: gone.v })
    const kept = await fx.alter(gone.v)
    fx.owing(kept.type === "Error" ? `the device's copy was not written (${kept.e})` : null)
  }
}
export default take
