/**
 * One conversation's loaded window with its live stream folded in: a pure `step` over `SourceState`,
 * and `createSource`, the effectful shell whose every effect ends in `dispatch`.
 * The shell's only closure state is the stream's closing, never the conversation's.
 * When `about` finds no turns the shell refetches the whole conversation; `step` records that as a
 * monotone `fallback`, so a page never un-says "nothing about this yet".
 */

import type { AttachmentRow, ConversationState, Frame, ProposalRow, TurnRow, UnplacedProposal } from "../wire/generated/wire";
import type { Reference } from "./refs";
import asks from "../prodrome/asks";
import intentsync from "../prodrome/intentsync";
import dressed from "./dressed";
import storesync from "../prodrome/storesync";
import d from "../wire/decoder";
import payload from "../wire/generated/payload";
import { print } from "./refs";
import sources from "../algebra/source";
import stream from "../algebra/stream";
import latest from "../algebra/latest";
import answer from "./answer";
import time from "../app/time";
import app from "../app/app";
import grouped from "./grouped";

export interface SourceDeps {
  readonly fetchWindow: (
    id: string,
    window: grouped.Asked,
  ) => Promise<dressed.Windowed>;
  /** One act on the conversation, an intent: resolves once the box took it and the replicas hold it, rejects with why not. */
  readonly ask: (a: asks.Ask) => Promise<void>;
  readonly openStream: (id: string) => stream.Socket;
  /** What says a given-up stream may open again. */
  readonly wake: stream.Wake;
  /** Bring the replicas level with the box, before a read that must see what it just wrote. */
  readonly pull: () => Promise<void>;
  /** Wake on every change of what the replicas hold that conversation `id` reads; returns its own removal. */
  readonly watch: (id: string, heard: () => void) => () => void;
}

const frameOf = d.json(payload.Frame);

export const defaultDeps: SourceDeps = {
  fetchWindow: (id, w) => dressed.window(id, w, app.now().zone),
  pull: storesync.sync,
  watch: storesync.watchConversation,
  ask: intentsync.asked,
  openStream: id => stream.of(`/api/secretary/conversations/${encodeURIComponent(id)}/stream`)(),
  wake: stream.browser,
};

// --- the state sum -----------------------------------------------------------

export interface Loading {
  readonly kind: "loading";
  readonly id: string;
  readonly about: Reference | undefined;
}

/** `problem` is a fetch that failed after a window loaded: the transcript stays on screen, since a dated answer beats none. */
export interface Loaded {
  readonly kind: "loaded";
  readonly id: string;
  readonly about: Reference | undefined;
  /** Monotone: `step` never clears it once set. */
  readonly fallback: boolean;
  readonly problem: string | null;
  readonly title: string;
  readonly model: string;
  readonly opened: string;
  readonly turns: readonly TurnRow[];
  /** The turns grouped into rounds, as the worker answered them: a window holds every turn the page holds. */
  readonly rounds: readonly grouped.Shape[];
  readonly first: number;
  readonly total: number;
  readonly proposals: readonly ProposalRow[];
  /** Proposal lines the inbox's reading cannot place, each drawn with why. */
  readonly unplaced: readonly UnplacedProposal[];
  readonly conversationState: ConversationState;
  /** The question's turn index the box would ask again from; `null` when it would not. */
  readonly again: number | null;
  readonly answer: answer.T;
}

/** Reachable only from `Loading`: a failure after a load stays `Loaded` with `problem`. */
export interface Failed {
  readonly kind: "failed";
  readonly id: string;
  readonly about: Reference | undefined;
  readonly reason: string;
}

/** Lines of the conversation are not on this device and could not be fetched: how many, and why. */
export interface NotHere {
  readonly kind: "nothere";
  readonly id: string;
  readonly about: Reference | undefined;
  readonly lines: number;
  readonly why: string;
}

export type SourceState = Loading | Loaded | Failed | NotHere;

export type SourceEvent =
  | {
      readonly kind: "window";
      readonly mode: "older" | "newest";
      readonly payload: grouped.Window;
      /** The shell measured this fetch as the fallback's second leg; `step` only ORs it in. */
      readonly fallback: boolean;
      /** When it arrived, for the answer's clock. */
      readonly at: number;
    }
  | { readonly kind: "frame"; readonly frame: Frame; readonly at: number }
  | { readonly kind: "failed"; readonly reason: string }
  | { readonly kind: "nothere"; readonly lines: number; readonly why: string };

/** Whether Stop could end a round: the one reading the Stop control and Esc share. */
export type Answering = "running" | "idle";

export const answering = (state: SourceState): Answering =>
  state.kind === "loaded" && state.conversationState !== "idle" ? "running" : "idle";

