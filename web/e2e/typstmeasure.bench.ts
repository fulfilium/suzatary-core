/**
 * Measures the Typst list: the modules' and the bundle's bytes on a first visit's wire, the time to first view, the
 * page's redraw after an Open intent, and the draw's first draw, one-row change and Open intent over 53 and 250 rows,
 * at CPU throttling 1, 4 and 6, the median of ten runs, written to `$SUZATARY_SHOTS/typst-measure.json`. Its one
 * assertion is that the draw was measured, and no gate runs it: `scripts/web-e2e.sh typstmeasure` does, by name.
 */

import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import type { Page } from "./cdp";
import { PHONE, TYPST_MODULE, WRITE_BASE, suite } from "./harness";
import type { Detail, Redraw } from "./typstdraw";
import { bench, entries, fixtureState, opened, redraw, retitled, sized } from "./typstdraw";

const OUT = process.env["SUZATARY_SHOTS"] ?? join(tmpdir(), "suzatary-shots");
mkdirSync(OUT, { recursive: true });

const app = await suite("typstmeasure", PHONE);

const RATES = [1, 4, 6] as const;
const RUNS = 10;

const median = (xs: readonly number[]): number => {
  const s = [...xs].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return s.length % 2 === 0 ? ((s[m - 1] ?? 0) + (s[m] ?? 0)) / 2 : (s[m] ?? 0);
};

async function typst(page: Page): Promise<void> {
  await page.navigate(`${WRITE_BASE}/#/typst`);
  await page.waitFor(`document.querySelector(".typstframe .tview") !== null`, { timeout: 30000 });
}

