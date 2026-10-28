// A store whose objects are small, as one store of the core's worker (`engine`): each node is the object with its
// literal, so it carries its content and no reading waits on a fetch, and the box locks none of its words. The inbox and
// the two topic stores are such stores.

import engine from "./engine"
import type excluded from "./excluded"
import locked from "./locked"
import noderows from "./noderows"
import { dropRows, heldNames, heldRow, parseReplica, storeRows } from "./objects"
import type progress from "./progress"
import pull from "./pull"
import { losses } from "./replica"
import type { Replica } from "./replica"
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

  // The rows a build before version 9 kept of the store in the rows' store: its replica under `pages/<name>`, and under
  // `store/<name>` beside the keys of its words, always none, or bare in a build before the keys. Each may hold what
  // this tab wrote and the box has not taken, so each is joined into the store's own rows, then deleted with its pieces.
  // A row that does not read, or a read that fails, deletes nothing and is said, to be carried at the next load.
  const keyedRows = storeRows(payload.StoredObject, locked.NONE, losses)
  const oldRows = {
    parse: (raw: unknown): lo.Settled<Replica> => {
      const keyed = keyedRows.parse(raw)
      return keyed.type === "Val" ? lo.val(keyed.v.replica) : parseReplica(raw)
    },
  }
  export const carried = async (name: Name, place: noderows.Place<StoredObject>): Promise<lo.Settled<null>> => {
    const prefixes = [`pages/${name}`, pull.kept(name)]
    const there = await Promise.all(prefixes.map(heldNames))
    const unlisted = there.flatMap((t, i) => (t.type === "Error" ? [`${prefixes[i]}: ${t.e}`] : []))
    if (unlisted.length > 0) return lo.error(`the rows a build before this one kept could not be listed (${unlisted.join("; ")})`)
    if (there.every(t => t.type === "Val" && t.v.length === 0)) return lo.val(null)
    const read = await Promise.all(prefixes.map(prefix => heldRow(oldRows, prefix)))
    const unread = read.flatMap((r, i) => (r.type === "Unreadable" ? [`${prefixes[i]}: ${r.why}`] : []))
    if (unread.length > 0) return lo.error(`a row a build before this one kept does not read (${unread.join("; ")})`)
    const rows = read.flatMap(r => (r.type === "Held" ? [locked.of(r.v)] : []))
    if (rows.length > 0) {
      const landed = await place.land(rows.reduce(locked.join))
      if (landed.row.type === "Error") return landed.row
    }
    for (const prefix of prefixes) {
      const dropped = await dropRows(prefix)
      if (dropped.type === "Error") return dropped
    }
    return lo.val(null)
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
        // A carry that failed is said, and tried again at the next load.
        restoring: () =>
          (idb.available() ? carried(name, place) : Promise.resolve(lo.val(null))).then(done =>
            fx.told(`carry/${pull.kept(name)}`, pull.unread(name, done.type === "Error" ? `the copy a build before this one kept was not moved: ${done.e}; it is tried again at the next load` : null)),
          ),
        landed: () => Promise.resolve(),
      },
      { told: fx.told, bars: e => fx.bars({ ...e, name }), replaced: fx.replaced },
    )
  }
}
export default objectsync