function initialState(id: string, about: Reference | undefined): SourceState {
  return { kind: "loading", id, about };
}

/**
 * What a page asks for: a newest window holding every turn it holds, from its first on; an older one, a window further
 * back as well. Before the first window, the core's own. A window about a reference is asked by how many turns it holds,
 * since the core pages it over the turns it keeps.
 */
export const asking = (state: SourceState, mode: "older" | "newest", about: string | undefined): grouped.Asked => {
  const on = about === undefined ? {} : { about };
  if (state.kind !== "loaded") return on;
  const more = mode === "older" ? grouped.WINDOW : 0;
  const limit = Math.max(grouped.WINDOW, state.turns.length + more);
  return about === undefined ? { ...on, limit, from: Math.max(0, state.first - more) } : { ...on, limit };
};

function applyWindow(
  state: SourceState,
  event: Extract<SourceEvent, { kind: "window" }>,
): Loaded {
  const priorFallback = state.kind === "loaded" ? state.fallback : false;
  return {
    kind: "loaded",
    id: state.id,
    about: state.about,
    fallback: priorFallback || event.fallback,
    problem: null,
    title: event.payload.title,
    model: event.payload.model,
    opened: event.payload.opened,
    turns: event.payload.turns,
    rounds: event.payload.rounds,
    first: event.payload.first,
    total: event.payload.total,
    proposals: event.payload.proposals,
    unplaced: event.payload.unplaced,
    conversationState: event.payload.state,
    again: event.payload.again,
    answer: answer.settle(state.kind === "loaded" ? state.answer : answer.done, event.payload.state, event.at),
  };
}

/** Turns are only appended, so a newest window holding fewer was read earlier (`say`'s answer can land after the refetch) and is dropped. */
const past = (state: SourceState, event: Extract<SourceEvent, { kind: "window" }>): boolean =>
  state.kind === "loaded" && event.mode === "newest" && event.payload.total < state.total;

function applyFrame(state: Loaded, frame: Frame, at: number): Loaded {
  const next = answer.step(state.answer, frame, at);
  const conversationState = frame.kind === "state" ? frame.state : state.conversationState;
  return next === state.answer && conversationState === state.conversationState
    ? state
    : { ...state, answer: next, conversationState };
}

/** A frame before the first load is dropped (the stream can connect first), and a later failure never discards what is on screen. */
export function step(state: SourceState, event: SourceEvent): SourceState {
  switch (event.kind) {
    case "window":
      return past(state, event) ? state : applyWindow(state, event);
    case "frame":
      return state.kind === "loaded" ? applyFrame(state, event.frame, event.at) : state;
    case "failed":
      if (state.kind === "loaded") return { ...state, problem: event.reason };
      return { kind: "failed", id: state.id, about: state.about, reason: event.reason };
    case "nothere":
      if (state.kind === "loaded") return { ...state, problem: storesync.notHere(event.lines, "this conversation", event.why) };
      return { kind: "nothere", id: state.id, about: state.about, lines: event.lines, why: event.why };
  }
}

// --- the effectful shell -------------------------------------------------

/** What a page can do to the conversation; every effect ends in the fold's `dispatch`. */
export interface SourceEffects {
  /** Safe to call once. */
  start: () => void;
  refresh: () => Promise<void>;
  /** Rejects on a fetch failure; the caller decides how to say so. */
  earlier: () => Promise<void>;
  /** Resolves once the person's turn is on the page; rejects with the box's refusal. */
  say: (text: string, references: readonly Reference[], files?: readonly AttachmentRow[]) => Promise<void>;
  /** Rejects with the box's refusal (409 when the round had already ended). */
  stop: () => Promise<void>;
  /** Ask the last round again; rejects with the box's refusal. */
  regenerate: () => Promise<void>;
  /** Replace the last round's question; rejects with the box's refusal. */
  edit: (text: string, references: readonly Reference[], files?: readonly AttachmentRow[]) => Promise<void>;
  destroy: () => void;
}

export type Source = sources.Source<SourceState, SourceEvent> & SourceEffects;

