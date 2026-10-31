/**
 * THE LINES AT A PHONE'S SIZE, MEASURED: the `lines` server's store (`fixture-chain --lines`, about 6,000 with
 * `SUZATARY_E2E_LINES=6000`), read at CPU throttling 6, its main thread profiled through three passes: a first load on a new device until the prefetch holds
 * every line, a landing of one new line, and a load whose budget holds half the store, so its landing evicts. Each
 * pass says its longest busy run of the main thread and the functions of this app that ran in it, by the time each
 * was on the stack, read through the bundle's source maps. The numbers go to `$SUZATARY_LINESBENCH` when it is set;
 * the scene asserts only that each pass ended. No gate runs it: `scripts/web-e2e.sh linesbench` does, by name.
 */

import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";

import type { CpuProfile } from "./cdp";
import { PHONE, SIGNED_POST, suite, trustThisDevice } from "./harness";

const BULK = process.env["E2E_BASE_LINES"] ?? "";
const app = await suite("linesbench", PHONE);
const RATE = 6;

// --- the source maps -----------------------------------------------------------

const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const vlq = (segment: string): number[] => {
  const out: number[] = [];
  let value = 0;
  let shift = 0;
  for (const c of segment) {
    const digit = B64.indexOf(c);
    value += (digit & 31) << shift;
    if (digit & 32) {
      shift += 5;
      continue;
    }
    out.push(value & 1 ? -(value >> 1) : value >> 1);
    value = 0;
    shift = 0;
  }
  return out;
};

type Lookup = (line: number, column: number) => string | null;

const lookupOf = (map: { readonly sources: readonly string[]; readonly mappings: string }): Lookup => {
  const lines: (readonly [number, number, number])[][] = [];
  let source = 0;
  let sourceLine = 0;
  for (const text of map.mappings.split(";")) {
    let column = 0;
    const segments: (readonly [number, number, number])[] = [];
    for (const segment of text.split(",")) {
      if (segment === "") continue;
      const v = vlq(segment);
      column += v[0] ?? 0;
      if (v.length > 1) {
        source += v[1] ?? 0;
        sourceLine += v[2] ?? 0;
      }
      segments.push([column, source, sourceLine]);
    }
    lines.push(segments);
  }
  return (line, column) => {
    let found: readonly [number, number, number] | undefined;
    for (const s of lines[line] ?? []) {
      if (s[0] > column) break;
      found = s;
    }
    return found === undefined ? null : `${(map.sources[found[1]] ?? "?").replace(/^(\.\.\/)+web\//, "")}:${found[2] + 1}`;
  };
};

const maps = new Map<string, Promise<Lookup | null>>();
const mapped = (url: string): Promise<Lookup | null> => {
  const known = maps.get(url);
  if (known !== undefined) return known;
  const read = fetch(`${url}.map`).then(
    (r) => (r.ok ? r.json().then((m: { sources: string[]; mappings: string }) => lookupOf(m)) : null),
    () => null,
  );
  maps.set(url, read);
  return read;
};

// --- the profile ---------------------------------------------------------------

type Busy = { readonly ms: number; readonly by: readonly (readonly [string, number])[] };
// Each function of this app: the most of one busy run it was on the stack for, and its whole time on the stack.
type Spent = readonly (readonly [string, number, number])[];
type Pass = { readonly wallMs: number; readonly busyMs: number; readonly longest: Busy; readonly next: readonly number[]; readonly most: Spent };

// `(program)` is the browser's own time between tasks, a throttled thread's pauses among it.
const IDLE = new Set(["(idle)", "(program)"]);

