/**
 * The reader's machine checks the store without the core: the browser never computes a fulfillment, but it
 * verifies, because handing over bytes and hashes is a different claim from asking to be believed.
 * Checked: each object hashes to its name, the walk from every head reaches genesis without a missing link,
 * the reached objects have no cycle, and nothing sent is left off the walk.
 */

import type { Condition, Found, Link, ObjectVerdict, Walk } from "../wire/generated/wire";

/**
 * Where the walk put an object, what verifying found and one object's verdict: the wire's (`wire::Walk`,
 * `wire::Found`, `wire::ObjectVerdict`), so this browser's check and the core's say it in one shape.
 */
export type { Condition, Found, ObjectVerdict, Walk };

export namespace found {
  export const REACHED: Found = { type: "Reached" };

  /** The one way in: no problem is `Reached`, any is `Unreached` with all of them. */
  export const of = (problems: readonly string[]): Found =>
    problems.length === 0 ? REACHED : { type: "Unreached", problems: [...problems] };

  export const fold = <R>(f: Found, on: { reached: () => R; unreached: (problems: readonly string[]) => R }): R =>
    f.type === "Reached" ? on.reached() : on.unreached(f.problems);

  /** Every problem found, none for `Reached`: what a list of them shows. */
  export const problems = (f: Found): readonly string[] => fold<readonly string[]>(f, { reached: () => [], unreached: (ps) => ps });
}

export interface ChainVerdict {
  tips: string[];
  /** Walk order, heads first, then any object the walk never reached. */
  objects: ObjectVerdict[];
  walked: number;
  /** How many objects hashed to their own name. */
  verified: number;
  /** The genesis objects the walk ended at: a legacy root or a `Genesis`, never a change with no deps. */
  genesis: string[];
  found: Found;
}

const encoder = new TextEncoder();

/** Over the UTF-8 print with no trailing newline: both the object's name and the hash of the file a third party downloads. */
export async function sha256Hex(literal: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", encoder.encode(literal));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export namespace condition {
  export const CLEAN: Condition = { type: "Clean" };
  export const flawed = (problem: string): Condition => ({ type: "Flawed", problem });
  /** What is wrong, as a table cell says it: nothing for a clean object. */
  export const said = (c: Condition): string => (c.type === "Clean" ? "" : c.problem);
}

function blank(link: Link, computed: string, condition: Condition): ObjectVerdict {
  const base = { hash: link.hash, computed, hashOk: computed === link.hash, walk: UNREACHED, condition };
  return link.holds === "envelope"
    ? { ...base, holds: "envelope", kind: link.kind }
    : { ...base, holds: "event", kind: link.kind, todo: link.todo, actor: link.actor, at: link.at };
}

const UNREACHED: Walk = { type: "Unreached" };

// Read off the object's own constructor: a change that supersedes nothing names no parents and begins nothing.
const isRoot = (link: Link): boolean => /^(Genesis\(|Sealed\(prev='',)/.test(link.literal);



/** Indexed by the claimed name, not the computed hash, so a tampered object reads as tampered rather than as a broken link. */
export async function verifyChain(links: readonly Link[], tips: readonly string[]): Promise<ChainVerdict> {
  const problems: string[] = [];
  const verdicts = new Map<string, ObjectVerdict>();
  const byHash = new Map<string, Link>();

  for (const link of links) {
    const computed = await sha256Hex(link.literal);
    if (verdicts.has(link.hash)) {
      problems.push(`object ${link.hash} was sent twice`);
      continue;
    }
    const verdict = blank(
      link,
      computed,
      computed === link.hash ? condition.CLEAN : condition.flawed(`does not hash to its own name — this browser computed ${computed}`),
    );
    if (verdict.condition.type === "Flawed") problems.push(`object ${link.hash} ${verdict.condition.problem}`);
    verdicts.set(link.hash, verdict);
    byHash.set(link.hash, link);
  }

  const walk: ObjectVerdict[] = [];
  const seen = new Set<string>();
  const parents = new Map<string, readonly string[]>();
  const genesis: string[] = [];
  if (tips.length === 0) {
    problems.push("the server named no tip");
  }
  // Breadth first from every head at once, so `depth` is the distance from the nearest head.
  const queue: Array<{ name: string; depth: number }> = tips.map((name) => ({ name, depth: 0 }));
  for (let i = 0; i < queue.length; i += 1) {
    const step = queue[i];
    if (step === undefined) continue;
    const { name, depth } = step;
    if (seen.has(name)) continue;
    const verdict = verdicts.get(name);
    if (verdict === undefined) {
      problems.push(
        depth === 0
          ? `the tip ${name} was not sent with the chain`
          : `object ${name} is named as a parent but was not sent`,
      );
      continue;
    }
    seen.add(name);
    verdict.walk = { type: "Reached", depth };
    walk.push(verdict);
    const link = byHash.get(name);
    // Cannot be undefined: `byHash` and `verdicts` are filled from one loop.
    const named = link === undefined ? [] : link.parents;
    parents.set(name, named);
    if (link !== undefined && isRoot(link)) genesis.push(name);
    for (const parent of named) queue.push({ name: parent, depth: depth + 1 });
  }
  if (genesis.length === 0 && problems.length === 0) {
    problems.push("the walk ended without reaching a genesis object (a root or a Genesis)");
  }
  problems.push(...cycles(seen, parents));

  const orphans = [...verdicts.values()].filter((verdict) => verdict.walk.type === "Unreached");
  for (const orphan of orphans) {
    if (orphan.condition.type === "Clean") orphan.condition = condition.flawed("not reachable from any head by its parents");
    problems.push(`object ${orphan.hash} is not reachable from any head`);
  }

  const objects = [...walk, ...orphans];
  return {
    tips: [...tips],
    objects,
    walked: walk.length,
    verified: objects.filter((verdict) => verdict.hashOk).length,
    genesis,
    found: found.of(problems),
  };
}

/** The walk's visited set hides a cycle, so a topological peel finds it: whatever is left unpeeled. */
function cycles(seen: ReadonlySet<string>, parents: ReadonlyMap<string, readonly string[]>): string[] {
  const waiting = new Map<string, number>();
  const children = new Map<string, string[]>();
  for (const name of seen) {
    const named = (parents.get(name) ?? []).filter((parent) => seen.has(parent));
    waiting.set(name, named.length);
    for (const parent of named) children.set(parent, [...(children.get(parent) ?? []), name]);
  }
  const ready = [...waiting.entries()].filter(([, count]) => count === 0).map(([name]) => name);
  let ordered = 0;
  for (let i = 0; i < ready.length; i += 1) {
    const name = ready[i];
    if (name === undefined) continue;
    ordered += 1;
    for (const child of children.get(name) ?? []) {
      const left = (waiting.get(child) ?? 0) - 1;
      waiting.set(child, left);
      if (left === 0) ready.push(child);
    }
  }
  if (ordered === seen.size) return [];
  const stuck = [...seen].filter((name) => (waiting.get(name) ?? 0) > 0).sort();
  return [`objects ${stuck.join(", ")} rest on each other — parents form a cycle, not a history`];
}

/** Written here so the tests pin the claim, not just the arithmetic behind it. */
export function summarise(verdict: ChainVerdict): string {
  return found.fold(verdict.found, {
    reached: () =>
      `${verdict.verified} objects verified by this browser: every one hashes to its own name, and the parent walk reaches genesis from every head without a gap.`,
    unreached: (problems) => `${verdict.verified} of ${verdict.objects.length} objects hash to their own name; ${problems.length} problem(s) found by this browser.`,
  });
}
