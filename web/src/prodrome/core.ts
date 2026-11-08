/**
 * prodrome-core as WebAssembly in this tab, so the tab verifies and folds with the box's own evaluator and the
 * chain has one reading; `verify.ts`'s SubtleCrypto walk is the labelled fallback. `loadCore()` is the only effect.
 */

import type {
  ConversationPayload,
  ConversationsPayload,
  FoldPayload,
  InboxProposalsPayload,
  MemoryPayload,
  Phase,
  ProvenanceRow,
  SearchPayload,
  SkeletonNode,
  StreamPayload,
  TopicsPayload,
  SpecNode,
  VerifiedPayload,
} from "../wire/generated/wire";
import once from "../algebra/once";
import register from "../algebra/register";
import d from "../wire/decoder";
import payload from "../wire/generated/payload";
import content from "./content";
import excluded from "./excluded";
import type lines from "./lines";
import type { Replica } from "./replica";

/**
 * Hand-written because the generated `prodrome.d.ts` is a build output `tsc` cannot wait for; the names
 * are the Rust ones so a grep lands on both sides.
 */
export interface Core {
  Todos: new (objects: string) => Todos;
  Proposals: new () => Proposals;
  Lines: new () => Lines;
  Memberships: new () => TopicStore;
  TopicAliases: new () => TopicStore;
  TopicReading: new (memberships: TopicStore, aliases: TopicStore) => TopicReading;
  Chains: new () => Chains;
  /** `undefined` where the term reads `∅`. */
  fulfillment: (term: string, now: string, env: string) => number | undefined;
  link: (term: string, specs: string) => string;
  series_knots: (term: string, from: string, to: string, history: string) => string;
  term_json: (literal: string) => string;
  grow: (members: string, by: string) => string;
  lifecycle: (kind: string, todo: string, at: string, actor: string, note: string) => string;
  /** `unescape_markup`: the prose behind escaped markup, or `undefined` for markup a person wrote. */
  prose: (markup: string) => string | undefined;
  /** The box's tripwire, `suzatary_core::tripwire`: the reason a compiled page is refused, or `undefined`. */
  suspicious: (html: string) => string | undefined;
  /** `prodrome::memo::name`: a node's content name, its own bytes and its children's names in order. */
  memo_name: (own: string, children: string[]) => string;
  /** The key the box keeps an answer block's HTML under: the answer function's name and the block's. */
  answer_key: (source: string) => string;
  /** `[<name>, …]`: the functions a device keeps blocks of (`rendering::kept`). */
  kept_functions: () => string;
  /** What an erasure does, the box's card's own sentence (`Erasure::for_good`). */
  erase_warning: () => string;
  /** `{hash, text}`: the `Signed` of `object` by `key`, refused unless `signature` verifies. */
  signed: (object: string, key: string, signature: string) => string;
  /** Why a text is no topic (`Topic::new`'s refusal), or `undefined` for a topic. */
  topic_refusal: (text: string) => string | undefined;
}

/** A set of todo objects read once by the core; every method asks its question of what was read. */
export interface Todos {
  verify: () => string;
  /** `verify`, with history beneath the epoch read for what it is, history before signatures: the box's own reading. */
  verify_history: () => string;
  fold: (at: string | null, untrusted: string) => string;
  /** The box's fold answer, priced through the set of these objects as `chain` and every chain `others` holds. */
  list: (at: string | null, untrusted: string, chain: string, others: Chains) => string;
  append: (event: string, genesis: string | null) => string;
  /** The name of the registration a device card asks for, in these objects' first prodrome. */
  key_added: (actor: string, key: string) => string;
  /** `trusted`, `untrusted` or `revoked` under `root`. */
  standing: (root: string | null, actor: string, key: string) => string;
  /** `[{hash, kind, key, said}]`: who wrote each object carrying an event, under `root`. */
  provenance: (root: string | null) => string;
  free: () => void;
}

/**
 * A store of objects the core holds (`object_rows::Store`): what its pull asks and each move it makes, each answering the
 * frame the worker keeps; what the tab wrote and the box's answers to it; and its readings, read once per move of what
 * they read, which `version` counts.
 */
