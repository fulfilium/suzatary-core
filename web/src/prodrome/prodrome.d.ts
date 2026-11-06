/**
 * `suzatary-wasm`'s glue, declared: the module is a build output copied beside the bundle, and a `tsc` that
 * needed a Nix build first would not be a type gate. `core.ts` restates it as `Core` so node tests can hand
 * it the nodejs glue; the names are the Rust ones, documented in `app/wasm/src/`.
 */

// A `.d.ts` beside where the `.js` will be, not a `declare module`, so `import("./prodrome.js")` resolves
// here while `build-web.sh` keeps that specifier external.

/** A set of todo objects, `[{hash, text}]`, read once; `free()` gives its memory back. */
export class Todos {
  constructor(objects: string);
  /** Rehash, parse, derive the tips, walk from every one, linearise. */
  verify(): string;
  /** The folds at an instant, under a trust policy. */
  fold(at: string | null, untrusted: string): string;
  /** The box's fold answer: every todo as its route composes the row. */
  list(at: string | null, untrusted: string, chain: string, others: Chains): string;
  /** `event` sealed into this replica, a `Change`: `{hash, objects}`, the objects the box lacks of it. */
  append(event: string, genesis: string | null): string;
  free(): void;
}

/** The todo chains a tab holds, the set every row is priced through. */
export class Chains {
  constructor();
  hold(name: string, objects: string, untrusted: string): string;
  drop(name: string): void;
  free(): void;
}

/** A store of objects the core holds (`object_rows::Store`), its pull's moves each answering a frame. */
declare class ObjectStore {
  constructor();
  since(): string;
  heads(): string;
  heard(): boolean;
  behind(heads: string): boolean;
  overtaken(epoch: number): boolean;
  went_back(epoch: number): boolean;
  piece(answer: string, fetched_at: number): Uint8Array;
  land(): Step;
  start_over(): Step;
  /** `event` sealed as this tab's `Change`, unsent; none for a twin. */
  append(event: string): Step | undefined;
  unsent(): string;
  answered(receipt: string): Step;
  refuse(names: string, reason: string): Step;
  apply(frame: Uint8Array): string;
  load(bytes: Uint8Array | undefined, frames: Uint8Array): string;
  snapshot(): Uint8Array;
  version(): number;
  tips(): string;
  /** Every entity's registers, each its maximal writes. */
  readings(at: string | null, untrusted: string): string;
  free(): void;
}

/** One move of a store: its frame and what it moved. */
export class Step {
  readonly delta: Uint8Array;
  readonly moved: string;
  free(): void;
}

/** The inbox the tab holds. */
export class Proposals extends ObjectStore {
  /** The Inbox's list at an instant. */
  inbox(at: string, chains: Chains, memory?: string): string;
}

/** The lines a tab holds, each print decoded once, read by what moved since the last reading. */
export class Lines {
  constructor();
  /** `[{hash, text}]` held, each hashed against its name, and `[<name>]` let go. */
  hold(add: string, drop: string): void;
  /** `[{hash, text}]` already admitted by their names held, hashed not again, and `[<name>]` let go. */
  hold_admitted(add: string, drop: string): void;
  /** The keys `{<name>: <hex>}` held and `[<name>]` let go. */
  keys(add: string, drop: string): void;
  /** Every conversation, newest first. */
  list(inbox: Proposals): string;
  /** One conversation's window, the box's route's reading. */
  conversation(
    id: string,
    inbox: Proposals,
    before?: string,
    limit?: string,
    about?: string,
    memory?: string,
  ): string;
  /** Every conversation's turns holding a query. */
  search(q: string): string;
  /** The skeleton's nodes held and let go. */
  nodes(add: string, drop: string): void;
  /** `[<name>]`: every line whose words the stream's newest pages read. */
  chosen(topics: TopicReading, chosen: string, pages: number): string;
  /** The stream's newest pages. */
  stream(topics: TopicReading, chosen: string, pages: number): string;
  /** `{<name>: [<answer key>]}`: the keys an erasure names, and the blocks their words fed. */
  erased(keys: string, told: string): string;
  /** `{excluded}`: what the history leaves out. */
  excluded(): string;
  free(): void;
}

/** The topic store the tab holds. */
export class Memberships extends ObjectStore {}

/** The topic aliases' store the tab holds. */
export class TopicAliases extends ObjectStore {}

/** The topics both replicas read as, and the events that change them, printed for `append`. */
export class TopicReading {
  constructor(memberships: Memberships, aliases: TopicAliases);
  /** The topics in use, as `TopicsPayload`. */
  bar(): string;
  add(target: string, topic: string, at: string, actor: string): string | undefined;
  /** A JSON array of event prints. */
  remove(target: string, topic: string, at: string, actor: string): string;
  /** A JSON array of event prints. */
  rename(from: string, to: string, at: string, actor: string): string;
  free(): void;
}

/** Why a text is no topic, or `undefined` for a topic. */
export function topic_refusal(text: string): string | undefined;

/** What a term is worth at an instant; `undefined` for `∅`. */
export function fulfillment(term: string, now: string, env: string): number | undefined;

/** Every `ref` bound to the todo it names. */
export function link(term: string, specs: string): string;

/** The list's tree of `members`, grouped `by` and then by ancestry, each node with its term. */
export function grow(members: string, by: string): string;

/** A stored term's canonical print, as the JSON shape. */
export function term_json(literal: string): string;

/** The curve over a window, and whether the knots are it. */
export function series_knots(term: string, from: string, to: string, history: string): string;

/** One lifecycle event, through its own `mk_*`, as its canonical print. */
export function lifecycle(kind: string, todo: string, at: string, actor: string, note: string): string;

/** The prose escaped markup stands for, or `undefined` when it is markup a person wrote. */
export function prose(markup: string): string | undefined;

/** The box's tripwire over typst's HTML: what is wrong with it, or `undefined`. */
export function suspicious(html: string): string | undefined;
/** How many values the module has scrubbed since it was loaded; read by the browser scenes. */
export function scrubs(): number;
/** How many prints the lines have decoded since the module was loaded; read by the browser scenes. */
export function decodes(): number;
/** How many of those prints were hashed against their names first; read by the browser scenes. */
export function hashes(): number;
/** How many sealed slots the module has decrypted since it was loaded; read by the browser scenes. */
export function opens(): number;

/** Every function above throws until this resolves, so `core.ts` alone calls it and memoises it. */
export default function init(init: { module_or_path: string }): Promise<unknown>;
