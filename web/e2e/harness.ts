/**
 * What every browser test needs before it can assert anything: a browser, the
 * fixture server's address, and a screenshot when something goes wrong.
 *
 * `scripts/web-e2e.sh` starts THREE fixture servers and names them here.
 * `E2E_BASE` is gated to a login this deployment knows as a VIEWER and not an
 * actor, so its chain cannot change however hard a test tries — every
 * read-only file shares it, and the refusal file drives it on purpose.
 * `E2E_BASE_PROPOSED` is that same viewer over a deployment with a triage card
 * waiting, because an empty Inbox and a full one are two claims and one page.
 * `E2E_BASE_WRITE` is gated to the writer login and has a chain of its own,
 * so a completed todo in one file is never a missing card in another.
 *
 * ONE BROWSER PER FILE. node's test runner gives each file its own process, so
 * a `launch()` at the top of a file is one Chromium for that file's tests, torn
 * down by `after`. It also means each file has its own profile and therefore
 * its own `localStorage`, which the local-first tests depend on.
 *
 * A FAILURE LEAVES A PICTURE. Every scene is wrapped: an assertion that throws
 * gets the page captured to a file, and the path is appended to the message —
 * so a red CI log says where to look rather than "expected true to be false".
 * The document's HTML lands beside it (`.html`): a picture shows what is
 * visible, and a query also answers from what is not.
 */

import assert from "node:assert/strict";
import { after } from "node:test";
import { test } from "node:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";

import type { Browser, Journal, Page } from "./cdp";
import { WAIT_MS, launch } from "./cdp";
import type { Fulfillment } from "../src/wire/generated/wire";
import http from "../src/wire/http";

function required(name: string): string {
  const value = process.env[name];
  if (value === undefined || value === "") {
    throw new Error(`${name} is unset — these tests are run by scripts/web-e2e.sh, which starts the servers`);
  }
  return value;
}

/** The read side. Gated to a viewer who is not an actor, so the chain behind
 * it is immutable by construction rather than by convention. */
export const BASE = required("E2E_BASE");

/**
 * The same viewer login, over a deployment whose triage run left a CARD
 * waiting (§10.6).
 *
 * `BASE` deliberately has nothing waiting, because the EMPTY Inbox is a claim
 * too — and one page cannot be both. So the read-only Inbox is two servers
 * rather than one server whose state depends on which file ran first.
 */
export const PROPOSED_BASE = required("E2E_BASE_PROPOSED");

/** The write side: its own chain, its own tip, safe to move. */
export const WRITE_BASE = required("E2E_BASE_WRITE");

/**
 * The headers a write from a page carries, as the app's own door sends them: the CSRF half and
 * this build's write contract, as a JavaScript object literal for a page expression
 * (`headers: ${WRITE_HEADERS}`). A spec spells no write header of its own (`web-static.sh`), so
 * a contract the box raises reaches every scene at once.
 */
export const WRITE_HEADERS = JSON.stringify(http.actRequest(null).headers);

const SHOTS = process.env["SUZATARY_E2E_SHOTS"] ?? join(tmpdir(), "suzatary-e2e-shots");

/** Phone. The width the app was designed at (CLAUDE.md: it is a todo app on a
 * phone), and under the 62rem breakpoint, so `#/todo/<id>` is a pushed screen. */
export const PHONE: readonly [number, number] = [390, 844];

/** Desktop, above 62rem: the same hash is the list with the detail pane. */
export const DESKTOP: readonly [number, number] = [1280, 800];

/** The audit's wide window: a desk's browser with room for the list, the todo and the secretary at once. */
export const WIDE: readonly [number, number] = [1440, 900];

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
  /** Resize, and wait for the route to be resolved again against the width. */
  resize: (size: readonly [number, number]) => Promise<void>;
}

const slug = (name: string): string => name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");

