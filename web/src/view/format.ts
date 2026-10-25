// Presentation only, in `suzatary_core::site`'s vocabulary, so both consumers say a number the same way.

import type { Fulfillment } from "../wire/generated/wire";
import { el } from "./dom";
import { hrefOf } from "../shell/route";
import time from "../app/time";

/** `site::pct`, verbatim. */
export function pct(value: number | null | undefined): string {
  return value === null || value === undefined ? "—" : `${(value * 100).toFixed(1)}%`;
}

/** A value as its whole percent, for a pie's hover title: `0.129` → `13%`. */
export function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

/** A row's reading in words: its percentage, "unpriced" for `∅`, "does not link" where its price cannot be read. */
export function worded(value: Fulfillment | null): string {
  return value === null ? "does not link" : value === "absent" ? "unpriced" : pct(value);
}

/** Every modifier `style.css` gives `.chip`, closed so a class the stylesheet lacks is a compile error. */
export type ChipKind = "" | "prov" | "ok" | "cat" | "kind" | "done" | "queuedmark";

/** One chip style for the whole app: `.chip`, plus one of the modifiers above. */
export function chip(label: string, kind: ChipKind = ""): HTMLElement {
  return el("span", { class: kind === "" ? "chip" : `chip ${kind}` }, label);
}

/** A hash's prefix, with the whole name in the title for a copy-paste. */
export function shortHash(hash: string): HTMLElement {
  return el("code", { class: "hash", title: hash }, hash.slice(0, 12));
}

/** The same, linking to the chain page focused on that object. */
export function hashLink(hash: string): HTMLElement {
  return el("a", { class: "hashlink", href: hrefOf({ type: "chain", focus: hash }), title: hash }, hash.slice(0, 12));
}

// --- an instant, a day and an age, as a person reads them -------------------
// Not `time.ts`, whose shapes are the wire's and a date input's contract.

/** An instant a person reads: `2026-09-04T11:32:07` → `2026-09-04 11:32:07`. */
export function iso(value: string): string {
  return value.replace("T", " ");
}

/** The leading `YYYY-MM-DD` of a written DAY, "" for "": a string cut, so a day stays the day it names. An instant's day is `dayOf`'s. */
export function day(value: string): string {
  return value.slice(0, 10);
}

/** The owner's day an instant falls on, `YYYY-MM-DD`; "" for "", and a string that names no instant as it came. */
export function dayOf(at: string, zone: time.Zone): string {
  const t = time.ms(at);
  return t === null ? at : time.date(zone, t);
}

/** The owner's day an instant falls on, in the locale's short day: `23 Sep`, with the year when it is not this one; "" for "". */
export function shortDay(at: string, zone: time.Zone, nowMs: number): string {
  const t = time.ms(at);
  if (t === null) return at;
  const day = time.date(zone, t);
  const year: Intl.DateTimeFormatOptions = day.slice(0, 4) === time.date(zone, nowMs).slice(0, 4) ? {} : { year: "numeric" };
  return new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", ...year, timeZone: "UTC" }).format(new Date(`${day}T12:00:00Z`));
}

/**
 * A wire instant to the minute, on the owner's clock: `2026-09-04T01:33:20+00:00` → `2026-09-04 01:33` in UTC.
 * "" for "" (an unparsable Date header); any other value that will not parse is handed back as the header said it.
 */
export function whenOf(date: string, zone: time.Zone): string {
  if (date === "") return "";
  const at = time.ms(date);
  return at === null ? date : time.minute(zone, at);
}

/**
 * A recent instant as a list reads it, on the owner's clock, against the owner's `today`: its minute today,
 * `yesterday`, else its day. "" for ""; a value that will not parse is handed back as it came.
 */
export function recent(date: string, today: string, zone: time.Zone): string {
  if (date === "") return "";
  const at = time.ms(date);
  if (at === null) return date;
  const d = time.date(zone, at);
  const days = time.daysBetween(d, today);
  return days === 0 ? time.minute(zone, at).slice(11) : days === 1 ? "yesterday" : d;
}

/** An instant for a sentence, `11:32 on 2026-09-04` on the owner's clock; zero is a moment nobody recorded, not 1970. */
export function when(ms: number, zone: time.Zone): string {
  if (ms === 0) return "an unknown time";
  const [d, hm] = time.minute(zone, ms).split(" ");
  return `${hm ?? ""} on ${d ?? ""}`;
}

/**
 * How old, in the coarsest unit that still says something: `today`, `3d`, `2w`, `4mo`, `2y`.
 * "" with no `created` and for a future date: a wrong device clock is a stated limit, and "in -3d" is worse than nothing.
 * A bare day is the owner's day; an instant is placed on the owner's clock first.
 */
export function ageLabel(created: string, nowMs: number, zone: time.Zone): string {
  const t = created.length === 10 ? null : time.ms(created);
  const born = t === null ? day(created) : time.date(zone, t);
  const days = time.daysBetween(born, time.date(zone, nowMs));
  if (days === null || days < 0) return "";
  if (days === 0) return "today";
  if (days === 1) return "1d";
  if (days < 14) return `${days}d`;
  if (days < 60) return `${Math.floor(days / 7)}w`;
  if (days < 365) return `${Math.floor(days / 30)}mo`;
  return `${Math.floor(days / 365)}y`;
}
