/**
 * SETTINGS, DRAWN BY ITS RUST PROGRAM (docs/FRAMEWORK-PLAN-2026-11-02.md §6): the read-only fixture at phone width.
 *
 * - The first draw: a fresh document at `#/settings` until the first line of the overview stands in it, read from a
 *   mark an observer sets. The HTTP cache is warm after the first visit, as on a phone that has opened the app before;
 *   the first visit is reported apart, as the cold one.
 * - A keystroke: a browser whose key nobody trusted opens the pairing field and types into it; each keystroke's time
 *   is the input event's synchronous handling, and each `painter:step`'s bytes are what crossed back.
 *
 * Run alone: `SUZATARY_E2E_BROWSER_CPU=0.67 scripts/web-e2e.sh settingsbench`; `$SUZATARY_SETTINGSBENCH` names a file
 * for the numbers as JSON. Its only assertion is that the page drew.
 */

import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";

import { BASE, PHONE, suite, WRITE_BASE } from "./harness";

const app = await suite("settingsbench", PHONE);
const RUNS = Number(process.env["SUZATARY_SETTINGSBENCH_RUNS"] ?? "9");
const KEYS = 24;

// When a line of the overview first stands in the page, marked by an observer every document starts with.
const LINES = `new MutationObserver((_, o) => { if (document.querySelector(".settingsline") !== null) { performance.mark("settings:lines"); o.disconnect(); } }).observe(document, { subtree: true, childList: true })`;
const drawn = `(performance.getEntriesByName("settings:lines")[0]?.startTime ?? null)`;

// Where the time went: when the program started (`page:draw` of `SettingsPage`: the module loaded, its chrome
// painted), the module's load (`views:load`: the glue and the wasm fetched, compiled, instantiated and rehearsed) and the
// first paint.
type Parts = { readonly frame: number; readonly load: number; readonly paint: number };
const parts = `(() => {
  const at = (name, f) => performance.getEntriesByName(name).find(f);
  const frame = at("page:draw", (e) => e.detail?.page === "SettingsPage");
  const load = at("views:load", () => true);
  const paint = at("painter:step", (e) => e.detail?.step === "first");
  return { frame: frame === undefined ? NaN : frame.startTime + frame.duration, load: load?.duration ?? NaN, paint: paint?.duration ?? NaN };
})()`;

const firstDraw = async (): Promise<readonly [number, Parts]> => {
  await app.page.navigate(`${BASE}/manifest.webmanifest`);
  await app.page.navigate(`${BASE}/#/settings`);
  await app.page.waitFor(`${drawn} !== null`, { timeout: 60000 });
  return [await app.page.evaluate<number>(drawn), await app.page.evaluate<Parts>(parts)];
};

const median = (xs: readonly number[]): number => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)] ?? NaN;
const spread = (xs: readonly number[]): readonly [number, number] => [Math.min(...xs), Math.max(...xs)];

await app.page.beforeEveryDocument(LINES);

const results: Record<string, unknown> = { cpu: process.env["SUZATARY_E2E_BROWSER_CPU"] ?? "all", width: PHONE[0] };

app.scene("the first draw, cold and warm", async () => {
  const runs: number[] = [];
  const split: Parts[] = [];
  // The first visit is the cold one.
  for (let i = 0; i < RUNS + 1; i++) {
    const [ms, p] = await firstDraw();
    runs.push(ms);
    split.push(p);
  }
  const [coldSplit, ...warmSplit] = split;
  const of = (k: keyof Parts) => median(warmSplit.map((p) => p[k]));
  results["parts"] = { cold: coldSplit, warm: { frame: of("frame"), load: of("load"), paint: of("paint") } };
  console.log(`program started at ${Math.round(of("frame"))} ms, module load ${Math.round(of("load"))} ms, first paint ${of("paint").toFixed(1)} ms (warm medians); cold ${JSON.stringify(coldSplit)}`);
  const [cold, ...warm] = runs;
  results["firstDraw"] = { cold, median: median(warm), range: spread(warm), runs: warm };
  console.log(`first draw cold ${Math.round(cold ?? NaN)} ms, warm median ${Math.round(median(warm))} ms (${spread(warm).map(Math.round).join(" to ")}) over ${warm.length}`);
  assert.ok(warm.length === RUNS);
});

// A key nobody trusted in this browser's place, so Settings offers the pairing field (pairing.test.ts's way).
const untrusted = (): Promise<unknown> =>
  app.page.navigate(`${WRITE_BASE}/manifest.webmanifest`).then(() =>
    app.page.evaluate(`(async () => {
      const request = (r) => new Promise((ok, no) => { r.onsuccess = () => ok(r.result); r.onerror = () => no(r.error); });
      const opening = indexedDB.open("suzatary-keys", 1);
      opening.onupgradeneeded = () => opening.result.createObjectStore("keys");
      const db = await request(opening);
      const pair = await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]);
      const raw = new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey));
      const key = { public: Array.from(raw, (b) => b.toString(16).padStart(2, "0")).join(""), key: pair.privateKey };
      await request(db.transaction("keys", "readwrite").objectStore("keys").put(key, "device"));
      db.close();
      return key.public;
    })()`),
  );

app.scene("a keystroke in the pairing field", async () => {
  await untrusted();
  await app.page.navigate(`${WRITE_BASE}/#/settings`);
  await app.page.waitFor(`document.querySelector(".settingshere .act-pair") !== null`, { timeout: 60000 });
  await app.page.click(".settingshere .act-pair");
  await app.page.waitFor(`document.querySelector(".settingshere .paircode") !== null`);
  const typed = await app.page.evaluate<{ ms: number[]; bytes: number[]; steps: number[]; disabled: boolean }>(`(() => {
    const f = document.querySelector(".settingshere .paircode");
    const since = performance.now();
    const ms = [];
    for (let i = 0; i < ${KEYS}; i++) {
      const t0 = performance.now();
      f.value = f.value + "B";
      f.dispatchEvent(new Event("input", { bubbles: true }));
      ms.push(performance.now() - t0);
    }
    const steps = performance.getEntriesByName("painter:step").filter((e) => e.startTime >= since && e.detail?.step === "event");
    return { ms, bytes: steps.map((e) => e.detail.bytes), steps: steps.map((e) => e.duration), disabled: document.querySelector(".settingshere .act-paircode").disabled };
  })()`);
  assert.equal(typed.disabled, false, "the code's button is enabled once a code is typed");
  results["keystroke"] = { median: median(typed.ms), range: spread(typed.ms), patchBytes: { median: median(typed.bytes), range: spread(typed.bytes) } };
  console.log(`keystroke median ${median(typed.ms).toFixed(3)} ms, patch ${median(typed.bytes)} bytes (${spread(typed.bytes).join(" to ")})`);
  const out = process.env["SUZATARY_SETTINGSBENCH"];
  if (out !== undefined) writeFileSync(out, JSON.stringify(results, null, 2));
});
