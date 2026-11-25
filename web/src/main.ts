// The shell: what the tab hears becomes an `app.Event`, and after each one the shell is drawn from `App` —
// the chrome around the page, and the page `pages.table` mounts for its screen.

import { me, reportPushToken, waitingCount } from "./wire/api";
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
import { listTitle, remember } from "./app/chain";
import type stores from "./prodrome/stores";
import once from "./algebra/once";
import program from "./pages/program";
import { installPresence } from "./app/presence";
import { byId, el } from "./view/dom";
import { announce } from "./view/focus";
import { aside, toast } from "./view/toast";
import outbox from "./shell/outbox";
import { installKeyboard, setPaletteOpener } from "./shell/keyboard";
import nav from "./shell/nav";
import { openPalette } from "./shell/palette";
import secretary from "./chat/aside";
import talk from "./chat/talk";
import tiles from "./shell/tiles";
import pages from "./pages/table";
import frame from "./view/frame";
import { type Route, hrefOf, hrefOfAddress, parseAddress, parseRoute } from "./shell/route";
import tokens from "./view/tokens";
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
import type { Unsent } from "./wire/generated/pages";
import { elsewhere, ensureFold, foldHeld, foldWhere, forgetChainReplica, keepObjects, refetchFold, resetChain, restoreChain, restoreFold, shellAsksAfter, signingHere, sync, unsent, watchReplica, watchUnsent } from "./prodrome/sync";
import signer from "./prodrome/signer";
import http from "./wire/http";
import clock from "./algebra/clock";
import due from "./app/due";
import ownerzone from "./app/ownerzone";
import login from "./app/login";
import page from "./shell/page";

// Drawn before anything reads a host by its id.
await page.draw(import.meta.url);

const desk = once.of(async () => new (await program.module()).Desk()).get;
const presses = ui.cell(registers.presses);
// The window's chrome, as every wide rule of `style.css` reads it: an input of the say, never a rule that hides a door.
const wide = window.matchMedia(`(min-width: ${tokens.measure.wide / 16}rem)`);
const chromeNow = (): secretary.Chrome => (wide.matches ? "Wide" : "Phone");
// The talk about the page in view, as heard since the page was arrived at.
let heard: { readonly about: string; readonly talk: secretary.Heard; readonly unhear: () => void } | null = null;
const heardTalk = (about: string): secretary.Heard => (heard?.about === about ? heard.talk : "Unheard");
// The reference the page in view names itself by, where it does (`data-about` beside `data-for`, its own address): a
// message names its row's hash, whatever id its address used.
let named: { readonly for: string; readonly about: string } | null = null;
const namedFor = (route: Route): string | null => (named !== null && named.for === hrefOf(route) ? named.about : null);
const said = (route: Route): secretary.Said => secretary.of(route, chromeNow(), app.now().chain, presses.get(), heardTalk, namedFor(route));
const host = tiles.install(byId("view"), byId("layers"), desk, {
  moved: () => paint(chrome.of(app.now(), seen())),
  said: () => JSON.stringify(said(app.route(app.now())).dock),
  release: (key) => void hide(key),
});

// The talk the say on the page in view reads, asked anew at each arrival and heard as it moves; none is asked where a
// press or the page decides. Only an answer that moves it tells the desk.
const listen = (arrived: boolean): void => {
  const asks = said(app.route(app.now())).asks;
  if (!arrived && (heard?.about ?? null) === asks) return;
  heard?.unhear();
  heard = null;
  if (asks === null) return;
  const unhear = talk.listen(asks, (t) => {
    if (heard === null || heard.about !== asks || heard.talk === t) return;
    heard = { ...heard, talk: t };
    void host.dock();
  });
  heard = { about: asks, talk: "Unheard", unhear };
};
wide.addEventListener("change", () => {
  listen(false);
  void host.dock();
});
// A page that names itself once its reading lands: the say and its talk are asked again about that name.
new MutationObserver(() => {
  const href = hrefOf(app.route(app.now()));
  const e = byId("view").querySelector<HTMLElement>(`[data-about][data-for="${CSS.escape(href)}"]`);
  const now = e === null ? null : { for: href, about: e.dataset.about ?? "" };
  if (now?.about === named?.about && now?.for === named?.for) return;
  named = now;
  listen(false);
  void host.dock();
}).observe(byId("view"), { subtree: true, childList: true, attributes: true, attributeFilter: ["data-about", "data-for"] });

