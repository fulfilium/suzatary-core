/**
 * A browser driven over the DevTools Protocol with no dependency: Chromium speaks it over a WebSocket and node has a
 * global `WebSocket`. `waitFor` polls `evaluate` in real time because the app holds an SSE connection to `/events`,
 * and an open stream means Chromium's virtual-time budgets never expire. Reads go through `evaluate`, and the
 * assertions are the tests'.
 */

import { spawn } from "node:child_process";
import type { ChildProcess } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, rmdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import http from "../src/wire/http";
import { BASELINE_MS, WAIT_MS } from "./budget";

const sleep = (ms: number): Promise<void> => new Promise((done) => setTimeout(done, ms));

export { WAIT_MS };
const STRETCH = WAIT_MS / BASELINE_MS;

/** How long the harness waits for one answer it cannot see coming: four wait budgets. */
export const ANSWER_MS = 4 * WAIT_MS * STRETCH;

/** `promise`, or a failure naming `what` once `ms` pass without it settling. */
async function within<T>(promise: Promise<T>, ms: number, what: string): Promise<T> {
  let stop: ReturnType<typeof setTimeout> | undefined;
  const late = new Promise<never>((_, no) => {
    stop = setTimeout(() => no(new Error(`waited ${Math.round(ms)} ms for ${what}, which did not settle`)), ms);
  });
  return Promise.race([promise, late]).finally(() => clearTimeout(stop));
}

function chromiumBinary(): string {
  return process.env["SUZATARY_CHROMIUM"] ?? "chromium";
}

/**
 * A slow device's whole CPU: `SUZATARY_E2E_BROWSER_CPU` cores for the browser and every process it starts, as a cgroup
 * of its own; `Emulation.setCPUThrottlingRate` slows a page's main thread alone and Chromium refuses it for a worker.
 * A machine whose cgroups this user cannot write refuses the run rather than measuring an unslowed browser.
 */
function cpuCap(): string | null {
  const cores = process.env["SUZATARY_E2E_BROWSER_CPU"];
  if (cores === undefined || cores === "") return null;
  const share = Number(cores);
  if (!(share > 0)) throw new Error(`SUZATARY_E2E_BROWSER_CPU is a count of cores above 0, not '${cores}'`);
  const period = 100000;
  const quota = Math.round(share * period);
  // cgroup v2 names the controller in one file; v1 mounts it as a directory of its own.
  const v2 = existsSync("/sys/fs/cgroup/cgroup.controllers");
  const group = join(v2 ? "/sys/fs/cgroup" : "/sys/fs/cgroup/cpu", `suzatary-e2e-${process.pid}`);
  try {
    mkdirSync(group, { recursive: true });
    if (v2) {
      writeFileSync(join(group, "cpu.max"), `${quota} ${period}`);
    } else {
      writeFileSync(join(group, "cpu.cfs_period_us"), String(period));
      writeFileSync(join(group, "cpu.cfs_quota_us"), String(quota));
    }
  } catch (error: unknown) {
    throw new Error(`SUZATARY_E2E_BROWSER_CPU needs a cgroup this user can write (${String(error)})`);
  }
  return group;
}

/**
 * The flags: `--headless=new` is the shipped browser (the old headless was a separate implementation); `--no-sandbox`
 * because a CI container may not grant the kernel namespaces the sandbox needs; `--disable-dev-shm-usage` because a CI
 * container's /dev/shm is 64 MB and a renderer that runs out of it dies silently; `--font-render-hinting=none` because
 * a hinted glyph can change a line count and so a clamped card's height.
 */
/** Whether the browser draws classic scrollbars, which take width from the page as on desktop Linux and Windows. The
 * suites hide them; a scene about the room a scrollbar leaves launches a browser of its own that shows them. */
export type Scrollbars = "Hidden" | "Shown";

