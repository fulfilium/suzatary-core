// One line at the top: the answer to a person's act (a refusal in the server's words, an Undo within `undo.ts`'s
// window), or an aside about what moved without them.

import { byId, el } from "./dom";
import { UNDO_WINDOW_MS, undoStillLive } from "../todo/undo";
import time from "../app/time";

const DWELL_MS = 6000;

export interface ToastAction {
  readonly label: string;
  readonly onClick: () => void;
}

// An aside never replaces an act's notice while it dwells: it waits, the newest alone, until that notice ends. An act's
// notice is shown at once.
export namespace notice {
  export type Notice =
    | { readonly type: "Act"; readonly message: string; readonly action?: ToastAction }
    | { readonly type: "Aside"; readonly message: string };
  export type Act = Extract<Notice, { type: "Act" }>;

  export type T =
    | { readonly type: "Hidden" }
    | { readonly type: "Shown"; readonly notice: Notice; readonly until: number }
    | { readonly type: "Holding"; readonly notice: Act; readonly until: number; readonly aside: string };

  // `Ended`: the dwell ran out, or the person used the button.
  export type Event = { readonly type: "Offered"; readonly notice: Notice; readonly at: number } | { readonly type: "Ended"; readonly at: number };

  export const HIDDEN: T = { type: "Hidden" };

  const dwell = (n: Notice): number => (n.type === "Act" && n.action !== undefined ? UNDO_WINDOW_MS : DWELL_MS);
  const shownFor = (n: Notice, at: number): T => ({ type: "Shown", notice: n, until: at + dwell(n) });

  const ended = (t: T, at: number): T => {
    switch (t.type) {
      case "Hidden":
      case "Shown":
        return HIDDEN;
      case "Holding":
        return shownFor({ type: "Aside", message: t.aside }, at);
    }
  };

  export const step = (t: T, e: Event): T => {
    if (e.type === "Ended") return ended(t, e.at);
    const now = t.type !== "Hidden" && e.at >= t.until ? ended(t, t.until) : t;
    const n = e.notice;
    switch (now.type) {
      case "Hidden":
        return shownFor(n, e.at);
      case "Shown":
        if (n.type === "Aside" && now.notice.type === "Act") return { type: "Holding", notice: now.notice, until: now.until, aside: n.message };
        return shownFor(n, e.at);
      case "Holding":
        return n.type === "Aside" ? { ...now, aside: n.message } : { type: "Holding", notice: n, until: e.at + dwell(n), aside: now.aside };
    }
  };

  export const shown = (t: T): Notice | null => (t.type === "Hidden" ? null : t.notice);
}

let state: notice.T = notice.HIDDEN;
let clearing = 0;

function drawn(n: notice.Notice): (string | HTMLElement)[] {
  if (n.type === "Aside" || n.action === undefined) return [n.message];
  const action = n.action;
  const shownAt = time.now();
  const button = el("button", { type: "button", class: "toastact" }, action.label);
  button.addEventListener("click", () => {
    // A click that raced the timeout by a tick fires nothing.
    if (!undoStillLive(shownAt, time.now())) return;
    settle({ type: "Ended", at: time.now() });
    action.onClick();
  });
  return [n.message, button];
}

// Redrawn only when the notice changes, so a button stays the one the person sees.
function settle(e: notice.Event): void {
  const was = notice.shown(state);
  state = notice.step(state, e);
  const now = notice.shown(state);
  const box = byId("toast");
  if (now !== was && now !== null) box.replaceChildren(...drawn(now));
  box.hidden = now === null;
  window.clearTimeout(clearing);
  if (state.type !== "Hidden") clearing = window.setTimeout(() => settle({ type: "Ended", at: time.now() }), Math.max(0, state.until - time.now()));
}

/** The answer to what the person just did. */
export function toast(message: string, action?: ToastAction): void {
  settle({ type: "Offered", notice: action === undefined ? { type: "Act", message } : { type: "Act", message, action }, at: time.now() });
}

/** What moved without the person. */
export function aside(message: string): void {
  settle({ type: "Offered", notice: { type: "Aside", message }, at: time.now() });
}
