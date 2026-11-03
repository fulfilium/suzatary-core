/**
 * SETTINGS, TODAY'S PAGE AGAINST ITS RUST PROGRAM (docs/FRAMEWORK-PLAN-2026-11-02.md §6): the same fixture, the same
 * phone width, the two shapes alternating so a slow minute falls on both.
 *
 * - The first draw: a fresh document at `#/settings` until the draw that first shows the lines ended, read from the
 *   page's own measures (`page:draw` of `settingspage` with a value today; of `settingsview`, the program's first
 *   paint, behind the flag). The HTTP cache is warm after the first visit of each shape, as on a phone that has
 *   opened the app before; the first visit of each is reported apart, as the cold one.
 * - A keystroke: a browser whose key nobody trusted opens the pairing field and types into it; each keystroke's time
 *   is the input event's synchronous handling, and behind the flag each `painter:step`'s bytes are what crossed back.
 *
 * Run alone: `SUZATARY_E2E_BROWSER_CPU=0.67 scripts/web-e2e.sh settingsbench`; `$SUZATARY_SETTINGSBENCH` names a file
 * for the numbers as JSON. Its only assertion is that each shape drew.
 */

import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";

import { BASE, PHONE, suite, WRITE_BASE } from "./harness";

const app = await suite("settingsbench", PHONE);
const RUNS = Number(process.env["SUZATARY_SETTINGSBENCH_RUNS"] ?? "9");
const KEYS = 24;

type Shape = "today" | "rust";
const SHAPES: readonly Shape[] = ["today", "rust"];

const flag = (base: string, s: Shape): Promise<unknown> =>
  app.page.navigate(`${base}/manifest.webmanifest`).then(() =>
    app.page.evaluate(s === "rust" ? `localStorage.setItem("suzatary.viewtree", "on")` : `localStorage.removeItem("suzatary.viewtree")`),
  );

const drawn = (s: Shape): string =>
  `(() => { const d = performance.getEntriesByName("page:draw").find((e) => e.detail?.page === ${JSON.stringify(s === "rust" ? "settingsview" : "settingspage")} && e.detail?.read === "Val" && document.querySelector(".settingsline") !== null); return d === undefined ? null : d.startTime + d.duration })()`;

// Behind the flag, where the time went: the frame's first draw of the value (its column, the island empty), the
// module's load (`views:load`: the glue and the wasm fetched, compiled and instantiated) and the program's first paint.
type Parts = { readonly frame: number; readonly load: number; readonly paint: number };
const parts = `(() => {
  const at = (name, f) => performance.getEntriesByName(name).find(f);
  const frame = at("page:draw", (e) => e.detail?.page === "settingspage" && e.detail?.read === "Val");
  const load = at("views:load", () => true);
  const paint = at("painter:step", (e) => e.detail?.step === "first");
  return { frame: frame === undefined ? NaN : frame.startTime + frame.duration, load: load?.duration ?? NaN, paint: paint?.duration ?? NaN };
})()`;

const firstDraw = async (s: Shape): Promise<readonly [number, Parts | null]> => {
  await flag(BASE, s);
  await app.page.navigate(`${BASE}/#/settings`);
  await app.page.waitFor(`${drawn(s)} !== null`, { timeout: 60000 });
  return [await app.page.evaluate<number>(drawn(s)), s === "rust" ? await app.page.evaluate<Parts>(parts) : null];
};

const median = (xs: readonly number[]): number => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)] ?? NaN;
const spread = (xs: readonly number[]): readonly [number, number] => [Math.min(...xs), Math.max(...xs)];

const results: Record<string, unknown> = { cpu: process.env["SUZATARY_E2E_BROWSER_CPU"] ?? "all", width: PHONE[0] };

app.scene("the first draw of each shape, alternating", async () => {
  const runs: Record<Shape, number[]> = { today: [], rust: [] };
  const split: Parts[] = [];
  // The first visit of each is its cold one; the order flips each run so neither shape always goes first.
  for (let i = 0; i < RUNS + 1; i++)
    for (const s of i % 2 === 0 ? SHAPES : [...SHAPES].reverse()) {
      const [ms, p] = await firstDraw(s);
      runs[s].push(ms);
      if (p !== null) split.push(p);
    }
  const [coldSplit, ...warmSplit] = split;
  const of = (k: keyof Parts) => median(warmSplit.map((p) => p[k]));
  results["rust.parts"] = { cold: coldSplit, warm: { frame: of("frame"), load: of("load"), paint: of("paint") } };
  console.log(`rust: frame drawn at ${Math.round(of("frame"))} ms, module load ${Math.round(of("load"))} ms, first paint ${of("paint").toFixed(1)} ms (warm medians); cold ${JSON.stringify(coldSplit)}`);
  for (const s of SHAPES) {
    const [cold, ...warm] = runs[s];
    results[`${s}.firstDraw`] = { cold, median: median(warm), range: spread(warm), runs: warm };
    console.log(`${s}: first draw cold ${Math.round(cold ?? NaN)} ms, warm median ${Math.round(median(warm))} ms (${spread(warm).map(Math.round).join(" to ")}) over ${warm.length}`);
    assert.ok(warm.length === RUNS);
  }
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

app.scene("a keystroke in the pairing field, in each shape", async () => {
  await untrusted();
  for (const s of SHAPES) {
    await flag(WRITE_BASE, s);
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
    assert.equal(typed.disabled, false, `${s}: the code's button is enabled once a code is typed`);
    results[`${s}.keystroke`] = { median: median(typed.ms), range: spread(typed.ms), patchBytes: typed.bytes.length === 0 ? null : { median: median(typed.bytes), range: spread(typed.bytes) } };
    console.log(`${s}: keystroke median ${median(typed.ms).toFixed(3)} ms${typed.bytes.length === 0 ? "" : `, patch ${median(typed.bytes)} bytes (${spread(typed.bytes).join(" to ")})`}`);
  }
  const out = process.env["SUZATARY_SETTINGSBENCH"];
  if (out !== undefined) writeFileSync(out, JSON.stringify(results, null, 2));
});
