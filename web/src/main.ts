// The shell: what the tab hears becomes an `app.Event`, and after each one the shell is drawn from `App` —
// the chrome around the page, and the page `pages.table` mounts for its screen.

import { me, reportPushToken, waitingCount, wasOurWrite } from "./wire/api";
import storesync from "./prodrome/storesync";
import intentsync from "./prodrome/intentsync";
import app from "./app/app";
import time from "./app/time";
import chrome from "./shell/chrome";
import beacon from "./app/beacon";
import pushtoken from "./app/pushtoken";
import wipe from "./app/wipe";
import rendered from "./app/rendered";
import asked from "./app/asked";
import d from "./wire/decoder";
import stream from "./algebra/stream";
import bfcache from "./app/bfcache";
import payload from "./wire/generated/payload";
import { remember } from "./app/chain";
import type stores from "./prodrome/stores";
import once from "./algebra/once";
import program from "./pages/program";
import { installPresence } from "./app/presence";
import { byId, clear, el } from "./view/dom";
import { announce } from "./view/focus";
import { toast } from "./view/toast";
import outbox from "./shell/outbox";
import { unsentTodos } from "./prodrome/replica";
import { WIDE_MIN_PX, isWide } from "./shell/layout";
import { installKeyboard, setPaletteOpener } from "./shell/keyboard";
import nav from "./shell/nav";
import { openPalette } from "./shell/palette";
import { updatePanel } from "./chat/panel";
import type { Mounted } from "./view/page";
import { inert } from "./view/page";
import pages from "./pages/table";
import frame from "./view/frame";
import { hrefOf } from "./shell/route";
import address from "./shell/address";
import routelog from "./shell/routelog";
import kept from "./view/kept";
import ui from "./app/ui";
import registers from "./app/registers";
import sweep from "./app/sweep";
import device from "./app/device";
import newbuild from "./app/newbuild";
import stale from "./shell/stale";
import leftout from "./shell/leftout";
import syncing from "./shell/syncing";
import readings from "./prodrome/readings";
import heads from "./prodrome/heads";
import { hearTheme, restoreTheme, theme } from "./app/theme";
import { store } from "./app/store";
import { ensureFold, foldHeld, foldWhere, forgetChainReplica, held, keepObjects, refetchFold, resetRow, restoreFold, restoreReplica, shellAsksAfter, signingHere, sync, watchReplica } from "./prodrome/sync";
import signer from "./prodrome/signer";
import http from "./wire/http";
import clock from "./algebra/clock";
import due from "./app/due";
import ownerzone from "./app/ownerzone";
import login from "./app/login";
import page from "./shell/page";

// Drawn before anything reads a host by its id.
await page.draw(import.meta.url);

let current: Mounted = inert;


const paint = chrome.painter({
  nav: (now, was) => nav.paint(byId("tabbar"), now, was, app.dispatch),
  panel: (p) => updatePanel(p),
  stale: stale.install(byId("stale"), refetchFold),
  excluded: leftout.install(byId("excluded"), (copy) => {
    const store = storesync.named(copy);
    void (store === null ? resetRow(copy, new Set(), "Unread") : storesync.reset(store, toast));
  }, ui.cell(registers.leftout)),
  signing: (s) => {
    const bar = byId("signing");
    bar.hidden = s.said === null;
    bar.replaceChildren(s.said ?? "");
  },
  build: (l) => {
    const host = byId("updated");
    host.hidden = l.type === "None";
    host.replaceChildren(...newbuild.view(l, builds.acts));
  },
  live: (l) => {
    const dot = byId("live");
    dot.className = l.className;
    dot.title = l.meaning;
    dot.setAttribute("aria-label", l.meaning);
  },
});

// A page that absorbs the change keeps its state; a new chain is a new page. A new page's heading is announced, except
// the first; a place read again (back, forward) is restored as left, and any other opens at the top.
// Only the newest navigation draws; a page loaded after a later one is dropped.
let latest = 0;

