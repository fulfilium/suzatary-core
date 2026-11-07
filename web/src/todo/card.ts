// A todo as a card, one function for every list: the body is typst's rendering through `rich.ts`, clamped,
// else plain text. The whole card is the link, `role="link"` rather than `<a>` because it contains buttons;
// a click on a control or one ending a text selection is not a navigation.

import { actButton } from "./acts";
import app from "../app/app";
import act from "../view/act";
import type { Acted } from "./acts";
import type { Entry, Fulfillment, Neighbour } from "../wire/generated/wire";
import { describeConflicts } from "./conflicts";
import { el } from "../view/dom";
import { chevronIcon } from "../view/icons";
import { ageLabel, chip, dayOf, worded } from "../view/format";
import pie from "../view/pie";
import keyed from "../algebra/keyed";
import empty from "../view/empty";
import { richOrTextInline } from "../rich";
import { hrefOf } from "../shell/route";
import {
  categoryOf,
  expandable,
  finish,
  headline,
  isDone,
  isOpen,
  isUnconfirmed,
  kindOf,
  preview,
  signature,
  subtodoCount,
} from "./todos";
import time from "../app/time";
import address from "../shell/address";

export interface CardOptions {
  /** Absent for a list that must not change state: a past day, where "done" would write about yesterday's screen. */
  readonly acted?: Acted;
  /** The todo currently filling a detail pane, "" for none. */
  readonly selected?: string;
  /** What opening a card does; by default the todo's own hash, and the router picks the pane or a pushed screen. */
  readonly open?: (todo: string) => void;
}

/** A value as its pie, or the words for one with no number: never a pie for `∅`. */
const pieOr = (value: Fulfillment | null, size: number): Element =>
  typeof value === "number" ? pie.fulfilled(value, size) : chip(worded(value));

/** The clamped body, and the toggle to its stored source, `null` when the preview is already the whole of it. */
function body(entry: Entry): readonly [HTMLElement, HTMLElement | null] {
  const source = entry.authored?.body ?? "";
  const rich = entry.authored?.rich.body ?? null;
  // `headline` is the fallback, and the answer for a todo with no `Authored` record (the id).
  const clamp =
    rich === null
      ? el("span", { class: "clamp" }, headline(entry))
      : richOrTextInline(rich, headline(entry), "clamp");
  if (!expandable(source)) return [el("div", { class: "cardbody" }, clamp), null];
  const full = el("span", { class: "full", hidden: true }, source);
  const button = el(
    "button",
    { type: "button", class: "srcbtn", "aria-expanded": "false", "aria-label": "show the stored typst source" },
    "source",
  );
  button.addEventListener("click", (event: Event) => {
    // The card is the link. A control inside it answers for itself.
    event.stopPropagation();
    const open = button.getAttribute("aria-expanded") === "true";
    button.setAttribute("aria-expanded", open ? "false" : "true");
    clamp.hidden = !open;
    full.hidden = open;
  });
  return [el("div", { class: "cardbody" }, clamp, full), button];
}

/** The footer: the source toggle and "Done", the one act worth a thumb; the rest are on the todo itself. */
function foot(entry: Entry, source: HTMLElement | null, acted?: Acted): HTMLElement | null {
  // A footer only when it carries the Done act or the source toggle.
  const done = acted === undefined || isDone(entry) ? null : act.offered(act.gate(), () => actButton(finish(entry), entry.todo, acted, "cardact"), null);
  if (source === null && done === null) return null;
  return el("div", { class: "cardfoot" }, source, done);
}

/** A click that landed on a control is that control's, not the card's. */
function onControl(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest("button, a, input, textarea, select") !== null;
}

/** Releasing the mouse after a drag over the body is a text selection, not a tap. */
function selecting(): boolean {
  const selection = window.getSelection();
  return selection !== null && selection.toString().trim() !== "";
}

function goTo(todo: string): void {
  address.go({ type: "todo", id: todo });
}

