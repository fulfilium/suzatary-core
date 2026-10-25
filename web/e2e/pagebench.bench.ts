/**
 * EVERY PAGE'S DRAW, MEASURED: each page at CPU throttling 4 on the read-only fixture, until it has drawn and gone
 * quiet: opened in a fresh tab (a new document, from its navigation's start), and visited again inside a tab (from
 * the address moving to it), several runs each, the median and the spread. A draw is the frame's
 * `page:draw` User Timing measure and, on a page drawn by a Typst view, its `typst:draw` with the nodes it
 * computed; the numbers are written to `$SUZATARY_PAGEBENCH` when it is set (the stage's report), and the scene's
 * assertions are only that every page drew.
 */

import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";

import { BASE, PHONE, suite } from "./harness";
import { hrefOf } from "../src/shell/route";
import type { Route } from "../src/shell/route";
import cast from "../test/cast";

const app = await suite("pagebench", PHONE);
const { todos } = cast.world;

const RATE = 4;
const RUNS = Number(process.env["SUZATARY_PAGEBENCH_RUNS"] ?? "5");
// One page or a few, by name, comma-separated; every page when unset.
const ONLY = (process.env["SUZATARY_PAGEBENCH_ONLY"] ?? "").split(",").filter((s) => s !== "");
// Quiet: no draw for this long after the last one.
const QUIET_MS = 700;

// A page drawn by Typst has its content when Typst has drawn it, not when the frame has: its time is to that.
const TYPST: ReadonlySet<string> = new Set(["typst"]);

const PAGES: readonly (readonly [string, Route])[] = [
  ["todos", { type: "todos" }],
  ["typst", { type: "typst", open: "" }],
  ["todo", { type: "todo", id: todos.urgent.id }],
  ["inbox", { type: "inbox" }],
  ["secretary", { type: "secretary", refs: [], topics: [] }],
  ["conversation", { type: "conversation", id: "default", refs: [] }],
  ["memory", { type: "memory" }],
  ["mail", { type: "mail", query: "", account: "" }],
  ["chain", { type: "chain", focus: "" }],
  ["usage", { type: "usage" }],
  ["about", { type: "about" }],
  ["settings", { type: "settings" }],
  ["briefings", { type: "briefings" }],
  ["calendar", { type: "calendar", date: "" }],
  ["briefing", { type: "briefing", date: "2026-10-01" }],
];

type Draw = { readonly name: string; readonly start: number; readonly duration: number; readonly detail: { readonly page?: string; readonly read?: string; readonly computed?: readonly unknown[]; readonly compiled?: number } };
// `drawn` is the time from the address moving until the page's last draw ended.
type Visit = { readonly drawn: number; readonly draws: number; readonly drawMs: number; readonly typstDraws: number; readonly computed: number; readonly compiled: number };

const draws = (since: number): string =>
  `[...performance.getEntriesByName("page:draw"), ...performance.getEntriesByName("typst:draw")].filter((e) => e.startTime >= ${since}).map((e) => ({ name: e.name, start: e.startTime, duration: e.duration, detail: e.detail ?? {} }))`;

// Opens `hash` in a fresh document and waits until a value is drawn and the page has gone quiet; times are from the
// navigation's start, which is the document's time origin.
const drawnBy = (name: string): string => (TYPST.has(name) ? `d.name === "typst:draw"` : `d.name === "page:draw" && d.detail.read === "Val"`);

const fresh = async (name: string, hash: string): Promise<Visit> => {
  await app.page.navigate("about:blank");
  await app.page.navigate(`${BASE}/${hash}`);
  await app.page.waitFor(
    `(() => { const ds = ${draws(0)}; const end = Math.max(0, ...ds.map((d) => d.start + d.duration));
      return ds.some((d) => ${drawnBy(name)}) && performance.now() - end > ${QUIET_MS}; })()`,
    { timeout: 60000 },
  );
  return measured(await app.page.evaluate<Draw[]>(draws(0)), 0);
};

const measured = (ds: readonly Draw[], t0: number): Visit => {
  const typst = ds.filter((d) => d.name === "typst:draw");
  return {
    drawn: Math.max(...ds.map((d) => d.start + d.duration)) - t0,
    draws: ds.filter((d) => d.name === "page:draw").length,
    drawMs: ds.filter((d) => d.name === "page:draw").reduce((t, d) => t + d.duration, 0) + typst.reduce((t, d) => t + d.duration, 0),
    typstDraws: typst.length,
    computed: typst.reduce((t, d) => t + (d.detail.computed?.length ?? 0), 0),
    compiled: typst.reduce((t, d) => t + (d.detail.compiled ?? 0), 0),
  };
};

// Moves the address to `hash` and waits until a value is drawn and the page has gone quiet.
const visit = async (name: string, hash: string): Promise<Visit> => {
  // The time is taken before the address moves: the shell may draw the page inside the assignment.
  const t0 = await app.page.evaluate<number>(`((t) => ((location.hash = ${JSON.stringify(hash)}), t))(performance.now())`);
  await app.page.waitFor(
    `(() => { const ds = ${draws(0)}.filter((d) => d.start >= ${t0}); const end = Math.max(0, ...ds.map((d) => d.start + d.duration));
      return ds.some((d) => ${drawnBy(name)}) && performance.now() - end > ${QUIET_MS}; })()`,
    { timeout: 60000 },
  );
  return measured((await app.page.evaluate<Draw[]>(draws(t0))).filter((d) => d.start >= t0), t0);
};

const median = (xs: readonly number[]): number => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length === 0 ? NaN : (s[Math.floor((s.length - 1) / 2)] ?? NaN);
};
const round = (x: number): number => Math.round(x * 10) / 10;

const spread = (xs: readonly number[]): string => `${round(Math.min(...xs))}–${round(Math.max(...xs))}`;

app.scene("measured: every page from a fresh tab and visited again, at throttling 4", async () => {
  await app.page.navigate(`${BASE}/#/nosuchpage`);
  await app.page.throttle(RATE);
  const rows: Record<string, unknown>[] = [];
  try {
    for (const [name, route] of PAGES.filter(([n]) => ONLY.length === 0 || ONLY.includes(n))) {
      const hash = hrefOf(route);
      const opened: Visit[] = [];
      for (let k = 0; k < RUNS; k++) opened.push(await fresh(name, hash));
      const again: Visit[] = [];
      for (let k = 0; k < RUNS; k++) {
        await visit("nosuchpage", "#/nosuchpage");
        again.push(await visit(name, hash));
      }
      assert.ok(opened.every((v) => Number.isFinite(v.drawn)), `${name} drew`);
      rows.push({
        page: name,
        fresh: round(median(opened.map((v) => v.drawn))),
        freshSpread: spread(opened.map((v) => v.drawn)),
        freshDrawMs: round(median(opened.map((v) => v.drawMs))),
        again: round(median(again.map((v) => v.drawn))),
        againSpread: spread(again.map((v) => v.drawn)),
        againDrawMs: round(median(again.map((v) => v.drawMs))),
        againDraws: median(again.map((v) => v.draws)),
        againComputed: median(again.map((v) => v.computed)),
        typst: opened.some((v) => v.typstDraws > 0),
      });
    }
  } finally {
    await app.page.throttle(1);
  }
  console.log(JSON.stringify(rows, null, 1));
  const out = process.env["SUZATARY_PAGEBENCH"];
  if (out !== undefined && out !== "") writeFileSync(out, JSON.stringify({ rate: RATE, runs: RUNS, rows }, null, 1));
});