app.scene("measured: bytes, first view, and the draw's first, one-row and open redraws, at throttling 1, 4 and 6", async () => {
  await app.page.cold(true);
  await app.page.navigate("about:blank");
  await typst(app.page);
  await app.page.cold(false);
  const sizes = (file: string) =>
    `(() => { const r = performance.getEntriesByType("resource").find((e) => e.name.endsWith("/${file}")); return { decoded: r?.decodedBodySize ?? 0, encoded: r?.encodedBodySize ?? 0 }; })()`;
  const bytes = {
    typst: await app.page.evaluate<{ decoded: number; encoded: number }>(sizes(TYPST_MODULE)),
    core: await app.page.evaluate<{ decoded: number; encoded: number }>(sizes("prodrome_bg.wasm")),
    bundle: await app.page.evaluate<{ decoded: number; encoded: number }>(sizes("app.js")),
  };
  const small = await fixtureState(app.page, WRITE_BASE);
  const SHAPES = [53, 250] as const;

  const results: Record<string, unknown>[] = [];
  for (const rate of RATES) {
    await app.page.throttle(rate);
    const first: { load: Detail; view: number; sinceNavigation: number; draw: number }[] = [];
    await app.page.cold(true);
    for (let k = 0; k < RUNS; k++) {
      await app.page.navigate("about:blank");
      await app.page.navigate(`${WRITE_BASE}/#/typst`);
      await app.page.waitFor(`performance.getEntriesByName("typst:first-view").length > 0`, { timeout: 120000 });
      const [load] = await app.page.evaluate<{ detail: Detail }[]>(entries("typst:load"));
      const [view] = await app.page.evaluate<{ duration: number; end: number }[]>(entries("typst:first-view"));
      const draws = (await app.page.evaluate<{ duration: number; end: number }[]>(entries("typst:draw"))).filter((d) => d.end <= (view?.end ?? 0));
      first.push({ load: load?.detail ?? {}, view: view?.duration ?? NaN, sinceNavigation: view?.end ?? NaN, draw: draws.reduce((t, d) => t + d.duration, 0) });
    }
    await app.page.cold(false);

    // The page's own redraws after an Open intent, pressed ten times on two rows: each moves two rows and the pane.
    await typst(app.page);
    const ids = await app.page.evaluate<string[]>(`[...document.querySelectorAll(".typstframe .topen")].slice(0, 2).map((b) => b.getAttribute("data-intent"))`);
    const before = await app.page.evaluate<number>(`performance.getEntriesByName("typst:draw").length`);
    for (let k = 0; k < RUNS; k++) {
      const text = ids[k % ids.length] ?? "";
      await app.page.click(`.typstframe [data-intent="${text}"]`);
      await app.page.waitFor(`performance.getEntriesByName("typst:draw").length > ${before + k} && document.querySelector('.trow:has(> .tinplace) [data-intent="${text}"]') !== null`, { timeout: 20000 });
    }
    const redraws = (await app.page.evaluate<{ duration: number; detail: Detail }[]>(entries("typst:draw"))).slice(before);

    // On a drawer of its own; an Open alternating between two rows, as the page's does, finds the page cached after two.
    const shapes: Record<string, unknown> = {};
    for (const n of SHAPES) {
      const firsts: Redraw[] = [];
      const ones: Redraw[] = [];
      const opens: Redraw[] = [];
      const reopens: Redraw[] = [];
      for (let k = 0; k < RUNS; k++) {
        await app.page.evaluate(bench);
        const s = sized(small, n, `r${rate}k${k}n`);
        firsts.push(await app.page.evaluate<Redraw>(redraw(s)));
        if (k === 0) {
          for (let j = 0; j < RUNS; j++) {
            ones.push(await app.page.evaluate<Redraw>(redraw(retitled(s, (j * 7) % n, `changed ${rate} ${j}`))));
          }
          await app.page.evaluate<Redraw>(redraw(s));
          for (let j = 0; j < RUNS; j++) opens.push(await app.page.evaluate<Redraw>(redraw(opened(s, (5 + 11 * j) % n))));
          for (let j = 0; j < RUNS; j++) reopens.push(await app.page.evaluate<Redraw>(redraw(opened(s, j % 2 === 0 ? 3 : 4))));
        }
      }
      await app.page.evaluate(`(window.__bench.done(), true)`);
      const summary = (rs: Redraw[]) => ({
        ms: median(rs.map((r) => r.ms)),
        tree: median(rs.map((r) => r.tree)),
        fold: median(rs.map((r) => r.fold)),
        place: median(rs.map((r) => r.place)),
        computed: median(rs.map((r) => r.computed.length)),
        compiled: median(rs.map((r) => r.compiled)),
        rowsComputed: median(rs.map((r) => r.computed.filter((c) => c.type === "Row").length)),
        made: median(rs.map((r) => r.placed.made)),
      });
      shapes[`rows${n}`] = { first: summary(firsts), one: summary(ones), open: summary(opens), reopen: summary(reopens) };
    }
    results.push({
      rate,
      fetch: median(first.map((f) => f.load["fetch"] ?? NaN)),
      instantiate: median(first.map((f) => f.load["instantiate"] ?? NaN)),
      firstDraws: median(first.map((f) => f.draw)),
      firstView: median(first.map((f) => f.view)),
      firstViewSinceNavigation: median(first.map((f) => f.sinceNavigation)),
      redraw: median(redraws.map((r) => r.duration)),
      redrawFold: median(redraws.map((r) => r.detail["fold"] ?? NaN)),
      redrawPlace: median(redraws.map((r) => r.detail["place"] ?? NaN)),
      redrawTree: median(redraws.map((r) => r.detail["tree"] ?? NaN)),
      redrawCompiled: median(redraws.map((r) => r.detail["compiled"] ?? NaN)),
      fixtureRows: small.rows.length,
      ...shapes,
    });
  }
  await app.page.throttle(1);
  const measured = { bytes, runs: RUNS, results };
  writeFileSync(join(OUT, "typst-measure.json"), JSON.stringify(measured, null, 2));
  console.log(JSON.stringify(measured, null, 2));
  const at4 = results.find((r) => r["rate"] === 4);
  assert.ok(at4 !== undefined && at4["rows250"] !== undefined, "the draw was measured at rate 4");
});
