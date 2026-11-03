// A store whose objects are small, as one store of the core's worker (`engine`): each node is the object with its
// literal, so it carries its content and no reading waits on a fetch, and the box locks none of its words. The inbox and
// the two topic stores are such stores.

import engine from "./engine"
import type excluded from "./excluded"
import locked from "./locked"
import noderows from "./noderows"
import type progress from "./progress"
import type stores from "./stores"
import idb from "../app/idb"
import lo from "../algebra/lo"
import payload from "../wire/generated/payload"
import storereads from "../wire/storereads"
import type { StoredObject } from "../wire/generated/wire"

export namespace objectsync {
  export type Name = Exclude<stores.Name, "lines">

  // The form of these stores' nodes on the device: a change of it takes them again from the box.
  export const FORMAT = 1

  // Who hears the store: a reading's leavings by reader, a pull's progress, a move, and a later epoch.
  export type Effects = {
    readonly told: (reader: string, left: excluded.Left) => void
    readonly bars: (e: progress.Event) => void
    readonly moved: () => void
    readonly replaced: () => void
  }

  export const of = (name: Name, db: idb.Db, fx: Effects): engine.T<StoredObject> => {
    // Its rows lock no words, so rows of another form find no block.
    const place = noderows.placeOf(db, payload.StoredObject, { version: FORMAT, dropped: () => Promise.resolve(lo.val(null)) })
    return engine.of<StoredObject>(
      {
        name,
        place,
        ask: since =>
          storereads.objects(name, since).then(a => ({ said: a, erased: [], piece: r => locked.of(r), rest: a.rest, size: a.size, weight: a.weight })),
        content: engine.INLINE,
        words: engine.open(),
        took: () => fx.moved(),
        landed: () => Promise.resolve(),
      },
      { told: fx.told, bars: e => fx.bars({ ...e, name }), replaced: fx.replaced },
    )
  }
}
export default objectsync
