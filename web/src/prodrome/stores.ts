// The secretary's stores, by the name the box serves each under; each is a replica the core's worker holds
// (`storeset`).

import type { StoreName, WrittenName } from "../wire/generated/wire"

export namespace stores {
  export type Name = StoreName
  export const NAMES: readonly Name[] = ["inbox", "lines", "topics", "topic-aliases", "asks"]
  // The stores a tab writes, as the box's rows say (`stores!`, its writer column); the box alone writes the rest.
  export type Written = WrittenName
}
export default stores
