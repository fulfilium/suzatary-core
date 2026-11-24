/**
 * The conversation regrouped for reading: a person's turn or an act opens a round, the calls and their
 * answers are its trace, and the trailing model turn is its answer.
 * The wire has no `calls` flag, so a call is read off the shape: a model turn with nothing after it
 * made none, and a round whose trace ends in a verb turn is still going.
 * Indices are each row's own `TurnRow.index`, never a position, because `?about=` leaves gaps.
 */

import type { ProposalRow, TurnRow } from "../wire/generated/wire";
import time from "../app/time";

export interface Placed {
  readonly turn: TurnRow;
  readonly index: number;
}

/** `key` is the opening's index, else the first trace turn's, so a round with no opening keeps a stable identity. */
export interface Round {
  readonly key: number;
  readonly startIndex: number;
  readonly endIndex: number;
  /** `null` only for a round the model opened unasked, as the brief does. */
  readonly opening: Placed | null;
  readonly trace: readonly Placed[];
  /** `null` while the round is still going, or when the answer was empty text. */
  readonly answer: Placed | null;
  /** Cards drawn where they were said. */
  readonly proposals: readonly ProposalRow[];
  /** Turns a rewind set aside; for a question an edit replaced, the whole round, leaving the rest empty. */
  readonly earlier: readonly Placed[];
}

/** A round's turns in the order they came; never empty, so its span and opening are total. */
type Run = [Placed, ...Placed[]];

/** A person's question, set aside or not, and a current act or box note open a round; every other turn joins the one before it. */
const opens = (turn: TurnRow): boolean =>
  turn.who === "person" || ((turn.who === "act" || turn.who === "box") && turn.attempt === "current");

// `?about=` drops whole rounds and never splits one, so each run is one contiguous span.
const runs = (turns: readonly TurnRow[]): Run[] => {
  const placed = (turn: TurnRow): Placed => ({ turn, index: turn.index });
  const heads = turns.flatMap((turn, at) => (at === 0 || opens(turn) ? [{ turn, at }] : []));
  return heads.map(({ turn, at }, k): Run => [placed(turn), ...turns.slice(at + 1, heads[k + 1]?.at).map(placed)]);
};

// A set-aside question heads its run without opening it, so it is drawn with the earlier attempt it was asked in; a
// set-aside act (a stop) opens nothing, so a regenerated answer stays by its question.
const roundOf = (run: Run): Round => {
  const [head, ...rest] = run;
  const opening = head.turn.attempt === "current" && opens(head.turn) ? head : null;
  const tail = opening === null ? run : rest;
  const body = tail.filter((p) => p.turn.attempt === "current");
  const last = body[body.length - 1];
  // A trailing model turn made no call, so even empty it never joins the trace.
  const said = last !== undefined && last.turn.who === "model" ? last : null;
  const answer = said !== null && said.turn.text !== "" ? said : null;
  const trace = said === null ? body : body.slice(0, -1);
  const indices = run.map((p) => p.index);
  const startIndex = Math.min(...indices);
  return {
    key: opening?.index ?? trace[0]?.index ?? answer?.index ?? startIndex,
    startIndex,
    endIndex: Math.max(...indices),
    opening,
    trace,
    answer,
    proposals: [],
    earlier: tail.filter((p) => p.turn.attempt === "earlier"),
  };
};

/** A card is drawn after turn `turn - 1`, in the last round begun by then, else the first; none when there is no round. */
const owner = (rs: readonly Round[], proposal: ProposalRow): Round | undefined =>
  rs.reduce<Round | undefined>((o, r) => (o === undefined || r.startIndex <= proposal.turn - 1 ? r : o), undefined);

export function rounds(turns: readonly TurnRow[], proposals: readonly ProposalRow[]): Round[] {
  const built = runs(turns).map(roundOf);
  const placed = proposals.reduce((byKey, proposal) => {
    const o = owner(built, proposal);
    if (o !== undefined) byKey.set(o.key, [...(byKey.get(o.key) ?? []), proposal]);
    return byKey;
  }, new Map<number, readonly ProposalRow[]>());
  return built.map((round) => ({ ...round, proposals: placed.get(round.key) ?? [] }));
}

/** A verb sorted into what a reader recognises; one outside the four families prints as itself, never an invented bucket. */
export function verbFamily(verb: string): string {
  if (verb === "search" || verb === "show" || verb === "thread") return "mail";
  if (
    verb === "todo_show" ||
    verb === "list_open_todos" ||
    verb === "todo_add" ||
    verb === "todo_update"
  ) {
    return "todos";
  }
  if (verb === "context" || verb === "briefing" || verb === "briefing_delta") {
    return "notes";
  }
  if (verb.startsWith("propose_")) return "proposals";
  return verb;
}

/** `refused` travels apart from the label so the page marks it without parsing the sentence. */
export interface Fold {
  readonly label: string;
  readonly refused: boolean;
}

/** `41 s`, `2 min`, `2 min 5 s`. */
function elapsed(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)} s`;
  const minutes = Math.floor(seconds / 60);
  const rest = Math.round(seconds - minutes * 60);
  return rest === 0 ? `${minutes} min` : `${minutes} min ${rest} s`;
}

/** `null` when nothing was checked, so no fold is drawn at all. */
export function foldOf(round: Round): Fold | null {
  if (round.trace.length === 0) return null;
  const counts = new Map<string, number>();
  let refused = false;
  for (const placed of round.trace) {
    if (placed.turn.who !== "verb") continue;
    if (placed.turn.outcome.is === "refused") refused = true;
    const family = verbFamily(placed.turn.verb);
    counts.set(family, (counts.get(family) ?? 0) + 1);
  }
  if (counts.size === 0) return null;
  const parts = [...counts.entries()].map(([family, count]) => `${family} (${count})`);
  let label = `Checked ${parts.join(", ")}`;
  const openedAt = round.opening?.turn.at ?? round.trace[0]?.turn.at ?? "";
  const closedAt = round.answer?.turn.at ?? "";
  if (openedAt !== "" && closedAt !== "") {
    const start = time.ms(openedAt);
    const end = time.ms(closedAt);
    if (start !== null && end !== null && end >= start) {
      label += ` · ${elapsed((end - start) / 1000)}`;
    }
  }
  return { label, refused };
}
