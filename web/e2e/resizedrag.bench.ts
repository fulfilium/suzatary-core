/**
 * A DRAG-RESIZE WITH A TODO OPEN (docs/TILEDLAYOUT-PLAN-2026-11-22.md §8.1: a resize must fit a frame). The window is
 * narrowed from 1,440 px to 900 px and widened back, 8 px a step, one step a frame, with a todo beside the list, so the
 * tiles are placed again at every step and the list leaves and comes back at the edge. The page's main thread runs
 * `SUZATARY_RESIZEDRAG_RATE` times slower (default 6, the slowest profile the scenes use). It prints the frames'
 * durations (median, 95th percentile, slowest) and the page's long animation frames. A measurement, not a test.
 *
 *   scripts/web-e2e.sh resizedrag
 */

import { BASE, SETTLED, suite } from "./harness"
import cast from "../test/cast"

const { todos } = cast.world
const RATE = Number(process.env["SUZATARY_RESIZEDRAG_RATE"] ?? "6")

const app = await suite("resizedrag")

app.scene(`a drag-resize with a todo open, at a ${RATE}x CPU throttle`, async () => {
  await app.resize([1440, 900])
  await app.page.navigate(`${BASE}/#/todo/${todos.flat.id}`)
  await app.page.waitFor(`document.querySelector("#view.tiled .tile .todospane") !== null && document.querySelector(".tile .todoview .todobody") !== null`, { timeout: 20000 })
  await app.page.evaluate(SETTLED)
  await app.page.throttle(RATE)
  await app.page.evaluate(`(() => {
    window.__frames = []; window.__long = [];
    let last = performance.now();
    const tick = (t) => { window.__frames.push(t - last); last = t; if (!window.__stop) requestAnimationFrame(tick); };
    requestAnimationFrame(tick);
    new PerformanceObserver((l) => window.__long.push(...l.getEntries().map((e) => Math.round(e.duration)))).observe({ type: "long-animation-frame", buffered: false });
  })()`)
  const widths = [...Array.from({ length: 68 }, (_, i) => 1440 - 8 * i), ...Array.from({ length: 68 }, (_, i) => 904 + 8 * i)]
  for (const w of widths) {
    await app.page.viewport(w, 900)
    await app.page.evaluate(`new Promise((done) => requestAnimationFrame(() => done()))`)
  }
  await app.page.evaluate(SETTLED)
  const { frames, long } = await app.page.evaluate<{ frames: number[]; long: number[] }>(`(window.__stop = true, { frames: window.__frames.slice(1), long: window.__long })`)
  await app.page.throttle(1)
  const sorted = [...frames].sort((a, b) => a - b)
  const at = (q: number): number => Math.round(sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? 0)
  console.log(`resizedrag: ${widths.length} steps, ${frames.length} frames at ${RATE}x: median ${at(0.5)} ms, p95 ${at(0.95)} ms, slowest ${at(1)} ms; long animation frames ${JSON.stringify(long)}`)
})
