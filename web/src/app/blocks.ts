// The rendered blocks as the page reads them: a view in memory by name over the core's worker, the one reader and writer
// of the device's rows (`rendered.device`), and the key a block is kept by, which the worker's core names. The page
// writes no block to the device and hashes no source.

import rendered from "./rendered"
import remote from "../prodrome/remote"
import type table from "../algebra/table"

export namespace blocks {
  export const store: rendered.Store = rendered.viaWorker({
    get: keys => remote.ask({ type: "Blocks", keys }),
    join: pairs => remote.ask({ type: "KeepBlocks", pairs }),
    usage: () => remote.ask({ type: "BlocksUsage" }),
    own: () => remote.ask({ type: "BlocksOwn" }),
  })

  // The key an answer block of `source` is kept by, or none where the worker does not answer.
  export const answerKey = (source: string): Promise<table.Key | null> =>
    remote.ask({ type: "AnswerKeys", sources: [source] }).then(ks => ks[0] ?? null, () => null)
}
export default blocks
