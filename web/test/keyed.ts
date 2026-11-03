// The keys a test hands the core's lines: landed as the one piece of a skeleton holding no node, since the lines take
// keys only with the rows of their pull.

import type { Lines } from "../src/prodrome/core"
import type { SkeletonPayload } from "../src/wire/generated/wire"

/** A skeleton holding no node: the one piece that hands the lines only keys. */
export const NOTHING: SkeletonPayload = {
  heads: { are: "these", heads: [] },
  geneses: [],
  epoch: 1,
  nodes: [],
  since: "Known",
  frontier: { digest: "0".repeat(64), count: 0 },
  keys: {},
  rest: 0,
  size: 0,
  weight: 0,
}

/** `lines` holding the keys `keys` (`{<name>: <hex>}`), landed. */
export const keyed = (lines: Lines, keys: Readonly<Record<string, string>>): void => {
  lines.piece(JSON.stringify(NOTHING), JSON.stringify(keys), 0)
  lines.land().free()
}
