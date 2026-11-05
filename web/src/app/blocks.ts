// The rendered blocks as the page reads them, through the core's worker, the one reader and writer of the device's rows.
// The page writes no block to the device and hashes no source.

import rendered from "./rendered"
import remote from "../prodrome/remote"
import type table from "../algebra/table"

export namespace blocks {
  export const store: rendered.Store = rendered.viaWorker({
    get: keys => remote.ask({ type: "Blocks", keys }, "Page"),
    join: pairs => remote.ask({ type: "KeepBlocks", pairs }, "Page"),
    usage: () => remote.ask({ type: "BlocksUsage" }, "Page"),
    own: () => remote.ask({ type: "BlocksOwn" }, "Page"),
  })

  export const held = (key: table.Key): Promise<string | null> => store.get([key]).then(r => r[0] ?? null)
}
export default blocks
