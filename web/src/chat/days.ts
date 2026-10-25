/**
 * The owner's days a transcript's rounds are separated by (`grouped`). A day is printed from its own `YYYY-MM-DD`,
 * never through a zoned `Date` that could move it; `Date.UTC` only names the weekday.
 */

import time from "../app/time";

const WEEKDAYS = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
] as const;

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
] as const;

/** `2026-09-14T09:00:00` → `Monday 14 September`; "" when the leading `YYYY-MM-DD` does not parse. */
export function dayLabel(at: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(at);
  if (match === null) return "";
  const [, yearText, monthText, dayText] = match;
  const year = Number(yearText);
  const month = Number(monthText);
  const day = Number(dayText);
  const weekday = WEEKDAYS[new Date(Date.UTC(year, month - 1, day)).getUTCDay()];
  const name = MONTHS[month - 1];
  if (weekday === undefined || name === undefined) return "";
  return `${weekday} ${day} ${name}`;
}

// The owner's day of each instant a round opened at, by zone and instant: an instant's day never moves, and every
// window reads its rounds'. Let go whole past a bound.
const days = new Map<string, string>();
export const dayOf = (zone: time.Zone, instant: string): string => {
  const key = `${zone.name} ${instant}`;
  const known = days.get(key);
  if (known !== undefined) return known;
  const at = time.ms(instant);
  const day = at === null ? "" : time.date(zone, at);
  if (days.size >= 4096) days.clear();
  days.set(key, day);
  return day;
};
