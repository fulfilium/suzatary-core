/**
 * What every browser test needs: a browser, the fixture servers' addresses, and a screenshot when a scene fails.
 * One browser per file: node's test runner gives each file its own process, so each file has its own profile and its
 * own `localStorage`, which the local-first tests depend on.
 */

import assert from "node:assert/strict";
import { after } from "node:test";
import { test } from "node:test";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { execFile, execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { basename, join, normalize } from "node:path";

import type { Browser, Conditions, Journal, Page } from "./cdp";
import { WAIT_MS, launch } from "./cdp";
import type { Fulfillment } from "../src/wire/generated/wire";
import http from "../src/wire/http";
import shell from "../src/app/shell";

// The Typst module's name in the build under test, which names it by its bytes (`shell.BY_CONTENT`).
export const TYPST_MODULE: string = (() => {
  const named = readdirSync("web/dist").find((f) => shell.byContent(f)?.plain === "typst_bg.wasm");
  if (named === undefined) throw new Error("web/dist holds no Typst module named by its bytes — run scripts/build-web.sh");
  return named;
})();

function required(name: string): string {
  const value = process.env[name];
  if (value === undefined || value === "") {
    throw new Error(`${name} is unset — these tests are run by scripts/web-e2e.sh, which starts the servers`);
  }
  return value;
}

/** The read side, gated to a viewer who is not an actor, so its chain cannot change. */
export const BASE = required("E2E_BASE");

/** The same viewer over a deployment with a triage card waiting (§10.6), since `BASE`'s empty Inbox is a claim too. */
export const PROPOSED_BASE = required("E2E_BASE_PROPOSED");

/** The write side: its own chain, so a write in one file is never a missing card in another. */
export const WRITE_BASE = required("E2E_BASE_WRITE");

/**
 * The headers a write from a page carries, as the app's own door sends them, as a JavaScript object literal; a spec
 * spells no write header of its own (`web-static.sh`), so a contract the box raises reaches every scene at once.
 */
export const WRITE_HEADERS = JSON.stringify(http.actRequest(null).headers);

const SHOTS = process.env["SUZATARY_E2E_SHOTS"] ?? join(tmpdir(), "suzatary-e2e-shots");

/** Phone, under the 62rem breakpoint, so `#/todo/<id>` is a pushed screen. */
export const PHONE: readonly [number, number] = [390, 844];

/** Desktop, above 62rem: the same hash is the list with the detail pane. */
export const DESKTOP: readonly [number, number] = [1280, 800];

/** The audit's wide window: a desk's browser with room for the list, the todo and the secretary at once. */
export const WIDE: readonly [number, number] = [1440, 900];

/** Lighthouse's Fast 4G (165 ms a request, about 1.0 MB/s down and 170 KB/s up): what a phone on a tailnet relay gets
 * on a good day, applied to a page and its workers (`Page.network`). */
export const FAST_4G: Conditions = { offline: false, latency: 60 * 2.75, downloadThroughput: (9_000_000 / 8) * 0.9, uploadThroughput: (1_500_000 / 8) * 0.9 };

/** A middle width: a small laptop, where the list, the todo and an open panel are tightest. */
export const MIDDLE: readonly [number, number] = [1100, 800];

export interface Suite {
  readonly page: Page;
  /** Another tab of this suite's browser, at the suite's phone size. */
  tab: () => Promise<Page>;
  /** Register one test. The name is also the screenshot's, on failure; `probe`, a page expression, is what the
   * failure also writes down of the state the scene was about. */
  scene: (name: string, body: () => Promise<void>, probe?: string) => void;
  /** The page's journal from the first call on, for this file; a failure writes it beside the picture. */
  journal: () => Promise<Journal>;
  /** Load the app at a hash and wait until it has drawn cards. */
  open: (base: string, hash?: string) => Promise<void>;
  /** This browser's key trusted on the writer's server, once per suite: what `open` does first there. */
  trust: () => Promise<string>;
  /** Resize, and wait for the tiles to be arranged again for the room it leaves. */
  resize: (size: readonly [number, number]) => Promise<void>;
}

const slug = (name: string): string => name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");

/**
 * ⚠️ The server is up before it is serving: on a shared runner a page can reach the app before axum has answered
 * anything, so each file asks the tip and then the fold from node first, once per base. A fold fetched here is evidence
 * the box can answer, never that the app drew it.
 */
const answering = new Map<string, Promise<void>>();

async function reachable(url: string, until: number): Promise<void> {
  let last = "never asked";
  while (Date.now() < until) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        // An unread body holds the socket.
        await response.text();
        return;
      }
      last = `HTTP ${response.status}`;
    } catch (error: unknown) {
      last = error instanceof Error ? error.message : String(error);
    }
    await new Promise((done) => setTimeout(done, 100));
  }
  throw new Error(`${url} never answered within ${WAIT_MS}ms — last: ${last}`);
}