export interface ObjectStore {
  since: () => string;
  heads: () => string;
  heard: () => boolean;
  behind: (heads: string) => boolean;
  overtaken: (epoch: number) => boolean;
  went_back: (epoch: number) => boolean;
  /** The box's `StorePayload` kept as a piece beside the landed row, heard at `fetchedAt`: its frame. */
  piece: (answer: string, fetchedAt: number) => Uint8Array;
  land: () => LineStep;
  start_over: () => LineStep;
  /** `event` sealed as a `Change` over the store's reading and kept as the tab's, unsent; none for a twin. */
  append: (event: string) => LineStep | undefined;
  /** `[<print>, …]`: what the tab wrote that the box may take and has not. */
  unsent: () => string;
  /** The box's `ReceiptPayload` for a post of what the tab wrote. */
  answered: (receipt: string) => LineStep;
  /** The objects `names` (a JSON array) refused for good in the box's words. */
  refuse: (names: string, reason: string) => LineStep;
  apply: (frame: Uint8Array) => string;
  load: (bytes: Uint8Array | undefined, frames: Uint8Array) => string;
  snapshot: () => Uint8Array;
  version: () => number;
  tips: () => string;
  readings: (at: string | null, untrusted: string) => string;
  free: () => void;
}

/** The inbox the tab holds. */
export interface Proposals extends ObjectStore {
  /** The Inbox's list at `at`, each proposal priced through `chains`: the box's own answer. */
  inbox: (at: string, chains: Chains, memory?: string) => string;
}

/** The todo chains held, the set a proposal is priced through: each its objects, read once, and its policy. */
export interface Chains {
  /** `{excluded}`, what the chain's reading leaves out, or `{why}` it did not read. */
  hold: (name: string, objects: string, untrusted: string) => string;
  drop: (name: string) => void;
  /** The set at `at`, read once, to link every term of one reading in a chain whose functions are `specs`. */
  linker: (specs: string, at: string) => Linker;
  free: () => void;
}

/** The set at one instant, read once: a term linked through it binds as the box binds a row. */
export interface Linker {
  link: (term: string) => string;
  free: () => void;
}

/** One chain as the tab holds it, by name: its replica whole, so its objects, heads and policy are read off one value. */
export interface HeldChain {
  readonly name: string;
  readonly replica: Replica;
}

const Held = (chain: string): d.Decoder<excluded.Left> =>
  d.alt(
    d.map(obj<{ why: string }>({ why: str }), (h) => excluded.left(excluded.NONE, [{ chain, why: h.why }])),
    d.map(Tipped, (t) => excluded.left(t.excluded)),
  );

/** `objects` held in `chains` as `name`, and what its reading left out. */
export function holdIn(chains: Chains, name: string, objects: readonly Named[], untrusted: readonly string[]): excluded.Left {
  return checked("hold", chains.hold(name, objectsOf(objects), JSON.stringify(untrusted)), Held(name));
}

/** One move of the lines' rows: its delta's frame and the conversations it moved; freed once read. */
export interface LineStep {
  readonly delta: Uint8Array;
  readonly moved: string;
  free: () => void;
}

/**
 * The lines a tab holds, each print decoded once as it is held and each key by name, read through the core's memoised
 * fold, so a reading computes only the conversations and lines that moved since the last one. A slot without its key
 * reads as missing, one an erasure names as erased.
 */
