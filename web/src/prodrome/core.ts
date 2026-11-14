/** prodrome-core as WebAssembly in this tab, so the tab and the box have one evaluator; `loadCore()` is the only effect. */

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

/** Hand-written, since `tsc` cannot wait for the generated `prodrome.d.ts`; names are the Rust ones. */
export interface Core {
  Todos: new (objects: string) => Todos;
  Proposals: new () => Proposals;
  Lines: new () => Lines;
  Memberships: new () => TopicStore;
  TopicAliases: new () => TopicStore;
  Asks: new () => ObjectStore;
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
  /** `prodrome::memo::name`: a node's content name. */
  memo_name: (own: string, children: string[]) => string;
  answer_key: (source: string) => string;
  /** `[<name>, …]` (`rendering::kept`). */
  kept_functions: () => string;
  /** `Erasure::for_good`'s sentence. */
  erase_warning: () => string;
  /** `{hash, text}`: the `Signed`, refused unless `signature` verifies. */
  signed: (object: string, key: string, signature: string) => string;
  /** Why a text is no topic (`Topic::new`'s refusal), or `undefined` for a topic. */
  topic_refusal: (text: string) => string | undefined;
}

export interface Todos {
  verify: () => string;
  /** `verify`, history beneath the epoch read as before signatures: the box's own reading. */
  verify_history: () => string;
  fold: (at: string | null, untrusted: string) => string;
  /** The box's fold answer, priced through `chain` and `others`. */
  list: (at: string | null, untrusted: string, chain: string, others: Chains) => string;
  append: (event: string, genesis: string | null) => string;
  key_added: (actor: string, key: string) => string;
  /** `trusted`, `untrusted` or `revoked` under `root`. */
  standing: (root: string | null, actor: string, key: string) => string;
  provenance: (root: string | null) => string;
  free: () => void;
}

/** A store of objects the core holds (`object_rows::Store`); each move answers its frame. */
export interface ObjectStore {
  since: () => string;
  heads: () => string;
  heard: () => boolean;
  behind: (heads: string) => boolean;
  overtaken: (epoch: number) => boolean;
  went_back: (epoch: number) => boolean;
  piece: (answer: string, fetchedAt: number) => Uint8Array;
  land: () => LineStep;
  start_over: () => LineStep;
  /** `event` sealed as a `Change` and kept as the tab's, unsent; none for a twin. */
  append: (event: string) => LineStep | undefined;
  /** `[[{hash, literal}]]`: what the tab wrote and may post, in posts the box takes whole. */
  unsent: () => string;
  answered: (receipt: string) => LineStep;
  /** `names` refused for good in the box's words. */
  refuse: (names: string, reason: string) => LineStep;
  /** `[{hash, reason}]`: what the box refused for good. */
  refusals: () => string;
  /** `[<name>, …]`: the unsent changes no signature covers. */
  unsigned: () => string;
  /** The device's `Signed` of a change, kept unsent. */
  signed: (literal: string) => LineStep;
  /** The asks' refusals of this store's writes, held as refused; none where nothing moved. */
  refused_in: (asks: ObjectStore) => LineStep | undefined;
  apply: (frame: Uint8Array) => string;
  load: (bytes: Uint8Array | undefined, frames: Uint8Array) => string;
  snapshot: () => Uint8Array;
  version: () => number;
  tips: () => string;
  readings: (at: string | null, untrusted: string) => string;
  free: () => void;
}

export interface Proposals extends ObjectStore {
  inbox: (at: string, chains: Chains, memory?: string) => string;
}

/** The todo chains held. */
export interface Chains {
  hold: (name: string, objects: string, untrusted: string) => string;
  drop: (name: string) => void;
  /** The set at `at`, read once, to link every term of one reading. */
  linker: (specs: string, at: string) => Linker;
  free: () => void;
}

/** The set at one instant, read once: a term linked through it binds as the box binds a row. */
export interface Linker {
  link: (term: string) => string;
  free: () => void;
}