function serving(base: string): Promise<void> {
  const known = answering.get(base);
  if (known !== undefined) return known;
  const asked = (async (): Promise<void> => {
    const until = Date.now() + WAIT_MS;
    await reachable(`${base}/api/tip`, until);
    await reachable(`${base}/api/fold`, until);
  })();
  answering.set(base, asked);
  return asked;
}

/**
 * This browser's device key, trusted at the box's terminal (SPEC law 42): minted in the page as
 * `web/src/prodrome/signer.ts` mints it (non-extractable, the same database), its public half registered by
 * `suzatary device trust`.
 */
export async function trustThisDevice(page: Page, base: string, instance?: string): Promise<string> {
  // A file of the origin, not the app: the key is trusted before the app's first pull.
  await page.navigate(`${base}/manifest.webmanifest`);
  const key = await page.evaluate<string>(`(async () => {
    const request = (r) => new Promise((ok, no) => { r.onsuccess = () => ok(r.result); r.onerror = () => no(r.error); });
    const opening = indexedDB.open("suzatary-keys", 1);
    opening.onupgradeneeded = () => opening.result.createObjectStore("keys");
    const db = await request(opening);
    const held = await request(db.transaction("keys", "readonly").objectStore("keys").get("device"));
    if (held !== undefined) { db.close(); return held.public; }
    const pair = await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]);
    const raw = new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey));
    const key = { public: Array.from(raw, (b) => b.toString(16).padStart(2, "0")).join(""), key: pair.privateKey };
    await request(db.transaction("keys", "readwrite").objectStore("keys").put(key, "device"));
    db.close();
    return key.public;
  })()`);
  atTheTerminal(["device", "trust", key], instance);
  return key;
}

/**
 * A write as the app sends it (`web/src/wire/http.ts`), stamped, named by its content and signed by this browser's
 * device key: an expression of a function of the path and the body's text, answering the `fetch`'s promise.
 */
export const SIGNED_POST = `(async (path, body) => {
  const request = (r) => new Promise((ok, no) => { r.onsuccess = () => ok(r.result); r.onerror = () => no(r.error); });
  const opening = indexedDB.open("suzatary-keys", 1);
  opening.onupgradeneeded = () => opening.result.createObjectStore("keys");
  const db = await request(opening);
  const held = await request(db.transaction("keys", "readonly").objectStore("keys").get("device"));
  db.close();
  const hex = (bytes) => Array.from(new Uint8Array(bytes), (b) => b.toString(16).padStart(2, "0")).join("");
  const text = (t) => new TextEncoder().encode(t);
  const sha256 = async (t) => hex(await crypto.subtle.digest("SHA-256", text(t)));
  const at = new Date().toISOString();
  const nonce = hex(crypto.getRandomValues(new Uint8Array(16)).buffer);
  const signed = held === undefined ? {} : {
    "X-Suzatary-At": at,
    "X-Suzatary-Nonce": nonce,
    "X-Suzatary-Key": held.public,
    "X-Suzatary-Signature": hex(await crypto.subtle.sign({ name: "Ed25519" }, held.key, text("prodrome object " +
      await sha256("suzatary request\\nPOST " + path + "\\n" + at + "\\n" + nonce + "\\n" + await sha256(body) + "\\n" + held.public)))),
  };
  return fetch(path, { method: "POST", headers: { ...${WRITE_HEADERS}, ...signed }, body });
})`;

/**
 * A new conversation opened as a tab opens one, through the app's own core module and this device's key, since no
 * route opens a conversation; the expression answers the new conversation's id.
 */
