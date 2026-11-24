// Fuzzy ranking by subsequence, not substring (`hwr` finds `harrowreply`), so a non-match is
// excluded rather than ranked last; word boundaries, consecutive letters and an early hit score.

const BOUNDARY = /[^a-z0-9]/;

function isBoundary(text: string, at: number): boolean {
  const before = text[at - 1];
  return before === undefined || BOUNDARY.test(before);
}

// How well `text` answers `query`, or `null` for no match; higher is better.
export function score(query: string, text: string): number | null {
  const needle = query.trim().toLowerCase();
  const hay = text.toLowerCase();
  if (needle === "") return 0;
  let total = 0;
  let at = 0;
  let previous = -2;
  for (const letter of needle) {
    if (letter === " ") continue;
    const found = hay.indexOf(letter, at);
    if (found === -1) return null;
    total += 10;
    if (isBoundary(hay, found)) total += 8;
    if (found === previous + 1) total += 6;
    if (found === 0) total += 4;
    previous = found;
    at = found + 1;
  }
  // A late first hit and a long text score less.
  return total - hay.indexOf((needle[0] ?? "").toLowerCase()) * 0.4 - hay.length * 0.02;
}

export interface Ranked<T> {
  readonly item: T;
  readonly score: number;
}

// The matching items, best first; ties keep the caller's order, so an empty query is the list as given.
export function rank<T>(query: string, items: readonly T[], textOf: (item: T) => string, limit = 12): T[] {
  const scored: Ranked<T>[] = [];
  for (const item of items) {
    const value = score(query, textOf(item));
    if (value !== null) scored.push({ item, score: value });
  }
  // Stable sort keeps the tie rule.
  scored.sort((a, b) => b.score - a.score);
  return scored.slice(0, limit).map((each) => each.item);
}