/** One chain as the tab holds it, by name. */
export interface HeldChain {
  readonly name: string;
  readonly replica: Replica;
}

const Held = (chain: string): d.Decoder<excluded.Left> =>
  d.alt(
    d.map(obj<{ why: string }>({ why: str }), (h) => excluded.left(excluded.NONE, [{ chain, why: h.why }])),
    d.map(Tipped, (t) => excluded.left(t.excluded)),
  );

export function holdIn(chains: Chains, name: string, objects: readonly Named[], untrusted: readonly string[]): excluded.Left {
  return checked("hold", chains.hold(name, objectsOf(objects), JSON.stringify(untrusted)), Held(name));
}

/** One move of the lines' rows; freed once read. */
export interface LineStep {
  readonly delta: Uint8Array;
  readonly moved: string;
  free: () => void;
}

/** The lines a tab holds; a slot without its key reads as missing. */
export interface Lines {
  hold: (add: string, drop: string) => void;
  /** `hold` of literals already checked against their names, so none is hashed again. */
  hold_admitted: (add: string, drop: string) => void;
  held: () => string;
  version: () => number;
  since: () => string;
  heads: () => string;
  heard: () => boolean;
  behind: (heads: string) => boolean;
  /** Whether an answer of this epoch is later than either row heard: the store starts over first. */
  overtaken: (epoch: number) => boolean;
  went_back: (epoch: number) => boolean;
  /** A skeleton answer kept as a piece: its frame. */
  piece: (answer: string, keys: string, fetchedAt: number) => Uint8Array;
  land: () => LineStep;
  start_over: () => LineStep;
  /** The keys let go from every row, each line whose every key is gone kept as its husk. */
  forget: (gone: string) => LineStep;
  /** `[{hash, husk}]`: husks answered for prints, kept where no held key opens them. */
  take_husks: (given: string) => LineStep;
  husks: () => string;
  /** Of the block keys, those a device may keep: an answer key only while its words' keys are held. */
  admits: (keys: string) => string;
  /** Whether a line the rows read is held with words each key of which is gone: one a purge husks. */
  husking: () => boolean;
  apply: (frame: Uint8Array) => string;
  rows: () => string;
  lacking: (scope: string) => string;
  lacks: (scope: string) => string;
  holds: (id: string) => boolean;
  owed: (told: string) => string;
  /** What the device keeps within a budget, and what of it lacks its print. */
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
  /** The node of every line the newest `pages` pages of the stream read, by the skeleton alone. */
  chosen: (topics: TopicReading, chosen: string, pages: number) => string;
  snapshot: () => Uint8Array;
  /** The replica replaced by a snapshot, then `frames`: the conversations moved. */
  load: (bytes: Uint8Array | undefined, frames: Uint8Array) => string;
  stream: (topics: TopicReading, chosen: string, pages: number) => string;
  /** Every key an erasure names, with the blocks its words fed where `keys` opens it. */
  erased: (keys: string, told: string) => string;
  excluded: () => string;
  free: () => void;
}

export type TopicStore = ObjectStore;

/** The topics both replicas read as, and their planned events. */
export interface TopicReading {
  bar: () => string;
  add: (target: string, topic: string, at: string, actor: string) => string | undefined;
  remove: (target: string, topic: string, at: string, actor: string) => string;
  rename: (from: string, to: string, at: string, actor: string) => string;
  free: () => void;
}

/** Not an `ApiError`: an API failure means ask again, a core failure means fall back and say so. */
export class CoreError extends Error {}


interface Glue extends Core {
  default: (init: { module_or_path: Response }) => Promise<unknown>;
}

/** Lazy; a failed load is not kept, so a retry retries. */
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


function checked<T>(what: string, text: string, decoder: d.Decoder<T>): T {
  const r = d.json(decoder)(text);
  if (r.type === "Val") return r.v;
  throw new CoreError(`${what}: unexpected shape (${r.e})`);
}

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