export const opened = (title: string): string => `(async (title) => {
  // The build's files sit beside its app, which the page preloaded.
  const app = [...document.querySelectorAll("link[rel=modulepreload]")].map((l) => l.href).find((h) => h.endsWith("/app.js"));
  const beside = (file) => new URL(file, app ?? document.baseURI).href;
  const core = await import(beside("prodrome.js"));
  await core.default({ module_or_path: fetch(beside("prodrome_bg.wasm")) });
  const lines = new core.Lines();
  let since = "";
  for (;;) {
    const piece = await (await fetch("/api/stores/lines/skeleton?since=" + since)).json();
    lines.piece(JSON.stringify(piece), JSON.stringify(piece.keys), Date.now());
    if (piece.rest === 0) break;
    since = piece.frontier.digest;
  }
  lines.land().free();
  const me = await (await fetch("/api/me")).json();
  const at = new Date().toISOString().slice(0, 23);
  const sealed = lines.seal(JSON.stringify({ type: "Open", title }), at, me.actor, crypto.getRandomValues(new Uint8Array(32)));
  if (sealed.type === "Unheld") throw new Error(sealed.why);
  const conversation = sealed.conversation;
  sealed.free();
  const [[line]] = JSON.parse(lines.unsent());
  const request = (r) => new Promise((ok, no) => { r.onsuccess = () => ok(r.result); r.onerror = () => no(r.error); });
  const db = await request(indexedDB.open("suzatary-keys", 1));
  const key = await request(db.transaction("keys", "readonly").objectStore("keys").get("device"));
  db.close();
  const hex = (bytes) => Array.from(new Uint8Array(bytes), (b) => b.toString(16).padStart(2, "0")).join("");
  const signature = hex(await crypto.subtle.sign({ name: "Ed25519" }, key.key, new TextEncoder().encode("prodrome object " + line.hash)));
  const signed = JSON.parse(core.signed(line.hash, key.public, signature));
  const body = JSON.stringify({ objects: [line.literal, signed.text], since: "", keys: { [line.hash]: line.keys } });
  const answer = await (await ${SIGNED_POST}("/api/stores/lines/objects", body)).json();
  if (!answer.adopted.includes(line.hash)) throw new Error("the box did not take the opening: " + JSON.stringify(answer));
  return conversation;
})(${JSON.stringify(title)})`;

/** The rounds the topic scenes and shots read, each given its topics by the scripted model's `add_topics`. */
export const TOPIC_ROUNDS: readonly (readonly [string, readonly string[]])[] = [
  ["tag the garden invoices", ["garden", "invoices"]],
  ["tag the garden alone", ["garden"]],
  ["tag the trip alone", ["trip"]],
];

/**
 * The topic rounds on the writer's deployment, asked once, each answered and tagged before the next, so the bar reads
 * garden 2, invoices 1, trip 1.
 */
export async function topicRounds(page: Page): Promise<void> {
  await page.navigate(`${WRITE_BASE}/#/secretary/default`);
  await page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 });
  const held = await page.evaluate<boolean>(
    `fetch("/api/stores/topics/objects").then((r) => r.json()).then((b) => b.objects.some((o) => o.literal.includes("'invoices'")))`,
  );
  if (held) return;
  const id = await page.evaluate<string>(
    opened("Topics"),
  );
  await page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
  await page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
  let memberships = 0
  for (const [said, topics] of TOPIC_ROUNDS) {
    memberships += topics.length
    await page.type(".composebox", said);
    await page.click(".composesend");
    await page.waitFor(
      `fetch("/api/stores/topics/objects").then((r) => r.json()).then((b) => b.objects.filter((o) => o.literal.startsWith("Change(")).length >= ${memberships})`,
      { timeout: 20000 },
    );
    await page.waitFor(`document.querySelector(".answerplace .answerstate") === null`, { timeout: 20000 });
  }
}

/** How long node gives one scene (`scripts/web-e2e.sh` passes the same default to `--test-timeout`). */
export const SCENE_MS = Number(process.env["SUZATARY_E2E_SCENE_MS"] ?? "300000");

/** The longest one step of a scene may take, so a stuck step fails naming itself before the scene's timeout does. */
export const STEP_MS = SCENE_MS / 4;

/** A verb run at the box's terminal (the writer's deployment by default); it blocks the process, so no scene timer
 * fires and it is killed after `STEP_MS` instead. */
export function atTheTerminal(args: readonly string[], instance = process.env["E2E_TRIAGE_INSTANCE"], env: Readonly<Record<string, string>> = {}): string {
  const bin = process.env["E2E_TRIAGE_BIN"];
  if (bin === undefined || instance === undefined) {
    throw new Error("E2E_TRIAGE_BIN/E2E_TRIAGE_INSTANCE are unset — scripts/web-e2e.sh sets them");
  }
  const started = Date.now();
  try {
    return execFileSync(bin, ["--instance", instance, ...args], { encoding: "utf8", env: { ...process.env, ...env }, timeout: STEP_MS });
  } catch (error: unknown) {
    if (!(error instanceof Error && "code" in error && error.code === "ETIMEDOUT")) throw error;
    const printed = ["stdout", "stderr"].map((stream) => (stream in error ? String(Reflect.get(error, stream) ?? "") : "")).join("");
    throw new Error(`\`suzatary ${args.join(" ")}\` did not end in ${Date.now() - started} ms (bound ${STEP_MS}) and was killed\n  it printed: ${printed}`);
  }
}

