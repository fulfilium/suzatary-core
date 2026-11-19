import { availableParallelism, loadavg } from "node:os";

/**
 * HOW LONG A WAIT IS ALLOWED TO TAKE, and why it is a knob.
 *
 * Every wait in this suite is REAL time — the page holds an SSE connection, so
 * Chromium's virtual-time budgets never expire and polling is the honest way to
 * wait (see this file's header). Real time is a property of the MACHINE, and
 * this suite runs on two very different ones: a box with the chain already warm
 * in its page cache, and a shared GitHub runner starting cold beside two other
 * jobs. A number tuned for the first is a flake on the second, and a number
 * tuned for the second makes every local failure take a minute to report.
 *
 * So it is one environment variable, read once, which a slow machine sets
 * higher. It is a BUDGET and not a delay: a wait that
 * is satisfied in 40 ms still costs 40 ms whatever this says.
 *
 * A value that is not a positive number is the default, because a mistyped
 * variable that silently made every timeout zero would turn one flake into a
 * suite that cannot pass at all.
 */
function budget(): number {
  const asked = Number(process.env["SUZATARY_E2E_WAIT_MS"]);
  return Number.isFinite(asked) && asked > 0 ? asked : loaded();
}

/**
 * An UNSET budget, read from the box's own load rather than assumed idle: the
 * baseline is a judgement made on an idle box, and under whatever else is
 * compiling beside chromium a scene green in isolation can lose a pure wait.
 * At or over one runnable task per core it grows with the load, to 60 s.
 */
function loaded(): number {
  const [load = 0] = loadavg();
  const cores = availableParallelism();
  return load < cores ? BASELINE_MS : Math.min(60000, Math.round(BASELINE_MS * (1 + load / cores)));
}

/**
 * The DEFAULT budget, and therefore the scale every hand-written timeout in
 * this suite was chosen against.
 *
 * Fifty-odd scenes name their own numbers — 4 s for a toast, 8 s for a card,
 * 30 s for the WebAssembly core — and each of those is a judgement about how
 * long that ONE thing takes RELATIVE to everything else, made on a developer's
 * box. Editing fifty numbers to survive a slow runner would be editing fifty
 * judgements; multiplying them by one factor keeps every relative claim and
 * moves the whole suite onto the machine it is running on. Locally the factor
 * is 1 and nothing changes at all.
 */
export const BASELINE_MS = 15000;

export const WAIT_MS = budget();