export function todoCard(entry: Entry, options: CardOptions = {}): HTMLElement {
  const category = categoryOf(entry);
  const kind = kindOf(entry);
  const subs = subtodoCount(entry);
  const waiting = entry.authored?.waiting_on ?? "";
  const on = options.selected === entry.todo;
  const [text, source] = body(entry);
  const resolved = isDone(entry);
  // A resolved card dims and does not leave: never moved, sunk or hidden, like the `(unconfirmed)` marker.
  const age = ageLabel(entry.authored?.created ?? "", time.now(), app.now().zone);
  const card = el(
    "article",
    {
      class: ["card", on ? "selected" : "", resolved ? "resolved" : ""].filter((c) => c !== "").join(" "),
      "data-todo": entry.todo,
      role: "link",
      tabindex: 0,
      "aria-label": `open ${entry.todo}`,
      "aria-current": on ? "true" : null,
    },
    el(
      "div",
      { class: "cardmeta" },
      pieOr(entry.value, 20),
      category === "" ? null : chip(category, "cat"),
      kind === "" ? null : chip(kind, "kind"),
      isUnconfirmed(entry) ? chip("unconfirmed", "prov") : null,
      describeConflicts(entry).length === 0 ? null : chip("conflict", "prov"),
      subs === "" ? null : chip(subs),
      isOpen(entry) ? null : chip(entry.at === "" ? entry.state : `${entry.state} ${dayOf(entry.at, app.now().zone)}`, "done"),
      el("code", { class: "cardid" }, entry.todo),
      // How long it has been owed, last in the row.
      age === "" ? null : el("span", { class: "cardage", title: entry.authored?.created ?? "" }, age),
    ),
    text,
    waiting === ""
      ? null
      : el(
          "div",
          { class: "waiting" },
          el("span", { class: "waitlabel" }, "waiting on "),
          richOrTextInline(entry.authored?.rich.waiting_on ?? null, preview(waiting), "waitingtext"),
        ),
    foot(entry, source, options.acted),
  );
  const open = options.open ?? goTo;
  card.addEventListener("click", (event: MouseEvent) => {
    if (onControl(event.target) || selecting()) return;
    open(entry.todo);
  });
  card.addEventListener("keydown", (event: KeyboardEvent) => {
    // Only the card's own Enter; one on a button inside bubbles here after doing its job.
    if (event.key !== "Enter" || event.target !== card) return;
    event.preventDefault();
    open(entry.todo);
  });
  return card;
}

/** A todo named by another todo, in one line: a real `<a>`, since nothing inside it is pressed. */
export function neighbourRow(neighbour: Neighbour): HTMLElement {
  return el(
    "a",
    {
      class: "linkrow",
      "data-todo": neighbour.todo,
      href: hrefOf({ type: "todo", id: neighbour.todo }),
    },
    pieOr(neighbour.value, 14),
    el("code", { class: "linkid" }, neighbour.todo),
    chip(neighbour.state, isOpen(neighbour) ? "" : "done"),
    neighbour.unconfirmed ? chip("unconfirmed", "prov") : null,
    chevronIcon(),
  );
}

/**
 * The reserved key of the "not priced" divider; a todo id is a word, so it cannot collide.
 * Written as an escape, never the byte: a literal NUL makes this a binary file to the one-door gate's `grep`.
 */
const DIVIDER = "\u0000divider";

function dividerNode(count: number): HTMLElement {
  return el(
    "div",
    { class: "divider" },
    `not priced (${count})`,
    el("span", { class: "dividernote" }, "no number: no spec, or a price that does not link"),
  );
}

/** The list, keyed by todo id: only a card whose `todos.signature` changed is rebuilt, so scroll, focus and selection survive. */
export interface CardList {
  /** The container. Mounted once and never replaced. */
  readonly node: HTMLElement;
  update: (priced: readonly Entry[], unpriced: readonly Entry[]) => void;
}

/** `said` is what the list says when empty, since an empty chain and an empty filter are different news. */
export function cardList(options: CardOptions = {}, said = "Nothing to show."): CardList {
  const node = el("div", { class: "cards" });
  const cards = keyed.list(node);
  const nothing = empty.note(said);
  const update = (priced: readonly Entry[], unpriced: readonly Entry[]): void => {
    // Asked once per update: a fact about this tab, not about any entry.
    const controls = options.acted === undefined ? "read" : String(app.writes(app.now()));
    // Whether this tab may act is part of a card's mark, not of the pure `signature`, so buttons appear once `/api/me` answers.
    const card = (entry: Entry): keyed.Item => ({ key: entry.todo, sig: `${signature(entry)}|${controls}`, make: () => todoCard(entry, options) });
    const divider: keyed.Item[] = unpriced.length === 0 ? [] : [{ key: DIVIDER, sig: `divider ${unpriced.length}`, make: () => dividerNode(unpriced.length) }];
    cards.draw([...priced.map(card), ...divider, ...unpriced.map(card)]);
    // The empty note is the list's own answer, not a keyed card.
    if (priced.length + unpriced.length === 0) node.appendChild(nothing);
    else nothing.remove();
  };
  return { node, update };
}