/** `app/wasm/src/grow.rs`'s tree. */
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


/** A state register at each instant it changed (`null` is open). */
export interface HistoryStep {
  at: string;
  binding: Bound | null;
}


/** A grow-only set is its own history, so `tended` is the env's half unchanged. */
export interface CoreHistory {
  bindings: Record<string, HistoryStep[]>;
  tended: CoreTended;
}


export interface CoreFold {
  at: string;
  env: CoreEnv;
  specs: Record<string, readonly SpecNode[]>;
  flatten: Record<string, SpecNode>;
  /** Every mentioned todo's function, `absent` where it has none. */
  functions: Record<string, SpecNode>;
  history: CoreHistory;
  stream: Record<string, Marker[]>;
}

export type CoreVerdict = VerifiedPayload;


export interface CoreSeries {
  knots: Knot[];
  exact: boolean;
}



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


export interface Named {
  hash: string;
  literal: string;
}

export function objectsOf(links: readonly Named[]): string {
  return JSON.stringify(links.map((link) => ({ hash: link.hash, text: link.literal })));
}

/** The objects cross into the module once per question, and its memory is freed however the question ends. */
function asked<S extends { free: () => void }, T>(store: new (objects: string) => S, links: readonly Named[], ask: (s: S) => T): T {
  const read = new store(objectsOf(links));
  try {
    return ask(read);
  } finally {
    read.free();
  }
}

/** A reading and what it left out (Prodrome SPEC law 44): a function of the objects, asked once per holding. */
export type Read<A> = A & { readonly excluded: excluded.T };

const Tipped = obj<{ excluded: excluded.T }>({ excluded: excluded.decoder });

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

/** Memoised by term; a refusal throws in the core's words. */
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

export function within(core: Core, functions: Readonly<Record<string, SpecNode>>): (term: string) => string {
  const specs = JSON.stringify(functions);
  return (term) => core.link(term, specs);
}

/** The caller frees it. */
export function linkerOf(chains: Chains, functions: Readonly<Record<string, SpecNode>>, at: string): Linker {
  return chains.linker(JSON.stringify(functions), at);
}

export function linked(core: Core, link: (term: SpecNode) => string, env: CoreEnv, now: string): (term: SpecNode) => number | null {
  const bound = JSON.stringify(env);
  return (term) => valued(core.fulfillment(link(term), now, bound));
}

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


/** The box's own `GET …/fold`, from these objects. */
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


/** `Tended` is a pass at a recurring todo. */
export type Lifecycle = "Completed" | "Cancelled" | "Reopened" | "Tended";

/** The core builds and prints the event; `at` is this browser's claim, which nothing clamps (`verify` reports it). */
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

/** The act's name and new object, or nothing for a twin. */
export interface Appended {
  hash: string;
  objects: Named[];
}

/** `event` as a `Change` over its todo's heads in `todos`. */
export function appendTo(todos: Todos, event: string): Appended {
  return checked("append", todos.append(event, null), Appended);
}

export interface Acted {
  appended: Appended;
  answer: FoldPayload;
}


export type Standing = "trusted" | "untrusted" | "revoked";
const Standing = d.oneOf<Standing>({ trusted: null, untrusted: null, revoked: null });

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


/** What was asked; the rest of its literal is the box's to read. */
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

export function proposalReadings(proposals: Proposals, at: string | null, untrusted: readonly string[]): Read<{ readings: ProposalReading[] }> {
  return checked("readings", proposals.readings(at, JSON.stringify(untrusted)), ProposalReadings);
}


export interface Window {
  before?: number;
  limit?: number;
  about?: string;
}

const shown = (n: number | undefined): string | undefined => (n === undefined ? undefined : String(n));

/** What a replica of the lines forgets: every key an erasure names, with the blocks its words fed. */
export type Erased = Readonly<Record<string, readonly string[]>>;
const Erased: d.Decoder<Erased> = rec(arr(str));

