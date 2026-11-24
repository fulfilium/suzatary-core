// The navigation, in words: the four a phone opens the app for, and More for the rest: the chain switcher, the owner's
// other places, this app's own pages apart from them, and the theme. `nav.of(app, theme, workspace)` is the bar as a value;
// `paint` puts it in the bar.

import app from "../app/app";
import { said } from "../app/chain";
import { el } from "../view/dom";
import { focusables } from "../view/focus";
import marks from "../view/marks";
import type { Route } from "./route";
import { hrefOf } from "./route";
import { THEMES, type Theme, labelFor, setTheme } from "../app/theme";

export namespace nav {
  /** Where a destination is offered: the bar, the sheet's places, or the sheet's part about this app itself. */
  export type Part = "Bar" | "Places" | "App";

  export interface Tab {
    readonly route: Route;
    readonly label: string;
    readonly part: Part;
  }

  /** Every destination: the bar's four, then the sheet's. */
  export const DESTINATIONS: readonly Tab[] = [
    { route: { type: "todos" }, label: "Todos", part: "Bar" },
    { route: { type: "inbox" }, label: "Inbox", part: "Bar" },
    { route: { type: "secretary", refs: [], topics: [] }, label: "Secretary", part: "Bar" },
    { route: { type: "briefings" }, label: "Briefing", part: "Bar" },
    { route: { type: "mail", query: "", account: "" }, label: "Mail", part: "Places" },
    { route: { type: "calendar", date: "" }, label: "Calendar", part: "Places" },
    { route: { type: "settings" }, label: "Settings", part: "App" },
    { route: { type: "about" }, label: "About", part: "App" },
  ];

  const inPart = (part: Part): readonly Tab[] => DESTINATIONS.filter((t) => t.part === part);
  export const TABS: readonly Tab[] = inPart("Bar");
  export const PLACES: readonly Tab[] = inPart("Places");
  export const APP: readonly Tab[] = inPart("App");
  export const OVERFLOW: readonly Tab[] = [...PLACES, ...APP];

  export type Link = { label: string; href: string; active: boolean; count: number };
  // `name` is the chain, `label` the owner's word for it.
  export type Pick = { name: string; label: string; here: boolean };
  export type More = { on: boolean; open: boolean; picks: Pick[]; places: Link[]; app: Link[]; theme: Theme };
  export type View = { tabs: Link[]; more: More };

  /** The word the bar lights: where the reader is in the app, not the exact hash. */
  export function activeFor(route: Route): string {
    switch (route.type) {
      // A todo and a share light the list they came from.
      case "todo":
      case "add":
        return "todos";
      case "message":
      case "thread":
        return "mail";
      case "briefing":
        return "briefings";
      case "conversation":
      case "conversations":
      case "memory":
      case "newtopic":
        return "secretary";
      default:
        return route.type;
    }
  }

  export function inOverflow(route: Route): boolean {
    return OVERFLOW.some((tab) => tab.route.type === activeFor(route));
  }

  // `workspace` is the one the tiles stand in, which a page opened from another workspace's page does not move.
  export const of = (a: app.App, theme: Theme, workspace: Route): View => {
    const here = activeFor(workspace);
    const link = (tab: Tab): Link => ({
      label: tab.label,
      href: hrefOf(tab.route),
      active: tab.route.type === here,
      count: tab.route.type === "inbox" ? (a.waiting?.count ?? 0) : 0,
    });
    const picks = a.served.length < 2 ? [] : a.served.map((name) => ({ name, label: said(name), here: name === a.chain }));
    return {
      tabs: TABS.map(link),
      more: {
        on: inOverflow(workspace),
        open: a.more,
        picks,
        places: PLACES.map(link),
        app: APP.map(link),
        theme,
      },
    };
  };

