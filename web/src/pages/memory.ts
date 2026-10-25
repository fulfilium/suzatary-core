// `#/secretary/memory`: every fact the secretary keeps, by kind, each with Forget.

import type { MemoryPayload } from "../wire/generated/wire"
import { getMemory } from "../wire/api"
import app from "../app/app"
import asked from "../app/asked"
import reading from "../algebra/reading"
import memory from "../chat/memory"
import frame from "../view/frame"
import placeholder from "../view/placeholder"

export namespace memoryPage {
  export type T = { readonly p: MemoryPayload; readonly writes: boolean }

  const kept = asked.question(getMemory)
  const writes = reading.held(() => app.writes(app.now()), moved => app.hear(app.writes, moved))

  export const page: frame.Page<T> = {
    className: "memorypage",
    title: "Memory",
    back: { label: "Secretary", fallback: { type: "secretary", refs: [], topics: [] } },
    legend: v => memory.legend(v.p),
    reads: () => reading.map(reading.both(kept, writes), ([p, w]) => ({ p, writes: w })),
    shape: placeholder.ROWS,
    view: (v, _, again) => memory.view(v.p, memory.forgets(v.writes, again)),
  }

  export const mount = (root: HTMLElement) => frame.mount(root, page)
}
export default memoryPage