/** `text` asked in conversation `id` at the box's terminal, as a writer other than this tab; it runs beside the scene
 * and settles once the round has ended. */
export function askAtTheTerminal(id: string, text: string, instance = process.env["E2E_TRIAGE_INSTANCE"]): Promise<string> {
  const bin = process.env["E2E_TRIAGE_BIN"];
  if (bin === undefined || instance === undefined) {
    return Promise.reject(new Error("E2E_TRIAGE_BIN/E2E_TRIAGE_INSTANCE are unset — scripts/web-e2e.sh sets them"));
  }
  return new Promise((asked, refused) =>
    execFile(bin, ["--instance", instance, "secretary", "ask", id, text], { encoding: "utf8", timeout: STEP_MS }, (error, stdout, stderr) =>
      error === null ? asked(stdout) : refused(new Error(`\`suzatary secretary ask ${id}\`: ${error.message}\n  it printed: ${stdout}${stderr}`)),
    ),
  );
}

/** `suzatary device pair` at the writer's box, with that server's own state directory, where it reads the code. */
export function pairAtTheBox(): string {
  const state = process.env["E2E_WRITE_STATE"];
  if (state === undefined) throw new Error("E2E_WRITE_STATE is unset — scripts/web-e2e.sh sets it");
  const printed = atTheTerminal(["device", "pair"], undefined, { XDG_STATE_HOME: state });
  const code = /Pairing code for \S+: ([A-Z]{4}-[A-Z]{4})/.exec(printed)?.[1];
  if (code === undefined) throw new Error(`device pair printed no code: ${printed}`);
  return code;
}

/** What a file's scenes do to the servers: `Writes` trusts this browser's key on the writer's server first, which `Reads`
 * skips for a file against a server whose writer is nobody (the live smoke), where trusting would fail. */
export type Reach = "Reads" | "Writes"

/** Two frames and a task: the tiles' host arranges a resized room in the frame after it hears it (`shell/tiles.ts`). */
export const SETTLED = `new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(done, 0))))`;

