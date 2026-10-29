// The one "I need something" popup, over everything waiting, so a new kind of request needs no popup of its
// own; the box pushes the phone only when no page is open. What waits when the app opens is the baseline.

import storesync from "../prodrome/storesync";
import dressed from "../chat/dressed";
import time from "../app/time";
import type { InboxProposal } from "../wire/generated/wire";
import { clear, el } from "../view/dom";
import type { Route } from "../shell/route";
import app from "../app/app";
import onscreen, { store } from "../app/store";
import waiting from "./waiting";

export function waitingIds(ws: readonly waiting.Waiting[]): Set<string> {
  return new Set(waiting.open(ws).map(waiting.key));
}

/** A proposal card's key, `<conversation>/<index>`, the pair its row names. */
export const cardKey = (conversation: string, index: number): string => `${conversation}/${index}`;

/** What is open in `after`, not in `before` and not already drawn, most urgent first. */
export function newlyWaiting(
  before: ReadonlySet<string>,
  after: readonly waiting.Waiting[],
  drawn: ReadonlySet<string>,
): waiting.Waiting[] {
  return waiting.open(after).filter((w) => {
    const at = waiting.where(w);
    const seen = at.type === "Proposal" && drawn.has(cardKey(at.conversation, at.index));
    return !before.has(waiting.key(w)) && !seen;
  });
}

const drawn = new Set<string>();

export function sawProposal(conversation: string, index: number): void {
  drawn.add(cardKey(conversation, index));
}

/** Whether the page at `route` already shows its card; a popup there would cover the button it is about. */
export function onScreen(w: waiting.Waiting, route: Route): boolean {
  if (route.type === "inbox") return true;
  const at = waiting.where(w);
  return at.type === "Proposal" && route.type === "conversation" && route.id === at.conversation;
}

export function hrefOfDecision(key: string): string {
  return `#/inbox?open=${encodeURIComponent(key)}`;
}

/** Pops up into `host` each thing that starts waiting after the store and the inbox were first read. The proposals are
 * read from the inbox alone, so no page pulls the conversations for the popup. */
export function installNeedsYou(host: HTMLElement): void {
  let baseline: Set<string> | null = null;
  let proposals: readonly InboxProposal[] | null = null;
  // A popup lives only while what it names is waiting and not on screen.
  const shown = new Map<string, HTMLElement>();
  const draw = (): void => {
    const state = store.state();
    const route = app.route(app.now());
    if (!onscreen.loaded(state) || proposals === null) {
      baseline = null;
      return;
    }
    const all = waiting.all(proposals, [{ chain: "", entries: onscreen.entries(state) }]);
    const open = new Map(waiting.open(all).map((w) => [waiting.key(w), w]));
    for (const [key, node] of shown) {
      const w = open.get(key);
      if (w === undefined || onScreen(w, route)) {
        node.remove();
        shown.delete(key);
      }
    }
    if (baseline === null) {
      baseline = waitingIds(all);
      return;
    }
    for (const w of newlyWaiting(baseline, all, drawn)) {
      const key = waiting.key(w);
      if (shown.has(key) || onScreen(w, route)) continue;
      const node = card(w, () => shown.delete(key));
      shown.set(key, node);
      host.appendChild(node);
    }
    baseline = waitingIds(all);
  };
  const read = (): void => {
    void dressed
      .asked()
      .then((memory) => storesync.proposals(time.moment(), memory, "Shell"))
      .then(
        (read) => {
          proposals = read.payload.proposals;
          draw();
        },
        () => undefined,
      );
  };
  store.subscribe(draw);
  app.hear(app.route, draw);
  storesync.watch(read);
  read();
  draw();
}

function card(w: waiting.Waiting, forget: () => void): HTMLElement {
  const box = el("div", { class: "needsyou", role: "alert" });
  const close = (): void => {
    box.remove();
    forget();
  };
  const open = el("a", { class: "needsyouopen", href: hrefOfDecision(waiting.key(w)) }, "Open");
  open.addEventListener("click", close);
  const dismiss = el("button", { type: "button", class: "needsyouclose", title: "Later" }, "×");
  dismiss.addEventListener("click", close);
  clear(box);
  box.appendChild(el("div", { class: "needsyouhead" }, "Needs you"));
  box.appendChild(el("div", { class: "needsyoutext" }, waiting.asked(w)));
  box.appendChild(el("div", { class: "needsyourow" }, open, dismiss));
  return box;
}