// Where the keyboard goes once a press's own move has ended: into the secretary's region after a show, to the header's
// door after a hide. It is that press's alone: called once the press has begun its move, it ends unfound when another
// press or another move (a link followed meanwhile) begins before that move ends, and is spent once it ends.
let pressed = 0;
const focusAfter = async (to: "Secretary" | "Door"): Promise<void> => {
  const mine = ++pressed;
  const own = host.goes();
  await host.idle();
  const c = host.committed();
  if (mine !== pressed || host.goes() !== own || c === null) return;
  const drawn = c.pinned.type === "Drawn";
  const target = to === "Secretary" ? (drawn ? document.querySelector<HTMLElement>("#view .panelbody") : null) : !drawn && said(c.here.route).door.type === "Header" ? byId("beside") : null;
  target?.focus({ preventScroll: to === "Door" });
};

// Show, from the header's door once no move is in flight, for the page the drawing committed: its press, and, where the
// room holds the secretary only behind the page, the tree focused there, a new entry of the history. Beside the page,
// no entry: the address names the secretary only where it has the focus.
const show = async (): Promise<void> => {
  await host.idle();
  const c = host.committed();
  const door = c === null ? null : said(c.here.route).door;
  if (door === null || door.type === "None") return;
  presses.set(door.shows);
  listen(false);
  const p = await host.dock();
  if (p.type === "Behind") address.went(parseAddress(p.to));
  await focusAfter("Secretary");
};
byId("beside").addEventListener("click", () => void show());

// Hide, the secretary's own, once no move is in flight: the press on the page it is about, then, where it has the
// focus, the page before it, as a close goes: Back where that page is the entry before (the show that made it), else in
// its place. Either way Back never shows it again. Unfocused beside the page, no entry.
const hide = async (key: number): Promise<void> => {
  await host.idle();
  const c = host.committed();
  if (c === null || c.pinned.type !== "Drawn" || c.pinned.key !== key) return;
  const leaf = parseRoute(c.pinned.href);
  const behind = parseAddress(c.pinned.without);
  if (leaf.type !== "aside") return;
  presses.set(secretary.hidden(leaf.about, behind.route, app.now().chain, presses.get()));
  listen(false);
  if (c.here.route.type !== "aside") await host.dock();
  else {
    const log = app.now().log;
    const previous = log.entries[log.at - 1]?.address;
    if (previous !== undefined && routelog.same(previous, behind)) {
      // Back is the browser's: its move starts when the browser says it moved.
      const moved = new Promise<void>((done) => window.addEventListener("popstate", () => done(), { once: true }));
      address.back(behind.route);
      await moved;
    } else address.replaced(behind);
  }
  await focusAfter("Door");
};

