/**
 * The transcript: rounds keyed by turn index, drawn from a `Source`, with the scroll rules applied to the
 * given scroll box only, never `document`, so a sheet or panel can never scroll the page under it.
 * The answer's place is the column's last row, never a header legend; where the box is held is `anchor.ts`'s.
 */

import type { BlockRow, ProposalRow, TurnGone, TurnRow } from "../wire/generated/wire";
import spoken from "../inbox/spoken";
import onscreen, { store } from "../app/store";
import waiting from "../inbox/waiting";
import { block, revealHeld } from "../wire/api";
import heldvalue from "../view/heldvalue";
import attachment from "./attachment";
import rendered from "../app/rendered";
import blocks from "../app/blocks";
import app from "../app/app";
import { el } from "../view/dom";
import placeholder from "../view/placeholder";
import memory from "./memory";
import { sawProposal } from "../inbox/needsyou";
import { proposalCard } from "../inbox/proposal";
import asks from "../prodrome/asks";
import intents from "../prodrome/intents";
import intentsync from "../prodrome/intentsync";
import keyed from "../algebra/keyed";
import { anchor, anchorOf, print, textWithReferences, tokenise } from "./refs";
import { type Round, foldOf } from "./rounds";
import grouped from "./grouped";
import { richOrText } from "../rich";
import { toast } from "../view/toast";
import held from "./anchor";
import again from "./again";
import erase from "./erase";
import turns from "./turns";
import answer from "./answer";
import clock from "../algebra/clock";
import { type Answering, type Source, type SourceState, answering } from "./source";
import time from "../app/time";
import ui from "../app/ui";
import registers from "../app/registers";
import lo from "../algebra/lo";
import marks from "./marks";
import slices from "./slices";
import page from "../shell/page";

export interface TranscriptActions {
  /** "Reply" on a turn — quotes it into the compose box. */
  quote: (text: string) => void;
  /** "Reply" on a proposal card — attaches it as a reference. */
  attachProposal: (proposal: ProposalRow) => void;
  /** "Edit" on the last question — brings it back into the compose box. */
  edit: (question: turns.Question) => void;
}

export interface TranscriptHandle {
  /** One node the caller places; this module never appends it. */
  readonly content: HTMLElement;
  /** A question was asked: the box follows it until its turn is known. */
  asked: () => void;
  /** The question asked is the box's now: hold it at the top of the box, unless the reader has scrolled since. */
  sent: () => void;
  /** `"idle"` when nothing runs or this login may not act. */
  stop: () => Answering;
  /** The question the box would ask again — what ↑ brings back. */
  offer: () => again.Offer;
  destroy: () => void;
}

function speaker(turn: TurnRow): string {
  switch (turn.who) {
    case "person":
      return "You";
    case "model":
      return "Secretary";
    case "act":
      return "You did";
    case "box":
      return "The box";
    case "verb":
      return turn.outcome.is === "refused" ? `${turn.verb} — refused` : turn.verb;
  }
}

/** Copies words or Typst, never markup. */
const copyButton = (label: string, text: string, className: string): HTMLElement => {
  const button = el("button", { type: "button", class: className, title: `${label} to the clipboard` }, label);
  button.addEventListener("click", () => {
    navigator.clipboard.writeText(text).then(
      () => toast("Copied"),
      (e: unknown) => toast(lo.reason(e)),
    );
  });
  return button;
};

/** An answer's whole source, behind a disclosure. */
const shownAsText = (text: string): HTMLElement =>
  el("details", { class: "turnsource" }, el("summary", {}, "Show as text"), el("div", { class: "turntext" }, text));

/** Block by block, typst's HTML through `rich.ts`, or the source as text when the box sent none. */
export function answerBlocks(turn: { readonly text: string; readonly blocks: readonly BlockRow[] }): HTMLElement {
  const found = tokenise(turn.text).flatMap((token) => ("reference" in token ? [token.reference] : []));
  return el(
    "div",
    { class: "turnanswer" },
    turn.blocks.map((b) =>
      el(
        "div",
        { class: "answerblockwrap" },
        richOrText(b.html, b.source, { class: "answerblock", "data-kind": b.kind }),
        copyButton("Copy", b.plain, "copyblock"),
      ),
    ),
    found.length > 0
      ? el("div", { class: "turnrefs" }, found.map((reference) => anchor(reference, "reflink turnref")))
      : null,
  );
}

