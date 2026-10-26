/**
 * A browser, driven, with no dependency.
 *
 * The frontend's unit tests run its pure modules under node. Everything a
 * browser actually does — a card that opens a todo, a key that moves a cursor,
 * a write that rolls back, a fold drawn from `localStorage` under a banner that
 * dates it — had no test at all until this file; the screenshots an agent took
 * and looked at were the UI's test suite, which is a test suite whose
 * instrument is an eye.
 *
 * The usual answer is Playwright or Puppeteer, and the usual answer is a
 * dependency tree in a repo that has no npm and does not want one
 * (`scripts/build-web.sh`: one esbuild bundle, no lockfile). It is also not
 * needed. Chromium speaks the DevTools Protocol over a WebSocket; node 24 has
 * a global `WebSocket`; the protocol is JSON in and JSON out. So the client is
 * this file, and the browser is a package in flake.nix beside typst.
 *
 * WHAT IT IS NOT. There is no auto-waiting, no locator algebra, no shadow-DOM
 * piercing, no network mocking beyond one blunt "fail these URLs". Everything
 * here exists because a test below needs it, which is why `waitFor` is a
 * polling loop over `evaluate` rather than a subscription to mutation events:
 * the app holds an SSE connection to `/events`, and an open stream means
 * Chromium's virtual-time budgets never expire, so REAL time and polling are
 * the honest way to wait. That is also why nothing here tries to be fast.
 *
 * READS GO THROUGH `evaluate`, and the assertions are the tests'. A helper
 * that returned "the visible text of the page" would be a second renderer to
 * disagree with; a test asks the DOM the question it means.
 */

import { spawn } from "node:child_process";
import type { ChildProcess } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import http from "../src/wire/http";
import { BASELINE_MS, WAIT_MS } from "./budget";

const sleep = (ms: number): Promise<void> => new Promise((done) => setTimeout(done, ms));

export { WAIT_MS };
const STRETCH = WAIT_MS / BASELINE_MS;

/**
 * The binary. `chromium` on PATH is the answer inside `nix develop .#triage`
 * and in CI, which is where this is meant to run; `SUZATARY_CHROMIUM` is for
 * driving it from somewhere else without editing anything.
 */
function chromiumBinary(): string {
  return process.env["SUZATARY_CHROMIUM"] ?? "chromium";
}

/**
 * The flags, and why each one is here.
 *
 * `--headless=new` is the real browser without a window — the old headless was
 * a separate implementation, and a test that passed only in it would be
 * testing something nobody ships. `--no-sandbox` because the sandbox needs
 * kernel namespaces a CI container may not grant, and the page under test is
 * ours. `--disable-dev-shm-usage` because a CI container's /dev/shm is 64 MB
 * and a renderer that runs out of it dies with no error anyone can read.
 * `--font-render-hinting=none` keeps text metrics stable across machines;
 * nothing here compares pixels, but a hinted glyph can change a line count and
 * therefore a clamped card's height.
 */
function flags(profile: string): string[] {
  return [
    "--headless=new",
    "--no-sandbox",
    "--disable-gpu",
    "--disable-dev-shm-usage",
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-extensions",
    "--disable-background-networking",
    // NO BACK/FORWARD CACHE (2026-09-16). The screenshot scenes walk three
    // fixture origins in one tab, and the browser kept every outgoing document
    // alive in its back/forward cache with its event streams open, so five to
    // seven sockets per host stayed ESTABLISHED (measured with `ss` during a
    // run) against a per-host limit of six, and every later request queued
    // forever — even a bare `fetch` from the console, while the server
    // answered the shell in under a millisecond. A scene is a sequence of
    // pages somebody looks at; nothing in it goes Back. The app closes its
    // streams on `pagehide` too, for the browsers that do keep a document.
    "--disable-features=BackForwardCache",
    // A backgrounded renderer throttles its timers, and every wait in these
    // tests is a real-time poll against a page that must keep running.
    "--disable-background-timer-throttling",
    "--disable-backgrounding-occluded-windows",
    "--disable-renderer-backgrounding",
    "--hide-scrollbars",
    "--force-device-scale-factor=1",
    "--font-render-hinting=none",
    `--user-data-dir=${profile}`,
    // 0: the kernel picks, and DevToolsActivePort in the profile says which —
    // a fixed port is a collision between two runs on one box.
    "--remote-debugging-port=0",
    "about:blank",
  ];
}

interface Pending {
  readonly resolve: (value: Record<string, unknown>) => void;
  readonly reject: (error: Error) => void;
}

/** One WebSocket to the browser, with `sessionId` routing for page targets. */
class Connection {
  private readonly socket: WebSocket;
  private readonly pending = new Map<number, Pending>();
  private readonly listeners = new Map<string, ((params: Record<string, unknown>, session?: string) => void)[]>();
  private next = 1;
  private closed = "";

  private constructor(socket: WebSocket) {
    this.socket = socket;
    socket.addEventListener("message", (event: MessageEvent) => {
      this.receive(JSON.parse(String(event.data)) as Record<string, unknown>);
    });
    socket.addEventListener("close", () => {
      this.closed = "the DevTools connection closed";
      for (const [id, waiter] of this.pending) {
        this.pending.delete(id);
        waiter.reject(new Error(this.closed));
      }
    });
  }

  static async open(url: string): Promise<Connection> {
    const socket = new WebSocket(url);
    await new Promise<void>((resolve, reject) => {
      socket.addEventListener("open", () => {
        resolve();
      });
      socket.addEventListener("error", () => {
        reject(new Error(`could not connect to ${url}`));
      });
    });
    return new Connection(socket);
  }

  private receive(message: Record<string, unknown>): void {
    const id = message["id"];
    if (typeof id === "number") {
      const waiter = this.pending.get(id);
      if (waiter === undefined) return;
      this.pending.delete(id);
      const error = message["error"] as { message?: string } | undefined;
      if (error !== undefined) waiter.reject(new Error(error.message ?? "DevTools refused the command"));
      else waiter.resolve((message["result"] ?? {}) as Record<string, unknown>);
      return;
    }
    const method = message["method"];
    if (typeof method !== "string") return;
    const session = message["sessionId"];
    for (const listener of this.listeners.get(method) ?? []) {
      listener((message["params"] ?? {}) as Record<string, unknown>, typeof session === "string" ? session : undefined);
    }
  }

  /** `session` is the page an event came from: one socket carries every page's. */
  on(method: string, listener: (params: Record<string, unknown>, session?: string) => void): void {
    const found = this.listeners.get(method);
    if (found === undefined) this.listeners.set(method, [listener]);
    else found.push(listener);
  }

  /** One command. `sessionId` is what makes a page command reach the page
   * rather than the browser — flat attachment, so there is one socket. */
  send(method: string, params: Record<string, unknown> = {}, sessionId?: string): Promise<Record<string, unknown>> {
    if (this.closed !== "") return Promise.reject(new Error(`${this.closed} (${method})`));
    const id = this.next++;
    const message: Record<string, unknown> = { id, method, params };
    if (sessionId !== undefined) message["sessionId"] = sessionId;
    this.socket.send(JSON.stringify(message));
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
    });
  }

  close(): void {
    this.closed = "the DevTools connection was closed by the test";
    this.socket.close();
  }
}