/** The functions this build keeps rendered blocks of; a block of any other was named by another build. */
export function functionsOf(core: Core): ReadonlySet<string> {
  return new Set(checked("kept functions", core.kept_functions(), arr(str)));
}

/** What a reading leaves out. */
export const excludedOf = (s: { tips: () => string }): excluded.T => checked("tips", s.tips(), Tipped).excluded;
export const linesLeft = (lines: Lines): excluded.T => checked("excluded", lines.excluded(), Tipped).excluded;

const memoryOf = (memory: MemoryPayload | null): string | undefined => (memory === null ? undefined : JSON.stringify(memory));

export const inboxOf = (proposals: Proposals, at: string, memory: MemoryPayload | null, chains: Chains): InboxProposalsPayload =>
  checked("inbox", proposals.inbox(at, chains, memoryOf(memory)), payload.InboxProposalsPayload);

export const listOfLines = (lines: Lines, inbox: Proposals): ConversationsPayload => checked("list", lines.list(inbox), payload.ConversationsPayload);

export const conversationOf = (lines: Lines, inbox: Proposals, id: string, window: Window, memory: MemoryPayload | null): ConversationPayload =>
  checked("conversation", lines.conversation(id, inbox, shown(window.before), shown(window.limit), window.about, memoryOf(memory)), payload.ConversationPayload);

export const searchOf = (lines: Lines, q: string): SearchPayload => checked("search", lines.search(q), payload.SearchPayload);

export const lackingIn = (held: Lines, scope: lines.Scope): readonly SkeletonNode[] =>
  checked("lacking", held.lacking(JSON.stringify(scope)), arr(payload.SkeletonNode));

export const lacksIn = (held: Lines, scope: lines.Scope): { readonly nodes: number; readonly lines: number } =>
  checked("lacks", held.lacks(JSON.stringify(scope)), obj<{ nodes: number; lines: number }>({ nodes: num, lines: num }));

/** Every dead key held, with the blocks its words fed: what a purge forgets. */
export const owedOf = (lines: Lines, told: readonly string[]): Erased => checked("owed", lines.owed(JSON.stringify(told)), Erased);

export const movedOf = (said: string): "All" | readonly string[] => checked("moved", said, d.alt(d.lit("All"), arr(str)));

/** Frames, each after its length as four little-endian bytes. */
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

export const planOf = (lines: Lines, budget: number): { readonly keep: readonly string[]; readonly ahead: readonly SkeletonNode[] } =>
  checked("plan", lines.plan(budget), obj<{ keep: string[]; ahead: SkeletonNode[] }>({ keep: arr(str), ahead: arr(payload.SkeletonNode) }));

export const chosenOf = (lines: Lines, topics: TopicReading, chosen: readonly string[], pages: number): readonly SkeletonNode[] =>
  checked("chosen", lines.chosen(topics, JSON.stringify(chosen), pages), arr(payload.SkeletonNode));

export const streamOf = (lines: Lines, topics: TopicReading, chosen: readonly string[], pages: number): StreamPayload =>
  checked("stream", lines.stream(topics, JSON.stringify(chosen), pages), payload.StreamPayload);

export const barOf = (topics: TopicReading): TopicsPayload => checked("topics", topics.bar(), payload.TopicsPayload);

/** `transcript::dead`. */
export const erasedOf = (lines: Lines, keys: Readonly<Record<string, string>>, told: readonly string[]): Erased =>
  checked("erased", lines.erased(JSON.stringify(keys), JSON.stringify(told)), Erased);

export const heldOf = (lines: Lines): readonly string[] => checked("held", lines.held(), arr(str));

export const admitsOf = (lines: Lines, keys: readonly string[]): readonly string[] => checked("admits", lines.admits(JSON.stringify(keys)), arr(str));

export const husksOf = (lines: Lines): readonly string[] => checked("husks", lines.husks(), arr(str));

// The names a core holds prints of, a view of the core's answer read again whenever its moves move.
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
