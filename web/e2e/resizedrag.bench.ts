/**
 * DRAGS WITH A TODO OPEN (docs/TILEDLAYOUT-PLAN-2026-11-22.md §8.1: a resize must fit a frame), with a todo beside the
 * list. The window is narrowed from 1,440 px to 900 px and widened back, 8 px a step, one step a frame, so the tiles are
 * placed again at every step; and the divider between the list and the todo is dragged by a mouse, 8 px a frame, across
 * the room and back, so it snaps from stop to stop. The page's main thread runs `SUZATARY_RESIZEDRAG_RATE` times slower
 * (default 6, the slowest profile the scenes use). Each prints the frames' durations (median, 95th percentile,
 * slowest), how many were over a 60 Hz frame's budget, and the page's long animation frames. A measurement, not a test.
 *
 *   scripts/web-e2e.sh resizedrag
 */

import { BASE, SETTLED, suite } from "./harness"
import cast from "../test/cast"

const { todos } = cast.world
const RATE = Number(process.env["SUZATARY_RESIZEDRAG_RATE"] ?? "6")

const app = await suite("resizedrag")

const BUDGET = 1000 / 60

// Opens a todo beside the list, throttles the page, and starts counting its frames.
const ready = async (): Promise<void> => {
  await app.resize([1440, 900])
  await app.page.navigate(`${BASE}/#/todo/${todos.flat.id}`)
  await app.page.waitFor(`document.querySelector("#view.tiled .tile .todospane") !== null && document.querySelector(".tile .todoview .todobody") !== null`, { timeout: 20000 })
  await app.page.evaluate(SETTLED)
  await app.page.throttle(RATE)
  await app.page.evaluate(`(() => {
    window.__stop = false; window.__frames = []; window.__long = [];
    let last = performance.now();
    const tick = (t) => { window.__frames.push(t - last); last = t; if (!window.__stop) requestAnimationFrame(tick); };
    requestAnimationFrame(tick);
    new PerformanceObserver((l) => window.__long.push(...l.getEntries().map((e) => Math.round(e.duration)))).observe({ type: "long-animation-frame", buffered: false });
  })()`)
}

const frame = (): Promise<unknown> => app.page.evaluate(`new Promise((done) => requestAnimationFrame(() => done()))`)

// Stops counting, and prints what was counted.
const told = async (what: string, steps: number): Promise<void> => {
  await app.page.evaluate(SETTLED)
  const { frames, long } = await app.page.evaluate<{ frames: number[]; long: number[] }>(`(window.__stop = true, { frames: window.__frames.slice(1), long: window.__long })`)
  await app.page.throttle(1)
  const sorted = [...frames].sort((a, b) => a - b)
  const at = (q: number): number => Math.round(sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? 0)
  const over = frames.filter(f => f > BUDGET + 1).length
  console.log(`${what}: ${steps} steps, ${frames.length} frames at ${RATE}x: median ${at(0.5)} ms, p95 ${at(0.95)} ms, slowest ${at(1)} ms, ${over} over ${BUDGET.toFixed(1)} ms; long animation frames ${JSON.stringify(long)}`)
}

app.scene(`a drag-resize with a todo open, at a ${RATE}x CPU throttle`, async () => {
  await ready()
  const widths = [...Array.from({ length: 68 }, (_, i) => 1440 - 8 * i), ...Array.from({ length: 68 }, (_, i) => 904 + 8 * i)]
  for (const w of widths) {
    await app.page.viewport(w, 900)
    await frame()
  }
  await told("resizedrag", widths.length)
})

app.scene(`a drag of the divider between the list and the todo, at a ${RATE}x CPU throttle`, async () => {
  await ready()
  const [x, y] = await app.page.evaluate<[number, number]>(`(() => { const r = document.querySelector(".tileline").getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`)
  const xs = [...Array.from({ length: 120 }, (_, i) => x + 8 * i), ...Array.from({ length: 160 }, (_, i) => x + 952 - 8 * i)].filter(p => p > 2 && p < 1438)
  const path = xs.map(p => [p, y] as const)
  await app.page.pointer("Down", [x, y], "Mouse")
  for (const p of path) {
    await app.page.pointer("Move", p, "Mouse")
    await frame()
  }
  await app.page.pointer("Up", path[path.length - 1] ?? [x, y], "Mouse")
  await told("dividerdrag", path.length)
})