async function mount(a: app.App, fresh: boolean, heard: boolean, again: boolean, turn: number): Promise<void> {
  const view = byId("view");
  const screen = app.screen(a);
  if (!fresh && current.absorb?.(screen) === true) return;
  // The box's fold, which carries what it rendered, is asked for beside a page's module, not after it (once per tab).
  if (pages.folds[screen.type] === "Shown") void ensureFold().catch(() => {});
  const draw = await pages.load(screen);
  if (turn !== latest) return;
  current.destroy();
  current = inert;
  if (!again) window.scrollTo(0, 0);
  current = draw(view);
  // In its own task, since focusing the heading lays the page out; a page that took the focus keeps it.
  if (heard) setTimeout(() => (view.contains(document.activeElement) ? undefined : announce(view)), 0);
  if (again) kept.restore(routelog.here(a.log).kept, () => drawn !== null && drawn.log === a.log);
}

// A page whose code would not load is never drawn as the error (`newbuild.lost`); its Reload is a reload, as a browser
// may keep a module's failed load for the document's life.
function go(a: app.App, fresh: boolean, heard: boolean, again = false): void {
  const turn = ++latest;
  void mount(a, fresh, heard, again, turn).catch((error: unknown) => {
    if (turn !== latest) return;
    console.warn("the page did not load", error);
    void builds.lost().then((l) => {
      if (turn !== latest) return;
      const view = byId("view");
      clear(view);
      view.appendChild(
        l.type === "Moved"
          ? el("p", { class: "problem" }, "Suzatary was updated: opening this page in the new version.")
          : el(
              "p",
              { class: "problem" },
              "This page could not be loaded. ",
              el("button", { type: "button", class: "retry", onclick: builds.acts.reload }, "Reload"),
            ),
      );
    });
  });
}

let close = (): void => {};

const tipFrame = d.json(payload.TipFrame);

// One stream per chain: another chain's tip would refetch a fold nobody is looking at.
function connect(): void {
  close();
  close = stream.open(stream.of(`${app.apiBase(app.now())}/events`), {
    // A reopened connection may have missed a deploy, or a login the box could not name.
    opened: () => {
      app.dispatch({ type: "Opened" });
      void sync();
      void builds.check().catch(() => {});
      askAgain();
    },
    frame: heardTip,
    // The live dot says the fold may be moving unheard.
    gaveUp: () => app.dispatch({ type: "GaveUp" }),
  });
}

function heardTip(data: string): void {
  const frame = tipFrame(data);
  const was = app.now().stream.heads;
  if (frame.type !== "Val" || heads.same(heads.of(frame.v.heads), was)) return;
  const now = heads.of(frame.v.heads);
  app.dispatch({ type: "Heard", heads: now });
  // A decision is a todo, so a move of the chain may move the badge.
  if (was.length !== 0) void refreshWaiting();
  // Our own write is already in the store; announcing it back would tell the reader what they just did.
  if (was.length !== 0 && !wasOurWrite(now)) {
    toast(`chain moved — heads ${heads.shown(now)}`);
    // A failed background refresh keeps what is on screen; the next tip move retries.
    void refetchFold().catch(() => {});
    asked.moved();
  }
  void keepObjects().catch(() => {});
}

function askWhoAndWhere(): void {
  void me()
    .then((answer) => {
      ownerzone.remember({ name: answer.zone });
      app.dispatch({
        type: "Met",
        me: login.of(answer),
        served: answer.chains,
        categories: answer.categories,
        zone: { name: answer.zone },
      });
      keyed();
    })
    .catch(() => app.dispatch({ type: "Unmet" }));
}

// The `Unmet` line stands meanwhile, never a login with no actor.
function askAgain(): void {
  if (app.now().who.type === "Unmet") askWhoAndWhere();
}

// Read after `/api/me` and every replica change, so a key that cannot sign is said before the first write.
function keyed(): void {
  if (app.now().who.type !== "Writes") return;
  void signingHere("Shell")
    .then((s) => app.dispatch({ type: "Keyed", key: keyOf(s) }))
    .catch(() => {});
}

