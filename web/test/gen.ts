/**
 * THE GENERATOR — the web app's `proptest`, by hand
 * (`docs/WEBALGEBRA-PLAN-2026-09-16.md` §3; `docs/WEB-SPEC.md` §3).
 *
 * The Rust side checks its laws on random logs because an example is a
 * sentence and a property is a rule. This module is what lets the TypeScript
 * side say the same rules over the same kind of input, with no dependency:
 * a seeded xorshift (`reconcile.test.ts` had one inline since 2026-09-04;
 * this is that one, shared), a few combinators, and one generator per type a
 * law is stated over. Deterministic by construction — a failing seed is a
 * reproducible case, and the seed is in the assertion message.
 *
 * ⚠️ NOT A DEPENDENCY, ON PURPOSE. The app has no npm tree and takes none;
 * twenty lines of xorshift against a package is the same trade the reconcile
 * test made. Nothing here is clever: `int` is modulo, `pick` is `int` over an
 * array, `array` is a length and a loop.
 */

import type { Route } from "../src/shell/route";
import type { Authored, Block, Entry, FailureClass, Group, LineStanding, OverviewLine } from "../src/wire/generated/wire";
import lo from "../src/algebra/lo";
import type kv from "../src/app/kv";

/** A seeded source of numbers. Same seed, same sequence, every run. */
export interface Rng {
  /** A non-negative integer below `bound` (`bound` ≥ 1). */
  int: (bound: number) => number;
  /** One of the given values. */
  pick: <T>(of: readonly T[]) => T;
  /** True with the given probability (default one half). */
  bool: (probability?: number) => boolean;
  /** A string of `length` characters drawn from `alphabet`. */
  string: (alphabet: string, length: number) => string;
  /** `length` values from `each`. */
  array: <T>(length: number, each: () => T) => T[];
}

export function rng(seed: number): Rng {
  let state = seed >>> 0 || 0x5eed;
  const next = (): number => {
    state ^= state << 13;
    state >>>= 0;
    state ^= state >>> 17;
    state ^= state << 5;
    state >>>= 0;
    return state;
  };
  const int = (bound: number): number => next() % Math.max(1, bound);
  const source: Rng = {
    int,
    pick: (of) => of[int(of.length)] as (typeof of)[number],
    bool: (probability = 0.5) => next() / 0x1_0000_0000 < probability,
    string: (alphabet, length) => {
      let out = "";
      for (let i = 0; i < length; i += 1) out += alphabet[int(alphabet.length)];
      return out;
    },
    array: (length, each) => {
      const out = [];
      for (let i = 0; i < length; i += 1) out.push(each());
      return out;
    },
  };
  return source;
}

/** Run `check` over `runs` seeds, naming the seed that fails. */
export function forAll(runs: number, check: (r: Rng, seed: number) => void): void {
  for (let seed = 1; seed <= runs; seed += 1) {
    try {
      check(rng(seed * 2654435761), seed);
    } catch (error: unknown) {
      const said = error instanceof Error ? error.message : String(error);
      throw new Error(`seed ${seed}: ${said}`);
    }
  }
}

/** `forAll` of a check that answers later, each seed's run awaited before the next. */
export async function forAllAsync(runs: number, check: (r: Rng, seed: number) => Promise<void>): Promise<void> {
  for (let seed = 1; seed <= runs; seed += 1) {
    await check(rng(seed * 2654435761), seed).catch((error: unknown) => {
      const said = error instanceof Error ? error.message : String(error);
      throw new Error(`seed ${seed}: ${said}`);
    });
  }
}

/** The characters an id, a hash or a slug is spelled with. */
export const SLUG = "abcdefghijklmnopqrstuvwxyz0123456789-";

/** A key list for the reconciler: distinct short slugs. */
export function keys(r: Rng, upTo = 12): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  const length = r.int(upTo + 1);
  while (out.length < length) {
    const key = r.string(SLUG, 1 + r.int(4));
    if (!seen.has(key)) {
      seen.add(key);
      out.push(key);
    }
  }
  return out;
}

/** A key list derived from another: some removed, some moved, some new. */
export function reshuffled(r: Rng, from: readonly string[]): string[] {
  const kept = from.filter(() => r.bool(0.8));
  for (let i = kept.length - 1; i > 0; i -= 1) {
    if (r.bool(0.3)) {
      const j = r.int(i + 1);
      [kept[i], kept[j]] = [kept[j] as string, kept[i] as string];
    }
  }
  const fresh = keys(r, 4).filter((key) => !from.includes(key));
  const at = r.int(kept.length + 1);
  return [...kept.slice(0, at), ...fresh, ...kept.slice(at)];
}

