// A cache is honest only while the page says which moment its numbers are from, which is also why `sw.ts` keeps only the shell.

import { el } from "../view/dom";
import { when } from "../view/format";
import app from "../app/app";
import onscreen from "../app/store";
import act from "../view/act";

export namespace stale {
  // `Old` is an old fold, `Unheard` a fresh one nobody can confirm because the stream is down, and `Offline` one this
  // browser folded itself while the stream is down; a fold of its own while the stream is up says nothing, since its
  // numbers are this moment's and an act it holds is marked on its own row.
  export type Why = { type: "Offline" } | { type: "Unheard"; since: number } | { type: "Old"; since: number };
  export type Banner = { type: "Hidden" } | { type: "Shown"; why: Why };

  const HIDDEN: Banner = { type: "Hidden" };

  export const of = (live: "unknown" | "on" | "off", fold: onscreen.Fold): Banner =>
    !onscreen.loaded(fold)
      ? HIDDEN
      : onscreen.by<Banner>(fold.by, {
          Local: () => (live === "off" ? { type: "Shown", why: { type: "Offline" } } : HIDDEN),
          Box: b => (b.fresh && live !== "off" ? HIDDEN : { type: "Shown", why: b.fresh ? { type: "Unheard", since: b.fetchedAt } : { type: "Old", since: b.fetchedAt } }),
          Cached: b => ({ type: "Shown", why: { type: "Old", since: b.fetchedAt } }),
        });

  export const said = (w: Why): string =>
    w.type === "Offline"
      ? "Offline: what you see is what this device holds, and a change made elsewhere may not be here yet."
      : w.type === "Unheard"
        ? `Stale since ${when(w.since, app.now().zone)} — no live updates, so a change since then would not have reached this page.`
        : `Stale since ${when(w.since, app.now().zone)} — this is the last fold this browser saw.`;

  // Retry is an act built once, so a redraw never re-enables a press in flight; the banner is already the report, so a
  // failed retry says nothing more.
  export const install = (bar: HTMLElement, retry: () => Promise<unknown>): ((b: Banner) => void) => {
    const message = el("span", { class: "stalewhat" });
    bar.replaceChildren(message, act.view({ label: "Retry", cls: "staleretry", run: () => act.again(retry()), failed: () => {} }));
    return b => {
      bar.hidden = b.type === "Hidden";
      if (b.type === "Shown") message.replaceChildren(said(b.why));
    };
  };
}
export default stale
