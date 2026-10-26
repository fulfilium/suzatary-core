// The box's sweep in a tab: the keys a row owes are found, the blocks their words were rendered to forgotten while the
// keys still open them, and only then may the keys go. A forget that fails keeps them, to be asked again.

import lo from "../algebra/lo"
import type { Erased } from "./core"
import locked from "./locked"
import type { Obj } from "./replica"

export namespace purge {
  export type Effects<O extends Obj> = {
    // Every key `r`'s objects name as erased, with the answer keys of the blocks `keys` opens: `Lines.erased`.
    readonly erased: (r: locked.Row<O>) => Promise<Erased>
    readonly forget: (blocks: readonly string[]) => Promise<lo.Settled<null>>
  }

  // The keys of `r` that may go now: none owed, or every owed one once its blocks are gone.
  export const forgotten = async <O extends Obj>(fx: Effects<O>, dead: locked.Dead<O>, r: locked.Row<O>): Promise<lo.Settled<readonly string[]>> => {
    // A purge that cannot read which keys are erased keeps every key, to be asked again.
    const read = await locked.owed(dead, r).then(
      (o): lo.Settled<string[]> => lo.val(o),
      (e: unknown): lo.Settled<string[]> => lo.error(lo.reason(e)),
    )
    if (read.type === "Error") return read
    const owed = read.v
    if (owed.length === 0) return lo.val(owed)
    const fed = await fx.erased(locked.of(r.replica, Object.fromEntries(owed.map(n => [n, r.keys[n]!])))).then(
      (e): lo.Settled<Erased> => lo.val(e),
      (e: unknown): lo.Settled<Erased> => lo.error(lo.reason(e)),
    )
    if (fed.type === "Error") return fed
    return lo.map(await fx.forget(owed.flatMap(n => fed.v[n] ?? [])), () => owed)
  }
}
export default purge
