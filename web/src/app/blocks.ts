// The rendered blocks as the page reads them, through the core's worker, the one reader and writer of the device's rows.
// The page writes no block to the device and hashes no source.

import rendered from "./rendered"
import remote from "../prodrome/remote"
import type table from "../algebra/table"

export namespace blocks {
  const worker: rendered.Store = rendered.viaWorker({
    get: keys => remote.ask({ type: "Blocks", keys }, "Page"),
    join: pairs => remote.ask({ type: "KeepBlocks", pairs }, "Page"),
    usage: () => remote.ask({ type: "BlocksUsage" }, "Page"),
  })

  // The blocks a page placed before this device held their HTML, by key: each is answered by the join that keeps it.
  const awaited = new Map<table.Key, ((html: string) => void)[]>()

  export const store: rendered.Store = {
    ...worker,
    join: pairs =>
      worker.join(pairs).then(findings => {
        const refused = new Set(findings)
        for (const [k, html] of pairs) {
          const waiting = awaited.get(k)
          if (waiting === undefined || refused.has(k)) continue
          awaited.delete(k)
          waiting.forEach(answer => answer(html))
        }
        return findings
      }),
  }

  // The HTML this device holds for `key`, now or once a join keeps it: what the painter places a block from, so a block
  // placed before its fold was kept shows its HTML when the fold lands, and its plain text until then. It waits for a
  // join before it reads, so a join that lands while the read is on its way is heard too; whichever answers first wins.
  export const held = (key: table.Key): Promise<string | null> =>
    new Promise<string>(answer => {
      awaited.set(key, [...(awaited.get(key) ?? []), answer])
      void store.get([key]).then(
        r => {
          const html = r[0]
          if (html === null || html === undefined) return
          const rest = (awaited.get(key) ?? []).filter(f => f !== answer)
          if (rest.length === 0) awaited.delete(key)
          else awaited.set(key, rest)
          answer(html)
        },
        () => {},
      )
    })

}
export default blocks