// The longest run of samples with no idle one between them, and what of this app was on the stack in it.
const read = async (profile: CpuProfile, wallMs: number): Promise<Pass> => {
  const byId = new Map(profile.nodes.map((n) => [n.id, n]));
  const parent = new Map<number, number>();
  for (const n of profile.nodes) for (const c of n.children ?? []) parent.set(c, n.id);
  const ours = async (id: number): Promise<string[]> => {
    const out: string[] = [];
    for (let at: number | undefined = id; at !== undefined; at = parent.get(at)) {
      const f = byId.get(at)?.callFrame;
      if (f === undefined) continue;
      if (f.url.startsWith("wasm://")) {
        out.push("core (wasm)");
        continue;
      }
      if (!f.url.startsWith("http")) continue;
      const lookup = await mapped(f.url);
      const where = lookup?.(f.lineNumber, f.columnNumber) ?? null;
      if (where !== null && !where.includes("node_modules")) out.push(`${where} ${f.functionName}`.trim());
    }
    return [...new Set(out)];
  };
  const runs: { ms: number; samples: number[] }[] = [];
  let run: { ms: number; samples: number[] } | null = null;
  let busyMs = 0;
  profile.samples.forEach((id, i) => {
    const ms = (profile.timeDeltas[i + 1] ?? 0) / 1000;
    if (IDLE.has(byId.get(id)?.callFrame.functionName ?? "")) {
      run = null;
      return;
    }
    busyMs += ms;
    if (run === null) runs.push((run = { ms: 0, samples: [] }));
    run.ms += ms;
    run.samples.push(i);
  });
  runs.sort((a, b) => b.ms - a.ms);
  const top = runs[0] ?? { ms: 0, samples: [] };
  const on = new Map<string, number>();
  for (const i of top.samples) {
    const ms = (profile.timeDeltas[i + 1] ?? 0) / 1000;
    for (const f of await ours(profile.samples[i] ?? 0)) on.set(f, (on.get(f) ?? 0) + ms);
  }
  const by = [...on].sort((a, b) => b[1] - a[1]).slice(0, 8).map(([f, ms]): readonly [string, number] => [f, Math.round(ms)]);
  const inRun = new Map<string, number>();
  const total = new Map<string, number>();
  for (const r of runs) {
    const here = new Map<string, number>();
    for (const i of r.samples) {
      const ms = (profile.timeDeltas[i + 1] ?? 0) / 1000;
      for (const f of await ours(profile.samples[i] ?? 0)) here.set(f, (here.get(f) ?? 0) + ms);
    }
    for (const [f, ms] of here) {
      inRun.set(f, Math.max(inRun.get(f) ?? 0, ms));
      total.set(f, (total.get(f) ?? 0) + ms);
    }
  }
  const most = [...inRun]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 25)
    .map(([f, ms]): readonly [string, number, number] => [f, Math.round(ms), Math.round(total.get(f) ?? 0)]);
  return { wallMs: Math.round(wallMs), busyMs: Math.round(busyMs), longest: { ms: Math.round(top.ms), by }, next: runs.slice(1, 6).map((r) => Math.round(r.ms)), most };
};

// --- what the device holds -----------------------------------------------------

const ROWS = `(store) => new Promise((ok, no) => {
  const r = indexedDB.open("suzatary")
  r.onerror = () => no(String(r.error))
  r.onsuccess = () => {
    const db = r.result
    if (!db.objectStoreNames.contains(store)) return (db.close(), ok([]))
    const tx = db.transaction(store, "readonly")
    const keys = tx.objectStore(store).getAllKeys()
    const values = tx.objectStore(store).getAll()
    values.onsuccess = () => (db.close(), ok(keys.result.map((k, i) => [k, values.result[i]])))
  }
})`;

// The lines the skeleton row names, their bytes, and how many of them the content store holds.
type Held = { readonly lines: number; readonly bytes: number; readonly held: number; readonly conversations: number };
const HELD = `Promise.all([(${ROWS})("chain"), (${ROWS})("content")]).then(([chain, content]) => {
  const row = chain.find(([k]) => k === "objects:store/lines")?.[1]
  const nodes = row === undefined ? [] : row.replica.objects.filter((n) => n.header.node === "line")
  const held = new Set(content.map(([k]) => k))
  return { lines: nodes.length, bytes: nodes.reduce((k, n) => k + n.size, 0), held: nodes.filter((n) => held.has(n.hash)).length, conversations: new Set(nodes.map((n) => n.header.conversation)).size }
})`;

const held = (): Promise<Held> => app.page.evaluate<Held>(HELD);