export interface Lines {
  hold: (add: string, drop: string) => void;
  /** `hold` of literals the content's admission checked against their names, so none is hashed again. */
  hold_admitted: (add: string, drop: string) => void;
  /** `[<name>, …]`: every print held. */
  held: () => string;
  /** Moves with each change of the prints held. */
  version: () => number;
  /** The frontier a pull names as `since`, of what is held, landed and not; `""` for none. */
  since: () => string;
  /** `[<name>, …]`, sorted: the heads of what is held, landed and not. */
  heads: () => string;
  /** Whether the landed row has heard the box. */
  heard: () => boolean;
  /** Whether the box's heads (`[<name>, …]`) name a node what is held does not read. */
  behind: (heads: string) => boolean;
  /** Whether an answer of this epoch is of a later epoch than either row heard: the store starts over first. */
  overtaken: (epoch: number) => boolean;
  /** Whether an answer of this epoch is of an earlier epoch than the word held. */
  went_back: (epoch: number) => boolean;
  /** A skeleton answer (`SkeletonPayload` JSON) kept as a piece holding the keys `{<name>: <hex>}`, heard at ms: the delta's frame. */
  piece: (answer: string, keys: string, fetchedAt: number) => Uint8Array;
  /** The pending pieces landed. */
  land: () => LineStep;
  /** Every row reset, in the generation after both. */
  start_over: () => LineStep;
  /** The keys `[<name>, …]` let go from every row, and each line every key of whose words is then gone kept as its husk. */
  forget: (gone: string) => LineStep;
  /** `[<name>, …]`: every line held as its husk, whose print the device deletes. */
  husks: () => string;
  /** Of the block keys `[<key>, …]`, those a device may keep: an answer key only while the held keys' words feed it. */
  admits: (keys: string) => string;
  /** Whether a line the rows read is held with words each key of which is gone: one a purge husks. */
  husking: () => boolean;
  /** Another tab's frame taken: the conversations moved, `"All"` or `[<id>, …]`. */
  apply: (frame: Uint8Array) => string;
  /** `{landed, pending, holding, landings}`: each row's generation, epoch, box word, frontier, nodes read, heads and keys. */
  rows: () => string;
  /** The nodes a scope reads, of the landed row and a pull's pieces, whose print is not held. */
  lacking: (scope: string) => string;
  /** `{nodes, lines}`: how many nodes a scope reads, and of its line nodes, lack their print. */
  lacks: (scope: string) => string;
  /** Whether the skeleton holds a line of a conversation. */
  holds: (id: string) => boolean;
  /** `{<name>: [<answer key>, …]}`: every dead key held, landed or pending, with the blocks its words fed. */
  owed: (told: string) => string;
  /** `{keep, ahead}`: the names the device keeps within a budget, and the plan's nodes lacking their print. */
  plan: (budget: number) => string;
  list: (inbox: Proposals) => string;
  conversation: (
    id: string,
    inbox: Proposals,
    before: string | undefined,
    limit: string | undefined,
    about: string | undefined,
    memory: string | undefined,
  ) => string;
  search: (q: string) => string;
  /** `[<node>, …]`: the node of every line whose words the newest `pages` pages of the stream read, by the skeleton alone. */
  chosen: (topics: TopicReading, chosen: string, pages: number) => string;
  /** The rows and the husks, whole, in the core's own form; it throws where they do not print. */
  snapshot: () => Uint8Array;
  /** The replica replaced by a snapshot's (none for the empty one), then each frame of `frames` taken in order, each
   * frame after its length as four little-endian bytes, the prints held kept and each husk standing for its line: the
   * conversations moved. It throws for a snapshot of another form or a frame that does not read, holding what it held. */
  load: (bytes: Uint8Array | undefined, frames: Uint8Array) => string;
  /** The newest `pages` pages of every conversation's rounds carrying each topic of `chosen`, a JSON array. */
  stream: (topics: TopicReading, chosen: string, pages: number) => string;
  /** `{<name>: [<answer key>, …]}`: every key an erasure names, with the blocks its words fed where `keys` opens it. */
  erased: (keys: string, told: string) => string;
  excluded: () => string;
  free: () => void;
}

/** A topic store the tab holds: the memberships, or the aliases. */
export type TopicStore = ObjectStore;

/** The topics both replicas read as, and the events that change them, printed for `append`. */
export interface TopicReading {
  /** The topics in use, as `TopicsPayload`. */
  bar: () => string;
  add: (target: string, topic: string, at: string, actor: string) => string | undefined;
  /** A JSON array of event prints. */
  remove: (target: string, topic: string, at: string, actor: string) => string;
  /** A JSON array of event prints. */
  rename: (from: string, to: string, at: string, actor: string) => string;
  free: () => void;
}

/** Not an `ApiError`: an API failure means ask again, a core failure means fall back and say so. */
export class CoreError extends Error {}

// --- the shell: loading it ---------------------------------------------------

interface Glue extends Core {
  default: (init: { module_or_path: Response }) => Promise<unknown>;
}

/**
 * Lazy, because most visits only look at the list and the `.wasm` is a few hundred kilobytes; hence a
 * runtime `import()` that `build-web.sh` keeps external. A failed load is not kept, so a retry retries.
 */
