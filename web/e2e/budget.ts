import { availableParallelism, loadavg } from "node:os";

/**
 * How long a wait may take, from `SUZATARY_E2E_WAIT_MS`: every wait is real time, because the page holds an SSE
 * connection and Chromium's virtual-time budgets then never expire. A value that is not a positive number is the
 * default, so a mistyped variable cannot make every timeout zero.
 */
function budget(): number {
  const asked = Number(process.env["SUZATARY_E2E_WAIT_MS"]);
  return Number.isFinite(asked) && asked > 0 ? asked : loaded();
}

/** An unset budget, read from the box's load: at or over one runnable task per core it grows with the load, to 60 s. */
function loaded(): number {
  const [load = 0] = loadavg();
  const cores = availableParallelism();
  return load < cores ? BASELINE_MS : Math.min(60000, Math.round(BASELINE_MS * (1 + load / cores)));
}

/** The default budget, the scale every hand-written timeout in this suite was chosen against. */
export const BASELINE_MS = 15000;

export const WAIT_MS = budget();