export async function suite(prefix: string, size: readonly [number, number] = PHONE, reach: Reach = "Writes"): Promise<Suite> {
  mkdirSync(SHOTS, { recursive: true });
  // Every server before the browser: half the files navigate straight to a hash without `open`.
  await Promise.all([serving(BASE), serving(PROPOSED_BASE), serving(WRITE_BASE)]);
  const browser: Browser = await launch();
  let failed = 0;
  after(async () => {
    if (failed === 0) recordReads(browser.fetched());
    await browser.close();
  });
  const page = browser.page;
  await page.viewport(size[0], size[1]);
  let journal: Promise<Journal> | null = null;
  // Once per browser: its profile, and so its key, lives as long as the suite.
  let trusted: Promise<string> | null = reach === "Writes" ? trustThisDevice(page, WRITE_BASE) : null;
  await trusted;
  // Every scene must leave the box's Content-Security-Policy unviolated; refusals are kept per tab and read after it.
  await page.beforeEveryDocument(
    `addEventListener("securitypolicyviolation", (e) => { try { const k = "e2e.csp"; sessionStorage.setItem(k, JSON.stringify([...JSON.parse(sessionStorage.getItem(k) ?? "[]"), e.effectiveDirective + " " + e.blockedURI + " " + (e.sample ?? "")])) } catch {} })`,
  );
  const violations = (): Promise<string[]> =>
    page.evaluate<string[]>(
      `(() => { try { const k = "e2e.csp"; const v = JSON.parse(sessionStorage.getItem(k) ?? "[]"); sessionStorage.removeItem(k); return v } catch { return [] } })()`,
    );

  return {
    page,
    tab: async () => {
      const other = await browser.tab();
      await other.viewport(size[0], size[1]);
      return other;
    },
    journal: () => (journal ??= page.journal()),
    scene(name: string, body: () => Promise<void>, probe?: string): void {
      test(`${prefix}: ${name}`, async () => {
        try {
          await body();
          const refused = await violations();
          assert.deepEqual(refused, [], "the page's policy refused something it loads or runs");
        } catch (error: unknown) {
          failed += 1;
          const path = join(SHOTS, `${prefix}-${slug(name)}.png`);
          try {
            const html = await page.evaluate<string>(`new XMLSerializer().serializeToString(document)`);
            writeFileSync(path.replace(/\.png$/, ".html"), html);
            await page.screenshot(path);
          } catch {
            // A browser that has died cannot photograph itself; the original failure is the one to report.
          }
          try {
            const asked = probe === undefined ? null : await page.evaluate<unknown>(probe);
            const fetched = await page.evaluate<unknown>(
              `performance.getEntriesByType("resource").map((e) => [Math.round(e.startTime), Math.round(e.duration), e.name.replace(location.origin, "")])`,
            );
            const kept = journal === null ? null : await journal;
            const lines = kept === null ? ["(no journal)"] : [`in flight ${JSON.stringify(kept.inFlight())}`, ...kept.lines()];
            writeFileSync(
              path.replace(/\.png$/, ".trace.txt"),
              [`probe ${JSON.stringify(asked, null, 2)}`, `fetches [start ms, took ms, path] ${JSON.stringify(fetched)}`, ...lines].join("\n"),
            );
          } catch {
            // A browser that has died cannot photograph itself; the original failure is the one to report.
          }
          const said = error instanceof Error ? error.message : String(error);
          throw new Error(`${said}\n  screenshot: ${path}`);
        }
      });
    },
    async open(base: string, hash = "#/"): Promise<void> {
      await serving(base);
      if (base === WRITE_BASE) await (trusted ??= trustThisDevice(page, base));
      await page.navigate(`${base}/${hash}`);
      // `SUZATARY_E2E_TRACE_REQUESTS=1` logs each signed request's stages to the console, which a failure's trace keeps.
      if (process.env["SUZATARY_E2E_TRACE_REQUESTS"] === "1") await page.evaluate(`localStorage.setItem("suzatary.debug.requests", "1")`);
      // A fresh profile draws the fold's own rows first and the composed groups once the replica has pulled.
      await page.waitFor(
        `document.querySelectorAll(".tree .row").length > 0 && (document.querySelector(".tree")?.getAttribute("data-at") ?? "") !== ""`,
      );
    },
    trust: () => (trusted ??= trustThisDevice(page, WRITE_BASE)),
    async resize(next: readonly [number, number]): Promise<void> {
      await page.viewport(next[0], next[1]);
      await page.waitFor(`window.innerWidth === ${next[0]}`);
      await page.evaluate(SETTLED);
    },
  };
}

/** A todo added through the list's add sheet, its id once the box holds it open. */
export async function addTodo(page: Page, body: string, category: string): Promise<string> {
  await page.click(".addsheet .pickerhandle");
  await page.waitFor(`document.querySelector(".addsheet.open") !== null`);
  await page.waitFor(`document.activeElement?.className.includes("draftbody") === true`);
  await page.type(".draftbody", body);
  await pickCategory(page, category);
  await page.click(".addbtn");
  const id = body.toLowerCase().replace(/[^a-z0-9]+/g, "");
  await page.waitFor(`document.querySelector('${row(id)}') !== null`, { timeout: 8000 });
  await page.waitFor(`document.querySelector(".addsheet.open") === null`, { timeout: 2000 });
  await page.waitFor(
    `fetch("/api/fold").then((r) => r.json()).then((b) => b.entries.some((e) => e.todo === "${id}" && e.state === "open"))`,
    { timeout: 15000 },
  );
  return id;
}

/** The todo ids the list is showing, in the order it is showing them. */
export async function shownTodos(page: Page): Promise<string[]> {
  return page.evaluate<string[]>(`[...document.querySelectorAll(".tree .row")].map((row) => row.getAttribute("data-todo"))`);
}

/** A todo's line in the list, as a selector. */
export const row = (todo: string): string => `.tree .row[data-todo="${todo}"]`;

/** Unfold what hides a todo's line: a Fine fold nobody touched is closed. */
export async function reveal(page: Page, todo: string): Promise<void> {
  for (let tries = 0; tries < 6; tries += 1) {
    if (await page.evaluate<boolean>(`document.querySelector('${row(todo)}') !== null`)) return;
    await page.evaluate(
      `document.querySelectorAll('.tree .grouphead[aria-expanded="false"], .tree .rowfold[aria-expanded="false"]').forEach((b) => b.click())`,
    );
  }
  await page.waitFor(`document.querySelector('${row(todo)}') !== null`, { timeout: 4000 });
}