function flags(profile: string, scrollbars: Scrollbars = "Hidden"): string[] {
  return [
    "--headless=new",
    "--no-sandbox",
    "--disable-gpu",
    "--disable-dev-shm-usage",
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-extensions",
    "--disable-background-networking",
    // A document kept in the back/forward cache keeps its event streams open against Chromium's six sockets per host,
    // and every later request queues forever.
    "--disable-features=BackForwardCache",
    // A backgrounded renderer throttles its timers, and every wait here is a real-time poll.
    "--disable-background-timer-throttling",
    "--disable-backgrounding-occluded-windows",
    "--disable-renderer-backgrounding",
    ...(scrollbars === "Hidden" ? ["--hide-scrollbars"] : []),
    "--force-device-scale-factor=1",
    "--font-render-hinting=none",
    `--user-data-dir=${profile}`,
    // 0: the kernel picks, and DevToolsActivePort in the profile says which, so two runs on one box never collide.
    "--remote-debugging-port=0",
    // The fixture servers' test certificate is trusted by the hash of its key alone, so any other still fails.
    ...((process.env["SUZATARY_E2E_SPKI"] ?? "") === "" ? [] : [`--ignore-certificate-errors-spki-list=${process.env["SUZATARY_E2E_SPKI"]}`]),
    // V8's own switches for a measurement (`--no-wasm-lazy-compilation`, say); no scene sets it.
    ...((process.env["SUZATARY_E2E_JS_FLAGS"] ?? "") === "" ? [] : [`--js-flags=${process.env["SUZATARY_E2E_JS_FLAGS"]}`]),
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

  /** One command; `sessionId` makes a page command reach the page rather than the browser (flat attachment). */
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

/** The named keys these tests press, each because a shortcut in `web/src/keys.ts` uses it. */
const NAMED: Record<string, KeySpec> = {
  Enter: { key: "Enter", code: "Enter", keyCode: 13, text: "\r" },
  Escape: { key: "Escape", code: "Escape", keyCode: 27 },
  Tab: { key: "Tab", code: "Tab", keyCode: 9, text: "\t" },
  ArrowDown: { key: "ArrowDown", code: "ArrowDown", keyCode: 40 },
  ArrowUp: { key: "ArrowUp", code: "ArrowUp", keyCode: 38 },
  ArrowLeft: { key: "ArrowLeft", code: "ArrowLeft", keyCode: 37 },
  ArrowRight: { key: "ArrowRight", code: "ArrowRight", keyCode: 39 },
  Home: { key: "Home", code: "Home", keyCode: 36 },
  End: { key: "End", code: "End", keyCode: 35 },
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
  /** Milliseconds, defaulting to `WAIT_MS`, the machine's budget. */
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
 * One page: every method takes a CSS selector or a JavaScript expression, never a statement, since `evaluate` hands
 * the string to `Runtime.evaluate`.
 */
/** A network as Chromium emulates it: each request waits `latency` ms and moves at the throughputs, in bytes a second. */
export type Conditions = { readonly offline: boolean; readonly latency: number; readonly downloadThroughput: number; readonly uploadThroughput: number };
const NO_CONDITIONS: Conditions = { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 };

/** One request as it ended: its path, who made it, when it was sent, answered and ended (epoch ms), its status (0 where
 * none came), the protocol it went over, the bytes it moved on the wire and its body's bytes once decoded. */
export type Wired = {
  readonly url: string;
  readonly from: "page" | "worker";
  readonly sent: number;
  readonly answered: number;
  readonly ended: number;
  readonly status: number;
  readonly protocol: string;
  readonly bytes: number;
  readonly decoded: number;
};

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
  /** What runs in every dedicated worker as it starts, before its own script (`beforeEveryWorker`). */
  private readonly workerScripts = new Set<string>();
  /** Each attached worker's session, with the URL of its script, oldest first. */
  private readonly workers = new Map<string, string>();
  private coldOn = false;
  /** The network `network` last set, which every worker attached later is given too; null is the box's own. */
  private networkOn: Conditions | null = null;

  /** The sentence `refuse` answers with. */
  private refusal = "";

  /** The trace `trace` is recording, and what ends its stop; null when none is. */
  private tracing: { readonly events: TraceEvent[]; readonly ended: () => void } | null = null;

  constructor(
    private readonly connection: Connection,
    private readonly session: string,
  ) {
    connection.on("Tracing.dataCollected", (params, from) => {
      if (from === session) this.tracing?.events.push(...(params["value"] as TraceEvent[]));
    });
    connection.on("Tracing.tracingComplete", (_, from) => {
      if (from === session) this.tracing?.ended();
    });
  }

  private async command(method: string, params: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
    return within(this.connection.send(method, params, this.session), ANSWER_MS, `the DevTools command ${method}`);
  }

  async close(): Promise<void> {
    await this.command("Page.close");
  }

  /** CSS pixels the page believes it has; the app resolves its route against `window.innerWidth`. */
  async viewport(width: number, height: number): Promise<void> {
    await this.command("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor: 1,
      mobile: false,
    });
  }

  /**
   * Go somewhere and wait until the location is the one asked for and the document is complete: a hash route fires no
   * load event, and a full navigation publishes its href only at commit, when `readyState` is "loading".
   * ⚠️ A fragment change is made from inside the page, since after an act on the write fixture, a full load elsewhere,
   * then a fragment-only `Page.navigate`, every later request of the page stayed unresolved.
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
      // ⚠️ A cross-origin load goes through `about:blank` first, which tears the outgoing document and its event
      // streams down, so the per-host connection pool cannot fill with connections no page will release.
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

  /** A real reload; the sentinel tells the fresh document from the old, since a reload does not change `location.href`. */
  async reload(): Promise<void> {
    await this.evaluate(`window.__e2eReloading = true`);
    await this.command("Page.reload", {});
    await this.waitFor(`window.__e2eReloading === undefined && document.readyState === "complete" && ${Page.STARTED}`, {
      timeout: 20000,
    });
  }

  /** A JavaScript expression evaluated in the page and returned by value; a throw, or a promise not settled within
   * `timeout`, throws here naming the expression. */
  async evaluate<T>(expression: string, timeout = ANSWER_MS): Promise<T> {
    return this.evaluateIn<T>(this.session, expression, timeout);
  }

  private async evaluateIn<T>(session: string, expression: string, timeout = ANSWER_MS): Promise<T> {
    const result = await within(
      this.connection.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true, userGesture: true }, session),
      timeout,
      `a promise evaluated in the page\n  expression: ${expression}`,
    );
    const failure = result["exceptionDetails"] as { exception?: { description?: string }; text?: string } | undefined;
    if (failure !== undefined) {
      const said = failure.exception?.description ?? failure.text ?? "unknown";
      throw new Error(`evaluate failed: ${said}\n  expression: ${expression}`);
    }
    return (result["result"] as { value: T }).value;
  }

  /**
   * Poll until the expression is truthy. ⚠️ The expression may be a promise: `Promise.resolve(…).then(Boolean)` awaits
   * it, where a bare `Boolean(aPromise)` is true on the first poll.
   */
  async waitFor(expression: string, options: WaitOptions = {}): Promise<void> {
    // `STRETCH` moves every scene's own number onto this machine, and the stretched wait is then floored at a quarter of
    // `WAIT_MS`, since a loaded box's jitter eats the shortest waits, so an idle box never sees a scene's own number move.
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
        // A navigation mid-poll destroys the context, so a throw is retried until the timeout.
        last = error instanceof Error ? error.message : String(error);
      }
      if (Date.now() >= until) throw new Error(`waited ${timeout}ms for ${expression}\n  last: ${last}`);
      await sleep(interval);
    }
  }

  /** The box of the Element `finder` evaluates to, after bringing it into view. */
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

  /** A real mouse click at the element's centre, never `element.click()`, which would skip hit testing. */
  async click(selector: string): Promise<void> {
    await this.clickNode(`document.querySelector(${JSON.stringify(selector)})`);
  }

  /** Click the first element matching `selector` whose visible text is `text`. */
  async clickText(selector: string, text: string): Promise<void> {
    await this.clickNode(
      `[...document.querySelectorAll(${JSON.stringify(selector)})].find((node) => node.textContent.trim() === ${JSON.stringify(text)})`,
    );
  }

  /**
   * Click whatever an expression yields once its box holds still for two reads 90 ms apart, since a control in a
   * transition (the add sheet slides up over 220ms) is in a position it is about to leave; the press counts only where
   * it lands on the thing (`pressAt`).
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
   * did, else what it met. A capture-phase guard swallows every event of a missed press, so it does nothing and is
   * aimed again; a press that landed is never repeated, and one that went down on a node the press itself took away
   * (nothing is found where it was) counts as landed.
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

  /** The page's DOM nodes and event listeners, and its JavaScript heap in bytes, read after a collection. */
  async counts(): Promise<{ nodes: number; listeners: number; heap: number }> {
    await this.command("HeapProfiler.collectGarbage");
    const dom = await this.command("Memory.getDOMCounters");
    const heap = await this.command("Runtime.getHeapUsage");
    return { nodes: Number(dom["nodes"]), listeners: Number(dom["jsEventListeners"]), heap: Number(heap["usedSize"]) };
  }

  /** One step of a press: a mouse's left button, or a finger, going down, moving held, or let go at `at`. */
  async pointer(step: "Down" | "Move" | "Up", at: readonly [number, number], by: "Mouse" | "Finger"): Promise<void> {
    const [x, y] = at;
    if (by === "Mouse") {
      const common = { x, y, button: "left", clickCount: 1 };
      if (step === "Down") await this.command("Input.dispatchMouseEvent", { ...common, type: "mouseMoved", button: "none", buttons: 0 });
      const type = step === "Down" ? "mousePressed" : step === "Move" ? "mouseMoved" : "mouseReleased";
      await this.command("Input.dispatchMouseEvent", { ...common, type, buttons: step === "Up" ? 0 : 1 });
    } else {
      const type = step === "Down" ? "touchStart" : step === "Move" ? "touchMove" : "touchEnd";
      await this.command("Input.dispatchTouchEvent", { type, touchPoints: step === "Up" ? [] : [{ x, y, id: 1 }] });
    }
  }

  /** A press at the path's first point, moved through the rest and let go at the last. */
  async drag(path: readonly (readonly [number, number])[], by: "Mouse" | "Finger"): Promise<void> {
    const [first, ...rest] = path;
    if (first === undefined) return;
    await this.pointer("Down", first, by);
    for (const p of rest) await this.pointer("Move", p, by);
    await this.pointer("Up", path[path.length - 1] ?? first, by);
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

  /** Focus a field and insert text, so a page's `input` listeners run as they do under a thumb. */
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
    // A modified key produces no character; sending one would type a letter into the field.
    const text = bits === 0 || bits === SHIFT ? spec.text : undefined;
    const base = { key: spec.key, code: spec.code, windowsVirtualKeyCode: spec.keyCode, modifiers: bits };
    await this.command("Input.dispatchKeyEvent", {
      ...base,
      type: text === undefined ? "rawKeyDown" : "keyDown",
      ...(text === undefined ? {} : { text }),
    });
    await this.command("Input.dispatchKeyEvent", { ...base, type: "keyUp" });
  }

  /** Intercept what `fragments` name with `verdict`; holding a POST is the only race-free check of optimism. */
  private async intercept(fragments: readonly string[], verdict: Verdict, stage: Stage = "Request"): Promise<void> {
    this.answer = verdict;
    this.paused = { fragments, stage };
    await this.fetchWhat();
  }

  /**
   * Pause what `intercept` and every running capture name, and nothing else, so a page's other reads never wait on this
   * process; one `Fetch.enable` holds both sets, since each replaces the last.
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
    // Chromium keeps no body for a redirect, so its Location is the answer read.
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
   * Intercept nothing new, and keep the domain enabled: `Fetch.disable` abandons a held response whose `requestPaused`
   * has not reached this process yet, and that request never finishes.
   */
  private async narrow(): Promise<void> {
    this.answer = null;
    this.paused = null;
    await this.fetchWhat();
  }

  /** Let every request whose URL contains one of these reach the box, and fail its answer on the way back. */
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

  /** Put the network back; `holdRequests` returns its own undo, since a held request is let through by hand. */
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

  /** Pause every request whose URL contains one of these; the returned function lets them through and stops. */
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
   * Record every request and response body matching one of `fragments`, each answer's body read over `Fetch` paused at
   * its headers, since `Network.getResponseBody` loses the body of a response that ends as `net::ERR_ABORTED`, and an
   * answer read as nothing would pass a scene that asserts no answer carried a secret. A body that cannot be read fails
   * the capture by its URL; `stop` waits for every other request to end and returns them.
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
      // A redirect's next hop comes under the same id, and the request still ends once.
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
      // A body over CDP's inline threshold arrives as `hasPostData` with no `postData`.
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

  /** Which theme the page believes the machine asked for, by emulating `prefers-color-scheme`; `null` hands the browser's
   * own answer back. */
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

  /** Every worker the page starts from now on attached, its network reported as the page's is, and the page's cache as
   * `cold` last set it. */
  async attachWorkers(): Promise<void> {
    await this.workersToo();
  }

  /** `expression` evaluated in the newest running dedicated worker whose script's URL holds `part`, in its own scope. */
  async inWorker<T>(part: string, expression: string): Promise<T> {
    await this.workersToo();
    const session = [...this.workers].filter(([, url]) => url.includes(part)).at(-1)?.[0];
    if (session === undefined) throw new Error(`no worker whose URL holds ${part} runs`);
    return this.evaluateIn<T>(session, expression);
  }

  /**
   * `source` evaluated in every dedicated worker the page starts from now on, before the worker's own script runs; the
   * answer stops that. Every target the page starts is held at its start until then, and let go.
   */
  async beforeEveryWorker(source: string): Promise<() => Promise<void>> {
    this.workerScripts.add(source);
    await this.workersToo();
    await this.command("Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: true, flatten: true });
    return async () => {
      this.workerScripts.delete(source);
      if (this.workerScripts.size === 0) await this.command("Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: false, flatten: true });
    };
  }

  private async workersToo(): Promise<void> {
    if (this.attaching) return;
    this.attaching = true;
    this.connection.on("Target.attachedToTarget", (params, session) => {
      const info = params["targetInfo"] as { type?: string; url?: string } | undefined;
      const child = params["sessionId"];
      if (session !== this.session || typeof child !== "string") return;
      const go = (): void => void this.connection.send("Runtime.runIfWaitingForDebugger", {}, child).catch(() => {});
      if (info?.type !== "worker") return go();
      this.workers.set(child, info.url ?? "");
      void this.connection.send("Network.enable", {}, child).catch(() => {});
      // As it starts: what it does before these land runs warm and unthrottled.
      void this.toWorker(child);
      const scripts = [...this.workerScripts].map((expression) => this.connection.send("Runtime.evaluate", { expression }, child));
      void Promise.all(scripts).catch(() => {}).then(go);
    });
    this.connection.on("Target.detachedFromTarget", (params) => {
      const child = params["sessionId"];
      if (typeof child === "string") this.workers.delete(child);
    });
    await this.command("Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: false, flatten: true });
  }

  /** The cache a worker shares with the page, as last set. Its CPU is the browser's (`SUZATARY_E2E_BROWSER_CPU`): Chromium
   * throttles a page's main thread alone, and refuses the throttle for a worker. */
  private async toWorker(child: string): Promise<void> {
    await this.connection.send("Network.setCacheDisabled", { cacheDisabled: this.coldOn }, child).catch(() => {});
    await this.connection.send("Network.emulateNetworkConditions", this.networkOn ?? NO_CONDITIONS, child).catch(() => {});
  }

  /**
   * Everything the page said and fetched from now on, each line stamped with milliseconds since the journal began:
   * a request sent, its status, its end (or failure), a console line, and whatever `mark` a scene adds between its
   * steps.
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

  /** A full collection of the page's heap: a wasm module only documents it left held is freed, not handed to the next. */
  async collectGarbage(): Promise<void> {
    await this.command("HeapProfiler.collectGarbage", {});
  }

  /** What the page's main thread allocates from now until the returned stop, sampled every `intervalBytes` by V8's heap
   * profiler (no CPU sampling), what was collected since included: each sampled allocation's size under the stack that
   * made it, as DevTools answers it. */
  async allocations(intervalBytes = 16384): Promise<() => Promise<HeapProfile>> {
    await this.command("HeapProfiler.enable", {});
    await this.command("HeapProfiler.startSampling", { samplingInterval: intervalBytes, includeObjectsCollectedByMajorGC: true, includeObjectsCollectedByMinorGC: true });
    return async () => (await this.command("HeapProfiler.stopSampling", {}))["profile"] as HeapProfile;
  }

  /** The page's main thread sampled from now until the returned stop, which answers DevTools' CPU profile. */
  async profile(intervalUs = 200): Promise<() => Promise<CpuProfile>> {
    await this.command("Profiler.enable", {});
    await this.command("Profiler.setSamplingInterval", { interval: intervalUs });
    await this.command("Profiler.start", {});
    return async () => (await this.command("Profiler.stop", {}))["profile"] as CpuProfile;
  }

  /** The browser's trace of `categories` (its timeline, V8's samples, the page's User Timing) from now until the
   * returned stop, which answers every event recorded, each on the trace's clock in microseconds. */
  async trace(categories: readonly string[]): Promise<() => Promise<readonly TraceEvent[]>> {
    const events: TraceEvent[] = [];
    let ended = (): void => {};
    const complete = new Promise<void>((done) => (ended = done));
    this.tracing = { events, ended };
    await this.command("Tracing.start", { transferMode: "ReportEvents", traceConfig: { recordMode: "recordAsMuchAsPossible", includedCategories: categories } });
    return async () => {
      await this.command("Tracing.end");
      await within(complete, ANSWER_MS, "the trace's end");
      this.tracing = null;
      return events;
    };
  }

  /** Each node a trace names by its backend id (a `Layout` event's roots, say), as `tag#id.class`, or `gone` when the
   * document no longer holds it. */
  async describeNodes(ids: readonly number[]): Promise<readonly string[]> {
    await this.command("DOM.getDocument", { depth: 0 });
    return Promise.all(
      ids.map((backendNodeId) =>
        this.command("DOM.describeNode", { backendNodeId }).then(
          (r) => {
            const node = r["node"] as { readonly nodeName: string; readonly attributes?: readonly string[] };
            const attr = (name: string): string => {
              const a = node.attributes ?? [];
              const at = a.indexOf(name);
              return at >= 0 && at % 2 === 0 ? (a[at + 1] ?? "") : "";
            };
            const id = attr("id");
            const cls = attr("class").trim().split(/\s+/).filter((c) => c !== "").join(".");
            return `${node.nodeName.toLowerCase()}${id === "" ? "" : `#${id}`}${cls === "" ? "" : `.${cls}`}`;
          },
          () => "gone",
        ),
      ),
    );
  }

  /** What the browser does when the system runs short of memory: at `critical`, it drops caches it can rebuild, such
   * as decoded font data. */
  async memoryPressure(level: "moderate" | "critical"): Promise<void> {
    await this.command("Memory.simulatePressureNotification", { level });
  }

  /** The storage quota `origin` is given, as `navigator.storage.estimate()` reads it; `null` is the browser's own again. */
  async quota(origin: string, bytes: number | null): Promise<void> {
    await this.command("Storage.overrideQuotaForOrigin", bytes === null ? { origin } : { origin, quotaSize: bytes });
  }

  /** The browser's default text size in CSS pixels, as a person sets it (16 is the browser's own): what `rem` and a
   * media query's `rem` are measured in. */
  async textSize(px: number): Promise<void> {
    await this.command("Page.setFontSizes", { fontSizes: { standard: px } });
  }

  /** A phone-class main thread: Chromium runs this page's main thread `rate` times slower (1 is none), and no worker. */
  async throttle(rate: number): Promise<void> {
    await this.command("Emulation.setCPUThrottlingRate", { rate });
  }

  /** A phone-class network, or the box's own again with `null`: each request waits `latency` ms and moves at the throughputs, in bytes a second.
   * Every worker attached (`attachWorkers`) is given it too, since the core's worker makes the stores' requests. */
  async network(conditions: Conditions | null): Promise<void> {
    this.networkOn = conditions;
    await this.command("Network.enable", {});
    await this.command("Network.emulateNetworkConditions", conditions ?? NO_CONDITIONS);
    await Promise.all([...this.workers.keys()].map((w) => this.toWorker(w)));
  }

  /** Every request the page and its attached workers make from now until the returned stop, each as it ended. */
  async wire(): Promise<() => readonly Wired[]> {
    type Open = { url: string; from: "page" | "worker"; sent: number; answered: number; status: number; protocol: string; received: number; decoded: number };
    const open = new Map<string, Open>();
    const done: Wired[] = [];
    // The network's clock is monotonic; the first request's wall time places it on the epoch's.
    let offset: number | null = null;
    const at = (seconds: unknown): number => (typeof seconds === "number" ? seconds * 1000 + (offset ?? 0) : NaN);
    const key = (p: Record<string, unknown>, session?: string): string => `${session ?? ""}/${String(p["requestId"])}`;
    let on = true;
    this.connection.on("Network.requestWillBeSent", (p, session) => {
      if (!on) return;
      if (offset === null && typeof p["wallTime"] === "number" && typeof p["timestamp"] === "number") offset = (p["wallTime"] - p["timestamp"]) * 1000;
      const r = p["request"] as { url?: string } | undefined;
      const from = session !== undefined && this.workers.has(session) ? "worker" : "page";
      open.set(key(p, session), { url: (r?.url ?? "").replace(/^https?:\/\/[^/]+/, ""), from, sent: at(p["timestamp"]), answered: NaN, status: 0, protocol: "", received: 0, decoded: 0 });
    });
    this.connection.on("Network.responseReceived", (p, session) => {
      const o = open.get(key(p, session));
      if (o === undefined) return;
      o.answered = at(p["timestamp"]);
      const r = p["response"] as { status?: number; protocol?: string } | undefined;
      o.status = r?.status ?? 0;
      o.protocol = r?.protocol ?? "";
    });
    // A worker's request ends with no encoded length, so its body's chunks are counted as they come.
    this.connection.on("Network.dataReceived", (p, session) => {
      const o = open.get(key(p, session));
      if (o === undefined) return;
      if (typeof p["encodedDataLength"] === "number") o.received += p["encodedDataLength"];
      if (typeof p["dataLength"] === "number") o.decoded += p["dataLength"];
    });
    const end = (p: Record<string, unknown>, session: string | undefined): void => {
      const o = open.get(key(p, session));
      if (o === undefined) return;
      open.delete(key(p, session));
      const { received, ...wired } = o;
      done.push({ ...wired, ended: at(p["timestamp"]), bytes: Math.max(received, typeof p["encodedDataLength"] === "number" ? p["encodedDataLength"] : 0) });
    };
    this.connection.on("Network.loadingFinished", (p, session) => end(p, session));
    this.connection.on("Network.loadingFailed", (p, session) => end(p, session));
    await this.command("Network.enable", {});
    return () => {
      on = false;
      return [...done];
    };
  }

  /** Every request whose URL matches one of `patterns` (`*` any run) failed, by the page and by every worker it runs, as
   * a network that drops them would; none to let them through again. Call `attachWorkers` before the page loads, so its
   * workers are attached. */
  async block(patterns: readonly string[]): Promise<void> {
    await this.command("Network.enable", {});
    await this.command("Network.setBlockedURLs", { urls: patterns });
    await Promise.all([...this.workers.keys()].map((w) => this.connection.send("Network.setBlockedURLs", { urls: patterns }, w)));
  }

  /** A first visit, every time: no HTTP cache and no service worker between this page and the box. */
  async cold(on: boolean): Promise<void> {
    this.coldOn = on;
    await this.command("Network.enable", {});
    await this.command("Network.setCacheDisabled", { cacheDisabled: on });
    await this.command("Network.setBypassServiceWorker", { bypass: on });
    await Promise.all([...this.workers.keys()].map((w) => this.toWorker(w)));
  }

  /** A full-page PNG at `path`. */
  async screenshot(path: string): Promise<void> {
    const result = await this.command("Page.captureScreenshot", { format: "png" });
    writeFileSync(path, String(result["data"]), "base64");
  }
}

/** DevTools' CPU profile: each node a frame under its parent, and the samples' nodes with the microseconds between them. */
/** A sampled allocation profile: each node a call frame, `selfSize` the bytes sampled as allocated there. */
export interface HeapProfile {
  readonly head: HeapNode;
}
export interface HeapNode {
  readonly callFrame: { readonly functionName: string; readonly url: string; readonly lineNumber: number; readonly columnNumber: number };
  readonly selfSize: number;
  readonly children: readonly HeapNode[];
}

export interface CpuProfile {
  readonly nodes: readonly {
    readonly id: number;
    readonly callFrame: { readonly functionName: string; readonly url: string; readonly lineNumber: number; readonly columnNumber: number };
    readonly children?: readonly number[];
  }[];
  readonly samples: readonly number[];
  readonly timeDeltas: readonly number[];
}

/** One event of a Chromium trace, as `Page.trace` answers it: `ph` is its phase (`X` complete, `B`/`E` a begin and its
 * end, `M` metadata), `ts`, `dur` and `tdur` (time on the processor) are microseconds. */
export interface TraceEvent {
  readonly name: string;
  readonly cat: string;
  readonly ph: string;
  readonly ts: number;
  readonly dur?: number;
  readonly tdur?: number;
  readonly pid: number;
  readonly tid: number;
  readonly id?: string;
  readonly id2?: { readonly local?: string; readonly global?: string };
  readonly args?: {
    readonly name?: string;
    readonly data?: Record<string, unknown>;
    readonly beginData?: Record<string, unknown>;
    readonly endData?: Record<string, unknown>;
    readonly elementCount?: number;
    readonly detail?: string;
    /** A `wasm.CompileLazy` event's function. */
    readonly func_index?: number;
  };
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
  /** The path of every request its tabs sent, as each tab's own recorder saw it. */
  fetched: () => ReadonlySet<string>;
  close: () => Promise<void>;
}

/** Where chromium wrote the port it chose: two lines, the port, then the browser target's path. */
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

export async function launch(scrollbars: Scrollbars = "Hidden"): Promise<Browser> {
  const profile = mkdtempSync(join(tmpdir(), "suzatary-e2e-profile-"));
  const group = cpuCap();
  // In the group before it execs, so every process it starts is born in it.
  const child =
    group === null
      ? spawn(chromiumBinary(), flags(profile, scrollbars), { stdio: ["ignore", "pipe", "pipe"] })
      : spawn("sh", ["-c", 'echo $$ > "$0/cgroup.procs" && exec "$@"', group, chromiumBinary(), ...flags(profile, scrollbars)], { stdio: ["ignore", "pipe", "pipe"] });
  let said = "";
  child.stderr?.on("data", (chunk) => {
    said += chunk.toString();
  });
  // What the browser and its renderers print (V8's `--trace-gc`, say), appended to `$SUZATARY_E2E_BROWSER_LOG`; without
  // it the output is read and dropped, so a full pipe never stops a renderer.
  const log = process.env["SUZATARY_E2E_BROWSER_LOG"] ?? "";
  const printed = (chunk: { toString: () => string }): void => (log === "" ? undefined : appendFileSync(log, chunk.toString()));
  child.stdout?.on("data", printed);
  child.stderr?.on("data", printed);
  child.on("error", (error: Error) => {
    said += `\n${error.message}`;
  });

  const devtools = await devtoolsUrl(profile, child, () => said);
  const connection = await Connection.open(devtools);
  // Every tab is recorded over a connection of its own, attached before the tab is handed out, so a scene's own Network
  // domain cannot hide a fetch and no scene's listener hears the recorder.
  const recording = await Connection.open(devtools);
  const fetched = new Set<string>();
  recording.on("Network.requestWillBeSent", (params) => {
    const url = (params["request"] as { url?: string } | undefined)?.url;
    if (url !== undefined) fetched.add(new URL(url).pathname);
  });
  // Each in a window of its own: a background tab is hidden, and a hidden page draws nothing on its frames.
  const tab = async (): Promise<Page> => {
    const target = await connection.send("Target.createTarget", { url: "about:blank", newWindow: true });
    const recorder = String((await recording.send("Target.attachToTarget", { targetId: target["targetId"], flatten: true }))["sessionId"]);
    await recording.send("Network.enable", {}, recorder);
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
    fetched: () => fetched,
    close: async () => {
      try {
        await connection.send("Browser.close");
      } catch {
        // A browser that has already gone is a browser that is closed.
      }
      connection.close();
      recording.close();
      child.kill("SIGKILL");
      await gone(child);
      await discard(profile);
      // An empty group goes.
      if (group !== null) {
        try {
          rmdirSync(group);
        } catch {
          // Another browser of this run still runs in it.
        }
      }
    },
  };
}

/**
 * ⚠️ `child.kill` returns once the signal is delivered while Chromium's process tree may still be writing the profile,
 * so the profile is removed only after the spawned process is reaped, within a bound so a wedged one cannot hang
 * teardown; `exit` never fires again for a child that has already exited.
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
 * Remove with retries, since a crashpad handler is re-parented to init and never waited for; a profile that will not
 * go is a temp directory, left with a warning rather than failing a green suite.
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