export const loadCore: () => Promise<Core> = once.of(async (): Promise<Core> => {
  try {
    // The glue and the module in one round trip; the glue compiles the module while it downloads.
    const [glue, response] = await Promise.all([
      import("./prodrome.js") as Promise<unknown> as Promise<Glue>,
      fetch(new URL("./prodrome_bg.wasm", import.meta.url).href),
    ]);
    await glue.default({ module_or_path: response });
    return glue;
  } catch (error: unknown) {
    // Loaded only on a failure, so the worker's bundle, which reads no page, evaluates none of the page's modules.
    void import("../app/beacon").then((b) => b.default.tell("core", error));
    throw new CoreError(`the core could not be loaded: ${String(error)}`);
  }
}).get;

// --- the answers' shapes ------------------------------------------------------

function checked<T>(what: string, text: string, decoder: d.Decoder<T>): T {
  const r = d.json(decoder)(text);
  if (r.type === "Val") return r.v;
  throw new CoreError(`${what}: unexpected shape (${r.e})`);
}

/** A todo the list places, and where a node of its tree stands. */
export type CorePlaced = {
  readonly todo: string;
  readonly title: string;
  readonly category: string;
  readonly worth: { readonly type: "Value"; readonly v: number | null } | { readonly type: "Unlinked" };
  readonly specs: readonly SpecNode[];
};
export type CoreSpot =
  | { readonly type: "All" }
  | { readonly type: "Category"; readonly v: string }
  | { readonly type: "Band"; readonly v: string }
  | { readonly type: "Todo"; readonly v: string };
export type CoreGrown = { readonly place: CoreSpot; readonly term: SpecNode; readonly children: readonly CoreGrown[] };

const spot: d.Decoder<CoreSpot> = d.sum<CoreSpot, "type">("type", { All: {}, Category: { v: d.str }, Band: { v: d.str }, Todo: { v: d.str } });
const grown: d.Decoder<CoreGrown> = d.lazy(() => d.obj({ place: spot, term: payload.SpecNode, children: d.arr(grown) }));

/** The list's tree of `members` grouped `by`, as the core grows it (`app/wasm/src/grow.rs`). */
export const grow = (core: Core, members: readonly CorePlaced[], by: string): CoreGrown =>
  checked("grow", core.grow(JSON.stringify(members), by), grown);

/** The environment is one shape with `fold`'s `env`, so a fold's answer goes to `fulfillment` unrewritten. */
export interface Binding {
  kind: string;
  at: string;
}


/** One binding, or a conflict's candidates with `null` for an open one. */
export type Bound = Binding | (Binding | null)[];

export type CoreTended = Record<string, string[]>;

export interface CoreEnv {
  outcomes: Record<string, Bound>;
  tended: CoreTended;
}


/** `[iso, value]`, `null` where the term reads `∅` at that instant. */
export type Knot = [string, number | null];


export interface Marker {
  at: string;
  kind: string;
  actor: string;
  hash: string;
}


/** A state register at each instant it changed (`null` is open), so a past knot is priced against its own past. */
export interface HistoryStep {
  at: string;
  binding: Bound | null;
}


/** A grow-only set is its own history, so `tended` is the env's half unchanged. */
export interface CoreHistory {
  bindings: Record<string, HistoryStep[]>;
  tended: CoreTended;
}


/** The five folds and the per-todo streams; `at` is the instant they were taken at, echoed back. */
export interface CoreFold {
  at: string;
  env: CoreEnv;
  /** A todo's spec register as its frontier: one candidate, or every one where it is in conflict. */
  specs: Record<string, readonly SpecNode[]>;
  flatten: Record<string, SpecNode>;
  /** Every mentioned todo's function, `absent` where it has none: what a `ref` links against. */
  functions: Record<string, SpecNode>;
  history: CoreHistory;
  stream: Record<string, Marker[]>;
}

/** Plus what the SubtleCrypto path cannot produce: the causal order the core put the objects in. */
export type CoreVerdict = VerifiedPayload;


export interface CoreSeries {
  knots: Knot[];
  exact: boolean;
}


// --- the decoders ------------------------------------------------------------