/** Lets the scripted model's running "take your time" round answer: it waits for this file or a Stop, never for a
 * time (`fixture.rs`, `Line.release`). */
export function releaseModel(): void {
  const file = process.env["E2E_MODEL_RELEASE"];
  if (file === undefined || file === "") throw new Error("E2E_MODEL_RELEASE is unset — scripts/web-e2e.sh sets it");
  writeFileSync(file, "");
}

/** Every todo the list holds, in its order, with every fold opened: which folds start open moves with the clock. */
export async function everyTodo(page: Page): Promise<string[]> {
  await page.waitFor(
    `(() => { const closed = [...document.querySelectorAll('.tree .grouphead[aria-expanded="false"], .tree .rowfold[aria-expanded="false"]')]; closed.forEach((b) => b.click()); return closed.length === 0 })()`,
  );
  return shownTodos(page);
}

/** Open the list's search, filters and grouping under its title, if they are shut. */
export async function openFinder(page: Page): Promise<void> {
  if (!(await page.evaluate<boolean>(`document.querySelector(".findbtn")?.getAttribute("aria-expanded") === "true"`))) await page.click(".findbtn");
  await page.waitFor(`document.querySelector(".finder")?.hidden === false`);
}

/** Light a category in the add sheet: a chip already lit is left alone, since a tap on it clears it. */
export async function pickCategory(page: Page, category: string): Promise<void> {
  const chip = `[...document.querySelectorAll(".addsheet .chiprow .chipbtn")].find((b) => b.textContent.trim() === ${JSON.stringify(category)})`;
  if (!(await page.evaluate<boolean>(`${chip}?.getAttribute("aria-pressed") === "true"`))) await page.clickText(".addsheet .chiprow .chipbtn", category);
  await page.waitFor(`${chip}?.getAttribute("aria-pressed") === "true"`);
}

/** Tap a todo's line and wait for it to open in place, its acts below it. */
export async function openInPlace(page: Page, todo: string): Promise<void> {
  await reveal(page, todo);
  const open = `document.querySelector('.rowopen[data-todo="${todo}"]') !== null`;
  if (!(await page.evaluate<boolean>(open))) await page.click(`${row(todo)} .rowmain`);
  await page.waitFor(open, { timeout: 8000 });
}

/** Mark a todo done from the list; the Done button arrives with `GET /api/me`'s answer, not with the line. */
export async function doneFromList(page: Page, todo: string): Promise<void> {
  await openInPlace(page, todo);
  await page.waitFor(`document.querySelector('.rowopen[data-todo="${todo}"] .act-done') !== null`, { timeout: 8000 });
  await page.click(`.rowopen[data-todo="${todo}"] .act-done`);
}

/** Go to a todo from the list the way a phone does: open it in place, then Open. */
export async function openTodo(page: Page, todo: string): Promise<void> {
  await openInPlace(page, todo);
  await page.click(`.rowopen[data-todo="${todo}"] a.capsule.primary`);
  await page.waitFor(`window.location.hash === "#/todo/${todo}"`);
}

/** `/api/fold`, fetched by the page, so through the same gate and peer as the app's own reads. */
export async function fold(page: Page): Promise<{ todo: string; state: string; value: Fulfillment | null }[]> {
  return page.evaluate(
    `fetch("/api/fold").then((response) => response.json()).then((body) => body.entries.map((entry) => ({ todo: entry.todo, state: entry.state, value: entry.value })))`,
  );
}

/** Wait until the core's log of `chain` holds a frame, which only a pull or an act writes; the app's open creates the
 * database whether or not anything was pulled. */
export async function untilReplica(page: Page, chain = "life", timeout = 20000): Promise<void> {
  await page.waitFor(`${chainKept(chain)}.then((n) => n > 0)`, { timeout });
}

/**
 * How many entries of the core's log the device keeps of `chain` (its snapshot and frames, under `chain/<name>/` in the
 * `replica` store): an expression a page evaluates. It never opens a database that is not there, which would create it.
 */
export const chainKept = (chain: string): string => `indexedDB.databases().then((known) => !known.some((db) => db.name === "suzatary") ? 0 : new Promise((resolve) => {
  const request = indexedDB.open("suzatary");
  request.onerror = () => resolve(-1);
  request.onsuccess = () => {
    const db = request.result;
    if (!db.objectStoreNames.contains("replica")) { db.close(); resolve(0); return; }
    const query = db.transaction("replica", "readonly").objectStore("replica").getAllKeys(IDBKeyRange.bound("chain/${chain}/", "chain/${chain}/\uffff"));
    query.onerror = () => { db.close(); resolve(-1); };
    query.onsuccess = () => { const n = query.result.length; db.close(); resolve(n); };
  };
}))`