/** Modifier bits, as the protocol counts them. */
const ALT = 1;
const CTRL = 2;
const META = 4;
const SHIFT = 8;

export interface KeyModifiers {
  readonly ctrl?: boolean;
  readonly shift?: boolean;
  readonly meta?: boolean;
  readonly alt?: boolean;
}

interface KeySpec {
  readonly key: string;
  readonly code: string;
  readonly keyCode: number;
  /** The character the key produces, absent for a key that produces none. */
  readonly text?: string;
}

/** The named keys these tests press. A table rather than a lookup library:
 * every entry is here because a shortcut in `web/src/keys.ts` uses it. */
const NAMED: Record<string, KeySpec> = {
  Enter: { key: "Enter", code: "Enter", keyCode: 13, text: "\r" },
  Escape: { key: "Escape", code: "Escape", keyCode: 27 },
  Tab: { key: "Tab", code: "Tab", keyCode: 9, text: "\t" },
  ArrowDown: { key: "ArrowDown", code: "ArrowDown", keyCode: 40 },
  ArrowUp: { key: "ArrowUp", code: "ArrowUp", keyCode: 38 },
  Backspace: { key: "Backspace", code: "Backspace", keyCode: 8 },
};

const PUNCTUATION: Record<string, { readonly code: string; readonly keyCode: number }> = {
  "/": { code: "Slash", keyCode: 191 },
  "?": { code: "Slash", keyCode: 191 },
};

function keySpec(key: string): KeySpec {
  const named = NAMED[key];
  if (named !== undefined) return named;
  if (key.length !== 1) throw new Error(`cdp: no key spec for ${key} — add one to NAMED`);
  const punctuation = PUNCTUATION[key];
  if (punctuation !== undefined) return { key, code: punctuation.code, keyCode: punctuation.keyCode, text: key };
  const upper = key.toUpperCase();
  const code = upper >= "A" && upper <= "Z" ? `Key${upper}` : `Digit${key}`;
  return { key, code, keyCode: upper.charCodeAt(0), text: key };
}

export interface WaitOptions {
  /** Milliseconds, defaulting to `WAIT_MS` — the machine's budget, not a
   * number chosen here. Every wait in this suite is real time against a page
   * holding an SSE connection, and a flaky timeout is a worse failure than a
   * slow test. A scene that names its own number is naming a claim about that
   * ONE wait; everything else follows the environment. */
  readonly timeout?: number;
  readonly interval?: number;
}

/**
 * What the network does with an intercepted request: `lose` lets it reach the box and fails its answer; `stale` sends
 * the next one as an older build would, without its write contract, and lets the rest through; `refuse` answers it as
 * the box answers a request it decided no to, a 403 with its sentence.
 */
type Verdict = "fail" | "hold" | "continue" | "lose" | "stale" | "refuse";
type Stage = "Request" | "Response";

/** A response's body as `captureNetwork` read it at its headers, or why it could not be read. */
type Answer = { type: "Read"; body: string } | { type: "Unread"; why: string };
/** A running `captureNetwork`: what it matches, each answer it read by the request's `Network` id, and how it hears of
 * an answer it cannot read, which fails it whether or not the request ever ends. */
type Capture = { fragments: readonly string[]; answers: Map<string, Answer>; unread: (networkId: string) => void };
// Answers that carry no body, so none is asked for.
const NO_BODY = [204, 304];

/**
 * One page, and the only handle the tests hold.
 *
 * Every method takes a CSS selector or a JavaScript EXPRESSION. Expressions,
 * not statements: `evaluate` hands the string to `Runtime.evaluate`, so a
 * multi-step read is written as an immediately-invoked arrow function, and
 * what comes back is whatever it returns by value.
 */
export class Page {
  /** What to do with an intercepted request; null when nothing is. */
  private answer: Verdict | null = null;
  /** What `intercept` pauses, and at which stage; null when nothing is. */
  private paused: { fragments: readonly string[]; stage: Stage } | null = null;
  private captures: Capture[] = [];
  private listening = false;
  private held: string[] = [];
  private heldAt: string[] = [];
  private attaching = false;
  private readonly workers = new Set<string>();
  private coldOn = false;
  private rate = 1;

  /** The sentence `refuse` answers with. */
  private refusal = "";

  constructor(
    private readonly connection: Connection,
    private readonly session: string,
  ) {}

  private async command(method: string, params: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
    return this.connection.send(method, params, this.session);
  }

  /** Close this tab. */
  async close(): Promise<void> {
    await this.command("Page.close");
  }