const { str, num, bool, arr, rec, nullable, obj } = d;
const strs = arr(str);
const Binding = obj<Binding>({ kind: str, at: str });
const Bound: d.Decoder<Bound> = d.alt(Binding, arr(nullable(Binding)));
const CoreEnv = obj<CoreEnv>({ outcomes: rec(Bound), tended: rec(strs) });
// The core prints a register's frontier of one as the value alone; read once, it is always the frontier.
const frontier = <A>(a: d.Decoder<A>): d.Decoder<readonly A[]> => d.alt(d.map(a, (x) => [x]), arr(a));
const Knot = d.tuple(str, nullable(num));
const Marker = obj<Marker>({ at: str, kind: str, actor: str, hash: str });
const HistoryStep = obj<HistoryStep>({ at: str, binding: nullable(Bound) });
const CoreHistory = obj<CoreHistory>({ bindings: rec(arr(HistoryStep)), tended: rec(strs) });
const CoreFold = obj<CoreFold>({
  at: str,
  env: CoreEnv,
  specs: rec(frontier(payload.SpecNode)),
  flatten: rec(payload.SpecNode),
  functions: rec(payload.SpecNode),
  history: CoreHistory,
  stream: rec(arr(Marker)),
});
const CoreSeries = obj<CoreSeries>({ knots: arr(Knot), exact: bool });
const Appended = d.map(
  obj<{ hash: string; objects: { hash: string; text: string }[] }>({
    hash: str,
    objects: arr(obj<{ hash: string; text: string }>({ hash: str, text: str })),
  }),
  (a): Appended => ({ hash: a.hash, objects: a.objects.map((o) => ({ hash: o.hash, literal: o.text })) }),
);

// --- the calls ---------------------------------------------------------------

/** Only the name and its bytes: the server's summary fields are never handed to the core, which reads the object itself. */
export interface Named {
  hash: string;
  literal: string;
}

export function objectsOf(links: readonly Named[]): string {
  return JSON.stringify(links.map((link) => ({ hash: link.hash, text: link.literal })));
}

/** The objects cross into the module once per question, and its memory is given back however the question ends. */
function asked<S extends { free: () => void }, T>(store: new (objects: string) => S, links: readonly Named[], ask: (s: S) => T): T {
  const read = new store(objectsOf(links));
  try {
    return ask(read);
  } finally {
    read.free();
  }
}

/**
 * A reading and what it left out (Prodrome SPEC law 44): a print that is not an object at the class's schema is left
 * out with everything resting on it, and the rest reads as if it had never arrived. What is left out is a function of
 * the objects, not of the question, so it is asked once per holding: a chain's when the set holds it (`keptChains`), the
 * stores' of each class's own `tips`. The classes' other answers stay exactly the box's for the objects they read.
 */
export type Read<A> = A & { readonly excluded: excluded.T };

const Tipped = obj<{ excluded: excluded.T }>({ excluded: excluded.decoder });

/** Rehash and parse every object, walk from every tip, and put the lot in one causal order. */
export function verifyObjects(core: Core, links: readonly Named[]): CoreVerdict {
  return checked("verify", asked(core.Todos, links, (t) => t.verify_history()), payload.VerifiedPayload);
}

/** `untrusted` comes from `/api/chain`: guessing it would fold a different chain while claiming the same one. */
export function foldOf(todos: Todos, at: string | null, untrusted: readonly string[]): CoreFold {
  return checked("fold", todos.fold(at, JSON.stringify(untrusted)), CoreFold);
}

export function foldObjects(
  core: Core,
  links: readonly Named[],
  at: string | null,
  untrusted: readonly string[],
): CoreFold {
  return asked(core.Todos, links, (t) => foldOf(t, at, untrusted));
}

// The Wasm's `registers` is not wrapped: `entries` carries each row's conflicts.

/** A stored print read back as JSON by the core; a parser here would be a second grammar. */
export function termJson(core: Core, literal: string): SpecNode {
  return checked("term_json", core.term_json(literal), payload.SpecNode);
}

/** A non-finite answer is the core misbehaving, and says so. */
function valued(value: number | undefined): number | null {
  if (value === undefined) return null;
  if (!Number.isFinite(value)) throw new CoreError(`fulfillment: the core answered ${String(value)}`);
  return value;
}

/** Each link memoised by its term; `bind` links a printed term, and a refusal (unknown todo, loop) throws in the core's words. */
export function linker(bind: (term: string) => string): (term: SpecNode) => string {
  const kept = new Map<string, string>();
  return (term) => {
    const key = JSON.stringify(term);
    const known = kept.get(key);
    if (known !== undefined) return known;
    const made = bind(key);
    kept.set(key, made);
    return made;
  };
}

/** Within one chain: its functions printed once, and a bare reference bound to them. */
export function within(core: Core, functions: Readonly<Record<string, SpecNode>>): (term: string) => string {
  const specs = JSON.stringify(functions);
  return (term) => core.link(term, specs);
}