  /** Light, dark, or the machine's answer: three buttons, because a switch cannot say "keep following it". */
  function themeRow(here: Theme): HTMLElement {
    return el(
      "div",
      { class: "themerow", role: "group", "aria-label": "Theme" },
      el("span", { class: "themelabel" }, "Theme"),
      ...THEMES.map((value) =>
        el(
          "button",
          {
            type: "button",
            class: value === here ? "themepick active" : "themepick",
            "aria-pressed": value === here ? "true" : "false",
            onclick: (event: Event) => {
              event.stopPropagation();
              setTheme(value);
            },
          },
          labelFor(value),
        ),
      ),
    );
  }

  function tabLink(l: Link, kind: "tab" | "morelink"): HTMLElement {
    return el(
      "a",
      {
        class: l.active ? `${kind} active` : kind,
        href: l.href,
        "aria-current": l.active ? "page" : null,
        "aria-label": l.count === 0 ? null : `${l.label}, ${l.count} waiting`,
      },
      l.active ? marks.diamond() : null,
      el("span", { class: "tablabel" }, l.label),
      l.count === 0 ? null : el("span", { class: "tabbadge", "aria-hidden": "true" }, String(l.count)),
    );
  }

  const chainRow = (picks: Pick[], dispatch: (e: app.Event) => void): HTMLElement | null =>
    picks.length === 0
      ? null
      : el(
          "div",
          { class: "chainrow", role: "group", "aria-label": "Which list" },
          ...picks.map((p) =>
            el(
              "button",
              {
                type: "button",
                class: p.here ? "chainpick active" : "chainpick",
                "aria-pressed": p.here ? "true" : "false",
                onclick: (event: Event) => {
                  event.stopPropagation();
                  dispatch({ type: "Pinned", chain: p.name });
                },
              },
              p.label,
            ),
          ),
        );

  const sameBut = (was: View, now: View) =>
    JSON.stringify({ ...was, more: { ...was.more, open: now.more.open } }) === JSON.stringify(now);

  /** Put `now` in the bar. A toggle of the sheet alone keeps the nodes and moves focus the way a disclosure does. */
  export function paint(bar: HTMLElement, now: View, was: View | null, dispatch: (e: app.Event) => void): void {
    const sheet = bar.querySelector<HTMLElement>(".moresheet");
    const button = bar.querySelector<HTMLElement>("button.tab");
    if (was !== null && sheet !== null && button !== null && sameBut(was, now)) {
      sheet.hidden = !now.more.open;
      button.setAttribute("aria-expanded", now.more.open ? "true" : "false");
      if (now.more.open) focusables(sheet)[0]?.focus();
      else if (sheet.contains(document.activeElement)) button.focus();
      return;
    }
    const more = el(
      "div",
      { class: "moresheet", id: "moresheet", hidden: !now.more.open },
      chainRow(now.more.picks, dispatch),
      ...now.more.places.map((l) => tabLink(l, "morelink")),
      el(
        "div",
        { class: "moreapp", role: "group", "aria-label": "This app" },
        el("p", { class: "morehead" }, "This app"),
        ...now.more.app.map((l) => tabLink(l, "morelink")),
        themeRow(now.more.theme),
      ),
    );
    const toggle = el(
      "button",
      {
        type: "button",
        class: now.more.on ? "tab active" : "tab",
        "aria-expanded": now.more.open ? "true" : "false",
        "aria-controls": "moresheet",
        onclick: (event: Event) => {
          event.stopPropagation();
          dispatch({ type: "More", open: !app.now().more });
        },
      },
      now.more.on ? marks.diamond() : null,
      el("span", { class: "tablabel" }, "More"),
    );
    // A theme picked from the sheet redraws it; the keyboard stays on the pick it made.
    const picking = document.activeElement?.classList.contains("themepick") === true;
    bar.replaceChildren(...now.tabs.map((l) => tabLink(l, "tab")), toggle, more);
    if (picking) more.querySelector<HTMLElement>(".themepick.active")?.focus();
  }
}
export default nav