  /** CSS pixels the page believes it has. The app resolves its route against
   * `window.innerWidth`, so this is what decides a pushed screen from a pane. */
  async viewport(width: number, height: number): Promise<void> {
    await this.command("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor: 1,
      mobile: false,
    });
  }

  /**
   * Go somewhere and wait until the page is there.
   *
   * Waiting on `Page.loadEventFired` looks like the obvious answer and is
   * wrong: this app is a hash router, so most of its navigation is
   * SAME-DOCUMENT and fires no load event at all, and a test that waited for
   * one would hang on every `#/todo/<id>`. The condition that holds for both
   * kinds is the one below — the location IS the one asked for, and the
   * document is complete. A full navigation only publishes the new href when
   * it commits, at which point `readyState` is "loading", so this cannot pass
   * on the outgoing page.
   *
   * ⚠️ A FRAGMENT CHANGE IS MADE FROM INSIDE THE PAGE, NOT THROUGH THE
   * PROTOCOL (2026-09-16). `Page.navigate` to a URL that differs from the
   * current one only in its hash is a same-document navigation the browser
   * process drives, and after one particular sequence — an act on the write
   * fixture, a full load elsewhere, then a fragment `Page.navigate` — every
   * request the page issued afterwards (`/api/usage`, `/api/life/fold`) was
   * called and never resolved, while the identical `fetch` from the console
   * completed at once and the server answered the shell in under a
   * millisecond. Reproduced in isolation four times; setting `location.hash`
   * from inside the page instead worked every time. No user navigates this
   * app through the devtools protocol, so the harness now changes a fragment
   * the way a click does and keeps `Page.navigate` for real loads. This was
   * the "shots flake under load" every agent reported this week: the four
   * screenshot scenes are the ones that walk a dozen fragments in one
   * document.
   */
  /** A page of the app has run its startup (`main.ts`), or its loader has said why it could not: `boot.js` loads the
   * app after the document's own load, so a complete document is not yet an app listening for keys. */
  static readonly STARTED = `(document.documentElement.dataset["booting"] === undefined || document.documentElement.dataset["started"] !== undefined)`;

  async navigate(url: string): Promise<void> {
    const target = new URL(url).href;
    const current = await this.evaluate<string>("location.href");
    const here = new URL(current);
    const there = new URL(target);
    const sameDocument =
      here.origin === there.origin && here.pathname === there.pathname && here.search === there.search &&
      there.hash !== "" && current !== target;
    if (sameDocument) {
      await this.evaluate(`(() => { location.hash = ${JSON.stringify(there.hash)}; return true; })()`);
    } else {
      // ⚠️ A CROSS-ORIGIN LOAD GOES THROUGH `about:blank` FIRST (2026-09-16).
      // The screenshot scenes walk three fixture origins in one tab, and
      // after enough switches every request to one of them hung — even a
      // bare `fetch` from the console — while the server answered the shell
      // in under a millisecond. The browser was keeping the outgoing
      // documents alive with their event streams open, and the per-host
      // connection pool filled with connections no page would ever release.
      // Landing on `about:blank` between origins tears the old document down
      // deterministically; a same-origin load replaces its document anyway.
      if (here.origin !== there.origin && here.origin !== "null") {
        await this.command("Page.navigate", { url: "about:blank" });
        await this.waitFor(`location.href === "about:blank"`, { timeout: 20000 });
      }
      const result = await this.command("Page.navigate", { url: target });
      const problem = result["errorText"];
      if (typeof problem === "string") throw new Error(`navigate ${target}: ${problem}`);
    }
    await this.waitFor(`location.href === ${JSON.stringify(target)} && document.readyState === "complete" && ${Page.STARTED}`, {
      timeout: 20000,
    });
  }

  /** A real reload. The sentinel is what tells a fresh document from the one
   * that was already complete a moment ago — `location.href` cannot, because a
   * reload does not change it. */
  async reload(): Promise<void> {
    await this.evaluate(`window.__e2eReloading = true`);
    await this.command("Page.reload", {});
    await this.waitFor(`window.__e2eReloading === undefined && document.readyState === "complete" && ${Page.STARTED}`, {
      timeout: 20000,
    });
  }

  /** A JavaScript expression, evaluated in the page, returned by value. A
   * promise is awaited; a thrown error becomes a thrown error here. */
  async evaluate<T>(expression: string): Promise<T> {
    const result = await this.command("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
      userGesture: true,
    });
    const failure = result["exceptionDetails"] as { exception?: { description?: string }; text?: string } | undefined;
    if (failure !== undefined) {
      const said = failure.exception?.description ?? failure.text ?? "unknown";
      throw new Error(`evaluate failed: ${said}\n  expression: ${expression}`);
    }
    return (result["result"] as { value: T }).value;
  }

  /**
   * Poll until the expression is truthy. THE waiting primitive: see the file
   * header for why it is a poll and not a subscription.
   *
   * ⚠️ THE EXPRESSION MAY BE A PROMISE, and it must be allowed to be — a wait
   * for something the page has to ASK for (`fetch("/api/fold")…`,
   * `indexedDB.databases()`) is the most useful kind there is. It used to be a
   * bare `Boolean(<expr>)`, and `Boolean(aPromise)` is `true` for every promise
   * ever made, so a promise-shaped wait returned on the FIRST poll and waited
   * for nothing. Several scenes were passing that way, and passing loudly:
   * every assertion after them still ran, just against whatever the page
   * happened to hold at that instant.
   *
   * `Promise.resolve(…).then(Boolean)` fixes both shapes at once. `evaluate`
   * sets `awaitPromise`, so a sync expression costs one microtask and an async
   * one is finally awaited. A synchronous THROW still throws before
   * `Promise.resolve` sees it, so the loop's catch is unchanged.
   */
  async waitFor(expression: string, options: WaitOptions = {}): Promise<void> {
    // A scene's own number is a claim about how long THAT wait takes relative
    // to the others; `STRETCH` moves all of them onto this machine at once.
    // FLOORED at a quarter of the budget (2026-09-14, project chain
    // `e2edestinationsflake`): a busy box's budget can be several times the
    // baseline, and the shortest scene-local waits — a 4 s toast — are
    // exactly the ones a loaded box's own scheduling jitter eats, so a
    // relative judgement that small is not trustworthy at any scale. The
    // floor is on the BUDGET, not on the scene's own number, so an idle box
    // (STRETCH 1) never sees it move.
    const timeout = Math.max(
      Math.round((options.timeout ?? WAIT_MS) * STRETCH),
      Math.round(WAIT_MS / 4),
    );
    const interval = options.interval ?? 50;
    const until = Date.now() + timeout;
    let last = "";
    for (;;) {
      try {
        if (await this.evaluate<boolean>(`Promise.resolve(${expression}).then(Boolean)`)) return;
        last = "it stayed false";
      } catch (error: unknown) {
        // A navigation mid-poll destroys the context; that is a reason to try
        // again, not to fail. A real error survives the timeout and is
        // reported with the expression that caused it.
        last = error instanceof Error ? error.message : String(error);
      }
      if (Date.now() >= until) throw new Error(`waited ${timeout}ms for ${expression}\n  last: ${last}`);
      await sleep(interval);
    }
  }

  /** The box of whatever `finder` evaluates to, after bringing it into view.
   * `finder` is an expression yielding an Element, so a caller can reach a
   * control the way a person does — "the chip that says home" — without the
   * page having to grow an attribute for the test's benefit. */
  private async box(finder: string): Promise<{ x: number; y: number }> {
    const literal = JSON.stringify(finder);
    return this.evaluate(`(() => {
      const node = (${finder});
      if (node === null || node === undefined) throw new Error("nothing found by " + ${literal});
      node.scrollIntoView({ block: "center", inline: "center" });
      const rect = node.getBoundingClientRect();
      // A closed <details> keeps its content laid out and skipped (content-visibility), so its box is a real
      // rectangle that hit testing passes through: hidden, not covered.
      if (rect.width === 0 || rect.height === 0 || !node.checkVisibility()) throw new Error(${literal} + " is hidden (no box, or inside a closed <details>)");
      const x = rect.left + rect.width / 2;
      const y = rect.top + rect.height / 2;
      const hit = document.elementFromPoint(x, y);
      if (hit === null || !(node === hit || node.contains(hit))) {
        const name = (e) => e.tagName.toLowerCase() + (e.id ? "#" + e.id : "") + [...e.classList].map((c) => "." + c).join("");
        const layer = (e) => { const s = getComputedStyle(e); return name(e) + "{" + s.position + " z=" + s.zIndex + "}"; };
        const path = (e) => { const out = []; for (let n = e; n !== null && n !== document.documentElement; n = n.parentElement) out.push(layer(n)); return out.join(" < "); };
        const r = (b) => [b.left, b.top, b.width, b.height].map(Math.round).join(",");
        throw new Error("something covers " + ${literal} + " at its centre: " + (hit === null ? "nothing" : path(hit)) +
          "\\n  target " + path(node) + " box " + r(rect) +
          "\\n  viewport " + innerWidth + "x" + innerHeight + " scroll " + Math.round(scrollX) + "," + Math.round(scrollY) +
          " document " + document.documentElement.scrollWidth + "x" + document.documentElement.scrollHeight);
      }
      return { x, y };
    })()`);
  }

  /**
   * A real mouse click at the element's centre — not `element.click()`.
   *
   * The difference matters here: a card is `role="link"` that CONTAINS
   * buttons, and its handler asks what the click landed on
   * (`card.ts::onControl`). Synthesising the DOM call would skip hit testing
   * and pass through anything covering the target, which is exactly the bug a
   * docked sheet over a list would cause.
   */
  async click(selector: string): Promise<void> {
    await this.clickNode(`document.querySelector(${JSON.stringify(selector)})`);
  }

  /** Click the first element matching `selector` whose visible text is `text`.
   * The app labels its controls with words rather than ids — a category chip
   * says "home" — and finding one by what it says is closer to what a person
   * does than adding a hook to the page for the test to grab. */
  async clickText(selector: string, text: string): Promise<void> {
    await this.clickNode(
      `[...document.querySelectorAll(${JSON.stringify(selector)})].find((node) => node.textContent.trim() === ${JSON.stringify(text)})`,
    );
  }

  /**
   * Click whatever an expression yields, once it is somewhere a finger could
   * reach it.
   *
   * The box is retried, and required to HOLD STILL, because "not clickable
   * yet" is a transient state in this app rather than an error: the add sheet
   * slides up over 220ms. A control asked for its rectangle mid-transition is
   * either off the bottom of the screen — which the box check catches — or, far
   * worse, in a position it is about to leave, so the click lands on whatever
   * has slid into that point by the time the events are dispatched. That is
   * not hypothetical: it is what happened here, and the symptom was a test
   * that chose the "Deadline" segment while asking for a category chip.
   *
   * So the wait is for the same rectangle twice, a frame or two apart, and the
   * press then counts only where it lands on the thing (`pressAt`). It ends when
   * the thing is actually hittable, actually still and actually pressed, and
   * the failure it eventually reports is the real one — covered by what,
   * hidden, never settled, or moved under every press.
   */
  async clickNode(finder: string, timeout = 5000): Promise<void> {
    const until = Date.now() + timeout;
    let last = "";
    for (;;) {
      let settled: { x: number; y: number } | null = null;
      try {
        const first = await this.box(finder);
        await sleep(90);
        const second = await this.box(finder);
        if (Math.abs(first.x - second.x) < 1 && Math.abs(first.y - second.y) < 1) settled = second;
        else last = `it was still moving: ${JSON.stringify(first)} then ${JSON.stringify(second)}`;
      } catch (error: unknown) {
        last = error instanceof Error ? error.message : String(error);
      }
      if (settled !== null) {
        const landed = await this.pressAt(finder, settled.x, settled.y);
        if (landed === null) return;
        last = landed;
      }
      if (Date.now() >= until) throw new Error(last);
      await sleep(50);
    }
  }

  /**
   * One press at a point, which counts only if it lands on what `finder` yields, down and click alike: null when it
   * did, else what it met.
   *
   * A box held still for 90 ms is not one held still until the press arrives: a banner the page draws above it or
   * takes away (the stale bar, a card that needs the owner) moves it, and the press lands on whatever slid into that
   * point; or it goes down on the node and comes up beside it, so its click reaches an ancestor; or the page draws the
   * node again between the two, so the click goes to one no longer on the page and the window never hears it. So the
   * page decides at the press itself. A guard in the capture phase lets the press go down only on the node, and lets
   * its own click through only when that too lands inside the node; every other event of a missed press is
   * swallowed, so it does nothing at all and is aimed again where the node is now. A press that landed is never
   * repeated, and one that went down on a node the press itself took away, so that nothing is found where it was,
   * landed.
   */
  private async pressAt(finder: string, x: number, y: number): Promise<string | null> {
    await this.evaluate(`(() => {
      const node = (${finder});
      const aim = { down: null, judged: false, clicked: false, at: "nothing" };
      const name = (t) => (t instanceof Element ? t.tagName.toLowerCase() + [...t.classList].map((c) => "." + c).join("") : String(t));
      const inside = (e) => node !== null && node !== undefined && node.contains(e.target);
      const swallow = (e) => { e.preventDefault(); e.stopImmediatePropagation(); };
      const guard = (e) => {
        if (e.type === "pointerdown" || e.type === "mousedown") {
          if (aim.down === null) { aim.down = inside(e); aim.at = name(e.target); }
          if (!aim.down) swallow(e);
        } else if (e.type === "click") {
          // The press's own click is judged, once; a click the page makes of it (a label's for, a picker's
          // click()) is the page's, and passes whenever the press went down on the node.
          if (e.isTrusted && !aim.judged) {
            aim.judged = true;
            aim.clicked = aim.down === true && inside(e);
            if (!aim.clicked) { aim.at = "a click on " + name(e.target); swallow(e); }
          } else if (aim.down !== true) swallow(e);
        } else if (aim.down !== true) swallow(e);
      };
      const kinds = ["pointerdown", "mousedown", "pointerup", "mouseup", "click"];
      kinds.forEach((k) => window.addEventListener(k, guard, true));
      aim.off = () => kinds.forEach((k) => window.removeEventListener(k, guard, true));
      window.__e2eAim = aim;
      return true;
    })()`);
    const common = { x, y, button: "left", clickCount: 1, buttons: 1 };
    await this.command("Input.dispatchMouseEvent", { ...common, type: "mouseMoved", buttons: 0 });
    await this.command("Input.dispatchMouseEvent", { ...common, type: "mousePressed" });
    await this.command("Input.dispatchMouseEvent", { ...common, type: "mouseReleased", buttons: 0 });
    // A press that navigated took the guard with its document; a swallowed one cannot navigate, so that one landed.
    const missed = await this.evaluate<string | null>(`(() => {
      const aim = window.__e2eAim;
      if (aim === undefined) return null;
      aim.off();
      delete window.__e2eAim;
      if (aim.clicked) return null;
      // Down on the node and no click heard: the press itself took the node away (a completion taken on
      // pointerdown) only if nothing is found where it was; a node found is the page's redraw, so the press missed.
      const now = (${finder});
      return aim.down === true && (now === null || now === undefined) ? null : aim.at;
    })()`).catch(() => null);
    return missed === null ? null : `it moved under the press, which met ${missed} instead`;
  }

  /** The reader's own scroll: a mouse wheel turned over the middle of the viewport, as a page's input sees it. */
  async wheel(deltaY: number): Promise<void> {
    const [x, y] = await this.evaluate<[number, number]>(`[innerWidth / 2, innerHeight / 2]`);
    await this.command("Input.dispatchMouseEvent", { type: "mouseWheel", x, y, deltaX: 0, deltaY });
  }

  /** Until the page's scroll holds still across three frames. */
  async scrollSettled(): Promise<void> {
    await this.waitFor(`new Promise((done) => {
      const at = () => (document.scrollingElement ?? document.documentElement).scrollTop;
      const first = at();
      requestAnimationFrame(() => requestAnimationFrame(() => requestAnimationFrame(() => done(at() === first))));
    })`);
  }

  /** Focus a field and insert text, character events and all — so a page's
   * `input` listeners run exactly as they do under a thumb. */
  async type(selector: string, text: string): Promise<void> {
    const literal = JSON.stringify(selector);
    await this.evaluate(`(() => {
      const node = document.querySelector(${literal});
      if (node === null) throw new Error("no element matches " + ${literal});
      node.focus();
      return true;
    })()`);
    await this.command("Input.insertText", { text });
  }

  async press(key: string, modifiers: KeyModifiers = {}): Promise<void> {
    const spec = keySpec(key);
    const bits =
      (modifiers.alt === true ? ALT : 0) |
      (modifiers.ctrl === true ? CTRL : 0) |
      (modifiers.meta === true ? META : 0) |
      (modifiers.shift === true ? SHIFT : 0);
    // A modified key produces no character; sending one would type a letter
    // into the field the chord was meant to escape.
    const text = bits === 0 || bits === SHIFT ? spec.text : undefined;
    const base = { key: spec.key, code: spec.code, windowsVirtualKeyCode: spec.keyCode, modifiers: bits };
    await this.command("Input.dispatchKeyEvent", {
      ...base,
      type: text === undefined ? "rawKeyDown" : "keyDown",
      ...(text === undefined ? {} : { text }),
    });
    await this.command("Input.dispatchKeyEvent", { ...base, type: "keyUp" });
  }

  /**
   * The network, held at arm's length.
   *
   * Two things need it and they are one mechanism. `failRequests` is a backend
   * that is not there — the page keeps running on what it already has, which
   * is what the local-first cache and its banner are about. `holdRequests` is
   * a backend that has not ANSWERED YET, which is the only way to check
   * optimism without a race: with the act's POST paused at the network the
   * server has not seen it, so a card already gone from the list is gone
   * because the app decided it was, and the fold still says otherwise. Polling
   * for the gap between a click and a 5ms round trip is how a test comes out
   * confident on a laptop and flaky in CI — measured, on the rollback scene.
   */
  private async intercept(fragments: readonly string[], verdict: Verdict, stage: Stage = "Request"): Promise<void> {
    this.answer = verdict;
    this.paused = { fragments, stage };
    await this.fetchWhat();
  }

  /**
   * Pause what `intercept` and every running capture name, and nothing else: only what a scene names passes through
   * this process, so a page's other reads never wait on it. One `Fetch.enable` holds both sets, since each replaces the
   * last.
   */
  private async fetchWhat(): Promise<void> {
    this.listen();
    const at = (fragment: string, stage: Stage) => ({ urlPattern: `*${fragment.replace(/[*?\\]/g, "\\$&")}*`, requestStage: stage });
    const paused = this.paused === null ? [] : this.paused.fragments.map((f) => at(f, this.paused?.stage ?? "Request"));
    const captured = this.captures.flatMap((c) => c.fragments.map((f) => at(f, "Response")));
    await this.command("Fetch.enable", { patterns: [...paused, ...captured] });
  }

  private listen(): void {
    if (this.listening) return;
    this.listening = true;
    this.connection.on("Fetch.requestPaused", (params, session) => {
      const id = params["requestId"];
      if (session !== this.session || typeof id !== "string") return;
      const sent = params["request"];
      const url = typeof sent === "object" && sent !== null && "url" in sent ? String(sent.url) : "";
      const stage: Stage = "responseStatusCode" in params || "responseErrorReason" in params ? "Response" : "Request";
      const taking = stage === "Response" ? this.captures.filter((c) => c.fragments.some((f) => url.includes(f))) : [];
      void this.read(id, params, taking).then(() => this.decide(id, params, stage, url));
    });
  }

  /** A paused answer's body, for the captures that match it, before the page reads any of it. */
  private async read(id: string, params: Record<string, unknown>, taking: readonly Capture[]): Promise<void> {
    if (taking.length === 0) return;
    const status = Number(params["responseStatusCode"] ?? 0);
    const headers = Array.isArray(params["responseHeaders"]) ? (params["responseHeaders"] as { name?: string; value?: string }[]) : [];
    const streamed = headers.some((h) => h.name?.toLowerCase() === "content-type" && (h.value ?? "").startsWith("text/event-stream"));
    // A redirect's body is never the page's (Chromium keeps none to give): what the page follows is
    // its Location, so that is the answer read, and a value it carried is still seen.
    const location = headers.find((h) => h.name?.toLowerCase() === "location")?.value ?? "";
    const answer: Answer =
      "responseErrorReason" in params || NO_BODY.includes(status)
        ? { type: "Read", body: "" }
        : status >= 300 && status < 400
          ? { type: "Read", body: location }
          : streamed
            ? { type: "Unread", why: "a stream, which never ends whole" }
            : await this.command("Fetch.getResponseBody", { requestId: id }).then(
              (r): Answer => {
                const body = String(r["body"] ?? "");
                return { type: "Read", body: r["base64Encoded"] === true ? new TextDecoder().decode(Uint8Array.from(atob(body), (c) => c.charCodeAt(0))) : body };
              },
              (e: unknown): Answer => ({ type: "Unread", why: String(e) }),
            );
    const networkId = String(params["networkId"] ?? id);
    for (const c of taking) {
      c.answers.set(networkId, answer);
      if (answer.type === "Unread") c.unread(networkId);
    }
  }

  /** What `intercept`'s verdict does with a paused request it names; anything else goes on. */
  private decide(id: string, params: Record<string, unknown>, stage: Stage, url: string): void {
    const ours = this.paused !== null && this.paused.stage === stage && this.paused.fragments.some((f) => url.includes(f));
    const verdict = ours ? (this.answer ?? "continue") : "continue";
    if (verdict === "hold") {
      this.held.push(id);
      this.heldAt.push(url);
      return;
    }
    const sent = params["request"];
    const posted = typeof sent === "object" && sent !== null && "method" in sent && sent.method === "POST";
    if (verdict === "stale" && posted) {
      this.answer = "continue";
      const headers =
        typeof sent === "object" && sent !== null && "headers" in sent && typeof sent.headers === "object" && sent.headers !== null
          ? Object.entries(sent.headers)
              .filter(([name]) => name.toLowerCase() !== http.CLIENT_HEADER.toLowerCase())
              .map(([name, value]) => ({ name, value: String(value) }))
          : [];
      void this.command("Fetch.continueRequest", { requestId: id, headers }).catch(() => {});
      return;
    }
    // A lost answer is a gateway's 502 in its place: the box acted, the page hears nothing usable.
    const refused = { responseCode: 403, responseHeaders: [{ name: "content-type", value: "application/json" }], body: btoa(JSON.stringify({ error: this.refusal })) };
    const [method, args] =
      verdict === "fail"
        ? ["Fetch.failRequest", { errorReason: "ConnectionRefused" }]
        : verdict === "lose"
          ? ["Fetch.fulfillRequest", { responseCode: 502, body: "" }]
          : verdict === "refuse" && posted
            ? ["Fetch.fulfillRequest", refused]
            : ["Fetch.continueRequest", {}];
    void this.command(method, { requestId: id, ...args }).catch(() => {
      // A request whose page navigated away is no longer ours to answer.
    });
  }

  /**
   * Intercept nothing new, and keep the domain enabled. `Fetch.disable` abandons a response the
   * interceptor holds whose `requestPaused` has not reached this process yet: the request never
   * finishes (a lost-tap scene's Inbox reload hung on one for good). Enabled, the late event still
   * arrives and is let through.
   */
  private async narrow(): Promise<void> {
    this.answer = null;
    this.paused = null;
    await this.fetchWhat();
  }

  /**
   * Let every request whose URL contains one of these reach the box, and fail its answer on the way
   * back: the act happened and the page never hears of it.
   */
  async loseResponses(fragments: readonly string[]): Promise<void> {
    await this.intercept(fragments, "lose", "Response");
  }

  /** Send the next request whose URL contains one of these as a build older than the box takes would. */
  async sendNextAsStale(fragments: readonly string[]): Promise<void> {
    await this.intercept(fragments, "stale");
  }

  /** Answer every POST whose URL contains one of these with the box's 403 and `said`, never reaching the box. */
  async refuseRequests(fragments: readonly string[], said: string): Promise<void> {
    this.refusal = said;
    await this.intercept(fragments, "refuse");
  }

  /** Fail every request whose URL contains one of these. */
  async failRequests(fragments: readonly string[]): Promise<void> {
    await this.intercept(fragments, "fail");
  }

  /**
   * Put the network back. `holdRequests` returns its own undo because a held
   * request has to be let through by hand; a FAILED one is over, so undoing
   * `failRequests` is just this — and the queue tests need it, because the
   * whole thing they are about is what happens when a connection comes back.
   */
  async resumeRequests(): Promise<void> {
    await this.narrow();
  }

  /** Until the page has made `count` of the requests `holdRequests` is holding: an event the scene waits for, since
   * a request the page makes on its own (a pull after the box moved) has no other sign. */
  async untilHeld(count = 1, timeout = WAIT_MS): Promise<void> {
    const until = Date.now() + timeout * STRETCH;
    while (this.held.length < count) {
      if (Date.now() >= until) throw new Error(`waited for ${count} held request(s); ${this.held.length} arrived`);
      await sleep(50);
    }
  }

  /** The URLs of the requests `holdRequests` is holding, in the order they came: a worker's among them, which the
   * page's own entries do not list. */
  heldUrls(): readonly string[] {
    return [...this.heldAt];
  }

  /** Pause every request whose URL contains one of these. The returned
   * function lets them all through and stops intercepting. */
  async holdRequests(fragments: readonly string[]): Promise<() => Promise<void>> {
    await this.intercept(fragments, "hold");
    return async () => {
      await this.narrow();
      const waiting = this.held;
      this.held = [];
      this.heldAt = [];
      for (const id of waiting) {
        await this.command("Fetch.continueRequest", { requestId: id }).catch(() => {
          // Already gone; nothing to release.
        });
      }
    };
  }

  /**
   * RECORD EVERY REQUEST AND RESPONSE BODY MATCHING ONE OF `fragments`: each
   * request over the `Network` domain, each answer's body over `Fetch`, paused
   * at its headers, where Chromium holds the whole body before the page reads
   * any of it, and let through. `Network.getResponseBody` cannot be asked: a
   * response the page has read whole can still end as `net::ERR_ABORTED`, its
   * body then gone ("No data found for resource"), and an answer read as
   * nothing passes a scene that asserts no answer carried a secret. A body
   * that cannot be read fails the capture by its URL, and `stop` waits for
   * no more of that request than its headers; a request that got no answer
   * has none. `stop` waits for every other request to end and returns them.
   *
   * This is the "not only the DOM" half of a scene like settings' secret
   * write: the DOM only ever shows what the page CHOSE to render, and a
   * route that leaked a value into a field nobody points at would still
   * pass a DOM-only assertion.
   */
  async captureNetwork(
    fragments: readonly string[],
  ): Promise<() => Promise<{ url: string; requestBody: string; responseBody: string }[]>> {
    interface Entry {
      url: string;
      requestId: string;
      requestBody: string;
      finished: Promise<void>;
      resolve: () => void;
    }
    const byId = new Map<string, Entry>();
    const capture: Capture = { fragments, answers: new Map(), unread: (networkId) => byId.get(networkId)?.resolve() };
    const matches = (url: string): boolean => fragments.some((fragment) => url.includes(fragment));
    let running = true;
    const onRequest = (params: Record<string, unknown>, session?: string): void => {
      const requestId = params["requestId"];
      const request = params["request"] as
        | { url?: string; postData?: string; hasPostData?: boolean }
        | undefined;
      const url = request?.url ?? "";
      if (!running || session !== this.session || typeof requestId !== "string" || !matches(url)) return;
      // A redirect's next hop comes under the same id, and the request still ends once: the entry `stop` may already
      // be waiting on is the one that hears it.
      const held = byId.get(requestId);
      if (held !== undefined) {
        held.url = url;
        return;
      }
      let resolve = (): void => {};
      const finished = new Promise<void>((done) => {
        resolve = done;
      });
      byId.set(requestId, { url, requestId, requestBody: request?.postData ?? "", finished, resolve });
      // A body over CDP's inline threshold arrives as `hasPostData` with no
      // `postData` — `Network.getRequestPostData` is the fetch for it.
      if (request?.hasPostData === true && (request.postData ?? "") === "") {
        void this.command("Network.getRequestPostData", { requestId })
          .then((result) => {
            const entry = byId.get(requestId);
            if (entry !== undefined) entry.requestBody = String(result["postData"] ?? "");
          })
          .catch(() => {
            // Gone before it could be asked for; the inline field (if any) stands.
          });
      }
    };
    const onFinished = (params: Record<string, unknown>, session?: string): void => {
      const requestId = params["requestId"];
      if (session !== this.session || typeof requestId !== "string") return;
      byId.get(requestId)?.resolve();
    };
    this.connection.on("Network.requestWillBeSent", onRequest);
    this.connection.on("Network.loadingFinished", onFinished);
    this.connection.on("Network.loadingFailed", onFinished);
    await this.command("Network.enable", {});
    this.captures.push(capture);
    await this.fetchWhat();
    return async () => {
      await Promise.all([...byId.values()].map((entry) => entry.finished));
      running = false;
      this.captures = this.captures.filter((c) => c !== capture);
      await this.fetchWhat();
      await this.command("Network.disable", {});
      return [...byId.values()].map(({ url, requestId, requestBody }) => {
        const answer = capture.answers.get(requestId) ?? { type: "Read", body: "" };
        if (answer.type === "Unread") throw new Error(`${url}: its answer's body could not be read (${answer.why})`);
        return { url, requestBody, responseBody: answer.body };
      });
    };
  }

  /**
   * WHICH THEME THE PAGE BELIEVES THE READER'S MACHINE ASKED FOR.
   *
   * The stylesheet's dark half is behind `prefers-color-scheme`, which is the
   * machine's setting and not somewhere a page can be navigated — so a scene
   * that wants to see the dark palette emulates the media feature rather than
   * clicking something. `null` hands the browser's own answer back.
   *
   * The app's OWN override (the More sheet's theme row) writes `data-theme` on
   * the root instead, and that one is clickable. The two are the same question
   * asked of two different authorities, which is why both are worth a picture.
   */
  async colorScheme(scheme: "light" | "dark" | null): Promise<void> {
    await this.command("Emulation.setEmulatedMedia", {
      features: scheme === null ? [] : [{ name: "prefers-color-scheme", value: scheme }],
    });
  }

  /** The scheme and whether the reader asked for less motion, as one setting: each call replaces the last. */
  async readerMedia(scheme: "light" | "dark", motion: "reduce" | "no-preference"): Promise<void> {
    await this.command("Emulation.setEmulatedMedia", {
      features: [
        { name: "prefers-color-scheme", value: scheme },
        { name: "prefers-reduced-motion", value: motion },
      ],
    });
  }

  /** `source` runs first in every document this page loads from now on; the answer stops that. */
  async beforeEveryDocument(source: string): Promise<() => Promise<void>> {
    const added = await this.command("Page.addScriptToEvaluateOnNewDocument", { source });
    const identifier = String(added["identifier"]);
    return async () => {
      await this.command("Page.removeScriptToEvaluateOnNewDocument", { identifier });
    };
  }

  /**
   * Who answered each response whose URL ends with one of `suffixes`, from now until the answer is
   * called: `"network"` for the box, `"cache-storage"` for the worker's cache, `"http-cache"` for the
   * browser's, and the worker's own fetch as `"worker-network"`.
   */
  async answeredBy(suffixes: readonly string[]): Promise<() => { url: string; by: string }[]> {
    const seen: { url: string; by: string }[] = [];
    let open = true;
    this.connection.on("Network.responseReceived", (params) => {
      const r = params["response"] as
        | { url?: string; fromServiceWorker?: boolean; serviceWorkerResponseSource?: string; fromDiskCache?: boolean }
        | undefined;
      const url = r?.url ?? "";
      if (!open || !suffixes.some((s) => url.endsWith(s))) return;
      const by =
        r?.fromServiceWorker === true
          ? r.serviceWorkerResponseSource === "cache-storage" ? "cache-storage" : "worker-network"
          : r?.fromDiskCache === true ? "http-cache" : "network";
      seen.push({ url, by });
    });
    await this.command("Network.enable", {});
    return () => {
      open = false;
      return seen;
    };
  }

  /** Each response whose URL holds `part`, with its status, from now on, as far as the answer is called: the page's and
   * its workers' alike, since the core's worker pulls the stores. */
  async statuses(part: string): Promise<() => { url: string; status: number }[]> {
    const seen: { url: string; status: number }[] = [];
    this.connection.on("Network.responseReceived", (params) => {
      const r = params["response"] as { url?: string; status?: number } | undefined;
      const url = r?.url ?? "";
      if (url.includes(part)) seen.push({ url, status: r?.status ?? 0 });
    });
    await this.workersToo();
    await this.command("Network.enable", {});
    return () => [...seen];
  }

  /** Every worker the page starts from now on attached, its network reported as the page's is, and the page's cache
   * and CPU as `cold` and `throttle` last set them, so a scene measures and starves the core's worker as the page. */
  async attachWorkers(): Promise<void> {
    await this.workersToo();
  }

  private async workersToo(): Promise<void> {
    if (this.attaching) return;
    this.attaching = true;
    this.connection.on("Target.attachedToTarget", (params, session) => {
      const info = params["targetInfo"] as { type?: string } | undefined;
      const child = params["sessionId"];
      if (session !== this.session || typeof child !== "string" || info?.type !== "worker") return;
      this.workers.add(child);
      void this.connection.send("Network.enable", {}, child).catch(() => {});
      // As it starts: what it does before these land runs warm and unthrottled.
      void this.toWorker(child);
      void this.connection.send("Runtime.runIfWaitingForDebugger", {}, child).catch(() => {});
    });
    this.connection.on("Target.detachedFromTarget", (params) => {
      const child = params["sessionId"];
      if (typeof child === "string") this.workers.delete(child);
    });
    await this.command("Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: false, flatten: true });
  }

  /** The cache and the CPU a worker shares with the page, as last set. */
  private async toWorker(child: string): Promise<void> {
    await this.connection.send("Network.setCacheDisabled", { cacheDisabled: this.coldOn }, child).catch(() => {});
    await this.connection.send("Emulation.setCPUThrottlingRate", { rate: this.rate }, child).catch(() => {});
  }

  /**
   * Everything the page said and fetched from now on, each line stamped with milliseconds since the journal began:
   * a request sent, its status, its end (or failure), a console line, and whatever `mark` a scene adds between its
   * steps. A scene that failed under load reads it beside its DOM, so the order of what happened is evidence.
   */
  async journal(): Promise<Journal> {
    const t0 = Date.now();
    const lines: string[] = [];
    const add = (line: string): void => void lines.push(`${String(Date.now() - t0).padStart(6)} ${line}`);
    const urls = new Map<string, string>();
    // What was sent and has neither finished nor failed: a stream stays here for its whole life.
    const open = new Map<string, string>();
    const ended = (id: string): string => {
      open.delete(id);
      return urls.get(id) ?? "?";
    };
    this.connection.on("Network.requestWillBeSent", (p) => {
      const r = p["request"] as { url?: string; method?: string } | undefined;
      const url = (r?.url ?? "").replace(/^https?:\/\/[^/]+/, "");
      urls.set(String(p["requestId"]), url);
      open.set(String(p["requestId"]), `${r?.method ?? "?"} ${url}`);
      add(`> ${r?.method ?? "?"} ${url}`);
    });
    this.connection.on("Network.responseReceived", (p) => {
      const r = p["response"] as { status?: number } | undefined;
      add(`< ${r?.status ?? "?"} ${urls.get(String(p["requestId"])) ?? "?"}`);
    });
    this.connection.on("Network.loadingFinished", (p) => add(`. done ${ended(String(p["requestId"]))}`));
    this.connection.on("Network.loadingFailed", (p) =>
      add(`! ${String(p["errorText"] ?? "failed")} ${ended(String(p["requestId"]))}`),
    );
    this.connection.on("Network.eventSourceMessageReceived", (p) =>
      add(`~ ${urls.get(String(p["requestId"])) ?? "?"} ${String(p["data"] ?? "")}`),
    );
    this.connection.on("Runtime.consoleAPICalled", (p) => {
      const args = (p["args"] as { value?: unknown; description?: string }[] | undefined) ?? [];
      add(`console.${String(p["type"])} ${args.map((a) => (a.value !== undefined ? String(a.value) : (a.description ?? ""))).join(" ")}`);
    });
    this.connection.on("Runtime.exceptionThrown", (p) => {
      const d = p["exceptionDetails"] as { exception?: { description?: string }; text?: string } | undefined;
      add(`exception ${d?.exception?.description ?? d?.text ?? ""}`);
    });
    await this.command("Network.enable", {});
    return { mark: (step) => add(`# ${step}`), lines: () => [...lines], inFlight: () => [...open.values()].sort() };
  }

  /** Every service worker stopped, as a browser stops an idle one: whatever it held in memory is gone. */
  async stopWorkers(): Promise<void> {
    await this.command("ServiceWorker.enable", {});
    await this.command("ServiceWorker.stopAllWorkers", {});
  }

  /** The page's main thread sampled from now until the returned stop, which answers DevTools' CPU profile. */
  async profile(intervalUs = 200): Promise<() => Promise<CpuProfile>> {
    await this.command("Profiler.enable", {});
    await this.command("Profiler.setSamplingInterval", { interval: intervalUs });
    await this.command("Profiler.start", {});
    return async () => (await this.command("Profiler.stop", {}))["profile"] as CpuProfile;
  }

  /** The storage quota `origin` is given, as `navigator.storage.estimate()` reads it; `null` is the browser's own again. */
  async quota(origin: string, bytes: number | null): Promise<void> {
    await this.command("Storage.overrideQuotaForOrigin", bytes === null ? { origin } : { origin, quotaSize: bytes });
  }

  /** A phone-class CPU: Chromium runs this page's main thread `rate` times slower (1 is none). */
  async throttle(rate: number): Promise<void> {
    this.rate = rate;
    await this.command("Emulation.setCPUThrottlingRate", { rate });
    await Promise.all([...this.workers].map((w) => this.toWorker(w)));
  }

  /** A phone-class network, or the box's own again with `null`: each request waits `latency` ms and moves at the throughputs, in bytes a second. */
  async network(
    conditions: { offline: boolean; latency: number; downloadThroughput: number; uploadThroughput: number } | null,
  ): Promise<void> {
    await this.command("Network.enable", {});
    await this.command(
      "Network.emulateNetworkConditions",
      conditions ?? { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 },
    );
  }

  /** A first visit, every time: no HTTP cache and no service worker between this page and the box. */
  async cold(on: boolean): Promise<void> {
    this.coldOn = on;
    await this.command("Network.enable", {});
    await this.command("Network.setCacheDisabled", { cacheDisabled: on });
    await this.command("Network.setBypassServiceWorker", { bypass: on });
    await Promise.all([...this.workers].map((w) => this.toWorker(w)));
  }

  /** A full-page PNG at `path`. The picture is the deliverable when the reader
   * is a person; the assertions above are the deliverable when it is CI. */
  async screenshot(path: string): Promise<void> {
    const result = await this.command("Page.captureScreenshot", { format: "png" });
    writeFileSync(path, String(result["data"]), "base64");
  }
}

