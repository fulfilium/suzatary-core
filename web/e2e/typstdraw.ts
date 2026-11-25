// The Typst list's own draw, run on a host of its own in a page of the box: what `typstview.test.ts` checks a redraw
// computes, and what `typstmeasure.bench.ts` times.

import { execFileSync } from "node:child_process";

import type { Page } from "./cdp";
import { TYPST_MODULE } from "./harness";
import type { Entry } from "../src/wire/generated/wire";
import viewstate from "../src/typst/state";

export type Detail = Readonly<Record<string, number>>;
export const entries = (name: string): string => `performance.getEntriesByName(${JSON.stringify(name)}).map((e) => ({ duration: e.duration, end: e.startTime + e.duration, detail: e.detail }))`;

// `typstbench.ts` bundled for the browser, evaluated rather than imported from a `blob:`, which the page's policy
// refuses; its modules are named as the dev build's.
const BENCH = execFileSync(
  "esbuild",
  ["web/e2e/typstbench.ts", "--bundle", "--format=iife", "--global-name=typstbench", "--target=es2022", "--define:__DEV__=false", "--external:./typst.js", "--external:./prodrome.js", "--log-level=error"],
  { encoding: "utf8" },
).replaceAll(`"./typst.js"`, `"./b/dev/typst.js"`).replaceAll(`"./prodrome.js"`, `"./b/dev/prodrome.js"`);

// Installs the bench and a drawer over the kit the page serves, at `window.__bench` (a fresh one each call).
export const bench = `(async () => {
  window.typstbench ??= (() => { ${BENCH}; return typstbench; })();
  if (window.__glue === undefined) {
    const g = await import(new URL("./b/dev/typst.js", location.href).href);
    await g.default({ module_or_path: new URL("./b/dev/${TYPST_MODULE}", location.href).href });
    const c = await import(new URL("./b/dev/prodrome.js", location.href).href);
    await c.default({ module_or_path: new URL("./b/dev/prodrome_bg.wasm", location.href).href });
    const read = (f) => fetch(new URL("./b/dev/typst/" + f, location.href).href).then((r) => r.text());
    const [kit, list, packages] = await Promise.all(["kit.typ", "list.typ", "packages.json"].map(read));
    window.__glue = { g, c, files: { ...JSON.parse(packages), "/kit.typ": kit, "/list.typ": list } };
  }
  window.__bench?.done();
  window.__bench = window.typstbench.drawer(window.__glue.g, window.__glue.c, window.__glue.files, "/list.typ");
  return true;
})()`;

export type Computed = { type: string; key: string | null; depth: number };
export type Redraw = { ms: number; tree: number; fold: number; place: number; compiled: number; computed: Computed[]; placed: { kept: number; refilled: number; made: number; moved: number } };
export const redraw = (s: viewstate.State): string => `window.__bench.draw(${JSON.stringify(s)})`;

// The fixture's rows, `n` of them under distinct references, titles and values, as long a list as the owner's or longer.
export const sized = (small: viewstate.State, n: number, salt: string): viewstate.State => ({
  ...small,
  rows: Array.from({ length: n }, (_, i) => {
    const r = small.rows[i % small.rows.length] ?? small.rows[0];
    if (r === undefined) throw new Error("the fixture has no rows");
    const title = viewstate.fold<viewstate.Text>(r.title, { Prose: (t) => viewstate.prose(`${t} ${salt}${i}`), Markup: (m) => ({ type: "Markup", source: `${m} ${salt}${i}` }) });
    return { ...r, ref: `todo:life/row${i}`, title, value: ((i * 37) % 100) / 100 };
  }),
});
export const retitled = (s: viewstate.State, at: number, title: string): viewstate.State => ({ ...s, rows: s.rows.map((r, i) => (i === at ? { ...r, title: viewstate.prose(title) } : r)) });
export const opened = (s: viewstate.State, at: number): viewstate.State => {
  const row = s.rows[at];
  return row === undefined ? s : { ...s, open: { type: "Todo", row, created: "2026-10-01", detail: viewstate.prose("what the todo says, in full"), why: [] } };
};

/** The Typst list's state over the fold `base` serves, as the page builds it. */
export async function fixtureState(page: Page, base: string): Promise<viewstate.State> {
  await page.navigate(`${base}/#/typst`);
  await page.waitFor(`document.querySelector(".typstframe .tview") !== null`, { timeout: 30000 });
  const fold = await page.evaluate<{ entries: Entry[]; at: string }>(`fetch("/api/fold").then((r) => r.json())`);
  return viewstate.of({ prose: (m) => m.replace(/\\(.)/g, "$1") }, { entries: fold.entries, at: fold.at, chain: "life", open: "", writes: true });
}
