// The secretary's stores, by the name the box serves each under. Every one is a replica the core's worker holds
// (`storeset`), one engine each (`engine`).

import type { StoreName } from "../wire/generated/wire"

export namespace stores {
  export type Name = StoreName
  export const NAMES: readonly Name[] = ["inbox", "lines", "topics", "topic-aliases"]
}
export default stores