/** Through the set at `at`, read once for every link of one reading; the caller frees it once the reading is replaced. */
export function linkerOf(chains: Chains, functions: Readonly<Record<string, SpecNode>>, at: string): Linker {
  return chains.linker(JSON.stringify(functions), at);
}

/** The environment is printed once per list; `null` for `∅`. */
export function linked(core: Core, link: (term: SpecNode) => string, env: CoreEnv, now: string): (term: SpecNode) => number | null {
  const bound = JSON.stringify(env);
  return (term) => valued(core.fulfillment(link(term), now, bound));
}

/** Each knot priced against the history at its own instant. */
export function linkedKnots(
  core: Core,
  link: (term: SpecNode) => string,
  history: CoreHistory,
  from: string,
  to: string,
): (term: SpecNode) => CoreSeries {
  const steps = JSON.stringify(history);
  return (term) => checked("series_knots", core.series_knots(link(term), from, to, steps), CoreSeries);
}

// --- the fold, as the wire's entries -----------------------------------------

/** The box's own `GET …/fold`, answered from these objects by the route the box runs: one composition of a row. */
export function listOf(todos: Todos, at: string | null, untrusted: readonly string[], chain: string, others: Chains): FoldPayload {
  return checked("list", todos.list(at, JSON.stringify(untrusted), chain, others), payload.FoldPayload);
}

export function entriesOf(
  core: Core,
  links: readonly Named[],
  at: string | null,
  untrusted: readonly string[],
  chain: string,
  others: Chains,
): FoldPayload {
  return asked(core.Todos, links, (t) => listOf(t, at, untrusted, chain, others));
}

// --- appending, so this replica can act ---------------------------------------

/** `Tended` is a pass at a recurring todo. */
export type Lifecycle = "Completed" | "Cancelled" | "Reopened" | "Tended";

/**
 * The core builds the event through its `mk_*` constructor and prints it; the print is opaque here.
 * `at` is this browser's claim and nothing clamps it: a wrong clock is reported by `verify`, never rewritten.
 */
export function lifecycleEvent(
  core: Core,
  kind: Lifecycle,
  todo: string,
  at: string,
  actor: string,
  note = "",
): string {
  return core.lifecycle(kind, todo, at, actor, note);
}

/** The act's name, and what the box lacks of it: the new object, or nothing for a twin of one held. */
export interface Appended {
  hash: string;
  objects: Named[];
}

/**
 * `event` written into the replica `todos` read: a `Change` whose deps are its todo's heads in that
 * replica's own fold: no head of the whole replica is chosen, so several are no reason to refuse. Every
 * later question of `todos` reads it.
 */
export function appendTo(todos: Todos, event: string): Appended {
  return checked("append", todos.append(event, null), Appended);
}

/** An act and the fold after it, answered by one reading of the replica. */
export interface Acted {
  appended: Appended;
  answer: FoldPayload;
}

// --- who wrote it (SPEC law 42) ------------------------------------------------

export type Standing = "trusted" | "untrusted" | "revoked";
const Standing = d.oneOf<Standing>({ trusted: null, untrusted: null, revoked: null });

/** One object's provenance in the core's reading: proven by a key, written before signatures, or not proven and why. */
export type Provenance = ProvenanceRow;

/** The `Signed` the core printed for a signature this browser made, after checking it verifies. */
export function signedObject(core: Core, object: string, key: string, signature: string): Named {
  return checked("signed", core.signed(object, key, signature), d.map(payload.SignedObject, (o) => ({ hash: o.hash, literal: o.text })));
}

export function keyAdded(core: Core, links: readonly Named[], actor: string, key: string): string {
  return asked(core.Todos, links, (t) => t.key_added(actor, key));
}

export function standingIn(todos: Todos, root: string | null, actor: string, key: string): Standing {
  return checked("standing", JSON.stringify(todos.standing(root, actor, key)), Standing);
}

export function standingOf(core: Core, links: readonly Named[], root: string | null, actor: string, key: string): Standing {
  return asked(core.Todos, links, (t) => standingIn(t, root, actor, key));
}

export function provenanceOf(core: Core, links: readonly Named[], root: string | null): Provenance[] {
  return checked("provenance", asked(core.Todos, links, (t) => t.provenance(root)), d.arr(payload.ProvenanceRow));
}

