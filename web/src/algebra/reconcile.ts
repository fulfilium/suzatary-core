// The smallest set of moves between two lists of keys, so a redraw keeps scroll, selection and focus. Kept keys in
// relative order (a longest increasing subsequence) stay; removals come first, then inserts and moves from the end
// backwards, so `before` names a key already placed.

export type Op =
  // `before` is the key it must precede, or null for the end.
  | { readonly op: "insert"; readonly key: string; readonly before: string | null }
  | { readonly op: "move"; readonly key: string; readonly before: string | null }
  | { readonly op: "remove"; readonly key: string };

interface Tail {
  readonly index: number;
  readonly value: number;
}

// The indices of a longest increasing subsequence (patience sorting); -1 means no predecessor.
function longestIncreasing(values: readonly number[]): number[] {
  const from: number[] = new Array<number>(values.length).fill(-1);
  const tails: Tail[] = [];
  values.forEach((value, index) => {
    let low = 0;
    let high = tails.length;
    while (low < high) {
      const mid = (low + high) >> 1;
      const tail = tails[mid];
      if (tail !== undefined && tail.value < value) low = mid + 1;
      else high = mid;
    }
    from[index] = low > 0 ? (tails[low - 1]?.index ?? -1) : -1;
    tails[low] = { index, value };
  });
  const out: number[] = [];
  let at = tails[tails.length - 1]?.index ?? -1;
  while (at !== -1) {
    out.push(at);
    at = from[at] ?? -1;
  }
  return out.reverse();
}

export function reconcile(oldKeys: readonly string[], newKeys: readonly string[]): Op[] {
  const wanted = new Set(newKeys);
  const ops: Op[] = [];
  for (const key of oldKeys) {
    if (!wanted.has(key)) ops.push({ op: "remove", key });
  }

  const oldIndex = new Map<string, number>();
  oldKeys.forEach((key, index) => oldIndex.set(key, index));

  // Kept keys in new order by old position; an increasing run is already in relative order.
  const keptAt: number[] = [];
  const keptKeys: string[] = [];
  for (const key of newKeys) {
    const at = oldIndex.get(key);
    if (at !== undefined) {
      keptAt.push(at);
      keptKeys.push(key);
    }
  }
  const stable = new Set<string>();
  for (const index of longestIncreasing(keptAt)) {
    const key = keptKeys[index];
    if (key !== undefined) stable.add(key);
  }

  let before: string | null = null;
  for (let index = newKeys.length - 1; index >= 0; index -= 1) {
    const key = newKeys[index];
    if (key === undefined) continue;
    if (!oldIndex.has(key)) ops.push({ op: "insert", key, before });
    else if (!stable.has(key)) ops.push({ op: "move", key, before });
    before = key;
  }
  return ops;
}
