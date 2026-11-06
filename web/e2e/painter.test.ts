/**
 * THE PAINTER'S LAWS, IN A BROWSER: `web/src/painter/painter.ts` and `run.ts` drive Settings' program from the module the
 * box serves, with a pure handler in place of the runtime's, over generated inputs (the overview, this device and what
 * waits held, the context), taps, typing and answers. After every step the painted DOM equals what a fresh
 * painter draws of the program's current view, node by node; a second run of the same steps paints the same DOM and
 * asks the same effects; and an input handed twice patches nothing the second time. `app/viewtree/tests/laws.rs` holds the same laws over generated views and a DOM kept as data.
 */

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";

import { BASE, PHONE, suite } from "./harness";
import { GROUPS, overviewLine, reshuffled, rng } from "../test/gen";
import type { Rng } from "../test/gen";
import type { Context, Device, SettingsFlags, SettingsHold } from "../src/wire/generated/pages";
import type { Offer, Overview, OverviewLine } from "../src/wire/generated/wire";
import type painterlaws from "./painterlaws";

const app = await suite("painter", PHONE);

// The laws' code bundled for the page and evaluated there, as `typstdraw.ts` does its bench.
const LAWS = execFileSync(
  "esbuild",
  ["web/e2e/painterlaws.ts", "--bundle", "--format=iife", "--global-name=painterlaws", "--target=es2022", "--log-level=error"],
  { encoding: "utf8" },
);

const install = `(async () => {
  window.painterlaws ??= (() => { ${LAWS}; return painterlaws.default; })();
  if (window.__views === undefined) {
    const g = await import(new URL("./b/dev/views.js", location.href).href);
    await g.default({ module_or_path: new URL("./b/dev/views_bg.wasm", location.href).href });
    window.__views = g;
  }
  return true;
})()`;

const RUNS = 40;
const STEPS = 30;

// A pool of lines per group, so successive readings keep, move, drop and bring back the same keyed rows.
const pool = (r: Rng): ReadonlyMap<string, readonly OverviewLine[]> =>
  new Map(GROUPS.map((g) => [g, r.bool(0.5) ? [] : Array.from({ length: 1 + r.int(4) }, (_, i) => ({ ...overviewLine(r), key: `${g}:${i}` }))]));

const device = (r: Rng): Device => ({
  persist: r.pick([{ type: "Unsupported" }, { type: "Granted" }, { type: "Denied" }] as const),
  usage: { bytes: r.int(10_000_000), count: r.int(3) },
  budget: 50_000_000,
  signing: r.pick([
    { type: "Unheard" },
    { type: "Reads" },
    { type: "Unsupported", e: "no keys here" },
    { type: "Unreadable", e: "it is not a key this browser made" },
    { type: "Keyed", key: "ab".repeat(32), standing: r.pick(["trusted", "untrusted", "revoked"] as const) },
  ] as const),
});

const offer = (r: Rng): Offer => ({ template: r.pick(["google", "imap", "anthropic"]), group: r.pick(GROUPS), title: r.pick(["a mail account", "a model"]) });

const overview = (r: Rng, lines: ReadonlyMap<string, readonly OverviewLine[]>): Overview => ({
  groups: GROUPS.map((group) => {
    const all = lines.get(group) ?? [];
    const order = reshuffled(r, all.map((l) => l.key)).slice(0, r.int(all.length + 1));
    return { group, lines: order.flatMap((k) => all.filter((l) => l.key === k)) };
  }),
  ask: "setup",
  offers: r.array(r.int(4), () => offer(r)),
});

const context = (r: Rng): Context => {
  const gate = r.pick(["writes", "reads", "views"] as const);
  return {
    wide: r.bool(),
    categories: ["home", "work"],
    gate,
    reach: gate === "writes" ? { type: "Writes" } : { type: "Reads", said: "This device reads only.", settings: r.bool(), key: r.pick(["untrusted", "revoked", "not_the_key"] as const) },
    offsets: [{ from: 0, minutes: r.pick([0, 60, -300]) }],
  };
};

// What the tab's register of a question may hold, as the host hands it over.
const held = (r: Rng, ask: SettingsHold, body: unknown): unknown => ({
  type: "Held",
  ask,
  reading: r.pick([{ type: "Loading" }, { type: "Error", e: "no answer" }, { type: "Val", v: { type: "Done", body } }, { type: "Val", v: { type: "Done", body } }]),
  behind: r.bool(0.2) ? { why: r.bool() ? "offline" : null, since: 1_780_000_000_000 } : null,
});

const input = (r: Rng, lines: ReadonlyMap<string, readonly OverviewLine[]>): unknown => {
  const o = overview(r, lines);
  return r.pick([
    () => held(r, "Overview", o),
    () => held(r, "Device", r.bool(0.8) ? device(r) : null),
    () => held(r, { Rows: { type: "Proposals" } }, { payload: { at: "2026-10-05T04:00:00", proposals: [], failed: [] }, held: [] }),
    () => ({ type: "Context", ...context(r) }),
  ])();
};

const steps = (r: Rng, lines: ReadonlyMap<string, readonly OverviewLine[]>): painterlaws.Step[] =>
  Array.from({ length: STEPS }, (): painterlaws.Step => {
    const k = r.int(10);
    return k < 4 ? { type: "input", input: input(r, lines) }
      : k < 6 ? { type: "click", at: r.int(8) }
      : k < 8 ? { type: "type", at: r.int(2), words: r.string("BCDF-GHJK ", r.int(10)) }
      : { type: "answer", at: r.int(3), body: { id: `setup-${r.int(3)}` }, refused: r.bool() ? null : "The box refused it." };
  });

app.scene("a patch applied equals a fresh draw, and the same steps paint the same DOM, for Settings' program", async () => {
  await app.page.navigate(`${BASE}/#/about`);
  await app.page.evaluate(install);
  let nodes = 0;
  for (let seed = 1; seed <= RUNS; seed++) {
    const r = rng(seed);
    const lines = pool(r);
    const flags: SettingsFlags = { context: context(r) };
    // The overview first, so most runs draw the page's body.
    const first: painterlaws.Step = { type: "input", input: { type: "Held", ask: "Overview", reading: { type: "Val", v: { type: "Done", body: overview(r, lines) } }, behind: null } };
    const report = await app.page.evaluate<painterlaws.Report>(
      `window.painterlaws.check(() => new window.__views.SettingsPage(${JSON.stringify(JSON.stringify(flags))}), ${JSON.stringify([first, ...steps(r, lines)])})`,
    );
    if (report.type === "Broke") assert.fail(`seed ${seed}, step ${report.step}: ${report.law} fails at ${report.at || "the root"}: painted ${report.painted}, expected ${report.expected}`);
    nodes = Math.max(nodes, report.nodes);
  }
  assert.ok(nodes > 20, `the readings drew pages of some size (${nodes} nodes at most)`);
});
