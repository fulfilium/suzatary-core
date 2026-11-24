// The last fold, slimmed and dated, so the app opens instantly offline and the page says it is stale;
// the service worker never serves a chain silently, since an undated number has no provenance.

import type { Authored, Entry } from "../wire/generated/wire";
import d from "../wire/decoder";
import lo from "../algebra/lo";
import payload from "../wire/generated/payload";
import app from "./app";

// One key per chain, so a cached fold is never shown as another chain's.
export function cacheKey(): string {
  return `suzatary.fold.${app.now().chain}`;
}

// `localStorage` throws when full; a fold that would not fit is not kept rather than evicting the reader's settings.
export const MAX_CHARS = 700_000;

export interface CachedFold {
  readonly at: string;
  /** ms since the epoch, from the writer's clock. */
  readonly fetchedAt: number;
  readonly entries: Entry[];
}

function slimAuthored(authored: Authored): Authored {
  return { ...authored, detail: "", rich: { ...authored.rich, detail: null } };
}

/** One entry without `detail`, `rich.detail` and `stream`, which a card never draws. */
export function slim(entry: Entry): Entry {
  return {
    ...entry,
    stream: [],
    authored: entry.authored === null ? null : slimAuthored(entry.authored),
  };
}

export function printFold(fold: CachedFold): string {
  return JSON.stringify({
    at: fold.at,
    fetchedAt: fold.fetchedAt,
    entries: fold.entries.map(slim),
  });
}

/** A cache from a build with another `Entry` shape is refused by the decoder, costing one fetch. */
export const parseFold = (raw: unknown): CachedFold | null =>
  lo.withDefault(
    d.json(d.obj<CachedFold>({ at: d.str, fetchedAt: d.num, entries: d.arr(payload.Entry) }))(raw),
    null,
  );

export function fits(printed: string): boolean {
  return printed.length <= MAX_CHARS;
}