/** DevTools' CPU profile: each node a frame under its parent, and the samples' nodes with the microseconds between them. */
export interface CpuProfile {
  readonly nodes: readonly {
    readonly id: number;
    readonly callFrame: { readonly functionName: string; readonly url: string; readonly lineNumber: number; readonly columnNumber: number };
    readonly children?: readonly number[];
  }[];
  readonly samples: readonly number[];
  readonly timeDeltas: readonly number[];
}

/** What `Page.journal` records: a scene's own marks between the page's lines, in the order they happened. */
export interface Journal {
  readonly mark: (step: string) => void;
  readonly lines: () => string[];
  /** The requests sent and not yet finished or failed, a stream among them for as long as it is open. A request a
   * reload cut off stays listed, since CDP reports no end for it. */
  readonly inFlight: () => string[];
}

export interface Browser {
  readonly page: Page;
  /** Another tab of the same browser, its profile and storage shared: what one tab of a device does, the others hear. */
  tab: () => Promise<Page>;
  close: () => Promise<void>;
}

/** Where chromium wrote the port it chose. Two lines: the port, then the
 * browser target's path. */
async function devtoolsUrl(profile: string, child: ChildProcess, stderr: () => string): Promise<string> {
  const file = join(profile, "DevToolsActivePort");
  // Scaled like every other wait: a cold runner starts a browser slowly too.
  const until = Date.now() + Math.round(20000 * STRETCH);
  for (;;) {
    if (existsSync(file)) {
      const lines = readFileSync(file, "utf8").split("\n");
      const port = lines[0]?.trim() ?? "";
      const path = lines[1]?.trim() ?? "";
      if (port !== "" && path !== "") return `ws://127.0.0.1:${port}${path}`;
    }
    if (Date.now() >= until) {
      child.kill("SIGKILL");
      throw new Error(`chromium never wrote ${file}\n  its stderr said:\n${stderr()}`);
    }
    await sleep(50);
  }
}