/** What an answer offers besides its words: its copies and its source, for the menu under it. */
export const answerCopies = (turn: { readonly text: string; readonly blocks: readonly BlockRow[] }): HTMLElement[] => [
  copyButton("Copy", answer.plain(turn.blocks), "copyanswer"),
  copyButton("Copy Typst", turn.text, "copyanswer"),
  shownAsText(turn.text),
];

/** The controls used rarely, behind one "…": nothing when there are none, open while one of them is mid-act. */
export const more = (items: readonly HTMLElement[], open = false): HTMLElement | null =>
  items.length === 0
    ? null
    : el(
        "details",
        { class: "turnmore", open },
        el("summary", { class: "turnmorebutton", title: "More", "aria-label": "More" }, "…"),
        el("div", { class: "turnmenu" }, items),
      );

/** A model turn with no words draws nothing; one whose words are gone draws why, never an empty bubble. */
const silent = (turn: TurnRow): boolean => turn.who === "model" && turn.text === "" && turn.gone === null;

/** Who erased words, or that their key is not here: the box's sentence, drawn as the box's and never as anyone's words. */
const goneText = (turn: TurnRow, gone: TurnGone): HTMLElement => el("div", { class: "turntext turngone", "data-gone": gone.why }, turn.text);

/** Who may erase: a login that acts, on a turn whose words are here; `null` where none may. */
export type Erasing = { readonly conversation: string; readonly asked: intentsync.T; readonly warning: Promise<string> } | null;

/** At rest a turn shows its words, Reply and one "…"; `tucked` is what else its menu holds (a finished round's calls). */
function turnBlock(
  turn: TurnRow,
  actions: TranscriptActions,
  mayReply: boolean,
  erasing: Erasing = null,
  tucked: HTMLElement | null = null,
): HTMLElement {
  const classes = ["turn", `turn-${turn.who}`];
  if (turns.refused(turn)) classes.push("refused");
  if (turn.gone !== null) classes.push("gone");
  const source = el("div", { class: "turntext" }, ...textWithReferences(turn.text, turn.who));
  const rich = turn.gone === null && turn.who === "model" && turn.blocks.length > 0 ? turn : null;
  const body = turn.gone !== null ? goneText(turn, turn.gone) : heldvalue.within(rich !== null ? answerBlocks(rich) : source, revealHeld);
  const block = el(
    "div",
    { class: classes.join(" "), "data-who": turn.who },
    el("div", { class: "turnwho" }, speaker(turn)),
    body,
  );
  const files = turns.attachments(turn);
  if (files.length > 0) {
    block.appendChild(
      el(
        "div",
        { class: "turnphotos" },
        // A photo opens full size; a PDF is only named, since the app's WebView cannot show one.
        files.map((file) =>
          attachment.fold(
            file,
            (url) => el("a", { href: url, target: "_blank", rel: "noopener" }, attachment.view(file)),
            () => attachment.view(file),
            () => attachment.view(file),
          ),
        ),
      ),
    );
  }
  const references = turns.references(turn);
  if (references.length > 0) {
    block.appendChild(
      el(
        "div",
        { class: "turnrefs" },
        references.map((reference) => anchorOf(reference, "reflink turnref")),
      ),
    );
  }
  const reply =
    mayReply && turn.gone === null && (turn.who === "model" || turn.who === "person")
      ? el("button", { type: "button", class: "turnreply", onclick: () => actions.quote(turn.text) }, "Reply")
      : null;
  const erasable = erasing !== null && turn.gone === null && !turns.queued(turn) ? erasing : null;
  const menu = more(
    [
      ...(rich !== null ? answerCopies(rich) : []),
      ...(erasable === null ? [] : [erase.view(erasable.conversation, turn, erasable.asked, erasable.warning)]),
      ...(tucked === null ? [] : [tucked]),
    ],
    erasable !== null && !erase.atRest(erasable.conversation, turn, erasable.asked),
  );
  if (reply !== null || menu !== null) block.appendChild(el("div", { class: "turncontrols" }, reply, menu));
  return block;
}

/**
 * A round as its parts, each kept by its own key and redrawn only when its own signature moves: an answer's HTML
 * arriving or a turn joining the trace leaves the cards beneath it the very nodes they were, so a tap pressed on one
 * is released on it.
 */