const keyOf = (s: device.Signing): app.Key => {
  switch (s.type) {
    case "Unheard":
    case "Reads":
      return { type: "Unchecked" };
    case "Unsupported":
    case "Unreadable":
      return { type: "Unsupported", e: s.e };
    case "Keyed":
      return s.standing === "trusted" ? { type: "Trusted" } : s.standing === "revoked" ? { type: "Revoked" } : { type: "Untrusted" };
  }
};

// Counted on the box, so the tab holds no conversation for a badge. No badge beats a wrong one.
const refreshWaiting = once.shared(() =>
  waitingCount()
    .then((w) => app.dispatch({ type: "Waiting", waiting: w }))
    .catch(() => {}),
);

// A switch of chain drops and re-asks everything chain-shaped, in startup order.
function switched(): void {
  forgetChainReplica();
  restoreFold();
  connect();
  void restoreReplica().then(() => sync());
  void refetchFold().catch(() => {});
  askWhoAndWhere();
}

const seen = (): chrome.Seen => ({ fold: store.state(), excluded: readings.left(), theme: theme() });

let drawn: app.App | null = null;

// The first reading of the owner's zone loads its rules, a long task on a phone, so it waits for an idle thread.
const idle = (f: () => void): void => void ("requestIdleCallback" in window ? requestIdleCallback(f, { timeout: 2000 }) : setTimeout(f, 1000));
// The shell's own reads wait until a page has drawn and the thread is idle.
shellAsksAfter(frame.firstDrawn.then(() => new Promise<void>((done) => idle(done))));
foldWhere(() => (pages.folds[app.screen(app.now()).type] === "Shown" ? "Page" : "Shell"));

function draw(a: app.App): void {
  const was = drawn;
  drawn = a;
  const moved = was !== null && was.chain !== a.chain;
  if (was === null || moved) remember(a.chain);
  if (moved) switched();
  paint(chrome.of(a, seen()));
  if (was === null) {
    go(a, true, false);
    idle(() => void time.date(a.zone, time.now()));
  } else if (moved || hrefOf(app.route(was)) !== hrefOf(app.route(a)) || was.wide !== a.wide)
    go(a, moved, true, !moved && a.again && hrefOf(app.route(was)) !== hrefOf(app.route(a)));
}