/**
 * ⚠️ THE SERVER IS UP BEFORE IT IS SERVING (CI, 2026-09-10: the first scene
 * waited 15 s for a card and the list was still empty).
 *
 * `scripts/web-e2e.sh` already waits for warm MARKUP before it starts node, and
 * that is the right gate for the typst cache. It is not the whole of "ready":
 * `node --test` then launches a Chromium per file, and on a shared runner the
 * first of those can reach the app before the axum process has answered
 * anything at all — so the page mounts, asks for a fold, and the scene's
 * budget is spent on the server's cold start rather than on the app.
 *
 * So each file asks the box, from NODE, before it asks the browser: the tip
 * first (the cheapest route there is — it proves the process is listening and
 * the gate admits this peer) and then the FOLD, which is the answer the first
 * scene is actually waiting to see rendered. Once per base per file, memoised,
 * because the second scene's server is the first scene's.
 *
 * It is not a substitute for the page's own waits. A fold this side fetched is
 * evidence the box can answer, never evidence the app drew it — that claim is
 * the scene's, and it is still a `waitFor` over the DOM.
 */
const answering = new Map<string, Promise<void>>();

async function reachable(url: string, until: number): Promise<void> {
  let last = "never asked";
  while (Date.now() < until) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        // Drain it: an unread body holds the socket, and the point of asking
        // was that the box can produce the whole answer.
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
 * THIS BROWSER'S DEVICE KEY, TRUSTED AT THE BOX'S TERMINAL (SPEC law 42): the writer's server
 * signs, so a page that seals an act must hold a key its chain trusts. The key is minted in the
 * page the way `web/src/prodrome/signer.ts` mints it (non-extractable, the same database), and
 * `suzatary device trust` registers its public half, as the owner would for a first device.
 */
export async function trustThisDevice(page: Page, base: string, instance?: string): Promise<string> {
  // A file of the origin, not the app: the key is trusted before the app's first pull, so the
  // replica it seals onto already says so.
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
 * A write as the app sends it, for a page's own `evaluate`: the request stamped now, named by its
 * content and signed by this browser's device key, as `web/src/wire/http.ts` does, so a scene that asks a
 * route directly passes the box's door the way a tap does. An expression of a function of the
 * path and the body's text, answering the `fetch`'s promise.
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

/** The rounds the topic scenes and shots read, each given its topics by the scripted model's `add_topics`. */
export const TOPIC_ROUNDS: readonly (readonly [string, readonly string[]])[] = [
  ["tag the garden invoices", ["garden", "invoices"]],
  ["tag the garden alone", ["garden"]],
  ["tag the trip alone", ["trip"]],
];

/**
 * THE TOPIC ROUNDS ON THE WRITER'S DEPLOYMENT, asked once: a conversation of their own, each question answered and
 * given its topics before the next is asked, so the bar reads garden 2, invoices 1, trip 1. Another file or scene
 * that ran first leaves them there, and nothing is asked again.
 */
export async function topicRounds(page: Page): Promise<void> {
  await page.navigate(`${WRITE_BASE}/#/secretary/default`);
  await page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 });
  const held = await page.evaluate<boolean>(
    `fetch("/api/stores/topics/objects").then((r) => r.json()).then((b) => b.objects.some((o) => o.literal.includes("'invoices'")))`,
  );
  if (held) return;
  const id = await page.evaluate<string>(
    `${SIGNED_POST}("/api/secretary/conversations", JSON.stringify({title: "Topics"})).then((r) => r.json()).then((j) => j.id)`,
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

/** A verb of a deployment (the writer's, by default) run at the box's terminal, as the owner would run it. */
export function atTheTerminal(args: readonly string[], instance = process.env["E2E_TRIAGE_INSTANCE"], env: Readonly<Record<string, string>> = {}): string {
  const bin = process.env["E2E_TRIAGE_BIN"];
  if (bin === undefined || instance === undefined) {
    throw new Error("E2E_TRIAGE_BIN/E2E_TRIAGE_INSTANCE are unset — scripts/web-e2e.sh sets them");
  }
  return execFileSync(bin, ["--instance", instance, ...args], { encoding: "utf8", env: { ...process.env, ...env } });
}

/**
 * `suzatary device pair` at the writer's box, run with that server's own state directory, where the server reads the
 * code it keeps: the code as the terminal printed it.
 */
export function pairAtTheBox(): string {
  const state = process.env["E2E_WRITE_STATE"];
  if (state === undefined) throw new Error("E2E_WRITE_STATE is unset — scripts/web-e2e.sh sets it");
  const printed = atTheTerminal(["device", "pair"], undefined, { XDG_STATE_HOME: state });
  const code = /Pairing code for \S+: ([A-Z]{4}-[A-Z]{4})/.exec(printed)?.[1];
  if (code === undefined) throw new Error(`device pair printed no code: ${printed}`);
  return code;
}

export async function suite(prefix: string, size: readonly [number, number] = PHONE): Promise<Suite> {
  mkdirSync(SHOTS, { recursive: true });
  // EVERY SERVER, BEFORE THE BROWSER. `open` waits too, but half the files
  // navigate straight to a hash without it (the chain, the gallery),
  // and those were exactly as exposed to a cold start. All of them are running
  // by the time node begins, so asking costs one request each and closes the
  // question for every scene in the file.
  await Promise.all([serving(BASE), serving(PROPOSED_BASE), serving(WRITE_BASE)]);
  const browser: Browser = await launch();
  after(async () => {
    await browser.close();
  });
  const page = browser.page;
  await page.viewport(size[0], size[1]);
  let journal: Promise<Journal> | null = null;
  // Once per browser, before any scene: its profile, and so its key, lives as long as the suite,
  // and every write the writer's box performs asks for a key it trusts.
  let trusted: Promise<string> | null = trustThisDevice(page, WRITE_BASE);
  await trusted;
  // Every scene runs under the box's Content-Security-Policy and must leave it unviolated: what a
  // page is refused is kept per tab and origin, and read after the scene.
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
          const path = join(SHOTS, `${prefix}-${slug(name)}.png`);
          try {
            const html = await page.evaluate<string>(`new XMLSerializer().serializeToString(document)`);
            writeFileSync(path.replace(/\.png$/, ".html"), html);
            await page.screenshot(path);
          } catch {
            // A browser that has already died cannot photograph itself; the
            // original failure is the one worth reporting.
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
            // A browser that has already died cannot photograph itself; the
            // original failure is the one worth reporting.
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
      // Lines, priced by the core in the tab: a fresh profile draws the fold's
      // own rows first and the composed groups once the replica has pulled.
      await page.waitFor(
        `document.querySelectorAll(".tree .row").length > 0 && (document.querySelector(".tree")?.getAttribute("data-at") ?? "") !== ""`,
      );
    },
    trust: () => (trusted ??= trustThisDevice(page, WRITE_BASE)),
    async resize(next: readonly [number, number]): Promise<void> {
      await page.viewport(next[0], next[1]);
      // `main.ts` re-routes on the breakpoint's matchMedia change, which fires
      // after the metrics land; nothing here may assert before it has.
      await page.waitFor(`window.innerWidth === ${next[0]}`);
    },
  };
}

/** The todo ids the list is showing, in the order it is showing them. */
export async function shownTodos(page: Page): Promise<string[]> {
  return page.evaluate<string[]>(`[...document.querySelectorAll(".tree .row")].map((row) => row.getAttribute("data-todo"))`);
}

/** A todo's line in the list, as a selector. */
export const row = (todo: string): string => `.tree .row[data-todo="${todo}"]`;

/** Unfold what hides a todo's line, the way a person looks for it: a Fine fold
 * nobody touched is closed, and a list that moved can put a line behind one. */
export async function reveal(page: Page, todo: string): Promise<void> {
  for (let tries = 0; tries < 6; tries += 1) {
    if (await page.evaluate<boolean>(`document.querySelector('${row(todo)}') !== null`)) return;
    await page.evaluate(
      `document.querySelectorAll('.tree .grouphead[aria-expanded="false"], .tree .rowfold[aria-expanded="false"]').forEach((b) => b.click())`,
    );
  }
  await page.waitFor(`document.querySelector('${row(todo)}') !== null`, { timeout: 4000 });
}

/** Lets the scripted model's running "take your time" round answer: it waits for this file or a Stop, never for
 * a time (`fixture.rs`, `Line.release`), so a scene sees the round running for as long as it looks. A scene that
 * stops the round writes none, since nothing would take it. */
export function releaseModel(): void {
  const file = process.env["E2E_MODEL_RELEASE"];
  if (file === undefined || file === "") throw new Error("E2E_MODEL_RELEASE is unset — scripts/web-e2e.sh sets it");
  writeFileSync(file, "");
}

/** Every todo the list holds, in its order, with every fold opened the way a person opens them: which folds start
 * open follows the groups' worth, which moves with the clock, so the lines in view are no list of what is held. */
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

/** Mark a todo done from the list: open it in place, then its Done. The button
 * arrives with `GET /api/me`'s answer, not with the line. */
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

/**
 * `/api/fold`, fetched by the PAGE — so it goes through the same origin, the
 * same gate and the same peer the app's own reads do. A test that asked node
 * for it would be asking a different client a different question.
 */
export async function fold(page: Page): Promise<{ todo: string; state: string; value: Fulfillment | null }[]> {
  return page.evaluate(
    `fetch("/api/fold").then((response) => response.json()).then((body) => body.entries.map((entry) => ({ todo: entry.todo, state: entry.state, value: entry.value })))`,
  );
}

/**
 * WAIT UNTIL THIS BROWSER IS ACTUALLY A REPLICA — holding objects, so an act
 * has a genesis to name and heads to rest on.
 *
 * ⚠️ NOT "the database exists", which is the obvious check and is WRONG.
 * `objects.ts` opens `suzatary` to read the replica back, and
 * `indexedDB.open` CREATES a database that is not there — so the database
 * exists the moment the app has looked for it, whether or not anything was
 * ever pulled. A scene that acted on that would be racing the first sync, and
 * an act on an empty replica is refused, so the failure would read as a UI
 * bug.
 *
 * The row's own `heads` is the real answer: it is written only by a commit,
 * and a commit only happens after a pull or an append.
 */
export async function untilReplica(page: Page, chain = "life", timeout = 20000): Promise<void> {
  await page.waitFor(
    `new Promise((resolve) => {
      const request = indexedDB.open("suzatary");
      request.onerror = () => resolve(false);
      request.onsuccess = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains("chain")) { db.close(); resolve(false); return; }
        const query = db.transaction("chain", "readonly").objectStore("chain").get("objects:${chain}");
        query.onerror = () => { db.close(); resolve(false); };
        query.onsuccess = () => {
          const row = query.result;
          db.close();
          resolve(row !== undefined && Array.isArray(row.objects) && row.objects.length > 0);
        };
      };
    })`,
    { timeout },
  );
}

/**
 * SWITCH THE LIST, the way a reader does: open the More sheet and tap a chain.
 *
 * Through the UI rather than by writing `localStorage`, because what these
 * scenes are about is that the switch RE-ASKS everything — the fold, the
 * replica, the stream and `me` — and setting the value behind the app's back
 * would skip exactly the code under test.
 *
 * ⚠️ THE SWITCHER IS NOT ALWAYS THERE (chain item `chainswitcherlies`,
 * `docs/AUDIT-UX-2026-09-12.md` finding 5): `nav.ts` drops the row while a
 * chain-specific destination — Project, today — is active, because a control
 * that disagrees with the page it sits on is worse than no control. Both
 * chains this deployment serves have a destination of their own (`life` is
 * Todos, `suzatary` is Project), so the tap lands on the bar's own tab when
 * the sheet has no row for it, and either way the PAGE TITLE — not the
 * switcher's own `active` class, which the tap into Project just erased —
 * is what says the switch landed.
 */
export async function switchChain(page: Page, name: string): Promise<void> {
  const label = name === "suzatary" ? "Project" : "Todos";
  const said = name === "suzatary" ? "This app's work" : "My todos";
  const hasSwitcher = await page.evaluate<boolean>(`document.querySelector(".tabbar .chainpick") !== null`);
  if (hasSwitcher) {
    await page.evaluate(
      `[...document.querySelectorAll(".tabbar .tab")].find((tab) => tab.textContent?.includes("More"))?.click()`,
    );
    await page.evaluate(
      `[...document.querySelectorAll(".chainpick")].find((pick) => pick.textContent === ${JSON.stringify(said)})?.click()`,
    );
  } else {
    await page.evaluate(
      `[...document.querySelectorAll(".tabbar .tab")].find((tab) => tab.textContent?.includes(${JSON.stringify(
        label,
      )}))?.click()`,
    );
  }
  await page.waitFor(`document.querySelector("#view .pagetitle")?.textContent === ${JSON.stringify(label)}`);
}