function roundParts(
  conversationId: string,
  round: Round,
  actions: TranscriptActions,
  mayReply: boolean,
  asked: intentsync.T,
  forget: memory.Forget,
  titles: spoken.Titles,
): keyed.Item[] {
  // A line once written never changes, so a turn draws anew only when its HTML arrives or its words go: what it is
  // drawn from is named by its shape, never by its words, which a redraw would copy into a string per turn.
  const piece = (placed: { readonly turn: TurnRow; readonly index: number }): string => {
    const blocks = turns.blocks(placed.turn);
    return `${placed.index}:${placed.turn.who}:${turns.verb(placed.turn) ?? ""}:${turns.refused(placed.turn)}:${turns.queued(placed.turn)}:${blocks.length}:${blocks.filter((b) => b.html !== null).length}:${placed.turn.text.length}:${placed.turn.at}:${placed.turn.gone?.why ?? ""}`;
  };
  const erasing: Erasing = mayReply ? { conversation: conversationId, asked, warning: erase.warning() } : null;
  const erased = (placed: { readonly turn: TurnRow }): string =>
    erasing === null ? "" : erase.sig(conversationId, placed.turn, asked);
  const turn = (
    key: string,
    placed: { readonly turn: TurnRow; readonly index: number } | null,
    tucked: { readonly sig: string; readonly make: () => HTMLElement } | null = null,
  ): keyed.Item[] =>
    placed === null || silent(placed.turn)
      ? []
      : [
          {
            key,
            sig: `${mayReply}|${piece(placed)}|${erased(placed)}|${tucked?.sig ?? ""}`,
            make: () => turnBlock(placed.turn, actions, mayReply, erasing, tucked?.make() ?? null),
          },
        ];
  const folded = (placed: readonly { readonly turn: TurnRow; readonly index: number }[], reply: boolean): HTMLElement =>
    el("div", { class: "roundfoldbody" }, placed.filter((p) => !silent(p.turn)).map((p) => turnBlock(p.turn, actions, reply, reply ? erasing : null)));
  // What a rewind set aside is read, never resumed: collapsed, with no Reply.
  const earlier: keyed.Item[] =
    round.earlier.length === 0
      ? []
      : [
          {
            key: "earlier",
            sig: round.earlier.map(piece).join("|"),
            make: () =>
              el(
                "details",
                { class: "roundfold earlierattempt" },
                el("summary", { class: "roundfoldsummary" }, "Earlier attempt"),
                folded(round.earlier, false),
              ),
          },
        ];
  const fold = foldOf(round);
  // The calls a round made: in plain sight while it is going or when one was refused, else in its answer's menu.
  const calls =
    fold === null
      ? null
      : {
          sig: `${fold.label}|${fold.refused}|${round.trace.map((p) => `${piece(p)}|${erased(p)}`).join("|")}`,
          make: () =>
            el(
              "details",
              { class: "roundfold" },
              el(
                "summary",
                { class: "roundfoldsummary" },
                fold.label,
                fold.refused ? el("span", { class: "roundfoldrefused" }, "refused") : null,
              ),
              folded(round.trace, mayReply),
            ),
        };
  const tucked = fold !== null && !fold.refused && round.answer !== null ? calls : null;
  const trace: keyed.Item[] = calls === null || tucked !== null ? [] : [{ key: "trace", sig: `${mayReply}|${calls.sig}`, make: calls.make }];
  // A memory reaches every later conversation, so its write is shown in plain sight with Forget, never folded.
  const receipts = round.trace.flatMap((placed): keyed.Item[] => {
    const remembered = turns.remembered(placed.turn);
    return remembered === null
      ? []
      : [{ key: `m${placed.index}`, sig: `${forget === null}|${intents.key(remembered)}`, make: () => memory.receipt(remembered, forget) }];
  });
  const cards = round.proposals.map((proposal): keyed.Item => {
    sawProposal(conversationId, proposal.index);
    const shown = intents.about(asked, asks.about(conversationId, proposal.index), asks.holdsRow(proposal));
    return {
      key: `p${proposal.index}`,
      sig: `${mayReply}|${intents.key(shown)}|${intents.key(proposal)}|${proposal.sources.map((s) => spoken.said(s, titles)).join("|")}`,
      make: () => proposalCard(conversationId, proposal, { intents: asked, reply: actions.attachProposal, titles }),
    };
  });
  return [...turn("opening", round.opening), ...earlier, ...trace, ...receipts, ...turn("answer", round.answer, tucked), ...cards];
}