// --- the inbox, by its registers -----------------------------------------------

/** What was asked, by the secretary (`Asked`), a plugin (`Offered`) or the box itself (`Noticed`); the rest of its literal is the box's to read. */
export interface Asked {
  kind: "Asked" | "Offered" | "Noticed";
  proposal: string;
  at: string;
  actor: string;
}

/** A register's maximal writes, each named by its object: one is a value, more a conflict, none unwritten. */
export interface Write<V> {
  hash: string;
  value: V;
}

/** A store register read as every register in the tab is: its frontier's values in the one reading, `register.of`. */
export const readingOf = <V>(ws: readonly Write<V>[]): register.Reading<V> => register.of(ws.map((w) => w.value));

export interface ProposalReading {
  genesis: string | null;
  proposal: string;
  registers: { content: Write<Asked>[]; state: Write<Phase>[] };
}

const Asked = obj<Asked>({ kind: d.oneOf({ Asked: null, Offered: null, Noticed: null }), proposal: str, at: str, actor: str });
const written = <V>(value: d.Decoder<V>): d.Decoder<Write<V>[]> => arr(obj<Write<V>>({ hash: str, value }));
const ProposalReadings = obj<Read<{ readings: ProposalReading[] }>>({
  excluded: excluded.decoder,
  readings: arr(
    obj<ProposalReading>({
      genesis: nullable(str),
      proposal: str,
      registers: obj<ProposalReading["registers"]>({ content: written(Asked), state: written(payload.Phase) }),
    }),
  ),
});

/** Every proposal's registers at `at` (`null` for every write held) under `untrusted`, read by the core, which answers what it left out itself. */
export function proposalReadings(proposals: Proposals, at: string | null, untrusted: readonly string[]): Read<{ readings: ProposalReading[] }> {
  return checked("readings", proposals.readings(at, JSON.stringify(untrusted)), ProposalReadings);
}

// --- the secretary's two stores ---------------------------------------------------

/** One conversation's window in the box's query's words: before a turn, at most `limit`, about a reference. */
export interface Window {
  before?: number;
  limit?: number;
  about?: string;
}

const shown = (n: number | undefined): string | undefined => (n === undefined ? undefined : String(n));

/** What a replica of the lines forgets: every key an erasure names, with the answer keys of the blocks its words fed. */
export type Erased = Readonly<Record<string, readonly string[]>>;
const Erased: d.Decoder<Erased> = rec(arr(str));

/** The functions this build keeps rendered blocks of; a block of any other was named by another build. */
export function functionsOf(core: Core): ReadonlySet<string> {
  return new Set(checked("kept functions", core.kept_functions(), arr(str)));
}

/** What a class's reading leaves out, as its `tips` (or the lines' `excluded`) answers it. */
export const excludedOf = (s: { tips: () => string }): excluded.T => checked("tips", s.tips(), Tipped).excluded;
export const linesLeft = (lines: Lines): excluded.T => checked("excluded", lines.excluded(), Tipped).excluded;

const memoryOf = (memory: MemoryPayload | null): string | undefined => (memory === null ? undefined : JSON.stringify(memory));

/** The Inbox's rows at `at`, each proposal priced through `chains`, a remember card's headline with `memory`'s words. */
export const inboxOf = (proposals: Proposals, at: string, memory: MemoryPayload | null, chains: Chains): InboxProposalsPayload =>
  checked("inbox", proposals.inbox(at, chains, memoryOf(memory)), payload.InboxProposalsPayload);

export const listOfLines = (lines: Lines, inbox: Proposals): ConversationsPayload => checked("list", lines.list(inbox), payload.ConversationsPayload);

export const conversationOf = (lines: Lines, inbox: Proposals, id: string, window: Window, memory: MemoryPayload | null): ConversationPayload =>
  checked("conversation", lines.conversation(id, inbox, shown(window.before), shown(window.limit), window.about, memoryOf(memory)), payload.ConversationPayload);

export const searchOf = (lines: Lines, q: string): SearchPayload => checked("search", lines.search(q), payload.SearchPayload);

/** The nodes `scope` reads, of the landed row and a pull's pieces, whose print the core does not hold. */
export const lackingIn = (held: Lines, scope: lines.Scope): readonly SkeletonNode[] =>
  checked("lacking", held.lacking(JSON.stringify(scope)), arr(payload.SkeletonNode));

