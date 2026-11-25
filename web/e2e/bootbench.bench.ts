/**
 * Measures a first visit (no worker, no HTTP cache) and a start under the worker, at CPU throttling 4 over DevTools'
 * Fast 4G, the median of several runs: the first API request, app.js's arrival, the app's start and the list's first
 * row, from the navigation's start. Its only assertion is that every start drew, and no gate runs it:
 * `scripts/web-e2e.sh bootbench` does, by name.
 */

import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";

import { PHONE, suite } from "./harness";

const SHELL = process.env["E2E_BASE_SHELL"] ?? "";
const app = await suite("bootbench", PHONE);

const RATE = 4;
const RUNS = Number(process.env["SUZATARY_BOOTBENCH_RUNS"] ?? "3");
// DevTools' Fast 4G as its network panel applies it: 9 Mbps down, 1.5 up, 60 ms scaled by its 2.75 for the radio.
const FAST_4G = { offline: false, latency: 60 * 2.75, downloadThroughput: (9_000_000 / 8) * 0.9, uploadThroughput: (1_500_000 / 8) * 0.9 };

// Written into every document before its scripts: the instants the app said it started and the list drew a row.
const WATCH = `(() => {
  window.e2eBoot = { started: null, row: null };
  new MutationObserver(() => {
    const b = window.e2eBoot;
    if (b.started === null && document.documentElement.dataset.started === "app") b.started = performance.now();
    if (b.row === null && document.querySelector(".tree .row") !== null) b.row = performance.now();
  }).observe(document, { subtree: true, childList: true, attributes: true });
})()`;

type Start = { readonly api: number; readonly app: number; readonly started: number; readonly row: number };

const START = `(() => {
  const rs = performance.getEntriesByType("resource");
  const api = rs.filter((e) => new URL(e.name).pathname.startsWith("/api/")).map((e) => e.startTime);
  const app = rs.filter((e) => new URL(e.name).pathname.endsWith("/app.js")).map((e) => e.responseEnd);
  return { api: Math.min(...api), app: Math.min(...app), started: window.e2eBoot.started, row: window.e2eBoot.row };
})()`;

const once = async (): Promise<Start> => {
  await app.page.navigate("about:blank");
  await app.page.navigate(`${SHELL}/#/`);
  await app.page.waitFor(`window.e2eBoot !== undefined && window.e2eBoot.row !== null`, { timeout: 120000 });
  return app.page.evaluate<Start>(START);
};

const median = (xs: readonly number[]): number => {
  const s = [...xs].sort((a, b) => a - b);
  return Math.round(s[Math.floor((s.length - 1) / 2)] ?? NaN);
};
const summary = (runs: readonly Start[]) => ({
  firstApi: median(runs.map((r) => r.api)),
  appArrived: median(runs.map((r) => r.app)),
  appStarted: median(runs.map((r) => r.started)),
  firstRow: median(runs.map((r) => r.row)),
});

app.scene("measured: a first visit and a start under the worker, at throttling 4 over Fast 4G", async () => {
  assert.notEqual(SHELL, "", "E2E_BASE_SHELL is unset — scripts/web-e2e.sh sets it");
  await app.page.navigate(`${SHELL}/#/`);
  await app.page.waitFor(`navigator.serviceWorker.controller !== null && document.querySelectorAll(".tree .row").length > 0`, { timeout: 60000 });
  await app.page.beforeEveryDocument(WATCH);
  await app.page.throttle(RATE);
  await app.page.network(FAST_4G);
  const cold: Start[] = [];
  const warm: Start[] = [];
  try {
    await app.page.cold(true);
    for (let k = 0; k < RUNS; k++) cold.push(await once());
    await app.page.cold(false);
    // The first start under the worker hears the build behind its answer; the runs measured are the ones after it.
    await once();
    for (let k = 0; k < RUNS; k++) warm.push(await once());
  } finally {
    await app.page.cold(false);
    await app.page.throttle(1);
    await app.page.network(null);
  }
  for (const r of [...cold, ...warm]) assert.ok(Number.isFinite(r.row), `a start drew: ${JSON.stringify(r)}`);
  const result = { rate: RATE, network: "Fast 4G", runs: RUNS, cold: summary(cold), warm: summary(warm), all: { cold, warm } };
  console.log(JSON.stringify(result, null, 1));
  const out = process.env["SUZATARY_BOOTBENCH"];
  if (out !== undefined && out !== "") writeFileSync(out, JSON.stringify(result, null, 1));
});
