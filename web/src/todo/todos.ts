// The list as a pure function: what the home page shows and in what order. The percentage is a
// fulfillment, so low means urgent and the sort ascends; `value` is the server's, compared, never derived.

import type { Block, Entry, EntryState, SpecNode } from "../wire/generated/wire";
import fulfillment from "./fulfillment";
import term from "./term";
import { describeConflicts } from "./conflicts";
import time from "../app/time";

// --- the filter --------------------------------------------------------------

/** "any" or one exact value, since `""` is itself a category. */
export type Choice = { readonly tag: "any" } | { readonly tag: "is"; readonly value: string };

export const ANY: Choice = { tag: "any" };
export const is = (value: string): Choice => ({ tag: "is", value });

/** `done` is Completed or Cancelled: the question is "is this still on me". */
export type StateFilter = "open" | "done" | "all";

export interface Filter {
  readonly state: StateFilter;
  readonly category: Choice;
  readonly kind: Choice;
  readonly query: string;
  /** Only what waits on a human: unconfirmed content, or a register two writers moved at once. */
  readonly attention: boolean;
  /** Only what is blocked on someone else (`waiting_on`). */
  readonly waiting: boolean;
  /** Only a deadline in the next fortnight, any `Decay` in the whole spec, since a `Conj` or `Gate` can bury it. */
  readonly dated: boolean;
}

export const DEFAULT_FILTER: Filter = {
  state: "open",
  category: ANY,
  kind: ANY,
  query: "",
  attention: false,
  waiting: false,
  dated: false,
};

/** The same two facts the card's chips draw, so the filter cannot disagree with a row. */
export function needsAttention(entry: Entry): boolean {
  return isUnconfirmed(entry) || describeConflicts(entry).length > 0;
}

/** `waiting_on` is "" when nothing holds the todo up: empty means absent. */
export function isWaiting(entry: Entry): boolean {
  return (entry.authored?.waiting_on ?? "") !== "";
}

export const FORTNIGHT_DAYS = 14;

/** Every `Decay`'s `endDate` in the tree, walked rather than a regex over a printed spec (a second reading). */
function decayEndDates(spec: SpecNode): string[] {
  return term.nodes(spec).flatMap((node) => (node.kind === "decay" && typeof node["endDate"] === "string" ? [node["endDate"]] : []));
}

/** Any `Decay` whose `endDate` falls in `[now, now + FORTNIGHT_DAYS days]`, both ends inclusive; `now` is passed, so this stays pure. */
export function isDated(entry: Entry, now: string): boolean {
  const spec = entry.authored?.spec ?? null;
  if (spec === null) return false;
  const from = time.ms(now) ?? NaN;
  const to = from + FORTNIGHT_DAYS * 24 * 60 * 60 * 1000;
  return decayEndDates(spec).some((endDate) => {
    const at = time.ms(endDate);
    return at !== null && at >= from && at <= to;
  });
}

/**
 * When `isDated` next reads otherwise for some row after `now`, epoch milliseconds, null for never: a decay's end date
 * enters the fortnight a fortnight before it and leaves it a millisecond after it.
 */
export function datedNext(entries: readonly Entry[], now: string): number | null {
  const from = time.ms(now) ?? NaN;
  const fortnight = FORTNIGHT_DAYS * 24 * 60 * 60 * 1000;
  const edges = entries
    .flatMap((entry) => (entry.authored?.spec == null ? [] : decayEndDates(entry.authored.spec)))
    .flatMap((endDate) => time.ms(endDate) ?? [])
    .flatMap((at) => [at - fortnight, at + 1])
    .filter((at) => at > from);
  return edges.length === 0 ? null : Math.min(...edges);
}

/**
 * A row with a lifecycle state. `isOpen` and `isDone` are not complements: a state neither recognises is
 * neither, and every other page compares through them or an exhaustive `switch`.
 */
interface Stateful {
  readonly state: EntryState;
}

export function isDone(row: Stateful): boolean {
  return row.state === "completed" || row.state === "cancelled";
}

export function isOpen(row: Stateful): boolean {
  return row.state === "open";
}

/** A decision is not a todo: it belongs under "Waiting on you", never the open list, here and in the Inbox. */
export function isDecision(entry: Entry): boolean {
  return entry.authored?.block.kind === "decision";
}

export function categoryOf(entry: Entry): string {
  return entry.authored?.category ?? "";
}

/** The constructor a block is drawn by, as its chip prints it (`wire::Block::name`). */
export function blockName(block: Block): string {
  switch (block.kind) {
    case "decision":
      return "decision";
    case "todo":
      return block.name;
  }
}

export function kindOf(entry: Entry): string {
  return entry.authored === null ? "" : blockName(entry.authored.block);
}

/** Provisional content, or a state an untrusted actor claims and the fold refuses: not confirmed by a human. */
export function isUnconfirmed(entry: Entry): boolean {
  return entry.unconfirmed; // the server decides: it holds the actor roster
}