/** Launch one browser with one page attached. */
export async function launch(): Promise<Browser> {
  const profile = mkdtempSync(join(tmpdir(), "suzatary-e2e-profile-"));
  const child = spawn(chromiumBinary(), flags(profile), { stdio: ["ignore", "pipe", "pipe"] });
  let said = "";
  child.stderr?.on("data", (chunk) => {
    said += chunk.toString();
  });
  child.on("error", (error: Error) => {
    said += `\n${error.message}`;
  });

  const connection = await Connection.open(await devtoolsUrl(profile, child, () => said));
  // Each in a window of its own: a background tab is hidden, and a hidden page draws nothing on its frames.
  const tab = async (): Promise<Page> => {
    const target = await connection.send("Target.createTarget", { url: "about:blank", newWindow: true });
    const attached = await connection.send("Target.attachToTarget", {
      targetId: target["targetId"],
      flatten: true,
    });
    const session = String(attached["sessionId"]);
    await connection.send("Page.enable", {}, session);
    await connection.send("Runtime.enable", {}, session);
    return new Page(connection, session);
  };
  const page = await tab();

  return {
    page,
    tab,
    close: async () => {
      try {
        await connection.send("Browser.close");
      } catch {
        // A browser that has already gone is a browser that is closed.
      }
      connection.close();
      child.kill("SIGKILL");
      await gone(child);
      await discard(profile);
    },
  };
}