export function createSource(
  id: string,
  about: Reference | undefined,
  deps: SourceDeps = defaultDeps,
  at?: number,
): Source {
  const fold = sources.create<SourceState, SourceEvent>(initialState(id, about), step);
  let destroyed = false;
  let close = (): void => {};

  // Once fallen back, stays fallen back, so the whole conversation never narrows under the reader on a later reading.
  let effectiveAbout: Reference | undefined = about;

  // A window holding every turn the page holds, and with an older one the turns before them too; or lines not on this
  // device.
  type Reading =
    | { readonly type: "Read"; readonly payload: grouped.Window; readonly fallback: boolean; readonly mode: "older" | "newest" }
    | { readonly type: "NotHere"; readonly lines: number; readonly why: string };

  async function fetchApplyingFallback(mode: "older" | "newest"): Promise<Reading> {
    const aboutText = effectiveAbout === undefined ? undefined : print(effectiveAbout);
    const first = await deps.fetchWindow(id, asking(fold.state(), mode, aboutText));
    if (first.type === "NotHere") return first;
    if (aboutText === undefined || first.payload.turns.length > 0) {
      return { type: "Read", payload: first.payload, fallback: false, mode };
    }
    effectiveAbout = undefined;
    const whole = await deps.fetchWindow(id, asking(fold.state(), mode, undefined));
    return whole.type === "NotHere" ? whole : { type: "Read", payload: whole.payload, fallback: true, mode };
  }

  let unwatch = (): void => {};

  // The box said a round moved, or a write was taken: pull, then read.
  async function refresh(): Promise<void> {
    await deps.pull();
    await read();
  }

  // The replicas only grow, so a reading asked later is never older than one asked before it; the two stores sync
  // apart, so two readings can hold the same turns and differ in their cards, and only the order asked tells them apart.
  // A reading that lands after a later-asked one has is dropped, its failure included.
  const readings = latest.of<Reading>((l) =>
    fold.dispatch(
      l.type === "Error"
        ? { kind: "failed", reason: l.e }
        : l.v.type === "Read"
          ? { kind: "window", mode: l.v.mode, payload: l.v.payload, fallback: l.v.fallback, at: time.now() }
          : { kind: "nothere", lines: l.v.lines, why: l.v.why },
    ),
  );
  const newest = readings.ask;

  const read = (): Promise<void> => newest(() => fetchApplyingFallback("newest"));

  /** A search hit's first window ends at its turn, so stick-to-bottom lands on it; later refreshes ask for the newest. */
  const openAt = (turn: number): Promise<void> =>
    newest(async (): Promise<Reading> => {
      const w = await deps.fetchWindow(id, { before: turn + 1 });
      return w.type === "NotHere" ? w : { type: "Read", payload: w.payload, fallback: false, mode: "newest" };
    });

  // In the readings' order, so a newest reading asked before it, which holds less, never lands over it.
  async function earlier(): Promise<void> {
    const current = fold.state();
    if (current.kind !== "loaded" || current.first <= 0 || destroyed) return;
    await newest(() => fetchApplyingFallback("older"));
  }

  // How many turns this page has seen: what a question or a stop is asked after.
  const seen = (): number => {
    const current = fold.state();
    return current.kind === "loaded" ? current.total : 0;
  };

  const question = (text: string, references: readonly Reference[], files: readonly AttachmentRow[]): asks.Question => ({
    conversation: id,
    after: seen(),
    text,
    references: references.map(print),
    files,
  });

  // A write's answer is not read: what it wrote reaches the page through the replica like anyone else's,
  // and the intent is held once the replicas have it, so reading now shows it.
  async function acting(a: asks.Ask): Promise<void> {
    await deps.ask(a);
    if (destroyed) return;
    await read();
  }

  // A stream given up opens again when the network may be back; meanwhile the replicas `watch` hears still move it.
  const heard = (data: string): void => {
    const decoded = frameOf(data);
    if (decoded.type !== "Val") return;
    const frame = decoded.v;
    fold.dispatch({ kind: "frame", frame, at: time.now() });
    // A `turn` frame refetches rather than drawing itself: cards, references and `queued` come only from the payload.
    if (frame.kind === "turn") void refresh();
  };

  return {
    ...fold,
    start: () => {
      void (at === undefined ? refresh() : openAt(at));
      // A stream that opens again may mean the box answers again, so lines not on this device are asked for again.
      close = stream.open(() => deps.openStream(id), { frame: heard, opened: () => (fold.state().kind === "nothere" ? void read() : undefined) }, deps.wake);
      unwatch = deps.watch(id, () => void read());
    },
    refresh,
    earlier,
    say: (text, references, files = []) => acting({ type: "Question", kind: "Say", question: question(text, references, files) }),
    stop: () => acting({ type: "Round", kind: "Stop", conversation: id, after: seen() }),
    regenerate: () => acting({ type: "Round", kind: "Regenerate", conversation: id, after: seen() }),
    edit: (text, references, files = []) => acting({ type: "Question", kind: "Edit", question: question(text, references, files) }),
    destroy: () => {
      destroyed = true;
      readings.close();
      unwatch();
      close();
    },
  };
}
