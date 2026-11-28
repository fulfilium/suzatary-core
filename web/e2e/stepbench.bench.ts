/**
 * Measures the steps each page makes while it opens, at CPU throttling 4 on the read-only fixture, in a fresh tab: a step is
 * `painter:step` (the whole of it: update, view, diff, patch bytes, apply) with its program's own part (`wasm`) and its patch
 * bytes in the detail, and `page:subs` is the host taking and applying that step's subscription patches. The numbers go to
 * `$SUZATARY_STEPBENCH` when it is set, the scene asserts only that every page stepped, and no gate runs it:
 * `scripts/web-e2e.sh stepbench` does, by name.
 */

import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";

import { BASE, PHONE, suite } from "./harness";
import { hrefOf } from "../src/shell/route";
import type { Route } from "../src/shell/route";
import cast from "../test/cast";

const app = await suite("stepbench", PHONE);
const { todos } = cast.world;

const RATE = 4;
const RUNS = Number(process.env["SUZATARY_STEPBENCH_RUNS"] ?? "5");
// Quiet: no step for this long after the last one.
const QUIET_MS = 1500;

const PAGES: readonly (readonly [string, Route])[] = [
  ["todos", { type: "todos" }],
  ["todo", { type: "todo", id: todos.urgent.id }],
  ["inbox", { type: "inbox" }],
  ["secretary", { type: "secretary", refs: [], topics: [] }],
  ["conversation", { type: "conversation", id: "default", refs: [] }],
  ["memory", { type: "memory" }],
  ["mail", { type: "mail", query: "", account: "" }],
  ["about", { type: "about" }],
  ["settings", { type: "settings" }],
];

type Entry = { readonly name: string; readonly duration: number; readonly detail: { readonly bytes?: number; readonly wasm?: number; readonly patches?: number } | null };

const entries = `[...performance.getEntriesByName("painter:step"), ...performance.getEntriesByName("page:subs")].map((e) => ({ name: e.name, start: e.startTime, duration: e.duration, detail: e.detail }))`;

const median = (xs: readonly number[]): number => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length === 0 ? NaN : (s[Math.floor((s.length - 1) / 2)] ?? NaN);
};
const round = (x: number): number => Math.round(x * 100) / 100;

const opened = async (hash: string): Promise<readonly Entry[]> => {
  await app.page.navigate("about:blank");
  await app.page.navigate(`${BASE}/${hash}`);
  await app.page.waitFor(
    `(() => { const es = ${entries}; const end = Math.max(0, ...es.map((e) => e.start + e.duration));
      return es.length > 0 && performance.now() - end > ${QUIET_MS}; })()`,
    { timeout: 60000 },
  );
  return app.page.evaluate<Entry[]>(entries);
};

app.scene("measured: the steps each page makes while it opens, at throttling 4", async () => {
  await app.page.navigate(`${BASE}/#/nosuchpage`);
  await app.page.throttle(RATE);
  const rows: Record<string, unknown>[] = [];
  try {
    for (const [name, route] of PAGES) {
      const steps: Entry[] = [];
      const subs: Entry[] = [];
      for (let k = 0; k < RUNS; k++) {
        for (const e of await opened(hrefOf(route))) (e.name === "painter:step" ? steps : subs).push(e);
      }
      assert.ok(steps.length > 0, `${name} stepped`);
      rows.push({
        page: name,
        steps: steps.length / RUNS,
        stepMs: round(median(steps.map((e) => e.duration))),
        stepSlowestMs: round(Math.max(...steps.map((e) => e.duration))),
        wasmMs: round(median(steps.map((e) => e.detail?.wasm ?? NaN))),
        bytes: median(steps.map((e) => e.detail?.bytes ?? NaN)),
        bytesMost: Math.max(...steps.map((e) => e.detail?.bytes ?? 0)),
        subsMs: round(median(subs.map((e) => e.duration))),
        subsSlowestMs: round(Math.max(0, ...subs.map((e) => e.duration))),
        subsPatches: Math.max(0, ...subs.map((e) => e.detail?.patches ?? 0)),
      });
    }
  } finally {
    await app.page.throttle(1);
  }
  console.log(JSON.stringify(rows, null, 1));
  const out = process.env["SUZATARY_STEPBENCH"];
  if (out !== undefined && out !== "") writeFileSync(out, JSON.stringify({ rate: RATE, runs: RUNS, rows }, null, 1));
});