/** A reference's text form, in one of the five spellings. */
export function referenceText(r: Rng): string {
  switch (r.int(5)) {
    case 0:
      return `todo:${r.pick(["life", "suzatary"])}/${r.string(SLUG, 1 + r.int(12))}`;
    case 1:
      return `mail:${r.string("0123456789abcdef", 8 + r.int(8))}`;
    case 2:
      return `thread:${r.string(SLUG, 4 + r.int(10))}`;
    case 3:
      return `briefing:2026-09-${String(1 + r.int(28)).padStart(2, "0")}`;
    default:
      return `proposal:default/${r.int(20)}`;
  }
}

/** An ISO date, September 2026. */
export function day(r: Rng): string {
  return `2026-09-${String(1 + r.int(30)).padStart(2, "0")}`;
}

/**
 * A ROUTE, one arm at a time — every arm the sum has, so a law over routes
 * covers the whole grammar. Nothing checks that this list is complete: a new
 * arm of `Route` needs its case added here by hand.
 */
export function route(r: Rng): Route {
  const id = (): string => r.string(SLUG, 1 + r.int(16));
  const refs = (): string[] => r.array(r.int(3), () => referenceText(r));
  const topics = (): string[] => r.array(r.int(3), () => r.pick(["garden", "invoices", "trip", "trip-plan", "سفر"]));
  switch (r.int(20)) {
    case 0:
      return { type: "todos" };
    case 1:
      return { type: "project" };
    case 2:
      return { type: "add", text: r.string(`${SLUG} `, r.int(20)), title: r.string(SLUG, r.int(10)) };
    case 3:
      return { type: "todo", id: id() };
    case 4:
      return { type: "inbox" };
    case 5:
      return { type: "mail", query: r.string(`${SLUG}: `, r.int(16)), account: r.string(SLUG, r.int(8)) };
    case 6:
      return { type: "message", id: r.string("0123456789abcdef", 8 + r.int(8)) };
    case 7:
      return { type: "thread", id: id() };
    case 8:
      return { type: "calendar", date: r.bool() ? "" : day(r) };
    case 9:
      return { type: "settings" };
    case 10:
      return r.bool() ? { type: "memory" } : { type: "newtopic" };
    case 11:
      return { type: "chain", focus: r.bool() ? "" : r.string("0123456789abcdef", 8) };
    case 12:
      return { type: "calendar", date: r.bool() ? "" : day(r) };
    case 13:
      return { type: "usage" };
    case 14:
      return { type: "about" };
    case 15:
      return { type: "briefings" };
    case 16:
      return { type: "briefing", date: day(r) };
    case 17:
      return r.bool() ? { type: "secretary", refs: refs(), topics: topics() } : { type: "secretary", refs: refs(), topics: topics(), focus: true };
    case 18:
      return { type: "typst", open: r.bool() ? "" : id() };
    default: {
      const at = r.bool() ? { at: r.int(400) } : {};
      return r.bool()
        ? { type: "conversation", id: r.pick(["default", id()]), refs: refs(), ...at }
        : { type: "conversation", id: r.pick(["default", id()]), refs: refs(), focus: true, ...at };
    }
  }
}

/** A viewport width, spanning both sides of the 62rem rule. */
export function width(r: Rng): number {
  return r.pick([320, 390, 414, 600, 991, 992, 1024, 1280, 1920]);
}

/** A value on its way over `value`, each arm equally likely. */
export function loState<S>(r: Rng, value: () => S): lo.T<S> {
  return r.pick<() => lo.T<S>>([
    () => lo.loading,
    () => lo.error(r.string("xyz", 3)),
    () => lo.val(value()),
  ])();
}

/** Text a mail could carry: words, and every character typst, HTML or a URL would read as structure. */
export const HOSTILE = ["#strong[x]", "<script>alert(1)</script>", "]", "#link(\"javascript:alert(1)\")[go]", "```raw```", "$x$", "\\", "\"", "&amp;"];