/**
 * ⚠️ A SIGNAL IS A REQUEST, NOT AN EVENT (CI, 2026-09-10: `ENOTEMPTY,
 * Directory not empty: …/suzatary-e2e-profile-…`).
 *
 * `child.kill` returns as soon as the signal is DELIVERED, and Chromium is a
 * tree — a browser process, a zygote, a renderer per tab, a crashpad handler —
 * each of which may still be flushing a Leveldb file into the profile when the
 * parent has already gone. Removing the directory in the same tick therefore
 * races children that are still creating entries inside it, and `rm -rf` on a
 * directory being written into fails with ENOTEMPTY rather than with anything
 * that names the cause. It passed on this box for weeks and failed on a loaded
 * runner, which is what a race looks like.
 *
 * So: wait for the process this file spawned to be REAPED, then remove. `exit`
 * fires for a killed child too, and a child that has already exited will never
 * fire it again — hence the `exitCode`/`signalCode` check before subscribing,
 * and a bounded wait so a wedged process cannot hang the suite's teardown.
 */
async function gone(child: ChildProcess, timeout = Math.round(5000 * STRETCH)): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return;
  await new Promise<void>((done) => {
    const finish = (): void => {
      clearTimeout(timer);
      done();
    };
    const timer = setTimeout(finish, timeout);
    child.once("exit", finish);
  });
}

/**
 * And remove with RETRIES, because the grandchildren are not ours to wait for:
 * a crashpad handler is re-parented to init and this process never learns when
 * it is done. A few attempts a beat apart covers the flush; a profile that
 * still will not go is a temp directory, so saying so and moving on is better
 * than failing a green suite on its own cleanup.
 */
async function discard(profile: string, attempts = 5): Promise<void> {
  for (let attempt = 1; ; attempt += 1) {
    try {
      rmSync(profile, { recursive: true, force: true });
      return;
    } catch (error: unknown) {
      if (attempt >= attempts) {
        console.warn(`cdp: left ${profile} behind (${error instanceof Error ? error.message : String(error)})`);
        return;
      }
      await sleep(Math.round(attempt * 100 * STRETCH));
    }
  }
}