/** Switch the list through the More sheet, never `localStorage`, since the switch re-asking everything is what is
 * tested; the picker names a chain as `said` in `web/src/app/chain.ts` does. */
export async function switchChain(page: Page, name: string): Promise<void> {
  const said = name === "life" ? "My todos" : name;
  await page.evaluate(
    `[...document.querySelectorAll(".tabbar .tab")].find((tab) => tab.textContent?.includes("More"))?.click()`,
  );
  await page.waitFor(`document.querySelector(".moresheet")?.hidden === false`);
  await page.clickText(".moresheet .chainpick", said);
  await page.waitFor(`document.querySelector(".chainpick.active")?.textContent === ${JSON.stringify(said)}`);
  await page.evaluate(`document.body.click()`);
  await page.waitFor(`document.querySelector(".moresheet")?.hidden === true`);
}

/**
 * In the core's worker, before its own script: each question as it arrives (its number, lane and kind) and each answer
 * as it is sent, in the order the worker saw them, each at the worker's clock; read back as `self.__questions`.
 */
export const WORKER_QUESTIONS = `(() => {
  const seen = (self.__questions = [])
  self.addEventListener("message", (e) => {
    const d = e.data
    if (d !== null && typeof d === "object" && typeof d.id === "number" && "question" in d) seen.push(["asked", d.id, Math.round(performance.now()), d.lane, d.question.type])
  })
  const post = self.postMessage.bind(self)
  self.postMessage = (m, ...rest) => {
    if (m !== null && typeof m === "object" && typeof m.id === "number" && "answer" in m) seen.push(["answered", m.id, Math.round(performance.now())])
    return post(m, ...rest)
  }
})()`;

/** What `WORKER_QUESTIONS` records: a question asked, or answered. */
export type Recorded = readonly ["asked", number, number, string, string] | readonly ["answered", number, number];

/** In the core's worker: each of its phases' marks (`prodrome/phases.ts`), the first of each name, at the worker's clock. */
export const WORKER_MARKS = `(() => {
  const first = {}
  for (const m of performance.getEntriesByType("mark")) if (!(m.name in first)) first[m.name] = m.startTime
  return first
})()`;

/** When the worker answered its first `Stream`, at its clock, from what `WORKER_QUESTIONS` recorded. */
export const streamAnswered = (seen: readonly Recorded[]): number => {
  const asked = seen.find((s) => s[0] === "asked" && s[4] === "Stream");
  const answered = asked === undefined ? undefined : seen.find((s) => s[0] === "answered" && s[1] === asked[1]);
  if (answered === undefined) throw new Error(`the worker answered no stream: ${JSON.stringify(seen)}`);
  return answered[2];
};

/** Whether the device keeps a snapshot of the store `store`, which its log is compacted into once the store is quiet. */
export const snapshotKept = (store: string): string => `new Promise((ok, no) => {
  const r = indexedDB.open("suzatary")
  r.onerror = () => no(String(r.error))
  r.onsuccess = () => {
    const q = r.result.transaction("replica").objectStore("replica").getAllKeys()
    q.onsuccess = () => (r.result.close(), ok(q.result.includes(${JSON.stringify(store)} + "/snapshot")))
  }
})`;

/**
 * `body` run in the page with the core the box serves, `core` its module: the build the box names, its glue and its
 * module, as the core's worker loads them, so the device is written and read in the core's own form.
 */
export const withCore = (body: string): string => `(async () => {
  const build = (await (await fetch("/b/current", { cache: "no-store" })).text()).trim()
  const glue = await import("/b/" + build + "/prodrome.js")
  await glue.default({ module_or_path: fetch("/b/" + build + "/prodrome_bg.wasm") })
  const core = glue
  ${body}
})()`

