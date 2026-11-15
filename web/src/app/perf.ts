// Time to cards on screen, from the cache and from the network, printed once each in a `--dev` build;
// `__DEV__` is an esbuild define, so production folds it away.

/** The first time is the measurement; a later redraw would bury it. */
const said = new Set<string>();

export function timed(label: string): void {
  if (!__DEV__ || said.has(label)) return;
  said.add(label);
  console.log(`suzatary perf: ${label} at ${performance.now().toFixed(1)} ms`);
}

/** The measures a tab keeps of one name: a tab open for days holds the latest of each, not all of them. */
export const KEPT = 256;

/** A User Timing measure from `start` to now, read by the benches and never by the app. */
export function measure(name: string, start: number, detail?: unknown): void {
  if (performance.getEntriesByName(name, "measure").length >= KEPT) performance.clearMeasures(name);
  performance.measure(name, { start, end: performance.now(), detail });
}