const measured = async (what: () => Promise<void>, until: string, timeout: number): Promise<Pass> => {
  console.log(`linesbench: a pass starts at ${new Date().toISOString()}`);
  const stop = await app.page.profile(1000);
  const started = Date.now();
  await what();
  await app.page.waitFor(until, { timeout }).catch(async (e: unknown) => {
    throw new Error(`${String(e)}; the device holds ${JSON.stringify(await held().catch((x: unknown) => String(x)))}`);
  });
  // A second with nothing asked, so work a landing set going is in the profile too.
  await new Promise((done) => setTimeout(done, 1000));
  const profile = await stop();
  const out = process.env["SUZATARY_LINESBENCH"];
  if (out !== undefined && out !== "") writeFileSync(`${out}.${(passes += 1)}.cpuprofile`, JSON.stringify(profile));
  return read(profile, Date.now() - started);
};
let passes = 0;

app.scene("measured: a first load, a landing and an eviction over about 6,000 lines at throttling 6", async () => {
  assert.notEqual(BULK, "", "E2E_BASE_LINES is unset — scripts/web-e2e.sh sets it");
  const origin = new URL(BULK).origin;
  // The landing's line is a conversation this page opens, so the page signs as a trusted device.
  await trustThisDevice(app.page, BULK, process.env["E2E_LINES_INSTANCE"]);
  // The core's worker pulls, fetches and evicts the lines, so it is cold as the page is (its CPU is the browser's, `SUZATARY_E2E_BROWSER_CPU`).
  await app.page.attachWorkers();
  await app.open(BULK);
  await app.page.evaluate(`Promise.all(["suzatary", "suzatary-rendered"].map((n) => new Promise((ok) => { const r = indexedDB.deleteDatabase(n); r.onsuccess = r.onerror = r.onblocked = () => ok(true) })))`);
  await app.page.navigate("about:blank");
  await app.page.cold(true);
  await app.page.throttle(RATE);
  // The fixture's second conversation, a quarter of the store, written after the first, which holds half of it.
  const newest = "bulk-1";
  const whole = `(${HELD}).then((h) => h.lines > 0 && h.held === h.lines)`;
  const first = await measured(() => app.page.navigate(`${BULK}/#/secretary/${newest}`), whole, 30 * 60_000);
  const store = await held();

  const landing = await measured(
    async () =>
      assert.ok(await app.page.evaluate<boolean>(`${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({ title: "A new one" })).then((r) => r.ok)`), "the box opened a conversation"),
    `(${HELD}).then((h) => h.conversations > ${store.conversations} && h.held === h.lines)`,
    10 * 60_000,
  );

  // A budget of half the store, a tenth of the quota, unless the origin already uses more than that quota: the browser
  // refuses a write past it.
  const used = await app.page.evaluate<number>(`navigator.storage.estimate().then((e) => e.usage ?? 0)`);
  const quota = Math.max(store.bytes * 5, Math.ceil(used * 1.5));
  await app.page.quota(origin, quota);
  await app.page.navigate("about:blank");
  // Two minutes of this load, and the fewest lines the device held in them: its first landing evicts before a reading
  // of the whole store pins every line again.
  const evicted = await measured(
    () => app.page.navigate(`${BULK}/?evict=1#/secretary/${newest}`),
    `(${HELD}).then((h) => ((window.e2eSince ??= Date.now()), (window.e2eLeast = Math.min(window.e2eLeast ?? h.lines, h.held)), Date.now() - window.e2eSince > 120000))`,
    10 * 60_000,
  );
  const after = { ...(await held()), fewestHeld: await app.page.evaluate<number>(`window.e2eLeast ?? -1`) };
  await app.page.quota(origin, null);
  await app.page.throttle(1);
  await app.page.cold(false);

  const report = { rate: RATE, store, used, quota, budget: Math.min(256 * 1024 * 1024, Math.floor(quota / 10)), afterEviction: after, first, landing, evicted };
  console.log(JSON.stringify(report, null, 2));
  const out = process.env["SUZATARY_LINESBENCH"];
  if (out !== undefined && out !== "") writeFileSync(out, JSON.stringify(report, null, 2));
});