/** One fold entry, authored or not, in any state, with any value. */
/** A record drawn as `block`, its prose plain words: for a row whose content is beside the point. */
export function authoredAs(block: Block, body = "A thing to do"): Authored {
  return {
    rich: { body: null, detail: null, rationale: null, waiting_on: null, keys: { body: null, detail: null, rationale: null, waiting_on: null } },
    block,
    created: "2026-09-17",
    body,
    text: body,
    detail: "",
    category: "",
    waiting_on: "",
    rationale: [],
    spec: null,
    actor: "owner",
    at: "",
    source: null,
    subtodos: [],
  };
}

export function entry(r: Rng): Entry {
  const words = (): string => r.array(r.int(4), () => r.bool(0.7) ? r.string("abcdefgh ", 1 + r.int(8)) : r.pick(HOSTILE)).join(" ");
  return {
    todo: r.string(SLUG.replace("-", ""), 1 + r.int(10)),
    state: r.pick(["open", "open", "completed", "cancelled"]),
    states: [],
    at: "",
    claimed: "",
    unconfirmed: r.bool(0.2),
    recurring: r.bool(0.2),
    value: r.bool(0.9) ? r.int(101) / 100 : null,
    conflicts: {},
    authored: r.bool(0.1)
      ? null
      : {
          rich: { body: null, detail: null, rationale: null, waiting_on: null, keys: { body: null, detail: null, rationale: null, waiting_on: null } },
          block: r.pick<Block>([{ kind: "todo", name: "todo" }, { kind: "todo", name: "ekaTodo" }, { kind: "decision" }]),
          created: day(r),
          ...((b: string) => ({ body: b, text: b }))(words()),
          detail: words(),
          category: r.pick(["home", "work", "study", ""]),
          waiting_on: "",
          rationale: r.array(r.int(3), words),
          spec: null,
          actor: "owner",
          at: "",
          source: null,
          subtodos: [],
        },
    candidates: [],
    stream: [],
  };
}

/** A browser storage in memory: one per device or tab a test plays. */
export function memoryStore(): kv.Store & { size: () => number; keys: () => string[] } {
  const kept = new Map<string, string>();
  return {
    getItem: (k) => kept.get(k) ?? null,
    setItem: (k, v) => void kept.set(k, v),
    removeItem: (k) => void kept.delete(k),
    size: () => kept.size,
    keys: () => [...kept.keys()],
  };
}

/** Every group, in the box's order. */
export const GROUPS: readonly Group[] = ["models", "mail", "calendar", "drive", "storage", "notifications", "coding", "plugins", "box"];

/** A failure's class, every arm. */
function failureClass(r: Rng): FailureClass {
  return r.pick<FailureClass>([
    { kind: "revoked", status: r.pick(["unauthorized", "forbidden", "not_found"] as const) },
    { kind: "needs_login" },
    { kind: "unresolved" },
    { kind: "key_refused" },
    { kind: "unusable" },
    { kind: "unreachable" },
    { kind: "unclassified" },
  ])
}

/** A line's standing, every arm. */
function lineStanding(r: Rng): LineStanding {
  const at = `2026-10-0${1 + r.int(9)}T0${r.int(10)}:00:00`;
  const words = (): string => r.string("abc ", 8);
  return r.pick<() => LineStanding>([
    () => ({ kind: "kept" }),
    () => ({ kind: "synced", at }),
    () => ({ kind: "answered", at }),
    () => ({ kind: "unobserved" }),
    () => ({ kind: "locked" }),
    () => ({ kind: "store_failed" }),
    () => ({ kind: "unreadable", words: words() }),
    () => ({ kind: "by_hand", command: words(), why: words() }),
    () => ({ kind: "failing", since: at, class: failureClass(r), words: words(), at }),
    () => ({ kind: "missing", what: r.array(1 + r.int(2), () => `the ${r.string("abc", 4)} key`) }),
  ])();
}

/** A card's reference, as the box names one. */
function cardRef(r: Rng): string {
  return `proposal:${r.pick(["setup", "secretask", "c"])}/${r.int(6)}`;
}

export function overviewLine(r: Rng): OverviewLine {
  return {
    key: `line:${r.string(SLUG, 6)}`,
    label: r.pick(["Work", "Home", "Data repository", "Chat", "ada@example.test"]),
    detail: r.bool() ? null : r.string("abc ", 12),
    standing: lineStanding(r),
    cards: r.array(r.int(3), () => cardRef(r)),
  };
}