/** How many nodes `scope` reads lack their print, and how many of those are lines. */
export const lacksIn = (held: Lines, scope: lines.Scope): { readonly nodes: number; readonly lines: number } =>
  checked("lacks", held.lacks(JSON.stringify(scope)), obj<{ nodes: number; lines: number }>({ nodes: num, lines: num }));

/**
 * Every key the core holds, landed or in a pull's pieces, that is dead (the prints name it erased, the box's rule over the
 * steps retires it, or `told` names it), with the blocks its words fed: what a purge forgets, the blocks first.
 */
export const owedOf = (lines: Lines, told: readonly string[]): Erased => checked("owed", lines.owed(JSON.stringify(told)), Erased);

/** What a move of the lines' rows moved, as the core says it. */
export const movedOf = (said: string): "All" | readonly string[] => checked("moved", said, d.alt(d.lit("All"), arr(str)));

/** The frames of a log as `Lines.load` takes them: each after its length as four little-endian bytes. */
export const framed = (frames: readonly Uint8Array[]): Uint8Array => {
  const out = new Uint8Array(frames.reduce((k, f) => k + 4 + f.length, 0));
  const view = new DataView(out.buffer);
  let at = 0;
  for (const f of frames) {
    view.setUint32(at, f.length, true);
    out.set(f, at + 4);
    at += 4 + f.length;
  }
  return out;
};

/** The names the device keeps of the lines' content within `budget` bytes, and the plan's nodes the core lacks. */
export const planOf = (lines: Lines, budget: number): { readonly keep: readonly string[]; readonly ahead: readonly SkeletonNode[] } =>
  checked("plan", lines.plan(budget), obj<{ keep: string[]; ahead: SkeletonNode[] }>({ keep: arr(str), ahead: arr(payload.SkeletonNode) }));

export const chosenOf = (lines: Lines, topics: TopicReading, chosen: readonly string[], pages: number): readonly SkeletonNode[] =>
  checked("chosen", lines.chosen(topics, JSON.stringify(chosen), pages), arr(payload.SkeletonNode));

export const streamOf = (lines: Lines, topics: TopicReading, chosen: readonly string[], pages: number): StreamPayload =>
  checked("stream", lines.stream(topics, JSON.stringify(chosen), pages), payload.StreamPayload);

export const barOf = (topics: TopicReading): TopicsPayload => checked("topics", topics.bar(), payload.TopicsPayload);

/**
 * Every key an erasure of its own conversation names, and every key named in `told`, each with the blocks its words fed
 * where `keys` opens it: the box's sweep's own reading (`transcript::dead`).
 */
export const erasedOf = (lines: Lines, keys: Readonly<Record<string, string>>, told: readonly string[]): Erased =>
  checked("erased", lines.erased(JSON.stringify(keys), JSON.stringify(told)), Erased);

export const heldOf = (lines: Lines): readonly string[] => checked("held", lines.held(), arr(str));

/** Of the block keys `keys`, those the core lets a device keep. */
export const admitsOf = (lines: Lines, keys: readonly string[]): readonly string[] => checked("admits", lines.admits(JSON.stringify(keys)), arr(str));

/** Every line the core holds as its husk, whose print the device deletes. */
export const husksOf = (lines: Lines): readonly string[] => checked("husks", lines.husks(), arr(str));

// The names a core holds prints of, read again from it whenever its count of moves moves, and never written here: a
// view of the core's answer, so it cannot say a print is held that the core let go.
const views = new WeakMap<Lines, { readonly version: number; readonly names: ReadonlySet<string> }>();

/** The names of `names` whose print the core does not hold: the one answer to "is this fetched yet". */
export function lackingOf(lines: Lines, names: readonly string[]): ReadonlySet<string> {
  const version = lines.version();
  const known = views.get(lines);
  const view = known !== undefined && known.version === version ? known : { version, names: new Set(heldOf(lines)) };
  views.set(lines, view);
  return new Set(names.filter((n) => !view.names.has(n)));
}

/** Literals the content's admission checked, held without hashing them again: the one caller of `hold_admitted`
 * (`scripts/web-static.sh`), so only an admitted row reaches the path that does not hash. */
export const holdAdmitted = (lines: Lines, rows: readonly content.Admitted[]): void =>
  lines.hold_admitted(objectsOf(rows.map(content.rowOf)), "[]");
