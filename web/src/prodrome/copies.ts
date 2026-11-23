// The names of the copies the device keeps, as a reading's rows name them: a store's log, and a chain's beside it under
// a name no store has.

import excluded from "./excluded"

export namespace copies {
  // The log of the chain `chain`.
  export const chain = (chain: string): string => `chain/${chain}`

  // A store's copy on the device, under a key no chain can be named.
  export const kept = (name: string): string => `store/${name}`

  // A copy that does not read is said, with its own reset, never passed over.
  export const unread = (name: string, why: string | null): excluded.Left =>
    excluded.left(excluded.NONE, [], why === null ? [] : [excluded.unread(kept(name), why)])

  // A part of a copy that did not read and was set aside (`what`), said with the reset that removes it: the store works
  // without it.
  export const aside = (name: string, what: string | null): excluded.Left =>
    excluded.left(excluded.NONE, [], what === null ? [] : [excluded.aside(kept(name), what)])

  // The box's own copy of a store that is not closed under its parents, held unchanged and not taken again for that.
  export const open = (name: string, why: string | null): excluded.Left =>
    excluded.left(excluded.NONE, [], why === null ? [] : [{ chain: kept(name), why, cure: "None" }])
}
export default copies