export function mountTranscript(
  scrollBox: Element,
  source: Source,
  actions: TranscriptActions,
): TranscriptHandle {
  const earlierBtn = el("button", { type: "button", class: "earlierbtn", hidden: true }, "Earlier…");
  const stopButton = el("button", { type: "button", class: "stopanswer" }, "Stop");
  const regenerateButton = el("button", { type: "button", class: "againbutton" }, "Regenerate");
  const editButton = el("button", { type: "button", class: "againbutton" }, "Edit question");
  const landing = el("div", { class: "answerlanding" });
  const controls = el("div", { class: "answercontrols" }, stopButton, regenerateButton, editButton);
  const place = el("div", { class: "answerplace", hidden: true }, landing, controls);
  // The room a held question's answer grows into: empty, the only node whose height is the room, and hidden while there
  // is none, so the column's gap is not drawn for it.
  const roomNode = el("div", { class: "answerroom", hidden: true });
  const roundList = el("div", { class: "rounds" });
  const turns = el("div", { class: "turns" }, roundList, place, roomNode);
  const newAnswerPill = el("button", { type: "button", class: "newanswerpill", hidden: true }, "↓ new answer");
  const problem = el("p", { class: "problem", hidden: true });
  const unplaced = el("div", { class: "unplaced" });
  // Until the first window is read, turns in grey above the composer: a conversation on its way never looks empty.
  const coming = placeholder.view(placeholder.TURNS);
  const content = el("div", { class: "transcriptcontent" }, coming, problem, unplaced, earlierBtn, turns, newAnswerPill);

  // Rounds by turn index; each a day mark and its parts, every list keyed, so a redraw replaces only what moved.
  const roundNodes = keyed.list(roundList);
  const inner = new Map<string, { readonly wrapper: HTMLElement; readonly wrap: keyed.List; readonly round: HTMLElement; readonly parts: keyed.List }>();
  const roundOf = (key: string): HTMLElement => {
    const wrapper = el("div", { class: "roundwrap" });
    const round = el("div", { class: "round" });
    inner.set(key, { wrapper, wrap: keyed.list(wrapper), round, parts: keyed.list(round) });
    return wrapper;
  };
  const wrapOf = (key: string): HTMLElement => inner.get(key)?.wrapper ?? roundOf(key);
  const onPage = scrollBox === document.scrollingElement;
  const hold = ui.cell(registers.hold, `${onPage ? "page" : "panel"}:${source.state().id}`);
  // A conversation's page reached afresh (a link, a search hit) opens at its newest turn, as every page opens at its
  // top; read again (back, forward), it opens where it was left. The panel beside every page keeps its place.
  if (onPage && !app.now().again) hold.set(registers.hold.initial);
  let holding: held.T = hold.get().at;
  // How many turns the box was last held against; null before the first window.
  let heldAt: number | null = null;

  // --- where the box is held ---------------------------------------------

  // The page's own box is the viewport, whose top is 0; the document element's rectangle moves up as it scrolls.
  const offsetOf = (node: Element): number =>
    node.getBoundingClientRect().top - (onPage ? 0 : scrollBox.getBoundingClientRect().top) + scrollBox.scrollTop;
  // The page's sticky header covers the top of its box, so a question placed at the top goes below it.
  const covered = (): number => (onPage ? page.header().getBoundingClientRect().bottom : 0);
  // The room's height now: what `held.room` last answered, and what the reader's place is read without.
  let room = 0;

  // When the reader last touched the box (below: who moved it is read from the reader's input).
  let readerAt = Number.NEGATIVE_INFINITY;

  // The box moved for the page, at the next frame and only while that move still stands (`held.stands`).
  const moveBox = (m: Extract<held.Move, { type: "ToBottom" | "ToTurn" }>, asked: held.Asked): void =>
    void requestAnimationFrame(() => {
      if (!held.stands(asked, readerAt, holding)) return;
      switch (m.type) {
        case "ToBottom":
          scrollBox.scrollTop = scrollBox.scrollHeight;
          newAnswerPill.hidden = true;
          return;
        case "ToTurn": {
          // The round's own node: its keyed wrapper is `display: contents`, so it has no box to measure.
          const round = inner.get(String(m.turn))?.round;
          if (round !== undefined) scrollBox.scrollTop = offsetOf(round) - covered();
          newAnswerPill.hidden = true;
          return;
        }
      }
    });

  const move = (e: held.Event): void => {
    const [next, m] = held.step(holding, e);
    holding = next;
    hold.set({ at: holding, top: Math.round(scrollBox.scrollTop) });
    // Where the room starts is read from its node, which has a box only while shown: hidden, the room was 0, and
    // `held.room` answers 0 or a window's height whatever `end` says.
    room = held.room(holding, room, {
      bottom: scrollBox.scrollTop + scrollBox.clientHeight,
      end: offsetOf(roomNode),
      height: scrollBox.clientHeight,
    });
    roomNode.style.height = `${room}px`;
    roomNode.hidden = room === 0;
    switch (m.type) {
      case "None":
        return;
      case "ToBottom":
      case "ToTurn":
        return moveBox(m, { at: performance.now(), held: holding });
      case "Pill":
        newAnswerPill.hidden = false;
        return;
    }
  };

  const scroller: EventTarget = scrollBox === document.scrollingElement ? window : scrollBox;
  // Who moved the box is read from the reader's input, never from the scroll: the box also moves when it places a
  // question, when its content shrinks under it, and when the browser anchors it against content drawn above. A
  // scroll within a moment of the reader's wheel, touch, scroll key or press on the scrollbar is theirs, and so is
  // every scroll that follows it without a pause (a fling's momentum).
  const READER_MS = 600;
  const SCROLL_KEYS = new Set(["PageUp", "PageDown", "ArrowUp", "ArrowDown", "Home", "End", " "]);
  const reading = (): void => void (readerAt = performance.now());
  const onKey = (e: Event): void =>
    e instanceof KeyboardEvent && SCROLL_KEYS.has(e.key) && !(e.target instanceof HTMLElement && e.target.closest("input, textarea, select, [contenteditable]") !== null)
      ? reading()
      : undefined;
  const onPress = (e: Event): void => (e.target === scrollBox ? reading() : undefined);
  const inputs: readonly [EventTarget, string, (e: Event) => void][] = [
    [scroller, "wheel", reading],
    [scroller, "touchmove", reading],
    [window, "keydown", onKey],
    [scroller, "pointerdown", onPress],
  ];
  inputs.forEach(([t, k, f]) => t.addEventListener(k, f, { passive: true }));
  const onScroll = (): void => {
    const by: held.Mover = performance.now() - readerAt < READER_MS ? "reader" : "page";
    if (by === "reader") reading();
    move({
      type: "scrolled",
      by,
      at: held.placed({ top: scrollBox.scrollTop, scrollHeight: scrollBox.scrollHeight, room, height: scrollBox.clientHeight }),
    });
  };
  scroller.addEventListener("scroll", onScroll, { passive: true });

  // --- the answer's place --------------------------------------------------

  const streamed = new Map<string, Node>();

  // A closed block is asked for by its sha, and shows its source until (or unless) the HTML arrives.
  const renderBlock = (b: answer.Block): Node => {
    const known = streamed.get(b.sha);
    if (known !== undefined) return known;
    const shown = richOrText(null, b.source, { class: "answerblock" });
    streamed.set(b.sha, shown);
    // Kept by the function that renders it and its source, so a kept copy is the one that function computes.
    const id = source.state().id;
    blocks
      .answerKey(b.source)
      .then((k) => rendered.through(blocks.store, k, () => block(id, b.sha)))
      .then(
        (html) => {
          const rich = richOrText(html, b.source, { class: "answerblock" });
          streamed.set(b.sha, rich);
          shown.replaceWith(rich);
        },
        () => {},
      );
    return shown;
  };

  // The elapsed count is drawn again when it next reads otherwise, never on a period.
  let unwake = (): void => {};

  const drawPlace = (): void => {
    const state = source.state();
    const a = state.kind === "loaded" ? state.answer : answer.done;
    const now = time.now();
    if (a.type === "Done") streamed.clear();
    landing.replaceChildren(
      a.type === "Streaming" ? el("div", { class: "turnwho" }, "Secretary") : "",
      answer.view(a, now, { render: renderBlock }),
    );
    const acting = app.writes(app.now());
    stopButton.hidden = !acting || answering(state) === "idle";
    const offered = acting && again.offer(state).type === "Last";
    regenerateButton.hidden = !offered;
    editButton.hidden = !offered;
    place.hidden = a.type === "Done" && stopButton.hidden && !offered;
    unwake();
    unwake = clock.wake(answer.next(a, now), drawPlace);
    move({ type: "grew" });
  };

  const told = (error: unknown): void => {
    toast(lo.reason(error));
  };

  function stop(): Answering {
    const state = source.state();
    if (!app.writes(app.now()) || answering(state) === "idle") return "idle";
    source.stop().catch(told);
    return "running";
  }

  stopButton.addEventListener("click", () => {
    stop();
  });
  regenerateButton.addEventListener("click", () => {
    source.regenerate().catch(told);
  });
  editButton.addEventListener("click", () => {
    again.fold(again.offer(source.state()), () => {}, actions.edit);
  });

  // A redraw reads its state once into a pass, and the pass draws the rounds newest first in slices (`slices`), from
  // where the last stopped; a round new to the page stands once drawn, and one it holds stands where it stood until
  // then, so a redraw moves nothing it does not change. When the last is drawn, the answer's place, whose move reads the
  // layout, is placed in the next frame, which lays the drawn rounds out once, never ahead of it in a task, and the page
  // marks it (`marks.PLACED`). A redraw replaces the pass it finds. A hidden tab, whose tasks are throttled and which
  // draws no frame, draws and places in one pass.
  // The rounds are the worker's (`grouped`), drawn as given: each read from the page's turns and cards by index as its
  // slice draws it.
  // Rows by their own index, read once per list the source holds.
  const indexer = <R extends { readonly index: number }>(): ((rows: readonly R[]) => ReadonlyMap<number, R>) => {
    const known = new WeakMap<readonly R[], ReadonlyMap<number, R>>();
    return (rows) => {
      const was = known.get(rows);
      if (was !== undefined) return was;
      const made = new Map(rows.map((r) => [r.index, r]));
      known.set(rows, made);
      return made;
    };
  };
  const turnsIndexed = indexer<TurnRow>();
  const cardsIndexed = indexer<ProposalRow>();
  type Pass = {
    readonly state: SourceState & { readonly kind: "loaded" };
    readonly shapes: readonly grouped.Shape[];
    readonly turnsAt: ReadonlyMap<number, TurnRow>;
    readonly cardsAt: ReadonlyMap<number, ProposalRow>;
    readonly mayReply: boolean;
    readonly asked: intentsync.T;
    readonly forget: memory.Forget;
    readonly titles: spoken.Titles;
    from: number;
  };
  const drawing = slices.of();
  let placing: number | undefined;
  const placings: (() => void)[] = [];

  const placeNow = (): void => {
    if (placing !== undefined) cancelAnimationFrame(placing);
    placing = undefined;
    for (const p of placings.splice(0)) p();
    performance.clearMarks(marks.PLACED);
    performance.mark(marks.PLACED);
  };
  const placeLater = (): void => {
    if (placing !== undefined) cancelAnimationFrame(placing);
    if (slices.browser.hidden()) return placeNow();
    placing = requestAnimationFrame(placeNow);
  };
  // A tab hidden mid-way draws and places the rest at once.
  const onHidden = (): void => {
    if (!document.hidden) return;
    drawing.flush();
    if (placing !== undefined) placeNow();
  };
  document.addEventListener("visibilitychange", onHidden);

  const passOf = (p: Pass): slices.Work => ({
    unit: () => {
      const shape = p.shapes[p.from - 1];
      p.from -= 1;
      if (shape !== undefined) {
        const round = grouped.resolve(shape, p.turnsAt, p.cardsAt);
        const key = String(round.key);
        if (!inner.has(key)) roundOf(key);
        const it = inner.get(key);
        if (it !== undefined) {
          const dayMark = shape.marked ? shape.label : undefined;
          it.wrap.draw([
            ...(dayMark === undefined ? [] : [{ key: "day", sig: dayMark, make: () => el("div", { class: "dayseparator" }, dayMark) }]),
            { key: "round", sig: "", make: () => it.round },
          ]);
          it.parts.draw(roundParts(p.state.id, round, actions, p.mayReply, p.asked, p.forget, p.titles));
          // A round new to the page stands once drawn, before the newer one this pass drew just before it; a round
          // made before stands where it stood, and the last slice puts every round in its place.
          if (it.wrapper.parentNode !== roundList) {
            const newer = p.shapes[p.from + 1];
            const before = newer === undefined ? null : (inner.get(String(newer.key))?.wrapper ?? null);
            roundList.insertBefore(it.wrapper, before?.parentNode === roundList ? before : null);
          }
        }
      }
      return p.from > 0;
    },
    done: () => {
      roundNodes.draw(p.shapes.map((round) => ({ key: String(round.key), sig: "", make: () => wrapOf(String(round.key)) })));
      const keys = new Set(p.shapes.map((round) => String(round.key)));
      for (const k of [...inner.keys()]) if (!keys.has(k)) inner.delete(k);
      placeLater();
    },
  });

  /** Idempotent: a keyed-round diff, so an unchanged state moves nothing. */
  function redraw(state: SourceState): void {
    drawing.cancel();
    if (state.kind !== "loading") coming.remove();
    if (state.kind !== "loaded") {
      problem.hidden = state.kind !== "failed";
      problem.textContent = state.kind === "failed" ? state.reason : "";
      earlierBtn.hidden = true;
      placeLater();
      return;
    }
    const mayReply = app.writes(app.now());
    earlierBtn.hidden = state.first <= 0;
    problem.hidden = state.problem === null;
    problem.textContent = state.problem ?? "";
    unplaced.replaceChildren(...state.unplaced.map(u =>
      el("p", { class: "problem" }, `${print({ kind: "proposal", conversation: state.id, index: u.index })} has no card: ${waiting.unheld(u.why)}.`)));
    const pass: Pass = {
      state,
      shapes: state.rounds,
      turnsAt: turnsIndexed(state.turns),
      cardsAt: cardsIndexed(state.proposals),
      mayReply,
      asked: intentsync.held(),
      forget: memory.forgets(mayReply, () => source.refresh()),
      titles: spoken.titles(onscreen.entries(store.state())),
      from: state.rounds.length,
    };
    if (state.rounds.length === 0) {
      roundNodes.draw([]);
      inner.clear();
      placeLater();
      return;
    }
    // The first slice in the task after the reading, so neither holds the thread the other's time.
    drawing.run(passOf(pass));
  }

  // --- wiring --------------------------------------------------------------

  // A frame of the answer is placed at once; anything else redraws, and is placed once the pass is drawn, in order.
  const unsubscribe = source.subscribe((state, event) => {
    if (event.kind === "frame") return drawPlace();
    placings.push(() => {
      drawPlace();
      // A refresh that brings no turn (a poll, a block's HTML, a second read) moves nothing under the reader's finger.
      if (event.kind === "window" && event.mode === "newest" && state.kind === "loaded") {
        const e = held.windowed(heldAt, state.total);
        const was = hold.get();
        // The first window of a conversation drawn again opens where the reader left it, unless that was the newest.
        if (e?.type === "window" && e.fresh && was.at.type !== "Newest") scrollBox.scrollTop = was.top;
        else if (e !== null) move(e);
        heldAt = state.total;
      }
    });
    redraw(state);
  });

  // A card is drawn against what this tab asked, so an ask redraws exactly the rounds whose cards it moved.
  const unwatchAsked = intentsync.watch(() => redraw(source.state()));

  const unwatchActor = app.hear(app.actor, () => {
    redraw(source.state());
    drawPlace();
  });

  earlierBtn.addEventListener("click", () => {
    const state = source.state();
    if (state.kind !== "loaded" || state.first <= 0) return;
    const before = scrollBox.scrollHeight;
    source
      .earlier()
      .then(() => {
        scrollBox.scrollTop += scrollBox.scrollHeight - before;
      })
      .catch((error: unknown) => {
        toast(lo.reason(error));
      });
  });
  newAnswerPill.addEventListener("click", () => move({ type: "pill" }));

  // The question just sent is the last person turn the box holds.
  const sent = (): void => {
    const state = source.state();
    const asked = state.kind === "loaded" ? state.turns.filter((t) => t.who === "person").at(-1) : undefined;
    if (asked !== undefined) move({ type: "sent", turn: asked.index });
  };

  return {
    content,
    asked: () => move({ type: "asked" }),
    sent,
    stop,
    offer: () => again.offer(source.state()),
    destroy: () => {
      drawing.cancel();
      if (placing !== undefined) cancelAnimationFrame(placing);
      document.removeEventListener("visibilitychange", onHidden);
      unsubscribe();
      unwatchAsked();
      unwatchActor();
      scroller.removeEventListener("scroll", onScroll);
      inputs.forEach(([t, k, f]) => t.removeEventListener(k, f));
      unwake();
      content.remove();
    },
  };
}