// Before the rest of startup, so a failure in any of it reaches the box.
beacon.install();
rendered.hear((k) => beacon.tell("finding", `the box rendered ${k} twice, differently`));
const wiping = wipe.page({ ...wipe.browser, stuck: () => app.dispatch({ type: "Undeleted" }) });
// A request signed by this device's revoked key is answered with the wipe.
http.whenGone(wiping.gone);
// A wipe a reload cut short is finished before this load opens anything.
wiping.resume();
// This tab holds the same replica in memory and must not write it back.
window.addEventListener("storage", (e) => {
  if (e.key === wipe.MARK) wiping.heard();
});
// From the bfcache: a wipe marked, or one that ended while the page was cached.
window.addEventListener("pageshow", (e) => {
  if (e.persisted) void wiping.shown();
});
http.signWith(async () => {
  const held = await signer.held();
  return held.type === "Held" ? { key: held.key.public, sign: (name: string) => signer.sign(held.key, name) } : null;
});
pushtoken.install(reportPushToken);
installKeyboard();
ui.install();
// A replaced draft key moves in; the rest go.
sweep.run(sweep.local, ui.tab);
// The palette reads the keyboard's target, so the keyboard cannot import it back.
setPaletteOpener(openPalette);
// The first route may pin another chain than the remembered one the replica was loaded with.
const loaded = app.now();
app.dispatch({ type: "Replaced", route: address.install() });
app.dispatch({ type: "Resized", wide: isWide(window.innerWidth) });
address.settle();
if (app.now().chain !== loaded.chain) forgetChainReplica();
// Before the first route, so the first paint is the last fold seen, in the right colours.
restoreTheme();
restoreFold();
void restoreReplica().then(() => sync());
askWhoAndWhere();
const heardStore = (name: stores.Name): void => void ((name === "inbox" || name === "lines") && refreshWaiting());
let closeStores = storesync.listen(heardStore);
storesync.watchProgress(syncing.install(byId("syncing")));
intentsync.listen();
bfcache.across(window, {
  close: () => {
    closeStores();
    close();
    close = () => {};
    current.destroy();
    current = inert;
  },
  open: () => {
    closeStores = storesync.listen(heardStore);
    connect();
    go(app.now(), true, false, true);
  },
});
const builds = newbuild.install((line) => app.dispatch({ type: "Build", line }));
// A refused write or an answer this build cannot read: perhaps a newer build is served.
http.whenStale(() => void builds.check().catch(() => {}));
storesync.hearWorker({
  gone: () => wiping.gone(),
  stale: () => void builds.check().catch(() => {}),
  unloaded: (why) => beacon.tell("core", new Error(why)),
});
void device.persist().catch(() => {});
// Started once the screen has drawn and the tab is idle, so its first draw (and compiling its module) is not the screen's.
void frame.firstDrawn.then(() => idle(() => void program.mount("NeedsYouPage", {}, { box: () => byId("needsyou"), lane: "Shell" }).then(draw => draw(byId("needsyou")))));
app.tab.subscribe(draw);
// These move without App, so each redraws the chrome.
store.subscribe(() => paint(chrome.of(app.now(), seen())));
let told = outbox.EMPTY;
let telling = 0;
const tell = (): void => {
  const unsent = unsentTodos(held());
  const { t, said } = outbox.step(told, unsent, Date.now());
  told = t;
  if (said.type === "Said") {
    const about = said.about;
    toast(said.text, about.type === "Refused" ? { label: "Open", onClick: () => address.go({ type: "todo", id: about.todo }) } : undefined);
  }
  window.clearTimeout(telling);
  const at = outbox.next(told, unsent);
  if (Number.isFinite(at)) telling = window.setTimeout(tell, Math.max(0, at - Date.now()));
};
watchReplica(() => {
  paint(chrome.of(app.now(), seen()));
  keyed();
  tell();
});
readings.watch(() => paint(chrome.of(app.now(), seen())));
hearTheme(() => paint(chrome.of(app.now(), seen())));
// The fold is read again when what it shows next moves (`due`); a read that moved nothing waits longer each time.
const rearm = clock.pacer();
const arm = (n: clock.Next): void =>
  rearm(n, () => {
    const was = store.state();
    void foldHeld()
      .then((folded) => (folded ? undefined : refetchFold()))
      .catch(() => {})
      .finally(() => {
        if (store.state() === was) arm(clock.at(Date.now(), false));
      });
  });
store.subscribe((s) => arm(due.of(s, Date.now(), app.now().zone)));
// A timer counts only awake time, so it is armed again on waking; a store that did not open before sleep may open now,
// and a deploy may have been missed.
const woke = (): void => {
  if (document.visibilityState !== "visible") return;
  arm(due.of(store.state(), Date.now(), app.now().zone));
  keyed();
  void builds.check().catch(() => {});
  askAgain();
};
document.addEventListener("visibilitychange", woke);
// The page's first show is its load, already asked.
window.addEventListener("pageshow", (e) => void (e.persisted ? woke() : undefined));
// Closes the More sheet; its own controls stop the click.
document.addEventListener("click", () => app.dispatch({ type: "More", open: false }));
// The same route is a different page either side of the breakpoint; `matchMedia` fires once at the edge.
window.matchMedia(`(min-width: ${WIDE_MIN_PX}px)`).addEventListener("change", () => app.dispatch({ type: "Resized", wide: isWide(window.innerWidth) }));
draw(app.now());
connect();
installPresence();
signer.watch(keyed);
// The loader brings the app in after the document's own load.
document.documentElement.dataset["started"] = "app";