/** The lines as the device keeps them, read back through the core: the landed row's generation, epoch, nodes and key names. */
export const LANDED_ROW = withCore(`
  const rows = await new Promise((ok, no) => {
    const r = indexedDB.open("suzatary")
    r.onerror = () => no(String(r.error))
    r.onsuccess = () => {
      const tx = r.result.transaction("replica", "readonly")
      const keys = tx.objectStore("replica").getAllKeys()
      const values = tx.objectStore("replica").getAll()
      values.onsuccess = () => (r.result.close(), ok(keys.result.map((k, i) => [k, values.result[i]])))
    }
  })
  const snapshot = rows.find(([k]) => k === "lines/snapshot")?.[1]?.bytes
  const frames = rows.filter(([k]) => k.startsWith("lines/log/")).map(([, v]) => v)
  const framed = new Uint8Array(frames.reduce((n, f) => n + 4 + f.length, 0))
  let at = 0
  for (const f of frames) (new DataView(framed.buffer).setUint32(at, f.length, true), framed.set(f, at + 4), (at += 4 + f.length))
  const lines = new core.Lines()
  lines.load(snapshot, framed)
  const landed = JSON.parse(lines.rows()).landed
  lines.free()
  return landed
`)
/** Whether the device's log of `chain`, read back through the core, holds a landed chain whole: heard from the box, and
 * every object its landing says it holds. */
export const chainLanded = (chain: string): string => withCore(`
  const rows = await new Promise((ok, no) => {
    const r = indexedDB.open("suzatary")
    r.onerror = () => no(String(r.error))
    r.onsuccess = () => {
      const range = IDBKeyRange.bound("chain/${chain}/", "chain/${chain}/\\uffff")
      const tx = r.result.transaction("replica", "readonly")
      const keys = tx.objectStore("replica").getAllKeys(range)
      const values = tx.objectStore("replica").getAll(range)
      values.onsuccess = () => (r.result.close(), ok(keys.result.map((k, i) => [k, values.result[i]])))
    }
  })
  const snapshot = rows.find(([k]) => k === "chain/${chain}/snapshot")?.[1]?.bytes
  const frames = rows.filter(([k]) => k.startsWith("chain/${chain}/log/")).map(([, v]) => v)
  const framed = new Uint8Array(frames.reduce((n, f) => n + 4 + f.length, 0))
  let at = 0
  for (const f of frames) (new DataView(framed.buffer).setUint32(at, f.length, true), framed.set(f, at + 4), (at += 4 + f.length))
  const held = new core.Chain()
  held.load(snapshot, framed)
  const whole = held.heard() && held.closed()
  held.free()
  return whole
`)

export type Landed = { readonly generation: number; readonly epoch: number; readonly frontier: { readonly digest: string } | null; readonly nodes: readonly string[]; readonly keys: readonly string[] }

/**
 * The lines as the device keeps them, the core's snapshot and frames (`replicalog`), each value's bytes in hex, by key:
 * the lines' rows alone, since the inbox and the topic stores are kept in the same store under their own names.
 */
export const REPLICA_ROWS = `new Promise((ok, no) => {
  const r = indexedDB.open("suzatary")
  r.onerror = () => no(String(r.error))
  r.onsuccess = () => {
    const tx = r.result.transaction("replica", "readonly")
    const lines = IDBKeyRange.bound("lines/", "lines/\\uffff")
    const keys = tx.objectStore("replica").getAllKeys(lines)
    const values = tx.objectStore("replica").getAll(lines)
    const hex = (bytes) => [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("")
    values.onsuccess = () => {
      r.result.close()
      ok(keys.result.map((k, i) => [k, hex(values.result[i] instanceof Uint8Array ? values.result[i] : values.result[i].bytes)]))
    }
  }
})`


/**
 * What this file's scenes read beyond their imports, for the merge gate's key (`SUZATARY_E2E_READS`), written only when
 * every scene passed; a script `web/dist` does not hold is another build's, so it reads every module.
 */
function recordReads(fetched: ReadonlySet<string>): void {
  const dir = process.env["SUZATARY_E2E_READS"];
  if (dir === undefined || dir === "") return;
  const dist = "web/dist";
  const reads = new Set<string>();
  for (const path of fetched) {
    // A build's files are served under `/b/<build>/` as well as at the root.
    const file = path.replace(/^\/+/, "").replace(/^b\/[^/]+\//, "");
    if (/^views(_bg\.wasm|\.js)$/.test(file)) {
      reads.add("app/pages");
      reads.add("app/viewtree");
    } else if (file.endsWith(".js") && !existsSync(join(dist, file))) {
      reads.add("web/src");
    } else if (file.endsWith(".js") && existsSync(join(dist, `${file}.map`))) {
      const map = JSON.parse(readFileSync(join(dist, `${file}.map`), "utf8")) as { sources: string[] };
      for (const source of map.sources) reads.add(normalize(join(dist, source)));
    }
  }
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, basename(process.argv[1] ?? "", ".mjs").replace(/\.test$/, "")), [...reads].sort().join("\n") + "\n");
}