// --- what may be done to a todo ----------------------------------------------

/** The five acts: `done` is `complete` on the wire, `tend` a pass at a recurring todo ("Did it"). */
export type ActionName = "done" | "tend" | "cancel" | "reopen" | "confirm";

/** The act that finishes this round of a todo: a recurring one is done again, not done. */
export const finish = (entry: Entry): ActionName => (entry.recurring ? "tend" : "done");

/**
 * Which acts apply, in the order offered. `confirm` rides along whenever the server says unconfirmed, since
 * the roster lives there; the one over-offer is answered by the box, better than hiding it on a guess.
 */
export function applicable(entry: Entry): ActionName[] {
  const acts: ActionName[] = isDone(entry) ? ["reopen"] : [finish(entry), "cancel"];
  return entry.unconfirmed ? [...acts, "confirm"] : acts;
}

function chosen(choice: Choice, value: string): boolean {
  return choice.tag === "any" || choice.value === value;
}

/** Case-insensitive substring over the id and the body, unranked so the urgency order stands. */
export function matchesQuery(entry: Entry, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === "") return true;
  return `${entry.todo}\n${entry.authored?.body ?? ""}`.toLowerCase().includes(needle);
}

/** `now` is read only when `filter.dated` is set. */
export function matches(entry: Entry, filter: Filter, now: string): boolean {
  if (filter.state === "open" && isDone(entry)) return false;
  if (filter.state === "done" && !isDone(entry)) return false;
  if (filter.attention && !needsAttention(entry)) return false;
  if (filter.waiting && !isWaiting(entry)) return false;
  if (filter.dated && !isDated(entry, now)) return false;
  return (
    chosen(filter.category, categoryOf(entry)) && chosen(filter.kind, kindOf(entry)) && matchesQuery(entry, filter.query)
  );
}

// --- order -------------------------------------------------------------------

/** The list order: ascending whole percent as the store holds it, ties on id so a redraw never shuffles; then every row with no number, by id. */
export function byUrgency(a: Entry, b: Entry): number {
  const left = fulfillment.number(a.value) ?? Number.POSITIVE_INFINITY;
  const right = fulfillment.number(b.value) ?? Number.POSITIVE_INFINITY;
  return left === right ? a.todo.localeCompare(b.todo) : left - right;
}

/** Whether a row has a number: neither `∅` nor a price that does not link. */
export function isPriced(entry: Entry): boolean {
  return fulfillment.number(entry.value) !== null;
}

/** The priced todos in urgency order, then the unpriced ones under their own divider, shown and never sunk. */
export interface Listing {
  readonly priced: readonly Entry[];
  readonly unpriced: readonly Entry[];
}

export function listing(entries: readonly Entry[], filter: Filter, now: string): Listing {
  const kept = entries.filter((entry) => !isDecision(entry) && matches(entry, filter, now));
  return {
    priced: kept.filter(isPriced).sort(byUrgency),
    unpriced: kept.filter((entry) => !isPriced(entry)).sort((a, b) => a.todo.localeCompare(b.todo)),
  };
}

export function total(list: Listing): number {
  return list.priced.length + list.unpriced.length;
}

// --- the preview -------------------------------------------------------------

/** One line of typst source for a card's clamp: only whitespace collapsed, nothing interpreted. */
export function preview(body: string): string {
  return body.replace(/\s+/g, " ").trim();
}

/** Roughly three lines at a phone's width; a hint for whether the preview hides anything, while the CSS clamp decides. */
export const CLAMP_CHARS = 110;

export function expandable(body: string): boolean {
  const shown = preview(body);
  return shown !== "" && (shown.length > CLAMP_CHARS || shown !== body.trim());
}

/** A card's title in words: the core's reading of its markup with the markup taken out, never rendered; the id with no `Authored` record. */
export function headline(entry: Entry): string {
  const text = preview(entry.authored?.text ?? "");
  return text === "" ? entry.todo : text;
}

/**
 * Everything a card draws, as one string, so a keyed list knows whether to rebuild a node. The fields are
 * listed, not derived, and exclude `stream`; a field the card starts showing must be added here.
 */
export function signature(entry: Entry): string {
  const authored = entry.authored;
  return JSON.stringify([
    entry.todo,
    entry.state,
    entry.at,
    entry.value,
    entry.unconfirmed,
    entry.recurring,
    describeConflicts(entry).length > 0,
    authored?.body ?? null,
    authored?.rich.body ?? null,
    authored?.category ?? null,
    authored === null ? null : blockName(authored.block),
    authored?.waiting_on ?? null,
    authored?.rich.waiting_on ?? null,
    subtodoCount(entry),
  ]);
}

/** `2 of 5 done`, or "" when there is no checklist. */
export function subtodoCount(entry: Entry): string {
  const subs = entry.authored?.subtodos ?? [];
  if (subs.length === 0) return "";
  return `${subs.filter((sub) => sub.done).length} of ${subs.length} done`;
}
