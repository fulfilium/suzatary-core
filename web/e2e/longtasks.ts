/**
 * The page's long tasks and its painter's steps, as the bigstore scene and the openprof bench wait on them: a phase is
 * over once what it waits for is drawn and the page has run no long task and no painter's step for `QUIET_MS`, so a
 * later slice or placing is counted in the phase it belongs to.
 */

import marks from "../src/chat/marks";
import type { Page } from "./cdp";

export const QUIET_MS = 1500;

// Every long task the page's thread ran since the document started, kept in the page as [start, ms].
export const OBSERVE = `(() => {
  window.__long = []
  new PerformanceObserver((list) => { for (const e of list.getEntries()) window.__long.push([e.startTime, e.duration]) })
    .observe({ type: "longtask", buffered: true })
})()`;

// When the latest conversation's window was placed, or -1 before any.
export const placed = `(performance.getEntriesByName(${JSON.stringify(marks.PLACED)}).at(-1)?.startTime ?? -1)`;

// A round of the generated conversations, drawn in the stream.
export const BULK = `document.querySelectorAll('.streamround .streammeta a[href*="bulk-"]').length > 0`;

// Whether the page has run no long task and no painter's step for `QUIET_MS`, counted from `drawn` at the earliest. The
// stream draws its rounds a slice per frame, each slice a step that can be short, so its last step ends the wait.
const quiet = (drawn: number): string => `(() => {
  const l = window.__long.at(-1)
  const s = performance.getEntriesByName("painter:step").at(-1)
  return performance.now() - Math.max(${drawn}, l === undefined ? 0 : l[0] + l[1], s === undefined ? 0 : s.startTime + s.duration) > ${QUIET_MS}
})()`;

/** Waits until `until` holds and then until the page is quiet; answers when `until` held, on the page's clock. */
export const settle = async (page: Page, until: string, timeout: number): Promise<number> => {
  await page.waitFor(until, { timeout });
  const drawn = await page.evaluate<number>(`performance.now()`);
  await page.waitFor(quiet(drawn), { timeout });
  return drawn;
};