const paint = chrome.painter({
  nav: (now, was) => nav.paint(byId("tabbar"), now, was, app.dispatch),
  beside: (t) => void (byId("beside").hidden = t.type !== "Header"),
  stale: stale.install(byId("stale"), refetchFold),
  excluded: leftout.install(byId("excluded"), (copy) => {
    const named = storesync.named(copy);
    if (named?.type === "Store") void storesync.reset(named.name, toast);
    if (named?.type === "Chain") void resetChain(named.name, new Set());
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

// Only the newest navigation draws; a page loaded after a later one is dropped.
let latest = 0;

async function mount(a: app.App, fresh: boolean, heard: boolean, again: boolean, turn: number): Promise<void> {
  const view = byId("view");
  const route = app.route(a);
  // The box's fold, which carries what it rendered, is asked for beside a page's module, not after it (once per tab).
  if (pages.folds[route.type] === "Shown") void ensureFold().catch(() => {});
  const made = await host.go(app.address(a), fresh, again);
  // No page drawn anew: the pages on screen took the move on themselves.
  if (turn !== latest || made.length === 0) return;
  // In its own task, since focusing the heading lays the page out; a page that took the focus keeps it, and so does the
  // header's door a hide gave it.
  if (heard) setTimeout(() => (view.contains(document.activeElement) || document.activeElement === byId("beside") ? undefined : announce(made[made.length - 1] ?? view)), 0);
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
      host.failed(
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
  const { chain, stream } = app.now();
  const was = stream.heads;
  if (frame.type !== "Val" || heads.same(heads.of(frame.v.heads), was)) return;
  const now = heads.of(frame.v.heads);
  app.dispatch({ type: "Heard", heads: now });
  void keepObjects().catch(() => {});
  if (was.length === 0) return;
  // A decision is a todo, so a move of the chain may move the badge.
  void refreshWaiting();
  const reread = (): void => {
    void refetchFold().catch(() => {});
    asked.moved();
  };
  // This tab's own writes were answered in their own words; a move it cannot judge is read unsaid.
  void elsewhere(chain, was, now).then((news) => {
    if (news.length === 0) return;
    aside("updated with changes made elsewhere");
    reread();
  }, reread);
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
  void restoreChain().then(() => sync());
  void refetchFold().catch(() => {});
  askWhoAndWhere();
}

// Before the tiles are first drawn, the bar lights the address's own page.
// The door is the drawing's on screen: none where it draws the secretary.
const seen = (): chrome.Seen => {
  const c = host.committed();
  return {
    fold: store.state(),
    excluded: readings.left(),
    theme: theme(),
    door: c === null || c.pinned.type === "Drawn" ? { type: "None" } : said(c.here.route).door,
    workspace: host.workspace() ?? app.route(app.now()),
  };
};

let drawn: app.App | null = null;

// The first reading of the owner's zone loads its rules, a long task on a phone, so it waits for an idle thread.
const idle = (f: () => void): void => void ("requestIdleCallback" in window ? requestIdleCallback(f, { timeout: 2000 }) : setTimeout(f, 1000));
// The shell's own reads wait until a page has drawn and the thread is idle.
shellAsksAfter(frame.firstDrawn.then(() => new Promise<void>((done) => idle(done))));
foldWhere(() => (pages.folds[app.route(app.now()).type] === "Shown" ? "Page" : "Shell"));

function draw(a: app.App): void {
  const was = drawn;
  drawn = a;
  // A page arrived at hears its talk anew. The secretary arrived at by a new entry of the history is asked for beside the
  // page it is about; Back and Forward to it ask nothing.
  const route = app.route(a);
  const arrived = was === null || hrefOf(app.route(was)) !== hrefOf(route);
  const entered = was === null || was.log.entries[a.log.at]?.address !== a.log.entries[a.log.at]?.address;
  if (arrived && entered && route.type === "aside") presses.set(secretary.went(route.about, presses.get()));
  listen(arrived);
  const moved = was !== null && was.chain !== a.chain;
  if (was === null || moved) remember(a.chain);
  if (moved) switched();
  paint(chrome.of(a, seen()));
  if (was === null) {
    go(a, true, false);
    idle(() => void time.date(a.zone, time.now()));
  } else if (moved || hrefOfAddress(app.address(was)) !== hrefOfAddress(app.address(a)))
    go(a, moved, true, !moved && a.again);
  // The served chains land after the first paint, and the list's title is drawn from them, wherever the list is drawn.
  else if (listTitle(was.chain, was.served) !== listTitle(a.chain, a.served)) {
    if (host.drawn().some(pages.lists)) go(a, true, false, true);
    else host.forget(pages.lists);
  }
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
app.dispatch({ type: "Replaced", to: address.install() });
address.settle();
if (app.now().chain !== loaded.chain) forgetChainReplica();
// Before the first route, so the first paint is the last fold seen, in the right colours.
restoreTheme();
restoreFold();
void restoreChain().then(() => sync());
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
    host.destroy();
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
  const todos = new Map<string, Unsent>();
  for (const [todo, u] of Object.entries(unsent().todos)) if (u !== undefined) todos.set(todo, u);
  const { t, said } = outbox.step(told, todos, Date.now());
  told = t;
  if (said.type === "Said") {
    const about = said.about;
    toast(said.text, about.type === "Refused" ? { label: "Open", onClick: () => address.go({ type: "todo", id: about.todo }) } : undefined);
  }
  window.clearTimeout(telling);
  const at = outbox.next(told, todos);
  if (Number.isFinite(at)) telling = window.setTimeout(tell, Math.max(0, at - Date.now()));
};
watchReplica(() => {
  paint(chrome.of(app.now(), seen()));
  keyed();
});
watchUnsent(tell);
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
draw(app.now());
connect();
installPresence();
signer.watch(keyed);
// The loader brings the app in after the document's own load.
document.documentElement.dataset["started"] = "app";
