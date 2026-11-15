/**
 * The bundle's source maps, read by the benches that name the app's functions in a profile: a position of a served
 * script (a zero-based line and column) as `<source file>:<line>`, or null where the map holds none.
 */

const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const vlq = (segment: string): number[] => {
  const out: number[] = [];
  let value = 0;
  let shift = 0;
  for (const c of segment) {
    const digit = B64.indexOf(c);
    value += (digit & 31) << shift;
    if (digit & 32) {
      shift += 5;
      continue;
    }
    out.push(value & 1 ? -(value >> 1) : value >> 1);
    value = 0;
    shift = 0;
  }
  return out;
};

export type Lookup = (line: number, column: number) => string | null;

const lookupOf = (map: { readonly sources: readonly string[]; readonly mappings: string }): Lookup => {
  const lines: (readonly [number, number, number])[][] = [];
  let source = 0;
  let sourceLine = 0;
  for (const text of map.mappings.split(";")) {
    let column = 0;
    const segments: (readonly [number, number, number])[] = [];
    for (const segment of text.split(",")) {
      if (segment === "") continue;
      const v = vlq(segment);
      column += v[0] ?? 0;
      if (v.length > 1) {
        source += v[1] ?? 0;
        sourceLine += v[2] ?? 0;
      }
      segments.push([column, source, sourceLine]);
    }
    lines.push(segments);
  }
  return (line, column) => {
    let found: readonly [number, number, number] | undefined;
    for (const s of lines[line] ?? []) {
      if (s[0] > column) break;
      found = s;
    }
    return found === undefined ? null : `${(map.sources[found[1]] ?? "?").replace(/^(\.\.\/)+web\//, "")}:${found[2] + 1}`;
  };
};

const maps = new Map<string, Promise<Lookup | null>>();

/** The map served beside `url` (`<url>.map`), fetched once; null when there is none. */
export const mapped = (url: string): Promise<Lookup | null> => {
  const known = maps.get(url);
  if (known !== undefined) return known;
  const read = fetch(`${url}.map`).then(
    (r) => (r.ok ? r.json().then((m: { sources: string[]; mappings: string }) => lookupOf(m)) : null),
    () => null,
  );
  maps.set(url, read);
  return read;
};
